import { describe, expect, test } from "bun:test";
import { mkdtempSync, mkdirSync, writeFileSync, rmSync, existsSync, readFileSync, cpSync } from "node:fs";
import { join, dirname, resolve, basename } from "node:path";
import { tmpdir } from "node:os";
import { fileURLToPath } from "node:url";
const repository=resolve(dirname(fileURLToPath(import.meta.url)),"../..");
const binary=process.env.WHATEVER_TEST_PLAYER ?? join(repository,"target/debug/Whatever"+(process.platform==="win32"?".exe":""));
if(!existsSync(binary))throw new Error("Run cargo build before bun run test:server");

async function runServer(source:string, options:{shutdownCommand?:string;expectedCode?:number;initialScene?:string;mode?:string;defaultMode?:string;stopAt?:string;commandsOnReady?:string[];serverSource?:string;files?:Record<string,string>;coreScript?:boolean;coreAssets?:string}={}) {
  const directory=mkdtempSync(join(tmpdir(),"whatever-server-"));
  let child: ReturnType<typeof Bun.spawn> | undefined;
  try {
    const put=(name:string,value:string)=>{const path=join(directory,name);mkdirSync(dirname(path),{recursive:true});writeFileSync(path,value);};
    put("mods/core/meta.toml",'[game]\nid="separation_fixture"\nname="Server Fixture"\ntick_rate=100\ndefault_mode="'+(options.defaultMode??"client")+'"\n');
    put("mods/core/mod.toml",(options.initialScene ? `initial_scene = "${options.initialScene}"\n` : "")+'[mod]\nid="core"\nname="Core"\nversion="1.0.0"\n');
    put("mods/fixture/mod.toml",'[mod]\nid="fixture"\nname="Fixture"\nversion="1.0.0"\n[dependencies]\ncore="*"\n[script]\nentry="wrong.ts"\nclient="wrong.ts"\nserver="server.ts"\n');
    put("mods/fixture/wrong.ts",'throw new Error("wrong entry selected");');
    put("mods/fixture/server.ts",source);
    if(options.coreScript) {
      put("mods/core/mod.toml",'[mod]\nid="core"\nname="Core"\nversion="1.0.0"\n[script]\nserver="server.ts"\n');
      put("mods/core/server.ts",source);
      put("mods/fixture/server.ts",'');
    }
    for(const [path,text] of Object.entries(options.files??{}))put((path.startsWith("scenes/")?"mods/core/":"mods/fixture/")+path,text);
    if(options.coreAssets)cpSync(options.coreAssets,join(directory,"mods/core/assets"),{recursive:true});
    put("mods/client_only/mod.toml",'[mod]\nid="client_only"\nname="Client"\nversion="1.0.0"\nsides=["client"]\n[dependencies]\nmissing_client_dependency="*"\n[script]\nentry="wrong.ts"\n');
    put("mods/client_only/wrong.ts",'throw new Error("client-only mod executed");');
    put("mods/server_only/mod.toml",'[mod]\nid="server_only"\nname="Server"\nversion="1.0.0"\nsides=["server"]\n[script]\nentry="server.ts"\n');
    put("mods/server_only/server.ts", options.serverSource ?? 'import { Engine } from "@whatever-engine/api"; Engine.on("init",()=>Engine.log("info","SERVER_ONLY_READY"));');
    const args=[binary,"--debug=ipc","--mods",join(directory,"mods"),...(options.mode?["--mode",options.mode]:[])];
    child=Bun.spawn(args,{cwd:directory,env:{...process.env,RUST_LOG:"whatever_engine=info",PATH:"",DISPLAY:"",WAYLAND_DISPLAY:"",ALSA_CONFIG_PATH:join(directory,"no-audio.conf")},stdin:"pipe",stdout:"pipe",stderr:"pipe",timeout:15_000});
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
          (active.stdin as import("bun").FileSink).write((options.shutdownCommand??"quit")+"\n");
        }
      }
    })();
    const [stderr,code]=await Promise.all([new Response(active.stderr as ReadableStream).text(),active.exited,read]);
    if(code!==(options.expectedCode??0) || ((options.expectedCode??0)===0 && !stopped)) {
      const diagnostics=join(repository,"target/ci-server-failures",basename(directory));
      mkdirSync(dirname(diagnostics),{recursive:true});
      cpSync(directory,diagnostics,{recursive:true});
      writeFileSync(join(diagnostics,"stdout.log"),stdout);
      writeFileSync(join(diagnostics,"stderr.log"),stderr);
      const log=[join(directory,"logs/latest.log"),join(dirname(binary),"logs/latest.log"),
        join(repository,"games/test/logs/latest.log")].find(existsSync);
      if(log)cpSync(log,join(diagnostics,"ipc.log"));
      const trace=log?readFileSync(log,"utf8").slice(-16000):"No IPC log produced.";
      throw new Error("Server exited "+code+" (fixture finished: "+stopped+")\n"+stdout+"\nSTDERR:\n"+stderr+"\nIPC TRACE:\n"+trace);
    }
    return {stdout,stderr,code,signal:active.signalCode,shutdownMs:stopped?performance.now()-stopTime:Infinity};
  } finally {
    if(child && child.exitCode===null){child.kill();await child.exited;}
    rmSync(directory,{recursive:true,force:true});
  }
}
const normal='import { Component, Engine, Scene, Mods } from "@whatever-engine/api";'+
  'export class Check extends Component { static readonly type="fixture:test"; ok=true; }'+
  'if(Engine.mode!=="server")throw new Error("mode missing before evaluation");'+
  'let ready=false;'+
  'Engine.on("init",async({mode})=>{if(mode!=="server")throw new Error("wrong init mode");'+
  'const entity=await Scene.createEntity().get();entity.setComponent(new Check());'+
  'const component=await entity.requestComponent(Check).get();if(!component?.ok)throw new Error("ECS IPC failed");'+
  'const mods=await Mods.list();if(mods.some(mod=>mod.id==="client_only"))throw new Error("side filtering failed");'+
  'if(!mods.find(mod=>mod.id==="server_only")?.sides.includes("server"))throw new Error("side metadata missing");'+
  'ready=true;Engine.setTickRate(-1);Engine.log("info","INIT_COMPLETE");});'+
  'Engine.on("tick",({tick_number,keys_pressed,mouse_delta})=>{if(keys_pressed.length||mouse_delta.some(x=>x!==0))throw new Error("server has input");'+
  'if(ready&&tick_number>=5)Engine.log("info","FIXTURE_DONE");});';

test("prefabs spawn independent trees through the sandboxed batched API", async () => {
  const prefab = JSON.stringify({version:1,name:"Pair",entity:{script_id:"root",components:{"fixture:link":{target:{$entity:"child"}}},children:[{script_id:"child",components:{"core:transform":{position:[0,1,0],rotation:[0,0,0,1],scale:[1,1,1]}}}]}});
  const source = `import { Component, Entity, Engine, Prefab, Scene } from "@whatever-engine/api";
export class Link extends Component { static readonly type="fixture:link"; target: Entity | null=null; }
Engine.on("init", async () => {
  const a = await Prefab.spawn("fixture://pair");
  const b = await Prefab.spawn("fixture://prefabs/pair.wprefab");
  if (a.entityCount !== 2 || a.root.id !== a.get("root").id || a.root.id === b.root.id) throw Error("independent prefab roots failed");
  const linkA = await a.root.requestComponent(Link).get();
  const linkB = await b.root.requestComponent(Link).get();
  if (linkA?.target?.id !== a.get("child").id || linkB?.target?.id !== b.get("child").id) throw Error("prefab-local references leaked");
  let rejected = 0;
  for (const path of ["fixture://missing", "server_only://foreign"]) {
    try { await Prefab.spawn(path); } catch { rejected++; }
  }
  if (rejected !== 2) throw Error("missing/dependency checks failed");
  if (!Scene.find(a.root.id) || !Scene.find(b.root.id)) throw Error("failed spawns disturbed earlier objects");
  Engine.log("info", "FIXTURE_DONE");
});`;
  await runServer(source,{mode:"server",files:{"prefabs/pair.wprefab":prefab}});
});

test("scene files instantiate independent hierarchies and reject invalid loads atomically", async () => {
  const source = `
import { Component, Entity, Engine, Scene } from "@whatever-engine/api";
export class Link extends Component { static readonly type="fixture:link"; target: Entity | null=null; text=""; }
Engine.on("init", async () => {
  try {
    const first = await Scene.loadScene("fixture://graph");
    const second = await Scene.loadScene("fixture://scenes/graph.wscene");
    if (first.entityCount !== 2 || second.entities().length !== 2 || first.loadTime < 0) throw Error("scene metadata");
    if (first.get("root").id === second.get("root").id) throw Error("instances reused IDs");
    const child = second.get("child");
    if ((await child.requestParent().get())?.id !== second.get("root").id) throw Error("parent");
    const link = await child.requestComponent(Link).get();
    if (link.target.id !== second.get("root").id || link.text !== "root") throw Error("reference remapping");
    for (const path of ["fixture://invalid", "fixture://missing", "fixture://../outside"]) {
      let rejected = false;
      try { await Scene.loadScene(path); } catch { rejected = true; }
      if (!rejected) throw Error("invalid load succeeded: " + path);
    }
    if ((await Scene.listEntities()).length !== 4) throw Error("partial scene leaked entities");
    Engine.log("info", "FIXTURE_DONE");
  } catch (error) { Engine.log("error", "SCENE_FAILURE " + error.stack); Engine.log("info", "FIXTURE_DONE"); }
});`;
  const result = await runServer(source, { mode: "server", files: {
    "./scenes/graph.wscene": JSON.stringify({version:1,entities:[{script_id:"root",children:[{script_id:"child",components:{"fixture:link":{target:{$entity:"root"},text:"root"}}}]}]}),
    "./scenes/invalid.wscene": JSON.stringify({version:1,entities:[{}, {components:{"fixture:link":{$entity:"missing"}}}]}),
  }});
  expect(result.code).toBe(0);
  expect(result.stdout).toContain("FIXTURE_DONE");
  expect(result.stdout).not.toContain("SCENE_FAILURE");
});

test("implicit tick buffers execute pending entity chains and capture intermediate query results", async () => {
  const source = `
import { Component, Engine, Scene, BuiltInComponents } from "@whatever-engine/api";
export class Selected extends Component { ok = true; }
Engine.on("init", async () => {
  try {
    const parent = Scene.createEntity();
    const value = new BuiltInComponents.Transform({position:[1,0,0]});
    const child = Scene.createEntity().setComponent(value).setParent(parent);
    const before = child.requestComponent("core:transform");
    value.position[0] = 99;
    child.setComponent(new BuiltInComponents.Transform({position:[2,0,0]}));
    const after = child.requestComponent("core:transform");
    const ancestry = child.requestParent();
    const children = parent.requestChildren();
    const firstGet = before.get();
    if (firstGet !== before.get()) throw new Error("get repeated work");
    const [entity, parentEntity, first, second, actualParent, actualChildren] = await Promise.all([
      child.get(), parent.get(), firstGet, after.get(), ancestry.get(), children.get(),
    ]);
    if (first.position[0] !== 1 || second.position[0] !== 2) throw new Error("query order or snapshot");
    if (actualParent.id !== parentEntity.id || actualChildren[0].id !== entity.id) throw new Error("pending references");
    const transform = { position: [0,0,0], rotation: [0,0,0,1], scale: [2,3,4] };
    const moved = Scene.createEntity().setComponent(new BuiltInComponents.Transform(transform)).move([3,4,5]);
    const position = await moved.requestComponent("core:transform").get();
    if (position.position.join() !== "3,4,5" || position.scale.join() !== "2,3,4") throw new Error("native move");
    const selected = await Scene.createEntity().setComponent(new Selected()).requestComponent(Selected).get();
    if (!selected.ok) throw new Error("selected result");
    Engine.log("info", "FIXTURE_DONE");
  } catch (error) { Engine.log("error", "CHAIN_FAILURE " + error); }
});`;
  const result = await runServer(source, { mode: "server" });
  expect(result.stdout).toContain("FIXTURE_DONE");
  expect(result.stdout).not.toContain("CHAIN_FAILURE");
});

test("mod dependency order wins even when its batch arrives after another mod's batch", async () => {
  const source = `
import { Engine, Scene, Entity, BuiltInComponents } from "@whatever-engine/api";
Engine.on("init", () => Scene.createEntity().setComponent(new BuiltInComponents.Transform()));
Engine.on("tick", ({ tick_number }) => {
  const previous = new Entity("0:0").requestComponent("core:transform");
  new Entity("0:0").setComponent(new BuiltInComponents.Transform({position:[1,0,0]}));
  const deadline = Date.now() + 20;
  while (Date.now() < deadline) {}
  if (tick_number > 1) void previous.get().then(value => {
    if (value.position[0] !== 2) Engine.log("error", "ORDER_FAILURE " + value.position[0]);
    if (tick_number >= 4) Engine.log("info", "FIXTURE_DONE");
  });
});`;
  const serverSource = `
import { Engine, Entity, BuiltInComponents } from "@whatever-engine/api";
Engine.on("init", () => Engine.log("info", "SERVER_ONLY_READY"));
Engine.on("tick", () => new Entity("0:0").setComponent(new BuiltInComponents.Transform({position:[2,0,0]})));`;
  const result = await runServer(source, { mode: "server", serverSource });
  expect(result.stdout).not.toContain("ORDER_FAILURE");
  expect(result.stdout).toContain("FIXTURE_DONE");
});

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
  test("shutdown completes while a mod's async handler never resolves",async()=>{
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
    'const game=Network.Specification("fixture:game",{login:{serverbound:{hello:Network.Packet({name:Network.Type.String})},clientbound:{welcome:Network.Packet({id:Network.Type.I32.with({min:0}),maybe:Network.Type.Optional(Network.Type.Bool)})}}});'+
    'Engine.on("init",async()=>{try{'+
    'const bad=Network.Specification("fixture:bad",{s:{serverbound:{p:Network.Packet({n:Network.Type.I32.with({bogus:1})})}}});'+
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

test("pending ECS references, physics configuration and intermediate results round-trip through the real host",async()=>{
  const source=String.raw`
import {Component,Engine,Entity,Physics,Scene} from "@whatever-engine/api";
export class Refs extends Component {static readonly type="fixture:refs";target:Entity|null=null;nested:Entity[]=[];}
function check(value,message){if(!value)throw new Error(message);}
Engine.on("init",async()=>{try{
  const arena=Scene.createEntity().setComponent(new Physics.World({gravity:[0,0,0],substeps:2,rapier:{integration:{num_solver_iterations:8,friction_model:"Coulomb"}}}));
  const body=Scene.createEntity().move([0,4,0]).setComponent(Physics.RigidBody.dynamic({world:arena,ccd:true,rapier:{additional_solver_iterations:3}}));
  const left=Scene.createEntity().setComponent(new Physics.Collider({body,position:[-1,0,0],shape:Physics.Shape.sphere(0.5),mass:3}));
  const right=Scene.createEntity().setComponent(new Physics.Collider({body,position:[1,0,0],shape:Physics.Shape.box({size:[1,1,1]}),mass:4}));
  left.setParent(Scene.ref(right));
  const references=new Refs();references.target=left;references.nested=[right];body.setComponent(references);
  Physics.body(body).setLinearVelocity([3,1,0]);
  const selected=left.requestComponent("core:collider");
  const selectedState=Physics.body(body).state();
  const selectedRefs=body.requestComponent(Refs);
  const [entity,collider,state,refs,settings,parent]=await Promise.all([body.get(),selected.get(),selectedState.get(),selectedRefs.get(),arena.requestComponent("core:physics_world").get(),left.requestParent().get()]);
  check(parent.id===right.id,"typed entity reference parenting");
  check(collider instanceof Physics.Collider,"typed collider hydration");
  check(collider.body.id===entity.id,"pending body reference");
  check(refs.target instanceof Entity && refs.nested[0] instanceof Entity,"custom nested reference hydration");
  check(refs.target.id===left.id && refs.nested[0].id===right.id,"custom reference targets");
  check(Math.abs(state.mass-7)<0.001 && state.linearVelocity[0]===3 && state.world.id===arena.id,"mass, velocity and world state: "+JSON.stringify(state));
  check(settings.rapier.integration.num_solver_iterations===8 && settings.rapier.integration.dt===undefined,"world settings and engine dt");
  const config=await entity.requestComponent("core:rigid_body").get();
  config.world=arena;
  config.rapier.additional_solver_iterations=6;entity.setComponent(config);
  left.removeComponent("core:collider");
  const next=await Physics.body(entity).state().get();
  check(Math.abs(next.mass-4)<0.001 && next.linearVelocity[0]===3,"ordered removal/configuration must preserve velocity");
  const defaultWorld=await Physics.defaultWorld.get();
  check(defaultWorld instanceof Entity && defaultWorld.id.includes(":"),"default world entity");
  const defaultSettings=await defaultWorld.requestComponent("core:physics_world").get();
  check(defaultSettings instanceof Physics.World,"default world ECS configuration");
  try {await Physics.body(left).applyImpulse([1,0,0]).get();throw new Error("missing body accepted");}
  catch(error){check(String(error).includes("no active rigid body"),"native mutation error must reach Pending.get");}
  Engine.log("info","PHYSICS_REFS_OK");
}catch(error){Engine.log("error","PHYSICS_FAILURE "+error.stack);}Engine.log("info","FIXTURE_DONE");});`;
  const result=await runServer(source,{mode:"server"});
  expect(result.stdout).toContain("PHYSICS_REFS_OK");expect(result.stdout).not.toContain("PHYSICS_FAILURE");
});

test("physics advances after commands, queries see prior mutations and collisions return on later ticks",async()=>{
  const source=String.raw`
import {Engine,Physics,Scene} from "@whatever-engine/api";
let body,floor,ready=false,started=0,collision=false,force=false,done=false;
function finish(error){if(done)return;done=true;if(error)Engine.log("error","PHYSICS_FAILURE "+error.stack);else Engine.log("info","PHYSICS_TICKS_OK");Engine.log("info","FIXTURE_DONE");}
Physics.onEvent(event=>{
 if(event.kind==="started" && !event.sensor)collision=true;
 if(event.kind==="contactForce" && event.totalForceMagnitude>0 && event.totalForce.length===3)force=true;
});
Engine.on("init",async()=>{try{
 floor=Scene.createEntity().move([0,-0.5,0]).setComponent(Physics.RigidBody.fixed()).setComponent(new Physics.Collider({shape:Physics.Shape.box({size:[30,1,30]}),events:["collision","contactForce"],rapier:{contact_force_event_threshold:0}}));
 body=Scene.createEntity().move([0,3,0]).setComponent(Physics.RigidBody.dynamic()).setComponent(new Physics.Collider({shape:Physics.Shape.box({size:[1,1,1]}),mass:2}));
 const first=await Physics.world().castRay([0,10,0],[0,-1,0],{maxDistance:20}).get();
 if(first.collider.id!==body.id || Math.abs(first.distance-6.5)>0.001)throw new Error("same-batch creation query");
 Physics.body(body).teleport([3,3,0]);
 const second=await Physics.world().castRay([0,10,0],[0,-1,0],{maxDistance:20,flags:Physics.QueryFilterFlag.EXCLUDE_SENSORS}).get();
 if(second.collider.id!==floor.id)throw new Error("ordered teleport query");
 ready=true;
}catch(error){finish(error);}});
Engine.on("tick",async({tick_number})=>{if(!ready||done)return;if(!started)started=tick_number;if(tick_number-started<180)return;try{
 const state=await Physics.body(body).state().get();
 const transform=await body.requestComponent("core:transform").get();
 if(Math.abs(state.position[1]-0.5)>0.03 || Math.abs(transform.position[1]-0.5)>0.03)throw new Error("body did not settle or write back to ECS");
 if(!collision || !force)throw new Error("collision/force events missing");
 finish();
}catch(error){finish(error);}});`;
  const result=await runServer(source,{mode:"server"});
  expect(result.stdout).toContain("PHYSICS_TICKS_OK");expect(result.stdout).not.toContain("PHYSICS_FAILURE");
});

test("the shipped physics playground scene simulates and cleans up headlessly",async()=>{
  const source=String.raw`
import {Engine,Physics,Scene} from "@whatever-engine/api";
import {createDemoScene,destroyDemoScene,spawnCubes} from "./physics-scene.ts";
let scene,ready=false,started=0,done=false;
function finish(error){if(done)return;done=true;if(error)Engine.log("error","PHYSICS_FAILURE "+error.stack);else Engine.log("info","PHYSICS_SCENE_OK");Engine.log("info","FIXTURE_DONE");}
Engine.on("init",async()=>{try{
 scene=await createDemoScene(undefined,"core://playground");
 const cubes=await Scene.findByTag("physics_demo:cube");if(cubes.length!==24)throw new Error("initial cubes");
 const transforms=await Promise.all(cubes.map(entity=>entity.requestComponent("core:transform").get()));
 if(transforms.some(transform=>Math.abs(transform.position[0]-20)>4.51))throw new Error("authored spawn point was ignored");
 if(await scene.baseCube.getEnabled())throw new Error("base cube must remain disabled");
 const colliders=await Scene.requestQuery(["core:collider"]).get();if(colliders.length!==30)throw new Error("compound frame colliders");
 spawnCubes(scene,3);
 const more=await Scene.findByTag("physics_demo:cube");if(more.length!==27)throw new Error("additional cubes");
 ready=true;
}catch(error){finish(error);}});
Engine.on("tick",async({tick_number})=>{if(!ready||done)return;if(!started)started=tick_number;if(tick_number-started<120)return;try{
 const state=await Physics.body(scene.player).state().get();
 if(Math.abs(state.position[1]-1)>0.04 || state.mass!==75)throw new Error("player capsule did not settle");
 const statuses=await Scene.requestQuery(["core:physics_status"]).get();
 for(const row of statuses)for(const status of Object.values(row.components["core:physics_status"]))if(!status.active)throw new Error("inactive playground physics: "+status.error);
 const cubes=await Scene.findByTag("physics_demo:cube");
 const cubeStates=await Promise.all(cubes.map(entity=>entity.requestComponent("core:rigid_body_state").get()));
 if(cubeStates.some(state=>state.position[1]<0))throw new Error("cube fell through floor");
 destroyDemoScene(scene);
 const remaining=await Scene.listEntities();if(remaining.length!==0)throw new Error("playground leaked "+remaining.length+" entities");
 finish();
}catch(error){finish(error);}});`;
  const sceneSource=readFileSync(join(repository,"games/test/mods/physics_demo/scripts/scene.ts"),"utf8");
  const authored=JSON.parse(readFileSync(join(repository,"games/test/mods/physics_demo/scenes/playground.wscene"),"utf8"));
  authored.entities.find((entity:{script_id?:string})=>entity.script_id==="cube_spawn").components["core:transform"].position[0]=20;
  const result=await runServer(source,{mode:"server",coreAssets:join(repository,"games/test/mods/physics_demo/assets"),files:{"physics-scene.ts":sceneSource,"scenes/playground.wscene":JSON.stringify(authored).replaceAll("physics_demo://","core://")}});
  expect(result.stdout).toContain("PHYSICS_SCENE_OK");expect(result.stdout).not.toContain("PHYSICS_FAILURE");
});

test("tick-rate changes preserve the current physics delta and affect the next tick",async()=>{
  const source=String.raw`
import {Engine,Physics,Scene} from "@whatever-engine/api";
Engine.on("init",async()=>{try{
 const arena=Scene.createEntity().setComponent(new Physics.World({gravity:[0,0,0]}));
 const body=Scene.createEntity().setComponent(Physics.RigidBody.dynamic({world:arena,rapier:{additional_mass:1}}));
 Physics.body(body).setLinearVelocity([1,0,0]);
 Engine.setTickRate(20);
 await body.get();
 const first=await Physics.body(body).state().get();
 const second=await Physics.body(body).state().get();
 if(Math.abs(first.position[0]-0.01)>0.0001 || Math.abs(second.position[0]-0.06)>0.0001)throw new Error("tick deltas were not stable: "+JSON.stringify([first.position,second.position]));
 Engine.log("info","PHYSICS_CLOCK_OK");
}catch(error){Engine.log("error","PHYSICS_FAILURE "+error.stack);}Engine.log("info","FIXTURE_DONE");});`;
  const result=await runServer(source,{mode:"server"});
  expect(result.stdout).toContain("PHYSICS_CLOCK_OK");expect(result.stdout).not.toContain("PHYSICS_FAILURE");
});


test("clones remap hierarchy references and disabled entities leave active queries",async()=>{
 const source=String.raw`
import {Component,Entity,Engine,Scene} from "@whatever-engine/api";
export class ChildLink extends Component {child:Entity|null=null;}
Engine.on("init",async()=>{try {
 const root=Scene.createEntity().setEnabled(false);
 const child=Scene.createEntity().setParent(root).addTag("fixture:active");
 const linked=new ChildLink();linked.child=child;root.setComponent(linked);
 const copy=root.clone({enabled:true});
 const children=await copy.requestChildren().get();
 if(children.length!==1)throw Error("clone hierarchy missing");
 const link=await copy.requestComponent(ChildLink).get();
 if(link.child.id!==children[0].id)throw Error("clone reference did not remap");
 if(!await copy.getEnabled()||await root.getEnabled())throw Error("enabled state mismatch");
 if((await Scene.findByTag("fixture:active")).length!==1)throw Error("inactive hierarchy queried");
 copy.setEnabled(false);
 if((await Scene.findByTag("fixture:active")).length!==0)throw Error("disable did not propagate");
 root.destroy();copy.destroy();let rejected=false;
 try{await root.clone().get();}catch{rejected=true;}
 if(!rejected)throw Error("stale clone did not reject");
 Engine.log("info","ENTITY_LIFECYCLE_OK");
 }catch(error){Engine.log("error","ENTITY_LIFECYCLE_FAILURE "+error.stack);}Engine.log("info","FIXTURE_DONE");});`;
 const result=await runServer(source,{mode:"server"});
 expect(result.stdout).toContain("ENTITY_LIFECYCLE_OK");expect(result.stdout).not.toContain("ENTITY_LIFECYCLE_FAILURE");
});

test("core initial scene is live before init and is not instantiated again by the API",async()=>{
 const scene={version:1,entities:[{name:"Startup Camera",script_id:"camera",components:{"core:transform":{position:[0,0,5],rotation:[0,0,0,1],scale:[1,1,1]},"core:camera":{}}},{name:"Template",script_id:"template",enabled:false,tags:["fixture:template"],components:{}}]};
 const source=String.raw`import {Engine,Scene} from "@whatever-engine/api";
Engine.on("init",async()=>{try{const startup=Scene.initialScene();if(!startup||startup.entityCount!==2||startup.path!=="core://startup")throw Error("startup metadata missing");
 if((await Scene.listEntities()).length!==2)throw Error("startup loaded twice");
 if(await startup.get("template").getEnabled())throw Error("startup disabled flag lost");Engine.log("info","INITIAL_SCENE_OK");
 }catch(error){Engine.log("error","INITIAL_SCENE_FAILURE "+error.stack);}Engine.log("info","FIXTURE_DONE");});`;
 const result=await runServer(source,{mode:"server",initialScene:"core://startup",files:{"scenes/startup.wscene":JSON.stringify(scene)}});
 expect(result.stdout).toContain("INITIAL_SCENE_OK");expect(result.stdout).not.toContain("INITIAL_SCENE_FAILURE");
});


test.each(["help", "clear", "quit", "scene"])("core mod command %s cannot collide with the engine namespace", async name => {
  const source = `import {Console} from "@whatever-engine/api"; Console.register({name:${JSON.stringify(name)},handler:()=>"SHOULD_NOT_RUN"});`;
  const result = await runServer(source, { mode: "server", expectedCode: 1, coreScript: true });
  expect(result.stdout).toContain(`command namespace collision: 'core:${name}'`);
  expect(result.stdout).toContain("simulation stopped");
  expect(result.stdout).not.toContain("SHOULD_NOT_RUN");
});

test("colliding mod commands both require namespaces and dispatch their original handler paths", async () => {
  const source = `import {Engine,Console} from "@whatever-engine/api";
Console.register({name:"probe",handler:()=>"CORE_PROBE"});
Engine.on("init",()=>Engine.log("info","CONSOLE_READY"));`;
  const serverSource = `import {Engine,Console} from "@whatever-engine/api";
Console.register({name:"probe",handler:()=>"FIXTURE_DONE SERVER_PROBE"});
Engine.on("init",()=>Engine.log("info","SERVER_ONLY_READY"));`;
  const result = await runServer(source, {mode:"server",serverSource,commandsOnReady:["probe", "server_only:probe"]});
  expect(result.stdout).toContain("ambiguous command 'probe' — use fixture:probe or server_only:probe");
  expect(result.stdout).toContain("SERVER_PROBE");
  expect(result.stdout).not.toContain("CORE_PROBE");
});

test.each(["fixture://graph", "fixture://scenes/graph.wscene", "fixture://missing", "fixture://invalid"])("builtin scene load %s runs on the simulation worker with atomic validation", async path => {
  const valid = !path.endsWith("missing") && !path.endsWith("invalid");
  const source = `import {Component,Entity,Engine,Scene} from "@whatever-engine/api";
export class ConsoleLink extends Component {static readonly type="fixture:link";target:Entity|null=null;}
let ready=false, checked=false;
Engine.on("init",async()=>{await Scene.createEntity().get();ready=true;Engine.log("info","CONSOLE_READY");});
Engine.on("tick",async({tick_number})=>{if(!ready||checked||tick_number<15)return;checked=true;
try {
 const entities=await Scene.listEntities();
 if(entities.length!==${valid ? 3 : 1})throw Error("wrong entity count: "+entities.length);
 if(${valid}) {
  const child=entities.find(entity=>entity.id==="2:0");
  const parent=await child.requestParent().get();
  const ref=await child.requestComponent(ConsoleLink).get();
  if(ref.target.id!==parent.id)throw Error("scene reference/hierarchy mismatch");
 }
 Engine.log("info","SCENE_CONSOLE_OK");
}catch(error){Engine.log("error","SCENE_CONSOLE_FAILURE "+error);}
Engine.log("info","FIXTURE_DONE");});`;
  const result = await runServer(source, {mode:"server",commandsOnReady:[`scene load ${path}`],files:{
    "./scenes/graph.wscene": JSON.stringify({version:1,entities:[{script_id:"root",children:[{components:{"fixture:link":{target:{$entity:"root"}}}}]}]}),
    "./scenes/invalid.wscene": JSON.stringify({version:1,entities:[{}, {components:{"fixture:link":{$entity:"missing"}}}]}),
  }});
  expect(result.stdout).toContain("SCENE_CONSOLE_OK");
  expect(result.stdout).not.toContain("SCENE_CONSOLE_FAILURE");
  expect(result.stdout).toContain(valid ? "Loaded 2 entities in" : "console: command failed:");
});


test("a mod cannot hijack builtin quit, which remains usable while its handler is pending", async () => {
  const source = `import {Engine} from "@whatever-engine/api";
Engine.on("init",()=>Engine.log("info","CONSOLE_READY"));`;
  const serverSource = `import {Engine,Console} from "@whatever-engine/api";
Console.register({name:"quit",handler:()=>{Engine.log("info","QUIT_HANDLER_PENDING");return new Promise(()=>{});}});
Engine.on("init",()=>Engine.log("info","SERVER_ONLY_READY"));`;
  const result = await runServer(source, {mode:"server",serverSource,
    commandsOnReady:["quit", "server_only:quit"], stopAt:"QUIT_HANDLER_PENDING", shutdownCommand:"core:quit"});
  expect(result.stdout).toContain("ambiguous command 'quit' — use core:quit or server_only:quit");
  expect(result.stdout).toContain("QUIT_HANDLER_PENDING");
  expect(result.shutdownMs).toBeLessThan(3000);
});


test("custom component lifecycles and field updates run in the native simulation", async () => {
  const source = `
import { Component, Engine, Scene, Type, field } from "@whatever-engine/api";
export class Counter extends Component {
  static readonly type = "fixture:counter";
  count = field(Type.I32.with({min:0,max:10}),0);
  samples: number[] = [];
  private starts = 0;
  override async start() {
    this.starts++;
    if (!(await this.entity.hasTag("tracked"))) throw Error("tag missing in start");
    if (this.entity.getComponent(Counter)?.count !== this.count) throw Error("component snapshot mismatch");
    Engine.log("info","COMPONENT_STARTED");
  }
  override tick(dt: number) {
    if (this.starts !== 1 || dt <= 0) throw Error("lifecycle order");
    this.samples.push(++this.count);
    if (this.count === 3) this.entity.destroy();
  }
  override destroy() { Engine.log("info","COMPONENT_DESTROYED"); Engine.log("info","FIXTURE_DONE"); }
}
Engine.on("init",()=>Scene.createEntity().addTag("tracked").setComponent(new Counter()));`;
  const result = await runServer(source, {mode:"server"});
  expect(result.stdout).toContain("COMPONENT_STARTED");
  expect(result.stdout).toContain("COMPONENT_DESTROYED");
  expect(result.stdout).not.toContain("failed:");
});

test("native tags enforce capacity, clone membership and filter disabled entities", async () => {
  const source = `
import { Engine, Scene } from "@whatever-engine/api";
Engine.on("init",async()=>{
  try {
    const entity = Scene.createEntity().addTag("enemy").addTag("flying");
    for(let i=2;i<32;i++) entity.addTag("tag"+i);
    entity.addTag("enemy");
    let capped=false;try{entity.addTag("overflow");}catch{capped=true;}if(!capped)throw Error("tag cap did not throw");
    if((await entity.getTags()).length!==32)throw Error("tag cap");
    if(await entity.hasTag("overflow"))throw Error("overflow accepted");
    const clone=entity.clone();
    if(!(await clone.hasTag("flying")))throw Error("clone tags");
    clone.setEnabled(false);
    if((await Scene.findByTags({all:["enemy"],any:["flying","walking"],none:["dead"]})).length!==1)throw Error("tag query");
    if((await Scene.findByTag("enemy",{includeDisabled:true})).length!==2)throw Error("disabled filter");
    entity.removeTag("enemy");
    if((await Scene.findByTag("enemy")).length!==0)throw Error("tag removal");
    Engine.log("info","FIXTURE_DONE");
  } catch(error){Engine.log("error","TAG_FAILURE "+error);Engine.log("info","FIXTURE_DONE");}
});`;
  const result = await runServer(source,{mode:"server"});
  expect(result.stdout).not.toContain("TAG_FAILURE");
});


test("removing and reattaching a component within one batch starts a new instance", async () => {
  const source = `
import {Component,Engine,Scene} from "@whatever-engine/api";
let starts=0,destroys=0;
export class Replaced extends Component {
  override start(){starts++;Engine.log("info","INSTANCE_START "+starts);}
  override tick(){if(starts===1)this.entity.removeComponent(Replaced).setComponent(new Replaced());else this.entity.destroy();}
  override destroy(){destroys++;if(destroys===2){if(starts!==2)throw Error("instance reused");Engine.log("info","FIXTURE_DONE");}}
}
Engine.on("init",()=>Scene.createEntity().setComponent(new Replaced()));`;
  const result=await runServer(source,{mode:"server"});
  expect(result.stdout).toContain("INSTANCE_START 1");
  expect(result.stdout).toContain("INSTANCE_START 2");
  expect(result.stdout).not.toContain("instance reused");
});

test("scene snapshots provide immediate immutable reads and reconcile native generations", async () => {
  const result = await runServer(`
import {Engine,Scene,Entity,BuiltInComponents as C,Profiler} from "@whatever-engine/api";
Engine.on("init",async()=>{try {
 const parent=Scene.createEntity().setComponent(new C.Transform({position:[10,0,0]}));
 const child=Scene.createEntity().setComponent(new C.Transform({position:[1,0,0]})).setParent(parent).addTag("snapshot");
 const first=child.getComponent("core:transform");
 if(!child.exists||!child.isPending||first.position[0]!==1||!Object.isFrozen(first.position))throw Error("immediate read");
 child.move([2,0,0]).setEulerDegrees(0,90,0);
 if(first.position[0]!==1||child.getComponent("core:world_transform").position[0]!==12)throw Error("retained/local world transform");
 await child.get();
 const retained=Scene.snapshot();const id=child.id;
 if(!retained.has(id)||!Scene.find(id)||child.isPending)throw Error("commit missing");
 const native=await child.requestComponent("core:transform").get();
 if(native.position[0]!==2||Math.abs(native.rotation[1]-Math.SQRT1_2)>0.001)throw Error("patch lost fields");
 child.destroy();if(child.exists||parent.getChildren().length)throw Error("local removal");
 const next=await Scene.createEntity().get();
 if(new Entity(id).exists||!next.exists||!retained.has(id))throw Error("generation/retained snapshot");
 if(!Profiler.latest()||Profiler.now()<0||Profiler.memory().rss<=0)throw Error("native metrics missing");
 Engine.log("info","SNAPSHOT_OK");
 }catch(error){Engine.log("error","SNAPSHOT_FAILURE "+error.stack);}Engine.log("info","FIXTURE_DONE");});`, {mode:"server"});
  expect(result.stdout).toContain("SNAPSHOT_OK");
  expect(result.stdout).not.toContain("SNAPSHOT_FAILURE");
});
