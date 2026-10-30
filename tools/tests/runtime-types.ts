import { BuiltInComponents, Entity, PendingEntity, Scene, type Pending } from "@whatever-engine/api";

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
