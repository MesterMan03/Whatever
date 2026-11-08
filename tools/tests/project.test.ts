import { text } from "node:stream/consumers";
import { spawnCaptured } from "../process.ts";
import { expect, test } from "bun:test";
import { existsSync, mkdtempSync, mkdirSync, readFileSync, rmSync, writeFileSync, copyFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const repository = resolve(dirname(fileURLToPath(import.meta.url)), "../..");
const extension = process.platform === "win32" ? ".exe" : "";
const binary = (name: string) => join(repository, "target/debug", name + extension);
for (const name of ["wmod", "Whatever", "whatever-launcher"]) {
  if (!existsSync(binary(name))) throw new Error("Run bun run build:native before test:project");
}
async function wmod(args: string[], success = true, executable=binary("wmod")) {
  const child = spawnCaptured([executable, ...args]);
  const [stdout, stderr, code] = await Promise.all([text(child.stdout), text(child.stderr), child.exited]);
  if (success && code !== 0) throw new Error(stdout + stderr);
  return { stdout, stderr, code };
}

test("CLI scaffolding, editor validation, branded deployment and relocated scene runtime share one project format", async () => {
  const directory = mkdtempSync(join(tmpdir(), "whatever-deployment-"));
  let player: ReturnType<typeof spawnCaptured> | undefined;
  try {
    const project = join(directory, "source");
    await wmod(["project", "create", project, "--id", "packaged_fixture", "--name", "Branded Fixture", "--core", join(repository, "engine/resources/core")]);
    await wmod(["mod","create",join(project,"mods/dev_tools"),"--id","dev_tools","--project",project,"--no-scripts"]);
    writeFileSync(join(project,"whatever.wproject"),readFileSync(join(project,"whatever.wproject"),"utf8").replace("exclude_mods = []",'exclude_mods = ["dev_tools"]'));
    const mod = join(project, "mods/core");
    expect(existsSync(join(mod, "scenes/main.wscene"))).toBe(true);
    expect(existsSync(join(project, "tsconfig.json"))).toBe(true);
    expect(existsSync(join(project, ".whatever/tsconfig.sdk.json"))).toBe(true);
    expect(existsSync(join(mod, "types/whatever-engine.d.ts"))).toBe(false);
    const source = `import { Component, Engine, Scene } from "@whatever-engine/api";
export class GameCounter extends Component { static readonly type = "core:GameCounter"; value = 0; }
Engine.on("init", async () => {
  const scene = await Scene.loadScene("core://main");
  if (Engine.mode !== "server" || scene.entityCount !== 2 || !scene.get("camera").id) throw Error("packaged scene mismatch");
  Engine.log("info", "PACKAGED_SCENE_READY");
});`;
    writeFileSync(join(mod, "scripts/index.ts"), source);
    mkdirSync(join(mod,"node_modules")); writeFileSync(join(mod,"node_modules/private.txt"), "development only");
    await wmod(["project", "check", project]);
    const before = readFileSync(join(project,"whatever.wproject"),"utf8");
    expect((await wmod(["project","add",project,mod],false)).code).not.toBe(0);
    expect(readFileSync(join(project,"whatever.wproject"),"utf8")).toBe(before);
    const kit = join(directory, "kit");
    await wmod(["runtime-kit", "--player", binary("Whatever"), "--launcher", binary("whatever-launcher"), "--source",repository,"--output",kit]);
    copyFileSync(binary("wmod"),join(kit,"bin/wmod"+extension));
    const installedWmod=join(kit,"bin/wmod"+extension);
    const output = join(directory,"game");
    mkdirSync(output);
    // An installed editor/CLI locates its own support payload; no runtime directory is supplied.
    await wmod(["package", project, "--output", output],true,installedWmod);
    expect(existsSync(join(output,"Branded Fixture"+extension))).toBe(true);
    expect(existsSync(join(output,"bin/Whatever"+extension))).toBe(true);
    expect(existsSync(join(output,"bin/native-libraries.json"))).toBe(true);
    expect(existsSync(join(output,"mods/packaged_fixture/node_modules"))).toBe(false);
    expect(existsSync(join(output,"branding/icon.png"))).toBe(true);
    expect(existsSync(join(output,"mods/dev_tools"))).toBe(false);
    expect(readFileSync(join(output,"mods/core/meta.toml"),"utf8")).toContain('id = "packaged_fixture"');
    expect(readFileSync(join(output,"mods/core/meta.toml"),"utf8")).toContain('name = "Branded Fixture"');
    expect(readFileSync(join(output,"whatever.wproject"),"utf8")).toContain('id = "packaged_fixture"');
    if(process.platform==="win32") {
      const executable=join(output,"Branded Fixture.exe").replaceAll("'","''");
      const info=spawnCaptured(["powershell","-NoProfile","-Command",`[System.Diagnostics.FileVersionInfo]::GetVersionInfo('${executable}').FileDescription`]);
      expect((await text(info.stdout)).trim()).toBe("Branded Fixture");
      expect(await info.exited).toBe(0);
    }
    const unrelated=join(directory,"other-files");mkdirSync(unrelated);writeFileSync(join(unrelated,"keep.txt"),"keep");
    expect((await wmod(["package",project,"--output",unrelated],false,installedWmod)).code).not.toBe(0);
    expect(readFileSync(join(unrelated,"keep.txt"),"utf8")).toBe("keep");
    // Rebuilding a managed output replaces the previous build safely.
    await wmod(["package",project,"--output",output],true,installedWmod);
    await wmod(["project","check",output]);
    const editor = spawnCaptured([binary("whatever-editor"), output, "--check"]);
    const [editorOutput, editorError, editorCode] = await Promise.all([
      text(editor.stdout), text(editor.stderr), editor.exited,
    ]);
    if (editorCode !== 0) throw Error(editorOutput + editorError);
    writeFileSync(join(mod,"scripts/index.ts"),'throw Error("source must not run after packaging")');
    const launcher = join(output,"Branded Fixture"+extension);
    player = spawnCaptured([launcher,"--mode","server"], {cwd:directory,env:{...process.env,PATH:"",RUST_LOG:"whatever_engine=info"},timeout:15000});
    const active = player;
    let stdout = ""; let stopped = false;
    const read = (async () => {
      for await (const bytes of active.stdout as ReadableStream<Uint8Array>) {
        stdout += new TextDecoder().decode(bytes);
        if (!stopped && stdout.includes("PACKAGED_SCENE_READY")) { stopped = true; active.stdin.write("quit\n"); }
      }
    })();
    const [stderr,code] = await Promise.all([text(active.stderr),active.exited,read]);
    if (code !== 0) throw Error(stdout+stderr);
    expect(stdout).toContain("PACKAGED_SCENE_READY");
    expect(stdout).not.toContain("source must not run");
    const invalid = spawnCaptured([launcher,"--mode","bad"],{cwd:directory});
    await Promise.all([text(invalid.stdout),text(invalid.stderr)]);
    expect(await invalid.exited).not.toBe(0);
  } finally {
    if (player?.exitCode === null) { player.kill(); await player.exited; }
    rmSync(directory,{recursive:true,force:true});
  }
}, 60000);
