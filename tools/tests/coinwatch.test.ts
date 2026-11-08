import { setTimeout as sleep } from "node:timers/promises";
import { text } from "node:stream/consumers";
import { spawnCaptured } from "../process.ts";
import { expect, test } from "vitest";
import { readFileSync, writeFileSync, cpSync, mkdirSync, rmSync } from "node:fs";
import { join, resolve } from "node:path";
import { advanceHunt, route, sweepLength } from "../../games/coinwatch/mods/core/scripts/navigation.ts";
const root = resolve(import.meta.dirname, "../..");
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

test("Coinwatch runs its actual components, prefabs and physics through the embedded VM", async () => {
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
  const child = spawnCaptured([binary, "--mode", "server", "--project", directory], { env: { ...process.env, RUST_LOG: "whatever_engine=info" } });
  let output = "";
  const reading = (async () => { for await (const bytes of child.stdout) output += new TextDecoder().decode(bytes).replace(/\x1b\[[0-9;]*m/g, ""); })();
  const errors = text(child.stderr);
  async function waitFor(predicate: () => boolean, timeout = 15000): Promise<void> {
    const start = Date.now();
    while (!predicate()) {
      if (Date.now() - start > timeout || child.exitCode !== null) throw Error(output + "\nTimed out waiting for game");
      await sleep(20);
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
    const line=output.slice(cursor).split("\n").find(line=>line.includes('"score":'));
    if(!line)throw Error("Missing status response\n"+output.slice(cursor));
    return JSON.parse(line.slice(line.indexOf('{')));
  }
  async function advance(seconds: number): Promise<void> {
    const first = (await status()).tick;
    const deadline = Date.now() + 30000;
    while ((await status()).tick < first + Math.ceil(seconds * 60)) {
      if (Date.now() > deadline) throw Error("Simulation did not advance\n" + output);
      await sleep(30);
    }
  }
  try {
    await waitFor(() => output.includes("COINWATCH_READY"));
    expect((await status()).nodes).toBe(25);
    // Compare with real time so a tick-count-only test cannot hide slow motion.
    const startedAt = performance.now();
    await advance(2);
    expect(performance.now() - startedAt).toBeLessThan(4_000);
    await command("coinwatch teleport 0 6", "Teleported");
    await advance(0.2);
    let s = await status();
    expect(s.score).toBe(10); expect(s.coins).toBe(1);
    await advance(0.15);
    expect((await status()).score).toBe(10);
    await command("coinwatch teleport 0 9", "Teleported");
    await command("coinwatch enemy 0 0", "Spawned");
    await advance(0.35);
    expect((await status()).states[0].mode).toBe("chase");
    await advance(1.0);
    expect((await status()).states[0].mode).toBe("cooldown");
    await advance(0.75);
    expect((await status()).states[0].mode).toBe("chase");
    await command("coinwatch shoot 0 1 0", "Fired");
    await advance(0.65);
    s = await status();
    expect(s.kills).toBe(1); expect(s.score).toBe(60); expect(s.enemies).toBe(0);
    expect(s.lastShot.reason).toBe("collider");
    await command("coinwatch teleport 14 20", "Teleported");
    await command("coinwatch enemy 14 4", "Spawned");
    await command("coinwatch shoot 14 1 4", "Fired");
    await advance(0.7);
    s = await status();
    expect(s.enemies).toBe(1); expect(s.kills).toBe(1);
    expect(s.lastShot.reason).toBe("collider"); expect(s.lastShot.distance).toBeLessThan(10);
    await command("coinwatch shoot 14 1000 20", "Fired");
    await advance(1.7);
    s = await status();
    expect(s.projectiles).toBe(0); expect(s.lastShot.reason).toBe("range"); expect(s.lastShot.distance).toBeCloseTo(40);
    const enemyPosition = s.states[0].position;
    await command(`coinwatch teleport ${enemyPosition[0]} ${enemyPosition[2]}`, "Teleported");
    await waitFor(() => output.includes("COINWATCH_GAME_OVER"));
    s = await status(); expect(s.dead).toBe(true); const finalScore = s.score;
    await command("coinwatch teleport 0 3", "Teleported"); await advance(0.2);
    expect((await status()).score).toBe(finalScore);
    await command("coinwatch restart", "Restarted"); await advance(0.2);
    s = await status(); expect(s.dead).toBe(false); expect(s.score).toBe(0); expect(s.enemies).toBe(0);
    await command("coinwatch teleport 0 6", "Teleported"); await advance(0.15);
    expect((await status()).coins).toBe(1);
    await command("coinwatch spawning true", "Spawning true"); await advance(1.3);
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
}, 120000);
