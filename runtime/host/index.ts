import { plugin } from "bun";
import { realpathSync } from "node:fs";
import { relative, isAbsolute, sep } from "node:path";
import { pathToFileURL } from "node:url";
import * as api from "../src/index.ts";
import manifest from "../package.json";
import { _configureMode } from "../src/environment.ts";

const exit = (process as typeof process & { reallyExit(code: number): never }).reallyExit.bind(process);
const stdout = Bun.stdout.writer();
const write = (data: string) => { stdout.write(data); stdout.flush(); };
const inspect = Bun.inspect;
const lockdown = (Bun as typeof Bun & {
  __whateverLockdown?: (root: string) => number;
}).__whateverLockdown;

try {
  if (typeof lockdown !== "function") {
    throw new Error("This host requires the Whatever Bun fork; stock Bun is not supported.");
  }
  if (process.argv[2] === "--runtime-info") {
    write(JSON.stringify({
      runtime: "whatever-mod-host",
      protocol: 6,
      native_policy: 1,
      bun_version: Bun.version,
      api_version: manifest.version,
    }) + "\n");
    exit(0);
  }
  const [entryArg, rootArg, mode = "client"] = process.argv.slice(2);
  if (!entryArg || !rootArg || ![4, 5].includes(process.argv.length)) {
    throw new Error("Usage: whatever-mod-host <entry> <mod-root> [client|server]");
  }
  if (mode !== "client" && mode !== "server") throw new Error("Invalid runtime mode");
  _configureMode(mode);
  const root = realpathSync(rootArg);
  const entry = realpathSync(entryArg);
  const within = relative(root, entry);
  if (!within || isAbsolute(within) || within === ".." || within.startsWith(".." + sep)) {
    throw new Error("The script entry must be inside its mod directory.");
  }
  const entryUrl = pathToFileURL(entry).href;

  plugin({
    name: "whatever-engine-api",
    setup(builder) {
      builder.module("@whatever-engine/api", () => ({
        exports: { ...api },
        loader: "object",
      }));
    },
  });

  const consoleMethods: Record<string, (...args: unknown[]) => void> = Object.create(null);
  for (const name of ["log", "info", "warn", "error", "debug", "trace"]) {
    consoleMethods[name] = (...args) => {
      const message = args.map(value => typeof value === "string" ? value : inspect(value)).join(" ");
      api.Engine.log(name === "error" ? "error" : name === "warn" ? "warn" : "info", message);
    };
  }
  Object.defineProperty(globalThis, "console", {
    value: Object.freeze(consoleMethods),
    writable: false,
    configurable: false,
  });

  if (lockdown(root) !== 1) {
    throw new Error("The native runtime policy version is unsupported.");
  }
  await import(entryUrl);
  write(JSON.stringify({ type: "RuntimeReady", protocol: 6 }) + "\n");
} catch (error) {
  write(JSON.stringify({
    type: "RuntimeFailed",
    message: "Mod host startup failed: " + String(error),
  }) + "\n");
  exit(1);
}
