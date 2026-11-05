use super::*;

impl Editor {
    // Native component helpers only. Object templates are discovered from JSON.
    pub(super) fn apply_preset(&mut self, action: Action) -> anyhow::Result<()> {
        let (name, component) = match action {
            Action::Camera => ("Camera", Some("core:camera")),
            Action::AmbientLight => ("Ambient Light", Some("core:ambient_light")),
            Action::DirectionalLight => ("Directional Light", Some("core:directional_light")),
            Action::PointLight => ("Point Light", Some("core:point_light")),
            _ => ("Entity", None),
        };
        let doc = self.document.as_mut().context("open a scene")?;
        let entity =
            entity_mut(&mut doc.scene.entities, &doc.selected).context("created entity")?;
        entity.name = name.into();
        entity.components.insert(
            "core:transform".into(),
            inspector::default_component("core:transform"),
        );
        if let Some(component) = component {
            entity
                .components
                .insert(component.into(), inspector::default_component(component));
        }
        self.preview_dirty = true;
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
            self.refresh_resources()?;
        }
        Ok(value)
    }
}
