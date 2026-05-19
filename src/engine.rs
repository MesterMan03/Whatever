use crate::debug::{DebugConfig, DebugLogger};
use crate::input::InputState;
use crate::mods::{ModRegistry, discover_and_load};
use crate::renderer::{Renderer, WgpuContext, grid_pos, load_from_vfs};
use crate::script::ipc::EngineMessage;
use crate::script::{ScriptHost, dispatch};
use crate::vfs::{LayeredVfs, VfsHandle, VfsPath};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Instant;
use winit::application::ApplicationHandler;
use winit::event::{DeviceEvent, DeviceId, ElementState, WindowEvent};
use winit::event_loop::ActiveEventLoop;
use winit::keyboard::{KeyCode, PhysicalKey};
use winit::window::{CursorGrabMode, Window, WindowId};

pub struct Engine {
    debug: DebugLogger,
    vfs: VfsHandle,
    registry: ModRegistry,
    script_host: ScriptHost,
    input: InputState,
    frame_number: u64,
    last_frame: Instant,
    renderer: Option<Renderer>,
    window: Option<Arc<Window>>,
}

impl Engine {
    pub fn new(debug_config: &DebugConfig, cwd: &PathBuf) -> anyhow::Result<Self> {
        let mut debug = DebugLogger::new(debug_config, cwd)?;

        let mut vfs = LayeredVfs::new();
        let mut registry = ModRegistry::new();

        let mods_dir = cwd.join("mods");
        let mods_user_dir = cwd.join("mods_user");
        discover_and_load(
            &[mods_dir.as_path(), mods_user_dir.as_path()],
            &mut vfs,
            &mut registry,
            &mut debug,
        )?;

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
            vfs,
            registry,
            script_host,
            input: InputState::new(),
            frame_number: 0,
            last_frame: Instant::now(),
            renderer: None,
            window: None,
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
                        tracing::warn!("failed to load texture {}: {e}", vfs_path.to_string())
                    }
                }
            }
        }
    }

    fn frame(&mut self) {
        let now = Instant::now();
        let dt = now.duration_since(self.last_frame).as_secs_f32();
        self.last_frame = now;

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
        if let Some(window) = self.window.as_ref() {
            let window = Arc::clone(window);
            for (mod_id, msg) in messages {
                dispatch(&mod_id, msg, &window, &mut self.debug);
            }
        }

        self.frame_number += 1;

        if let Some(renderer) = self.renderer.as_mut() {
            if let Err(e) = renderer.render() {
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
}

impl ApplicationHandler for Engine {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.window.is_some() {
            return;
        }

        let attrs = Window::default_attributes().with_title("Whatever");
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
        match event {
            WindowEvent::CloseRequested => {
                self.debug.window("window close requested");
                let final_msgs = self.script_host.shutdown_all(0, &mut self.debug);
                if let Some(window) = self.window.as_ref() {
                    let window = Arc::clone(window);
                    for (mod_id, msg) in final_msgs {
                        dispatch(&mod_id, msg, &window, &mut self.debug);
                    }
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
                self.frame();
                if let Some(w) = self.window.as_ref() {
                    w.request_redraw();
                }
            }
            WindowEvent::MouseInput {
                state: ElementState::Pressed,
                ..
            } => {
                self.set_cursor_captured(!self.input.mouse_captured);
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
