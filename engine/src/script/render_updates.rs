use crate::ecs::{EntityId, World};

#[derive(Debug)]
pub enum RenderCommand {
    UpsertSprite { entity_idx: u32 },
    RemoveSprite { entity_idx: u32 },
    UpsertText { entity_idx: u32 },
    RemoveText { entity_idx: u32 },
    UpsertMesh { entity_idx: u32 },
    RemoveMesh { entity_idx: u32 },
}

impl RenderCommand {
    pub fn key(&self) -> (u8, u32) {
        match *self {
            Self::UpsertSprite { entity_idx } | Self::RemoveSprite { entity_idx } => {
                (0, entity_idx)
            }
            Self::UpsertText { entity_idx } | Self::RemoveText { entity_idx } => (1, entity_idx),
            Self::UpsertMesh { entity_idx } | Self::RemoveMesh { entity_idx } => (2, entity_idx),
        }
    }

    /// Geometry is baked in world space; every rendered descendant needs refreshing.
    pub fn upsert_hierarchy(world: &World, roots: impl IntoIterator<Item = EntityId>) -> Vec<Self> {
        let mut commands = Vec::new();
        for id in world.hierarchy(roots) {
            if world.transforms.contains_key(&id.index) {
                Self::entity(world, id, !world.is_active(&id), &mut commands);
            }
        }
        commands
    }

    /// Capture before deleting ECS entities, while their renderer components still exist.
    pub fn remove_hierarchy(world: &World, roots: impl IntoIterator<Item = EntityId>) -> Vec<Self> {
        let mut commands = Vec::new();
        for id in world.hierarchy(roots) {
            Self::entity(world, id, true, &mut commands);
        }
        commands
    }

    pub fn remove_entity(world: &World, id: EntityId) -> Vec<Self> {
        let mut commands = Vec::new();
        Self::entity(world, id, true, &mut commands);
        commands
    }

    fn entity(world: &World, id: EntityId, remove: bool, commands: &mut Vec<Self>) {
        let entity_idx = id.index;
        if world.sprite_renderers.contains_key(&entity_idx) {
            commands.push(if remove {
                Self::RemoveSprite { entity_idx }
            } else {
                Self::UpsertSprite { entity_idx }
            });
        }
        if world.text_renderers.contains_key(&entity_idx) {
            commands.push(if remove {
                Self::RemoveText { entity_idx }
            } else {
                Self::UpsertText { entity_idx }
            });
        }
        if world.mesh_renderers.contains_key(&entity_idx) {
            commands.push(if remove {
                Self::RemoveMesh { entity_idx }
            } else {
                Self::UpsertMesh { entity_idx }
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    #[test]
    fn parent_updates_and_deletion_cover_all_rendered_descendants_once() {
        let mut world = World::new();
        let parent = world.create_entity();
        let child = world.create_entity();
        let grandchild = world.create_entity();
        let other = world.create_entity();
        world.set_parent(child, Some(parent));
        world.set_parent(grandchild, Some(child));
        for id in [child, grandchild, other] {
            world.set_component(
                &id,
                "core:transform",
                json!({"position":[0,0,0],"rotation":[0,0,0,1],"scale":[1,1,1]}),
            );
            world.set_component(
                &id,
                "core:mesh_renderer",
                json!({"mesh":"demo://cube.obj","shader":"core://shaders/mesh_lit.wgsl"}),
            );
        }
        let commands = RenderCommand::upsert_hierarchy(&world, [parent, grandchild]);
        assert_eq!(
            commands.iter().map(RenderCommand::key).collect::<Vec<_>>(),
            vec![(2, child.index), (2, grandchild.index)]
        );
        let removals = RenderCommand::remove_hierarchy(&world, [parent]);
        assert_eq!(removals.len(), 2);
        assert!(
            removals
                .iter()
                .all(|command| matches!(command, RenderCommand::RemoveMesh { .. }))
        );
        world.destroy_entity(parent);
        assert!(RenderCommand::upsert_hierarchy(&world, [parent]).is_empty());
        assert!(world.is_alive(&other));
    }
}
