use crate::console::command_node_from_spec;
use crate::console::{ConsoleAction, DevConsole};
use crate::debug::{DebugConfig, DebugLogger};
use crate::input::InputState;
use crate::mods::{GameMeta, ModRegistry, discover_and_load};
use crate::renderer::{EguiOutput, Renderer, WgpuContext, grid_pos, load_from_vfs};
use crate::script::ipc::{EngineMessage, ScriptMessage};
use crate::script::{ScriptHost, dispatch};
use crate::vfs::{LayeredVfs, VfsHandle, VfsPath};
use anyhow::Context;
use std::path::Path;
use std::sync::Arc;
use std::time::Instant;
use winit::application::ApplicationHandler;
use winit::event::{DeviceEvent, DeviceId, ElementState, WindowEvent};
use winit::event_loop::ActiveEventLoop;
use winit::keyboard::{KeyCode, PhysicalKey};
use winit::window::{CursorGrabMode, Window, WindowId};

pub struct Engine {
    debug: DebugLogger,
    debug_mirror: Arc<std::sync::Mutex<Vec<String>>>,
    game_meta: GameMeta,
    vfs: VfsHandle,
    registry: ModRegistry,
    script_host: ScriptHost,
    input: InputState,
    frame_number: u64,
    last_frame: Instant,
    renderer: Option<Renderer>,
    window: Option<Arc<Window>>,
    console: DevConsole,
    egui_ctx: egui::Context,
    egui_state: Option<egui_winit::State>,
    should_quit: bool,
}

impl Engine {
    pub fn new(debug_config: &DebugConfig, cwd: &Path) -> anyhow::Result<Self> {
        let mut debug = DebugLogger::new(debug_config, cwd)?;

        let debug_mirror = debug.console_mirror();
        let mut vfs = LayeredVfs::new();
        vfs.set_log(debug.vfs_writer(), Arc::clone(&debug_mirror), debug.shared_switches());
        let mut registry = ModRegistry::new();

        let mods_dir = cwd.join("mods");
        let mods_user_dir = cwd.join("mods_user");
        discover_and_load(
            &[mods_dir.as_path(), mods_user_dir.as_path()],
            &mut vfs,
            &mut registry,
            &mut debug,
        )?;

        let meta_path = cwd.join("mods").join("core").join("meta.toml");
        let game_meta = if meta_path.exists() {
            let src = std::fs::read_to_string(&meta_path)
                .with_context(|| format!("reading {}", meta_path.display()))?;
            toml::from_str::<GameMeta>(&src)
                .with_context(|| format!("parsing {}", meta_path.display()))?
        } else {
            GameMeta::default()
        };

        let vfs: VfsHandle = Arc::new(vfs);

        let mut script_host = ScriptHost::new();
        for loaded_mod in registry.iter() {
            let Some(ref script_cfg) = loaded_mod.manifest.script else {
                continue;
            };
            let entry = loaded_mod.root.join(&script_cfg.entry);
            let mod_id = &loaded_mod.manifest.meta.id;
            if let Err(e) = script_host.spawn(mod_id, &entry, &mut debug) {
                tracing::error!(mod_id, entry = %entry.display(), "script spawn failed: {e:#}");
            }
        }

        Ok(Engine {
            debug,
            debug_mirror,
            game_meta,
            vfs,
            registry,
            script_host,
            input: InputState::new(),
            frame_number: 0,
            last_frame: Instant::now(),
            renderer: None,
            window: None,
            console: DevConsole::new(),
            egui_ctx: egui::Context::default(),
            egui_state: None,
            should_quit: false,
        })
    }

    fn populate_scene(&mut self) {
        let Some(renderer) = self.renderer.as_mut() else {
            return;
        };
        let mut index = 0usize;

        let mod_ids: Vec<String> = self.registry.mod_ids().map(str::to_owned).collect();
        for mod_id in &mod_ids {
            let paths = match self.vfs.list(mod_id, "") {
                Ok(p) => p,
                Err(e) => {
                    tracing::warn!("vfs list error for {mod_id}: {e}");
                    continue;
                }
            };
            for rel_path in paths {
                if !rel_path.ends_with(".png") {
                    continue;
                }
                let vfs_path = VfsPath {
                    mod_id: mod_id.clone(),
                    path: rel_path.clone(),
                };
                match load_from_vfs(
                    &renderer.ctx.device,
                    &renderer.ctx.queue,
                    self.vfs.as_ref(),
                    &vfs_path,
                ) {
                    Ok(tex) => {
                        let pos = grid_pos(index);
                        renderer.scene.add_sprite(&renderer.ctx.device, tex, pos);
                        index += 1;
                    }
                    Err(e) => {
                        tracing::warn!("failed to load texture {}: {e}", vfs_path.as_string())
                    }
                }
            }
        }
    }

    fn frame(&mut self) {
        if let Ok(mut lines) = self.debug_mirror.lock() {
            for line in lines.drain(..) {
                self.console.push_debug_line(line);
            }
        }

        let now = Instant::now();
        let dt = now.duration_since(self.last_frame).as_secs_f32();
        self.last_frame = now;

        self.console.fps = if dt > 0.0 { 1.0 / dt } else { 0.0 };

        let (dx, dy) = self.input.flush_mouse();

        if let Some(renderer) = self.renderer.as_mut() {
            if self.input.mouse_captured {
                renderer
                    .camera_controller
                    .process_mouse(&mut renderer.camera, dx, dy);
            }
            renderer.camera_controller.update(&mut renderer.camera, dt);
        }

        let messages = self.script_host.drain_messages(&mut self.debug);
        self.dispatch_messages(messages);

        self.frame_number += 1;

        // Run egui for this frame
        let egui_raw_input = if let (Some(state), Some(window)) =
            (self.egui_state.as_mut(), self.window.as_ref())
        {
            Some(state.take_egui_input(window))
        } else {
            None
        };

        let egui_render_data = if let Some(raw_input) = egui_raw_input {
            let egui_ctx = self.egui_ctx.clone();
            let vfs = Arc::clone(&self.vfs);

            let debug = self.debug.shared_switches();
            let mut console_action = ConsoleAction::None;
            let full_output = egui_ctx.run(raw_input, |ctx| {
                console_action = self.console.render(ctx, &self.registry, vfs.as_ref(), Arc::clone(&debug));
            });

            match console_action {
                ConsoleAction::Quit => self.should_quit = true,
                ConsoleAction::SendIpc { mod_id, message } => {
                    self.script_host.send(&mod_id, &message, &mut self.debug);
                }
                ConsoleAction::None => {}
            }

            if let (Some(state), Some(window)) =
                (self.egui_state.as_mut(), self.window.as_ref())
            {
                state.handle_platform_output(window, full_output.platform_output);
            }

            let paint_jobs =
                egui_ctx.tessellate(full_output.shapes, full_output.pixels_per_point);

            Some((paint_jobs, full_output.textures_delta, full_output.pixels_per_point))
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

        if let Some(renderer) = self.renderer.as_mut() {
            if let Err(e) = renderer.render(egui_out.as_ref()) {
                tracing::error!("render error: {e}");
            }
        }
    }

    fn set_cursor_captured(&mut self, captured: bool) {
        self.input.mouse_captured = captured;
        if let Some(window) = self.window.as_ref() {
            if captured {
                let _ = window
                    .set_cursor_grab(CursorGrabMode::Confined)
                    .or_else(|_| window.set_cursor_grab(CursorGrabMode::Locked));
                window.set_cursor_visible(false);
            } else {
                let _ = window.set_cursor_grab(CursorGrabMode::None);
                window.set_cursor_visible(true);
            }
        }
    }

    fn dispatch_messages(&mut self, messages: Vec<(String, ScriptMessage)>) {
        if let Some(window) = self.window.as_ref() {
            let window = Arc::clone(window);
            for (mod_id, msg) in messages {
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
                        self.console.handle_command_response(output.clone(), error.clone());
                    }
                    continue;
                }

                dispatch(
                    &mod_id,
                    msg,
                    &window,
                    &mut self.script_host,
                    &self.registry,
                    &self.game_meta.game.id,
                    &mut self.debug,
                );
            }
        }
    }
}

impl ApplicationHandler for Engine {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.window.is_some() {
            return;
        }

        let attrs = Window::default_attributes().with_title(&self.game_meta.game.name);
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

        let renderer = rt.block_on(Renderer::new(ctx, self.vfs.as_ref()));
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

        self.populate_scene();

        let engine_version = env!("CARGO_PKG_VERSION").to_owned();
        let mod_ids: Vec<String> = self.script_host.mod_ids().map(str::to_owned).collect();
        for mod_id in mod_ids {
            let init_msg = EngineMessage::Init {
                mod_id: mod_id.clone(),
                engine_version: engine_version.clone(),
            };
            self.script_host.send(&mod_id, &init_msg, &mut self.debug);
        }

        window.request_redraw();
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, _id: WindowId, event: WindowEvent) {
        // Check Ctrl+Alt+Enter console toggle BEFORE anything else
        if let WindowEvent::KeyboardInput { ref event, .. } = event {
            if let PhysicalKey::Code(code) = event.physical_key {
                if code == KeyCode::Enter && event.state == ElementState::Pressed {
                    let ctrl = self.input.is_pressed(KeyCode::ControlLeft)
                        || self.input.is_pressed(KeyCode::ControlRight);
                    let alt = self.input.is_pressed(KeyCode::AltLeft)
                        || self.input.is_pressed(KeyCode::AltRight);
                    if ctrl && alt {
                        self.console.toggle();
                        if self.console.is_open {
                            self.set_cursor_captured(false);
                            // Release all held keys so camera doesn't get stuck
                            if let Some(r) = self.renderer.as_mut() {
                                r.camera_controller.release_all();
                            }
                            self.input.keys_pressed.clear();
                        }
                        return;
                    }
                }
                // Escape closes the console when it is open
                if code == KeyCode::Escape
                    && event.state == ElementState::Pressed
                    && self.console.is_open
                {
                    self.console.toggle();
                    return;
                }
            }
        }

        // Feed event to egui
        let egui_consumed = if let (Some(state), Some(window)) =
            (self.egui_state.as_mut(), self.window.as_ref())
        {
            state.on_window_event(window, &event).consumed
        } else {
            false
        };

        // If console is open and egui handled it, don't propagate to game input
        if self.console.is_open && egui_consumed {
            // Still track physical key state so modifier detection stays accurate
            if let WindowEvent::KeyboardInput { ref event, .. } = event {
                if let PhysicalKey::Code(code) = event.physical_key {
                    match event.state {
                        ElementState::Pressed => self.input.press(code),
                        ElementState::Released => self.input.release(code),
                    }
                }
            }
            return;
        }

        match event {
            WindowEvent::CloseRequested => {
                self.debug.window("window close requested");
                let final_messages = self.script_host.shutdown_all(0, &mut self.debug);
                self.dispatch_messages(final_messages);
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
                self.frame();
                if self.should_quit {
                    self.debug.window("quit command issued");
                    let final_messages = self.script_host.shutdown_all(0, &mut self.debug);
                    self.dispatch_messages(final_messages);
                    event_loop.exit();
                    return;
                }
                if let Some(w) = self.window.as_ref() {
                    w.request_redraw();
                }
            }
            WindowEvent::MouseInput {
                state: ElementState::Pressed,
                ..
            } => {
                if !self.console.is_open {
                    self.set_cursor_captured(!self.input.mouse_captured);
                }
            }
            WindowEvent::KeyboardInput { event, .. } => {
                if let PhysicalKey::Code(code) = event.physical_key {
                    if event.state == ElementState::Pressed {
                        self.input.press(code);
                        if let Some(r) = self.renderer.as_mut() {
                            r.camera_controller.process_key(code, true);
                        }
                        if code == KeyCode::Escape {
                            self.set_cursor_captured(false);
                        }
                    } else {
                        self.input.release(code);
                        if let Some(r) = self.renderer.as_mut() {
                            r.camera_controller.process_key(code, false);
                        }
                    }
                }
            }
            _ => {}
        }
    }

    fn device_event(&mut self, _event_loop: &ActiveEventLoop, _id: DeviceId, event: DeviceEvent) {
        if let DeviceEvent::MouseMotion { delta: (dx, dy) } = event {
            self.input.accumulate_mouse(dx as f32, dy as f32);
        }
    }
}
