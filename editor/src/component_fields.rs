use super::*;
use engine::schema::Schema;
use serde_json::{Value, json};

fn initial(schema: &Schema) -> Value {
    match schema {
        Schema::I32 { min, .. } | Schema::F64 { min, .. } | Schema::Varint { min, .. } => {
            json!(min.unwrap_or(0.0))
        }
        Schema::Bool => json!(false),
        Schema::String { .. } | Schema::Asset => json!(""),
        Schema::Enum { values } => json!(values.first().cloned().unwrap_or_default()),
        Schema::Array { .. } => json!([]),
        Schema::Tuple { elements } => Value::Array(elements.iter().map(initial).collect()),
        Schema::Struct { fields } => Value::Object(
            fields
                .iter()
                .map(|field| (field.name.clone(), initial(&field.schema)))
                .collect(),
        ),
        Schema::Optional { .. }
        | Schema::Nullable { .. }
        | Schema::Entity
        | Schema::Component { .. } => Value::Null,
    }
}

impl Editor {
    pub(super) fn custom_component_form(
        &mut self,
        ui: &mut egui::Ui,
        definition: &engine::ecs::ComponentDefinition,
        data: &mut Value,
        names: &[String],
    ) -> bool {
        let Some(object) = data.as_object_mut() else {
            ui.colored_label(egui::Color32::LIGHT_RED, "Component data must be an object");
            return false;
        };
        let mut changed = false;
        for field in &definition.fields {
            ui.push_id(&field.name, |ui| {
                let value = object
                    .entry(field.name.clone())
                    .or_insert_with(|| field.default.clone());
                let label = field.editor.label.as_deref().unwrap_or(&field.name);
                ui.horizontal(|ui| {
                    let response = ui.label(label);
                    if let Some(tooltip) = &field.editor.tooltip {
                        response.on_hover_text(tooltip);
                    }
                    if ui
                        .small_button("↺")
                        .on_hover_text("Reset to class default")
                        .clicked()
                    {
                        *value = field.default.clone();
                        changed = true;
                    }
                });
                changed |= self.schema_value(ui, &field.schema, value, names, 0);
            });
        }
        changed
    }

    fn schema_value(
        &mut self,
        ui: &mut egui::Ui,
        schema: &Schema,
        value: &mut Value,
        names: &[String],
        depth: usize,
    ) -> bool {
        if depth > 32 {
            return false;
        }
        match schema {
            Schema::I32 { min, max } | Schema::F64 { min, max } | Schema::Varint { min, max } => {
                let mut number = value.as_f64().unwrap_or(0.0);
                let integer = !matches!(schema, Schema::F64 { .. });
                let low = min.unwrap_or(if integer { i32::MIN as f64 } else { f64::MIN });
                let high = max.unwrap_or(if integer { i32::MAX as f64 } else { f64::MAX });
                if ui
                    .add(
                        egui::DragValue::new(&mut number)
                            .range(low..=high)
                            .speed(if integer { 1.0 } else { 0.05 })
                            .max_decimals(if integer { 0 } else { 6 }),
                    )
                    .changed()
                {
                    *value = if integer {
                        json!(number.round() as i32)
                    } else {
                        json!(number)
                    };
                    return true;
                }
                false
            }
            Schema::Bool => {
                let mut flag = value.as_bool().unwrap_or(false);
                if ui.checkbox(&mut flag, "").changed() {
                    *value = json!(flag);
                    true
                } else {
                    false
                }
            }
            Schema::String { .. } => {
                let mut text = value.as_str().unwrap_or_default().to_owned();
                if ui.text_edit_singleline(&mut text).changed() {
                    *value = json!(text);
                    true
                } else {
                    false
                }
            }
            Schema::Enum { values } => {
                let mut choice = value.as_str().unwrap_or_default().to_owned();
                let old = choice.clone();
                egui::ComboBox::from_id_salt("choice")
                    .selected_text(&choice)
                    .show_ui(ui, |ui| {
                        for option in values {
                            ui.selectable_value(&mut choice, option.clone(), option);
                        }
                    });
                if old != choice {
                    *value = json!(choice);
                    true
                } else {
                    false
                }
            }
            Schema::Entity | Schema::Component { .. } => {
                let old = value
                    .get("$entity")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .to_owned();
                let mut selected = old.clone();
                egui::ComboBox::from_id_salt("entity")
                    .selected_text(if selected.is_empty() {
                        "Choose entity"
                    } else {
                        &selected
                    })
                    .show_ui(ui, |ui| {
                        for name in names {
                            if let Schema::Component { component_type } = schema {
                                fn has(
                                    items: &[engine::scene::SceneEntity],
                                    name: &str,
                                    kind: &str,
                                ) -> bool {
                                    items.iter().any(|entity| {
                                        (entity.script_id.as_deref() == Some(name)
                                            && entity.components.contains_key(kind))
                                            || has(&entity.children, name, kind)
                                    })
                                }
                                if !self.document.as_ref().is_some_and(|doc| {
                                    has(&doc.scene.entities, name, component_type)
                                }) {
                                    continue;
                                }
                            }
                            ui.selectable_value(&mut selected, name.clone(), name);
                        }
                    });
                if old != selected {
                    *value = match schema {
                        Schema::Component { component_type } => {
                            json!({"$entity":selected,"$component":component_type})
                        }
                        _ => json!({"$entity":selected}),
                    };
                    true
                } else {
                    false
                }
            }
            Schema::Asset => {
                let mut holder = json!({"asset":value.clone()});
                if self.resource_field(
                    ui,
                    &mut holder,
                    "asset",
                    "Asset",
                    &[
                        "png", "jpg", "jpeg", "obj", "gltf", "glb", "wgsl", "ttf", "otf", "wav",
                        "ogg", "mp3", "json",
                    ],
                    false,
                ) {
                    *value = holder["asset"].take();
                    true
                } else {
                    false
                }
            }
            Schema::Nullable { inner } | Schema::Optional { inner } => {
                let mut changed = false;
                if matches!(inner.as_ref(), Schema::Entity | Schema::Component { .. }) {
                    changed |= self.schema_value(ui, inner, value, names, depth + 1);
                    if !value.is_null() && ui.small_button("Clear reference").clicked() {
                        *value = Value::Null;
                        changed = true;
                    }
                    return changed;
                }
                let mut present = !value.is_null();
                if ui.checkbox(&mut present, "Set value").changed() {
                    *value = if present { initial(inner) } else { Value::Null };
                    changed = true;
                }
                if present {
                    changed |= self.schema_value(ui, inner, value, names, depth + 1);
                }
                changed
            }
            Schema::Struct { fields } => {
                let Some(object) = value.as_object_mut() else {
                    ui.weak("Invalid object");
                    return false;
                };
                let mut changed = false;
                for field in fields {
                    ui.push_id(&field.name, |ui| {
                        ui.label(&field.name);
                        let item = object
                            .entry(field.name.clone())
                            .or_insert_with(|| initial(&field.schema));
                        changed |= self.schema_value(ui, &field.schema, item, names, depth + 1);
                    });
                }
                changed
            }
            Schema::Array {
                element,
                max_length,
            } => {
                let Some(items) = value.as_array_mut() else {
                    ui.weak("Invalid array");
                    return false;
                };
                let mut changed = false;
                let mut remove = None;
                for (index, item) in items.iter_mut().enumerate() {
                    ui.push_id(index, |ui| {
                        ui.horizontal(|ui| {
                            ui.label(index.to_string());
                            if ui.small_button("×").clicked() {
                                remove = Some(index);
                            }
                        });
                        changed |= self.schema_value(ui, element, item, names, depth + 1);
                    });
                }
                if let Some(index) = remove {
                    items.remove(index);
                    changed = true;
                }
                if items.len() < max_length.unwrap_or(engine::schema::MAX_ITEMS)
                    && ui.small_button("+ Item").clicked()
                {
                    items.push(initial(element));
                    changed = true;
                }
                changed
            }
            Schema::Tuple { elements } => {
                let Some(items) = value.as_array_mut() else {
                    ui.weak("Invalid tuple");
                    return false;
                };
                let mut changed = false;
                for (index, (schema, item)) in elements.iter().zip(items).enumerate() {
                    ui.push_id(index, |ui| {
                        changed |= self.schema_value(ui, schema, item, names, depth + 1);
                    });
                }
                changed
            }
        }
    }
}
