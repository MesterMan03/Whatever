import { Engine } from "@whatever-engine/api";
import { registerSuite } from "./probes.ts";
import { benchmarkCommand } from "./benchmark.ts";
registerSuite([benchmarkCommand]);
Engine.on("init", () => Engine.log("info", "Debug suite ready in server mode"));
