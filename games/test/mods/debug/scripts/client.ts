import { Engine } from "@whatever-engine/api";
import "./camera.ts";
import { registerSuite, spriteCommand } from "./probes.ts";
import { renderCommands } from "./render.ts";
import { presentationCommands } from "./presentation.ts";
registerSuite([
  spriteCommand,
  { name: "render", description: "Mesh, shader and light probes", subcommands: renderCommands },
  ...presentationCommands,
]);
Engine.on("init", () => Engine.log("info", "Debug suite ready: help dev (F2 console)"));
