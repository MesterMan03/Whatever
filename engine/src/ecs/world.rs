use super::components::{
    AmbientLight, COMPONENT_AMBIENT_LIGHT, COMPONENT_CAMERA, COMPONENT_DIRECTIONAL_LIGHT,
    COMPONENT_MESH_RENDERER, COMPONENT_POINT_LIGHT, COMPONENT_SPRITE_RENDERER,
    COMPONENT_TEXT_RENDERER, COMPONENT_TRANSFORM, CameraComponent, DirectionalLight, MeshRenderer,
    PointLight, SpriteRenderer, TextRenderer, Transform,
};
use super::entity::{EntityAllocator, EntityId};
use glam::{Quat, Vec3};
use std::collections::HashMap;

#[derive(Clone, serde::Serialize, serde::Deserialize)]
pub struct World {
    #[serde(skip)]
    pub(crate) scene_changes: super::snapshot::SceneChanges,
    #[serde(default)]
    pub names: HashMap<u32, String>,
    #[serde(default)]
    pub tags: HashMap<u32, Vec<String>>,
    #[serde(default)]
    pub component_types: std::collections::BTreeMap<String, super::ComponentDefinition>,
    #[serde(default)]
    pub component_instances: HashMap<String, HashMap<u32, u64>>,
    #[serde(default)]
    next_component_instance: u64,
    #[serde(skip)]
    pub(crate) allow_missing_components: bool,
    #[serde(default)]
    pub root_order: Vec<EntityId>,
    pub disabled: std::collections::HashSet<u32>,
    pub rigid_body_states: HashMap<u32, super::RigidBodyState>,
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
            scene_changes: Default::default(),
            names: HashMap::new(),
            tags: HashMap::new(),
            component_types: Default::default(),
            component_instances: Default::default(),
            next_component_instance: 0,
            allow_missing_components: false,
            root_order: Vec::new(),
            disabled: Default::default(),
            rigid_body_states: HashMap::new(),
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
        let id = self.allocator.alloc();
        self.root_order.push(id);
        self.scene_changes.created.insert(id);
        id
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

        if let Some(parent) = parent_id {
            if !self.allocator.is_alive(&parent) || parent == child_id {
                return false;
            }
            let mut cursor = parent;
            for _ in 0..=self.allocator.len() {
                if cursor == child_id {
                    return false;
                }
                match self.get_parent(&cursor) {
                    Some(next) => cursor = next,
                    None => break,
                }
            }
        }
        self.mark_hierarchy(child_id);
        if let Some(old_parent) = self.get_parent(&child_id) {
            self.mark_metadata(old_parent);
        }
        if let Some(parent) = parent_id {
            self.mark_metadata(parent);
        }
        if let Some(old_parent) = self.parents.remove(&child_id.index)
            && let Some(siblings) = self.children.get_mut(&old_parent.index)
        {
            siblings.retain(|&idx| idx != child_id.index);
        }
        let Some(parent_id) = parent_id else {
            self.root_order.retain(|id| *id != child_id);
            self.root_order.push(child_id);
            return true;
        };

        self.root_order.retain(|id| *id != child_id);
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
                        (self.parents.get(&idx) == Some(id))
                            .then(|| self.allocator.live_id(idx))
                            .flatten()
                    })
                    .collect()
            })
            .unwrap_or_default()
    }

    /// A stable traversal, deduplicated when roots overlap; stale roots are ignored.
    pub fn hierarchy(&self, roots: impl IntoIterator<Item = EntityId>) -> Vec<EntityId> {
        let mut pending: Vec<_> = roots.into_iter().collect();
        pending.reverse();
        let mut seen = std::collections::HashSet::new();
        let mut result = Vec::new();
        while let Some(id) = pending.pop() {
            if !self.is_alive(&id) || !seen.insert(id) {
                continue;
            }
            result.push(id);
            pending.extend(self.get_children(&id).into_iter().rev());
        }
        result
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

            world_pos += world_rot * (world_scale * local_pos);
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

        if let Some(parent) = self.get_parent(&id) {
            self.mark_metadata(parent);
        }
        // Detach the root from its own parent.
        if let Some(old_parent) = self.parents.remove(&id.index)
            && let Some(siblings) = self.children.get_mut(&old_parent.index)
        {
            siblings.retain(|&i| i != id.index);
        }

        // Free every entity in the subtree.
        for idx in to_destroy {
            let eid = self.allocator.live_id(idx);
            if let Some(eid) = eid {
                self.scene_changes.removed.insert(eid);
                self.disabled.remove(&idx);
                self.root_order.retain(|id| *id != eid);
                self.allocator.free(eid);
            }
            self.names.remove(&idx);
            self.tags.remove(&idx);
            self.transforms.remove(&idx);
            self.rigid_body_states.remove(&idx);
            self.sprite_renderers.remove(&idx);
            self.text_renderers.remove(&idx);
            self.camera_components.remove(&idx);
            self.mesh_renderers.remove(&idx);
            self.ambient_lights.remove(&idx);
            self.directional_lights.remove(&idx);
            self.point_lights.remove(&idx);
            for instances in self.component_instances.values_mut() {
                instances.remove(&idx);
            }
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
        let data = if let Some(definition) = self.component_types.get(type_id) {
            match definition.normalize(&data) {
                Ok(data) => data,
                Err(error) => {
                    tracing::warn!(%type_id, %error, "component rejected");
                    return false;
                }
            }
        } else if !type_id.starts_with("core:") && !self.allow_missing_components {
            tracing::warn!(%type_id, "unregistered component rejected");
            return false;
        } else {
            data
        };
        self.mark_component(*id, type_id);
        if type_id == "core:transform" {
            self.mark_hierarchy(*id);
        }
        let idx = id.index;
        if self.component_types.contains_key(type_id)
            && !self
                .custom
                .get(type_id)
                .is_some_and(|entities| entities.contains_key(&idx))
        {
            let Some(next) = self.next_component_instance.checked_add(1) else {
                return false;
            };
            self.next_component_instance = next;
            self.component_instances
                .entry(type_id.to_owned())
                .or_default()
                .insert(idx, next);
        }
        match type_id {
            "core:entity" | "core:world_transform" => return false,
            super::COMPONENT_RIGID_BODY_STATE => return false,
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
            COMPONENT_DIRECTIONAL_LIGHT => match serde_json::from_value::<DirectionalLight>(data) {
                Ok(l) => {
                    self.directional_lights.insert(idx, l);
                }
                Err(e) => {
                    tracing::warn!("invalid core:directional_light data: {e}");
                    return false;
                }
            },
            COMPONENT_POINT_LIGHT => match serde_json::from_value::<PointLight>(data) {
                Ok(l) => {
                    self.point_lights.insert(idx, l);
                }
                Err(e) => {
                    tracing::warn!("invalid core:point_light data: {e}");
                    return false;
                }
            },
            _ if !type_id.starts_with("core:")
                || matches!(
                    type_id,
                    "core:physics_world"
                        | "core:rigid_body"
                        | "core:collider"
                        | "core:joint"
                        | "core:physics_status"
                ) =>
            {
                self.custom
                    .entry(type_id.to_owned())
                    .or_default()
                    .insert(idx, data);
            }
            _ => return false,
        }
        true
    }

    /// Remove a component. Returns false if the entity is stale or had no such component.
    pub fn remove_component(&mut self, id: &EntityId, type_id: &str) -> bool {
        if !self.allocator.is_alive(id) {
            return false;
        }
        self.mark_component(*id, type_id);
        if type_id == "core:transform" {
            self.mark_hierarchy(*id);
        }
        let idx = id.index;
        if let Some(instances) = self.component_instances.get_mut(type_id) {
            instances.remove(&idx);
        }
        match type_id {
            "core:entity" | "core:world_transform" | super::COMPONENT_RIGID_BODY_STATE => false,
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
            "core:world_transform" => self
                .world_transform(id)
                .and_then(|value| serde_json::to_value(value).ok()),
            "core:entity" => {
                Some(serde_json::json!({"enabled":self.enabled(id),"active":self.is_active(id)}))
            }
            super::COMPONENT_RIGID_BODY_STATE => self
                .rigid_body_states
                .get(&idx)
                .and_then(|state| serde_json::to_value(state).ok()),
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
            .filter(|id| self.is_active(id))
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

impl Default for World {
    fn default() -> Self {
        Self::new()
    }
}
