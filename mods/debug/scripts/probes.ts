import {
  Console,
  Engine,
  Entity,
  File,
  Message,
  Scene,
  type CommandSpec,
} from "@whatever-engine/api";
import { sandboxCommand } from "./sandbox.ts";

let traceTicks = false;
let tickDelay = 0;
Engine.on("tick", ({ tick_number, delta_seconds }) => {
  if (traceTicks)
    Engine.log("info", `Got tick event for #${tick_number}, delta ${delta_seconds} seconds`);
  // This diagnostic deliberately blocks submission. Async waits no longer stall ticks.
  const deadline = Date.now() + tickDelay;
  while (Date.now() < deadline) {}
});
Message.registerMessageHandler(({ message }) => ({ suite: "debug", received: message }));

export const commonCommands: CommandSpec[] = [
  {
    name: "echo",
    description: "Command parsing and typed argument probe",
    args: [{ name: "value", type: "string", required: true }],
    handler: ({ value }) => `You entered: ${value}`,
  },
  {
    name: "error",
    description: "Throw a deliberate command-handler error",
    handler: () => {
      throw new Error("Deliberate debug command failure");
    },
  },
  {
    name: "ticks",
    description: "Enable or disable tick logging",
    args: [{ name: "enabled", type: "bool", required: true }],
    handler: ({ enabled }) => {
      traceTicks = enabled as boolean;
      return `Tick logging ${traceTicks ? "enabled" : "disabled"}`;
    },
  },
  {
    name: "watchdog",
    description: "Delay each tick to exercise watchdog/shutdown; 0 disables",
    args: [{ name: "milliseconds", type: "int", required: true }],
    handler: ({ milliseconds }) => {
      const delay = milliseconds as number;
      if (delay < 0 || delay > 5000) throw new Error("Delay must be 0–5000 ms");
      tickDelay = delay;
      return `Tick delay: ${delay} ms`;
    },
  },
  {
    name: "file",
    description: "Roundtrip through the per-mod file broker",
    handler: async () => {
      const path = `probe-${Date.now()}-${Math.random().toString(16).slice(2)}.txt`;
      try {
        await File.write(path, "Whatever debug suite");
        if ((await File.read(path)) !== "Whatever debug suite")
          throw new Error("File roundtrip failed");
        return "File broker roundtrip passed";
      } finally {
        await File.delete(path);
      }
    },
  },
  {
    name: "message",
    description: "Request a reply from one loaded mod",
    args: [{ name: "target", type: "mod_id", required: true }],
    handler: async ({ target }) =>
      JSON.stringify(await Message.send(target as string, { type: "debug_ping" }, 1000)),
  },
  sandboxCommand,
];
const previousSprites = new Set<Entity>();
export const spriteCommand: CommandSpec = {
  name: "sprite",
  description: "Spawn a sprite and reposition previous probes",
  args: [
    {
      name: "texture",
      type: "string",
      required: false,
      suggest: () => ["debug://textures/sample.png"],
    },
  ],
  handler: async ({ texture }) => {
    const entity = await Scene.spawnSprite(
      (texture as string | undefined) ?? "debug://textures/sample.png",
      "core://shaders/sprite.wgsl",
      [0, 0, 0],
      [1, 1, 3],
    ).get();
    for (const previous of previousSprites) {
      const transform = await previous.getComponent("core:transform").get();
      if (transform) {
        transform.addX(1.5).rotateY(15).rotateZ(2);
        previous.setComponent(transform);
      }
    }
    previousSprites.add(entity);
    return `Spawned sprite ${entity.id}`;
  },
};
export function registerSuite(extra: CommandSpec[] = []): void {
  Console.register({
    name: "dev",
    description: "Whatever first-party debugging suite",
    subcommands: [...commonCommands, ...extra],
    handler: () => "Use help dev for available debugging tools",
  });
}
