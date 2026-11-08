import { text } from "node:stream/consumers";
import { spawnCaptured } from "../process.ts";
import {test,expect} from "vitest";
import {mkdtempSync,rmSync} from "node:fs";
import {join,resolve,dirname} from "node:path";
import {tmpdir} from "node:os";
import {fileURLToPath} from "node:url";
const repository=resolve(dirname(fileURLToPath(import.meta.url)),"../..");
test("shipped server suite registers the shared protocol and runs consolidated diagnostics",async()=>{
  const directory=mkdtempSync(join(tmpdir(),"whatever-shipped-"));
  const child=spawnCaptured([join(repository,"target/debug/Whatever"+(process.platform==="win32"?".exe":"")),"--mods",join(repository,"games/test/mods"),"--mode","server"],{
    cwd:directory,env:{...process.env,RUST_LOG:"whatever_engine=info",PATH:"",DISPLAY:"",WAYLAND_DISPLAY:""},timeout:20_000,
  });
  let stdout="",stage=0;
  const read=(async()=>{for await(const bytes of child.stdout){
    stdout+=new TextDecoder().decode(bytes);
    if(stage===0&&stdout.includes("Server codec:")&&stdout.includes("Debug suite ready in server mode")){stage=1;child.stdin.write("netdemo spec\n");}
    if(stage===1&&stdout.includes("clientbound chat.message")){stage=2;child.stdin.write("dev file\n");}
    if(stage===2&&stdout.includes("File broker roundtrip passed")){stage=3;child.stdin.write("dev sandbox\n");}
    if(stage===3&&stdout.includes("runtime policy checks passed")){stage=4;child.stdin.write("dev message debug\n");}
    if(stage===4&&stdout.includes('"suite":"debug"')){stage=5;child.stdin.write("quit\n");}
  }})();
  try {
    const [code,stderr]=await Promise.all([child.exited,text(child.stderr),read]);
    expect({code,stderr,stage,stdout:stage===5?"passed":stdout}).toEqual({code:0,stderr:"",stage:5,stdout:"passed"});
    expect(stdout).not.toContain("Network example failed");
    expect(stdout).not.toContain("script_test");expect(stdout).not.toContain("shader_test");
  } finally {if(child.exitCode===null){child.kill();await child.exited;}rmSync(directory,{recursive:true,force:true});}
});
