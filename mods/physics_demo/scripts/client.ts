import { BuiltInComponents, Console, Engine, Physics, type Entity } from "@whatever-engine/api";
import { createDemoScene, destroyDemoScene, spawnCubes, type DemoScene } from "./scene.ts";

let scene: DemoScene | undefined;
let previousCamera: Entity | null = null;
let generation = 0;
let starting = false;
let yaw = 0;
let pitch = -0.3;
let resetHeld = false;
const distance = 6;

function updateCamera(): void {
  if (!scene) return;
  scene.orbit.setComponent(
    new BuiltInComponents.Transform({ position: [0, 0.45, 0] }).setEulerRadians(0, yaw, 0),
  );
  scene.camera.setComponent(
    new BuiltInComponents.Transform({
      position: [0, -Math.sin(pitch) * distance, Math.cos(pitch) * distance],
    }).setEulerRadians(pitch, 0, 0),
  );
}
async function start(seed: number): Promise<string> {
  if (scene || starting) return "Physics playground is already active. Use physicsdemo stop first.";
  const token = ++generation;
  starting = true;
  try {
    const camera = await Engine.getMainCamera().get();
    if (token !== generation) return "Physics playground start cancelled.";
    previousCamera = camera;
    yaw = 0;
    pitch = -0.3;
    resetHeld = false;
    scene = createDemoScene(seed);
    updateCamera();
    // Pending camera references resolve inside this same command batch.
    Engine.setMainCamera(scene.camera);
    await scene.player.get();
    if (token !== generation) return "Physics playground start cancelled.";
    return `Physics playground ready (seed ${seed}). WASD walk, Shift sprint, mouse orbit, R respawn. Click the window to capture the mouse; Escape releases it. physicsdemo cubes [count] adds cubes; physicsdemo stop restores the previous camera.`;
  } finally {
    if (token === generation) starting = false;
  }
}
function stop(): string {
  generation++;
  starting = false;
  if (!scene) return "Physics playground is inactive.";
  destroyDemoScene(scene);
  scene = undefined;
  if (previousCamera) Engine.setMainCamera(previousCamera);
  else Engine.clearCamera();
  previousCamera = null;
  return "Physics playground stopped; previous camera restored.";
}
Console.register({
  name: "physicsdemo",
  description: "Components-first Rapier playground",
  handler: () => "Use physicsdemo start [seed], cubes [count], or stop",
  subcommands: [
    {
      name: "start",
      description: "Spawn the floor, bean, orbit camera, cubes and hinged gate",
      args: [{ name: "seed", type: "int" }],
      handler: ({ seed }) =>
        start(typeof seed === "number" ? seed : Math.floor(Math.random() * 4294967296)),
    },
    {
      name: "stop",
      description: "Remove the playground and restore the previous camera",
      handler: stop,
    },
    {
      name: "cubes",
      description: "Drop more cubes into the active playground",
      args: [{ name: "count", type: "int" }],
      handler: ({ count }) => {
        if (!scene || starting) return "Start the playground first: physicsdemo start";
        const amount = typeof count === "number" ? count : 12;
        if (amount < 1 || amount > 200) return "Count must be between 1 and 200.";
        spawnCubes(scene, amount);
        return `Dropped ${amount} cubes.`;
      },
    },
  ],
});
Engine.on("tick", ({ keys_pressed, mouse_delta }) => {
  if (!scene || starting) return;
  yaw -= mouse_delta[0] * 0.002;
  pitch = Math.max(-1.1, Math.min(-0.08, pitch - mouse_delta[1] * 0.002));
  const forward = (keys_pressed.includes("KeyW") ? 1 : 0) - (keys_pressed.includes("KeyS") ? 1 : 0);
  const strafe = (keys_pressed.includes("KeyD") ? 1 : 0) - (keys_pressed.includes("KeyA") ? 1 : 0);
  let x = -Math.sin(yaw) * forward + Math.cos(yaw) * strafe;
  let z = -Math.cos(yaw) * forward - Math.sin(yaw) * strafe;
  const length = Math.hypot(x, z);
  if (length > 0) {
    x /= length;
    z /= length;
    scene.model.setComponent(
      new BuiltInComponents.Transform().setEulerRadians(0, Math.atan2(-x, -z), 0),
    );
  }
  const speed = keys_pressed.includes("ShiftLeft") || keys_pressed.includes("ShiftRight") ? 7 : 4;
  Physics.body(scene.player).setLinearVelocity([x * speed, 0, z * speed], { axes: ["x", "z"] });
  const reset = keys_pressed.includes("KeyR");
  if (reset && !resetHeld)
    Physics.body(scene.player)
      .teleport([0, 1.1, 6])
      .setLinearVelocity([0, 0, 0])
      .setAngularVelocity([0, 0, 0]);
  resetHeld = reset;
  updateCamera();
});
Engine.on("init", () =>
  Engine.log(
    "info",
    "Physics playground available: physicsdemo start (F2 console). Nothing spawns until activated.",
  ),
);
