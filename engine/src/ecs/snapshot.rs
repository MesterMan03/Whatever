use super::{EntityId, World};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Default)]
pub(crate) struct SceneChanges {
    pub created: BTreeSet<EntityId>,
    pub removed: BTreeSet<EntityId>,
    pub metadata: BTreeSet<EntityId>,
    pub components: BTreeMap<EntityId, BTreeSet<String>>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct EntityMetadata {
    pub name: String,
    pub enabled: bool,
    pub active: bool,
    pub parent: Option<String>,
    pub children: Vec<String>,
    pub tags: Vec<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct EntitySnapshot {
    pub id: String,
    pub metadata: EntityMetadata,
    pub components: BTreeMap<String, Value>,
    pub instances: BTreeMap<String, String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct EntityDelta {
    pub id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metadata: Option<EntityMetadata>,
    pub components: BTreeMap<String, Option<Value>>,
    pub instances: BTreeMap<String, Option<String>>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SceneDelta {
    pub revision: u64,
    pub base_revision: u64,
    pub tick_number: u64,
    pub reset: bool,
    pub camera: Option<String>,
    pub created: Vec<EntitySnapshot>,
    pub removed: Vec<String>,
    pub changed: Vec<EntityDelta>,
}

#[derive(Default)]
pub struct ScenePublisher {
    pub revision: u64,
    entities: BTreeMap<String, EntitySnapshot>,
}

fn metadata(world: &World, id: EntityId) -> EntityMetadata {
    EntityMetadata {
        name: world.names.get(&id.index).cloned().unwrap_or_default(),
        enabled: world.enabled(&id),
        active: world.is_active(&id),
        parent: world.get_parent(&id).map(|id| id.to_string()),
        children: world
            .get_children(&id)
            .iter()
            .map(ToString::to_string)
            .collect(),
        tags: world.tags.get(&id.index).cloned().unwrap_or_default(),
    }
}

fn kinds(world: &World) -> BTreeSet<String> {
    let mut kinds: BTreeSet<_> = [
        "core:transform",
        "core:world_transform",
        "core:sprite_renderer",
        "core:text_renderer",
        "core:mesh_renderer",
        "core:camera",
        "core:ambient_light",
        "core:directional_light",
        "core:point_light",
        "core:rigid_body_state",
    ]
    .into_iter()
    .map(str::to_owned)
    .collect();
    kinds.extend(world.custom.keys().cloned());
    kinds
}

fn instance(world: &World, id: EntityId, kind: &str) -> Option<String> {
    world
        .component_instances
        .get(kind)?
        .get(&id.index)
        .map(ToString::to_string)
}

impl ScenePublisher {
    pub fn reset(&mut self, world: &mut World, tick: u64, camera: Option<EntityId>) -> SceneDelta {
        world.scene_changes = SceneChanges::default();
        world
            .scene_changes
            .created
            .extend(world.allocator.alive_entity_ids());
        self.entities.clear();
        let mut delta = self.publish(world, tick, camera);
        delta.reset = true;
        delta.base_revision = 0;
        delta
    }

    /// Consume mutation journals. Only dirty component values are materialized/compared.
    pub fn publish(
        &mut self,
        world: &mut World,
        tick: u64,
        camera: Option<EntityId>,
    ) -> SceneDelta {
        let dirty = std::mem::take(&mut world.scene_changes);
        let mut delta = SceneDelta {
            revision: self.revision + 1,
            base_revision: self.revision,
            tick_number: tick,
            reset: false,
            camera: camera.map(|id| id.to_string()),
            created: vec![],
            removed: vec![],
            changed: vec![],
        };
        let all_kinds = kinds(world);
        for id in dirty.removed {
            if self.entities.remove(&id.to_string()).is_some() {
                delta.removed.push(id.to_string());
            }
        }
        for id in &dirty.created {
            if !world.is_alive(id) {
                continue;
            }
            let components = all_kinds
                .iter()
                .filter_map(|kind| {
                    world
                        .get_component(id, kind)
                        .map(|data| (kind.clone(), data))
                })
                .collect();
            let instances = world
                .component_instances
                .iter()
                .filter_map(|(kind, ids)| {
                    ids.get(&id.index)
                        .map(|value| (kind.clone(), value.to_string()))
                })
                .collect();
            let entity = EntitySnapshot {
                id: id.to_string(),
                metadata: metadata(world, *id),
                components,
                instances,
            };
            delta.created.push(entity.clone());
            self.entities.insert(entity.id.clone(), entity);
        }
        let touched: BTreeSet<_> = dirty
            .metadata
            .iter()
            .chain(dirty.components.keys())
            .copied()
            .collect();
        for id in touched {
            if dirty.created.contains(&id) || !world.is_alive(&id) {
                continue;
            }
            let Some(previous) = self.entities.get_mut(&id.to_string()) else {
                continue;
            };
            let mut change = EntityDelta {
                id: id.to_string(),
                metadata: None,
                components: BTreeMap::new(),
                instances: BTreeMap::new(),
            };
            if dirty.metadata.contains(&id) {
                let current = metadata(world, id);
                if current != previous.metadata {
                    previous.metadata = current.clone();
                    change.metadata = Some(current);
                }
            }
            for kind in dirty.components.get(&id).into_iter().flatten() {
                let data = world.get_component(&id, kind);
                let current_instance = instance(world, id, kind);
                if previous.components.get(kind) != data.as_ref()
                    || previous.instances.get(kind) != current_instance.as_ref()
                {
                    if let Some(data) = &data {
                        previous.components.insert(kind.clone(), data.clone());
                    } else {
                        previous.components.remove(kind);
                    }
                    change.components.insert(kind.clone(), data);
                    if let Some(value) = &current_instance {
                        previous.instances.insert(kind.clone(), value.clone());
                    } else {
                        previous.instances.remove(kind);
                    }
                    change.instances.insert(kind.clone(), current_instance);
                }
            }
            if change.metadata.is_some() || !change.components.is_empty() {
                delta.changed.push(change);
            }
        }
        self.revision = delta.revision;
        delta
    }

    pub fn entity_count(&self) -> usize {
        self.entities.len()
    }
}

impl World {
    pub fn mark_metadata(&mut self, id: EntityId) {
        self.scene_changes.metadata.insert(id);
    }
    pub fn mark_component(&mut self, id: EntityId, kind: &str) {
        self.scene_changes
            .components
            .entry(id)
            .or_default()
            .insert(kind.to_owned());
    }
    pub fn mark_hierarchy(&mut self, id: EntityId) {
        for child in self.hierarchy([id]) {
            self.mark_metadata(child);
            self.mark_component(child, "core:world_transform");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn journals_publish_only_changes_and_preserve_generations() {
        let mut world = World::new();
        let entity = world.create_entity();
        world.set_component(
            &entity,
            "core:transform",
            json!({"position":[0,0,0],"rotation":[0,0,0,1],"scale":[1,1,1]}),
        );
        let mut publisher = ScenePublisher::default();
        assert_eq!(publisher.reset(&mut world, 0, None).created.len(), 1);
        assert!(publisher.publish(&mut world, 1, None).changed.is_empty());
        world.add_tag(&entity, "enemy".into()).expect("tag");
        let change = publisher.publish(&mut world, 2, None);
        assert_eq!(change.changed.len(), 1);
        assert!(change.changed[0].components.is_empty());
        world.destroy_entity(entity);
        let replacement = world.create_entity();
        let change = publisher.publish(&mut world, 3, None);
        assert_eq!(change.removed, vec![entity.to_string()]);
        assert_eq!(change.created[0].id, replacement.to_string());
    }
}
