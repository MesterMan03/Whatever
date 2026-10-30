import { BuiltInComponents, Engine, Entity, EntityRef, PendingEntity, Physics, Scene, type Pending } from "@whatever-engine/api";

const parent = Scene.createEntity();
const child = Scene.createEntity()
  .setComponent(new BuiltInComponents.Transform())
  .setParent(parent)
  .move([1, 2, 3]);

const pendingEntity: Pending<Entity> = child;
const entity: Promise<Entity> = child.get();
const transform: Pending<BuiltInComponents.Transform | null> = child.getComponent("core:transform");
const selected: Promise<BuiltInComponents.Transform | null> = Scene.createEntity()
  .setComponent(new BuiltInComponents.Transform())
  .getComponent("core:transform")
  .get();
const children: Pending<Entity[]> = child.getChildren();
const list: Promise<Entity[]> = Scene.listEntities().get();
const sprite: PendingEntity = Scene.spawnSprite("core://sprite.png", "core://sprite.wgsl", [0, 0, 0]);

// @ts-expect-error Explicit get() is required to obtain a promise.
const wrongPromise: Promise<Entity> = child;
// @ts-expect-error A component query selects the component, not the original entity.
const wrongResult: Promise<Entity> = transform.get();
// @ts-expect-error Built-in component setters retain their argument types.
child.setComponent("core:transform", { position: "invalid" });
// @ts-expect-error Result handles do not expose Entity methods.
transform.setParent(null);

void [pendingEntity, entity, selected, children, list, sprite, wrongPromise, wrongResult];

const physicsWorld=Scene.createEntity().setComponent(new Physics.World({substeps:2,rapier:{integration:{num_solver_iterations:8,contact_softness:{natural_frequency:30}}}}));
const rigidBody=Scene.createEntity().setComponent(Physics.RigidBody.dynamic({world:physicsWorld,lockedAxes:["rotationX"],rapier:{additional_solver_iterations:4}}));
const collider=Scene.createEntity().setComponent(new Physics.Collider({body:rigidBody,shape:Physics.Shape.box({size:[1,2,3]}),mass:2}));
const hinge=Scene.createEntity().setComponent(Physics.Joint.revolute({bodyA:rigidBody,bodyB:parent,limits:{min:-1,max:1},rapier:{motors:{angularX:{targetVelocity:1}}}}));
const bodyState:Pending<Physics.BodyState>=Physics.body(rigidBody).state();
const componentState:Pending<Physics.BodyState|null>=rigidBody.getComponent("core:rigid_body_state");
const settings:Pending<Physics.World|null>=physicsWorld.getComponent("core:physics_world");
const ray:Pending<Physics.RayHit|null>=Physics.world(physicsWorld).castRay([0,10,0],[0,-1,0],{maxDistance:20,excludeCollider:collider});
const mutations:Promise<void>=Physics.body(rigidBody).applyImpulse([1,0,0]).setLinearVelocity([1,0,1],{axes:["x","z"]}).get();
const previousCamera:Pending<Entity|null>=Engine.getMainCamera();Engine.setMainCamera(parent);
child.setComponent("fixture:target",{target:rigidBody,nested:[Scene.ref(parent),new EntityRef("2:5")]});
// @ts-expect-error Native state components cannot be set through the raw setter.
child.setComponent("core:rigid_body_state",{mass:3});
// @ts-expect-error Known physics components retain their configuration types.
child.setComponent("core:collider",{shape:Physics.Shape.sphere("bad")});
// @ts-expect-error Native tuning options are typed, including spelling.
new Physics.World({rapier:{integration:{num_solver_interations:4}}});
// @ts-expect-error References are entities or opaque IDs, not arbitrary objects.
new Physics.Collider({body:{index:2},shape:Physics.Shape.sphere(1)});
Physics.onEvent(event=>{const entity:Entity=event.colliderA;void entity;});
Physics.onEvent(event=>{if(event.kind==="contactForce"){const force:Physics.Vec3=event.totalForce;void force;}});
declare module "@whatever-engine/api" {
  interface ComponentRegistry { "fixture:typed_ref":{target:Entity} }
  interface ComponentSetRegistry { "fixture:typed_ref":{target:Physics.EntityInput} }
}
child.setComponent("fixture:typed_ref",{target:parent});
const typedRef:Pending<{target:Entity}|null>=child.getComponent("fixture:typed_ref");
child.setParent(Scene.ref(parent));
const editableBody=Physics.RigidBody.dynamic();editableBody.world=physicsWorld;
const editableCollider=new Physics.Collider({shape:Physics.Shape.sphere(1)});editableCollider.body=parent;
const editableJoint=Physics.Joint.fixed({bodyA:parent,bodyB:child});editableJoint.bodyB=Scene.ref(rigidBody);
Physics.body(editableCollider.body).wake();
void typedRef;
void [hinge,bodyState,componentState,settings,ray,mutations,previousCamera];
