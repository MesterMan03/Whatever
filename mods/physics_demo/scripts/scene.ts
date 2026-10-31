import { BuiltInComponents, Physics, Scene, type Entity } from "@whatever-engine/api";

export type DemoScene = {
  entities: Entity[];
  world: Entity;
  player: Entity;
  model: Entity;
  orbit: Entity;
  camera: Entity;
  random: () => number;
};

/** Seeded randomness belongs to the mod; the engine has no playground/player behavior. */
function randomSource(seed: number): () => number {
  let state = seed >>> 0;
  return () => {
    state = (Math.imul(state, 1664525) + 1013904223) >>> 0;
    return state / 4294967296;
  };
}

/** Load authored geometry, hierarchy and physics; scripts only own behavior and procedural variation. */
export async function createDemoScene(
  seed?: number,
  path = "physics_demo://playground",
): Promise<DemoScene> {
  const loaded = await Scene.loadScene(path);
  const scene: DemoScene = {
    entities: loaded.entities(),
    world: loaded.get("world"),
    player: loaded.get("player"),
    model: loaded.get("model"),
    orbit: loaded.get("orbit"),
    camera: loaded.get("camera"),
    random: randomSource(seed ?? Math.floor(Math.random() * 4294967296)),
  };
  // Plain start honors the editor's layout. Explicit seeds request procedural rearrangement.
  if (seed !== undefined) {
    for (let index = 0; index < 24; index++) configureCube(loaded.get(`cube_${index}`), scene);
  }
  return scene;
}

function configureCube(entity: Entity, scene: DemoScene): void {
  const size = 0.65 + scene.random() * 0.65;
  const position: Physics.Vec3 = [
    -4.5 + scene.random() * 9,
    1.5 + scene.random() * 6,
    -6 + scene.random() * 9,
  ];
  entity.setComponent(
    new BuiltInComponents.Transform({ position, scale: [size, size, size] }).setEulerRadians(
      scene.random() * 0.4,
      scene.random() * Math.PI,
      scene.random() * 0.4,
    ),
  );
  entity.setComponent(
    new Physics.Collider({
      shape: Physics.Shape.box({ size: [size, size, size], borderRadius: 0.025 }),
      mass: 3 + size * 3,
      friction: 0.65,
      restitution: 0.15,
    }),
  );
}
export function spawnCubes(scene: DemoScene, count: number): void {
  for (let index = 0; index < count; index++) {
    const size = 0.65 + scene.random() * 0.65;
    const position: Physics.Vec3 = [
      -4.5 + scene.random() * 9,
      1.5 + scene.random() * 6,
      -6 + scene.random() * 9,
    ];
    const transform = new BuiltInComponents.Transform({
      position,
      scale: [size, size, size],
    }).setEulerRadians(scene.random() * 0.4, scene.random() * Math.PI, scene.random() * 0.4);
    scene.entities.push(
      Scene.createEntity()
        .setComponent(transform)
        .setComponent(
          new BuiltInComponents.MeshRenderer({
            mesh: "physics_demo://meshes/cube.obj",
            shader: "physics_demo://shaders/cube.wgsl",
          }),
        )
        .setComponent(Physics.RigidBody.dynamic({ world: scene.world, ccd: true }))
        .setComponent(
          new Physics.Collider({
            shape: Physics.Shape.box({ size: [size, size, size], borderRadius: 0.025 }),
            mass: 3 + size * 3,
            friction: 0.65,
            restitution: 0.15,
          }),
        )
        .setComponent("physics_demo:cube", {}),
    );
  }
}

export function destroyDemoScene(scene: DemoScene): void {
  // Dependents first, world last. No entity outside this mod's scene is destroyed.
  for (const entity of [...scene.entities].reverse()) entity.destroy();
  scene.entities.length = 0;
}
