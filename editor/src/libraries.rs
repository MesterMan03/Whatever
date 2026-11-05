use super::*;

#[derive(Clone)]
pub(super) struct PrefabChoice {
    name: String,
    category: String,
    description: String,
    path: String,
    library: Option<PathBuf>,
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn placing_prefabs_remaps_named_references_and_is_one_undo_step_per_copy() {
        let directory = std::env::temp_dir().join(format!(
            "whatever-editor-prefab-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("clock")
                .as_nanos()
        ));
        struct Cleanup(PathBuf);
        impl Drop for Cleanup {
            fn drop(&mut self) {
                let _ = std::fs::remove_dir_all(&self.0);
            }
        }
        let _cleanup = Cleanup(directory.clone());
        let project = engine::project::create_project(
            &directory,
            "prefab_qa",
            "Prefab QA",
            &engine::development_root().join("engine/resources/core"),
        )
        .expect("project");
        let root = project.roots().expect("roots")[0].clone();
        std::fs::create_dir(root.join("prefabs")).expect("prefabs");
        std::fs::write(root.join("prefabs/world.wprefab"),serde_json::to_vec(&serde_json::json!({"version":1,"name":"Catalog Label","entity":{"name":"Authored root","script_id":"world","components":{"core:physics_world":{}},"children":[{"script_id":"body","components":{"core:rigid_body":{"world":{"$entity":"world"}}}}]}})).expect("json")).expect("prefab");
        let mut editor = Editor::new(None, None);
        editor.vfs = project
            .registry(engine::runtime::RuntimeMode::Client)
            .expect("VFS")
            .1;
        editor.document = Some(Document::open(&root.join("scenes/main.wscene")).expect("scene"));
        editor.mod_roots = vec![("core".into(), root)];
        editor.project = Some(project);
        editor.place_prefab("core://world").expect("first copy");
        editor.place_prefab("core://world").expect("second copy");
        let doc = editor.document.as_mut().expect("document");
        assert_eq!(doc.scene.entities.len(), 4);
        for entity in &doc.scene.entities[2..] {
            assert_eq!(entity.name, "Authored root");
            assert_eq!(
                entity.children[0].components["core:rigid_body"]["world"]["$entity"],
                entity.script_id.as_deref().expect("name")
            );
        }
        assert_ne!(
            doc.scene.entities[2].script_id,
            doc.scene.entities[3].script_id
        );
        doc.undo();
        assert_eq!(doc.scene.entities.len(), 3);
        doc.undo();
        assert_eq!(doc.scene.entities.len(), 2);
        doc.redo();
        assert_eq!(doc.scene.entities.len(), 3);
    }
}

impl Editor {
    pub(super) fn refresh_prefabs(&mut self) -> anyhow::Result<()> {
        let mut choices = Vec::new();
        for resource in &self.resources {
            if resource.path.extension().is_some_and(|e| e == "wprefab") && !resource.uri.is_empty()
            {
                match engine::scene::PrefabDocument::parse(&std::fs::read(&resource.path)?) {
                    Ok(prefab) => choices.push(PrefabChoice {
                        name: prefab.name,
                        category: prefab.category,
                        description: prefab.description,
                        path: resource.uri.clone(),
                        library: None,
                    }),
                    Err(error) => self
                        .logs
                        .push(format!("Invalid prefab {}: {error:#}", resource.uri)),
                }
            }
        }
        if let Ok(root) = engine::project::starter_library() {
            for path in engine::vfs::DiskLayer::new("starter", &root).list("starter", "prefabs")? {
                if path.ends_with(".wprefab") {
                    let prefab =
                        engine::scene::PrefabDocument::parse(&std::fs::read(root.join(&path))?)?;
                    choices.push(PrefabChoice {
                        name: prefab.name,
                        category: format!("Official / {}", prefab.category),
                        description: prefab.description,
                        path,
                        library: Some(root.clone()),
                    });
                }
            }
        }
        choices.sort_by(|a, b| (&a.category, &a.name).cmp(&(&b.category, &b.name)));
        self.prefabs = choices;
        Ok(())
    }

    pub(super) fn prefabs_ui(&mut self, ctx: &egui::Context) {
        if !self.component_reload.is_busy()
            && self.component_reload.error.is_none()
            && self.job.is_none()
            && let Some((path, scene)) = self.pending_prefab.take()
        {
            if self.document.as_ref().is_some_and(|doc| doc.path == scene) {
                let result = self.place_prefab(&path);
                self.report(result);
            } else {
                self.status = "Prefab placement cancelled because the open scene changed".into();
            }
        }
        if !self.show_prefabs {
            return;
        }
        let mut close = false;
        let mut selected = None;
        egui::Modal::new(egui::Id::new("prefab_catalog")).show(ctx, |ui| {
            ui.set_width(560.0);
            ui.horizontal(|ui| { ui.heading("Prefabs"); if ui.button("Close").clicked() { close = true; } });
            ui.label("Place an independent, editable object. Official prefabs import their library on first use.");
            ui.add(egui::TextEdit::singleline(&mut self.prefab_search).hint_text("Search prefabs…").desired_width(f32::INFINITY));
            let search = self.prefab_search.to_lowercase();
            egui::ScrollArea::vertical().max_height(460.0).auto_shrink([false,false]).show(ui, |ui| {
                for choice in &self.prefabs {
                    if !format!("{} {}",choice.category,choice.name).to_lowercase().contains(&search) { continue; }
                    let allowed = choice.library.is_some() || engine::vfs::VfsPath::parse(&choice.path).is_some_and(|p| self.reference_allowed(&p.mod_id));
                    ui.horizontal(|ui| {
                        if ui.add_enabled(allowed && self.job.is_none() && self.live.is_none(), egui::Button::new(&choice.name)).on_hover_text(&choice.description).clicked() { selected = Some(choice.clone()); }
                        ui.weak(&choice.category);
                        if !allowed { ui.weak("Declare mod dependency first"); }
                    });
                }
            });
            if ui.button("Import .wasset library…").clicked() {
                let result = self.choose_library_import(); self.report(result); close = true;
            }
        });
        if let Some(choice) = selected {
            close = true;
            let result = if let Some(library) = choice.library {
                self.start_library_import(library, Some(choice.path), true)
            } else {
                self.place_prefab(&choice.path)
            };
            self.report(result);
        }
        if close {
            self.show_prefabs = false;
        }
    }

    pub(super) fn place_prefab(&mut self, path: &str) -> anyhow::Result<()> {
        ensure!(self.live.is_none(), "Stop Play before placing prefabs");
        self.component_reload.ensure_ready()?;
        self.commit_inspector()?;
        let target = engine::scene::prefab_path(path)?;
        ensure!(
            self.reference_allowed(&target.mod_id),
            "declare this prefab's mod dependency first"
        );
        let prefab = engine::scene::PrefabDocument::parse(&self.vfs.read(&target)?)?;
        let (registry, _) = self
            .project
            .as_ref()
            .context("project")?
            .registry(engine::runtime::RuntimeMode::Client)?;
        let owner = registry.get(&target.mod_id).context("prefab owner")?;
        engine::scene::validate_references(
            &prefab.scene(),
            owner,
            &self.vfs,
            &self.world.component_types,
        )?;
        let mut validation = World::new();
        validation.component_types = self.world.component_types.clone();
        prefab.scene().instantiate(&mut validation)?;
        let doc = self.document.as_ref().context("open a scene")?;
        self.project.as_ref().context("project")?.validate_scene(
            &doc.path,
            &prefab.scene(),
            &self.vfs,
            &self.world.component_types,
        )?;
        let doc = self.document.as_mut().context("scene")?;
        let entity = prefab.scene().entities.remove(0);
        let instance_name = entity.name.clone();
        doc.paste(&serde_json::to_string(
            &serde_json::json!({"whatever_entity": entity}),
        )?)?;
        if let Some(entity) = entity_mut(&mut doc.scene.entities, &doc.selected) {
            entity.name = instance_name;
        }
        self.sync_selection();
        self.preview_dirty = true;
        self.status = format!("Instantiated {}", prefab.name);
        Ok(())
    }

    pub(super) fn save_prefab(&mut self) -> anyhow::Result<()> {
        ensure!(self.live.is_none(), "Stop Play before saving a prefab");
        self.component_reload.ensure_ready()?;
        self.commit_inspector()?;
        let doc = self.document.as_ref().context("open a scene")?;
        let entity = entity_at(&doc.scene.entities, &doc.selected)
            .context("select an entity")?
            .clone();
        let owner = self.scene_owner(&doc.path)?;
        let root = &self.mod_roots[owner].1;
        let manifest: engine::mods::ModManifest =
            toml::from_str(&std::fs::read_to_string(root.join("mod.toml"))?)?;
        let directory = engine::sandbox::resolve_contained(
            root,
            manifest
                .mounts
                .get("prefabs")
                .map(String::as_str)
                .unwrap_or("prefabs"),
        )?;
        std::fs::create_dir_all(&directory)?;
        let prefab = engine::scene::PrefabDocument {
            version: 1,
            name: if entity.name.is_empty() {
                "Prefab".into()
            } else {
                entity.name.clone()
            },
            description: String::new(),
            category: "My prefabs".into(),
            entity,
        };
        let bytes = serde_json::to_vec_pretty(&prefab)?;
        engine::scene::PrefabDocument::parse(&bytes).context(
            "prefab must be self-contained; include referenced entities in its children",
        )?;
        if let Some(path) = self
            .file_dialog("Save selected hierarchy as prefab")
            .set_directory(&directory)
            .set_file_name("NewPrefab.wprefab")
            .add_filter("Whatever prefab", &["wprefab"])
            .save_file()
        {
            ensure!(
                engine::project::portable_path(&std::path::absolute(&path)?)
                    .starts_with(engine::project::portable_path(root)),
                "save the prefab inside its owning mod"
            );
            ensure!(
                !path.exists(),
                "choose a new prefab filename to preserve the existing file"
            );
            std::fs::write(&path, bytes)?;
            self.refresh_file_inventory()?;
            self.status = format!("Saved prefab {}", display_path(&path));
        }
        Ok(())
    }

    pub(super) fn choose_library_import(&mut self) -> anyhow::Result<()> {
        if let Some(path) = self
            .file_dialog("Import asset library")
            .add_filter("Whatever asset library", &["wasset"])
            .pick_file()
        {
            self.start_library_import(path, None, false)?;
        }
        Ok(())
    }

    pub(super) fn start_library_import(
        &mut self,
        path: PathBuf,
        prefab: Option<String>,
        folder: bool,
    ) -> anyhow::Result<()> {
        ensure!(
            self.live.is_none() && self.job.is_none(),
            "Stop Play and wait for current operations before importing"
        );
        let project = self.project.clone().context("open a project")?;
        let owner = if prefab.is_some()
            && let Some(doc) = &self.document
        {
            self.scene_owner(&doc.path)?
        } else {
            self.active_mod
        };
        let owner = self
            .mod_roots
            .get(owner)
            .context("receiving mod")?
            .0
            .clone();
        let (sender, receiver) = mpsc::channel();
        self.job = Some(receiver);
        self.job_label = "Reading and validating asset library…".into();
        std::thread::spawn(move || {
            let result = (|| -> anyhow::Result<engine::project::LibraryImport> {
                let library = if folder {
                    engine::project::AssetLibrary::from_directory(&path)?
                } else {
                    engine::project::AssetLibrary::open(&path)?
                };
                let _ = sender.send(JobEvent::Line(format!(
                    "Importing {} by {} ({}) into {} — {} files…",
                    library.manifest.name,
                    library.manifest.authors.join(", "),
                    library.manifest.license,
                    owner,
                    library.files.len()
                )));
                project.import_library(&owner, &library)
            })()
            .map_err(|e| format!("{e:#}"));
            let _ = sender.send(JobEvent::LibraryImported(prefab, result));
        });
        Ok(())
    }

    pub(super) fn finish_library_import(
        &mut self,
        prefab: Option<String>,
        result: Result<engine::project::LibraryImport, String>,
    ) -> anyhow::Result<()> {
        let imported = result.map_err(anyhow::Error::msg)?;
        self.pending_prefab = prefab.and_then(|p| {
            self.document
                .as_ref()
                .map(|doc| (format!("{}{p}", imported.prefix), doc.path.clone()))
        });
        self.vfs = self
            .project
            .as_ref()
            .context("project")?
            .registry(engine::runtime::RuntimeMode::Client)?
            .1;
        self.refresh_resources()?;
        self.status = format!(
            "{} library: {}",
            if imported.reused {
                "Reused"
            } else {
                "Imported"
            },
            display_path(&imported.directory)
        );
        self.logs.push(self.status.clone());
        Ok(())
    }

    pub(super) fn export_library(&mut self) -> anyhow::Result<()> {
        let Some(source) = self
            .file_dialog("Choose library folder containing wasset.toml")
            .pick_folder()
        else {
            return Ok(());
        };
        let Some(destination) = self
            .file_dialog("Export asset library")
            .add_filter("Whatever asset library", &["wasset"])
            .set_file_name("Library.wasset")
            .save_file()
        else {
            return Ok(());
        };
        let (sender, receiver) = mpsc::channel();
        self.job = Some(receiver);
        self.job_label = "Packing asset library…".into();
        std::thread::spawn(move || {
            let result = (|| {
                engine::project::AssetLibrary::from_directory(&source)?.write(&destination)?;
                Ok(destination)
            })()
            .map_err(|e: anyhow::Error| format!("{e:#}"));
            let _ = sender.send(JobEvent::LibraryExported(result));
        });
        Ok(())
    }
}
