use super::components::{
    COMPONENT_SPRITE_RENDERER, COMPONENT_TEXT_RENDERER, COMPONENT_TRANSFORM, SpriteRenderer,
    TextRenderer, Transform,
};
use super::entity::{EntityAllocator, EntityId};
use std::collections::HashMap;

pub struct World {
    pub allocator: EntityAllocator,
    pub transforms: HashMap<u32, Transform>,
    pub sprite_renderers: HashMap<u32, SpriteRenderer>,
    pub text_renderers: HashMap<u32, TextRenderer>,
    /// `type_id` → (`entity_index` → JSON blob)
    pub custom: HashMap<String, HashMap<u32, serde_json::Value>>,
}

impl World {
    pub fn new() -> Self {
        World {
            allocator: EntityAllocator::new(),
            transforms: HashMap::new(),
            sprite_renderers: HashMap::new(),
            text_renderers: HashMap::new(),
            custom: HashMap::new(),
        }
    }

    pub fn create_entity(&mut self) -> EntityId {
        self.allocator.alloc()
    }

    /// Returns false if `id` is stale.
    pub fn destroy_entity(&mut self, id: EntityId) -> bool {
        let idx = id.index;
        if !self.allocator.free(id) {
            return false;
        }
        self.transforms.remove(&idx);
        self.sprite_renderers.remove(&idx);
        self.text_renderers.remove(&idx);
        for type_map in self.custom.values_mut() {
            type_map.remove(&idx);
        }
        true
    }

    pub fn is_alive(&self, id: &EntityId) -> bool {
        self.allocator.is_alive(id)
    }

    /// Set a component on an entity. Returns false and logs a warning on failure
    /// (stale entity, or invalid JSON shape for built-in types).
    pub fn set_component(&mut self, id: &EntityId, type_id: &str, data: serde_json::Value) -> bool {
        if !self.allocator.is_alive(id) {
            tracing::warn!(entity_id = %id, type_id, "ComponentSet on stale entity");
            return false;
        }
        let idx = id.index;
        match type_id {
            COMPONENT_TRANSFORM => match serde_json::from_value::<Transform>(data) {
                Ok(t) => {
                    self.transforms.insert(idx, t);
                }
                Err(e) => {
                    tracing::warn!("invalid core:transform data: {e}");
                    return false;
                }
            },
            COMPONENT_SPRITE_RENDERER => match serde_json::from_value::<SpriteRenderer>(data) {
                Ok(s) => {
                    self.sprite_renderers.insert(idx, s);
                }
                Err(e) => {
                    tracing::warn!("invalid core:sprite_renderer data: {e}");
                    return false;
                }
            },
            COMPONENT_TEXT_RENDERER => match serde_json::from_value::<TextRenderer>(data) {
                Ok(t) => {
                    self.text_renderers.insert(idx, t);
                }
                Err(e) => {
                    tracing::warn!("invalid core:text_renderer data: {e}");
                    return false;
                }
            },
            _ => {
                self.custom
                    .entry(type_id.to_owned())
                    .or_default()
                    .insert(idx, data);
            }
        }
        true
    }

    /// Remove a component. Returns false if the entity is stale or had no such component.
    pub fn remove_component(&mut self, id: &EntityId, type_id: &str) -> bool {
        if !self.allocator.is_alive(id) {
            return false;
        }
        let idx = id.index;
        match type_id {
            COMPONENT_TRANSFORM => self.transforms.remove(&idx).is_some(),
            COMPONENT_SPRITE_RENDERER => self.sprite_renderers.remove(&idx).is_some(),
            COMPONENT_TEXT_RENDERER => self.text_renderers.remove(&idx).is_some(),
            _ => self
                .custom
                .get_mut(type_id)
                .and_then(|m| m.remove(&idx))
                .is_some(),
        }
    }

    /// Get a component value. Returns None if entity is stale or has no such component.
    pub fn get_component(&self, id: &EntityId, type_id: &str) -> Option<serde_json::Value> {
        if !self.allocator.is_alive(id) {
            return None;
        }
        let idx = id.index;
        match type_id {
            COMPONENT_TRANSFORM => self
                .transforms
                .get(&idx)
                .and_then(|t| serde_json::to_value(t).ok()),
            COMPONENT_SPRITE_RENDERER => self
                .sprite_renderers
                .get(&idx)
                .and_then(|s| serde_json::to_value(s).ok()),
            COMPONENT_TEXT_RENDERER => self
                .text_renderers
                .get(&idx)
                .and_then(|t| serde_json::to_value(t).ok()),
            _ => self.custom.get(type_id)?.get(&idx).cloned(),
        }
    }

    /// Return all alive entities that have every component in `type_ids`,
    /// together with the values for those components.
    pub fn query(&self, type_ids: &[&str]) -> Vec<(EntityId, HashMap<String, serde_json::Value>)> {
        if type_ids.is_empty() {
            return Vec::new();
        }
        self.allocator
            .alive_entity_ids()
            .filter_map(|id| {
                let mut components = HashMap::new();
                for &type_id in type_ids {
                    match self.get_component(&id, type_id) {
                        Some(v) => {
                            components.insert(type_id.to_owned(), v);
                        }
                        None => return None,
                    }
                }
                Some((id, components))
            })
            .collect()
    }
}
