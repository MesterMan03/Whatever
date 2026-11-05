import { expect, test } from "bun:test";
import { readFileSync, writeFileSync, cpSync, mkdirSync, rmSync } from "node:fs";
import { join, resolve } from "node:path";
import { advanceHunt, route, sweepLength } from "../../games/coinwatch/mods/core/scripts/navigation.ts";
const root = resolve(import.meta.dir, "../..");
test("navigation follows weighted authored links and refuses disconnected shortcuts", () => {
  const nodes = [
    { key: "a", position: [0, 0, 0] as const, links: ["b"] },
    { key: "b", position: [0, 0, 4] as const, links: ["c", "a"] },
    { key: "c", position: [4, 0, 4] as const, links: ["b"] },
    { key: "island", position: [1, 0, 1] as const, links: [] },
  ];
  expect(route(nodes, "a", "c").map(n => n.key)).toEqual(["a", "b", "c"]);
  expect(route(nodes, "a", "island")).toEqual([]);
});
test("continuous sight cannot bypass the 20 second hunt and 10 second roaming cooldown", () => {
  let state = advanceHunt({ mode: "roam", remaining: 0 }, 0, true, 20, 10);
  expect(state.mode).toBe("chase");
  state = advanceHunt(state, 19.9, false, 20, 10);
  expect(state.mode).toBe("chase");
  state = advanceHunt(state, .11, true, 20, 10);
  expect(state).toEqual({ mode: "cooldown", remaining: 10 });
  state = advanceHunt(state, 9.9, true, 20, 10);
  expect(state.mode).toBe("cooldown");
  expect(advanceHunt(state, .11, true, 20, 10)).toEqual({ mode: "chase", remaining: 20 });
});
test("a long or delayed projectile step never exceeds the remaining 40 unit range", () => {
  expect(sweepLength(30, 10, 39.8, 40)).toBeCloseTo(.2);
  expect(sweepLength(30, 1 / 60, 40, 40)).toBe(0);
});

test("Coinwatch runs its actual components, prefabs and physics through the sandboxed host", async () => {
  const directory = join(root, "target", "coinwatch-test-" + Date.now());
  cpSync(join(root, "games/coinwatch"), directory, { recursive: true });
  const scenePath = join(directory, "mods/core/scenes/main.wscene");
  const scene = JSON.parse(readFileSync(scenePath, "utf8"));
  Object.assign(scene.entities.find((e: any) => e.script_id === "spawner").components["core:EnemySpawner"], { enabled: false, firstDelay: .1, interval: .25, maxEnemies: 3 });
  scene.entities[0].components["core:SoundEffects"].enabled = false;
  const enemyPath = join(directory, "mods/core/prefabs/sentinel.wprefab");
  const prefab = JSON.parse(readFileSync(enemyPath, "utf8"));
  Object.assign(prefab.entity.components["core:EnemyNavigation"], { roamSpeed: .1, chaseSpeed: .1, sightFov: 360, chaseSeconds: 1, cooldownSeconds: .6, sightInterval: .05 });
  writeFileSync(enemyPath, JSON.stringify(prefab));
  writeFileSync(scenePath, JSON.stringify(scene));
  const binary = process.env.WHATEVER_TEST_PLAYER ?? join(root, "target/debug/Whatever" + (process.platform === "win32" ? ".exe" : ""));
  const child = Bun.spawn([binary, "--mode", "server", "--project", directory], { env: { ...process.env, RUST_LOG: "whatever_engine=info" }, stdin: "pipe", stdout: "pipe", stderr: "pipe" });
  let output = "";
  const reading = (async () => { for await (const bytes of child.stdout) output += new TextDecoder().decode(bytes).replace(/\x1b\[[0-9;]*m/g, ""); })();
  const errors = new Response(child.stderr).text();
  async function waitFor(predicate: () => boolean, timeout = 6000): Promise<void> {
    const start = Date.now();
    while (!predicate()) {
      if (Date.now() - start > timeout || child.exitCode !== null) throw Error(output + "\nTimed out waiting for game");
      await Bun.sleep(20);
    }
  }
  async function command(text: string, expected: string): Promise<void> {
    const cursor = output.length;
    child.stdin.write(text + "\n");
    await waitFor(() => output.slice(cursor).includes(expected));
  }
  async function status(): Promise<any> {
    const cursor = output.length;
    await command("coinwatch status", '"score":');
    return JSON.parse(output.slice(cursor).split("\n").find(line => line.includes('{"score":'))!.slice(output.slice(cursor).split("\n").find(line => line.includes('{"score":'))!.indexOf('{"score":')));
  }
  try {
    await waitFor(() => output.includes("COINWATCH_READY"));
    expect((await status()).nodes).toBe(25);
    await command("coinwatch teleport 0 6", "Teleported");
    await Bun.sleep(200);
    let s = await status();
    expect(s.score).toBe(10); expect(s.coins).toBe(1);
    await Bun.sleep(150);
    expect((await status()).score).toBe(10);
    await command("coinwatch teleport 0 9", "Teleported");
    await command("coinwatch enemy 0 0", "Spawned");
    await Bun.sleep(350);
    expect((await status()).states[0].mode).toBe("chase");
    await Bun.sleep(1000);
    expect((await status()).states[0].mode).toBe("cooldown");
    await Bun.sleep(750);
    expect((await status()).states[0].mode).toBe("chase");
    await command("coinwatch shoot 0 1 0", "Fired");
    await Bun.sleep(650);
    s = await status();
    expect(s.kills).toBe(1); expect(s.score).toBe(60); expect(s.enemies).toBe(0);
    expect(s.lastShot.reason).toBe("collider");
    await command("coinwatch teleport 14 20", "Teleported");
    await command("coinwatch enemy 14 4", "Spawned");
    await command("coinwatch shoot 14 1 4", "Fired");
    await Bun.sleep(700);
    s = await status();
    expect(s.enemies).toBe(1); expect(s.kills).toBe(1);
    expect(s.lastShot.reason).toBe("collider"); expect(s.lastShot.distance).toBeLessThan(10);
    await command("coinwatch shoot 14 1000 20", "Fired");
    await Bun.sleep(1700);
    s = await status();
    expect(s.projectiles).toBe(0); expect(s.lastShot.reason).toBe("range"); expect(s.lastShot.distance).toBeCloseTo(40);
    const enemyPosition = s.states[0].position;
    await command(`coinwatch teleport ${enemyPosition[0]} ${enemyPosition[2]}`, "Teleported");
    await waitFor(() => output.includes("COINWATCH_GAME_OVER"));
    s = await status(); expect(s.dead).toBe(true); const finalScore = s.score;
    await command("coinwatch teleport 0 3", "Teleported"); await Bun.sleep(200);
    expect((await status()).score).toBe(finalScore);
    await command("coinwatch restart", "Restarted"); await Bun.sleep(200);
    s = await status(); expect(s.dead).toBe(false); expect(s.score).toBe(0); expect(s.enemies).toBe(0);
    await command("coinwatch teleport 0 6", "Teleported"); await Bun.sleep(150);
    expect((await status()).coins).toBe(1);
    await command("coinwatch spawning true", "Spawning true"); await Bun.sleep(1300);
    s = await status(); expect(s.enemies).toBe(3);
    expect(s.states.every((e: any) => Math.hypot(e.position[0], e.position[2] - 6) > 13)).toBe(true);
    expect(output).not.toMatch(/ERROR|Tick handler failed|component .*failed/i);
    await command("quit", "simulation stopped");
    await child.exited;
    expect(await errors).toBe("");
  } finally {
    if (child.exitCode === null) child.kill();
    await reading;
    mkdirSync(join(root, "target/coinwatch-qa"), { recursive: true });
    writeFileSync(join(root, "target/coinwatch-qa/latest.log"), output);
    rmSync(directory, { recursive: true, force: true });
  }
}, 30000);
