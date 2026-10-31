use super::{EntityId, World};
use anyhow::{Context, ensure};

impl World {
    pub fn enabled(&self, id: &EntityId) -> bool {
        self.is_alive(id) && !self.disabled.contains(&id.index)
    }
    pub fn is_active(&self, id: &EntityId) -> bool {
        if !self.is_alive(id) {
            return false;
        }
        let mut current = *id;
        for _ in 0..=self.allocator.len() {
            if !self.enabled(&current) {
                return false;
            }
            match self.get_parent(&current) {
                Some(parent) => current = parent,
                None => return true,
            }
        }
        false
    }
    pub fn set_enabled(&mut self, id: &EntityId, enabled: bool) -> bool {
        if !self.is_alive(id) {
            return false;
        }
        if enabled {
            self.disabled.remove(&id.index);
        } else {
            self.disabled.insert(id.index);
        }
        true
    }
    /// Clone authored components and hierarchy; derived native state is rebuilt, never copied.
    pub fn clone_entity(
        &mut self,
        source: EntityId,
        recursive: bool,
        enabled: Option<bool>,
    ) -> anyhow::Result<EntityId> {
        ensure!(self.is_alive(&source), "cannot clone a stale entity");
        let originals = if recursive {
            self.hierarchy([source])
        } else {
            vec![source]
        };
        let copies: Vec<_> = originals.iter().map(|_| self.create_entity()).collect();
        let mapping: std::collections::HashMap<_, _> = originals
            .iter()
            .zip(&copies)
            .map(|(a, b)| (a.to_string(), *b))
            .collect();
        let result = (|| -> anyhow::Result<()> {
            let mut kinds = vec![
                "core:transform",
                "core:camera",
                "core:mesh_renderer",
                "core:sprite_renderer",
                "core:text_renderer",
                "core:ambient_light",
                "core:directional_light",
                "core:point_light",
            ]
            .into_iter()
            .map(str::to_owned)
            .collect::<Vec<_>>();
            kinds.extend(
                self.custom
                    .keys()
                    .filter(|kind| !crate::physics::PhysicsRegistry::is_read_only(kind))
                    .cloned(),
            );
            for (original, copy) in originals.iter().zip(&copies) {
                if let Some(name) = self.names.get(&original.index).cloned() {
                    self.names.insert(copy.index, format!("{name} (Clone)"));
                }
                self.set_enabled(
                    copy,
                    if *original == source {
                        enabled.unwrap_or(self.enabled(original))
                    } else {
                        self.enabled(original)
                    },
                );
                if let Some(parent) = self.get_parent(original) {
                    let parent = mapping.get(&parent.to_string()).copied().unwrap_or(parent);
                    ensure!(
                        self.set_parent(*copy, Some(parent)),
                        "invalid cloned parent"
                    );
                }
                for kind in &kinds {
                    if let Some(mut data) = self.get_component(original, kind) {
                        super::resolve_references(&mut data, &mut |reference| {
                            mapping
                                .get(reference)
                                .copied()
                                .or_else(|| EntityId::parse(reference))
                        })?;
                        ensure!(
                            self.set_component(copy, kind, data),
                            "invalid cloned component {kind}"
                        );
                    }
                }
            }
            Ok(())
        })();
        if let Err(error) = result {
            for copy in &copies {
                self.destroy_entity(*copy);
            }
            return Err(error);
        }
        copies
            .first()
            .copied()
            .context("clone did not allocate an entity")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    #[test]
    fn cloning_remaps_subtree_references_and_enabled_state_without_copying_native_state() {
        let mut world = World::new();
        let external = world.create_entity();
        let parent = world.create_entity();
        let child = world.create_entity();
        world.set_parent(child, Some(parent));
        world.set_enabled(&parent, false);
        world.set_component(
            &parent,
            "demo:link",
            json!({"child":{"$entity":child.to_string()},"world":{"$entity":external.to_string()}}),
        );
        world
            .custom
            .entry("core:physics_status".into())
            .or_default()
            .insert(parent.index, json!({"body":{"active":true}}));
        let copy = world.clone_entity(parent, true, Some(true)).expect("clone");
        let copied_child = world.get_children(&copy)[0];
        assert_ne!(copy, parent);
        assert_ne!(copied_child, child);
        assert!(world.is_active(&copy));
        assert!(world.is_active(&copied_child));
        assert!(!world.is_active(&child));
        assert_eq!(
            world.get_component(&copy, "demo:link"),
            Some(
                json!({"child":{"$entity":copied_child.to_string()},"world":{"$entity":external.to_string()}})
            )
        );
        assert!(world.get_component(&copy, "core:physics_status").is_none());
        world.set_enabled(&copy, false);
        assert!(!world.is_active(&copied_child));
        assert!(world.query(&["demo:link"]).is_empty());
        world.destroy_entity(copy);
        let reused = world.create_entity();
        assert!(world.enabled(&reused));
        let count = world.allocator.alive_entity_ids().count();
        assert!(world.clone_entity(copy, true, None).is_err());
        assert_eq!(world.allocator.alive_entity_ids().count(), count);
    }
}
