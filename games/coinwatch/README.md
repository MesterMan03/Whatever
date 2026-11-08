# Coinwatch

Session fields reference `RunSession` components directly. The Inspector filters compatible
targets, and gameplay callbacks invoke the linked session behavior. Each scripted mod runs in
an isolated QuickJS Wasm VM hosted directly by the engine.

A small first-person survival game built entirely as an ordinary `core` mod. It exercises the official Whatever Starter `.wasset`: 31 distinct prefab designs decorate the arena, with 23 collectible spinning/bobbing coins, starter meshes for the sentinel and pulse launcher, textures, shaders, font, and audio. The complete imported library stays editable under `mods/core/libraries/whatever_starter`.

Open `whatever.wproject` in the **Coinwatch editor build**, or run the portable `dist/Coinwatch/Coinwatch.exe` from the repository root. This game needs the mouse-button API added alongside it; older editor/runtime builds cannot fire via left click.

- Click the game to capture the mouse; move the mouse to look.
- WASD or arrow keys walk; Shift sprints. Diagonal movement is normalized.
- Left click fires; holding it repeats. Pulses sweep for collisions and stop at the first solid collider or exactly 40 units, including when a frame is delayed.
- Walk into gold coins for 10 points. Coins return after 15 seconds.
- Shoot sentinels for 50 points. Any sentinel contact ends the run.
- The world-space score board sits above world origin. Death switches to an elevated camera with a clear view of it and freezes the final score. R restarts the arena and restores coins. After death, left click also starts a fresh run.
- Escape releases the mouse. F2 opens the developer console.

Orange sentinels roam along the authored navigation graph. A clear physics ray within their sight range and field of view starts a 20-second pursuit. They then roam for 10 seconds with acquisition disabled, even if you remain visible. Red means pursuing; blue means the roaming cooldown. Walls obstruct both vision and projectiles. Native dynamic capsules keep the player and enemies against physical obstacles; the TypeScript navigation system selects paths and sets horizontal velocities.

The scene composes small components with explicit entity references:

| Entity / asset | Components and editable settings |
| --- | --- |
| Run Session | `RunState` (ECS data), `RunSession` (score and lifecycle rules), `RestartInput`, `SoundEffects` (volume), `GameWindow` (size), optional `PlaytestTools` |
| Player | `FPSMovement` (walk/sprint speed, look reference), `PlayerLife` (spawn point, fall limit), native rigid body and collider |
| Mouse yaw pivot | `MouseLook` (sensitivity, pitch limit, camera reference); eye height is its authored Y position |
| Camera | Native camera (FOV), `ProjectileWeapon` (prefab, speed, range, damage, fire interval, live projectile cap) |
| Each coin | `CoinPickup` (collector/session references, radius, score, respawn time), imported `StarterSpin` and `StarterBob` |
| Enemy Spawner | `EnemySpawner` (world/target/graph/session references, prefab, interval, first delay, cap, minimum distance, separation) |
| Sentinel prefab | `EnemyNavigation` (roam/chase speeds, 20/10-second timers, sight range/FOV/query interval, waypoint tolerance, repath interval), `Health` (hit points, kill score), `ContactDamage`, `EnemyAppearance` |
| Navigation graph | 25 `NavNode` entities with editable links, enabled state and spawn permission |
| Game over camera | `GameOverView`, native camera and editable viewpoint transform |
| Score board | `Scoreboard` and the native text renderer |
| Each fired pulse | Its own `Projectile` component owns movement, sweep queries, damage and expiry |

Enemy behavior defaults belong to `mods/core/prefabs/sentinel.wprefab`; the spawner binds external references without replacing those settings. Spawned instances can be inspected and edited in Play mode. Transient run data lives in the referenced entity’s `RunState` component; narrow events deliver scoring, damage and lifecycle changes. The optional developer console reads ECS data and calls the same spawn factories; gameplay does not depend on it. The physics world exposes gravity, substeps and solver settings.

The graph deliberately follows the visible lanes around houses and cover. When editing the map, keep links clear of obstacles and connect both directions for bidirectional travel. This is authored graph navigation rather than a generated navmesh. Projectiles use point sweeps; their small visual sphere is cosmetic. The game is local single-player.

The two game-specific prefabs, `prefabs/sentinel.wprefab` and `prefabs/pulse.wprefab`, assemble ordinary imported starter assets. Placed scenery and coins are independent prefab copies. `starter-usage.json` lists the source prefab designs used by the arena. `tools/generate-coinwatch.py` at the repository root reproducibly authors the scene and game prefabs; rerunning it replaces those files, so preserve manual scene edits first.

For repeatable playtesting, the F2 console provides:

```text
coinwatch status
coinwatch restart
coinwatch spawning false
coinwatch teleport 0 6
coinwatch enemy 0 0
coinwatch shoot 0 1 0
```

`status` reports score, live enemies and their navigation modes, projectile count and last collision/range distance. The developer commands use the same gameplay code as normal input. `bun run test:coinwatch` runs navigation/timer/range checks and an actual native headless playthrough covering pickup, enemy pursuit/cooldown, kills, walls, range, contact death, score freeze, restart and random spawning. Its native runtime path can be overridden with `WHATEVER_TEST_PLAYER`.

Original game scripts are Apache-2.0, like the engine. Imported assets retain the starter library's Apache-2.0 license and the font's OFL; see the imported `LICENSE` and `fonts/OFL.txt`.
