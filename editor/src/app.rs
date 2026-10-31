use crate::document::{Document, entity_at, entity_mut};
use anyhow::{Context, ensure};
use glam::{Mat4, Vec3};
use std::{
    path::{Path, PathBuf},
    process::{Child, Command},
    sync::{Arc, mpsc},
};
use whatever_engine::{
    self as engine,
    ecs::World,
    project::{Project, display_path},
    renderer::{EguiOutput, Renderer, WgpuContext},
    scene::{SceneDocument, SceneEntity},
    vfs::{LayeredVfs, Vfs},
};
use winit::{
    application::ApplicationHandler,
    event::WindowEvent,
    event_loop::{ActiveEventLoop, EventLoop},
    window::{Window, WindowId},
};

#[path = "actions.rs"]
mod actions;
#[path = "hierarchy.rs"]
mod hierarchy;
#[path = "inspector.rs"]
mod inspector;
#[path = "menus.rs"]
mod menus;
#[path = "ui.rs"]
mod ui;
use menus::{Action, Menus};

pub fn run() -> anyhow::Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let path = args
        .first()
        .filter(|arg| !arg.starts_with('-'))
        .map(PathBuf::from)
        .unwrap_or(engine::runtime::installation_root()?);
    let project = Project::open(&path);
    if args.iter().any(|arg| arg == "--check" || arg == "--export") {
        let project = project?;
        project.validate()?;
        if let Some(index) = args.iter().position(|arg| arg == "--export") {
            let destination = Path::new(
                args.get(index + 1)
                    .context("--export needs an output directory")?,
            );
            engine::project::build_game(&project, destination, |line| tracing::info!("{line}"))?;
        } else {
            tracing::info!("project and scenes are valid");
        }
        return Ok(());
    }
    let menus = Menus::new()?;
    let mut builder = EventLoop::builder();
    #[cfg(windows)]
    {
        use winit::platform::windows::EventLoopBuilderExtWindows;
        let bar = menus.bar.clone();
        builder.with_msg_hook(move |message| {
            use windows::Win32::UI::WindowsAndMessaging::{HACCEL, MSG, TranslateAcceleratorW};
            // Muda's native accelerator table must participate in winit's Win32 message loop.
            unsafe {
                let message = message as *const MSG;
                TranslateAcceleratorW((*message).hwnd, HACCEL(bar.haccel() as _), message) != 0
            }
        });
    }
    #[cfg(target_os = "macos")]
    {
        use winit::platform::macos::EventLoopBuilderExtMacOS;
        builder.with_default_menu(false);
    }
    let event_loop = builder.build()?;
    let mut editor = Editor::new(project, menus);
    event_loop.run_app(&mut editor)?;
    if let Some(error) = &editor.fatal {
        anyhow::bail!("{error}");
    }
    Ok(())
}

struct Resource {
    uri: String,
    path: PathBuf,
    scene: bool,
    mod_id: String,
}
enum JobEvent {
    Line(String),
    Built(Result<PathBuf, String>),
    Player(Result<PathBuf, String>),
}
struct Editor {
    project: Option<Project>,
    document: Option<Document>,
    vfs: LayeredVfs,
    world: World,
    resources: Vec<Resource>,
    mod_roots: Vec<(String, PathBuf)>,
    active_mod: usize,
    inspector_entity: Option<SceneEntity>,
    inspector_error: Option<String>,
    inspector_dirty: bool,
    renderer: Option<Renderer>,
    window: Option<Arc<Window>>,
    ctx: egui::Context,
    state: Option<egui_winit::State>,
    menus: Menus,
    status: String,
    logs: Vec<String>,
    job: Option<mpsc::Receiver<JobEvent>>,
    job_label: String,
    player: Option<Child>,
    build_destination: Option<PathBuf>,
    show_settings: bool,
    show_new_project: bool,
    show_new_mod: bool,
    draft_name: String,
    draft_id: String,
    draft_directory: Option<PathBuf>,
    asset_search: String,
    browser_tab: usize,
    orbit: [f32; 2],
    distance: f32,
    target: Vec3,
    viewport: egui::Rect,
    preview_dirty: bool,
    dragging_entity: Option<Vec<usize>>,
    hierarchy_press: Option<egui::Pos2>,
    should_quit: bool,
    fatal: Option<String>,
}

impl Editor {
    fn new(project: anyhow::Result<Project>, menus: Menus) -> Self {
        let mut editor = Self {
            project: None,
            document: None,
            vfs: LayeredVfs::new(),
            world: World::new(),
            resources: Vec::new(),
            mod_roots: Vec::new(),
            active_mod: 0,
            inspector_entity: None,
            inspector_error: None,
            inspector_dirty: false,
            renderer: None,
            window: None,
            ctx: egui::Context::default(),
            state: None,
            menus,
            status: "Open a project to begin".into(),
            logs: Vec::new(),
            job: None,
            job_label: String::new(),
            player: None,
            build_destination: None,
            show_settings: false,
            show_new_project: false,
            show_new_mod: false,
            draft_name: "My Game".into(),
            draft_id: "my_game".into(),
            draft_directory: None,
            asset_search: String::new(),
            browser_tab: 0,
            orbit: [0.5, 0.35],
            distance: 12.0,
            target: Vec3::ZERO,
            viewport: egui::Rect::NOTHING,
            preview_dirty: true,
            dragging_entity: None,
            hierarchy_press: None,
            should_quit: false,
            fatal: None,
        };
        match project {
            Ok(project) => {
                let result = editor.set_project(project);
                editor.report(result);
            }
            Err(error) => editor.status = format!("{error:#}"),
        }
        editor
    }
    fn report(&mut self, result: anyhow::Result<()>) {
        if let Err(error) = result {
            self.status = format!("{error:#}");
            self.logs.push(self.status.clone());
            self.browser_tab = 2;
        }
    }
    fn poll_jobs(&mut self) {
        let events = self
            .job
            .as_ref()
            .map(|receiver| receiver.try_iter().collect::<Vec<_>>())
            .unwrap_or_default();
        for event in events {
            match event {
                JobEvent::Line(line) => {
                    self.job_label = line.clone();
                    self.logs.push(line);
                    if self.logs.len() > 2000 {
                        self.logs.remove(0);
                    }
                }
                JobEvent::Built(result) => {
                    self.job = None;
                    match result {
                        Ok(path) => {
                            self.status = format!("Build complete — {}", display_path(&path))
                        }
                        Err(error) => {
                            self.status = error.clone();
                            self.logs.push(error);
                        }
                    }
                }
                JobEvent::Player(result) => {
                    self.job = None;
                    let result = result
                        .map_err(anyhow::Error::msg)
                        .and_then(|path| self.launch_player(&path));
                    self.report(result);
                }
            }
        }
    }
    fn redraw(&mut self) -> anyhow::Result<()> {
        self.poll_jobs();
        if let Some(child) = &mut self.player
            && let Some(status) = child.try_wait()?
        {
            self.player = None;
            self.status = format!("Play finished: {status}");
        }
        for action in self.menus.poll() {
            let result = self.action(action);
            self.report(result);
        }
        let input = self
            .state
            .as_mut()
            .context("input state")?
            .take_egui_input(self.window.as_ref().context("window")?);
        let ctx = self.ctx.clone();
        let output = ctx.run(input, |ctx| self.ui(ctx));
        self.state
            .as_mut()
            .context("input state")?
            .handle_platform_output(
                self.window.as_ref().context("window")?,
                output.platform_output,
            );
        if self.preview_dirty {
            let mut world = World::new();
            if let Some(doc) = &self.document
                && let Err(error) = doc.scene.instantiate(&mut world)
            {
                self.status = format!("Scene: {error:#}");
            }
            if let Some(renderer) = &mut self.renderer {
                renderer.scene = engine::renderer::Scene::new();
                renderer.text = engine::renderer::GlyphonText::new();
                for command in engine::script::RenderCommand::upsert_hierarchy(
                    &world,
                    world.allocator.alive_entity_ids(),
                ) {
                    engine::renderer::apply_render_command(&command, renderer, &world, &self.vfs);
                }
            }
            self.world = world;
            self.preview_dirty = false;
        }
        let position = self.camera_position();
        let aspect = self.viewport.width() / self.viewport.height().max(1.0);
        let vp = Mat4::perspective_rh(45_f32.to_radians(), aspect, 0.01, 100000.0)
            * Mat4::look_at_rh(position, self.target, Vec3::Y);
        let renderer = self.renderer.as_mut().context("renderer")?;
        renderer.update_lighting(&engine::renderer::collect_lights(&self.world));
        let ppp = output.pixels_per_point;
        let viewport = [
            (self.viewport.min.x * ppp).max(0.0) as u32,
            (self.viewport.min.y * ppp).max(0.0) as u32,
            (self.viewport.width() * ppp).max(1.0) as u32,
            (self.viewport.height() * ppp).max(1.0) as u32,
        ];
        let egui = EguiOutput {
            paint_jobs: ctx.tessellate(output.shapes, ppp),
            textures_delta: output.textures_delta,
            screen_descriptor: egui_wgpu::ScreenDescriptor {
                size_in_pixels: [renderer.ctx.config.width, renderer.ctx.config.height],
                pixels_per_point: ppp,
            },
        };
        renderer.render_viewport(
            Some(vp),
            Some(position),
            Some(&egui),
            &self.vfs,
            Some(viewport),
        )
    }
    fn camera_position(&self) -> Vec3 {
        self.target
            + Vec3::new(
                self.orbit[0].sin() * self.orbit[1].cos(),
                self.orbit[1].sin(),
                self.orbit[0].cos() * self.orbit[1].cos(),
            ) * self.distance
    }
}

impl ApplicationHandler for Editor {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.window.is_some() {
            return;
        }
        let result = (|| -> anyhow::Result<()> {
            let icon =
                winit::window::Icon::from_rgba(engine::project::default_icon_rgba(64), 64, 64)?;
            let window = Arc::new(
                event_loop.create_window(
                    Window::default_attributes()
                        .with_title("Whatever Editor")
                        .with_maximized(true)
                        .with_window_icon(Some(icon))
                        .with_inner_size(winit::dpi::LogicalSize::new(1440.0, 850.0))
                        .with_min_inner_size(winit::dpi::LogicalSize::new(1080.0, 700.0)),
                )?,
            );
            self.menus.attach(&window)?;
            let runtime = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()?;
            self.renderer = Some(runtime.block_on(async {
                Renderer::new(WgpuContext::new(Arc::clone(&window)).await?).await
            })?);
            self.state = Some(egui_winit::State::new(
                self.ctx.clone(),
                egui::ViewportId::ROOT,
                event_loop,
                None,
                None,
                None,
            ));
            self.theme();
            if let Some(project) = &self.project {
                window.set_title(&format!("{} — Whatever Editor", project.name));
            }
            self.window = Some(window);
            Ok(())
        })();
        if let Err(error) = result {
            self.fatal = Some(format!("{error:#}"));
            event_loop.exit();
        }
    }
    fn window_event(&mut self, event_loop: &ActiveEventLoop, _: WindowId, event: WindowEvent) {
        if let (Some(state), Some(window)) = (&mut self.state, &self.window) {
            let _ = state.on_window_event(window, &event);
        }
        match event {
            WindowEvent::CloseRequested => {
                let result = self.action(Action::Quit);
                self.report(result);
            }
            WindowEvent::Resized(size) if size.width > 0 && size.height > 0 => {
                if let Some(renderer) = &mut self.renderer {
                    renderer.resize(size.width, size.height);
                }
            }
            WindowEvent::RedrawRequested => {
                if let Err(error) = self.redraw() {
                    self.status = format!("{error:#}");
                }
            }
            WindowEvent::DroppedFile(path) => {
                let result = if path.extension().is_some_and(|ext| ext == "wscene") {
                    self.request_scene(&path)
                } else {
                    Project::open(&path).and_then(|project| self.change_project(project))
                };
                self.report(result);
            }
            _ => {}
        }
        if self.should_quit {
            event_loop.exit();
        }
    }
    fn about_to_wait(&mut self, _: &ActiveEventLoop) {
        if let Some(window) = &self.window {
            window.request_redraw();
        }
    }
}
impl Drop for Editor {
    fn drop(&mut self) {
        self.stop();
    }
}
