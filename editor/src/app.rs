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
#[path = "asset_actions.rs"]
mod asset_actions;
#[path = "browser.rs"]
mod browser;
#[path = "component_fields.rs"]
mod component_fields;
#[path = "component_handles.rs"]
mod component_handles;
#[path = "component_reload.rs"]
mod component_reload;
#[path = "entity_picker.rs"]
mod entity_picker;
#[path = "hierarchy.rs"]
mod hierarchy;
#[path = "inspector.rs"]
mod inspector;
#[path = "libraries.rs"]
mod libraries;
#[path = "live.rs"]
mod live;
#[path = "menus.rs"]
mod menus;
#[path = "mod_editor.rs"]
mod mod_editor;
#[path = "player.rs"]
mod player;
#[path = "preferences.rs"]
mod preferences;
#[path = "presets.rs"]
mod presets;
#[path = "ui.rs"]
mod ui;
#[path = "viewport.rs"]
mod viewport;
#[path = "viewport_geometry.rs"]
mod viewport_geometry;
use menus::{Action, Menus};

pub fn run() -> anyhow::Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let path = args
        .first()
        .filter(|arg| !arg.starts_with('-'))
        .map(PathBuf::from);
    let project = path.as_ref().map(|path| Project::open(path));
    if args.iter().any(|arg| arg == "--check" || arg == "--export") {
        let project =
            project.unwrap_or_else(|| Project::open(&engine::runtime::installation_root()?))?;
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
        let shortcuts = Arc::clone(&menus.shortcuts);
        builder.with_msg_hook(move |message| {
            use windows::Win32::UI::WindowsAndMessaging::{HACCEL, MSG, TranslateAcceleratorW};
            // Muda's native accelerator table must participate in winit's Win32 message loop.
            unsafe {
                let message = message as *const MSG;
                shortcuts.load(std::sync::atomic::Ordering::Relaxed)
                    && TranslateAcceleratorW((*message).hwnd, HACCEL(bar.haccel() as _), message)
                        != 0
            }
        });
    }
    #[cfg(target_os = "macos")]
    {
        use winit::platform::macos::EventLoopBuilderExtMacOS;
        builder.with_default_menu(false);
    }
    let event_loop = builder.build()?;
    let mut editor = Editor::new(project, Some(menus));
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
    source: bool,
    mod_id: String,
    relative: String,
    directory: bool,
}
enum JobEvent {
    Line(String),
    Built(Result<PathBuf, String>),
    Player(Result<PathBuf, String>),
    ModValidated(Result<(), String>),
    AssetChanged(asset_actions::AssetAction, Result<PathBuf, String>),
    LibraryImported(
        Option<String>,
        Result<engine::project::LibraryImport, String>,
    ),
    LibraryExported(Result<PathBuf, String>),
}
struct Editor {
    show_prefabs: bool,
    prefab_search: String,
    prefabs: Vec<libraries::PrefabChoice>,
    pending_prefab: Option<(String, PathBuf)>,
    live: Option<live::LiveSession>,
    view: live::View,
    preferences: preferences::Preferences,
    show_preferences: bool,
    mod_draft: Option<mod_editor::ModDraft>,
    entity_clipboard: Option<String>,
    rename_requested: bool,
    hierarchy_focused: bool,
    component_handle: Option<component_handles::ComponentHandle>,
    component_reload: component_reload::ComponentReload,
    snap: viewport::Snap,
    project: Option<Project>,
    document: Option<Document>,
    vfs: LayeredVfs,
    world: World,
    resources: Vec<Resource>,
    mod_roots: Vec<(String, PathBuf)>,
    mod_dependencies: std::collections::BTreeMap<String, Vec<String>>,
    active_mod: usize,
    inspector_entity: Option<SceneEntity>,
    inspector_error: Option<String>,
    inspector_dirty: bool,
    renderer: Option<Renderer>,
    window: Option<Arc<Window>>,
    ctx: egui::Context,
    state: Option<egui_winit::State>,
    menus: Option<Menus>,
    status: String,
    logs: Vec<String>,
    job: Option<mpsc::Receiver<JobEvent>>,
    job_label: String,
    player: Option<Child>,
    player_host: Option<player::EmbeddedWindow>,
    player_detached: bool,
    build_destination: Option<PathBuf>,
    show_settings: bool,
    show_new_project: bool,
    show_new_mod: bool,
    draft_name: String,
    draft_id: String,
    draft_directory: Option<PathBuf>,
    asset_search: String,
    browser_tab: browser::BrowserTab,
    asset_action: Option<asset_actions::AssetAction>,
    browser_path: String,
    orbit: [f32; 2],
    distance: f32,
    target: Vec3,
    viewport: egui::Rect,
    preview_camera: Option<engine::ecs::EntityId>,
    tool: viewport::Tool,
    gizmo_drag: Option<viewport::Drag>,
    viewport_press: Option<egui::Pos2>,
    mesh_cache: std::collections::HashMap<String, engine::renderer::CpuMesh>,
    preview_dirty: bool,
    dragging_entity: Option<Vec<usize>>,
    hierarchy_press: Option<egui::Pos2>,
    should_quit: bool,
    fatal: Option<String>,
    next_redraw: std::time::Instant,
}

impl Editor {
    fn new(project: Option<anyhow::Result<Project>>, menus: Option<Menus>) -> Self {
        let mut editor = Self {
            show_prefabs: false,
            prefab_search: String::new(),
            prefabs: Vec::new(),
            pending_prefab: None,
            live: None,
            view: live::View::Scene,
            preferences: preferences::Preferences::load(),
            show_preferences: false,
            mod_draft: None,
            entity_clipboard: None,
            rename_requested: false,
            hierarchy_focused: false,
            component_handle: None,
            component_reload: component_reload::ComponentReload::default(),
            snap: viewport::Snap::default(),
            project: None,
            document: None,
            vfs: LayeredVfs::new(),
            world: World::new(),
            resources: Vec::new(),
            mod_roots: Vec::new(),
            mod_dependencies: Default::default(),
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
            player_host: None,
            player_detached: false,
            build_destination: None,
            show_settings: false,
            show_new_project: false,
            show_new_mod: false,
            draft_name: "My Game".into(),
            draft_id: "my_game".into(),
            draft_directory: None,
            asset_search: String::new(),
            browser_tab: browser::BrowserTab::Assets,
            asset_action: None,
            browser_path: String::new(),
            orbit: [0.5, 0.35],
            distance: 12.0,
            target: Vec3::ZERO,
            viewport: egui::Rect::NOTHING,
            preview_camera: None,
            tool: viewport::Tool::default(),
            gizmo_drag: None,
            viewport_press: None,
            mesh_cache: Default::default(),
            preview_dirty: true,
            dragging_entity: None,
            hierarchy_press: None,
            should_quit: false,
            fatal: None,
            next_redraw: std::time::Instant::now(),
        };
        match project {
            Some(Ok(project)) => {
                let result = editor.set_project(project);
                editor.report(result);
            }
            Some(Err(error)) => editor.status = format!("{error:#}"),
            None => editor.status = "Choose a project to get started".into(),
        }
        editor
    }
    fn report(&mut self, result: anyhow::Result<()>) {
        if let Err(error) = result {
            self.status = format!("{error:#}");
            self.logs.push(self.status.clone());
            self.browser_tab = browser::BrowserTab::Log;
        }
    }
    fn poll_jobs(&mut self) {
        let mut events = Vec::new();
        let mut disconnected = false;
        if let Some(receiver) = &self.job {
            loop {
                match receiver.try_recv() {
                    Ok(event) => events.push(event),
                    Err(mpsc::TryRecvError::Empty) => break,
                    Err(mpsc::TryRecvError::Disconnected) => {
                        disconnected = true;
                        break;
                    }
                }
            }
        }
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
                JobEvent::ModValidated(result) => {
                    self.job = None;
                    self.finish_mod_validation(result);
                }
                JobEvent::AssetChanged(action, result) => {
                    self.job = None;
                    self.finish_asset_action(action, result);
                }
                JobEvent::LibraryImported(prefab, result) => {
                    self.job = None;
                    let result = self.finish_library_import(prefab, result);
                    self.report(result);
                }
                JobEvent::LibraryExported(result) => {
                    self.job = None;
                    match result {
                        Ok(path) => {
                            self.status = format!("Library exported: {}", display_path(&path))
                        }
                        Err(error) => self.report(Err(anyhow::anyhow!(error))),
                    }
                }
            }
        }
        if disconnected && self.job.is_some() {
            self.job = None;
            let error = "Background operation stopped unexpectedly. Try the operation again.";
            if self.mod_draft.is_some() {
                self.finish_mod_validation(Err(error.into()));
            } else if let Some(action) = self.asset_action.take() {
                self.finish_asset_action(action, Err(error.into()));
            } else {
                self.report(Err(anyhow::anyhow!(error)));
            }
        }
    }
    fn redraw(&mut self) -> anyhow::Result<()> {
        self.poll_jobs();
        self.poll_component_types();
        if let Some(child) = &mut self.player
            && let Some(status) = child.try_wait()?
        {
            self.stop();
            self.status = format!("Play finished: {status}");
        }
        #[cfg(any(windows, target_os = "macos"))]
        if let Some(menus) = &self.menus {
            menus.shortcuts.store(
                !self.input_blocked() && !self.ctx.wants_keyboard_input(),
                std::sync::atomic::Ordering::Relaxed,
            );
        }
        self.poll_live();
        let actions = self
            .menus
            .as_ref()
            .map(|menus| menus.poll())
            .unwrap_or_default();
        let input = self
            .state
            .as_mut()
            .context("input state")?
            .take_egui_input(self.window.as_ref().context("window")?);
        let editing = !actions.is_empty()
            || !input.events.is_empty()
            || self.gizmo_drag.is_some()
            || self.inspector_dirty;
        let live_before = editing
            .then(|| {
                self.live
                    .as_ref()
                    .and_then(|_| self.document.as_ref().map(|d| d.scene.clone()))
            })
            .flatten();
        for action in actions {
            if self.has_dialog() {
                continue;
            }
            let result = self.action(action);
            self.report(result);
        }
        let ctx = self.ctx.clone();
        let output = ctx.run(input, |ctx| self.ui(ctx));
        if let Some(before) = live_before
            && (self.preview_dirty || self.document.as_ref().is_some_and(|d| d.dirty))
        {
            let result = self.live_changes(&before);
            self.report(result);
        }

        self.state
            .as_mut()
            .context("input state")?
            .handle_platform_output(
                self.window.as_ref().context("window")?,
                output.platform_output,
            );
        if self.preview_dirty && self.live.is_none() {
            let mut world = World::new();
            world.component_types = self.world.component_types.clone();
            world.allow_missing_components_for_editor();
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
        if let (Some(child), Some(window)) = (&self.player, &self.window) {
            if !self.player_detached && self.player_host.is_none() {
                match player::EmbeddedWindow::attach(child.id(), window) {
                    Ok(host) => {
                        if host.is_some() {
                            self.status = "Playing in editor viewport".into();
                        }
                        self.player_host = host;
                    }
                    Err(error) => {
                        self.status = format!(
                            "Embedded Play unavailable: {error}. Player remains in its own window."
                        );
                        self.player_detached = true;
                    }
                }
            }
            let host_visible = self.view == live::View::Play
                && !self.has_dialog()
                && !self.ctx.memory(|m| m.any_popup_open());
            if let Some(host) = &mut self.player_host {
                host.layout(self.viewport, output.pixels_per_point);
                host.visible(host_visible);
            }
        }
        let covered = self.player_host.is_some()
            && self.view == live::View::Play
            && !self.player_detached
            && !self.has_dialog()
            && !self.ctx.memory(|m| m.any_popup_open());
        if !covered {
            self.sync_live_renderer();
        }
        let game_camera = if self.view == live::View::Play {
            self.live.as_ref().and_then(|s| s.camera).and_then(|id| {
                if !self.world.is_active(&id) {
                    return None;
                }
                Some((
                    self.world.world_transform(&id)?,
                    self.world.camera_components.get(&id.index)?.clone(),
                ))
            })
        } else {
            self.scene_camera()
        };
        let position = game_camera
            .as_ref()
            .map(|(pose, _)| Vec3::from(pose.position))
            .unwrap_or_else(|| self.camera_position());
        let renderer = self.renderer.as_mut().context("renderer")?;
        // Avoid two synchronized swapchains blocking the parent and child input queues.
        renderer.ctx.set_vsync(self.player_host.is_none());
        if !covered {
            renderer.update_lighting(&engine::renderer::collect_lights(&self.world));
        }
        let ppp = output.pixels_per_point;
        let viewport = viewport::physical_viewport(
            self.viewport,
            ppp,
            [renderer.ctx.config.width, renderer.ctx.config.height],
        );
        let vp = viewport
            .filter(|_| !covered)
            .and_then(|[_, _, width, height]| {
                if let Some((pose, camera)) = &game_camera {
                    return Some({
                        engine::renderer::view_proj_from_entity(
                            pose,
                            camera,
                            width as f32 / height as f32,
                        )
                    });
                }
                if self.view == live::View::Play {
                    return None;
                }
                Some(
                    Mat4::perspective_rh(
                        45_f32.to_radians(),
                        width as f32 / height as f32,
                        0.01,
                        100000.0,
                    ) * Mat4::look_at_rh(position, self.target, Vec3::Y),
                )
            });
        let egui = EguiOutput {
            paint_jobs: ctx.tessellate(output.shapes, ppp),
            textures_delta: output.textures_delta,
            screen_descriptor: egui_wgpu::ScreenDescriptor {
                size_in_pixels: [renderer.ctx.config.width, renderer.ctx.config.height],
                pixels_per_point: ppp,
            },
        };
        renderer.render_viewport(vp, vp.map(|_| position), Some(&egui), &self.vfs, viewport)
    }
    fn camera_position(&self) -> Vec3 {
        if let Some((pose, _)) = self.scene_camera() {
            return Vec3::from(pose.position);
        }
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
            if let Some(menus) = &self.menus {
                menus.attach(&window)?;
            }
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
        #[cfg(windows)]
        if matches!(
            event,
            WindowEvent::MouseInput {
                state: winit::event::ElementState::Pressed,
                ..
            }
        ) && let Some(window) = &self.window
        {
            player::EmbeddedWindow::focus_editor(window);
        }
        if let (Some(state), Some(window)) = (&mut self.state, &self.window) {
            let _ = state.on_window_event(window, &event);
        }
        match event {
            WindowEvent::Focused(true)
                if self.project.is_some()
                    && self.live.is_none()
                    && self.player.is_none()
                    && self.job.is_none()
                    && !self.has_dialog() =>
            {
                let result = self.refresh_resources();
                self.report(result);
            }
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
                self.next_redraw =
                    std::time::Instant::now() + std::time::Duration::from_secs_f64(1.0 / 60.0);
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
    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
        event_loop.set_control_flow(winit::event_loop::ControlFlow::WaitUntil(self.next_redraw));
        if std::time::Instant::now() >= self.next_redraw
            && let Some(window) = &self.window
        {
            window.request_redraw();
        }
    }
}
impl Drop for Editor {
    fn drop(&mut self) {
        self.stop();
    }
}

impl Editor {
    pub(super) fn has_dialog(&self) -> bool {
        self.show_prefabs
            || self.show_settings
            || self.show_new_project
            || self.show_new_mod
            || self.show_preferences
            || self.mod_draft.is_some()
            || self.asset_action.is_some()
    }
    pub(super) fn input_blocked(&self) -> bool {
        self.has_dialog() || self.ctx.memory(|m| m.any_popup_open())
    }
}
