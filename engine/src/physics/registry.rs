use super::config::*;
use crate::ecs::{EntityId, RigidBodyState, World, referenced_entity};
use anyhow::{Context, bail, ensure};
use rapier3d::parry::partitioning::{Bvh, BvhBuildStrategy};
use rapier3d::prelude::*;
use serde::Deserialize;
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::sync::mpsc::{self, Receiver};

struct NativeWorld {
    native: PhysicsWorld,
    config: WorldConfig,
    events: ChannelEventCollector,
    collisions: Receiver<CollisionEvent>,
    forces: Receiver<ContactForceEvent>,
    collider_ids: HashMap<ColliderHandle, EntityId>,
    query_bvh: Bvh,
    queries_dirty: bool,
    last_tick: u64,
}
impl NativeWorld {
    fn new(config: WorldConfig) -> anyhow::Result<Self> {
        let (collision_tx, collisions) = mpsc::channel();
        let (force_tx, forces) = mpsc::channel();
        let (tear_tx, _) = mpsc::channel();
        let mut result = Self {
            native: PhysicsWorld::default(),
            config,
            events: ChannelEventCollector::new(collision_tx, force_tx, tear_tx),
            collisions,
            forces,
            collider_ids: HashMap::new(),
            query_bvh: Bvh::new(),
            queries_dirty: true,
            last_tick: 0,
        };
        result.configure()?;
        Ok(result)
    }
    fn configure(&mut self) -> anyhow::Result<()> {
        self.native.gravity = vec3(self.config.gravity)?;
        self.native.integration_parameters = self.config.integration()?;
        Ok(())
    }
    fn drain_events(
        &mut self,
        world: EntityId,
        tick: u64,
        substep: u32,
        phase: &str,
    ) -> Vec<Value> {
        let mut result = Vec::new();
        for event in self.collisions.try_iter() {
            let (a, b, kind, flags) = match event {
                CollisionEvent::Started(a, b, flags) => (a, b, "started", flags),
                CollisionEvent::Stopped(a, b, flags) => (a, b, "stopped", flags),
            };
            if let (Some(a), Some(b)) = (self.collider_ids.get(&a), self.collider_ids.get(&b)) {
                let (a, b) = if a <= b { (*a, *b) } else { (*b, *a) };
                result.push(json!({"world":reference(world),"colliderA":reference(a),"colliderB":reference(b),"kind":kind,"sensor":flags.contains(CollisionEventFlags::SENSOR),"removed":flags.contains(CollisionEventFlags::REMOVED),"tickNumber":tick,"substep":substep,"phase":phase}));
            }
        }
        for event in self.forces.try_iter() {
            if let (Some(a), Some(b)) = (
                self.collider_ids.get(&event.collider1),
                self.collider_ids.get(&event.collider2),
            ) {
                result.push(json!({"world":reference(world),"colliderA":reference(*a),"colliderB":reference(*b),"kind":"contactForce","totalForce":event.total_force.to_array(),"totalForceMagnitude":event.total_force_magnitude,"maxForceDirection":event.max_force_direction.to_array(),"maxForceMagnitude":event.max_force_magnitude,"tickNumber":tick,"substep":substep,"phase":phase}));
            }
        }
        result.sort_by_key(|event| {
            (
                event["colliderA"]["$entity"]
                    .as_str()
                    .unwrap_or_default()
                    .to_owned(),
                event["colliderB"]["$entity"]
                    .as_str()
                    .unwrap_or_default()
                    .to_owned(),
                event["kind"].as_str().unwrap_or_default().to_owned(),
            )
        });
        result
    }
}
struct BodyEntry {
    world: EntityId,
    handle: RigidBodyHandle,
    data: Value,
    last_pose: Pose,
}
struct ColliderEntry {
    world: EntityId,
    handle: ColliderHandle,
    body: Option<EntityId>,
    data: Value,
}
#[derive(Clone, Copy)]
enum JointHandle {
    Impulse(ImpulseJointHandle),
    Multibody(MultibodyJointHandle),
}
struct JointEntry {
    world: EntityId,
    handle: JointHandle,
    a: EntityId,
    b: EntityId,
    data: Value,
}

/// Native solver state is deliberately outside the cloneable presentation/ECS snapshot.
#[derive(Default)]
pub struct PhysicsRegistry {
    worlds: BTreeMap<EntityId, NativeWorld>,
    bodies: BTreeMap<EntityId, BodyEntry>,
    colliders: BTreeMap<EntityId, ColliderEntry>,
    joints: BTreeMap<EntityId, JointEntry>,
    default_world: Option<EntityId>,
    dirty: bool,
    tick: u64,
    pending_events: Vec<Value>,
    moved: BTreeSet<EntityId>,
    pose_dependencies: BTreeSet<EntityId>,
    pub subscribers: BTreeSet<String>,
}
pub fn reference(id: EntityId) -> Value {
    json!({"$entity":id.to_string()})
}
fn entity(value: &Value) -> anyhow::Result<EntityId> {
    referenced_entity(value).context("expected a typed ECS entity reference")
}
fn component_data(world: &World, kind: &str) -> BTreeMap<EntityId, Value> {
    world
        .custom
        .get(kind)
        .into_iter()
        .flat_map(|components| components.iter())
        .filter_map(|(&index, data)| {
            world
                .allocator
                .live_id(index)
                .filter(|id| world.is_active(id))
                .map(|id| (id, data.clone()))
        })
        .collect()
}
fn status(
    statuses: &mut BTreeMap<EntityId, Value>,
    id: EntityId,
    kind: &str,
    error: Option<String>,
) {
    let data = statuses.entry(id).or_insert_with(|| json!({}));
    data[kind] = json!({"active":error.is_none(),"error":error});
}
impl PhysicsRegistry {
    pub fn invalidate(&mut self) {
        self.dirty = true;
    }
    pub fn component_changed(&mut self, world: &World, id: EntityId, kind: &str) {
        if matches!(kind, WORLD | BODY | COLLIDER | JOINT)
            || kind == "core:transform"
                && (self.dirty
                    || self.pose_dependencies.contains(&id)
                    || world
                        .custom
                        .get(BODY)
                        .is_some_and(|components| components.contains_key(&id.index))
                    || world
                        .custom
                        .get(COLLIDER)
                        .and_then(|components| components.get(&id.index))
                        .is_some_and(|data| data["body"].is_null()))
        {
            self.dirty = true;
        }
    }
    pub fn entity_destroyed(&mut self, world: &World, id: EntityId) {
        if self.pose_dependencies.contains(&id)
            || world.hierarchy([id]).into_iter().any(|id| {
                [WORLD, BODY, COLLIDER, JOINT].iter().any(|kind| {
                    world
                        .custom
                        .get(*kind)
                        .is_some_and(|components| components.contains_key(&id.index))
                })
            })
        {
            self.dirty = true;
        }
    }
    pub fn begin_tick(&mut self, tick: u64) {
        self.tick = tick;
    }
    pub fn is_read_only(kind: &str) -> bool {
        matches!(kind, STATE | STATUS)
    }
    pub fn ensure_default(&mut self, world: &mut World) -> EntityId {
        if let Some(id) = self.default_world.filter(|id| {
            world.allocator.is_alive(id)
                && world
                    .custom
                    .get(WORLD)
                    .is_some_and(|components| components.contains_key(&id.index))
        }) {
            return id;
        }
        let id = world.create_entity();
        let mut config = WorldConfig::default();
        if let Err(error) = config.normalize() {
            tracing::error!(%error,"invalid engine physics defaults");
        }
        match serde_json::to_value(config) {
            Ok(data) => {
                world.set_component(&id, WORLD, data);
            }
            Err(error) => tracing::error!(%error,"serialize physics defaults"),
        }
        self.default_world = Some(id);
        self.dirty = true;
        id
    }
    /// Validate and normalize before touching ECS. Invalid edits leave the previous component intact.
    pub fn normalize_component(
        &mut self,
        world: &mut World,
        id: EntityId,
        kind: &str,
        mut data: Value,
    ) -> anyhow::Result<Value> {
        ensure!(
            !Self::is_read_only(kind),
            "{kind} is native read-only state"
        );
        Ok(match kind {
            WORLD => {
                let mut config: WorldConfig = serde_json::from_value(data)?;
                config.normalize()?;
                serde_json::to_value(config)?
            }
            BODY => {
                let mut config: BodyConfig = serde_json::from_value(data)?;
                config.validate()?;
                if config.world.is_null() {
                    config.world = reference(self.ensure_default(world));
                }
                entity(&config.world)?;
                serde_json::to_value(config)?
            }
            COLLIDER => {
                if let Some(fields) = data.as_object_mut() {
                    fields.entry("body").or_insert_with(|| reference(id));
                }
                let mut config: ColliderConfig = serde_json::from_value(data)?;
                if config.mass.is_none()
                    && config.density.is_none()
                    && config.rapier.mass_properties.is_none()
                {
                    config.density = Some(1.0);
                }
                let _ = config.builder()?;
                if config.body.is_null() {
                    if config.world.is_none() {
                        config.world = Some(reference(self.ensure_default(world)));
                    }
                } else {
                    entity(&config.body)?;
                }
                if let Some(world) = &config.world {
                    entity(world)?;
                }
                serde_json::to_value(config)?
            }
            JOINT => {
                let config: JointConfig = serde_json::from_value(data)?;
                ensure!(
                    (if config.body_a.is_null() {
                        id
                    } else {
                        entity(&config.body_a)?
                    }) != entity(&config.body_b)?,
                    "a joint must connect two different bodies"
                );
                config.native()?;
                serde_json::to_value(config)?
            }
            _ => data,
        })
    }
    fn remove_body(&mut self, id: EntityId) {
        let _ = self.take_body(id);
    }
    fn take_body(&mut self, id: EntityId) -> Option<RigidBody> {
        let entry = self.bodies.remove(&id)?;
        self.worlds
            .get_mut(&entry.world)?
            .native
            .remove_body(entry.handle)
    }
    fn remove_collider(&mut self, id: EntityId) {
        if let Some(entry) = self.colliders.remove(&id)
            && let Some(world) = self.worlds.get_mut(&entry.world)
        {
            world.native.remove_collider(entry.handle);
        }
    }
    fn remove_joint(&mut self, id: EntityId) {
        if let Some(entry) = self.joints.remove(&id)
            && let Some(world) = self.worlds.get_mut(&entry.world)
        {
            match entry.handle {
                JointHandle::Impulse(handle) => {
                    world.native.remove_impulse_joint(handle);
                }
                JointHandle::Multibody(handle) => world.native.remove_multibody_joint(handle),
            }
        }
    }
    pub fn synchronize(&mut self, world: &mut World) -> anyhow::Result<()> {
        if !self.dirty {
            return Ok(());
        }
        let settings = component_data(world, WORLD);
        let bodies = component_data(world, BODY);
        let colliders = component_data(world, COLLIDER);
        let joints = component_data(world, JOINT);
        let mut statuses = BTreeMap::new();
        // Extract migrating bodies before an old world is removed, preserving velocity,
        // forces and sleeping state. Collider/joint internal links are rebuilt by Rapier.
        let migrations: Vec<_> = bodies
            .iter()
            .filter_map(|(&id, data)| {
                let target = referenced_entity(&data["world"])?;
                self.bodies
                    .get(&id)
                    .filter(|entry| entry.world != target && settings.contains_key(&target))
                    .map(|_| id)
            })
            .collect();
        let mut migrating: BTreeMap<_, _> = migrations
            .into_iter()
            .filter_map(|id| self.take_body(id).map(|body| (id, body)))
            .collect();
        self.worlds.retain(|id, _| settings.contains_key(id));
        for (&id, data) in &settings {
            let config: WorldConfig = serde_json::from_value(data.clone())?;
            if let Some(native) = self.worlds.get_mut(&id) {
                native.config = config;
                native.configure()?;
            } else {
                self.worlds.insert(id, NativeWorld::new(config)?);
            }
            status(&mut statuses, id, "world", None);
        }
        let removed: Vec<_> = self
            .bodies
            .keys()
            .filter(|id| {
                !bodies.contains_key(id) || !self.worlds.contains_key(&self.bodies[id].world)
            })
            .copied()
            .collect();
        for id in removed {
            self.remove_body(id);
        }
        for (&id, data) in &bodies {
            let config: BodyConfig = serde_json::from_value(data.clone())?;
            let world_id = entity(&config.world)?;
            if self
                .bodies
                .get(&id)
                .is_some_and(|entry| entry.world != world_id)
            {
                self.remove_body(id);
            }
            if !self.worlds.contains_key(&world_id) {
                self.remove_body(id);
                status(
                    &mut statuses,
                    id,
                    "body",
                    Some(format!("physics world {world_id} is missing")),
                );
                continue;
            }
            if !world.transforms.contains_key(&id.index) {
                self.moved.insert(id);
            }
            world.transforms.entry(id.index).or_default();
            if let Some(parent) = world.get_parent(&id) {
                let scale = world.world_transform(&parent).unwrap_or_default().scale;
                if scale
                    .iter()
                    .any(|value| !value.is_finite() || value.abs() <= 1.0e-8)
                {
                    self.remove_body(id);
                    status(
                        &mut statuses,
                        id,
                        "body",
                        Some("physics parent has a zero or invalid world scale".into()),
                    );
                    continue;
                }
            }
            let transform = world.world_transform(&id).unwrap_or_default();
            let pose = match pose(transform.position, transform.rotation) {
                Ok(pose) => pose,
                Err(error) => {
                    self.remove_body(id);
                    status(&mut statuses, id, "body", Some(error.to_string()));
                    continue;
                }
            };
            let native = self
                .worlds
                .get_mut(&world_id)
                .context("validated physics world")?;
            if let Some(entry) = self.bodies.get_mut(&id) {
                let body = native
                    .native
                    .bodies
                    .get_mut(entry.handle)
                    .context("registered body")?;
                if entry.data != *data {
                    config.apply(body)?;
                    entry.data = data.clone();
                }
                if entry.last_pose != pose {
                    body.set_position(pose, true);
                    entry.last_pose = pose;
                }
            } else {
                let mut body = match migrating.remove(&id) {
                    Some(mut body) => {
                        body.set_position(pose, false);
                        body
                    }
                    None => RigidBodyBuilder::new(config.kind()?).pose(pose).build(),
                };
                config.apply(&mut body)?;
                let handle = native.native.insert_body(body);
                self.bodies.insert(
                    id,
                    BodyEntry {
                        world: world_id,
                        handle,
                        data: data.clone(),
                        last_pose: pose,
                    },
                );
            }
            status(&mut statuses, id, "body", None);
        }
        let removed: Vec<_> = self
            .colliders
            .keys()
            .filter(|id| {
                !colliders.contains_key(id)
                    || self
                        .worlds
                        .get(&self.colliders[id].world)
                        .is_none_or(|native| {
                            native
                                .native
                                .colliders
                                .get(self.colliders[id].handle)
                                .is_none()
                        })
            })
            .copied()
            .collect();
        for id in removed {
            self.remove_collider(id);
        }
        for (&id, data) in &colliders {
            let config: ColliderConfig = serde_json::from_value(data.clone())?;
            let body_id = if config.body.is_null() {
                None
            } else {
                Some(entity(&config.body)?)
            };
            let (world_id, parent) = if let Some(body_id) = body_id {
                let Some(body) = self.bodies.get(&body_id) else {
                    self.remove_collider(id);
                    status(
                        &mut statuses,
                        id,
                        "collider",
                        Some(format!("rigid body {body_id} is missing or inactive")),
                    );
                    continue;
                };
                if let Some(explicit) = &config.world
                    && entity(explicit)? != body.world
                {
                    self.remove_collider(id);
                    status(
                        &mut statuses,
                        id,
                        "collider",
                        Some("attached collider must use its body's world".into()),
                    );
                    continue;
                }
                (body.world, Some(body.handle))
            } else {
                (
                    entity(config.world.as_ref().context("standalone collider world")?)?,
                    None,
                )
            };
            if !self.worlds.contains_key(&world_id) {
                self.remove_collider(id);
                status(
                    &mut statuses,
                    id,
                    "collider",
                    Some(format!("physics world {world_id} is missing")),
                );
                continue;
            }
            if self
                .colliders
                .get(&id)
                .is_some_and(|entry| entry.world != world_id || entry.body != body_id)
            {
                self.remove_collider(id);
            }
            let native = self.worlds.get_mut(&world_id).context("collider world")?;
            let local = pose(config.position, config.rotation)?;
            let collider_pose = if parent.is_some() {
                local
            } else {
                let transform = world.world_transform(&id).unwrap_or_default();
                pose(transform.position, transform.rotation)? * local
            };
            if let Some(entry) = self.colliders.get_mut(&id) {
                let collider = native
                    .native
                    .colliders
                    .get_mut(entry.handle)
                    .context("registered collider")?;
                if entry.data != *data {
                    let mut replacement = config.builder()?.build();
                    replacement.set_position(collider_pose);
                    collider.copy_from(&replacement);
                    entry.data = data.clone();
                }
                if parent.is_some() {
                    collider.set_position_wrt_parent(collider_pose);
                } else {
                    collider.set_position(collider_pose);
                }
            } else {
                let handle = native
                    .native
                    .insert_collider(config.builder()?.position(collider_pose), parent);
                native.collider_ids.insert(handle, id);
                self.colliders.insert(
                    id,
                    ColliderEntry {
                        world: world_id,
                        handle,
                        body: body_id,
                        data: data.clone(),
                    },
                );
            }
            status(&mut statuses, id, "collider", None);
        }
        let removed: Vec<_> = self
            .joints
            .keys()
            .filter(|id| !joints.contains_key(id) || !self.joint_alive(&self.joints[id]))
            .copied()
            .collect();
        for id in removed {
            self.remove_joint(id);
        }
        for (&id, data) in &joints {
            if self
                .joints
                .get(&id)
                .is_some_and(|entry| !self.joint_alive(entry))
            {
                self.remove_joint(id);
            }
            let config: JointConfig = serde_json::from_value(data.clone())?;
            let a = if config.body_a.is_null() {
                id
            } else {
                entity(&config.body_a)?
            };
            let b = entity(&config.body_b)?;
            let Some((world_id, a_handle, b_handle)) = self
                .bodies
                .get(&a)
                .zip(self.bodies.get(&b))
                .filter(|(a, b)| a.world == b.world)
                .map(|(a, b)| (a.world, a.handle, b.handle))
            else {
                self.remove_joint(id);
                status(
                    &mut statuses,
                    id,
                    "joint",
                    Some("joint endpoints must be active bodies in the same world".into()),
                );
                continue;
            };
            let data_native = config.native()?;
            if self.joints.get(&id).is_some_and(|entry| {
                entry.world != world_id
                    || entry.a != a
                    || entry.b != b
                    || matches!(
                        (entry.handle, config.solver.as_str()),
                        (JointHandle::Impulse(_), "multibody")
                            | (JointHandle::Multibody(_), "impulse")
                    )
            }) {
                self.remove_joint(id);
            }
            // Updating articulation topology changes its generalized coordinates; rebuild that joint.
            if self.joints.get(&id).is_some_and(|entry| {
                matches!(entry.handle, JointHandle::Multibody(_)) && entry.data != *data
            }) {
                self.remove_joint(id);
            }
            let native = self.worlds.get_mut(&world_id).context("joint world")?;
            if let Some(entry) = self.joints.get_mut(&id) {
                if entry.data != *data {
                    if let JointHandle::Impulse(handle) = entry.handle {
                        let joint = native
                            .native
                            .impulse_joints
                            .get_mut(handle, true)
                            .context("registered impulse joint")?;
                        joint.data = data_native;
                    }
                    entry.data = data.clone();
                }
            } else {
                let handle = if config.solver == "multibody" {
                    match native
                        .native
                        .insert_multibody_joint(a_handle, b_handle, data_native)
                    {
                        Some(handle) => JointHandle::Multibody(handle),
                        None => {
                            status(&mut statuses,id,"joint",Some("invalid multibody articulation (cycle, incompatible root, or duplicate link)".into()));
                            continue;
                        }
                    }
                } else {
                    JointHandle::Impulse(native.native.insert_impulse_joint(
                        a_handle,
                        b_handle,
                        data_native,
                    ))
                };
                self.joints.insert(
                    id,
                    JointEntry {
                        world: world_id,
                        handle,
                        a,
                        b,
                        data: data.clone(),
                    },
                );
            }
            status(&mut statuses, id, "joint", None);
        }
        for entry in self.bodies.values() {
            if let Some(native) = self.worlds.get_mut(&entry.world)
                && let Some(body) = native.native.bodies.get_mut(entry.handle)
            {
                body.recompute_mass_properties_from_colliders(&native.native.colliders);
            }
        }
        world.custom.insert(
            STATUS.into(),
            statuses
                .into_iter()
                .map(|(id, data)| (id.index, data))
                .collect(),
        );
        self.refresh_states(world);
        self.pose_dependencies.clear();
        for id in bodies.keys().copied().chain(
            colliders
                .iter()
                .filter(|(_, data)| data["body"].is_null())
                .map(|(&id, _)| id),
        ) {
            let mut cursor = Some(id);
            while let Some(id) = cursor {
                self.pose_dependencies.insert(id);
                cursor = world.get_parent(&id);
            }
        }
        for native in self.worlds.values_mut() {
            native.queries_dirty = true;
        }
        self.dirty = false;
        Ok(())
    }
    fn joint_alive(&self, entry: &JointEntry) -> bool {
        self.worlds
            .get(&entry.world)
            .is_some_and(|world| match entry.handle {
                JointHandle::Impulse(handle) => world.native.impulse_joints.get(handle).is_some(),
                JointHandle::Multibody(handle) => {
                    world.native.multibody_joints.get(handle).is_some()
                }
            })
    }
    fn body_state(&self, id: EntityId) -> anyhow::Result<RigidBodyState> {
        let entry = self
            .bodies
            .get(&id)
            .context("entity has no active rigid body (inspect core:physics_status)")?;
        let native = self
            .worlds
            .get(&entry.world)
            .context("inactive physics world")?;
        let body = native
            .native
            .bodies
            .get(entry.handle)
            .context("inactive rigid body")?;
        Ok(RigidBodyState {
            world: entry.world,
            position: body.translation().to_array(),
            rotation: body.rotation().to_array(),
            linear_velocity: body.linvel().to_array(),
            angular_velocity: body.angvel().to_array(),
            sleeping: body.is_sleeping(),
            mass: body.mass(),
            center_of_mass: body.center_of_mass().to_array(),
            tick_number: native.last_tick,
        })
    }
    fn refresh_states(&self, world: &mut World) {
        world.rigid_body_states.clear();
        world.rigid_body_states.extend(
            self.bodies
                .keys()
                .filter_map(|&id| self.body_state(id).ok().map(|state| (id.index, state))),
        );
    }
    pub fn step(&mut self, world: &mut World, delta: f64) -> anyhow::Result<Vec<EntityId>> {
        self.synchronize(world)?;
        for (&id, native) in &mut self.worlds {
            if native.config.time_scale == 0.0 {
                continue;
            }
            let dt = (delta * f64::from(native.config.time_scale)
                / f64::from(native.config.substeps)) as f32;
            ensure!(dt.is_finite() && dt > 0.0, "invalid physics step duration");
            native.native.integration_parameters.dt = dt;
            let targets: Vec<_> = native
                .native
                .bodies
                .iter()
                .filter(|(_, body)| {
                    body.body_type() == RigidBodyType::KinematicPositionBased
                        && body.position() != body.next_position()
                })
                .map(|(handle, body)| (handle, *body.position(), *body.next_position()))
                .collect();
            for substep in 0..native.config.substeps {
                for (handle, start, target) in &targets {
                    if let Some(body) = native.native.bodies.get_mut(*handle) {
                        body.set_next_kinematic_position(
                            start
                                .lerp(target, (substep + 1) as f32 / native.config.substeps as f32),
                        );
                    }
                }
                native.native.step_with_events(&(), &native.events);
                self.pending_events
                    .extend(native.drain_events(id, self.tick, substep, "step"));
            }
            native.queries_dirty = true;
            native.last_tick = self.tick;
            native
                .collider_ids
                .retain(|handle, _| native.native.colliders.get(*handle).is_some());
        }
        self.write_poses(world)?;
        self.refresh_states(world);
        Ok(std::mem::take(&mut self.moved).into_iter().collect())
    }
    pub fn take_events(&mut self) -> Vec<Value> {
        std::mem::take(&mut self.pending_events)
    }
    /// Parents are written before children so conversion back to local ECS coordinates is exact.
    fn write_poses(&mut self, world: &mut World) -> anyhow::Result<Vec<EntityId>> {
        let mut poses = Vec::new();
        for (&id, entry) in &self.bodies {
            let native = self
                .worlds
                .get(&entry.world)
                .and_then(|world| world.native.bodies.get(entry.handle))
                .context("body pose")?;
            let mut depth = 0;
            let mut parent = world.get_parent(&id);
            while let Some(id) = parent {
                depth += 1;
                parent = world.get_parent(&id);
            }
            poses.push((depth, id, *native.position()));
        }
        poses.sort_by_key(|(depth, id, _)| (*depth, *id));
        let mut changed = Vec::new();
        for (_, id, pose) in poses {
            let mut position = pose.translation.to_array();
            let mut rotation = pose.rotation.to_array();
            if let Some(parent) = world.get_parent(&id) {
                let parent = world.world_transform(&parent).unwrap_or_default();
                let parent_rotation = glam::Quat::from_array(parent.rotation).normalize();
                let scale = glam::Vec3::from_array(parent.scale);
                if !scale.is_finite() || scale.abs().min_element() <= 1.0e-8 {
                    bail!("physics parent of {id} has a zero or invalid scale");
                }
                position = (parent_rotation.inverse()
                    * (glam::Vec3::from_array(position) - glam::Vec3::from_array(parent.position))
                    / scale)
                    .to_array();
                rotation = (parent_rotation.inverse() * glam::Quat::from_array(rotation))
                    .normalize()
                    .to_array();
            }
            let transform = world.transforms.entry(id.index).or_default();
            if transform.position != position || transform.rotation != rotation {
                transform.position = position;
                transform.rotation = rotation;
                changed.push(id);
                self.moved.insert(id);
            }
            if let Some(entry) = self.bodies.get_mut(&id) {
                // Match the actual ECS composition (including float rounding) to avoid false teleports.
                let actual = world.world_transform(&id).unwrap_or_default();
                entry.last_pose = super::config::pose(actual.position, actual.rotation)?;
            }
        }
        Ok(changed)
    }
    pub fn request(&mut self, world: &mut World, data: Value) -> anyhow::Result<Value> {
        let request: Request = serde_json::from_value(data)?;
        self.synchronize(world)?;
        match request {
            Request::WorldEntity { world: target } => {
                let id = entity(&target)?;
                ensure!(
                    self.worlds.contains_key(&id),
                    "entity has no active physics world"
                );
                Ok(reference(id))
            }
            Request::State { entity: target } => {
                Ok(serde_json::to_value(self.body_state(entity(&target)?)?)?)
            }
            Request::Body {
                entity: target,
                action,
                vector,
                point,
                rotation: quaternion,
                wake,
                axes,
            } => {
                let id = entity(&target)?;
                let entry = self
                    .bodies
                    .get(&id)
                    .context("entity has no active rigid body (inspect core:physics_status)")?;
                let native = self.worlds.get_mut(&entry.world).context("body world")?;
                let body = native
                    .native
                    .bodies
                    .get_mut(entry.handle)
                    .context("body handle")?;
                let vector = vector.map(vec3).transpose()?;
                let point = point.map(vec3).transpose()?;
                let quaternion = quaternion.map(super::config::rotation).transpose()?;
                match action.as_str() {
                    "applyImpulse" => {
                        let vector = vector.context("impulse vector required")?;
                        if let Some(point) = point {
                            body.apply_impulse_at_point(vector, point, wake);
                        } else {
                            body.apply_impulse(vector, wake);
                        }
                    }
                    "addForce" => {
                        let vector = vector.context("force vector required")?;
                        if let Some(point) = point {
                            body.add_force_at_point(vector, point, wake);
                        } else {
                            body.add_force(vector, wake);
                        }
                    }
                    "applyTorqueImpulse" => {
                        body.apply_torque_impulse(vector.context("torque impulse required")?, wake)
                    }
                    "addTorque" => body.add_torque(vector.context("torque required")?, wake),
                    "resetForces" => body.reset_forces(wake),
                    "resetTorques" => body.reset_torques(wake),
                    "setLinearVelocity" => {
                        let mut value = vector.context("linear velocity required")?;
                        if let Some(axes) = axes {
                            ensure!(
                                axes.iter()
                                    .all(|axis| matches!(axis.as_str(), "x" | "y" | "z")),
                                "velocity axes must be x, y or z"
                            );
                            for (index, axis) in ["x", "y", "z"].into_iter().enumerate() {
                                if !axes.iter().any(|item| item == axis) {
                                    value[index] = body.linvel()[index];
                                }
                            }
                        }
                        body.set_linvel(value, wake);
                    }
                    "setAngularVelocity" => {
                        body.set_angvel(vector.context("angular velocity required")?, wake)
                    }
                    "teleport" => {
                        let mut value = *body.position();
                        if let Some(position) = vector {
                            value.translation = position;
                        }
                        if let Some(rotation) = quaternion {
                            value.rotation = rotation;
                        }
                        body.set_position(value, wake);
                    }
                    "setKinematicTarget" => {
                        ensure!(
                            body.is_kinematic()
                                && body.body_type() == RigidBodyType::KinematicPositionBased,
                            "setKinematicTarget requires a kinematicPosition body"
                        );
                        let mut value = *body.position();
                        if let Some(position) = vector {
                            value.translation = position;
                        }
                        if let Some(rotation) = quaternion {
                            value.rotation = rotation;
                        }
                        body.set_next_kinematic_position(value);
                    }
                    "sleep" => body.sleep(),
                    "wake" => body.wake_up(true),
                    _ => bail!("unknown physics body action '{action}'"),
                }
                if action == "teleport" {
                    native.queries_dirty = true;
                    self.write_poses(world)?;
                }
                world
                    .rigid_body_states
                    .insert(id.index, self.body_state(id)?);
                Ok(Value::Null)
            }
            Request::CastRay {
                world: target,
                origin,
                direction,
                max_distance,
                solid,
                groups,
                flags,
                exclude_body,
                exclude_collider,
            } => {
                let id = entity(&target)?;
                let direction = vec3(direction)?;
                ensure!(
                    direction.length_squared().is_finite() && direction.length_squared() > 1.0e-12,
                    "ray direction must be nonzero"
                );
                ensure!(
                    max_distance.is_finite() && max_distance >= 0.0,
                    "maxDistance must be nonnegative and finite"
                );
                let origin = vec3(origin)?;
                let exclude_body = exclude_body
                    .as_ref()
                    .map(|reference| {
                        let entry = self
                            .bodies
                            .get(&entity(reference)?)
                            .context("excluded body is inactive")?;
                        ensure!(
                            entry.world == id,
                            "excluded body belongs to a different world"
                        );
                        Ok::<_, anyhow::Error>(entry.handle)
                    })
                    .transpose()?;
                let exclude_collider = exclude_collider
                    .as_ref()
                    .map(|reference| {
                        let entry = self
                            .colliders
                            .get(&entity(reference)?)
                            .context("excluded collider is inactive")?;
                        ensure!(
                            entry.world == id,
                            "excluded collider belongs to a different world"
                        );
                        Ok::<_, anyhow::Error>(entry.handle)
                    })
                    .transpose()?;
                let native = self
                    .worlds
                    .get_mut(&id)
                    .context("entity has no active physics world")?;
                // Scene queries have their own BVH. CollisionPipeline::step consumes body
                // modification flags without registering new solver islands in Rapier 0.36.1.
                // Keeping queries read-only with respect to those flags also avoids altering
                // contact/joint caches merely because a mod requested a ray cast.
                if native.queries_dirty {
                    let poses: Vec<_> = native
                        .native
                        .colliders
                        .iter()
                        .filter_map(|(handle, collider)| {
                            let body = native.native.bodies.get(collider.parent()?)?;
                            let pose = *body.position() * (*collider.position_wrt_parent()?);
                            (pose != *collider.position()).then_some((handle, pose))
                        })
                        .collect();
                    for (handle, pose) in poses {
                        if let Some(collider) = native.native.colliders.get_mut(handle) {
                            collider.set_position(pose);
                        }
                    }
                    let leaves = native
                        .native
                        .colliders
                        .iter()
                        .filter(|(handle, collider)| {
                            native
                                .collider_ids
                                .get(handle)
                                .and_then(|id| self.colliders.get(id))
                                .is_some_and(|entry| entry.data["enabled"] == true)
                                && collider.parent().is_none_or(|handle| {
                                    native
                                        .native
                                        .bodies
                                        .get(handle)
                                        .is_some_and(|body| body.is_enabled())
                                })
                        })
                        .map(|(handle, collider)| {
                            (handle.into_raw_parts().0 as usize, collider.compute_aabb())
                        });
                    native.query_bvh = Bvh::from_iter(BvhBuildStrategy::default(), leaves);
                    native.queries_dirty = false;
                }
                let filter = QueryFilter {
                    flags: QueryFilterFlags::from_bits(flags)
                        .context("invalid query filter flags")?,
                    groups: groups.map(|g| g.native()),
                    exclude_rigid_body: exclude_body,
                    exclude_collider,
                    predicate: None,
                };
                let ray = Ray::new(origin, direction.normalize());
                let pipeline = QueryPipeline {
                    dispatcher: native.native.narrow_phase.query_dispatcher(),
                    bvh: &native.query_bvh,
                    bodies: &native.native.bodies,
                    colliders: &native.native.colliders,
                    filter,
                };
                let hit = pipeline.cast_ray_and_get_normal(&ray, max_distance, solid);
                Ok(match hit {
                    Some((handle, hit)) => {
                        let collider = *native
                            .collider_ids
                            .get(&handle)
                            .context("unmapped native collider")?;
                        let body = self
                            .colliders
                            .get(&collider)
                            .and_then(|entry| entry.body)
                            .map(reference);
                        json!({"collider":reference(collider),"body":body,"distance":hit.time_of_impact,"point":ray.point_at(hit.time_of_impact).to_array(),"normal":hit.normal.to_array()})
                    }
                    None => Value::Null,
                })
            }
        }
    }
}

#[derive(Deserialize)]
#[serde(tag = "op", rename_all = "camelCase", deny_unknown_fields)]
enum Request {
    WorldEntity {
        world: Value,
    },
    State {
        entity: Value,
    },
    Body {
        entity: Value,
        action: String,
        #[serde(default)]
        vector: Option<[f32; 3]>,
        #[serde(default)]
        point: Option<[f32; 3]>,
        #[serde(default)]
        rotation: Option<[f32; 4]>,
        #[serde(default = "yes")]
        wake: bool,
        #[serde(default)]
        axes: Option<Vec<String>>,
    },
    CastRay {
        world: Value,
        origin: [f32; 3],
        direction: [f32; 3],
        #[serde(rename = "maxDistance")]
        max_distance: f32,
        #[serde(default = "yes")]
        solid: bool,
        #[serde(default)]
        groups: Option<Groups>,
        #[serde(default)]
        flags: u32,
        #[serde(default, rename = "excludeBody")]
        exclude_body: Option<Value>,
        #[serde(default, rename = "excludeCollider")]
        exclude_collider: Option<Value>,
    },
}
fn yes() -> bool {
    true
}
