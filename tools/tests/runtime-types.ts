import { Component, Type, field, BuiltInComponents, Engine, Entity, EntityRef, PendingEntity, Physics, Scene, type Pending, type ComponentSnapshot } from "@whatever-engine/api";

const loadedScene: Promise<import("@whatever-engine/api").LoadedScene> = Scene.loadScene("fixture://main");
void loadedScene.then(scene => {
  const entity: Entity = scene.get("camera");
  const entities: Entity[] = scene.entities();
  const count: number = scene.entityCount;
  const time: number = scene.loadTime;
  void [entity, entities, count, time];
});

const parent = Scene.createEntity();
const child = Scene.createEntity()
  .setComponent(new BuiltInComponents.Transform())
  .setParent(parent)
  .move([1, 2, 3]);

const pendingEntity: Pending<Entity> = child;
const entity: Promise<Entity> = child.get();
const transform: ComponentSnapshot<BuiltInComponents.Transform> | null = child.getComponent("core:transform");
const selected: ComponentSnapshot<BuiltInComponents.Transform> | null = Scene.createEntity()
  .setComponent(new BuiltInComponents.Transform())
  .getComponent("core:transform");
const children: readonly Entity[] = child.getChildren();
const list: readonly Entity[] = Scene.listEntities();
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
const componentState:ComponentSnapshot<Physics.BodyState>|null=rigidBody.getComponent("core:rigid_body_state");
const settings:ComponentSnapshot<Physics.World>|null=physicsWorld.getComponent("core:physics_world");
const ray:Pending<Physics.RayHit|null>=Physics.world(physicsWorld).castRay([0,10,0],[0,-1,0],{maxDistance:20,excludeCollider:collider});
const mutations:Promise<void>=Physics.body(rigidBody).applyImpulse([1,0,0]).setLinearVelocity([1,0,1],{axes:["x","z"]}).get();
const previousCamera:Entity|null=Engine.getMainCamera();Engine.setMainCamera(parent);
export class Target extends Component {
  static readonly type = "fixture:target";
  target: Entity | null = null;
  count = field(Type.I32.with({ min: 0 }), 1);
  override tick(delta: number): void { this.count += 1; }
}
const target = new Target(); target.target = rigidBody;
child.setComponent(target);
const targetResult: ComponentSnapshot<Target> | null = child.getComponent(Target);
// @ts-expect-error Arbitrary component names and JSON blobs are not component instances.
child.setComponent("fixture:random", {});
// @ts-expect-error Custom component lookups require the registered class.
child.getComponent("fixture:random");
const tags: readonly string[] = child.addTag("enemy").removeTag("dead").getTags();
const tagged: readonly Entity[] = Scene.findByTags({ all: ["enemy"], any: ["flying", "walking"], none: ["dead"] });
const hasTag: boolean = child.hasTag("enemy");
void [tags, tagged, hasTag, targetResult];
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
const typedRef: ComponentSnapshot<Target> | null = child.getComponent(Target);
child.setParent(Scene.ref(parent));
const editableBody=Physics.RigidBody.dynamic();editableBody.world=physicsWorld;
const editableCollider=new Physics.Collider({shape:Physics.Shape.sphere(1)});editableCollider.body=parent;
const editableJoint=Physics.Joint.fixed({bodyA:parent,bodyB:child});editableJoint.bodyB=Scene.ref(rigidBody);
Physics.body(editableCollider.body).wake();
void typedRef;
void [hinge,bodyState,componentState,settings,ray,mutations,previousCamera];

// @ts-expect-error Snapshot tuples cannot be mutated.
if (transform) transform.position[0] = 2;
// @ts-expect-error Snapshot data has no mutable class helper methods.
transform?.setX(2);
child.editComponent("core:transform", draft => { draft.position[0] = 2; });
const ordered: Pending<BuiltInComponents.Transform | null> = child.requestComponent("core:transform");
void ordered;
