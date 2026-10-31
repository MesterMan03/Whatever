use super::*;

impl Editor {
    pub(super) fn file_dialog(&self, title: &str) -> rfd::FileDialog {
        let mut dialog = rfd::FileDialog::new().set_title(title);
        if let Some(window) = &self.window {
            dialog = dialog.set_parent(window.as_ref());
        }
        if let Some(project) = &self.project
            && let Some(parent) = project.path.parent()
        {
            dialog = dialog.set_directory(engine::project::portable_path(parent));
        }
        dialog
    }
    pub(super) fn unsaved(&self) -> bool {
        self.inspector_dirty || self.document.as_ref().is_some_and(|doc| doc.dirty)
    }
    pub(super) fn confirm_leave(&mut self) -> anyhow::Result<bool> {
        if !self.unsaved() {
            return Ok(true);
        }
        let mut dialog = rfd::MessageDialog::new()
            .set_title("Unsaved scene")
            .set_description("Save your scene changes before continuing?")
            .set_buttons(rfd::MessageButtons::YesNoCancel);
        if let Some(window) = &self.window {
            dialog = dialog.set_parent(window.as_ref());
        }
        match dialog.show() {
            rfd::MessageDialogResult::Yes => {
                self.save_scene()?;
                Ok(true)
            }
            rfd::MessageDialogResult::No => Ok(true),
            _ => Ok(false),
        }
    }
    pub(super) fn action(&mut self, action: Action) -> anyhow::Result<()> {
        if self.job.is_some() {
            self.status =
                "A build operation is running. You can continue after it finishes.".into();
            return Ok(());
        }
        match action {
            Action::OpenProject => {
                if let Some(path) = self
                    .file_dialog("Open Whatever project")
                    .add_filter("Whatever project", &["wproject"])
                    .pick_file()
                {
                    let project = Project::open(&path)?;
                    self.change_project(project)?;
                }
            }
            Action::OpenFolder => {
                if let Some(path) = self
                    .file_dialog("Open game or mod collection")
                    .pick_folder()
                {
                    let project = Project::open(&path)?;
                    self.change_project(project)?;
                }
            }
            Action::NewProject => {
                self.draft_name = "My Game".into();
                self.draft_id = "my_game".into();
                self.draft_directory = None;
                self.show_new_project = true;
            }
            Action::NewMod => {
                ensure!(self.project.is_some(), "open a project first");
                self.draft_name = "New Mod".into();
                self.draft_id = "new_mod".into();
                self.show_new_mod = true;
            }
            Action::AddMod => {
                if let Some(path) = self.file_dialog("Choose a mod directory").pick_folder() {
                    self.save_if_dirty()?;
                    let mut project = self.project.clone().context("open a project")?;
                    project.add_mod(&path)?;
                    self.set_project(project)?;
                }
            }
            Action::OpenScene => {
                if let Some(path) = self
                    .file_dialog("Open scene")
                    .add_filter("Whatever scene", &["wscene"])
                    .pick_file()
                {
                    self.request_scene(&path)?;
                }
            }
            Action::NewScene => {
                let directory = self.scene_directory()?;
                std::fs::create_dir_all(&directory)?;
                if let Some(path) = self
                    .file_dialog("Create scene")
                    .set_directory(engine::project::portable_path(&directory))
                    .set_file_name("New Scene.wscene")
                    .add_filter("Whatever scene", &["wscene"])
                    .save_file()
                {
                    ensure!(!path.exists(), "this scene already exists; open it instead");
                    self.scene_owner(&path)?;
                    if self.confirm_leave()? {
                        std::fs::write(
                            &path,
                            serde_json::to_vec_pretty(&SceneDocument::default())?,
                        )?;
                        self.open_scene(&path)?;
                        self.refresh_resources()?;
                    }
                }
            }
            Action::Save => self.save_scene()?,
            Action::SaveAs => {
                let directory = self.scene_directory()?;
                std::fs::create_dir_all(&directory)?;
                if let Some(path) = self
                    .file_dialog("Save scene as")
                    .set_directory(engine::project::portable_path(&directory))
                    .add_filter("Whatever scene", &["wscene"])
                    .save_file()
                {
                    self.scene_owner(&path)?;
                    self.commit_inspector()?;
                    let doc = self.document.as_mut().context("open a scene")?;
                    let old = doc.path.clone();
                    doc.path = path;
                    if let Err(error) = doc.save() {
                        doc.path = old;
                        return Err(error);
                    }
                    self.refresh_resources()?;
                }
            }
            Action::Undo | Action::Redo => {
                self.inspector_dirty = false;
                self.inspector_error = None;
                if let Some(doc) = &mut self.document {
                    if action == Action::Undo {
                        doc.undo();
                    } else {
                        doc.redo();
                    }
                }
                self.sync_selection();
                self.preview_dirty = true;
            }
            Action::Delete => {
                self.inspector_dirty = false;
                if let Some(doc) = &mut self.document {
                    doc.delete();
                }
                self.sync_selection();
                self.preview_dirty = true;
            }
            Action::AddEntity | Action::AddChild => {
                self.commit_inspector()?;
                if let Some(doc) = &mut self.document {
                    doc.add(action == Action::AddChild);
                }
                self.sync_selection();
                self.preview_dirty = true;
            }
            Action::Frame => self.frame_selected(),
            Action::Settings => {
                ensure!(self.project.is_some(), "open a project first");
                self.show_settings = true;
            }
            Action::Build => self.start_build()?,
            Action::Quit => {
                if self.confirm_leave()? {
                    self.should_quit = true;
                }
            }
        }
        Ok(())
    }
    pub(super) fn set_project(&mut self, project: Project) -> anyhow::Result<()> {
        let (_, vfs) = project.registry(engine::runtime::RuntimeMode::Client)?;
        self.mod_roots = project
            .manifests()?
            .into_iter()
            .map(|(root, manifest)| (manifest.meta.id, root))
            .collect();
        self.active_mod = usize::from(self.mod_roots.len() > 1);
        self.project = Some(project);
        self.vfs = vfs;
        self.document = None;
        self.inspector_dirty = false;
        self.inspector_entity = None;
        self.inspector_error = None;
        self.world = World::new();
        self.build_destination = None;
        self.preview_dirty = true;
        self.refresh_resources()?;
        self.status = "Project opened".into();
        if let Some(window) = &self.window {
            window.set_title(&format!(
                "{} — Whatever Editor",
                self.project
                    .as_ref()
                    .map(|p| p.name.as_str())
                    .unwrap_or("Untitled")
            ));
        }
        let first = self
            .resources
            .iter()
            .find(|resource| {
                resource.scene
                    && self
                        .mod_roots
                        .get(self.active_mod)
                        .is_some_and(|(id, _)| *id == resource.mod_id)
            })
            .map(|resource| resource.path.clone());
        if let Some(path) = first {
            self.open_scene(&path)?;
        }
        Ok(())
    }
    pub(super) fn change_project(&mut self, project: Project) -> anyhow::Result<()> {
        if self.confirm_leave()? {
            self.stop();
            self.set_project(project)?;
        }
        Ok(())
    }
    pub(super) fn refresh_resources(&mut self) -> anyhow::Result<()> {
        self.resources.clear();
        for (root, manifest) in self
            .project
            .as_ref()
            .context("open a project")?
            .manifests()?
        {
            let scenes = engine::sandbox::resolve_contained(
                &root,
                manifest
                    .mounts
                    .get("scenes")
                    .map(String::as_str)
                    .unwrap_or("scenes"),
            )?;
            for path in engine::vfs::DiskLayer::new(&manifest.meta.id, &scenes)
                .list(&manifest.meta.id, "")?
            {
                if path.ends_with(".wscene") {
                    self.resources.push(Resource {
                        uri: format!("{}://scenes/{path}", manifest.meta.id),
                        path: scenes.join(&path),
                        scene: true,
                        mod_id: manifest.meta.id.clone(),
                    });
                }
            }
            for path in self.vfs.list(&manifest.meta.id, "")? {
                if path.starts_with("scenes/") {
                    continue;
                }
                let asset_root = if manifest.assets.root == "." {
                    root.clone()
                } else {
                    engine::sandbox::resolve_contained(&root, &manifest.assets.root)?
                };
                self.resources.push(Resource {
                    uri: format!("{}://{path}", manifest.meta.id),
                    path: asset_root.join(&path),
                    scene: false,
                    mod_id: manifest.meta.id.clone(),
                });
            }
        }
        self.resources.sort_by(|a, b| a.uri.cmp(&b.uri));
        Ok(())
    }
    pub(super) fn scene_directory(&self) -> anyhow::Result<PathBuf> {
        let (_, root) = self
            .mod_roots
            .get(self.active_mod)
            .context("choose a mod in the Project browser")?;
        let manifest: engine::mods::ModManifest =
            toml::from_str(&std::fs::read_to_string(root.join("mod.toml"))?)?;
        engine::sandbox::resolve_contained(
            root,
            manifest
                .mounts
                .get("scenes")
                .map(String::as_str)
                .unwrap_or("scenes"),
        )
    }
    pub(super) fn scene_owner(&self, path: &Path) -> anyhow::Result<usize> {
        let path = if path.exists() {
            path.canonicalize()?
        } else {
            path.parent()
                .context("scene directory")?
                .canonicalize()?
                .join(path.file_name().context("scene filename")?)
        };
        for (index, (_, root)) in self.mod_roots.iter().enumerate() {
            let manifest: engine::mods::ModManifest =
                toml::from_str(&std::fs::read_to_string(root.join("mod.toml"))?)?;
            let scenes = engine::sandbox::resolve_contained(
                root,
                manifest
                    .mounts
                    .get("scenes")
                    .map(String::as_str)
                    .unwrap_or("scenes"),
            )?;
            let scenes = if scenes.exists() {
                scenes.canonicalize()?
            } else {
                scenes
            };
            if path.starts_with(scenes) {
                return Ok(index);
            }
        }
        anyhow::bail!("choose a scene inside a mod's scenes folder in this project")
    }
    pub(super) fn request_scene(&mut self, path: &Path) -> anyhow::Result<()> {
        self.scene_owner(path)?;
        if self.confirm_leave()? {
            self.open_scene(path)?;
        }
        Ok(())
    }
    pub(super) fn open_scene(&mut self, path: &Path) -> anyhow::Result<()> {
        let owner = self.scene_owner(path)?;
        let doc = Document::open(path)?;
        self.document = Some(doc);
        self.active_mod = owner;
        self.inspector_dirty = false;
        self.inspector_error = None;
        self.inspector_entity = None;
        self.preview_dirty = true;
        self.status = format!(
            "Opened {}",
            path.file_name().unwrap_or_default().to_string_lossy()
        );
        Ok(())
    }
    pub(super) fn sync_selection(&mut self) {
        self.inspector_entity = self
            .document
            .as_ref()
            .and_then(|doc| entity_at(&doc.scene.entities, &doc.selected))
            .cloned();
        self.inspector_dirty = false;
        self.inspector_error = None;
    }
    pub(super) fn select(&mut self, path: Vec<usize>) -> anyhow::Result<()> {
        self.commit_inspector()?;
        if let Some(doc) = &mut self.document {
            doc.selected = path;
        }
        self.sync_selection();
        Ok(())
    }
    pub(super) fn commit_inspector(&mut self) -> anyhow::Result<()> {
        if !self.inspector_dirty {
            return Ok(());
        }
        let entity = self.inspector_entity.clone().context("select an entity")?;
        let doc = self.document.as_mut().context("open a scene")?;
        let mut candidate = doc.scene.clone();
        let old_id =
            entity_at(&candidate.entities, &doc.selected).and_then(|item| item.script_id.clone());
        let new_id = entity.script_id.clone();
        *entity_mut(&mut candidate.entities, &doc.selected).context("selected entity")? = entity;
        if let (Some(old), Some(new)) = (old_id, new_id)
            && old != new
        {
            fn rewrite(value: &mut serde_json::Value, old: &str, new: &str) {
                match value {
                    serde_json::Value::Object(fields)
                        if fields.len() == 1
                            && fields.get("$entity").and_then(|v| v.as_str()) == Some(old) =>
                    {
                        fields.insert("$entity".into(), serde_json::Value::String(new.into()));
                    }
                    serde_json::Value::Object(fields) => {
                        for value in fields.values_mut() {
                            rewrite(value, old, new);
                        }
                    }
                    serde_json::Value::Array(items) => {
                        for value in items {
                            rewrite(value, old, new);
                        }
                    }
                    _ => {}
                }
            }
            fn walk(items: &mut [SceneEntity], old: &str, new: &str) {
                for entity in items {
                    for value in entity.components.values_mut() {
                        rewrite(value, old, new);
                    }
                    walk(&mut entity.children, old, new);
                }
            }
            walk(&mut candidate.entities, &old, &new);
            self.inspector_entity = entity_at(&candidate.entities, &doc.selected).cloned();
        }
        if let Err(error) = candidate.validate() {
            self.inspector_error = Some(format!("{error:#}"));
            return Err(error);
        }
        doc.checkpoint();
        doc.scene = candidate;
        self.inspector_dirty = false;
        self.inspector_error = None;
        self.preview_dirty = true;
        Ok(())
    }
    pub(super) fn save_scene(&mut self) -> anyhow::Result<()> {
        self.commit_inspector()?;
        self.document.as_mut().context("open a scene")?.save()?;
        self.status = "Scene saved".into();
        Ok(())
    }
    pub(super) fn save_if_dirty(&mut self) -> anyhow::Result<()> {
        if self.unsaved() {
            self.save_scene()?;
        }
        Ok(())
    }
    pub(super) fn frame_selected(&mut self) {
        if let Some(doc) = &self.document {
            let mut paths = Vec::new();
            hierarchy::entity_paths(&doc.scene.entities, &mut Vec::new(), &mut paths);
            if let Some(index) = paths.iter().position(|path| *path == doc.selected)
                && let Some(id) = self.world.allocator.alive_entity_ids().nth(index)
                && let Some(transform) = self.world.world_transform(&id)
            {
                self.target = Vec3::from(transform.position);
                self.distance = transform
                    .scale
                    .into_iter()
                    .fold(3.0_f32, |m, s| m.max(s.abs() * 3.0));
            }
        }
    }
    pub(super) fn start_build(&mut self) -> anyhow::Result<()> {
        self.save_if_dirty()?;
        let project = self.project.clone().context("open a project")?;
        project.validate()?;
        project.save()?;
        if self.build_destination.is_none() {
            self.build_destination = self.file_dialog("Choose game build folder").pick_folder();
        }
        let Some(destination) = self.build_destination.clone() else {
            return Ok(());
        };
        let (sender, receiver) = mpsc::channel();
        self.job = Some(receiver);
        self.job_label = "Preparing game build…".into();
        self.logs.clear();
        self.browser_tab = 2;
        std::thread::spawn(move || {
            let result = engine::project::build_game(&project, &destination, |line| {
                let _ = sender.send(JobEvent::Line(line));
            })
            .map_err(|error| format!("{error:#}"));
            let _ = sender.send(JobEvent::Built(result));
        });
        Ok(())
    }
    pub(super) fn play(&mut self) -> anyhow::Result<()> {
        if self.job.is_some() || self.player.is_some() {
            return Ok(());
        }
        self.save_if_dirty()?;
        let project = self.project.as_ref().context("open a project")?;
        project.validate()?;
        project.save()?;
        let (sender, receiver) = mpsc::channel();
        self.job = Some(receiver);
        self.job_label = "Preparing Play…".into();
        std::thread::spawn(move || {
            let result = engine::project::prepare_player(|line| {
                let _ = sender.send(JobEvent::Line(line));
            })
            .map_err(|e| format!("{e:#}"));
            let _ = sender.send(JobEvent::Player(result));
        });
        Ok(())
    }
    pub(super) fn launch_player(&mut self, path: &Path) -> anyhow::Result<()> {
        let project = self.project.as_ref().context("open a project")?;
        let mut command = Command::new(path);
        command.arg("--project").arg(&project.path);
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            command.creation_flags(0x08000000);
        }
        self.player = Some(command.spawn().context("start player")?);
        self.status = "Playing — close the game window or press Stop".into();
        Ok(())
    }
    pub(super) fn stop(&mut self) {
        if let Some(mut child) = self.player.take() {
            let _ = child.kill();
            let _ = child.wait();
        }
    }
    pub(super) fn import_asset(&mut self, path: &Path) -> anyhow::Result<String> {
        let canonical = path.canonicalize()?;
        if let Some(resource) = self.resources.iter().find(|resource| {
            !resource.scene
                && resource
                    .path
                    .canonicalize()
                    .is_ok_and(|candidate| candidate == canonical)
        }) {
            return Ok(resource.uri.clone());
        }
        let (id, root) = self
            .mod_roots
            .get(self.active_mod)
            .context("choose a destination mod")?;
        let manifest: engine::mods::ModManifest =
            toml::from_str(&std::fs::read_to_string(root.join("mod.toml"))?)?;
        let assets = engine::sandbox::resolve_contained(root, &manifest.assets.root)?;
        std::fs::create_dir_all(&assets)?;
        let filename = path.file_name().context("asset filename")?;
        let mut destination = assets.join(filename);
        let mut index = 2;
        while destination.exists() {
            let stem = path.file_stem().unwrap_or_default().to_string_lossy();
            let ext = path.extension().unwrap_or_default().to_string_lossy();
            destination = assets.join(format!("{stem}-{index}.{ext}"));
            index += 1;
        }
        std::fs::copy(path, &destination)?;
        let uri = format!(
            "{id}://{}",
            destination
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
        );
        self.refresh_resources()?;
        Ok(uri)
    }
}
