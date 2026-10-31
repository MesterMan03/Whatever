import { afterAll, beforeAll, describe, expect, test } from "bun:test";
import { mkdtempSync, mkdirSync, writeFileSync, rmSync, copyFileSync, existsSync, symlinkSync, readFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join, dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import runtimeManifest from "../../runtime/package.json";

const repository = resolve(dirname(fileURLToPath(import.meta.url)), "../..");
const hostName = "whatever-mod-host" + (process.platform === "win32" ? ".exe" : "");
const builtHost = process.env.WHATEVER_TEST_HOST ?? join(repository, "runtime/dist", hostName);
if (!existsSync(builtHost)) throw new Error("Build the native mod host with bun run build:host before running these tests.");

let standaloneDirectory: string;
let standaloneHost: string;
beforeAll(() => {
  standaloneDirectory = mkdtempSync(join(tmpdir(), "whatever-runtime-"));
  standaloneHost = join(standaloneDirectory, hostName);
  copyFileSync(builtHost, standaloneHost);
});
afterAll(() => rmSync(standaloneDirectory, { recursive: true, force: true }));

async function runHost(source: string, extra: Record<string, string> = {}, links: Record<string, string> = {}, entryName = "index.ts", mode: "client" | "server" = "client", onMessage?: (message: Record<string, any>, reply: (message: object) => void) => boolean, tickInput: {keys_pressed:string[];mouse_delta:[number,number];mouse_captured?:boolean} = {keys_pressed:[],mouse_delta:[0,0]}) {
  const directory = mkdtempSync(join(tmpdir(), "whatever-host-"));
  let tickTimer: ReturnType<typeof setInterval> | undefined;
  let deadline: ReturnType<typeof setTimeout> | undefined;
  let stopChild: (() => Promise<void>) | undefined;
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
    const active = Bun.spawn([host, join(root, entryName), root, mode], {
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
    });
    stopChild = async () => { if (active.exitCode === null) { active.kill(9); await active.exited; } };
    let timedOut = false;
    let shuttingDown = false;
    let transportError: unknown;
    const send = (message: object) => {
      if (active.exitCode !== null || timedOut || shuttingDown) return;
      try {
        active.stdin.write(JSON.stringify(message) + "\n");
        const flushed = active.stdin.flush();
        if (flushed instanceof Promise) void flushed.catch(error => {
          transportError = error;
          if (active.exitCode === null) active.kill(9);
        });
      } catch (error) {
        transportError = error;
        if (active.exitCode === null) active.kill(9);
      }
    };
    deadline = setTimeout(() => {
      timedOut = true;
      clearInterval(tickTimer);
      if (active.exitCode === null) active.kill(9);
    }, 10_000);
    const messages: Array<Record<string, any>> = [];
    const batches: Array<Record<string, any>> = [];
    let tickNumber = 0;
    const tick = () => send({ type: "Tick", tick_number: ++tickNumber, delta_seconds: 0.01, mouse_captured: true, ...tickInput });
    const output = (async () => {
      let remaining = "";
      const decoder = new TextDecoder();
      for await (const chunk of active.stdout) {
        remaining += decoder.decode(chunk, {stream:true});
        let newline: number;
        while ((newline = remaining.indexOf("\n")) >= 0) {
          const line = remaining.slice(0, newline);
          remaining = remaining.slice(newline + 1);
          const packet = JSON.parse(line);
          if (packet.type === "CommandBatch") batches.push(packet);
          for (const message of packet.type === "CommandBatch" ? packet.commands : [packet]) {
            messages.push(message);
            if (onMessage?.(message, send)) continue;
            if (message.type === "RuntimeReady") {
              send({ type: "Init", mod_id: "fixture", engine_version: runtimeManifest.version, mode });
              tick();
              tickTimer = setInterval(tick, 10);
            } else if (message.type === "EntityCreate") {
              send({ type: "EntityCreated", request_id: message.request_id, entity_id: "0:0" });
            } else if (message.type === "Log" && message.message === "fixture-done") {
              clearInterval(tickTimer);
              send({ type: "Shutdown", exit_code: 0 });
              shuttingDown = true;
            }
          }
        }
      }
    })();
    const [stderr, exitCode] = await Promise.all([new Response(active.stderr).text(), active.exited, output]);
    if (timedOut || transportError || (exitCode !== 0 && !messages.some(message => message.type === "RuntimeFailed"))) {
      throw new Error(JSON.stringify({entryName, timedOut, transportError:String(transportError ?? ""), exitCode,
        signal:active.signalCode, tickNumber, stderr, messages:messages.slice(-12), batches:batches.slice(-2)}, null, 2));
    }
    return { messages, batches, stderr, exitCode, signalCode: active.signalCode };
  } finally {
    clearInterval(tickTimer);
    clearTimeout(deadline);
    await stopChild?.();
    rmSync(directory, { recursive: true, force: true });
  }
}

function fixture(body: string) {
  return 'import { Engine, Scene } from "@whatever-engine/api";\n' +
    'Engine.on("init", async () => {\n' + body +
    '\nEngine.log("info", "fixture-done");\n});\n';
}

test("uncaptured ticks clear async subscribers' pending mouse motion", async()=>{
  let input={keys_pressed:[] as string[],mouse_delta:[7,3] as [number,number],mouse_captured:true};
  const result=await runHost(`import { Engine, File } from "@whatever-engine/api";
    let calls=0;
    Engine.on("tick",async tick=>{
      calls++;
      if(calls===1){Engine.log("info","RELEASE_MOUSE");await File.read("data://hold.txt");}
      else {Engine.log("info",JSON.stringify({capture:tick.mouse_captured,mouse:tick.mouse_delta}));Engine.log("info","fixture-done");}
    });`,{}, {},"index.ts","client",(message,reply)=>{
      if(message.type==="Log"&&message.message==="RELEASE_MOUSE"){setTimeout(()=>{input.mouse_captured=false;input.mouse_delta=[99,88];},30);}
      if(message.type==="FileRead"){setTimeout(()=>reply({type:"FileResponse",request_id:message.request_id,data_base64:"",error:null}),70);return true;}
      return false;
    },input);
  expect(result.messages.some((m:any)=>m.type==="Log"&&m.message==='{"capture":false,"mouse":[0,0]}')).toBe(true);
});

describe("bundled mod host", () => {
  test("debug free camera is invisible and consumes input only while its camera is active", async () => {
    const cameraSource=readFileSync(join(repository,"games/test/mods/debug/scripts/camera.ts"),"utf8");
    for (const active of [false,true]) {
      let entityIndex=0;
      const result=await runHost(
        'import {Engine} from "@whatever-engine/api";import "./camera.ts";Engine.on("tick",({tick_number})=>{if(tick_number===12)Engine.log("info","fixture-done");});',
        {"camera.ts":cameraSource},{},"index.ts","client",
        (message,reply)=>{
          if(message.type==="EntityCreate") {
            reply({type:"EntityCreated",request_id:message.request_id,entity_id:`${entityIndex++}:0`});return true;
          }
          if(message.type==="GetMainCamera") {
            reply({type:"MainCameraResponse",request_id:message.request_id,entity_id:active?"1:0":"9:0"});return true;
          }
          if(message.type==="ComponentGet") {
            reply({type:"ComponentGetResponse",request_id:message.request_id,entity_id:message.entity_id,component_type:message.component_type,data:{position:[0,0,0],rotation:[0,0,0,1],scale:[1,1,1]},error:null});return true;
          }
          return false;
        },
        {keys_pressed:["Space","KeyW"],mouse_delta:[5,-2]},
      );
      expect(result.exitCode).toBe(0);expect(result.stderr).toBe("");
      expect(result.messages.filter(message=>message.type==="EntityCreate")).toHaveLength(2);
      expect(result.messages.some(message=>message.type==="ComponentSet" && message.component_type==="core:sprite_renderer")).toBe(false);
      const movement=result.messages.filter(message=>message.type==="ComponentSet" && message.component_type==="core:transform" && message.entity_id==="0:0");
      if(active) {
        expect(movement.length).toBeGreaterThan(0);
        expect(movement.some(message=>message.data.position[1]>0 && message.data.position[2]<0)).toBe(true);
      } else {
        // The initial transforms use pending IDs; no movement may target the resolved rig.
        expect(movement).toHaveLength(0);
        expect(result.messages.filter(message=>message.type==="ComponentSet" && message.component_type==="core:transform")).toHaveLength(2);
      }
    }
  });
  test("preserves engine-provided console argument types without mod suggestion callbacks", async () => {
    const result = await runHost('import { Engine, Console } from "@whatever-engine/api";' +
      'Console.register({name:"targets",args:[' +
      '{name:"mod",type:"mod_id",required:true,suggest:()=>{throw new Error("engine owns suggestions")}},' +
      '{name:"entity",type:"entity_id",suggest:()=>[]},' +
      '{name:"custom",type:"string",suggest:()=>["value"]}],' +
      'handler:()=>"ok"});' +
      'Engine.on("init",()=>Engine.log("info","fixture-done"));');
    const registration = result.messages.find(message => message.type === "RegisterCommand");
    expect(registration?.args.map((arg: any) => [arg.type, arg.has_suggest])).toEqual([
      ["mod_id", false], ["entity_id", false], ["string", true],
    ]);
    expect(result.exitCode).toBe(0);
    expect(result.stderr).toBe("");
  });

  test("runs outside the engine tree without PATH or node_modules and preserves IPC", async () => {
    const result = await runHost(fixture('const entity = await Scene.createEntity().get(); console.log("entity", entity.id);'));
    expect(result.messages.filter(message => message.type === "Log").map(message => message.message)).toEqual(["entity 0:0", "fixture-done"]);
    expect(result.stderr).toBe("");
    expect(result.signalCode).toBeNull();
    expect(result.exitCode).toBe(0);
  });

  test("records dependent entity commands eagerly in one implicit tick buffer", async () => {
    const result = await runHost(fixture(
      'const parent=Scene.createEntity();' +
      'const child=Scene.createEntity().setComponent("fixture:value",{value:1}).setParent(parent);' +
      'const selected=child.getComponent("fixture:value");' +
      'if(typeof selected.then!=="undefined")throw new Error("unexpected thenable");'
    ));
    const batch = result.batches.find(packet => packet.commands.some((command: any) => command.type === "EntityCreate"));
    const commands = batch!.commands.filter((command: any) => command.type !== "Subscribe");
    expect(batch!.tick_number).toBe(1);
    expect(commands.map((command: any) => command.type)).toEqual([
      "EntityCreate", "EntityCreate", "ComponentSet", "EntitySetParent", "ComponentGet", "Log",
    ]);
    expect(commands[2].entity_id).toBe("pending:" + commands[1].request_id);
    expect(commands[3].parent_id).toBe("pending:" + commands[0].request_id);
    expect(result.exitCode).toBe(0);
  });

  test("flushes ticks while async handlers wait without overlapping their calls", async () => {
    const result = await runHost('import { Engine } from "@whatever-engine/api";' +
      'let active=false,done=false,calls=0; Engine.on("tick", async ({delta_seconds}) => { if(done)return; if(active)throw new Error("overlap"); if(++calls===1){active=true; await new Promise(resolve => setTimeout(resolve, 35)); active=false;}else{if(delta_seconds<=0.01)throw new Error("lost elapsed time");done=true;Engine.log("info","fixture-done");} });');
    expect(result.batches.length).toBeGreaterThanOrEqual(3);
    expect(result.messages.filter(message => message.type === "Log").map(message => message.message))
      .toEqual(["fixture-done"]);
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
  test("exposes server mode before evaluation and rejects client facilities", async () => {
    const result = await runHost('import { Engine, Window, Audio } from "@whatever-engine/api";' +
      'if (Engine.mode !== "server") throw new Error("mode unavailable at evaluation");' +
      'Engine.on("init", async ({mode}) => {' +
      'if (mode !== "server") throw new Error("incorrect init mode");' +
      'for (const operation of [() => Window.setTitle("test"), () => Engine.setVsync(false), () => Audio.load({path:"core://test.wav"})]) {' +
      'try { await operation(); throw new Error("UNEXPECTED"); } catch (error) { Engine.log("info", String(error)); }}' +
      'Engine.log("info","fixture-done"); });', {}, {}, "index.ts", "server");
    const logs=result.messages.filter(message=>message.type==="Log").map(message=>message.message);
    expect(logs).toHaveLength(4);
    for(const log of logs.slice(0,3)) expect(log).toContain("unavailable in server mode");
    expect(result.exitCode).toBe(0);
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
      'Engine.on("init", () => { try {' +
      'if (module.parent !== null) throw new Error("trusted parent exposed");' +
      'for (const key of Object.keys(require.cache)) {' +
      'if (key.startsWith("node:") || key.includes("$bunfs") || key.includes("~BUN")) throw new Error("trusted cache exposed: " + key); }' +
      'try { module.require("node:fs"); throw new Error("UNEXPECTED"); }' +
      'catch (error) { Engine.log("info", String(error)); }' +
      '} catch (error) { Engine.log("error", String(error)); }' +
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

test("registered SDK peers keep typed data, ordered writes and disconnect state", async () => {
  const writes: Array<Record<string, any>>=[];
  const source='import {Engine,Network} from "@whatever-engine/api";'+
    'Engine.on("init",async()=>{const spec=Network.Specification("fixture:peers",{s:{serverbound:{hello:Network.Packet({name:Network.Type.String})}}});'+
    'const server=await Network.registerServer(spec,{clientData:Network.Struct({name:Network.Type.String,tags:Network.Type.Array(Network.Type.String)}),initialClientData:{name:"",tags:[]}});'+
    'server.onConnect(async client=>{if(client.id==="a"){client.data.name="Alex";client.data.tags.push("player");await client.flushData();Engine.log("info","NETWORK_DATA_WRITTEN");}else if(client.data.tags.length)throw new Error("shared defaults");});'+
    'server.onDisconnect(client=>{if(client.connected||client.data.name!=="Alex")throw new Error("bad disconnect state");try{client.data.name="bad";throw new Error("UNEXPECTED");}catch(e){if(!String(e).includes("disconnected"))throw e;}Engine.log("info","fixture-done");});'+
    'Engine.log("info","NETWORK_HOST_READY");});';
  const result=await runHost(source,{}, {}, "index.ts", "server", (message,reply)=>{
    if(message.type==="NetworkRequest") {
      const req=message.request;
      if(req.op==="Register")reply({type:"NetworkResponse",request_id:message.request_id,error:null,result:{registration_id:"test:1",specification_id:"fixture:peers",fingerprint:"fixture",packets:[]}});
      else {writes.push(req);reply({type:"NetworkResponse",request_id:message.request_id,error:null,result:null});}
      return true;
    }
    if(message.type==="Log"&&message.message==="NETWORK_HOST_READY") {
      for(const client_id of ["a","b"])reply({type:"NetworkEvent",event:"connected",registration_id:"test:1",client_id,data:{name:"",tags:[]}});
      return true;
    }
    if(message.type==="Log"&&message.message==="NETWORK_DATA_WRITTEN") {
      reply({type:"NetworkEvent",event:"disconnected",registration_id:"test:1",client_id:"a"});return true;
    }
    return false;
  });
  expect(result.exitCode).toBe(0);expect(result.stderr).toBe("");
  expect(writes.map(w=>w.data)).toEqual([{name:"Alex",tags:[]},{name:"Alex",tags:["player"]}]);
  expect(result.messages.filter(m=>m.type==="Log"&&m.level==="error")).toHaveLength(0);
});
