import { Engine } from "@whatever-engine/api";
import { registerSuite } from "./probes.ts";
registerSuite([]);
Engine.on("init", () => Engine.log("info", "Debug suite ready in server mode"));
