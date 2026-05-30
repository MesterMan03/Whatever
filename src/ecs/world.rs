use super::components::{
    COMPONENT_AMBIENT_LIGHT, COMPONENT_CAMERA, COMPONENT_DIRECTIONAL_LIGHT,
    COMPONENT_MESH_RENDERER, COMPONENT_POINT_LIGHT, COMPONENT_SPRITE_RENDERER,
    COMPONENT_TEXT_RENDERER, COMPONENT_TRANSFORM, AmbientLight, CameraComponent, DirectionalLight,
    MeshRenderer, PointLight, SpriteRenderer, TextRenderer, Transform,
};
use super::entity::{EntityAllocator, EntityId};
use glam::{Quat, Vec3};
use std::collections::HashMap;

pub struct World {
    pub allocator: EntityAllocator,
    pub transforms: HashMap<u32, Transform>,
    pub sprite_renderers: HashMap<u32, SpriteRenderer>,
    pub text_renderers: HashMap<u32, TextRenderer>,
    pub camera_components: HashMap<u32, CameraComponent>,
    pub mesh_renderers: HashMap<u32, MeshRenderer>,
    pub ambient_lights: HashMap<u32, AmbientLight>,
    pub directional_lights: HashMap<u32, DirectionalLight>,
    pub point_lights: HashMap<u32, PointLight>,
    /// `type_id` → (`entity_index` → JSON blob)
    pub custom: HashMap<String, HashMap<u32, serde_json::Value>>,
    /// child index → parent `EntityId`
    pub parents: HashMap<u32, EntityId>,
    /// parent index → child indices
    pub children: HashMap<u32, Vec<u32>>,
}

impl World {
    pub fn new() -> Self {
        World {
            allocator: EntityAllocator::new(),
            transforms: HashMap::new(),
            sprite_renderers: HashMap::new(),
            text_renderers: HashMap::new(),
            camera_components: HashMap::new(),
            mesh_renderers: HashMap::new(),
            ambient_lights: HashMap::new(),
            directional_lights: HashMap::new(),
            point_lights: HashMap::new(),
            custom: HashMap::new(),
            parents: HashMap::new(),
            children: HashMap::new(),
        }
    }

    pub fn create_entity(&mut self) -> EntityId {
        self.allocator.alloc()
    }

    /// Set (or clear) the parent of `child_id`.
    ///
    /// - Pass `Some(parent_id)` to attach; pass `None` to detach.
    /// - Returns `false` if either entity is stale or if the operation would
    ///   create a cycle (parent is a descendant of child).
    pub fn set_parent(&mut self, child_id: EntityId, parent_id: Option<EntityId>) -> bool {
        if !self.allocator.is_alive(&child_id) {
            tracing::warn!(entity_id = %child_id, "set_parent: child entity is stale");
            return false;
        }

        // Detach from current parent first.
        if let Some(old_parent) = self.parents.remove(&child_id.index) {
            if let Some(siblings) = self.children.get_mut(&old_parent.index) {
                siblings.retain(|&idx| idx != child_id.index);
            }
        }

        let Some(parent_id) = parent_id else {
            // Detach only — done.
            return true;
        };

        if !self.allocator.is_alive(&parent_id) {
            tracing::warn!(entity_id = %parent_id, "set_parent: parent entity is stale");
            return false;
        }
        if parent_id.index == child_id.index {
            tracing::warn!(entity_id = %child_id, "set_parent: entity cannot be its own parent");
            return false;
        }

        // Cycle check: walk up from the proposed parent; if we reach child_id it's a cycle.
        let mut cursor = parent_id.index;
        for _ in 0..self.allocator.len() {
            let Some(ancestor) = self.parents.get(&cursor) else {
                break;
            };
            if ancestor.index == child_id.index {
                tracing::warn!(
                    child = %child_id,
                    parent = %parent_id,
                    "set_parent: would create a cycle"
                );
                return false;
            }
            cursor = ancestor.index;
        }

        self.parents.insert(child_id.index, parent_id);
        self.children
            .entry(parent_id.index)
            .or_default()
            .push(child_id.index);
        true
    }

    /// Return the parent of `id`, or `None` if it has no parent or is stale.
    pub fn get_parent(&self, id: &EntityId) -> Option<EntityId> {
        if !self.allocator.is_alive(id) {
            return None;
        }
        self.parents.get(&id.index).copied()
    }

    /// Return all live children of `id`.
    pub fn get_children(&self, id: &EntityId) -> Vec<EntityId> {
        if !self.allocator.is_alive(id) {
            return Vec::new();
        }
        self.children
            .get(&id.index)
            .map(|idxs| {
                idxs.iter()
                    .filter_map(|&idx| {
                        let candidate = self.parents.get(&idx)?;
                        // Look up the live EntityId for this child index.
                        self.allocator
                            .alive_entity_ids()
                            .find(|e| e.index == idx)
                            .filter(|_| self.allocator.is_alive(candidate))
                    })
                    .collect()
            })
            .unwrap_or_default()
    }

    /// Compute the world-space `Transform` for `id` by composing all ancestor
    /// transforms.  Returns `None` if the entity is stale or has no `Transform`.
    ///
    /// Composition (TRS, parent-first):
    ///   world_pos   = parent_world_pos + parent_world_rot × (parent_world_scale * local_pos)
    ///   world_rot   = parent_world_rot * local_rot
    ///   world_scale = parent_world_scale * local_scale  (component-wise)
    pub fn world_transform(&self, id: &EntityId) -> Option<Transform> {
        if !self.allocator.is_alive(id) {
            return None;
        }

        // Collect the ancestor chain (child-most first).
        let mut chain: Vec<u32> = Vec::new();
        let mut cursor = id.index;
        // Guard against degenerate cycles that somehow slipped through.
        for _ in 0..=self.allocator.len() {
            chain.push(cursor);
            match self.parents.get(&cursor) {
                Some(parent) => cursor = parent.index,
                None => break,
            }
        }

        // Compose from root down.
        let mut world_pos = Vec3::ZERO;
        let mut world_rot = Quat::IDENTITY;
        let mut world_scale = Vec3::ONE;

        for &idx in chain.iter().rev() {
            let Some(t) = self.transforms.get(&idx) else {
                // A parent without a Transform is treated as identity.
                continue;
            };
            let local_pos = Vec3::from(t.position);
            let [qx, qy, qz, qw] = t.rotation;
            let local_rot = Quat::from_xyzw(qx, qy, qz, qw);
            let local_scale = Vec3::from(t.scale);

            world_pos = world_pos + world_rot * (world_scale * local_pos);
            world_rot = (world_rot * local_rot).normalize();
            world_scale *= local_scale;
        }

        Some(Transform {
            position: world_pos.to_array(),
            rotation: [world_rot.x, world_rot.y, world_rot.z, world_rot.w],
            scale: world_scale.to_array(),
        })
    }

    /// Returns false if `id` is stale.
    ///
    /// All children of the destroyed entity are also destroyed recursively.
    pub fn destroy_entity(&mut self, id: EntityId) -> bool {
        if !self.allocator.is_alive(&id) {
            return false;
        }

        // Collect the full subtree (breadth-first) so we can free everything.
        let mut to_destroy: Vec<u32> = vec![id.index];
        let mut head = 0;
        while head < to_destroy.len() {
            let idx = to_destroy[head];
            head += 1;
            if let Some(kids) = self.children.get(&idx) {
                to_destroy.extend_from_slice(kids);
            }
        }

        // Detach the root from its own parent.
        if let Some(old_parent) = self.parents.remove(&id.index) {
            if let Some(siblings) = self.children.get_mut(&old_parent.index) {
                siblings.retain(|&i| i != id.index);
            }
        }

        // Free every entity in the subtree.
        for idx in to_destroy {
            // Build a synthetic EntityId to free the allocator slot.
            // We need the correct generation, which the allocator tracks internally.
            // Walk alive_entity_ids to find it (subtree is usually tiny).
            let eid = self
                .allocator
                .alive_entity_ids()
                .find(|e| e.index == idx);
            if let Some(eid) = eid {
                self.allocator.free(eid);
            }
            self.transforms.remove(&idx);
            self.sprite_renderers.remove(&idx);
            self.text_renderers.remove(&idx);
            self.camera_components.remove(&idx);
            self.mesh_renderers.remove(&idx);
            self.ambient_lights.remove(&idx);
            self.directional_lights.remove(&idx);
            self.point_lights.remove(&idx);
            for type_map in self.custom.values_mut() {
                type_map.remove(&idx);
            }
            self.children.remove(&idx);
            self.parents.remove(&idx);
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
            COMPONENT_CAMERA => match serde_json::from_value::<CameraComponent>(data) {
                Ok(c) => {
                    self.camera_components.insert(idx, c);
                }
                Err(e) => {
                    tracing::warn!("invalid core:camera data: {e}");
                    return false;
                }
            },
            COMPONENT_MESH_RENDERER => match serde_json::from_value::<MeshRenderer>(data) {
                Ok(m) => {
                    self.mesh_renderers.insert(idx, m);
                }
                Err(e) => {
                    tracing::warn!("invalid core:mesh_renderer data: {e}");
                    return false;
                }
            },
            COMPONENT_AMBIENT_LIGHT => match serde_json::from_value::<AmbientLight>(data) {
                Ok(l) => {
                    self.ambient_lights.insert(idx, l);
                }
                Err(e) => {
                    tracing::warn!("invalid core:ambient_light data: {e}");
                    return false;
                }
            },
            COMPONENT_DIRECTIONAL_LIGHT => {
                match serde_json::from_value::<DirectionalLight>(data) {
                    Ok(l) => {
                        self.directional_lights.insert(idx, l);
                    }
                    Err(e) => {
                        tracing::warn!("invalid core:directional_light data: {e}");
                        return false;
                    }
                }
            }
            COMPONENT_POINT_LIGHT => match serde_json::from_value::<PointLight>(data) {
                Ok(l) => {
                    self.point_lights.insert(idx, l);
                }
                Err(e) => {
                    tracing::warn!("invalid core:point_light data: {e}");
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
            COMPONENT_CAMERA => self.camera_components.remove(&idx).is_some(),
            COMPONENT_MESH_RENDERER => self.mesh_renderers.remove(&idx).is_some(),
            COMPONENT_AMBIENT_LIGHT => self.ambient_lights.remove(&idx).is_some(),
            COMPONENT_DIRECTIONAL_LIGHT => self.directional_lights.remove(&idx).is_some(),
            COMPONENT_POINT_LIGHT => self.point_lights.remove(&idx).is_some(),
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
            COMPONENT_CAMERA => self
                .camera_components
                .get(&idx)
                .and_then(|c| serde_json::to_value(c).ok()),
            COMPONENT_MESH_RENDERER => self
                .mesh_renderers
                .get(&idx)
                .and_then(|m| serde_json::to_value(m).ok()),
            COMPONENT_AMBIENT_LIGHT => self
                .ambient_lights
                .get(&idx)
                .and_then(|l| serde_json::to_value(l).ok()),
            COMPONENT_DIRECTIONAL_LIGHT => self
                .directional_lights
                .get(&idx)
                .and_then(|l| serde_json::to_value(l).ok()),
            COMPONENT_POINT_LIGHT => self
                .point_lights
                .get(&idx)
                .and_then(|l| serde_json::to_value(l).ok()),
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
