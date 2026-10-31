import { test, expect } from "bun:test";
import { mkdtempSync, mkdirSync, writeFileSync, cpSync, rmSync } from "node:fs";
import { join, resolve } from "node:path";
import { tmpdir } from "node:os";
const repository = resolve(import.meta.dir, "../..");
const binary = process.env.WHATEVER_TEST_PLAYER ?? join(repository, "target/debug/Whatever" + (process.platform === "win32" ? ".exe" : ""));
test("Markbench runs every headless workload, produces native timings and a complete score", async () => {
  const directory = mkdtempSync(join(tmpdir(), "whatever-markbench-"));
  mkdirSync(join(directory, "mods/core"), { recursive: true });
  writeFileSync(join(directory, "mods/core/mod.toml"), '[mod]\nid="core"\nname="Core"\nversion="1.0.0"\n');
  cpSync(join(repository, "games/test/mods/markbench"), join(directory, "mods/markbench"), { recursive: true });
  const preset = process.env.WHATEVER_MARKBENCH_PRESET ?? "quick";
  const child = Bun.spawn([binary, "--mode", "server", "--mods", join(directory, "mods")], { cwd: directory, env: { ...process.env, RUST_LOG: "whatever_engine=info", PATH: "" }, stdin: "pipe", stdout: "pipe", stderr: "pipe", timeout: 180_000 });
  let stdout = "", started = false, stopped = false;
  const read = (async () => {
    for await (const bytes of child.stdout) {
      stdout += new TextDecoder().decode(bytes);
      if (!started && stdout.includes("Markbench ready:")) { started = true; child.stdin.write(`markbench run ${preset}\n`); }
      if (!stopped && (stdout.includes("MARKBENCH_RESULT") || stdout.includes("Markbench stopped:"))) { stopped = true; child.stdin.write("quit\n"); }
    }
  })();
  try {
    const [code, stderr] = await Promise.all([child.exited, new Response(child.stderr).text(), read]);
    if (code !== 0 || !stopped || !stdout.includes("MARKBENCH_RESULT")) throw new Error(`Markbench failed (${code})\n${stdout}\n${stderr}`);
    const line = stdout.split(/\r?\n/).find(line => line.includes("MARKBENCH_RESULT"))!;
    const report = JSON.parse(line.slice(line.indexOf("MARKBENCH_RESULT ") + "MARKBENCH_RESULT ".length));
    expect(report.complete).toBe(true);
    expect(report.results).toHaveLength(8);
    expect(report.results.every((stage: any) => stage.valid && stage.samples === (preset === "quick" ? 12 : 240))).toBe(true);
    expect(report.score).toBeGreaterThan(0);
    expect(report.presentation_score).toBeNull();
    expect(report.codec_round_trips).toBeGreaterThan(0);
    expect(report.environment.hosts).toBe(1);
    expect(report.environment.frame).toBeNull();
    expect(report.results.find((stage: any) => stage.id === "transform_writes").native.snapshot_bytes.mean).toBeGreaterThan(report.results.find((stage: any) => stage.id === "snapshot_reads").native.snapshot_bytes.mean);
    expect(stdout).not.toContain("failed:");
    console.log(`Markbench ${preset}: score ${report.score.toFixed(1)}, ${report.results.length} stages, ${report.elapsed_ms.toFixed(0)} ms`);
  } finally {
    if (child.exitCode === null) { child.kill(); await child.exited; }
    rmSync(directory, { recursive: true, force: true });
  }
}, 180_000);
