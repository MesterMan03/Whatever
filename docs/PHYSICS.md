# Components-first physics

Whatever 0.6.0 runs Rapier **3D 0.36.1** natively on the simulation worker. Mods define
persistent ECS configuration and enqueue operations; JavaScript does no collision
detection or integration. The player, camera controller and playground are mod code.

```ts
import { Engine, Physics, Scene } from "@whatever-engine/api";

const ball = Scene.createEntity().move([0, 5, 0])
  .setComponent(Physics.RigidBody.dynamic({ ccd: true }))
  .setComponent(new Physics.Collider({ shape: Physics.Shape.sphere(0.5), mass: 2 }));

const impulse = Physics.body(ball).applyImpulse([0, 3, 0]);
const selectedState = Physics.body(ball).state();
const [entity, state] = await Promise.all([ball.get(), selectedState.get(), impulse.get()]);
```

This queues creation, configuration, impulse and state selection in one batch. The
state query captures the pre-step state after the impulse. Results arrive after the
physics phase; follow-up work belongs to a later tick. `.get()` never flushes commands.
`Physics.body(entity)` binds operations to a reference; it does not add a body component.

## ECS components and references

| Component ID | Data / class | Meaning |
|---|---|---|
| `core:physics_world` | `Physics.World` | Independent native simulation and settings |
| `core:rigid_body` | `Physics.RigidBody` | Body configuration on an object entity |
| `core:collider` | `Physics.Collider` | One collision shape per entity |
| `core:joint` | `Physics.Joint` | One relationship between two body entities |
| `core:rigid_body_state` | `Physics.BodyState` | Native read-only body snapshot |
| `core:physics_status` | `Physics.Status` | Read-only active/error status per physics component |

The six IDs are also `Physics.WORLD_ID`, `BODY_ID`, `COLLIDER_ID`, `JOINT_ID`,
`BODY_STATE_ID`, and `STATUS_ID`. ECS queries include them normally. Configuration
reads hydrate the corresponding classes; states/statuses are plain snapshots.

Reference fields accept `Entity`, `PendingEntity`, `EntityRef`, or opaque ID strings
(`Physics.EntityInput`). Pending references resolve within their originating mod's
batch. Stored references contain real generational IDs; destroying an entity never
retargets a stored reference to another entity reusing its slot.

Use entities directly inside custom component data, or `Scene.ref(entityOrId)` /
`new EntityRef(entityOrId)`. Nested references also work. Reads hydrate references
to `Entity` objects; physics configuration classes wrap their fields in `EntityRef`,
which exposes `.id`. Reading an unresolved reference's ID throws; recording it does not.
When editing reference properties on an existing component, assign an entity/reference;
wrap raw ID strings with `Scene.ref(id)`. References also work as scene parents and cameras.
The reserved wire object `{ "$entity": "..." }` is private to ECS serialization.
Ordinary strings are never interpreted as embedded references.

Extend the public `ComponentRegistry` and `ComponentSetRegistry` interfaces to type
custom component reads and writes. Read types may use `Entity`; input types may use
`Physics.EntityInput`. Only the reserved reference representation receives native resolution.

Removing a physics component removes its native object. Removing a body also removes
dependent native colliders/joints, while their ECS components remain inspectable and
inactive. Re-adding the body on the same live entity reactivates dependencies. Missing
worlds, missing bodies, invalid parent scales and incompatible joint endpoints appear
in `core:physics_status`. Removing a world makes its dependents inactive. Invalid
configuration edits are logged and leave the previous component intact. Native body
state and status cannot be set or removed by mods.

Configuration edits preserve motion and sleeping state; moving a body to another live
world preserves its velocity and forces. Type changes follow native body-type semantics.
Impulse joints update in place; multibody configuration changes rebuild the affected
articulation joint because its generalized coordinates may change.

## Bodies and motion

```ts
Physics.RigidBody.dynamic(options?)
Physics.RigidBody.fixed(options?)
Physics.RigidBody.kinematicPosition(options?)
Physics.RigidBody.kinematicVelocity(options?)
new Physics.RigidBody({ type: "dynamic", ...options })
```

`Physics.BodyOptions` contains `world`, `gravityScale` (1), `linearDamping` (0),
`angularDamping` (0), `ccd` (false), `canSleep` (true), `enabled` (true), `lockedAxes`
(empty), and `rapier`. Locked axes are `translationX/Y/Z` and `rotationX/Y/Z`.
Omitting `world` selects the default world. Bodies receive a default transform if needed.

Dynamic bodies write their world pose into ECS after stepping. Parented body poses
convert back to local coordinates, with physical parents updated before children.
An explicit transform change or `entity.move()` teleports the body; ordinary movement
should use velocity/force or kinematic targets. Transform scale changes visual geometry;
set collider shape dimensions explicitly to resize physics geometry.

`Physics.body(reference)` returns a chainable `Physics.BodyHandle`:

- `applyImpulse(vector, { point?, wake? }?)`, `applyTorqueImpulse(vector, wake?)`
- `addForce(vector, { point?, wake? }?)`, `addTorque(vector, wake?)`
- `resetForces(wake?)`, `resetTorques(wake?)`
- `setLinearVelocity(vector, { axes?: ("x" | "y" | "z")[], wake? }?)`
- `setAngularVelocity(vector, wake?)`
- `teleport(position, { rotation?, wake? }?)`
- `setKinematicTarget(position, rotation?)`
- `sleep()`, `wake()`
- `get(): Promise<void>` waits for mutations recorded on this handle so far and reports
  native validation errors. Repeated calls return the same promise until another mutation
  is recorded on this handle; previously obtained promises keep their original boundary.
- `state(): Pending<Physics.BodyState>` selects state at this command's position.

Vectors and points are world-space. Angular velocities use radians/second; quaternion
arrays use **xyzw**. `wake` defaults to true. Forces accumulate until reset, while an
impulse changes velocity immediately. Select velocity axes to preserve the others:
`setLinearVelocity([x, 0, z], { axes: ["x", "z"] })` steers without cancelling gravity.
Position-based kinematic targets interpolate across world substeps; teleporting preserves
velocity and does not represent continuous kinematic movement.

`BodyState` has `world: Entity`, `position`, `rotation`, `linearVelocity`, `angularVelocity`,
`sleeping`, `mass`, `centerOfMass`, and `tickNumber`. The latter is the world's last
completed solver tick; ordered command mutations may already be reflected. Use normal
`getComponent("core:rigid_body_state")` for ECS access to the same view. Native snapshots
stay in compact Rust storage and serialize only when explicitly requested.

## Colliders and compound objects

```ts
const car = Scene.createEntity().move([0, 2, 0])
  .setComponent(Physics.RigidBody.dynamic());
const chassis = Scene.createEntity().setComponent(new Physics.Collider({
  body: car, shape: Physics.Shape.box({ size: [2, 0.5, 4] }), mass: 800,
}));
const roof = Scene.createEntity().setComponent(new Physics.Collider({
  body: car, position: [0, 0.55, 0],
  shape: Physics.Shape.box({ size: [1.6, 0.6, 2] }), mass: 100,
}));
const sensor = Scene.createEntity().setComponent(new Physics.Collider({
  body: car, shape: Physics.Shape.sphere(5), sensor: true, density: 0,
  events: ["collision"],
})).setComponent("game:proximity_sensor", {});
```

These are one native body and three independently queryable/removable collider entities.
Collider ownership is explicit and independent of scene parenting:

- Omitted `body`: attach to the same entity's body.
- `body: anotherEntity`: attach to that body; `position` and `rotation` are body-local offsets.
- `body: null`: standalone fixed collider; use the entity's world pose plus its offset.

An attached collider inherits its body's world. An optional explicit `world` must match.
Standalone colliders may select `world`; omission selects the default world. A collider
entity's own transform does not reposition an explicitly attached collider. Parent that
entity for rendering if desired and set the collider offset separately.

`ColliderOptions` includes `shape`, `body`, `world`, `position` ([0,0,0]), `rotation`
(identity), `mass`, `density`, `friction` (0.5), `restitution` (0), `sensor` (false),
`enabled` (true), `events` (empty), and `rapier`. `mass` and `density` are mutually
exclusive; omission selects density 1. Explicit mass properties are a third exclusive
alternative. Sensors also contribute mass unless density/mass is set to zero.

Shape factories return `Physics.ShapeData`:

- `sphere(radius)`
- `box({ size: [width, height, depth], borderRadius? })`
- `capsule({ radius, height })` — Y-aligned; height includes both caps, at least `2 * radius`
- `cylinder({ radius, height })`, `cone({ radius, height })` — Y-aligned
- `plane(normal?)` — infinite half-space, default upward normal; use fixed/standalone geometry
- `convexHull(points)`
- `triMesh(vertices, triangleIndices, flags?)`
- `compound([{ shape, position?, rotation? }, ...])` — convex parts; nested compounds are rejected

Sizes are full dimensions. A box's border radius preserves those outside dimensions.
Triangle indices are triples of u32 indices; geometry must be nonempty and valid.
`Physics.TriMeshFlag` exposes the pinned native flag values, including internal-edge fixes.

## Joints

```ts
const frame = Scene.createEntity().setComponent(Physics.RigidBody.fixed());
const door = Scene.createEntity().move([0.5, 0, 0])
  .setComponent(Physics.RigidBody.dynamic())
  .setComponent(new Physics.Collider({
    shape: Physics.Shape.box({ size: [1, 2, 0.1] }), mass: 15,
  }));
const hinge = Scene.createEntity().setComponent(Physics.Joint.revolute({
  bodyA: frame, bodyB: door,
  anchorA: [0, 0, 0], anchorB: [-0.5, 0, 0],
  axisA: [0, 1, 0], axisB: [0, 1, 0],
  limits: { min: -Math.PI / 2, max: Math.PI / 2 },
})).setComponent("game:door_hinge", {});
```

Factories are `fixed`, `revolute`, `prismatic`, `spherical`, and `generic`; each takes
`Physics.JointOptions`. `new Physics.Joint({ type, ...options })` is equivalent.
`bodyA` and `bodyB` must be different active bodies in the same world. The joint entity's
transform is irrelevant. Removing it removes the joint without deleting either body.

Options include `solver: "impulse" | "multibody"` (impulse), `anchorA/B` (origin),
`rotationA/B` (identity), `axisA/B`, `contactsEnabled` (false), `enabled` (true), `limits`,
`motor`, and `rapier`. Anchors/orientations/axes are local to each body. Revolute and
prismatic axes default to each frame's X axis. Define each frame with either its full
rotation or an explicit axis; combining a non-identity rotation and explicit axis is
rejected. Fixed/spherical/generic joints use rotations rather than the axis shortcuts.

Friendly `limits: { min, max }` and `motor` configure the free axis of revolute/prismatic
joints. Angles are radians; prismatic distances are world units. `Motor` contains
`targetPosition` (0), `targetVelocity` (0), `stiffness` (0), `damping` (0), `maxForce`
(f32 maximum), and `model: "AccelerationBased" | "ForceBased"` (AccelerationBased).
Generic joints use per-axis settings under `rapier`. Friendly and low-level limits or
motors cannot both configure the same axis. Invalid multibody cycles/links are inactive
with a status diagnostic.

## World settings and native options

```ts
const arena = Scene.createEntity().setComponent(new Physics.World({
  dimension: "3d", gravity: [0, -9.81, 0], substeps: 2,
  rapier: { integration: {
    num_solver_iterations: 8, num_internal_pgs_iterations: 2, max_ccd_substeps: 4,
  } },
}));
const object = Scene.createEntity().setComponent(Physics.RigidBody.dynamic({
  world: arena, ccd: true, rapier: { additional_solver_iterations: 4 },
}));
```

`WorldOptions` includes `dimension` (3d), `gravity` ([0,-9.81,0]), `substeps` (1,
1–1024), `timeScale` (1; zero pauses integration), and `rapier.integration`.
Read/modify/set the world component to change settings. `Physics.defaultWorld` is a
lazy symbolic entity reference with normal component methods; `.get()` resolves its
actual entity. Its `.id` is the resource alias; use the resolved entity's ID for identity
comparisons. References stored in components are always resolved to actual generational IDs.

Each substep uses `engineTickDelta * timeScale / substeps`. Mods cannot override `dt`.
Changing the engine tick rate changes the next tick's interval; every participant in
the current tick uses the already sampled delta. Physics writes ECS poses before the
engine publishes the world/render snapshot and buffered results. Rendering FPS has no
effect on physics stepping.

Expert options are typed and use native snake_case. Unknown keys, invalid enum/flag
values, invalid references and illegal geometry are rejected. Configuration reads fill
defaults; calculated quantities such as effective body mass belong in `BodyState`.

| Scope | `rapier` configuration |
|---|---|
| World `integration` | `min_ccd_dt`, `contact_softness`, `static_contact_softness`, `warmstart_coefficient`, `length_unit`, `normalized_allowed_linear_error`, `normalized_max_corrective_velocity`, `normalized_prediction_distance`, `normalized_max_linear_velocity`, `num_solver_iterations`, `num_internal_pgs_iterations`, `num_internal_stabilization_iterations`, `max_ccd_substeps`, `contact_clustering`, `contact_recycling`, `normalized_contact_recycle_distance`, `friction_in_bias_pass`, `warmstart_joints`, `friction_model`, `soft_bodies` |
| Body | `additional_solver_iterations`, `additional_pgs_iterations`, `dominance_group`, `soft_ccd_prediction`, `allow_fast_rotation`, `gyroscopic_forces_enabled`, `additional_mass`, `additional_mass_properties`, `activation`, `user_data` |
| Collider | `friction_combine_rule`, `restitution_combine_rule`, `collision_groups`, `solver_groups`, `active_collision_types`, `contact_skin`, `contact_force_event_threshold`, `mass_properties`, `user_data` |
| Joint | `locked_axes`, `coupled_axes`, `limits`, `motors`, `softness`, `user_data` |

`Physics.IntegrationOptions`, `BodyRapierOptions`, `ColliderRapierOptions` and
`JointRapierOptions` define all supported fields. Spring coefficients are
`{ natural_frequency, damping_ratio }`; world patches may set either member. Friction
models are `Simplified` / `Coulomb`. Combine rules are `Average`, `Min`, `Multiply`, `Max`.
World `soft_bodies` retains the compiled backend's typed tuning structure for completeness;
there are no soft-body ECS components yet. The optional FEM backend is not compiled in.

`Physics.Groups` contains unsigned u32 `memberships` and `filter` masks (both all bits
by default), plus `test_mode: "And" | "Or"` (And). Collider collision-type bits are
available as `Physics.CollisionType`. Native mass properties use `Physics.MassProperties`
with `mass`, `center_of_mass`, `principal_angular_inertia`, and optional
`principal_inertia_local_frame` (xyzw identity). Additional body mass and additional
mass properties are exclusive. Body activation tuning contains
`normalized_linear_threshold`, `angular_threshold`, and `time_until_sleep`; negative
thresholds disable sleeping. `user_data` is a decimal **u128 string**, without JS precision loss.

Joint axes are `linearX/Y/Z` and `angularX/Y/Z`. Per-axis `rapier.limits` maps these
names to `{ min, max }`; `rapier.motors` maps them to `Physics.Motor`. `locked_axes`
overrides the factory's constraints, `coupled_axes` enables coupled degrees of freedom,
and `softness` controls constraint softness.

`Physics.capabilities` reports the backend version, dimensions, enhanced determinism,
parallel/SIMD solver features, soft-body support and JavaScript hooks. This build uses
enhanced determinism with serial execution, 3D only, and no JavaScript physics hooks.
Use collision/solver groups and native settings in the hot path; JS callbacks observe
events after the step. Reproducibility also requires the same commands, inputs, timing,
settings and backend version; arbitrary JS randomness/asynchrony is still game code.

## Queries and events

```ts
const hit = await Physics.world(arena).castRay([0, 10, 0], [0, -1, 0], {
  maxDistance: 100, excludeBody: object,
  flags: Physics.QueryFilterFlag.EXCLUDE_SENSORS,
}).get();
Physics.onEvent(event => {
  if (event.kind === "contactForce") Engine.log("info", String(event.totalForceMagnitude));
});
```

`Physics.world(reference?)` returns a `WorldHandle`; omission selects the default world.
`castRay(origin, direction, RayOptions)` returns `Pending<RayHit | null>`. Direction is
normalized; `maxDistance` is in world units. Options are `maxDistance`, `solid` (true),
`groups`, `flags`, `excludeBody`, and `excludeCollider`. Exclusions must be active in
the queried world. `QueryFilterFlag` exposes native fixed/kinematic/dynamic/sensor/solid
exclusions and the three body-type-only combinations. Hits contain `collider: Entity`,
`body: Entity | null`, `distance`, `point`, and `normal`.

Queries see earlier creation, geometry and pose commands without stepping physics or
consuming solver modification flags. A separate cached native BVH keeps queries from
altering collision detection and joint/contact solver caches.

Enable collider `events: ["collision", "contactForce"]` as needed, then subscribe with
`Physics.onEvent(handler)` or `Engine.on("physics", ({ event }) => ...)`. A collider may
enable either event independently; contact forces use its configured threshold. Events
contain `world`, `colliderA/B`, `tickNumber`, `substep`, `phase: "step"`, and either:

- `kind: "started" | "stopped"`, `sensor`, `removed`.
- `kind: "contactForce"`, `totalForce`, `totalForceMagnitude`, `maxForceDirection`, `maxForceMagnitude`.

Events are ordered by world and substep, then collider identities/kind. The force vector
uses Rapier's colliderA/colliderB orientation. Events publish after physics; handler
commands execute on a later tick. Collider identities remain generational on removal.

## Playground

The client-only `physics_demo` mod registers `physicsdemo start [seed]`, `cubes [count]`,
and `stop`. See [its controls and scene](../mods/physics_demo/README.md). It demonstrates
dynamic bodies, a compound frame, a joint entity, a per-scene world and camera parenting.
