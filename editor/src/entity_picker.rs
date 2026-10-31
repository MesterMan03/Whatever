use super::*;
use serde_json::{Value, json};

impl Editor {
    pub(super) fn connected_body_picker(&mut self, ui: &mut egui::Ui, data: &mut Value) -> bool {
        let old = data
            .get("connectedBody")
            .or_else(|| data.get("bodyB"))
            .and_then(|value| value.get("$entity"))
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_owned();
        let mut choices = Vec::new();
        if let Some(doc) = &self.document {
            let mut paths = Vec::new();
            hierarchy::entity_paths(&doc.scene.entities, &mut Vec::new(), &mut paths);
            for path in paths {
                if path == doc.selected {
                    continue;
                }
                if let Some(entity) = entity_at(&doc.scene.entities, &path) {
                    choices.push((path, entity.name.clone(), entity.script_id.clone()));
                }
            }
        }
        let mut selected = None;
        let mut clear = false;
        let field = ui
            .horizontal(|ui| {
                ui.label("Connected Body");
                egui::ComboBox::from_id_salt("connected_body_picker")
                    .selected_text(
                        choices
                            .iter()
                            .find(|(_, _, id)| id.as_deref() == Some(&old))
                            .map(|(_, name, _)| name.as_str())
                            .unwrap_or(if old.is_empty() {
                                "Choose entity"
                            } else {
                                &old
                            }),
                    )
                    .show_ui(ui, |ui| {
                        if ui.selectable_label(old.is_empty(), "None").clicked() {
                            clear = true;
                        }
                        for (path, name, id) in &choices {
                            if ui
                                .selectable_label(
                                    id.as_deref() == Some(&old),
                                    if name.is_empty() {
                                        id.as_deref().unwrap_or("Entity")
                                    } else {
                                        name
                                    },
                                )
                                .clicked()
                            {
                                selected = Some(path.clone());
                            }
                        }
                    });
            })
            .response;
        if self.has_dialog() {
            return false;
        }
        if let Some(drop) = ui.ctx().data_mut(|state| {
            state.get_temp::<hierarchy::DroppedEntity>(egui::Id::new("entity_drop"))
        }) && field.rect.contains(drop.position)
        {
            ui.ctx().data_mut(|state| {
                state.remove::<hierarchy::DroppedEntity>(egui::Id::new("entity_drop"))
            });
            selected = Some(drop.path);
        }
        if clear {
            data["connectedBody"] = Value::Null;
            return true;
        }
        if let Some(path) = selected {
            let result = (|| -> anyhow::Result<String> {
                let doc = self.document.as_mut().context("scene")?;
                ensure!(doc.selected != path, "a joint cannot connect to itself");
                if let Some(id) = entity_at(&doc.scene.entities, &path)
                    .and_then(|entity| entity.script_id.clone())
                {
                    return Ok(id);
                }
                let mut paths = Vec::new();
                hierarchy::entity_paths(&doc.scene.entities, &mut Vec::new(), &mut paths);
                let ids = paths
                    .iter()
                    .filter_map(|path| {
                        entity_at(&doc.scene.entities, path)
                            .and_then(|entity| entity.script_id.as_deref())
                    })
                    .collect::<Vec<_>>();
                let mut index = 1;
                let id = loop {
                    let candidate = format!("entity_{index}");
                    if !ids.contains(&candidate.as_str()) {
                        break candidate;
                    }
                    index += 1;
                };
                doc.checkpoint();
                entity_mut(&mut doc.scene.entities, &path)
                    .context("referenced entity")?
                    .script_id = Some(id.clone());
                self.preview_dirty = true;
                Ok(id)
            })();
            match result {
                Ok(id) => {
                    data["connectedBody"] = json!({"$entity":id});
                    if let Some(fields) = data.as_object_mut() {
                        fields.remove("bodyA");
                        fields.remove("bodyB");
                    }
                    return true;
                }
                Err(error) => self.inspector_error = Some(error.to_string()),
            }
        }
        false
    }
}
