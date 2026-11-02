use super::*;
use serde_json::json;

impl Editor {
    pub(super) fn apply_preset(&mut self, action: Action) -> anyhow::Result<()> {
        let preset_mesh = match action {
            Action::Cube => Some("cube"),
            Action::Sphere => Some("sphere"),
            Action::Bean => Some("bean"),
            _ => None,
        };
        let imported = if let Some(mesh) = preset_mesh {
            let owner = self.scene_owner(&self.document.as_ref().context("open a scene")?.path)?;
            let root = &self.mod_roots[owner].1;
            Some((
                engine::project::import_starter(root, &format!("meshes/{mesh}.obj"))?,
                engine::project::import_starter(root, "shaders/mesh_lit.wgsl")?,
            ))
        } else {
            None
        };
        let physics_world = if matches!(action, Action::Cube | Action::Sphere | Action::Bean) {
            Some(self.scene_physics_world(false)?)
        } else {
            None
        };
        let doc = self.document.as_mut().context("open a scene")?;
        let mut paths = Vec::new();
        hierarchy::entity_paths(&doc.scene.entities, &mut Vec::new(), &mut paths);
        let existing = paths
            .iter()
            .filter_map(|path| {
                entity_at(&doc.scene.entities, path).and_then(|e| e.script_id.clone())
            })
            .collect::<std::collections::HashSet<_>>();
        let (name, _kind) = match action {
            Action::Cube => ("Cube", Some("cube")),
            Action::Sphere => ("Sphere", Some("sphere")),
            Action::Bean => ("Bean", Some("bean")),
            Action::Camera => ("Camera", None),
            Action::AmbientLight => ("Ambient Light", None),
            Action::DirectionalLight => ("Directional Light", None),
            Action::PointLight => ("Point Light", None),
            _ => ("Entity", None),
        };
        let base = name.to_ascii_lowercase().replace(' ', "_");
        let mut id = base.clone();
        let mut suffix = 2;
        while existing.contains(&id) {
            id = format!("{base}_{suffix}");
            suffix += 1;
        }
        let entity =
            entity_mut(&mut doc.scene.entities, &doc.selected).context("created entity")?;
        entity.name = name.into();
        entity.script_id = Some(id);
        entity.components.insert(
            "core:transform".into(),
            inspector::default_component("core:transform"),
        );
        if let Some((mesh, shader)) = imported {
            entity.components.insert(
                "core:mesh_renderer".into(),
                json!({"mesh":mesh,"shader":shader}),
            );
        }
        if let Some(world) = physics_world {
            let shape = match action {
                Action::Sphere => json!({"type":"sphere","radius":1.0}),
                Action::Bean => json!({"type":"capsule","radius":0.42,"height":2.0}),
                _ => json!({"type":"box","size":[1,1,1]}),
            };
            entity.components.insert(
                "core:collider".into(),
                json!({"body":null,"world":{"$entity":world},"shape":shape,"friction":0.5}),
            );
        }
        let component = match action {
            Action::Camera => Some("core:camera"),
            Action::AmbientLight => Some("core:ambient_light"),
            Action::DirectionalLight => Some("core:directional_light"),
            Action::PointLight => Some("core:point_light"),
            _ => None,
        };
        if let Some(component) = component {
            entity
                .components
                .insert(component.into(), inspector::default_component(component));
        }
        self.preview_dirty = true;
        if preset_mesh.is_some() {
            self.refresh_file_inventory()?;
        }
        Ok(())
    }

    pub(super) fn new_component(&mut self, kind: &str) -> anyhow::Result<serde_json::Value> {
        let mut value = inspector::default_component(kind);
        let assets: &[(&str, &str)] = match kind {
            "core:mesh_renderer" => &[("shader", "shaders/mesh_lit.wgsl")],
            "core:sprite_renderer" => &[("shader", "shaders/sprite.wgsl")],
            "core:text_renderer" => &[
                ("shader", "shaders/sprite.wgsl"),
                ("font", "fonts/default.ttf"),
            ],
            _ => &[],
        };
        if !assets.is_empty() {
            let owner = self.scene_owner(&self.document.as_ref().context("scene")?.path)?;
            for (field, path) in assets {
                value[*field] = serde_json::json!(engine::project::import_starter(
                    &self.mod_roots[owner].1,
                    path
                )?);
            }
            self.refresh_file_inventory()?;
        }
        Ok(value)
    }
}
