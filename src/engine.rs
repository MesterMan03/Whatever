use crate::audio::AudioManager;
use crate::console::command_node_from_spec;
use crate::console::{ConsoleAction, DevConsole, EngineSettingAction};
use crate::debug::{DebugConfig, DebugLogger};
use crate::ecs::{EntityId, World};
use crate::input::InputState;
use crate::logging::{LogMirror, SharedLogWriter};
use crate::mods::{GameMeta, ModRegistry, discover_and_load};
use crate::renderer::{
    EguiOutput, GpuDirectionalLight, GpuLightingData, GpuPointLight, MeshUpdateTarget,
    RenderContext, Renderer, SpriteUpdateTarget, WgpuContext, view_proj_from_entity,
};
use crate::sandbox::SandboxConfig;
use crate::script::ipc::{EngineMessage, ScriptMessage};
use crate::script::{
    DispatchResult, EngineContext, RecvOutcome, RenderCommand, ScriptHost, dispatch, mod_data_root,
};
use crate::vfs::{LayeredVfs, Vfs, VfsHandle};
use crate::watchdog::Watchdog;
use anyhow::Context;
use std::collections::HashSet;
use std::path::Path;
use std::sync::Arc;
use std::time::{Duration, Instant};
use winit::application::ApplicationHandler;
use winit::event::{DeviceEvent, DeviceId, ElementState, WindowEvent};
use winit::event_loop::{ActiveEventLoop, ControlFlow};
use winit::keyboard::{KeyCode, PhysicalKey};
use winit::window::{CursorGrabMode, Window, WindowId};

pub struct Engine {
    debug: DebugLogger,
    debug_mirror: Arc<std::sync::Mutex<Vec<String>>>,
    log_mirror: LogMirror,
    game_meta: GameMeta,
    vfs: VfsHandle,
    registry: ModRegistry,
    script_host: ScriptHost,
    input: InputState,
    frame_number: u64,
    last_frame: Instant,
    fps_ema: f32,
    fps_cap: Option<f64>,
    vsync: bool,
    next_frame_target: Instant,
    renderer: Option<Renderer>,
    window: Option<Arc<Window>>,
    world: World,
    /// The entity whose `core:camera` + `core:transform` is used as the active
    /// camera.  `None` means no camera is set; the renderer clears to black.
    main_camera: Option<EntityId>,
    tick_number: u64,
    last_tick: Instant,
    tick_interval: Duration,
    tick_subscribers: HashSet<String>,
    watchdog: Watchdog,
    console: DevConsole,
    debug_overlay: bool,
    egui_ctx: egui::Context,
    egui_state: Option<egui_winit::State>,
    should_quit: bool,
    audio: AudioManager,
}

impl Engine {
    pub fn new(
        debug_config: &DebugConfig,
        cwd: &Path,
        mods_override: Option<&Path>,
        log_mirror: LogMirror,
        log_writer: SharedLogWriter,
    ) -> anyhow::Result<Self> {
        let mut debug = DebugLogger::new(debug_config, log_writer);

        let debug_mirror = debug.console_mirror();
        let mut vfs = LayeredVfs::new();
        vfs.set_log(
            debug.log_writer(),
            Arc::clone(&debug_mirror),
            debug.shared_switches(),
        );
        let mut registry = ModRegistry::new();

        let (mods_dirs, meta_path) = if let Some(override_path) = mods_override {
            (
                vec![override_path.to_path_buf()],
                override_path.join("core").join("meta.toml"),
            )
        } else {
            (
                vec![cwd.join("mods"), cwd.join("mods_user")],
                cwd.join("mods").join("core").join("meta.toml"),
            )
        };
        let mods_dir_refs: Vec<&Path> = mods_dirs.iter().map(|p| p.as_path()).collect();
        discover_and_load(&mods_dir_refs, &mut vfs, &mut registry, &mut debug)?;
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

            let mod_data_dir = match mod_data_root(&game_meta.game.id, mod_id) {
                Ok(d) => d,
                Err(e) => {
                    tracing::error!(mod_id, "could not determine mod data dir: {e:#}");
                    continue;
                }
            };
            if let Err(e) = std::fs::create_dir_all(&mod_data_dir) {
                tracing::error!(mod_id, "could not create mod data dir: {e:#}");
                continue;
            }

            let sandbox_cfg = SandboxConfig {
                mod_id: mod_id.clone(),
                mod_root: loaded_mod.root.clone(),
                mod_data_dir,
                engine_root: cwd.to_path_buf(),
            };

            if let Err(e) = script_host.spawn(mod_id, &entry, &sandbox_cfg, &mut debug) {
                tracing::error!(mod_id, entry = %entry.display(), "script spawn failed: {e:#}");
            }
        }

        let tick_interval = Duration::from_secs_f64(1.0 / game_meta.game.tick_rate);

        Ok(Engine {
            debug,
            debug_mirror,
            log_mirror,
            game_meta,
            vfs,
            registry,
            script_host,
            world: World::new(),
            main_camera: None,
            tick_number: 0,
            last_tick: Instant::now(),
            tick_interval,
            tick_subscribers: HashSet::new(),
            watchdog: Watchdog::new(Duration::from_millis(500)),
            input: InputState::new(),
            frame_number: 0,
            last_frame: Instant::now(),
            fps_ema: 0.0,
            fps_cap: None,
            vsync: true,
            next_frame_target: Instant::now(),
            renderer: None,
            window: None,
            console: DevConsole::new(),
            debug_overlay: false,
            egui_ctx: egui::Context::default(),
            egui_state: None,
            should_quit: false,
            audio: AudioManager::new()?,
        })
    }

    fn frame(&mut self) {
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

        let now = Instant::now();
        let dt = now.duration_since(self.last_frame).as_secs_f32();
        self.last_frame = now;

        let raw_fps = if dt > 0.0 { 1.0 / dt } else { self.fps_ema };
        self.fps_ema = if self.fps_ema == 0.0 {
            raw_fps
        } else {
            self.fps_ema * 0.9 + raw_fps * 0.1
        };
        self.console.fps = self.fps_ema;
        self.console.fps_cap = self.fps_cap;
        self.console.vsync = self.vsync;

        let messages = self.script_host.drain_messages(&mut self.debug);
        self.dispatch_messages(messages);

        for (mod_id, audio_id) in self.audio.poll_finished() {
            self.script_host.send(
                &mod_id,
                &EngineMessage::AudioClose { audio_id },
                &mut self.debug,
            );
        }

        // Flush mouse only when a tick fires so accumulated delta isn't lost
        // across the many frames that may occur between ticks at high FPS.
        let now = Instant::now();
        while now.duration_since(self.last_tick) >= self.tick_interval {
            self.last_tick += self.tick_interval;
            let (dx, dy) = self.input.flush_mouse();
            self.run_tick(dx, dy);
        }

        self.frame_number += 1;

        // Compute camera view-projection and position for this frame.
        let (camera_vp, camera_pos) = self.compute_camera_info();
        let no_camera = camera_vp.is_none();

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
                    self.script_host.send(&mod_id, &message, &mut self.debug);
                }
                ConsoleAction::EngineSettings(action) => match action {
                    EngineSettingAction::SetFpsCap(cap) => self.fps_cap = cap,
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

        if let Some(renderer) = self.renderer.as_mut() {
            renderer.update_lighting(&collect_lights(&self.world));
            if let Err(e) =
                renderer.render(camera_vp, camera_pos, egui_out.as_ref(), self.vfs.as_ref())
            {
                tracing::error!("render error: {e}");
            }
        }
    }

    /// Derive a view-projection matrix and world-space camera position from the
    /// current main camera entity.  Returns `(None, None)` if no camera is set,
    /// the entity is stale, or it is missing `core:transform` / `core:camera`.
    fn compute_camera_info(&self) -> (Option<glam::Mat4>, Option<glam::Vec3>) {
        let Some(camera_id) = self.main_camera.as_ref() else {
            return (None, None);
        };
        if !self.world.is_alive(camera_id) {
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
        self.input.mouse_captured = captured;
        if let Some(window) = self.window.as_ref() {
            if captured {
                // Prefer Locked (cursor position frozen, ideal for camera look).
                // Fall back to Confined (cursor stays inside the window) on
                // platforms that don't support Locked.
                let _ = window
                    .set_cursor_grab(CursorGrabMode::Locked)
                    .or_else(|_| window.set_cursor_grab(CursorGrabMode::Confined));
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
            let vfs = Arc::clone(&self.vfs);
            for (mod_id, msg) in messages {
                // Intercept tick subscriptions before the general dispatcher
                if let ScriptMessage::Subscribe { ref events } = msg {
                    if events.iter().any(|e| e == "Tick") {
                        self.tick_subscribers.insert(mod_id.clone());
                        tracing::debug!(mod_id, "subscribed to tick");
                    }
                    continue;
                }

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

                let ctx = EngineContext {
                    window: &window,
                    script_host: &mut self.script_host,
                    registry: &self.registry,
                    game_id: &self.game_meta.game.id,
                    debug: &mut self.debug,
                    world: &mut self.world,
                    vfs: vfs.as_ref(),
                    audio: &mut self.audio,
                };

                let result = dispatch(&mod_id, msg, ctx);
                self.apply_dispatch_result(result);
            }
        }
    }

    fn apply_dispatch_result(&mut self, result: DispatchResult) {
        if let Some(rate) = result.new_tick_rate {
            self.tick_interval = Duration::from_secs_f64(1.0 / rate);
        }
        if let Some(cap) = result.new_fps_cap {
            self.fps_cap = cap;
        }
        if let Some(enabled) = result.new_vsync {
            self.vsync = enabled;
            self.apply_vsync();
        }
        if let Some(cam) = result.set_main_camera {
            self.main_camera = cam;
        }
        if let Some(renderer) = self.renderer.as_mut() {
            for cmd in &result.render_cmds {
                apply_render_command(cmd, renderer, &self.world, self.vfs.as_ref());
            }
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

    fn run_tick(&mut self, mouse_dx: f32, mouse_dy: f32) {
        self.tick_number += 1;

        // When the dev console is open, suppress raw input so typing in the
        // console doesn't also trigger in-game actions.
        let (eff_dx, eff_dy, keys_pressed) = if self.console.is_open {
            (0.0_f32, 0.0_f32, Vec::new())
        } else {
            let keys = self
                .input
                .keys_pressed
                .iter()
                .map(|k| format!("{k:?}"))
                .collect();
            (mouse_dx, mouse_dy, keys)
        };

        let subscribers: Vec<String> = self.tick_subscribers.iter().cloned().collect();
        if subscribers.is_empty() {
            return;
        }

        let tick_msg = EngineMessage::Tick {
            tick_number: self.tick_number,
            delta_seconds: self.tick_interval.as_secs_f64(),
            keys_pressed,
            mouse_delta: [eff_dx, eff_dy],
        };

        // Send tick to each subscriber.  If the send fails (broken pipe /
        // dead process) count that subscriber as immediately done so the wait
        // loop below doesn't block forever.
        let mut done: HashSet<String> = HashSet::new();
        for id in &subscribers {
            if !self.script_host.send(id, &tick_msg, &mut self.debug) {
                done.insert(id.clone());
            }
        }

        self.watchdog.start_tick();
        while done.len() < subscribers.len() {
            match self
                .script_host
                .recv_blocking(Duration::from_millis(100), &mut self.debug)
            {
                RecvOutcome::Message(mod_id, ScriptMessage::TickDone { tick_number: n })
                    if n == self.tick_number =>
                {
                    done.insert(mod_id);
                }
                RecvOutcome::Message(mod_id, msg) => {
                    // Mirror the same early-interception logic used in
                    // dispatch_messages().  Async command handlers (and other
                    // console messages) can resolve while the engine is
                    // blocked waiting for TickDone; without these guards they
                    // would fall through to dispatch() and trigger spurious
                    // "reached dispatcher unexpectedly" warnings.

                    if let ScriptMessage::Subscribe { ref events } = msg {
                        if events.iter().any(|e| e == "Tick") {
                            self.tick_subscribers.insert(mod_id.clone());
                            tracing::debug!(mod_id, "subscribed to tick (mid-tick)");
                        }
                        continue;
                    }

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

                    let Some(window) = self.window.as_ref() else {
                        break;
                    };
                    let window = Arc::clone(window);
                    let vfs = Arc::clone(&self.vfs);
                    let ctx = EngineContext {
                        window: &window,
                        script_host: &mut self.script_host,
                        registry: &self.registry,
                        game_id: &self.game_meta.game.id,
                        debug: &mut self.debug,
                        world: &mut self.world,
                        vfs: vfs.as_ref(),
                        audio: &mut self.audio,
                    };
                    let result = dispatch(&mod_id, msg, ctx);
                    self.apply_dispatch_result(result);
                }
                // All stdout reader threads have exited — every process is
                // dead.  No TickDone will ever arrive; bail out.
                RecvOutcome::Disconnected => break,
                RecvOutcome::Timeout => {}
            }
        }
        self.watchdog.end_tick();
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

fn apply_render_command(
    cmd: &RenderCommand,
    renderer: &mut Renderer,
    world: &World,
    vfs: &dyn Vfs,
) {
    match cmd {
        RenderCommand::UpsertSprite { entity_idx } => {
            let entity_id = world
                .allocator
                .alive_entity_ids()
                .find(|e| e.index == *entity_idx);
            let Some(entity_id) = entity_id else { return };
            let Some(world_transform) = world.world_transform(&entity_id) else {
                return;
            };
            let Some(sprite) = world.sprite_renderers.get(entity_idx) else {
                return;
            };
            let target = SpriteUpdateTarget {
                entity_idx: *entity_idx,
                transform: &world_transform,
                sprite,
            };
            let ctx = RenderContext {
                device: &renderer.ctx.device,
                queue: &renderer.ctx.queue,
                bgl: &renderer.texture_bind_group_layout,
            };
            if let Err(e) = renderer.scene.update_sprite(vfs, target, ctx) {
                tracing::warn!("update_sprite error (entity {}): {e}", entity_idx);
            }
            // Pre-warm pipeline so the first render frame has no stall.
            renderer.get_or_build_pipeline(vfs, &sprite.shader, false);
        }
        RenderCommand::RemoveSprite { entity_idx } => {
            renderer.scene.remove_sprite(*entity_idx);
        }
        RenderCommand::UpsertText { entity_idx } => {
            let entity_id = world
                .allocator
                .alive_entity_ids()
                .find(|e| e.index == *entity_idx);
            let Some(entity_id) = entity_id else { return };
            let Some(world_transform) = world.world_transform(&entity_id) else {
                return;
            };
            let Some(text_comp) = world.text_renderers.get(entity_idx) else {
                return;
            };
            let ctx = RenderContext {
                device: &renderer.ctx.device,
                queue: &renderer.ctx.queue,
                bgl: &renderer.texture_bind_group_layout,
            };
            if let Err(e) =
                renderer
                    .text
                    .upsert_text(ctx, vfs, *entity_idx, &world_transform, text_comp)
            {
                tracing::warn!("upsert_text error (entity {}): {e}", entity_idx);
            }
        }
        RenderCommand::RemoveText { entity_idx } => {
            renderer.text.remove_text(*entity_idx);
        }
        RenderCommand::UpsertMesh { entity_idx } => {
            let entity_id = world
                .allocator
                .alive_entity_ids()
                .find(|e| e.index == *entity_idx);
            let Some(entity_id) = entity_id else { return };
            let Some(world_transform) = world.world_transform(&entity_id) else {
                return;
            };
            let Some(mesh_comp) = world.mesh_renderers.get(entity_idx) else {
                return;
            };
            let target = MeshUpdateTarget {
                entity_idx: *entity_idx,
                transform: &world_transform,
                mesh_renderer: mesh_comp,
            };
            let ctx = RenderContext {
                device: &renderer.ctx.device,
                queue: &renderer.ctx.queue,
                bgl: &renderer.texture_bind_group_layout,
            };
            // Clone shader path before the mutable borrow of renderer.scene.
            let shader_path = mesh_comp.shader.clone();
            if let Err(e) = renderer.scene.update_mesh(vfs, target, ctx) {
                tracing::warn!("update_mesh error (entity {}): {e}", entity_idx);
            }
            // Pre-warm pipeline cache so the first render frame has no stall.
            renderer.get_or_build_pipeline(vfs, &shader_path, true);
        }
        RenderCommand::RemoveMesh { entity_idx } => {
            renderer.scene.remove_mesh(*entity_idx);
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
        if let WindowEvent::KeyboardInput { ref event, .. } = event
            && let PhysicalKey::Code(code) = event.physical_key
        {
            if code == KeyCode::Enter && event.state == ElementState::Pressed {
                let ctrl = self.input.is_pressed(KeyCode::ControlLeft)
                    || self.input.is_pressed(KeyCode::ControlRight);
                let alt = self.input.is_pressed(KeyCode::AltLeft)
                    || self.input.is_pressed(KeyCode::AltRight);
                if ctrl && alt {
                    self.console.toggle();
                    if self.console.is_open {
                        self.set_cursor_captured(false);
                        self.input.keys_pressed.clear();
                    }
                    return;
                }
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
            WindowEvent::CloseRequested => {
                self.debug.window("window close requested");
                let final_messages = self.script_host.shutdown_all(0, &mut self.debug);
                // Clear subscribers so any stray frame() after exit() returns immediately.
                self.tick_subscribers.clear();
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
                    self.tick_subscribers.clear();
                    self.dispatch_messages(final_messages);
                    event_loop.exit();
                    return;
                }
                if let Some(cap) = self.fps_cap {
                    self.next_frame_target = self.last_frame + Duration::from_secs_f64(1.0 / cap);
                    event_loop.set_control_flow(ControlFlow::WaitUntil(self.next_frame_target));
                } else if let Some(w) = self.window.as_ref() {
                    w.request_redraw();
                }
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
    }

    fn device_event(&mut self, _event_loop: &ActiveEventLoop, _id: DeviceId, event: DeviceEvent) {
        if let DeviceEvent::MouseMotion { delta: (dx, dy) } = event {
            self.input.accumulate_mouse(dx as f32, dy as f32);
        }
    }

    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
        if self.fps_cap.is_some() {
            // Only request a redraw once the target time has elapsed; otherwise
            // keep sleeping. Calling request_redraw() unconditionally here would
            // bypass WaitUntil and ignore the cap entirely.
            if Instant::now() >= self.next_frame_target {
                if let Some(w) = self.window.as_ref() {
                    w.request_redraw();
                }
            } else {
                event_loop.set_control_flow(ControlFlow::WaitUntil(self.next_frame_target));
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Lighting helpers
// ---------------------------------------------------------------------------

fn collect_lights(world: &World) -> GpuLightingData {
    use bytemuck::Zeroable;

    let mut data = GpuLightingData::zeroed();

    // Additive ambient: sum all ambient light components.
    for al in world.ambient_lights.values() {
        let c = glam::Vec3::from(al.color) * al.intensity;
        data.ambient_color = (glam::Vec3::from(data.ambient_color) + c).to_array();
        data.ambient_intensity = 1.0;
    }

    // Directional lights (up to 4).
    let mut dir_count = 0u32;
    for dl in world.directional_lights.values().take(4) {
        data.dir_lights[dir_count as usize] = GpuDirectionalLight {
            direction: dl.direction,
            _pad0: 0.0,
            color: dl.color,
            intensity: dl.intensity,
        };
        dir_count += 1;
    }
    data.dir_light_count = dir_count;

    // Point lights (up to 8) — position comes from core:transform.
    let mut pt_count = 0u32;
    for (idx, pl) in world.point_lights.iter().take(8) {
        let position = if let Some(t) = world.transforms.get(idx) {
            t.position
        } else {
            [0.0, 0.0, 0.0]
        };
        data.point_lights[pt_count as usize] = GpuPointLight {
            position,
            range: pl.range,
            color: pl.color,
            intensity: pl.intensity,
        };
        pt_count += 1;
    }
    data.point_light_count = pt_count;

    data
}
