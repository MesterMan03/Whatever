import { afterAll, beforeAll, describe, expect, test } from "bun:test";
import { mkdtempSync, mkdirSync, writeFileSync, rmSync, copyFileSync, existsSync, symlinkSync } from "node:fs";
import { tmpdir } from "node:os";
import { join, dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const repository = resolve(dirname(fileURLToPath(import.meta.url)), "../..");
const hostName = "whatever-mod-host" + (process.platform === "win32" ? ".exe" : "");
const builtHost = join(repository, "runtime/dist", hostName);
if (!existsSync(builtHost)) throw new Error("Build the native mod host with bun run build:host before running these tests.");

let standaloneDirectory: string;
let standaloneHost: string;
beforeAll(() => {
  standaloneDirectory = mkdtempSync(join(tmpdir(), "whatever-runtime-"));
  standaloneHost = join(standaloneDirectory, hostName);
  copyFileSync(builtHost, standaloneHost);
});
afterAll(() => rmSync(standaloneDirectory, { recursive: true, force: true }));

async function runHost(source: string, extra: Record<string, string> = {}, links: Record<string, string> = {}, entryName = "index.ts") {
  const directory = mkdtempSync(join(tmpdir(), "whatever-host-"));
  try {
    const host = standaloneHost;
    const root = join(directory, "mod");
    mkdirSync(root);
    for (const [name, content] of Object.entries({ [entryName]: source, ...extra })) {
      const path = join(root, name);
      mkdirSync(dirname(path), { recursive: true });
      writeFileSync(path, content);
    }
    for (const [name, target] of Object.entries(links)) {
      symlinkSync(join(directory, target), join(root, name), process.platform === "win32" ? "junction" : "dir");
    }
    const child = Bun.spawn([host, join(root, entryName), root], {
      cwd: directory,
      env: {
        PATH: "",
        BUN_DEBUG_QUIET_LOGS: "1",
        SystemRoot: process.env.SystemRoot,
        WINDIR: process.env.WINDIR,
        TEMP: tmpdir(),
        TMP: tmpdir(),
      },
      stdin: "pipe",
      stdout: "pipe",
      stderr: "pipe",
      timeout: 10_000,
    });
    const messages: Array<Record<string, any>> = [];
    const output = (async () => {
      let remaining = "";
      for await (const chunk of child.stdout) {
        remaining += new TextDecoder().decode(chunk);
        let newline: number;
        while ((newline = remaining.indexOf("\n")) >= 0) {
          const line = remaining.slice(0, newline);
          remaining = remaining.slice(newline + 1);
          const message = JSON.parse(line);
          messages.push(message);
          if (message.type === "RuntimeReady") {
            child.stdin.write(JSON.stringify({ type: "Init", mod_id: "fixture", engine_version: "0.4.0" }) + "\n");
          } else if (message.type === "EntityCreate") {
            child.stdin.write(JSON.stringify({ type: "EntityCreated", request_id: message.request_id, entity_id: "0:0" }) + "\n");
          } else if (message.type === "Log" && message.message === "fixture-next-tick") {
            child.stdin.write(JSON.stringify({ type: "Tick", tick_number: 7, delta_seconds: 0.016, keys_pressed: [], mouse_delta: [0, 0] }) + "\n");
          } else if (message.type === "TickDone") {
            child.stdin.write(JSON.stringify({ type: "Shutdown", exit_code: 0 }) + "\n");
          } else if (message.type === "Log" && message.message === "fixture-done") {
            child.stdin.write(JSON.stringify({ type: "Shutdown", exit_code: 0 }) + "\n");
          }
        }
      }
    })();
    const [stderr, exitCode] = await Promise.all([new Response(child.stderr).text(), child.exited, output]);
    return { messages, stderr, exitCode, signalCode: child.signalCode };
  } finally {
    rmSync(directory, { recursive: true, force: true });
  }
}

function fixture(body: string) {
  return 'import { Engine, Scene } from "@whatever-engine/api";\n' +
    'Engine.on("init", async () => {\n' + body +
    '\nEngine.log("info", "fixture-done");\n});\n';
}

describe("bundled mod host", () => {
  test("runs outside the engine tree without PATH or node_modules and preserves IPC", async () => {
    const result = await runHost(fixture('const entity = await Scene.createEntity(); console.log("entity", entity.id);'));
    expect(result.messages.filter(message => message.type === "Log").map(message => message.message)).toEqual(["entity 0:0", "fixture-done"]);
    expect(result.stderr).toBe("");
    expect(result.signalCode).toBeNull();
    expect(result.exitCode).toBe(0);
  });

  test("waits for asynchronous tick handlers before replying", async () => {
    const result = await runHost('import { Engine } from "@whatever-engine/api";' +
      'Engine.on("init", () => Engine.log("info", "fixture-next-tick"));' +
      'Engine.on("tick", async () => { await new Promise(resolve => setTimeout(resolve, 1)); Engine.log("info", "tick-work"); });');
    expect(result.messages.filter(message => message.type === "Log" || message.type === "TickDone").map(message => message.type === "Log" ? message.message : message.tick_number))
      .toEqual(["fixture-next-tick", "tick-work", 7]);
    expect(result.exitCode).toBe(0);
  });

  test("disables compile-time macros before they can execute", async () => {
    const result = await runHost('import { run } from "./macro.ts" with { type: "macro" }; run();', {
      "macro.ts": 'export function run() { throw new Error("macro body executed"); }',
    });
    expect(result.exitCode).toBe(1);
    expect(result.messages).toHaveLength(1);
    expect(result.messages[0]!.message).toContain("Macros are disabled");
    expect(result.messages[0]!.message).not.toContain("macro body executed");
  });
  test("resolves the engine API before a mod-supplied package", async () => {
    const result = await runHost(fixture(""), {
      "node_modules/@whatever-engine/api/package.json": '{"main":"index.js"}',
      "node_modules/@whatever-engine/api/index.js": 'throw new Error("shadow API executed")',
    });
    expect(result.messages.some(message => message.message === "fixture-done")).toBe(true);
    expect(result.exitCode).toBe(0);
  });

  test("loads local ESM, CommonJS and JSON files", async () => {
    const result = await runHost('import value from "./local.ts"; import data from "./data.json";\n' +
      fixture('Engine.log("info", String(value + require("./local.cjs") + data.value));'), {
      "local.ts": "export default 10;",
      "local.cjs": "module.exports = 20;",
      "data.json": '{"value":12}',
    });
    expect(result.messages.filter(message => message.type === "Log").map(message => message.message)).toEqual(["42", "fixture-done"]);
    expect(result.exitCode).toBe(0);
  });

  test.each(["fs", "node:fs", "node:fs/promises", "node:module", "bun", "bun:ffi", "bun:sqlite", "lodash", "@scope/package", "https://example.invalid/mod.js", "data:text/javascript,export default 1", "@whatever-engine/api/internal", "./node_modules/package/index.js"])(
    "denies dynamic import and require of %s",
    async specifier => {
      const literal = JSON.stringify(specifier);
      const result = await runHost(fixture(
        'for (const load of [() => import(' + literal + '), () => require(' + literal + ')]) {' +
        'try { await load(); Engine.log("error", "UNEXPECTED"); }' +
        'catch (error) { Engine.log("info", String(error)); }}'
      ), { "node_modules/package/index.js": "export default 1;" });
      const logs = result.messages.filter(message => message.type === "Log").map(message => message.message);
      expect(logs).toHaveLength(3);
      expect(logs[0]).toStartWith("TypeError: Whatever module policy denied:");
      expect(logs[1]).toStartWith("TypeError: Whatever module policy denied:");
      expect(logs[2]).toBe("fixture-done");
      expect(result.exitCode).toBe(0);
    },
  );

  test("denies a static external import before its body executes", async () => {
    const result = await runHost('import "package";\n' + fixture(""), {
      "node_modules/package/package.json": '{"main":"index.js"}',
      "node_modules/package/index.js": 'throw new Error("package body executed");',
    });
    expect(result.messages).toHaveLength(1);
    expect(result.messages[0]!.message).toContain("Whatever module policy denied: package");
    expect(result.exitCode).toBe(1);
  });

  test.each(["bun", "node:fs", "bun:ffi"])("denies optimized static imports of %s", async name => {
    const result = await runHost("import " + JSON.stringify(name) + ";\n" + fixture(""));
    expect(result.messages).toHaveLength(1);
    expect(result.messages[0]!.message).toContain("Whatever module policy denied:");
    expect(result.exitCode).toBe(1);
  });

  test.each(["sqlite", "file", "text"])("denies the %s import loader for local sources", async type => {
    const result = await runHost(fixture(
      'try { await import("./probe.ts", { with: { type: ' + JSON.stringify(type) + ' } }); Engine.log("error", "UNEXPECTED"); }' +
      'catch (error) { Engine.log("info", String(error)); }'
    ), { "probe.ts": "" });
    const logs = result.messages.filter(message => message.type === "Log").map(message => message.message);
    expect(logs[0]).toStartWith("TypeError: Whatever module policy denied loader:");
    expect(logs[1]).toBe("fixture-done");
    expect(result.exitCode).toBe(0);
  });
  test("runs a CommonJS entry without exposing trusted parents or cache entries", async () => {
    const result = await runHost('const { Engine } = require("@whatever-engine/api");' +
      'Engine.on("init", () => {' +
      'if (module.parent !== null) throw new Error("trusted parent exposed");' +
      'for (const key of Object.keys(require.cache)) {' +
      'if (key.startsWith("node:") || key.includes("$bunfs")) throw new Error("trusted cache exposed"); }' +
      'try { module.require("node:fs"); throw new Error("UNEXPECTED"); }' +
      'catch (error) { Engine.log("info", String(error)); }' +
      'Engine.log("info", "fixture-done"); });', {}, {}, "index.cjs");
    const logs = result.messages.filter(message => message.type === "Log").map(message => message.message);
    expect(logs[0]).toStartWith("TypeError: Whatever module policy denied:");
    expect(logs[1]).toBe("fixture-done");
    expect(result.exitCode).toBe(0);
  });

  test("checks computed imports, require.resolve and import.meta.resolve", async () => {
    const result = await runHost(fixture(
      'const name = ["node", "fs"].join(":");' +
      'for (const load of [() => import(name), () => require(name), () => require.resolve(name), () => import.meta.resolve(name)]) {' +
      'try { await load(); Engine.log("error", "UNEXPECTED"); } catch (error) { Engine.log("info", String(error)); }}'
    ));
    const logs = result.messages.filter(message => message.type === "Log").map(message => message.message);
    expect(logs).toHaveLength(5);
    for (const log of logs.slice(0, 4)) expect(log).toStartWith("TypeError: Whatever module policy denied:");
    expect(logs[4]).toBe("fixture-done");
    expect(result.exitCode).toBe(0);
  });

  test.each([
    ["outside", "../outside/secret.ts", "outside"],
    ["packages", "node_modules/package/index.js", "mod/node_modules/package"],
  ])("rejects imports through a %s directory link", async (name, file, target) => {
    const result = await runHost(fixture(
      'try { await import("./' + name + '/' + (name === "outside" ? "secret.ts" : "index.js") + '"); }' +
      'catch (error) { Engine.log("info", String(error)); }'
    ), { [file]: "export default 42;" }, { [name]: target });
    const logs = result.messages.filter(message => message.type === "Log").map(message => message.message);
    expect(logs[0]).toStartWith("TypeError: Whatever module policy denied:");
    expect(logs[1]).toBe("fixture-done");
    expect(result.exitCode).toBe(0);
  });
  test("hides non-allowlisted globals and blocks reconstruction", async () => {
    const result = await runHost(fixture(
      'const blocked = ["Bun", "process", "fetch", "WebSocket", "Worker", "ShadowRealm"];' +
      'for (const name of blocked) {' +
      'if (globalThis[name] !== undefined || Function("return typeof " + name)() !== "undefined") throw new Error(name);try { Object.defineProperty(globalThis, name, { value: {} }); throw new Error("restored " + name); } catch (error) {if (!(error instanceof TypeError)) throw error; }}' +
      'Engine.log("info", String(new TextEncoder().encode("ok").length));'
    ));
    expect(result.messages.filter(message => message.type === "Log").map(message => message.message)).toEqual(["2", "fixture-done"]);
    expect(result.exitCode).toBe(0);
  });

  test("rejects a path outside the mod directory", async () => {
    const result = await runHost(fixture(
      'try { await import("../outside.ts"); } catch (error) { Engine.log("info", String(error)); }'
    ), { "../outside.ts": "export default 42;" });
    const logs = result.messages.filter(message => message.type === "Log").map(message => message.message);
    expect(logs[0]).toStartWith("TypeError: Whatever module policy denied:");
    expect(logs[1]).toBe("fixture-done");
    expect(result.exitCode).toBe(0);
  });
});