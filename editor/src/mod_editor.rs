use super::*;

pub(super) struct ModDraft {
    path: PathBuf,
    original: String,
    text: String,
    error: Option<String>,
}
fn field(
    ui: &mut egui::Ui,
    table: &mut toml::map::Map<String, toml::Value>,
    key: &str,
    label: &str,
) -> bool {
    let mut value = table
        .get(key)
        .and_then(toml::Value::as_str)
        .unwrap_or("")
        .to_owned();
    ui.label(label);
    if ui.text_edit_singleline(&mut value).changed() {
        if value.is_empty() {
            table.remove(key);
        } else {
            table.insert(key.into(), toml::Value::String(value));
        }
        true
    } else {
        false
    }
}
impl Editor {
    pub(super) fn edit_mod(&mut self) -> anyhow::Result<()> {
        let (_, root) = self
            .mod_roots
            .get(self.active_mod)
            .context("select a mod")?;
        let path = root.join("mod.toml");
        let text = std::fs::read_to_string(&path)?;
        self.mod_draft = Some(ModDraft {
            path,
            original: text.clone(),
            text,
            error: None,
        });
        Ok(())
    }
    pub(super) fn new_script(&mut self) -> anyhow::Result<()> {
        let (_, root) = self
            .mod_roots
            .get(self.active_mod)
            .context("select a mod")?;
        let root = root.clone();
        let directory = root.join("scripts");
        std::fs::create_dir_all(&directory)?;
        if let Some(path) = self
            .file_dialog("Create a script")
            .set_directory(&directory)
            .set_file_name("new_script.ts")
            .add_filter("Scripts", &["ts", "js"])
            .save_file()
        {
            let relative = path
                .strip_prefix(&root)
                .context("scripts must be inside the selected mod")?;
            let path = engine::sandbox::resolve_contained(&root, &relative.to_string_lossy())?;
            ensure!(!path.exists(), "script already exists");
            std::fs::write(
                &path,
                "import { Engine } from \"@whatever-engine/api\";\n\nEngine.on(\"init\", () => {\n  // Add your mod behavior here.\n});\n",
            )?;
            self.refresh_resources()?;
            self.open_script(&path)?;
        }
        Ok(())
    }
    pub(super) fn mod_editor_ui(&mut self, ctx: &egui::Context) {
        let Some(mut draft) = self.mod_draft.take() else {
            return;
        };
        let mut close = false;
        let mut save = false;
        egui::Modal::new(egui::Id::new("mod_settings")).show(ctx, |ui| {
            ui.set_width(530.0);
            ui.heading("Mod Settings");
            ui.label(display_path(&draft.path));
            egui::ScrollArea::vertical()
                .max_height(570.0)
                .drag_to_scroll(false)
                .show(ui, |ui| {
                    if let Ok(mut value) = toml::from_str::<toml::Value>(&draft.text) {
                        let mut changed = false;
                        if let Some(meta) = value.get_mut("mod").and_then(toml::Value::as_table_mut)
                        {
                            ui.label(format!(
                                "Namespace: {}",
                                meta.get("id").and_then(toml::Value::as_str).unwrap_or("")
                            ));
                            for (key, label) in [
                                ("name", "Display name"),
                                ("version", "Version"),
                                ("description", "Description"),
                                ("license", "License"),
                            ] {
                                changed |= field(ui, meta, key, label);
                            }
                            let mut authors = meta
                                .get("authors")
                                .and_then(toml::Value::as_array)
                                .map(|a| {
                                    a.iter()
                                        .filter_map(toml::Value::as_str)
                                        .collect::<Vec<_>>()
                                        .join(", ")
                                })
                                .unwrap_or_default();
                            ui.label("Authors (comma separated)");
                            if ui.text_edit_singleline(&mut authors).changed() {
                                meta.insert(
                                    "authors".into(),
                                    toml::Value::Array(
                                        authors
                                            .split(',')
                                            .map(str::trim)
                                            .filter(|s| !s.is_empty())
                                            .map(|s| toml::Value::String(s.into()))
                                            .collect(),
                                    ),
                                );
                                changed = true;
                            }
                        }
                        ui.separator();
                        ui.label("Script entry points (paths relative to this mod)");
                        if let Some(root) = value.as_table_mut() {
                            let script = root
                                .entry("script")
                                .or_insert_with(|| toml::Value::Table(Default::default()));
                            if let Some(table) = script.as_table_mut() {
                                for (key, label) in [
                                    ("entry", "Shared"),
                                    ("client", "Client"),
                                    ("server", "Server"),
                                ] {
                                    ui.push_id(key, |ui| {
                                        changed |= field(ui, table, key, label);
                                        if ui.small_button("Choose script…").clicked()
                                            && let Some(root) = draft.path.parent()
                                            && let Some(path) = self
                                                .file_dialog("Choose mod script")
                                                .set_directory(root)
                                                .add_filter(
                                                    "Mod scripts",
                                                    &["ts", "js", "tsx", "jsx"],
                                                )
                                                .pick_file()
                                        {
                                            let path = engine::project::portable_path(&path);
                                            let root = engine::project::portable_path(root);
                                            match path.strip_prefix(&root) {
                                                Ok(relative) => {
                                                    table.insert(
                                                        key.into(),
                                                        toml::Value::String(
                                                            relative
                                                                .to_string_lossy()
                                                                .replace('\\', "/"),
                                                        ),
                                                    );
                                                    changed = true;
                                                }
                                                Err(_) => {
                                                    draft.error = Some(
                                                        "Choose a script inside this mod".into(),
                                                    )
                                                }
                                            }
                                        }
                                    });
                                }
                                if table.is_empty() {
                                    root.remove("script");
                                }
                            }
                        }
                        if changed && let Ok(text) = toml::to_string_pretty(&value) {
                            draft.text = text;
                        }
                    }
                    egui::CollapsingHeader::new("Complete mod.toml / dependencies / startup scene")
                        .show(ui, |ui| {
                            ui.add(
                                egui::TextEdit::multiline(&mut draft.text)
                                    .font(egui::TextStyle::Monospace)
                                    .desired_width(f32::INFINITY)
                                    .desired_rows(15),
                            );
                        });
                });
            if let Some(error) = &draft.error {
                ui.colored_label(egui::Color32::LIGHT_RED, error);
            }
            ui.horizontal(|ui| {
                save = ui.button("Save Mod").clicked();
                close = ui.button("Cancel").clicked();
            });
        });
        close |= ctx.input(|i| i.key_pressed(egui::Key::Escape));
        if save {
            let result = (|| -> anyhow::Result<()> {
                let original: engine::mods::ModManifest = toml::from_str(&draft.original)?;
                let next: engine::mods::ModManifest = toml::from_str(&draft.text)?;
                ensure!(
                    next.meta.id == original.meta.id,
                    "mod namespaces cannot be renamed here because assets and scripts reference them"
                );
                ensure!(
                    std::fs::read_to_string(&draft.path)? == draft.original,
                    "mod.toml changed outside the editor; reopen Mod Settings"
                );
                std::fs::write(&draft.path, &draft.text)?;
                let validation = self.project.as_ref().context("project")?.validate();
                if let Err(error) = validation {
                    if std::fs::read_to_string(&draft.path)? == draft.text {
                        std::fs::write(&draft.path, &draft.original)?;
                    }
                    return Err(error);
                }
                self.vfs = self
                    .project
                    .as_ref()
                    .context("project")?
                    .registry(engine::runtime::RuntimeMode::Client)?
                    .1;
                self.refresh_resources()?;
                self.preview_dirty = true;
                self.status = "Mod metadata saved".into();
                Ok(())
            })();
            match result {
                Ok(()) => close = true,
                Err(error) => draft.error = Some(format!("{error:#}")),
            }
        }
        if !close {
            self.mod_draft = Some(draft);
        }
    }
}
