use crate::console::{ConsoleAction, DevConsole, EngineSettingAction, command_node_from_spec};
use crate::debug::DebugLogger;
use crate::ecs::{EntityId, World};
use crate::input::InputState;
use crate::logging::LogMirror;
use crate::mods::ModRegistry;
use crate::renderer::{
    EguiOutput, Renderer, WgpuContext, apply_render_command, collect_lights, view_proj_from_entity,
};
use crate::runtime::{LoadedGame, SimulationHandle};
use crate::script::ipc::ScriptMessage;
use crate::vfs::VfsHandle;
use std::{
    sync::Arc,
    time::{Duration, Instant},
};
use winit::{
    application::ApplicationHandler,
    event::{DeviceEvent, DeviceId, ElementState, WindowEvent},
    event_loop::{ActiveEventLoop, ControlFlow},
    keyboard::{KeyCode, PhysicalKey},
    window::{CursorGrabMode, Window, WindowId},
};
pub struct Engine {
    debug: DebugLogger,
    debug_mirror: Arc<std::sync::Mutex<Vec<String>>>,
    log_mirror: LogMirror,
    game_name: String,
    vfs: VfsHandle,
    registry: Arc<ModRegistry>,
    simulation: SimulationHandle,
    editor_link: Option<crate::runtime::EditorClient>,
    input: InputState,
    #[cfg(windows)]
    embedded_cursor: Arc<crate::cursor::EmbeddedCursor>,
    frame_number: u64,
    last_frame: Instant,
    fps_meter: crate::presentation::FrameRate,
    frame_clock: crate::presentation::FrameClock,
    motion: crate::presentation::MotionHistory,
    rendered_poses: std::collections::HashMap<u32, (EntityId, crate::ecs::Transform)>,
    fps_cap: Option<f64>,
    vsync: bool,
    renderer: Option<Renderer>,
    window: Option<Arc<Window>>,
    world: World,
    main_camera: Option<EntityId>,
    tick_interval: Duration,
    console: DevConsole,
    debug_overlay: bool,
    egui_ctx: egui::Context,
    egui_state: Option<egui_winit::State>,
    should_quit: bool,
}
impl Engine {
    pub fn new(game: LoadedGame, log_mirror: LogMirror) -> anyhow::Result<Self> {
        let debug = game.debug.clone();
        let debug_mirror = debug.console_mirror();
        let game_name = game.meta.game.name.clone();
        let vfs = Arc::clone(&game.vfs);
        let registry = Arc::clone(&game.registry);
        let tick_interval = Duration::from_secs_f64(1.0 / game.meta.game.tick_rate);
        let simulation = SimulationHandle::spawn(game)?;
        let editor_link =
            crate::runtime::EditorClient::connect(simulation.commands.clone(), &simulation.shared)?;
        Ok(Self {
            editor_link,
            debug,
            debug_mirror,
            log_mirror,
            game_name,
            vfs,
            registry,
            simulation,
            input: InputState::new(),
            #[cfg(windows)]
            embedded_cursor: Arc::default(),
            frame_number: 0,
            last_frame: Instant::now(),
            fps_meter: Default::default(),
            frame_clock: Default::default(),
            motion: Default::default(),
            rendered_poses: Default::default(),
            fps_cap: None,
            vsync: true,
            renderer: None,
            window: None,
            world: World::new(),
            main_camera: None,
            tick_interval,
            console: DevConsole::new(),
            debug_overlay: false,
            egui_ctx: egui::Context::default(),
            egui_state: None,
            should_quit: false,
        })
    }
    fn frame(&mut self) {
        #[cfg(windows)]
        if self.input.mouse_captured
            && std::env::var_os("WHATEVER_EDITOR_EMBEDDED").is_some()
            && self
                .window
                .as_ref()
                .is_some_and(|w| !crate::cursor::EmbeddedCursor::focused(w))
        {
            self.set_cursor_captured(false);
        }

        if self
            .editor_link
            .as_ref()
            .is_some_and(|link| link.inspecting())
        {
            self.set_cursor_captured(false);
            self.input.keys_pressed.clear();
            self.input.mouse_delta = (0.0, 0.0);
            self.sync_input();
        }
        if let Ok(mut lines) = self.debug_mirror.lock() {
            for line in lines.drain(..) {
                self.console.push_debug_line(line);
            }
        }
        if let Ok(mut entries) = self.log_mirror.lock() {
            for (level, msg) in entries.drain(..) {
                self.console.push_log_line(&level, msg);
            }
        }

        #[cfg(windows)]
        if self.input.mouse_captured
            && std::env::var_os("WHATEVER_EDITOR_EMBEDDED").is_some()
            && let Err(error) = self.embedded_cursor.confine()
        {
            tracing::warn!("play viewport confinement: {error:#}");
            self.set_cursor_captured(false);
        }
        let now = Instant::now();
        self.last_frame = now;
        self.console.fps = self.fps_meter.record(now);
        self.console.fps_cap = self.fps_cap;
        self.console.vsync = self.vsync;

        self.receive_simulation();

        self.frame_number += 1;

        // Compute camera view-projection and position for this frame.
        let no_camera = self.compute_camera_info().0.is_none();

        // Run egui for this frame
        let egui_raw_input =
            if let (Some(state), Some(window)) = (self.egui_state.as_mut(), self.window.as_ref()) {
                Some(state.take_egui_input(window))
            } else {
                None
            };

        let egui_render_data = if let Some(raw_input) = egui_raw_input {
            let egui_ctx = self.egui_ctx.clone();
            let vfs = Arc::clone(&self.vfs);

            let debug = self.debug.shared_switches();
            let show_overlay = self.debug_overlay;
            let overlay_fps = self.console.fps;
            let overlay_tick_rate = 1.0 / self.tick_interval.as_secs_f64();
            let overlay_entities = self.world.allocator.alive_entity_ids().count();
            let overlay_fps_cap = self.fps_cap;
            let overlay_vsync = self.vsync;
            let mut console_action = ConsoleAction::None;
            let full_output = egui_ctx.run(raw_input, |ctx| {
                console_action = self.console.render(
                    ctx,
                    &self.registry,
                    vfs.as_ref(),
                    Arc::clone(&debug),
                    &self.world,
                );
                if show_overlay {
                    render_debug_overlay(
                        ctx,
                        overlay_fps,
                        overlay_tick_rate,
                        overlay_entities,
                        overlay_fps_cap,
                        overlay_vsync,
                    );
                }
                if no_camera {
                    render_no_camera_warning(ctx);
                }
            });

            match console_action {
                ConsoleAction::Quit => self.should_quit = true,
                ConsoleAction::SendIpc { mod_id, message } => {
                    self.simulation.send(mod_id, message);
                }
                ConsoleAction::EngineSettings(action) => match action {
                    EngineSettingAction::SetFpsCap(cap) => self.set_fps_cap(cap),
                    EngineSettingAction::SetVsync(enabled) => {
                        self.vsync = enabled;
                        self.apply_vsync();
                    }
                },
                ConsoleAction::None => {}
            }

            if let (Some(state), Some(window)) = (self.egui_state.as_mut(), self.window.as_ref()) {
                state.handle_platform_output(window, full_output.platform_output);
                // egui-winit calls set_cursor_visible(true) whenever egui
                // outputs a non-None cursor icon, which it does every frame.
                // Re-hide the cursor if we still own it.
                if self.input.mouse_captured {
                    window.set_cursor_visible(false);
                }
            }

            let paint_jobs = egui_ctx.tessellate(full_output.shapes, full_output.pixels_per_point);

            Some((
                paint_jobs,
                full_output.textures_delta,
                full_output.pixels_per_point,
            ))
        } else {
            None
        };

        // Build egui render output (needs renderer dimensions)
        let egui_out = if let (Some((paint_jobs, textures_delta, ppp)), Some(renderer)) =
            (egui_render_data, self.renderer.as_ref())
        {
            Some(EguiOutput {
                paint_jobs,
                textures_delta,
                screen_descriptor: egui_wgpu::ScreenDescriptor {
                    size_in_pixels: [renderer.ctx.config.width, renderer.ctx.config.height],
                    pixels_per_point: ppp,
                },
            })
        } else {
            None
        };

        // Only the client render pass sees these poses. Console, simulation and editor snapshots
        // keep authoritative tick values; all visible objects and the camera share one render time.
        let transforms = self.motion.sample(&self.world, Instant::now());
        let authoritative = std::mem::replace(&mut self.world.transforms, transforms);
        let (camera_vp, camera_pos) = self.compute_camera_info();
        if let Some(renderer) = self.renderer.as_mut() {
            crate::renderer::sync_render_transforms(
                renderer,
                &self.world,
                self.vfs.as_ref(),
                &mut self.rendered_poses,
            );
            renderer.update_lighting(&collect_lights(&self.world));
            if let Err(e) =
                renderer.render(camera_vp, camera_pos, egui_out.as_ref(), self.vfs.as_ref())
            {
                tracing::error!("render error: {e}");
            }
        }
        self.world.transforms = authoritative;
    }

    /// Derive a view-projection matrix and world-space camera position from the
    /// current main camera entity.  Returns `(None, None)` if no camera is set,
    /// the entity is stale, or it is missing `core:transform` / `core:camera`.
    fn compute_camera_info(&self) -> (Option<glam::Mat4>, Option<glam::Vec3>) {
        let Some(camera_id) = self.main_camera.as_ref() else {
            return (None, None);
        };
        if !self.world.is_active(camera_id) {
            return (None, None);
        }
        let idx = camera_id.index;
        let Some(transform) = self.world.world_transform(camera_id) else {
            return (None, None);
        };
        let Some(cam) = self.world.camera_components.get(&idx) else {
            return (None, None);
        };
        let Some(aspect) = self.renderer.as_ref().map(|r| r.aspect) else {
            return (None, None);
        };
        let vp = view_proj_from_entity(&transform, cam, aspect);
        let pos = glam::Vec3::from(transform.position);
        (Some(vp), Some(pos))
    }

    fn set_cursor_captured(&mut self, captured: bool) {
        #[cfg(windows)]
        if std::env::var_os("WHATEVER_EDITOR_EMBEDDED").is_some()
            && let Some(window) = &self.window
        {
            let captured = captured && crate::cursor::EmbeddedCursor::focused(window);
            match self.embedded_cursor.capture(window, captured) {
                Ok(()) => self.input.mouse_captured = captured,
                Err(error) => {
                    self.input.mouse_captured = false;
                    window.set_cursor_visible(true);
                    tracing::warn!("cursor capture: {error:#}");
                }
            }
            self.sync_input();
            return;
        }

        self.input.mouse_captured = false;
        if let Some(window) = self.window.as_ref() {
            if captured {
                // Prefer Locked (cursor position frozen, ideal for camera look).
                // Fall back to Confined (cursor stays inside the window) on
                // platforms that don't support Locked.
                match window
                    .set_cursor_grab(CursorGrabMode::Locked)
                    .or_else(|_| window.set_cursor_grab(CursorGrabMode::Confined))
                {
                    Ok(()) => self.input.mouse_captured = true,
                    Err(error) => tracing::warn!("cursor capture: {error}"),
                }
                window.set_cursor_visible(!self.input.mouse_captured);
            } else {
                let _ = window.set_cursor_grab(CursorGrabMode::None);
                window.set_cursor_visible(true);
            }
        }
        self.sync_input();
    }

    fn receive_simulation(&mut self) {
        if self.simulation.is_finished() {
            self.should_quit = true;
        }
        let Some(update) = self.simulation.take_presentation() else {
            return;
        };
        self.world = update.world;
        self.motion = update.motion;
        self.main_camera = update.main_camera;
        self.tick_interval = Duration::from_secs_f64(1.0 / update.tick_rate);
        for (mod_id, msg) in update.events {
            // Intercept console-specific messages before the general dispatcher
            if let ScriptMessage::RegisterCommand {
                ref name,
                ref description,
                ref subcommands,
                ref args,
                has_handler,
            } = msg
            {
                let mut node = command_node_from_spec(
                    &crate::script::ipc::CommandNodeSpec {
                        name: name.clone(),
                        description: description.clone(),
                        subcommands: subcommands.clone(),
                        args: args.clone(),
                        has_handler,
                    },
                    &mod_id,
                );
                // Override the top-level source (from_spec sets it for children too)
                node.source = crate::console::CommandSource::Mod(mod_id.clone());
                self.console.registry.register_mod(&mod_id, node);
                continue;
            }

            if let ScriptMessage::CommandResponse {
                ref request_id,
                ref output,
                ref error,
            } = msg
            {
                let matches = self
                    .console
                    .pending_invoke
                    .as_ref()
                    .map(|p| &p.request_id == request_id)
                    .unwrap_or(false);
                if matches {
                    self.console
                        .handle_command_response(output.clone(), error.clone());
                }
                continue;
            }

            if let ScriptMessage::ArgSuggestResponse {
                ref request_id,
                ref suggestions,
            } = msg
            {
                self.console
                    .handle_arg_suggest_response(request_id, suggestions.clone());
                continue;
            }
        }
        for command in update.client_commands {
            if std::env::var_os("WHATEVER_EDITOR_EMBEDDED").is_some()
                && matches!(
                    command,
                    ScriptMessage::SetWindowSize { .. } | ScriptMessage::SetWindowMode { .. }
                )
            {
                continue;
            }
            match command {
                ScriptMessage::SetWindowTitle { title } => {
                    if let Some(window) = &self.window {
                        window.set_title(&title);
                    }
                }
                ScriptMessage::SetWindowSize { width, height } => {
                    if let Some(window) = &self.window {
                        let _ =
                            window.request_inner_size(winit::dpi::PhysicalSize::new(width, height));
                    }
                }
                ScriptMessage::SetWindowMode { mode } => {
                    if let Some(window) = &self.window {
                        let fullscreen = match mode.as_str() {
                            "borderless" => Some(winit::window::Fullscreen::Borderless(None)),
                            "fullscreen" => window
                                .current_monitor()
                                .and_then(|monitor| monitor.video_modes().next())
                                .map(winit::window::Fullscreen::Exclusive)
                                .or(Some(winit::window::Fullscreen::Borderless(None))),
                            _ => None,
                        };
                        window.set_fullscreen(fullscreen);
                    }
                }
                ScriptMessage::SetFpsCap { fps } => self.set_fps_cap(fps),
                ScriptMessage::SetVsync { enabled } => {
                    self.vsync = enabled;
                    self.apply_vsync();
                }
                _ => {}
            }
        }
        if let Some(renderer) = self.renderer.as_mut() {
            for command in update.render_cmds.into_values() {
                self.rendered_poses.remove(&command.key().1);
                apply_render_command(&command, renderer, &self.world, self.vfs.as_ref());
            }
        }
    }
    fn sync_input(&mut self) {
        if let Ok(mut input) = self.simulation.shared.input.lock() {
            input.keys = if self.console.is_open {
                vec![]
            } else {
                self.input
                    .keys_pressed
                    .iter()
                    .map(|key| format!("{key:?}"))
                    .collect()
            };
            input.keys.sort_unstable();
            input.mouse_captured = self.input.mouse_captured && !self.console.is_open;
            let (dx, dy) = self.input.flush_mouse();
            if !input.mouse_captured {
                input.mouse = (0.0, 0.0);
            } else {
                input.mouse.0 += dx;
                input.mouse.1 += dy;
            }
        }
    }

    fn set_fps_cap(&mut self, cap: Option<f64>) {
        match self.frame_clock.set_cap(cap, self.last_frame) {
            Ok(()) => self.fps_cap = cap,
            Err(error) => tracing::warn!("FPS cap: {error}"),
        }
    }
    fn apply_vsync(&mut self) {
        let mode = if self.vsync {
            wgpu::PresentMode::AutoVsync
        } else {
            wgpu::PresentMode::AutoNoVsync
        };
        if let Some(renderer) = self.renderer.as_mut() {
            renderer.ctx.set_present_mode(mode);
        }
    }
}

fn render_no_camera_warning(ctx: &egui::Context) {
    egui::Area::new(egui::Id::new("no_camera_warning"))
        .anchor(egui::Align2::CENTER_CENTER, egui::vec2(0.0, 0.0))
        .order(egui::Order::Foreground)
        .show(ctx, |ui| {
            egui::Frame {
                fill: egui::Color32::from_rgba_premultiplied(20, 20, 20, 220),
                inner_margin: egui::Margin::same(20.0),
                rounding: egui::Rounding::same(8.0),
                ..Default::default()
            }
            .show(ui, |ui| {
                ui.visuals_mut().override_text_color = Some(egui::Color32::YELLOW);
                ui.label(
                    egui::RichText::new("⚠  No active camera")
                        .font(egui::FontId::proportional(22.0))
                        .strong(),
                );
                ui.add_space(4.0);
                ui.visuals_mut().override_text_color = Some(egui::Color32::from_gray(180));
                ui.label(
                    egui::RichText::new(
                        "Attach core:camera + core:transform to an entity\nand call Engine.setMainCamera(entity.id) to see the scene.",
                    )
                    .font(egui::FontId::proportional(14.0)),
                );
            });
        });
}

fn render_debug_overlay(
    ctx: &egui::Context,
    fps: f32,
    tick_rate: f64,
    entity_count: usize,
    fps_cap: Option<f64>,
    vsync: bool,
) {
    egui::Area::new(egui::Id::new("debug_overlay"))
        .fixed_pos(egui::pos2(8.0, 8.0))
        .order(egui::Order::Foreground)
        .show(ctx, |ui| {
            egui::Frame {
                fill: egui::Color32::from_rgba_premultiplied(0, 0, 0, 180),
                inner_margin: egui::Margin::same(8.0),
                rounding: egui::Rounding::same(4.0),
                ..Default::default()
            }
            .show(ui, |ui| {
                let font = egui::FontId::monospace(13.0);
                ui.visuals_mut().override_text_color = Some(egui::Color32::WHITE);
                ui.label(egui::RichText::new(format!("FPS        {fps:.1}")).font(font.clone()));
                ui.label(
                    egui::RichText::new(format!("Tick rate  {tick_rate:.0}/s")).font(font.clone()),
                );
                ui.label(
                    egui::RichText::new(format!("Entities   {entity_count}")).font(font.clone()),
                );
                let cap_str = fps_cap.map_or_else(|| "off".to_owned(), |c| format!("{c:.0}"));
                ui.label(egui::RichText::new(format!("FPS cap    {cap_str}")).font(font.clone()));
                ui.label(
                    egui::RichText::new(format!("VSync      {}", if vsync { "on" } else { "off" }))
                        .font(font),
                );
            });
        });
}

#[cfg(windows)]
fn embedded_window_attributes(
    attributes: winit::window::WindowAttributes,
) -> anyhow::Result<winit::window::WindowAttributes> {
    use anyhow::Context;
    use std::num::NonZeroIsize;
    use windows::Win32::{Foundation::HWND, UI::WindowsAndMessaging::IsWindow};
    use winit::raw_window_handle::{RawWindowHandle, Win32WindowHandle};
    let Ok(parent) = std::env::var("WHATEVER_EDITOR_PARENT") else {
        return Ok(attributes);
    };
    let handle = NonZeroIsize::new(parent.parse()?).context("invalid editor window handle")?;
    anyhow::ensure!(
        unsafe { IsWindow(HWND(handle.get() as _)).as_bool() },
        "editor window closed"
    );
    let bounds = std::env::var("WHATEVER_EDITOR_RECT")?
        .split(',')
        .map(str::parse::<i32>)
        .collect::<Result<Vec<_>, _>>()?;
    anyhow::ensure!(
        bounds.len() == 4 && bounds[2] > 0 && bounds[3] > 0,
        "invalid editor viewport bounds"
    );
    // The editor owns this live parent for the entire Play session and kills its child on exit.
    let attributes = unsafe {
        attributes.with_parent_window(Some(RawWindowHandle::Win32(Win32WindowHandle::new(handle))))
    };
    Ok(attributes
        .with_decorations(false)
        .with_resizable(false)
        .with_position(winit::dpi::PhysicalPosition::new(bounds[0], bounds[1]))
        .with_inner_size(winit::dpi::PhysicalSize::new(
            bounds[2] as u32,
            bounds[3] as u32,
        )))
}

impl ApplicationHandler for Engine {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if std::env::var_os("WHATEVER_EDITOR_EMBEDDED").is_some() {
            event_loop.listen_device_events(winit::event_loop::DeviceEvents::Always);
        }
        if self.window.is_some() {
            return;
        }

        let icon = crate::runtime::installation_root()
            .ok()
            .and_then(|root| image::open(root.join("branding/icon.png")).ok())
            .and_then(|image| {
                let image = image.to_rgba8();
                let (width, height) = image.dimensions();
                winit::window::Icon::from_rgba(image.into_raw(), width, height).ok()
            });
        let attrs = Window::default_attributes()
            .with_title(&self.game_name)
            .with_window_icon(icon)
            .with_visible(false);
        #[cfg(windows)]
        let attrs = match embedded_window_attributes(attrs) {
            Ok(attrs) => attrs,
            Err(error) => {
                tracing::error!("embedded player window: {error:#}");
                event_loop.exit();
                return;
            }
        };
        let window = match event_loop.create_window(attrs) {
            Ok(w) => Arc::new(w),
            Err(e) => {
                tracing::error!("create window: {e}");
                event_loop.exit();
                return;
            }
        };
        self.debug.window("window created");

        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build();
        let rt = match rt {
            Ok(r) => r,
            Err(e) => {
                tracing::error!("{e}");
                event_loop.exit();
                return;
            }
        };

        let ctx = rt.block_on(WgpuContext::new(Arc::clone(&window)));
        let ctx = match ctx {
            Ok(c) => c,
            Err(e) => {
                tracing::error!("wgpu init: {e}");
                event_loop.exit();
                return;
            }
        };

        // egui_winit state needs a display handle source (event_loop satisfies HasDisplayHandle)
        self.egui_state = Some(egui_winit::State::new(
            self.egui_ctx.clone(),
            egui::ViewportId::ROOT,
            event_loop,
            None,
            None, // theme
            None, // max_texture_side
        ));

        let renderer = rt.block_on(Renderer::new(ctx));
        let renderer = match renderer {
            Ok(r) => r,
            Err(e) => {
                tracing::error!("renderer init: {e}");
                event_loop.exit();
                return;
            }
        };

        self.renderer = Some(renderer);
        self.window = Some(Arc::clone(&window));
        self.set_cursor_captured(false);
        window.set_visible(true);
        if std::env::var_os("WHATEVER_EDITOR_EMBEDDED").is_none() {
            window.focus_window();
            self.debug.window(&format!(
                "player ready; startup focus={}",
                window.has_focus()
            ));
        }
        self.simulation.ready();

        window.request_redraw();
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, _id: WindowId, event: WindowEvent) {
        #[cfg(windows)]
        if matches!(
            event,
            WindowEvent::MouseInput {
                state: ElementState::Pressed,
                ..
            }
        ) && std::env::var_os("WHATEVER_EDITOR_EMBEDDED").is_some()
            && let Some(window) = &self.window
        {
            use winit::raw_window_handle::{HasWindowHandle, RawWindowHandle};
            if let Ok(handle) = window.window_handle()
                && let RawWindowHandle::Win32(handle) = handle.as_raw()
            {
                // Child windows do not receive keyboard focus automatically on a mouse click.
                unsafe {
                    let _ = windows::Win32::UI::Input::KeyboardAndMouse::SetFocus(
                        windows::Win32::Foundation::HWND(handle.hwnd.get() as _),
                    );
                }
            }
        }
        // F2 avoids the Windows Alt+Enter fullscreen shortcut.
        if let WindowEvent::KeyboardInput { ref event, .. } = event
            && let PhysicalKey::Code(code) = event.physical_key
        {
            if code == KeyCode::F2 {
                if event.state == ElementState::Pressed && !event.repeat {
                    self.console.toggle();
                    if self.console.is_open {
                        self.set_cursor_captured(false);
                        self.input.keys_pressed.clear();
                    }
                    self.sync_input();
                }
                return;
            }
            // F1: toggle debug overlay
            if code == KeyCode::F1 && event.state == ElementState::Pressed {
                self.debug_overlay = !self.debug_overlay;
                return;
            }
            // Escape: clear autocomplete first, close console only when already clear
            if code == KeyCode::Escape
                && event.state == ElementState::Pressed
                && self.console.is_open
            {
                self.console.escape();
                return;
            }
        }

        // Feed event to egui
        let egui_consumed =
            if let (Some(state), Some(window)) = (self.egui_state.as_mut(), self.window.as_ref()) {
                state.on_window_event(window, &event).consumed
            } else {
                false
            };

        // If console is open and egui handled it, don't propagate to game input
        if self.console.is_open && egui_consumed {
            // Still track physical key state so modifier detection stays accurate
            if let WindowEvent::KeyboardInput { ref event, .. } = event
                && let PhysicalKey::Code(code) = event.physical_key
            {
                match event.state {
                    ElementState::Pressed => self.input.press(code),
                    ElementState::Released => self.input.release(code),
                }
            }
            return;
        }

        match event {
            WindowEvent::Focused(false) => {
                #[cfg(windows)]
                if std::env::var_os("WHATEVER_EDITOR_EMBEDDED").is_some()
                    && self
                        .window
                        .as_ref()
                        .is_some_and(|w| crate::cursor::EmbeddedCursor::focused(w))
                {
                    return;
                }
                self.set_cursor_captured(false);
                self.input.keys_pressed.clear();
                self.input.mouse_delta = (0.0, 0.0);
            }
            WindowEvent::CloseRequested => {
                self.debug.window("window close requested");
                if let Err(error) = self.simulation.shutdown() {
                    tracing::error!("shutdown: {error:#}");
                }
                event_loop.exit();
            }
            WindowEvent::Resized(size) => {
                self.debug
                    .window(&format!("resized to {}x{}", size.width, size.height));
                if let Some(r) = self.renderer.as_mut() {
                    r.resize(size.width, size.height);
                }
            }
            WindowEvent::RedrawRequested => {
                // OS redraw requests must obey the same cap as scheduled frames.
                if !self.frame_clock.ready(Instant::now()) {
                    if let Some(deadline) = self.frame_clock.deadline() {
                        event_loop.set_control_flow(ControlFlow::WaitUntil(deadline));
                    }
                    return;
                }
                self.frame();
                if self.should_quit {
                    self.debug.window("quit command issued");
                    if let Err(error) = self.simulation.shutdown() {
                        tracing::error!("shutdown: {error:#}");
                    }
                    event_loop.exit();
                    self.sync_input();
                    return;
                }
                self.frame_clock.complete(self.last_frame);
            }
            WindowEvent::MouseInput {
                state: ElementState::Pressed,
                ..
            } if !self.console.is_open && !self.input.mouse_captured => {
                self.set_cursor_captured(true);
            }
            WindowEvent::KeyboardInput { event, .. } => {
                if let PhysicalKey::Code(code) = event.physical_key {
                    if event.state == ElementState::Pressed {
                        self.input.press(code);
                        // Escape releases the mouse cursor when captured.
                        if code == KeyCode::Escape && self.input.mouse_captured {
                            self.set_cursor_captured(false);
                        }
                    } else {
                        self.input.release(code);
                    }
                }
            }
            _ => {}
        }
        if self
            .editor_link
            .as_ref()
            .is_some_and(|link| link.inspecting())
        {
            self.set_cursor_captured(false);
            self.input.keys_pressed.clear();
            self.input.mouse_delta = (0.0, 0.0);
        }
        self.sync_input();
    }

    fn device_event(&mut self, _event_loop: &ActiveEventLoop, _id: DeviceId, event: DeviceEvent) {
        if let DeviceEvent::MouseMotion { delta: (dx, dy) } = event {
            if !self.input.mouse_captured {
                return;
            }
            if self
                .editor_link
                .as_ref()
                .is_some_and(|link| link.inspecting())
            {
                return;
            }
            self.input.accumulate_mouse(dx as f32, dy as f32);
            self.sync_input();
        }
    }

    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
        if self.frame_clock.ready(Instant::now()) {
            event_loop.set_control_flow(ControlFlow::Wait);
            if let Some(window) = &self.window {
                window.request_redraw();
            }
        } else if let Some(deadline) = self.frame_clock.deadline() {
            event_loop.set_control_flow(ControlFlow::WaitUntil(deadline));
        }
    }
}

// ---------------------------------------------------------------------------
// Lighting helpers
// ---------------------------------------------------------------------------
