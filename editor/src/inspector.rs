use super::*;
use serde_json::{Value, json};

#[derive(Clone)]
struct RawDraft {
    text: String,
    base: Value,
    error: Option<String>,
}

fn raw_editor(ui: &mut egui::Ui, data: &mut Value) -> bool {
    let id = ui.make_persistent_id("raw_component");
    let mut draft = ui
        .ctx()
        .data_mut(|state| state.get_temp::<RawDraft>(id))
        .unwrap_or_else(|| RawDraft {
            text: serde_json::to_string_pretty(data).unwrap_or_default(),
            base: data.clone(),
            error: None,
        });
    if draft.base != *data {
        draft.text = serde_json::to_string_pretty(data).unwrap_or_default();
        draft.base = data.clone();
        draft.error = None;
    }
    ui.add(
        egui::TextEdit::multiline(&mut draft.text)
            .code_editor()
            .desired_rows(7)
            .desired_width(f32::INFINITY),
    );
    let mut changed = false;
    if ui.button("Apply JSON").clicked() {
        match serde_json::from_str::<Value>(&draft.text) {
            Ok(value) => {
                *data = value;
                draft.base = data.clone();
                draft.error = None;
                changed = true;
            }
            Err(error) => draft.error = Some(error.to_string()),
        }
    }
    if let Some(error) = &draft.error {
        ui.colored_label(egui::Color32::LIGHT_RED, error);
    }
    ui.ctx().data_mut(|state| state.insert_temp(id, draft));
    changed
}

pub(super) const COMPONENTS: &[(&str, &str)] = &[
    ("core:transform", "Transform"),
    ("core:camera", "Camera"),
    ("core:mesh_renderer", "Mesh Renderer"),
    ("core:sprite_renderer", "Sprite Renderer"),
    ("core:text_renderer", "Text Renderer"),
    ("core:ambient_light", "Ambient Light"),
    ("core:directional_light", "Directional Light"),
    ("core:point_light", "Point Light"),
    ("core:physics_world", "Physics World"),
    ("core:rigid_body", "Rigid Body"),
    ("core:collider", "Collider"),
    ("core:joint", "Joint"),
];
pub(super) fn default_component(kind: &str) -> Value {
    match kind {
        "core:transform" => json!({"position":[0,0,0],"rotation":[0,0,0,1],"scale":[1,1,1]}),
        "core:camera" => json!({"fovy_degrees":60,"znear":0.05,"zfar":1000}),
        "core:mesh_renderer" => json!({"mesh":"","shader":"core://shaders/mesh_lit.wgsl"}),
        "core:sprite_renderer" => json!({"texture":"","shader":"core://shaders/sprite.wgsl"}),
        "core:text_renderer" => {
            json!({"text":"Text","font":"core://fonts/default.ttf","shader":"core://shaders/sprite.wgsl","font_size":24,"color":[1,1,1,1]})
        }
        "core:ambient_light" => json!({"color":[1,1,1],"intensity":0.2}),
        "core:directional_light" => json!({"color":[1,1,1],"intensity":1,"direction":[0.4,1,0.6]}),
        "core:point_light" => json!({"color":[1,1,1],"intensity":1,"range":10}),
        "core:physics_world" => json!({"gravity":[0,-9.81,0],"substeps":1,"timeScale":1}),
        "core:rigid_body" => json!({"type":"dynamic","world":null,"gravityScale":1,"ccd":false}),
        "core:collider" => json!({"shape":{"type":"box","size":[1,1,1]},"friction":0.5}),
        "core:joint" => json!({"type":"fixed","connectedBody":null}),
        _ => json!({}),
    }
}

fn number(
    ui: &mut egui::Ui,
    data: &mut Value,
    key: &str,
    label: &str,
    default: f64,
    range: std::ops::RangeInclusive<f64>,
) -> bool {
    let mut value = data.get(key).and_then(Value::as_f64).unwrap_or(default);
    let changed = ui
        .horizontal(|ui| {
            ui.label(label);
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.add(egui::DragValue::new(&mut value).speed(0.05).range(range))
                    .changed()
            })
            .inner
        })
        .inner;
    if changed {
        data[key] = json!(value);
    }
    changed
}
fn boolean(ui: &mut egui::Ui, data: &mut Value, key: &str, label: &str, default: bool) -> bool {
    let mut value = data.get(key).and_then(Value::as_bool).unwrap_or(default);
    let changed = ui.checkbox(&mut value, label).changed();
    if changed {
        data[key] = json!(value);
    }
    changed
}
fn vector(ui: &mut egui::Ui, data: &mut Value, key: &str, label: &str, default: [f64; 3]) -> bool {
    let mut values = default;
    for (i, value) in values.iter_mut().enumerate() {
        *value = data
            .get(key)
            .and_then(|v| v.get(i))
            .and_then(Value::as_f64)
            .unwrap_or(*value);
    }
    ui.label(label);
    let mut changed = false;
    ui.horizontal(|ui| {
        for i in 0..3 {
            let color = [
                egui::Color32::from_rgb(220, 106, 117),
                egui::Color32::from_rgb(125, 198, 142),
                egui::Color32::from_rgb(106, 166, 235),
            ][i];
            ui.colored_label(color, ["X", "Y", "Z"][i]);
            changed |= ui
                .add(
                    egui::DragValue::new(&mut values[i])
                        .speed(0.05)
                        .max_decimals(3),
                )
                .changed();
        }
    });
    if changed {
        data[key] = json!(values);
    }
    changed
}
fn enumeration(
    ui: &mut egui::Ui,
    data: &mut Value,
    key: &str,
    label: &str,
    default: &str,
    values: &[&str],
) -> bool {
    let mut value = data
        .get(key)
        .and_then(Value::as_str)
        .unwrap_or(default)
        .to_owned();
    let old = value.clone();
    ui.horizontal(|ui| {
        ui.label(label);
        egui::ComboBox::from_id_salt(key)
            .selected_text(&value)
            .show_ui(ui, |ui| {
                for option in values {
                    ui.selectable_value(&mut value, (*option).into(), *option);
                }
            });
    });
    if value != old {
        data[key] = json!(value);
        true
    } else {
        false
    }
}
fn color(ui: &mut egui::Ui, data: &mut Value, key: &str, alpha: bool) -> bool {
    let mut value = [1_f32; 4];
    for (i, v) in value.iter_mut().enumerate() {
        *v = data
            .get(key)
            .and_then(|v| v.get(i))
            .and_then(Value::as_f64)
            .unwrap_or(1.0) as f32;
    }
    let changed = ui
        .horizontal(|ui| {
            ui.label("Color");
            if alpha {
                ui.color_edit_button_rgba_unmultiplied(&mut value).changed()
            } else {
                let mut rgb = [value[0], value[1], value[2]];
                let changed = ui.color_edit_button_rgb(&mut rgb).changed();
                value[..3].copy_from_slice(&rgb);
                changed
            }
        })
        .inner;
    if changed {
        data[key] = if alpha {
            json!(value)
        } else {
            json!(&value[..3])
        };
    }
    changed
}
fn reference(
    ui: &mut egui::Ui,
    data: &mut Value,
    key: &str,
    label: &str,
    names: &[String],
) -> bool {
    let old = data
        .get(key)
        .and_then(|v| v.get("$entity"))
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_owned();
    let mut current = old.clone();
    ui.horizontal(|ui| {
        ui.label(label);
        egui::ComboBox::from_id_salt(key)
            .selected_text(if current.is_empty() {
                "Choose entity"
            } else {
                &current
            })
            .show_ui(ui, |ui| {
                ui.selectable_value(&mut current, String::new(), "None");
                for name in names {
                    ui.selectable_value(&mut current, name.clone(), name);
                }
            });
    });
    if current != old {
        data[key] = if current.is_empty() {
            Value::Null
        } else {
            json!({"$entity":current})
        };
        true
    } else {
        false
    }
}

impl Editor {
    pub(super) fn scene_physics_world(&mut self, checkpoint: bool) -> anyhow::Result<String> {
        let doc = self.document.as_mut().context("open a scene")?;
        let mut paths = Vec::new();
        hierarchy::entity_paths(&doc.scene.entities, &mut Vec::new(), &mut paths);
        let existing = paths.iter().find(|path| {
            crate::document::entity_at(&doc.scene.entities, path)
                .is_some_and(|entity| entity.components.contains_key("core:physics_world"))
        });
        if let Some(name) = existing
            .and_then(|path| crate::document::entity_at(&doc.scene.entities, path))
            .and_then(|entity| entity.script_id.clone())
        {
            return Ok(name);
        }
        let names = paths
            .iter()
            .filter_map(|path| {
                crate::document::entity_at(&doc.scene.entities, path)
                    .and_then(|entity| entity.script_id.as_deref())
            })
            .collect::<Vec<_>>();
        let mut name = "physics_world".to_owned();
        let mut suffix = 2;
        while names.contains(&name.as_str()) {
            name = format!("physics_world_{suffix}");
            suffix += 1;
        }
        let mut candidate = doc.scene.clone();
        if let Some(path) = existing {
            crate::document::entity_mut(&mut candidate.entities, path)
                .context("physics world")?
                .script_id = Some(name.clone());
        } else {
            candidate.entities.push(SceneEntity {
                enabled: true,
                name: "Physics world".into(),
                script_id: Some(name.clone()),
                components: [(
                    "core:physics_world".into(),
                    default_component("core:physics_world"),
                )]
                .into(),
                children: Vec::new(),
                tags: Vec::new(),
            });
        }
        candidate.validate()?;
        if checkpoint {
            doc.checkpoint();
        }
        doc.scene = candidate;
        self.preview_dirty = true;
        Ok(name)
    }
    pub(super) fn resource_field(
        &mut self,
        ui: &mut egui::Ui,
        data: &mut Value,
        key: &str,
        label: &str,
        extensions: &[&str],
        optional: bool,
    ) -> bool {
        let old = data
            .get(key)
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_owned();
        let mut value = old.clone();
        ui.label(label);
        ui.horizontal(|ui| {
            let text = if value.is_empty() {
                if optional { "None" } else { "Choose asset" }
            } else {
                value.rsplit('/').next().unwrap_or(&value)
            };
            egui::ComboBox::from_id_salt(key)
                .width((ui.available_width() - 40.0).max(100.0))
                .selected_text(text)
                .show_ui(ui, |ui| {
                    if optional {
                        ui.selectable_value(&mut value, String::new(), "None");
                    }
                    for resource in &self.resources {
                        if !resource.scene
                            && resource.path.extension().is_some_and(|ext| {
                                extensions.iter().any(|kind| ext.eq_ignore_ascii_case(kind))
                            })
                        {
                            ui.selectable_value(&mut value, resource.uri.clone(), &resource.uri);
                        }
                    }
                })
                .response
                .on_hover_text(&value);
            if ui
                .small_button("…")
                .on_hover_text("Choose a file; external assets are imported into the active mod")
                .clicked()
                && let Some(path) = self
                    .file_dialog(label)
                    .add_filter(label, extensions)
                    .pick_file()
            {
                match self.import_asset(&path) {
                    Ok(uri) => value = uri,
                    Err(error) => self.status = format!("{error:#}"),
                }
            }
        });
        if value != old {
            if value.is_empty() && optional {
                data.as_object_mut().map(|fields| fields.remove(key));
            } else {
                data[key] = json!(value);
            }
            true
        } else {
            false
        }
    }
    fn component_form(
        &mut self,
        ui: &mut egui::Ui,
        kind: &str,
        data: &mut Value,
        names: &[String],
    ) -> bool {
        if let Some(definition) = self.world.component_types.get(kind).cloned() {
            return self.custom_component_form(ui, &definition, data, names);
        }
        let mut changed = false;
        if !data.is_object() {
            return json_tree(ui, data, 0);
        }
        match kind {
            "core:transform" => {
                changed |= vector(ui, data, "position", "Position", [0.0; 3]);
                let quaternion = data
                    .get("rotation")
                    .and_then(Value::as_array)
                    .map(|items| {
                        std::array::from_fn(|i| {
                            items.get(i).and_then(Value::as_f64).unwrap_or(if i == 3 {
                                1.0
                            } else {
                                0.0
                            }) as f32
                        })
                    })
                    .unwrap_or([0.0, 0.0, 0.0, 1.0]);
                let quat = glam::Quat::from_array(quaternion);
                let quat = if quat.is_finite() && quat.length_squared() > 1.0e-8 {
                    quat.normalize()
                } else {
                    glam::Quat::IDENTITY
                };
                let (x, y, z) = quat.to_euler(glam::EulerRot::XYZ);
                let mut euler = json!({"euler":[x.to_degrees(),y.to_degrees(),z.to_degrees()]});
                if vector(ui, &mut euler, "euler", "Rotation (degrees)", [0.0; 3]) {
                    let values: [f32; 3] =
                        std::array::from_fn(|i| euler["euler"][i].as_f64().unwrap_or(0.0) as f32);
                    data["rotation"] = json!(
                        glam::Quat::from_euler(
                            glam::EulerRot::XYZ,
                            values[0].to_radians(),
                            values[1].to_radians(),
                            values[2].to_radians()
                        )
                        .to_array()
                    );
                    changed = true;
                }
                changed |= vector(ui, data, "scale", "Scale", [1.0; 3]);
            }
            "core:camera" => {
                changed |= number(ui, data, "fovy_degrees", "Field of view", 45.0, 1.0..=179.0);
                changed |= number(ui, data, "znear", "Near clip", 0.1, 0.001..=1000.0);
                changed |= number(ui, data, "zfar", "Far clip", 1000.0, 0.01..=1000000.0);
            }
            "core:mesh_renderer" => {
                changed |= self.resource_field(
                    ui,
                    data,
                    "mesh",
                    "Mesh",
                    &["obj", "glb", "gltf", "json"],
                    false,
                );
                changed |= self.resource_field(ui, data, "shader", "Shader", &["wgsl"], false);
                changed |= self.resource_field(ui, data, "texture", "Texture", &["png"], true);
            }
            "core:sprite_renderer" => {
                changed |= self.resource_field(ui, data, "texture", "Texture", &["png"], false);
                changed |= self.resource_field(ui, data, "shader", "Shader", &["wgsl"], false);
            }
            "core:text_renderer" => {
                let mut text = data["text"].as_str().unwrap_or("").to_owned();
                ui.label("Text");
                if ui
                    .add(
                        egui::TextEdit::multiline(&mut text)
                            .desired_rows(3)
                            .desired_width(f32::INFINITY),
                    )
                    .changed()
                {
                    data["text"] = json!(text);
                    changed = true;
                }
                changed |= self.resource_field(ui, data, "font", "Font", &["ttf", "otf"], false);
                changed |= self.resource_field(ui, data, "shader", "Shader", &["wgsl"], false);
                changed |= number(ui, data, "font_size", "Font size", 24.0, 1.0..=1000.0);
                changed |= color(ui, data, "color", true);
            }
            "core:ambient_light" | "core:directional_light" | "core:point_light" => {
                changed |= color(ui, data, "color", false);
                changed |= number(
                    ui,
                    data,
                    "intensity",
                    "Intensity",
                    if kind == "core:ambient_light" {
                        0.1
                    } else {
                        1.0
                    },
                    0.0..=100000.0,
                );
                if kind == "core:directional_light" {
                    changed |= vector(ui, data, "direction", "Direction", [0.0, 1.0, 0.0]);
                }
                if kind == "core:point_light" {
                    changed |= number(ui, data, "range", "Range", 10.0, 0.01..=100000.0);
                }
            }
            "core:physics_world" => {
                ui.weak("3D · native Rapier simulation");
                changed |= vector(ui, data, "gravity", "Gravity", [0.0, -9.81, 0.0]);
                changed |= number(ui, data, "substeps", "Substeps", 1.0, 1.0..=1024.0);
                if changed {
                    data["substeps"] =
                        json!(data["substeps"].as_f64().unwrap_or(1.0).round() as u32);
                }
                changed |= number(ui, data, "timeScale", "Time scale", 1.0, 0.0..=100.0);
            }
            "core:rigid_body" => {
                changed |= enumeration(
                    ui,
                    data,
                    "type",
                    "Body type",
                    "dynamic",
                    &["dynamic", "fixed", "kinematicPosition", "kinematicVelocity"],
                );
                changed |= reference(ui, data, "world", "Physics world", names);
                changed |= number(
                    ui,
                    data,
                    "gravityScale",
                    "Gravity scale",
                    1.0,
                    -100.0..=100.0,
                );
                changed |= number(
                    ui,
                    data,
                    "linearDamping",
                    "Linear damping",
                    0.0,
                    0.0..=1000.0,
                );
                changed |= number(
                    ui,
                    data,
                    "angularDamping",
                    "Angular damping",
                    0.0,
                    0.0..=1000.0,
                );
                changed |= boolean(ui, data, "ccd", "Continuous collision detection", false);
                changed |= boolean(ui, data, "canSleep", "Allow sleeping", true);
                changed |= boolean(ui, data, "enabled", "Enabled", true);
                let mut locks = data
                    .get("lockedAxes")
                    .and_then(Value::as_array)
                    .cloned()
                    .unwrap_or_default();
                ui.label("Lock axes");
                for (label, axes) in [
                    ("Position", ["translationX", "translationY", "translationZ"]),
                    ("Rotation", ["rotationX", "rotationY", "rotationZ"]),
                ] {
                    ui.horizontal(|ui| {
                        ui.label(label);
                        for (i, axis) in axes.iter().enumerate() {
                            let mut locked = locks.iter().any(|value| value == axis);
                            if ui.checkbox(&mut locked, ["X", "Y", "Z"][i]).changed() {
                                locks.retain(|value| value != axis);
                                if locked {
                                    locks.push(json!(axis));
                                }
                                changed = true;
                                data["lockedAxes"] = json!(locks);
                            }
                        }
                    });
                }
            }
            "core:collider" => {
                if ui.button("Edit collider in Scene").clicked() {
                    self.component_handle = Some(component_handles::ComponentHandle::Collider);
                    self.tool = viewport::Tool::Move;
                }

                let mut attachment = match data.get("body") {
                    None => "This entity",
                    Some(Value::Null) => "Standalone",
                    _ => "Another body",
                }
                .to_owned();
                let before = attachment.clone();
                egui::ComboBox::from_id_salt("collider_attachment")
                    .selected_text(&attachment)
                    .show_ui(ui, |ui| {
                        for label in ["This entity", "Another body", "Standalone"] {
                            ui.selectable_value(&mut attachment, label.into(), label);
                        }
                    });
                if attachment != before {
                    if let Some(fields) = data.as_object_mut() {
                        fields.remove("world");
                        if attachment == "This entity" {
                            fields.remove("body");
                        } else {
                            fields.insert(
                                "body".into(),
                                if attachment == "Standalone" {
                                    Value::Null
                                } else {
                                    json!({"$entity":""})
                                },
                            );
                        }
                    }
                    changed = true;
                }
                if attachment == "Another body" {
                    changed |= reference(ui, data, "body", "Attached body", names);
                }
                if attachment == "Standalone" {
                    changed |= reference(ui, data, "world", "Physics world", names);
                }
                let old = data
                    .get("shape")
                    .cloned()
                    .unwrap_or(json!({"type":"box","size":[1,1,1]}));
                let mut shape = old.clone();
                if enumeration(
                    ui,
                    &mut shape,
                    "type",
                    "Shape",
                    "box",
                    &["box", "sphere", "capsule", "cylinder", "cone", "plane"],
                ) {
                    shape = match shape["type"].as_str().unwrap_or("box") {
                        "sphere" => json!({"type":"sphere","radius":0.5}),
                        "capsule" | "cylinder" | "cone" => {
                            json!({"type":shape["type"],"radius":0.5,"height":2})
                        }
                        "plane" => json!({"type":"plane","normal":[0,1,0]}),
                        _ => json!({"type":"box","size":[1,1,1]}),
                    };
                }
                match shape["type"].as_str().unwrap_or("box") {
                    "box" => {
                        vector(ui, &mut shape, "size", "Dimensions", [1.0; 3]);
                        number(
                            ui,
                            &mut shape,
                            "borderRadius",
                            "Border radius",
                            0.0,
                            0.0..=100.0,
                        );
                    }
                    "plane" => {
                        vector(ui, &mut shape, "normal", "Normal", [0.0, 1.0, 0.0]);
                    }
                    _ => {
                        number(ui, &mut shape, "radius", "Radius", 0.5, 0.001..=10000.0);
                        if shape.get("height").is_some() {
                            number(ui, &mut shape, "height", "Height", 2.0, 0.001..=10000.0);
                        }
                    }
                }
                if shape != old {
                    data["shape"] = shape;
                    changed = true;
                }
                changed |= vector(ui, data, "position", "Offset", [0.0; 3]);
                if data.get("mass").is_some() {
                    changed |= number(ui, data, "mass", "Mass", 1.0, 0.001..=1000000.0);
                } else {
                    changed |= number(ui, data, "density", "Density", 1.0, 0.001..=1000000.0);
                }
                changed |= number(ui, data, "friction", "Friction", 0.5, 0.0..=10.0);
                changed |= number(ui, data, "restitution", "Restitution", 0.0, 0.0..=1.0);
                changed |= boolean(ui, data, "sensor", "Sensor", false);
                changed |= boolean(ui, data, "enabled", "Enabled", true);
            }
            "core:joint" => {
                ui.horizontal(|ui| {
                    for (label, handle) in [
                        (
                            "Edit owner anchor",
                            component_handles::ComponentHandle::JointA,
                        ),
                        (
                            "Edit connected anchor",
                            component_handles::ComponentHandle::JointB,
                        ),
                    ] {
                        let available = handle != component_handles::ComponentHandle::JointB
                            || data
                                .get("connectedBody")
                                .or_else(|| data.get("bodyB"))
                                .and_then(|v| v.get("$entity"))
                                .is_some();
                        if ui
                            .add_enabled(available, egui::Button::new(label).small())
                            .clicked()
                        {
                            self.component_handle = Some(handle);
                            self.tool = viewport::Tool::Move;
                        }
                    }
                });

                changed |= enumeration(
                    ui,
                    data,
                    "type",
                    "Joint type",
                    "fixed",
                    &["fixed", "revolute", "prismatic", "spherical", "generic"],
                );
                ui.weak("This entity is the first rigid body.");
                changed |= self.connected_body_picker(ui, data);
                changed |= vector(ui, data, "anchorA", "Anchor A", [0.0; 3]);
                changed |= vector(ui, data, "anchorB", "Anchor B", [0.0; 3]);
                if matches!(data["type"].as_str(), Some("revolute" | "prismatic")) {
                    changed |= vector(ui, data, "axisA", "Axis A", [0.0, 1.0, 0.0]);
                    changed |= vector(ui, data, "axisB", "Axis B", [0.0, 1.0, 0.0]);
                }
                changed |= boolean(
                    ui,
                    data,
                    "contactsEnabled",
                    "Contacts between bodies",
                    false,
                );
                changed |= boolean(ui, data, "enabled", "Enabled", true);
            }
            "core:rigid_body_state" | "core:physics_status" => {
                ui.weak("Native read-only state — generated during Play.");
            }
            _ => {
                changed |= json_tree(ui, data, 0);
            }
        }
        if kind.starts_with("core:") {
            egui::CollapsingHeader::new("Advanced data").show(ui, |ui| {
                changed |= json_tree(ui, data, 0);
            });
        }
        egui::CollapsingHeader::new("Edit JSON").show(ui, |ui| {
            changed |= raw_editor(ui, data);
        });
        changed
    }
    pub(super) fn inspector_ui(&mut self, ui: &mut egui::Ui) {
        if self.has_dialog() || self.live_waiting() {
            ui.disable();
        }
        let Some(mut entity) = self.inspector_entity.take() else {
            ui.add_space(30.0);
            ui.vertical_centered(|ui| {
                ui.weak("Select an entity in the Hierarchy\nto edit its components.");
            });
            return;
        };
        let mut names = Vec::new();
        fn collect(items: &[SceneEntity], names: &mut Vec<String>) {
            for item in items {
                if let Some(name) = &item.script_id {
                    names.push(name.clone());
                }
                collect(&item.children, names);
            }
        }
        if let Some(doc) = &self.document {
            collect(&doc.scene.entities, &mut names);
        }
        let mut changed = false;
        changed |= ui
            .checkbox(&mut entity.enabled, "Enabled")
            .on_hover_text("Disabled entities and descendants do not render or simulate")
            .changed();
        ui.add_space(8.0);
        let response = ui.add(
            egui::TextEdit::singleline(&mut entity.name)
                .id_salt("entity_name")
                .font(egui::TextStyle::Heading)
                .desired_width(f32::INFINITY),
        );
        changed |= response.changed();
        if self.rename_requested {
            response.request_focus();
            response.scroll_to_me(Some(egui::Align::Center));
            let mut state = egui::TextEdit::load_state(ui.ctx(), response.id).unwrap_or_default();
            state
                .cursor
                .set_char_range(Some(egui::text::CCursorRange::two(
                    egui::text::CCursor::new(0),
                    egui::text::CCursor::new(entity.name.chars().count()),
                )));
            state.store(ui.ctx(), response.id);
            self.rename_requested = false;
        }
        ui.horizontal(|ui| {
            ui.weak(if self.live.is_some() {
                "Runtime ID"
            } else {
                "Script ID"
            });
            let mut id = entity.script_id.clone().unwrap_or_default();
            if ui
                .add_enabled(self.live.is_none(), egui::TextEdit::singleline(&mut id))
                .on_hover_text("Unique name used by scripts and scene entity references")
                .changed()
            {
                entity.script_id = (!id.is_empty()).then_some(id);
                changed = true;
            }
        });
        ui.add_space(8.0);
        ui.horizontal_wrapped(|ui| {
            ui.label(format!("Tags ({}/32)", entity.tags.len()));
            let mut remove_tag = None;
            for (index, tag) in entity.tags.iter().enumerate() {
                if ui.small_button(format!("{tag} ×")).clicked() {
                    remove_tag = Some(index);
                }
            }
            if let Some(index) = remove_tag {
                entity.tags.remove(index);
                changed = true;
            }
        });
        ui.horizontal(|ui| {
            let id = ui.make_persistent_id("new_entity_tag");
            let mut tag = ui
                .ctx()
                .data_mut(|state| state.get_temp::<String>(id).unwrap_or_default());
            let response = ui.add(egui::TextEdit::singleline(&mut tag).hint_text("New tag"));
            let valid = entity.tags.len() < 32
                && engine::ecs::validate_tag(&tag).is_ok()
                && !entity.tags.contains(&tag);
            if (ui
                .add_enabled(valid, egui::Button::new("Add tag"))
                .clicked()
                || (response.lost_focus()
                    && ui.input(|i| i.key_pressed(egui::Key::Enter))
                    && valid))
                && valid
            {
                entity.tags.push(std::mem::take(&mut tag));
                changed = true;
            }
            ui.ctx().data_mut(|state| state.insert_temp(id, tag));
        });
        ui.add_space(8.0);
        let mut remove = None;
        let mut kinds = entity.components.keys().cloned().collect::<Vec<_>>();
        kinds.sort_by_key(|kind| {
            COMPONENTS
                .iter()
                .position(|(id, _)| *id == kind)
                .unwrap_or(COMPONENTS.len())
        });
        for kind in kinds {
            let Some(data) = entity.components.get_mut(&kind) else {
                continue;
            };
            let title = COMPONENTS
                .iter()
                .find(|(id, _)| *id == kind)
                .map(|(_, title)| *title)
                .unwrap_or(&kind);
            egui::Frame::group(ui.style())
                .inner_margin(egui::Margin::same(10.0))
                .show(ui, |ui| {
                    ui.push_id(&kind, |ui| {
                        ui.horizontal(|ui| {
                            ui.strong(title);
                            ui.with_layout(
                                egui::Layout::right_to_left(egui::Align::Center),
                                |ui| {
                                    if ui
                                        .small_button("×")
                                        .on_hover_text("Remove component")
                                        .clicked()
                                    {
                                        remove = Some(kind.clone());
                                    }
                                },
                            );
                        });
                        ui.add_space(6.0);
                        changed |= self.component_form(ui, &kind, data, &names);
                    });
                });
            ui.add_space(5.0);
        }
        if let Some(kind) = remove {
            entity.components.remove(&kind);
            changed = true;
        }
        ui.add_space(8.0);
        ui.menu_button("+  Add Component", |ui| {
            for (kind, title) in COMPONENTS {
                if !entity.components.contains_key(*kind) && ui.button(*title).clicked() {
                    let mut component = default_component(kind);
                    if *kind == "core:rigid_body" {
                        match self.scene_physics_world(true) {
                            Ok(world) => {
                                component["world"] = json!({"$entity":world});
                                if entity.script_id.is_none() {
                                    entity.script_id = self
                                        .document
                                        .as_ref()
                                        .and_then(|doc| {
                                            entity_at(&doc.scene.entities, &doc.selected)
                                        })
                                        .and_then(|selected| selected.script_id.clone());
                                }
                            }
                            Err(error) => {
                                self.inspector_error = Some(format!("{error:#}"));
                                continue;
                            }
                        }
                    }
                    if *kind == "core:rigid_body"
                        && let Some(collider) = entity
                            .components
                            .get_mut("core:collider")
                            .and_then(serde_json::Value::as_object_mut)
                        && collider.get("body").is_some_and(serde_json::Value::is_null)
                    {
                        collider.remove("body");
                        collider.remove("world");
                    }
                    entity.components.insert((*kind).into(), component);
                    changed = true;
                    ui.close_menu();
                }
            }
            ui.separator();
            let definitions: Vec<_> = self.world.component_types.values().cloned().collect();
            for definition in definitions {
                if !entity.components.contains_key(&definition.id)
                    && ui
                        .button(&definition.export)
                        .on_hover_text(&definition.id)
                        .clicked()
                {
                    entity
                        .components
                        .insert(definition.id.clone(), definition.defaults());
                    changed = true;
                    ui.close_menu();
                }
            }
            if ui
                .add_enabled(
                    self.live.is_none(),
                    egui::Button::new("Reload component types"),
                )
                .clicked()
            {
                let result = self.refresh_component_types();
                self.report(result);
                ui.close_menu();
            }
        });
        if self.live.is_some()
            && let Some(id) = self.selected_id()
        {
            for kind in ["core:rigid_body_state", "core:physics_status"] {
                if let Some(data) = self.world.get_component(&id, kind) {
                    egui::CollapsingHeader::new(format!("Runtime state · {kind}")).show(ui, |ui| {
                        ui.monospace(serde_json::to_string_pretty(&data).unwrap_or_default());
                    });
                }
            }
        }
        self.inspector_entity = Some(entity);
        self.inspector_dirty |= changed;
        if changed {
            let _ = self.commit_inspector();
        }
        if let Some(error) = &self.inspector_error {
            ui.add_space(8.0);
            ui.colored_label(egui::Color32::from_rgb(242, 142, 137), error);
        }
    }
}

fn json_tree(ui: &mut egui::Ui, value: &mut Value, depth: usize) -> bool {
    if depth > 10 {
        ui.weak("Deep data is preserved");
        return false;
    }
    let mut changed = false;
    match value {
        Value::Object(fields) => {
            for (key, item) in fields {
                ui.push_id(key, |ui| {
                    if item.is_object() || item.is_array() {
                        egui::CollapsingHeader::new(key).show(ui, |ui| {
                            changed |= json_tree(ui, item, depth + 1);
                        });
                    } else {
                        ui.horizontal(|ui| {
                            ui.label(key);
                            changed |= json_tree(ui, item, depth + 1);
                        });
                    }
                });
            }
        }
        Value::Array(items) => {
            for (index, item) in items.iter_mut().enumerate() {
                ui.push_id(index, |ui| {
                    ui.horizontal(|ui| {
                        ui.weak(index.to_string());
                        changed |= json_tree(ui, item, depth + 1);
                    });
                });
            }
        }
        Value::Bool(value) => changed |= ui.checkbox(value, "").changed(),
        Value::String(value) => {
            changed |= ui
                .add(egui::TextEdit::singleline(value).desired_width(140.0))
                .changed()
        }
        Value::Number(number) => {
            let mut data = number.as_f64().unwrap_or(0.0);
            if ui
                .add(egui::DragValue::new(&mut data).speed(0.05))
                .changed()
            {
                *value = json!(data);
                changed = true;
            }
        }
        Value::Null => {
            ui.weak("None");
        }
    }
    changed
}
