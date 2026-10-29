import { describe, expect, test } from "bun:test";
import { mkdtempSync, mkdirSync, writeFileSync, rmSync, existsSync } from "node:fs";
import { join, dirname, resolve } from "node:path";
import { tmpdir } from "node:os";
import { fileURLToPath } from "node:url";
const repository=resolve(dirname(fileURLToPath(import.meta.url)),"../..");
const binary=join(repository,"target/debug/Whatever"+(process.platform==="win32"?".exe":""));
if(!existsSync(binary))throw new Error("Run cargo build before bun run test:server");

async function runServer(source:string, options:{mode?:string;defaultMode?:string;stopAt?:string;commandsOnReady?:string[]}={}) {
  const directory=mkdtempSync(join(tmpdir(),"whatever-server-"));
  let child: ReturnType<typeof Bun.spawn> | undefined;
  try {
    const put=(name:string,value:string)=>{const path=join(directory,name);mkdirSync(dirname(path),{recursive:true});writeFileSync(path,value);};
    put("mods/core/meta.toml",'[game]\nid="separation_fixture"\nname="Server Fixture"\ntick_rate=100\ndefault_mode="'+(options.defaultMode??"client")+'"\n');
    put("mods/core/mod.toml",'[mod]\nid="core"\nname="Core"\nversion="1.0.0"\n[script]\nentry="wrong.ts"\nclient="wrong.ts"\nserver="server.ts"\n');
    put("mods/core/wrong.ts",'throw new Error("wrong entry selected");');
    put("mods/core/server.ts",source);
    put("mods/client_only/mod.toml",'[mod]\nid="client_only"\nname="Client"\nversion="1.0.0"\nsides=["client"]\n[dependencies]\nmissing_client_dependency="*"\n[script]\nentry="wrong.ts"\n');
    put("mods/client_only/wrong.ts",'throw new Error("client-only mod executed");');
    put("mods/server_only/mod.toml",'[mod]\nid="server_only"\nname="Server"\nversion="1.0.0"\nsides=["server"]\n[script]\nentry="server.ts"\n');
    put("mods/server_only/server.ts",'import { Engine } from "@whatever-engine/api"; Engine.on("init",()=>Engine.log("info","SERVER_ONLY_READY"));');
    const args=[binary,"--mods",join(directory,"mods"),...(options.mode?["--mode",options.mode]:[])];
    child=Bun.spawn(args,{cwd:directory,env:{...process.env,RUST_LOG:"Whatever=info",PATH:"",DISPLAY:"",WAYLAND_DISPLAY:"",ALSA_CONFIG_PATH:join(directory,"no-audio.conf")},stdin:"pipe",stdout:"pipe",stderr:"pipe",timeout:15_000});
    const active=child;
    let stdout="";
    let stopped=false;
    let stopTime=0;
    let commandsSent=false;
    const read=(async()=>{
      for await (const bytes of active.stdout as ReadableStream<Uint8Array>){
        stdout+=new TextDecoder().decode(bytes);
        if(!commandsSent && options.commandsOnReady && stdout.includes("CONSOLE_READY")) {commandsSent=true;(active.stdin as import("bun").FileSink).write(options.commandsOnReady.join("\n")+"\n");}
        if(!stopped && stdout.includes(options.stopAt??"FIXTURE_DONE") && stdout.includes("SERVER_ONLY_READY")){
          stopped=true;stopTime=performance.now();
          (active.stdin as import("bun").FileSink).write("quit\n");
        }
      }
    })();
    const [stderr,code]=await Promise.all([new Response(active.stderr as ReadableStream).text(),active.exited,read]);
    if(code!==0)throw new Error("Server exited "+code+"\n"+stdout+"\nSTDERR:\n"+stderr);
    return {stdout,stderr,code,signal:active.signalCode,shutdownMs:stopped?performance.now()-stopTime:Infinity};
  } finally {
    if(child && child.exitCode===null){child.kill();await child.exited;}
    rmSync(directory,{recursive:true,force:true});
  }
}
const normal='import { Engine, Scene, Mods } from "@whatever-engine/api";'+
  'if(Engine.mode!=="server")throw new Error("mode missing before evaluation");'+
  'let ready=false;'+
  'Engine.on("init",async({mode})=>{if(mode!=="server")throw new Error("wrong init mode");'+
  'const entity=await Scene.createEntity();entity.setComponent("fixture:test",{ok:true});'+
  'const component=await entity.getComponent("fixture:test");if(!component?.ok)throw new Error("ECS IPC failed");'+
  'const mods=await Mods.list();if(mods.some(mod=>mod.id==="client_only"))throw new Error("side filtering failed");'+
  'if(!mods.find(mod=>mod.id==="server_only")?.sides.includes("server"))throw new Error("side metadata missing");'+
  'ready=true;Engine.setTickRate(-1);Engine.log("info","INIT_COMPLETE");});'+
  'Engine.on("tick",({tick_number,keys_pressed,mouse_delta})=>{if(keys_pressed.length||mouse_delta.some(x=>x!==0))throw new Error("server has input");'+
  'if(ready&&tick_number>=5)Engine.log("info","FIXTURE_DONE");});';

describe("headless runtime",()=>{
  test("CLI overrides the client default; ECS and ticks work without a display, audio or PATH",async()=>{
    const result=await runServer(normal,{mode:"server"});
    expect(result.code).toBe(0);expect(result.signal).toBeNull();
    expect(result.stdout).toContain("INIT_COMPLETE");expect(result.stdout).toContain("FIXTURE_DONE");
    expect(result.stdout).toContain("ignoring invalid tick rate");
    expect(result.stdout).not.toContain("wrong entry selected");expect(result.stderr).toBe("");
    expect(result.shutdownMs).toBeLessThan(3000);
  });
  test("uses core's server default when no mode flag is supplied",async()=>{
    const result=await runServer(normal,{defaultMode:"server"});
    expect(result.code).toBe(0);expect(result.stdout).toContain("FIXTURE_DONE");expect(result.stderr).toBe("");
  });
  test("shutdown interrupts a tick whose mod never finishes",async()=>{
    const source='import {Engine} from "@whatever-engine/api";Engine.on("tick",()=>{Engine.log("info","TICK_BLOCKED");return new Promise(()=>{});});';
    const result=await runServer(source,{mode:"server",stopAt:"TICK_BLOCKED"});
    expect(result.code).toBe(0);expect(result.stdout).toContain("simulation stopped");expect(result.shutdownMs).toBeLessThan(3000);
  });
  test.each(["", "async"])("a crashed %s tick subscriber does not strand the simulation",async kind=>{
    const source='import {Engine} from "@whatever-engine/api";Engine.on("tick",'+kind+'()=>{throw new Error("deliberate tick failure");});';
    const result=await runServer(source,{mode:"server",stopAt:"mod process exited"});
    expect(result.code).toBe(0);expect(result.stdout).toContain("mod process exited");expect(result.shutdownMs).toBeLessThan(3000);
  });});

test("native network registration and offline codecs work through the bundled server host", async () => {
  const source = 'import {Engine,Network} from "@whatever-engine/api";'+
    'const game=Network.Specification("core:game",{login:{serverbound:{hello:Network.Packet({name:Network.Type.String})},clientbound:{welcome:Network.Packet({id:Network.Type.I32.with({min:0}),maybe:Network.Type.Optional(Network.Type.Bool)})}}});'+
    'Engine.on("init",async()=>{try{'+
    'const bad=Network.Specification("core:bad",{s:{serverbound:{p:Network.Packet({n:Network.Type.I32.with({bogus:1})})}}});'+
    'try{await Network.registerServer(bad,{clientData:Network.Struct({}),initialClientData:{}});throw new Error("UNEXPECTED");}catch(e){if(!String(e).includes("unknown field"))throw e;}'+
    'const server=await Network.registerServer(game,{clientData:Network.Struct({tags:Network.Type.Array(Network.Type.String)}),initialClientData:{tags:[]}});'+
    'if(server.packets[0].id!==0||server.packets[1].id!==1)throw new Error("packet ID assignment");'+
    'const bytes=await server.encodePacket("login.welcome",{id:42});'+
    'if(bytes.join(",")!=="0,1,0,0,0,42,0")throw new Error("incorrect wire bytes: "+bytes);'+
    'const decoded=await server.decodePacket(bytes);if(decoded.packet!=="login.welcome"||decoded.data.id!==42||decoded.data.maybe!==undefined)throw new Error("decode mismatch");'+
    'const data=await server.validateClientData({tags:["ok"]});if(data.tags[0]!=="ok"||server.clients.length)throw new Error("client defaults");'+
    'try{await server.broadcast("login.welcome",{id:1});throw new Error("UNEXPECTED");}catch(e){if(!String(e).includes("server is not listening"))throw e;}'+
    'await server.close();try{await server.encodePacket("login.welcome",{id:1});throw new Error("UNEXPECTED");}catch(e){if(!String(e).includes("closed"))throw e;}'+
    'Engine.log("info","FIXTURE_DONE");}catch(e){Engine.log("error","NETWORK_FAILURE "+e);}});';
  const result=await runServer(source,{mode:"server"});
  expect(result.stdout).toContain("FIXTURE_DONE");expect(result.stdout).not.toContain("NETWORK_FAILURE");
});
test("headless stdin executes registered mod commands using engine argument validation", async () => {
  const source='import {Engine,Console} from "@whatever-engine/api";'+
    'Console.register({name:"probe",args:[{name:"target",type:"mod_id",required:true}],handler:({target})=>"FIXTURE_DONE target="+target});'+
    'Engine.on("init",()=>Engine.log("info","CONSOLE_READY"));';
  const result=await runServer(source,{mode:"server",commandsOnReady:["probe core"]});
  expect(result.stdout).toContain("FIXTURE_DONE target=core");
});
