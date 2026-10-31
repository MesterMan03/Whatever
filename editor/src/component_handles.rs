use super::*;
use engine::ecs::Transform;
use glam::Quat;
use serde_json::{Value, json};
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum ComponentHandle {
    Collider,
    JointA,
    JointB,
}
fn vector(value: &Value, default: [f32; 3]) -> Vec3 {
    value
        .as_array()
        .filter(|a| a.len() == 3)
        .map(|a| {
            Vec3::new(
                a[0].as_f64().unwrap_or(default[0] as f64) as f32,
                a[1].as_f64().unwrap_or(default[1] as f64) as f32,
                a[2].as_f64().unwrap_or(default[2] as f64) as f32,
            )
        })
        .unwrap_or(Vec3::from(default))
}
fn rotation(value: &Value) -> Quat {
    serde_json::from_value::<[f32; 4]>(value.clone())
        .map(Quat::from_array)
        .unwrap_or(Quat::IDENTITY)
}
impl Editor {
    fn reference_pose(&self, reference: &Value) -> Option<Transform> {
        let id = reference.get("$entity")?.as_str()?;
        let doc = self.document.as_ref()?;
        let mut paths = Vec::new();
        hierarchy::entity_paths(&doc.scene.entities, &mut Vec::new(), &mut paths);
        let path = paths.iter().find(|p| {
            entity_at(&doc.scene.entities, p).is_some_and(|e| e.script_id.as_deref() == Some(id))
        })?;
        crate::document::world_transform(&doc.scene, path).ok()
    }
    fn handle_data(&self) -> Option<(&Value, Transform)> {
        let kind = self.component_handle?;
        let doc = self.document.as_ref()?;
        let entity = entity_at(&doc.scene.entities, &doc.selected)?;
        let owner = crate::document::world_transform(&doc.scene, &doc.selected).ok()?;
        let data = entity
            .components
            .get(if kind == ComponentHandle::Collider {
                "core:collider"
            } else {
                "core:joint"
            })?;
        let basis = match kind {
            ComponentHandle::Collider => self.reference_pose(&data["body"]).unwrap_or(owner),
            ComponentHandle::JointB => self
                .reference_pose(&data["connectedBody"])
                .or_else(|| self.reference_pose(&data["bodyB"]))?,
            ComponentHandle::JointA => self.reference_pose(&data["bodyA"]).unwrap_or(owner),
        };
        Some((data, basis))
    }
    pub(super) fn edit_pose(&self) -> Option<Transform> {
        if let Some(kind) = self.component_handle {
            let (data, basis) = self.handle_data()?;
            let q = Quat::from_array(basis.rotation);
            let key = match kind {
                ComponentHandle::Collider => "position",
                ComponentHandle::JointA => "anchorA",
                ComponentHandle::JointB => "anchorB",
            };
            let local_rotation = if kind == ComponentHandle::Collider {
                rotation(&data["rotation"])
            } else if matches!(data["type"].as_str(), Some("revolute" | "prismatic")) {
                let key = if kind == ComponentHandle::JointA {
                    "axisA"
                } else {
                    "axisB"
                };
                Quat::from_rotation_arc(
                    Vec3::Y,
                    vector(&data[key], [0.0, 1.0, 0.0])
                        .normalize_or_zero()
                        .try_normalize()
                        .unwrap_or(Vec3::Y),
                )
            } else {
                rotation(
                    &data[if kind == ComponentHandle::JointA {
                        "rotationA"
                    } else {
                        "rotationB"
                    }],
                )
            };
            let shape = &data["shape"];
            let radius = shape["radius"].as_f64().unwrap_or(0.5) as f32;
            let scale = if kind != ComponentHandle::Collider {
                Vec3::ONE
            } else {
                match shape["type"].as_str().unwrap_or("box") {
                    "box" => vector(&shape["size"], [1.0; 3]),
                    "sphere" => Vec3::splat(2.0 * radius),
                    _ => Vec3::new(
                        2.0 * radius,
                        shape["height"].as_f64().unwrap_or(2.0) as f32,
                        2.0 * radius,
                    ),
                }
            };
            return Some(Transform {
                position: (Vec3::from(basis.position) + q * vector(&data[key], [0.0; 3]))
                    .to_array(),
                rotation: (q * local_rotation).to_array(),
                scale: scale.to_array(),
            });
        }
        let doc = self.document.as_ref()?;
        crate::document::world_transform(&doc.scene, &doc.selected).ok()
    }
    pub(super) fn apply_handle_pose(
        &mut self,
        pose: &Transform,
        checkpoint: bool,
    ) -> anyhow::Result<()> {
        let kind = self.component_handle.context("component handle")?;
        let (_, basis) = self
            .handle_data()
            .context("component target is no longer available")?;
        let q = Quat::from_array(basis.rotation);
        let doc = self.document.as_mut().context("scene")?;
        if checkpoint {
            doc.checkpoint();
        }
        let entity = entity_mut(&mut doc.scene.entities, &doc.selected).context("entity")?;
        let data = entity
            .components
            .get_mut(if kind == ComponentHandle::Collider {
                "core:collider"
            } else {
                "core:joint"
            })
            .context("component")?;
        match self.tool {
            viewport::Tool::Move => {
                let key = match kind {
                    ComponentHandle::Collider => "position",
                    ComponentHandle::JointA => "anchorA",
                    ComponentHandle::JointB => "anchorB",
                };
                data[key] = json!(
                    (q.inverse() * (Vec3::from(pose.position) - Vec3::from(basis.position)))
                        .to_array()
                );
            }
            viewport::Tool::Rotate => {
                if kind == ComponentHandle::Collider {
                    data["rotation"] = json!(
                        (q.inverse() * Quat::from_array(pose.rotation))
                            .normalize()
                            .to_array()
                    );
                } else if matches!(data["type"].as_str(), Some("revolute" | "prismatic")) {
                    let key = if kind == ComponentHandle::JointA {
                        "axisA"
                    } else {
                        "axisB"
                    };
                    data[key] = json!(
                        (q.inverse() * Quat::from_array(pose.rotation) * Vec3::Y)
                            .normalize()
                            .to_array()
                    );
                } else {
                    data[if kind == ComponentHandle::JointA {
                        "rotationA"
                    } else {
                        "rotationB"
                    }] = json!(
                        (q.inverse() * Quat::from_array(pose.rotation))
                            .normalize()
                            .to_array()
                    );
                }
            }
            viewport::Tool::Scale if kind == ComponentHandle::Collider => {
                let size = Vec3::from(pose.scale).abs().max(Vec3::splat(0.001));
                let shape = &mut data["shape"];
                match shape["type"].as_str().unwrap_or("box") {
                    "box" => shape["size"] = json!(size.to_array()),
                    "sphere" => {
                        let axis = self.gizmo_drag.as_ref().map(|d| d.axis).unwrap_or(0).min(2);
                        shape["radius"] = json!(size[axis] * 0.5);
                    }
                    _ => {
                        let axis = self.gizmo_drag.as_ref().map(|d| d.axis).unwrap_or(0).min(2);
                        let radius = if axis == 1 {
                            shape["radius"].as_f64().unwrap_or(0.5) as f32
                        } else {
                            size[axis] * 0.5
                        };
                        let height = if shape["type"].as_str() == Some("capsule") {
                            size.y.max(radius * 2.0)
                        } else {
                            size.y
                        };
                        shape["radius"] = json!(radius);
                        shape["height"] = json!(height);
                    }
                }
            }
            _ => {}
        }
        self.sync_selection();
        self.preview_dirty = true;
        Ok(())
    }
    pub(super) fn draw_component_handle(&self, ui: &egui::Ui) {
        let Some(pose) = self.component_handle.and_then(|_| self.edit_pose()) else {
            return;
        };
        let center = Vec3::from(pose.position);
        let q = Quat::from_array(pose.rotation);
        let color = egui::Color32::from_rgb(86, 229, 221);
        let stroke = egui::Stroke::new(1.8_f32, color);
        let line = |a: Vec3, b: Vec3| {
            if let (Some(a), Some(b)) = (self.screen_point(a), self.screen_point(b)) {
                ui.painter().line_segment([a, b], stroke);
            }
        };
        if self.component_handle == Some(ComponentHandle::Collider) {
            let data = self.handle_data().map(|v| v.0);
            let shape = data
                .and_then(|d| d["shape"]["type"].as_str())
                .unwrap_or("box");
            let half = Vec3::from(pose.scale) * 0.5;
            if shape == "box" {
                let corners: Vec<_> = (0..8)
                    .map(|i| {
                        center
                            + q * (Vec3::new(
                                if i & 1 == 0 { -1.0 } else { 1.0 },
                                if i & 2 == 0 { -1.0 } else { 1.0 },
                                if i & 4 == 0 { -1.0 } else { 1.0 },
                            ) * half)
                    })
                    .collect();
                for i in 0..8 {
                    for bit in [1, 2, 4] {
                        if i & bit == 0 {
                            line(corners[i], corners[i | bit]);
                        }
                    }
                }
            } else if matches!(
                data.and_then(|d| d.get("shape"))
                    .and_then(|s| s.get("type"))
                    .and_then(Value::as_str),
                Some("cylinder" | "capsule" | "cone")
            ) {
                let shape = data
                    .and_then(|d| d.get("shape"))
                    .and_then(|s| s.get("type"))
                    .and_then(Value::as_str)
                    .unwrap_or("cylinder");
                let radius = half.x;
                let segment = if shape == "capsule" {
                    (half.y - radius).max(0.0)
                } else {
                    half.y
                };
                for sign in [-1.0, 1.0] {
                    let r = if shape == "cone" && sign > 0.0 {
                        0.0
                    } else {
                        radius
                    };
                    for i in 0..64 {
                        let a = i as f32 * std::f32::consts::TAU / 64.0;
                        let b = (i + 1) as f32 * std::f32::consts::TAU / 64.0;
                        line(
                            center + q * Vec3::new(a.cos() * r, sign * segment, a.sin() * r),
                            center + q * Vec3::new(b.cos() * r, sign * segment, b.sin() * r),
                        );
                    }
                }
                for axis in [Vec3::X, Vec3::Z] {
                    for sign in [-1.0, 1.0] {
                        let radial = axis * radius * sign;
                        let top = if shape == "cone" {
                            Vec3::Y * segment
                        } else {
                            radial + Vec3::Y * segment
                        };
                        line(center + q * (radial - Vec3::Y * segment), center + q * top);
                    }
                    if shape == "capsule" {
                        for sign in [-1.0, 1.0] {
                            for i in 0..32 {
                                let a = i as f32 * std::f32::consts::PI / 32.0;
                                let b = (i + 1) as f32 * std::f32::consts::PI / 32.0;
                                line(
                                    center
                                        + q * (axis * a.cos() * radius
                                            + Vec3::Y * sign * (segment + a.sin() * radius)),
                                    center
                                        + q * (axis * b.cos() * radius
                                            + Vec3::Y * sign * (segment + b.sin() * radius)),
                                );
                            }
                        }
                    }
                }
            } else {
                for axis in 0..3 {
                    let mut previous = None;
                    for i in 0..=64 {
                        let angle = i as f32 * std::f32::consts::TAU / 64.0;
                        let mut point = Vec3::ZERO;
                        point[(axis + 1) % 3] = angle.cos() * half[(axis + 1) % 3];
                        point[(axis + 2) % 3] = angle.sin() * half[(axis + 2) % 3];
                        let point = center + q * point;
                        if let Some(old) = previous {
                            line(old, point);
                        }
                        previous = Some(point);
                    }
                }
            }
        } else {
            for axis in [Vec3::X, Vec3::Y, Vec3::Z] {
                line(center - axis * 0.18, center + axis * 0.18);
            }
            line(center - q * Vec3::Y * 0.7, center + q * Vec3::Y * 0.7);
        }
        if let Some(point) = self.screen_point(center) {
            ui.painter().circle_stroke(point, 8.0, stroke);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn component_handles_edit_local_physics_data_without_moving_the_entity() {
        let path = std::env::temp_dir().join(format!(
            "whatever-handles-{}-{}.wscene",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("time")
                .as_nanos()
        ));
        let parent_rotation = Quat::from_rotation_y(std::f32::consts::FRAC_PI_2);
        std::fs::write(&path,serde_json::to_vec(&json!({"version":1,"entities":[{"script_id":"parent","components":{"core:transform":{"position":[4,5,6],"rotation":parent_rotation.to_array(),"scale":[3,3,3]}},"children":[{"script_id":"body","components":{"core:transform":{"position":[1,0,0],"rotation":[0,0,0,1],"scale":[1,1,1]},"core:collider":{"shape":{"type":"box","size":[2,4,6]},"position":[1,0,0]},"core:joint":{"type":"fixed","connectedBody":{"$entity":"target"},"anchorB":[-1,0,0]}}}]},{"script_id":"target","components":{"core:transform":{"position":[20,0,0],"rotation":[0,0,0,1],"scale":[1,1,1]}}}]})).expect("serialize")).expect("fixture");
        let mut editor = Editor::new(None, None);
        editor.document = Some(Document::open(&path).expect("open"));
        std::fs::remove_file(&path).expect("cleanup");
        editor.document.as_mut().expect("document").selected = vec![0, 0];
        editor.sync_selection();
        editor.component_handle = Some(ComponentHandle::Collider);
        editor.tool = viewport::Tool::Move;
        let original = editor.document.as_ref().expect("doc").scene.entities[0].children[0]
            .components["core:transform"]
            .clone();
        let mut pose = editor.edit_pose().expect("collider");
        assert!(Vec3::from(pose.position).distance(Vec3::new(4.0, 5.0, 2.0)) < 1e-5);
        pose.position[0] += 2.0;
        editor.apply_handle_pose(&pose, true).expect("move");
        let data = &editor.document.as_ref().expect("doc").scene.entities[0].children[0].components
            ["core:collider"];
        assert!(vector(&data["position"], [0.0; 3]).distance(Vec3::new(1.0, 0.0, 2.0)) < 1e-5);
        editor.document.as_mut().expect("doc").undo();
        assert_eq!(editor.document.as_ref().expect("doc").selected, vec![0, 0]);
        editor.tool = viewport::Tool::Scale;
        pose.scale = [4.0, 6.0, 8.0];
        editor.apply_handle_pose(&pose, true).expect("reshape");
        assert_eq!(
            editor.document.as_ref().expect("doc").scene.entities[0].children[0].components["core:collider"]
                ["shape"]["size"],
            json!([4.0, 6.0, 8.0])
        );
        editor.component_handle = Some(ComponentHandle::JointB);
        editor.tool = viewport::Tool::Move;
        let mut pose = editor.edit_pose().expect("connected anchor");
        assert!(Vec3::from(pose.position).distance(Vec3::new(19.0, 0.0, 0.0)) < 1e-5);
        pose.position[1] = 2.0;
        editor.apply_handle_pose(&pose, true).expect("anchor");
        editor.tool = viewport::Tool::Rotate;
        pose.rotation = Quat::from_rotation_x(0.5).to_array();
        editor.apply_handle_pose(&pose, true).expect("frame");
        let data = &editor.document.as_ref().expect("doc").scene.entities[0].children[0].components
            ["core:joint"];
        assert_eq!(data["anchorB"], json!([-1.0, 2.0, 0.0]));
        assert!(rotation(&data["rotationB"]).abs_diff_eq(Quat::from_rotation_x(0.5), 1e-5));
        assert_eq!(
            editor.document.as_ref().expect("doc").scene.entities[0].children[0].components["core:transform"],
            original
        );
    }
}
