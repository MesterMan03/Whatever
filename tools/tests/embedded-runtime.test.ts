import { text } from "node:stream/consumers";
import { spawnCaptured } from "../process.ts";
import { collectNativeLibraries } from "../native-libraries.ts";
import { nativeBinary } from "./native.ts";
import { afterAll, beforeAll, expect, test } from "bun:test";
import { copyFileSync, existsSync, mkdirSync, mkdtempSync, rmSync, writeFileSync } from "node:fs";
import { basename, dirname, join, resolve } from "node:path";
import { tmpdir } from "node:os";

const repository = resolve(import.meta.dirname, "../..");
const suffix = process.platform === "win32" ? ".exe" : "";
const built = nativeBinary("Whatever");
let directory: string;
let player: string;
beforeAll(() => {
  if (!existsSync(built)) throw Error("Run bun run build:native first");
  directory = mkdtempSync(join(tmpdir(), "whatever-portable-vm-"));
  player = join(directory, "Whatever" + suffix);
  copyFileSync(built, player);
  collectNativeLibraries(dirname(built), directory, [basename(built)]);
  // A leftover old host must never be launched, even when placed beside the player.
  writeFileSync(join(directory, "runtime" + suffix), "not an executable");
});
afterAll(() => rmSync(directory, { recursive: true, force: true }));

async function run(source: string, files: Record<string, string> = {}, expected = "PORTABLE_OK") {
  const project = mkdtempSync(join(directory, "game-"));
  const put = (name: string, value: string) => {
    const path = join(project, name);
    mkdirSync(dirname(path), { recursive: true });
    writeFileSync(path, value);
  };
  put("mods/core/mod.toml", '[mod]\nid="core"\nname="Portable"\nversion="1.0.0"\n[script]\nentry="scripts/index.ts"\n');
  put("mods/core/meta.toml", '[game]\nid="portable_vm"\nname="Portable"\ntick_rate=60\n');
  put("mods/core/scripts/index.ts", source);
  put("mods/observer/mod.toml", '[mod]\nid="observer"\nname="Observer"\nversion="1.0.0"\n[dependencies]\ncore="*"\n[script]\nentry="index.ts"\n');
  put("mods/observer/index.ts", "import {Engine} from '@whatever-engine/api';let first=true;Engine.on('tick',()=>{if(first){first=false;Engine.log('info','HEALTHY_RUNNING');}});");
  for (const [name, value] of Object.entries(files)) put("mods/core/" + name, value);
  const active = spawnCaptured([player, "--mode", "server", "--mods", join(project, "mods")], {
    cwd: directory,
    env: {
      PATH: "", RUST_LOG: "whatever_engine=info", NO_COLOR: "1",
      SystemRoot: process.env.SystemRoot, WINDIR: process.env.WINDIR,
      TEMP: tmpdir(), TMP: tmpdir(),
    },
    timeout: 15_000,
  });
  let output = "";
  let finished = false;
  const reading = (async () => {
    for await (const bytes of active.stdout) {
      output += new TextDecoder().decode(bytes);
      if (!finished && output.includes(expected)) {
        finished = true;
        active.stdin.write("quit\n");
      }
    }
  })();
  try {
    const [stderr, code] = await Promise.all([text(active.stderr), active.exited, reading]);
    if (code !== 0 || !finished) throw Error(`Portable player exited ${code}\n${output}\n${stderr}`);
    return output;
  } finally {
    if (active.exitCode === null) { active.kill(); await active.exited; }
    rmSync(project, { recursive: true, force: true });
  }
}

test("a relocated player compiles raw TypeScript, local ESM and JSON with an empty PATH", async () => {
  const source = `import {Component,Engine,Scene,BuiltInComponents as C,Physics} from '@whatever-engine/api';
import {answer} from './answer.ts';
import config from './config.json';
import common from './helper.cjs';
let destroyed=0;
export class Counter extends Component {value=0; award(n:number){this.value+=n;} override destroy(){if(this.value!==42)throw Error('cleanup field read');destroyed++;}}
export class Caller extends Component {session:Counter|null=null; call(){this.session?.award(answer);}}
export class SelfRemoving extends Component {override start(){this.entity.destroy();} override tick(){throw Error('removed component ticked');}}
Engine.on('init',()=>{
  if(answer!==config.answer || common.answer!==42 || Engine.mode!=='server')throw Error('sources or mode');
  const a=Scene.createEntity().setComponent(new C.Transform()).setComponent(new Counter());
  const t=a.getComponent('core:transform')!;
  t.position[0]=4;
  if(a.getComponent('core:transform')!.position[0]!==4)throw Error('delayed nested write');
  t.addX(3);
  if(t.position[0]!==7)throw Error('native helpers');
  const retained=Scene.snapshot();
  a.move([9,0,0]);
  if(t.position[0]!==9||retained.get(a.id)!.components['core:transform'].position[0]!==7)throw Error('live/snapshot');
  const b=Scene.createEntity(); const c=new Caller();c.session=a.getComponent(Counter);b.setComponent(c);
  b.getComponent(Caller)!.call();
  if(!(b.getComponent(Caller)!.session instanceof Counter))throw Error('component reference prototype');
  if(a.getComponent(Counter)!.value!==42)throw Error('component callback delayed');
  const stale=t.position;
  a.removeComponent('core:transform').setComponent(new C.Transform());
  let rejected=false;try{stale[0]=3;}catch{rejected=true;}
  if(!rejected)throw Error('stale handle');
  if(Physics.world().castRay([0,10,0],[0,-1,0],{maxDistance:20})!==null)throw Error('synchronous raycast');
  a.destroy();
  if(destroyed!==1)throw Error('component cleanup was delayed');
  Scene.createEntity().setComponent(new SelfRemoving());
  if([typeof process,typeof require,typeof fetch,typeof WebSocket,typeof Worker].some(v=>v!=='undefined'))throw Error('ambient capability');
  Engine.log('info','PORTABLE_OK');
});`;
  const output = await run(source, { "scripts/answer.ts": "export const answer:number=42;", "scripts/config.json": '{"answer":42}', "scripts/helper.cjs": "module.exports = require('./config.json');" });
  expect(output).not.toContain("mod initialization failed");
  expect(output).not.toContain("removed component ticked");
});

test("source changes compile on the destination machine and do not reuse stale transforms", async () => {
  for (const answer of [13, 29]) {
    const output = await run(`import {Engine} from '@whatever-engine/api';import {value} from './value.ts';
Engine.on('init',()=>{Engine.log('info','PORTABLE_OK '+value);});`, {
      "scripts/value.ts": `export const value:number=${answer};`,
    });
    expect(output).toContain("PORTABLE_OK " + answer);
  }
});

test.each(["node:fs", "node:child_process", "some-package", "../../outside.ts", "https://example.com/code.js"])(
  "the native module loader rejects %s", async specifier => {
    const output = await run(`import '${specifier}';`, {}, "mod initialization failed");
    expect(output).toContain("mod initialization failed");
  },
);

test("a runaway mod exhausts fuel while another mod keeps running", async () => {
  const output = await run(`import {Engine} from '@whatever-engine/api';Engine.on('tick',()=>{while(true){}});`, {}, "mod execution failed");
  expect(output).toContain("mod execution failed");
  expect(output).toContain("HEALTHY_RUNNING");
  expect(output).toContain("simulation stopped");
});
