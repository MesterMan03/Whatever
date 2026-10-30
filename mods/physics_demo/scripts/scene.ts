import { BuiltInComponents, Physics, Scene, type PendingEntity } from "@whatever-engine/api";

export type DemoScene = {
  entities: PendingEntity[];
  world: PendingEntity;
  player: PendingEntity;
  model: PendingEntity;
  orbit: PendingEntity;
  camera: PendingEntity;
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

export function createDemoScene(seed: number): DemoScene {
  const entities: PendingEntity[] = [];
  const keep = (entity: PendingEntity) => {
    entities.push(entity);
    return entity;
  };
  const world = keep(
    Scene.createEntity().setComponent(
      new Physics.World({
        substeps: 2,
        rapier: { integration: { num_solver_iterations: 8, max_ccd_substeps: 4 } },
      }),
    ),
  );
  keep(
    Scene.spawnMesh(
      "physics_demo://meshes/cube.obj",
      "physics_demo://shaders/floor.wgsl",
      [0, -0.5, 0],
      { scale: [60, 1, 60] },
    )
      .setComponent(Physics.RigidBody.fixed({ world }))
      .setComponent(
        new Physics.Collider({ shape: Physics.Shape.box({ size: [60, 1, 60] }), friction: 0.75 }),
      ),
  );
  keep(
    Scene.createEntity().setComponent(
      new BuiltInComponents.AmbientLight({ color: [0.8, 0.88, 1], intensity: 0.35 }),
    ),
  );
  keep(
    Scene.createEntity().setComponent(
      new BuiltInComponents.DirectionalLight({
        direction: [0.4, 1, 0.6],
        color: [1, 0.96, 0.88],
        intensity: 1.4,
      }),
    ),
  );

  const player = keep(
    Scene.createEntity()
      .move([0, 1.1, 6])
      .setComponent(
        Physics.RigidBody.dynamic({
          world,
          ccd: true,
          canSleep: false,
          lockedAxes: ["rotationX", "rotationY", "rotationZ"],
        }),
      )
      .setComponent(
        new Physics.Collider({
          shape: Physics.Shape.capsule({ radius: 0.42, height: 2 }),
          mass: 75,
          friction: 0,
          restitution: 0,
        }),
      )
      .setComponent("physics_demo:player", {}),
  );
  const model = keep(
    Scene.spawnMesh(
      "physics_demo://meshes/bean.obj",
      "physics_demo://shaders/bean.wgsl",
      [0, 0, 0],
    ).setParent(player),
  );
  for (const x of [-0.14, 0.14]) {
    keep(
      Scene.spawnMesh(
        "physics_demo://meshes/sphere.obj",
        "physics_demo://shaders/eye.wgsl",
        [x, 0.4, -0.41],
        { scale: [0.09, 0.12, 0.05] },
      ).setParent(model),
    );
    keep(
      Scene.spawnMesh(
        "physics_demo://meshes/sphere.obj",
        "physics_demo://shaders/pupil.wgsl",
        [x, 0.4, -0.46],
        { scale: [0.04, 0.065, 0.02] },
      ).setParent(model),
    );
  }
  const orbit = keep(Scene.createEntity().move([0, 0.45, 0]).setParent(player));
  const camera = keep(
    Scene.createEntity()
      .setParent(orbit)
      .setComponent(new BuiltInComponents.Transform({ position: [0, 2, 6] }))
      .setComponent(new BuiltInComponents.Camera({ fovy_degrees: 65, znear: 0.05, zfar: 200 })),
  );

  // A fixed frame has three explicitly attached collider entities. Parenting here
  // controls rendering; the collider's `body` reference controls physics ownership.
  const frame = keep(
    Scene.createEntity().move([-7, 0, -3]).setComponent(Physics.RigidBody.fixed({ world })),
  );
  const framePart = (position: Physics.Vec3, size: Physics.Vec3) =>
    keep(
      Scene.spawnMesh(
        "physics_demo://meshes/cube.obj",
        "physics_demo://shaders/pupil.wgsl",
        position,
        { scale: size },
      )
        .setParent(frame)
        .setComponent(
          new Physics.Collider({ body: frame, position, shape: Physics.Shape.box({ size }) }),
        ),
    );
  framePart([0, 1.5, 0], [0.2, 3, 0.25]);
  framePart([2.8, 1.5, 0], [0.2, 3, 0.25]);
  framePart([1.4, 3, 0], [3, 0.2, 0.25]);
  const gate = keep(
    Scene.spawnMesh(
      "physics_demo://meshes/cube.obj",
      "physics_demo://shaders/cube.wgsl",
      [-5.6, 1.4, -3],
      { scale: [2.5, 2.5, 0.15] },
    )
      .setComponent(Physics.RigidBody.dynamic({ world, ccd: true, angularDamping: 0.3 }))
      .setComponent(
        new Physics.Collider({ shape: Physics.Shape.box({ size: [2.5, 2.5, 0.15] }), mass: 12 }),
      ),
  );
  keep(
    Scene.createEntity().setComponent(
      Physics.Joint.revolute({
        bodyA: frame,
        bodyB: gate,
        anchorA: [0.15, 1.4, 0],
        anchorB: [-1.25, 0, 0],
        axisA: [0, 1, 0],
        axisB: [0, 1, 0],
        limits: { min: -1.4, max: 1.4 },
      }),
    ),
  );

  const scene = { entities, world, player, model, orbit, camera, random: randomSource(seed) };
  spawnCubes(scene, 24);
  return scene;
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
