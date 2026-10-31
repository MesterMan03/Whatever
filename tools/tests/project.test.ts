import { expect, test } from "bun:test";
import { existsSync, mkdtempSync, mkdirSync, readFileSync, rmSync, writeFileSync, copyFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const repository = resolve(dirname(fileURLToPath(import.meta.url)), "../..");
const extension = process.platform === "win32" ? ".exe" : "";
const binary = (name: string) => join(repository, "target/debug", name + extension);
for (const name of ["wmod", "Whatever", "whatever-launcher"]) {
  if (!existsSync(binary(name))) throw new Error("Run cargo build --workspace before test:project");
}
async function wmod(args: string[], success = true, executable=binary("wmod")) {
  const child = Bun.spawn([executable, ...args], { stdout: "pipe", stderr: "pipe" });
  const [stdout, stderr, code] = await Promise.all([new Response(child.stdout).text(), new Response(child.stderr).text(), child.exited]);
  if (success && code !== 0) throw new Error(stdout + stderr);
  return { stdout, stderr, code };
}

test("CLI scaffolding, editor validation, branded deployment and relocated scene runtime share one project format", async () => {
  const directory = mkdtempSync(join(tmpdir(), "whatever-deployment-"));
  let player: ReturnType<typeof Bun.spawn> | undefined;
  try {
    const project = join(directory, "source");
    await wmod(["project", "create", project, "--id", "packaged_fixture", "--name", "Branded Fixture", "--core", join(repository, "mods/core")]);
    const mod = join(project, "mods/packaged_fixture");
    expect(existsSync(join(mod, "scenes/main.wscene"))).toBe(true);
    expect(existsSync(join(mod, "tsconfig.json"))).toBe(true);
    const source = `import { Engine, Scene } from "@whatever-engine/api";
Engine.on("init", async () => {
  const scene = await Scene.loadScene("packaged_fixture://main");
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
    await wmod(["runtime-kit", "--player", binary("Whatever"), "--host", join(repository,"runtime/dist/whatever-mod-host"+extension), "--launcher", binary("whatever-launcher"), "--source",repository,"--output",kit]);
    copyFileSync(binary("wmod"),join(kit,"wmod"+extension));
    const installedWmod=join(kit,"wmod"+extension);
    const output = join(directory,"game");
    mkdirSync(output);
    // An installed editor/CLI locates its own support payload; no runtime directory is supplied.
    await wmod(["package", project, "--output", output],true,installedWmod);
    expect(existsSync(join(output,"Branded Fixture"+extension))).toBe(true);
    expect(existsSync(join(output,"engine/Whatever"+extension))).toBe(true);
    expect(existsSync(join(output,"engine/runtime"+extension))).toBe(true);
    expect(existsSync(join(output,"mods/packaged_fixture/node_modules"))).toBe(false);
    expect(existsSync(join(output,"branding/icon.png"))).toBe(true);
    if(process.platform==="win32") {
      const executable=join(output,"Branded Fixture.exe").replaceAll("'","''");
      const info=Bun.spawn(["powershell","-NoProfile","-Command",`[System.Diagnostics.FileVersionInfo]::GetVersionInfo('${executable}').FileDescription`],{stdout:"pipe",stderr:"pipe"});
      expect((await new Response(info.stdout).text()).trim()).toBe("Branded Fixture");
      expect(await info.exited).toBe(0);
    }
    const unrelated=join(directory,"other-files");mkdirSync(unrelated);writeFileSync(join(unrelated,"keep.txt"),"keep");
    expect((await wmod(["package",project,"--output",unrelated],false,installedWmod)).code).not.toBe(0);
    expect(readFileSync(join(unrelated,"keep.txt"),"utf8")).toBe("keep");
    // Rebuilding a managed output replaces the previous build safely.
    await wmod(["package",project,"--output",output],true,installedWmod);
    await wmod(["project","check",output]);
    const editor = Bun.spawn([binary("whatever-editor"), output, "--check"], {stdout:"pipe",stderr:"pipe"});
    const editorError = await new Response(editor.stderr).text();
    if (await editor.exited !== 0) throw Error(editorError);
    writeFileSync(join(mod,"scripts/index.ts"),'throw Error("source must not run after packaging")');
    const launcher = join(output,"Branded Fixture"+extension);
    player = Bun.spawn([launcher,"--mode","server"], {cwd:directory,env:{...process.env,PATH:"",RUST_LOG:"whatever_engine=info"},stdin:"pipe",stdout:"pipe",stderr:"pipe",timeout:15000});
    const active = player;
    let stdout = ""; let stopped = false;
    const read = (async () => {
      for await (const bytes of active.stdout as ReadableStream<Uint8Array>) {
        stdout += new TextDecoder().decode(bytes);
        if (!stopped && stdout.includes("PACKAGED_SCENE_READY")) { stopped = true; (active.stdin as import("bun").FileSink).write("quit\n"); }
      }
    })();
    const [stderr,code] = await Promise.all([new Response(active.stderr as ReadableStream).text(),active.exited,read]);
    if (code !== 0) throw Error(stdout+stderr);
    expect(stdout).toContain("PACKAGED_SCENE_READY");
    expect(stdout).not.toContain("source must not run");
    const invalid = Bun.spawn([launcher,"--mode","bad"],{cwd:directory,stdout:"pipe",stderr:"pipe"});
    await Promise.all([new Response(invalid.stdout).text(),new Response(invalid.stderr).text()]);
    expect(await invalid.exited).not.toBe(0);
  } finally {
    if (player?.exitCode === null) { player.kill(); await player.exited; }
    rmSync(directory,{recursive:true,force:true});
  }
}, 60000);
