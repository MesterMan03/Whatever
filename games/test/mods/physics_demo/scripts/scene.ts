import { BuiltInComponents, Physics, Scene, type Entity } from "@whatever-engine/api";

export type DemoScene = {
  entities: Entity[];
  world: Entity;
  player: Entity;
  model: Entity;
  orbit: Entity;
  camera: Entity;
  baseCube: Entity;
  spawnPoint: Physics.Vec3;
  random: () => number;
  collider: Physics.Collider;
  baseScale: Physics.Vec3;
};
function randomSource(seed: number): () => number {
  let state = seed >>> 0;
  return () => {
    state = (Math.imul(state, 1664525) + 1013904223) >>> 0;
    return state / 4294967296;
  };
}
let adoptedStartup = false;
const sceneKey = (path: string) => path.replace("://scenes/", "://").replace(/\.wscene$/, "");

/** Adopt the preloaded scene or load it, then instantiate the authored cube template. */
export async function createDemoScene(
  seed?: number,
  path = "physics_demo://playground",
): Promise<DemoScene> {
  const startup = Scene.initialScene();
  const adopt = !adoptedStartup && startup && sceneKey(startup.path) === sceneKey(path);
  const loaded = adopt ? startup : await Scene.loadScene(path);
  if (adopt) adoptedStartup = true;
  const [spawn, collider, baseTransform] = await Promise.all([
    loaded.get("cube_spawn").getComponent("core:world_transform"),
    loaded.get("base_cube").requestComponent("core:collider").get(),
    loaded.get("base_cube").getComponent("core:transform"),
  ]);
  if (!collider || !baseTransform) throw new Error("Base Cube needs a Collider and Transform");
  if (!spawn) throw new Error("Cube Spawn Point needs a Transform");
  const scene: DemoScene = {
    entities: loaded.entities(),
    world: loaded.get("world"),
    player: loaded.get("player"),
    model: loaded.get("model"),
    orbit: loaded.get("orbit"),
    camera: loaded.get("camera"),
    baseCube: loaded.get("base_cube"),
    spawnPoint: [spawn.position[0], spawn.position[1], spawn.position[2]],
    collider,
    baseScale: [...baseTransform.scale],
    random: randomSource(seed ?? Math.floor(Math.random() * 4294967296)),
  };
  spawnCubes(scene, 24);
  return scene;
}

export function spawnCubes(scene: DemoScene, count: number): void {
  for (let index = 0; index < count; index++) {
    const size = 0.65 + scene.random() * 0.65;
    const [x, y, z] = scene.spawnPoint;
    const position: Physics.Vec3 = [
      x + (scene.random() - 0.5) * 9,
      y + (scene.random() - 0.5) * 6,
      z + (scene.random() - 0.5) * 9,
    ];
    const transform = new BuiltInComponents.Transform({
      position,
      scale: scene.baseScale.map((value) => value * size) as Physics.Vec3,
    }).setEulerRadians(scene.random() * 0.4, scene.random() * Math.PI, scene.random() * 0.4);
    const cube = scene.baseCube
      .clone({ enabled: true })
      .setParent(null)
      .setComponent(transform)
      .removeTag("physics_demo:cube_template")
      .addTag("physics_demo:cube")
      .setComponent(
        new Physics.Collider({
          ...scene.collider,
          body: undefined,
          world: undefined,
          shape:
            scene.collider.shape.type === "box"
              ? {
                  ...scene.collider.shape,
                  size: scene.collider.shape.size.map((value) => value * size) as Physics.Vec3,
                  borderRadius: (scene.collider.shape.borderRadius ?? 0) * size,
                }
              : scene.collider.shape,
          mass: scene.collider.mass == null ? undefined : scene.collider.mass * size ** 3,
        }),
      );
    scene.entities.push(cube);
  }
}
export function destroyDemoScene(scene: DemoScene): void {
  for (const entity of [...scene.entities].reverse()) entity.destroy();
  scene.entities.length = 0;
}
