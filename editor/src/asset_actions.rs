use super::*;

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum AssetOperation {
    Folder,
    Rename,
    Move,
    Delete,
}
#[derive(Clone)]
pub(super) struct AssetAction {
    operation: AssetOperation,
    owner: String,
    source: String,
    destination: String,
    error: Option<String>,
}
impl AssetAction {
    fn title(&self) -> &'static str {
        match self.operation {
            AssetOperation::Folder => "New folder",
            AssetOperation::Rename => "Rename",
            AssetOperation::Move => "Move",
            AssetOperation::Delete => "Delete",
        }
    }
}
impl Editor {
    pub(super) fn begin_asset_action(
        &mut self,
        owner: &str,
        source: &str,
        operation: AssetOperation,
    ) {
        let destination = if operation == AssetOperation::Folder {
            format!(
                "{}NewFolder",
                if source.is_empty() {
                    String::new()
                } else {
                    format!("{source}/")
                }
            )
        } else {
            source.into()
        };
        self.asset_action = Some(AssetAction {
            operation,
            owner: owner.into(),
            source: source.into(),
            destination,
            error: None,
        });
    }
    pub(super) fn asset_actions_ui(&mut self, ctx: &egui::Context) {
        let Some(mut action) = self.asset_action.take() else {
            return;
        };
        let title = action.title();
        let waiting = self.job.is_some();
        let mut apply = false;
        let mut close = false;
        egui::Modal::new(egui::Id::new("asset_operation")).show(ctx,|ui|{
            ui.set_width(480.0);ui.heading(title);ui.label(format!("{} / {}",action.owner,action.source));
            if waiting{ui.spinner();ui.label("Updating files and scene references…");ui.disable();}
            if action.operation==AssetOperation::Delete{
                ui.label("Move this item to the mod's local trash? Referencing scenes will report missing assets.");
                ui.weak("Files can be recovered from .whatever/trash. They are excluded from game builds.");
            }else{
                ui.label("Destination path, relative to this mod");ui.text_edit_singleline(&mut action.destination);
                if action.operation!=AssetOperation::Folder{ui.weak("Scene references and manifest entry points are updated. Update JS/TS imports and asset strings in scripts yourself.");}
            }
            if let Some(error)=&action.error{ui.colored_label(egui::Color32::LIGHT_RED,error);}
            ui.horizontal(|ui|{apply=ui.button(title).clicked();close=ui.button("Cancel").clicked();});
        });
        close |= !waiting && ctx.input(|input| input.key_pressed(egui::Key::Escape));
        if apply {
            let result = (|| -> anyhow::Result<()> {
                ensure!(
                    self.live.is_none() && self.job.is_none(),
                    "Stop Play or wait for the current operation before changing files"
                );
                if matches!(
                    action.operation,
                    AssetOperation::Rename | AssetOperation::Move
                ) {
                    self.component_reload.ensure_ready()?;
                }
                self.save_if_dirty()?;
                let project = self.project.clone().context("open a project")?;
                let operation = action.clone();
                let component_types = self.world.component_types.clone();
                let (sender, receiver) = mpsc::channel();
                self.job = Some(receiver);
                self.job_label = format!("{title} — {}…", action.source);
                action.error = None;
                std::thread::spawn(move || {
                    let result = match operation.operation {
                        AssetOperation::Folder => {
                            project.create_folder(&operation.owner, &operation.destination)
                        }
                        AssetOperation::Rename | AssetOperation::Move => project.move_resource(
                            &operation.owner,
                            &operation.source,
                            &operation.destination,
                            &component_types,
                        ),
                        AssetOperation::Delete => {
                            project.trash_resource(&operation.owner, &operation.source)
                        }
                    }
                    .map_err(|error| format!("{error:#}"));
                    let _ = sender.send(JobEvent::AssetChanged(operation, result));
                });
                Ok(())
            })();
            if let Err(error) = result {
                action.error = Some(format!("{error:#}"));
            }
        }
        if !close {
            self.asset_action = Some(action);
        }
    }
    pub(super) fn finish_asset_action(
        &mut self,
        mut action: AssetAction,
        result: Result<PathBuf, String>,
    ) {
        let result = result.map_err(anyhow::Error::msg).and_then(|result| {
            let root = &self
                .mod_roots
                .iter()
                .find(|(id, _)| id == &action.owner)
                .context("mod")?
                .1;
            let old = engine::sandbox::resolve_contained(root, &action.source).ok();
            if action.operation != AssetOperation::Folder
                && let Some(doc) = &mut self.document
            {
                let normalized = engine::project::portable_path(&doc.path);
                if let Some(old) = &old
                    && let Ok(tail) = normalized.strip_prefix(engine::project::portable_path(old))
                {
                    if action.operation == AssetOperation::Delete {
                        self.document = None;
                        self.inspector_entity = None;
                    } else {
                        doc.path = if tail.as_os_str().is_empty() {
                            result.clone()
                        } else {
                            result.join(tail)
                        };
                    }
                }
            }
            if matches!(
                action.operation,
                AssetOperation::Move | AssetOperation::Rename
            ) && let Some(doc) = &self.document
            {
                let path = doc.path.clone();
                let selected = doc.selected.clone();
                self.open_scene(&path)?;
                if let Some(doc) = &mut self.document {
                    doc.selected = selected;
                }
                self.sync_selection();
            }
            self.vfs = self
                .project
                .as_ref()
                .context("project")?
                .registry(engine::runtime::RuntimeMode::Client)?
                .1;
            self.refresh_resources()?;
            self.preview_dirty = true;
            self.status = if action.operation == AssetOperation::Delete {
                format!("Moved to trash: {}", display_path(&result))
            } else {
                format!("{}: {}", action.title(), display_path(&result))
            };
            self.logs.push(self.status.clone());
            Ok(())
        });
        match result {
            Ok(()) => self.asset_action = None,
            Err(error) => {
                action.error = Some(format!("{error:#}"));
                self.asset_action = Some(action);
            }
        }
    }
}
