# Whatever Engine

## Philosophy

The engine is a barebones backend. Every game is just a mod. The engine owns window (winit), GPU renderer (wgpu), virtual file system, mod loader, and TypeScript/Bun scripting host — nothing else. No game logic lives in engine code. This allows both players and developers to truly "do whatever" since even finished games using this engine have no special behavior, they just mods using the same rule book as other mods.

## Architecture

| Module | Purpose |
|---|---|
| `src/main.rs` | CLI args, Engine::new(), engine.run() |
| `src/engine.rs` | Client presentation: window, graphics, input and world snapshots |
| `src/runtime/` | Mode selection, independent simulation worker, tick clock and headless server |
| `src/debug.rs` | DebugConfig (CLI parsing) + DebugLogger (file writers) |
| `src/input.rs` | InputState: key + mouse delta accumulator |
| `src/vfs/` | VFS trait, layered VFS, disk-backed layer |
| `src/mods/` | Mod manifest, discovery, toposort, registry |
| `src/renderer/` | wgpu context, camera, texture loading, scene |
| `src/network/` | Native schemas/codecs, QUIC transport, TLS trust and per-client data |
| `src/script/` | Bun subprocess host, NDJSON IPC, message dispatch |
| `runtime/` | `@whatever-engine/api` npm package — TypeScript scripting API for mods |

Every `mod.rs` is a thin re-export file only. Logic lives in named sibling files.

## VFS Path Convention

`mod_id://relative/path/to/asset`

- `core://shaders/sprite.wgsl` — engine shader
- `my_game://textures/player.png` — mod asset

## Mod System

Mods live in `mods/` (engine-shipped) and `mods_user/` (user-installed, git-ignored). Each mod directory must contain `mod.toml`. Mods are loaded in toposorted dependency order. The `core` mod always loads first.

## IPC Protocol

One bundled `whatever-mod-host` process per scripted mod. The host embeds the engine API and runs the pinned Bun fork in `vendor/bun`; do not launch system Bun for mods. The submodule commit includes the native policy; `vendor/patches/bun-whatever.patch` exports its changes from the documented upstream base. `prepare:bun` verifies the committed fork and normalizes manifest line endings. Transport: NDJSON on stdin/stdout. Stderr → engine logger with mod_id prefix. `RuntimeReady` gates `Init`. Host protocol 5 passes the runtime side before mod evaluation; `Init` includes `mode`. Simulation ticking and IPC run on the runtime worker independently of window redraws.

The native module allowlist starts with only `@whatever-engine/api` and local JS/TS/JSON source inside the mod root. Node/Bun built-ins, npm packages and non-allowlisted globals are denied. Never add an unrestricted fallback or mod-controlled permission grants. See `docs/SANDBOX.md`.

Engine → Script: `Init`, `Tick`, `CommandResults`, notifications/responses, `Shutdown`.
Script → Engine: startup `RuntimeReady`/`RuntimeFailed`, then one `CommandBatch` per tick.
Application commands execute only from batches, in dependency/load order across mods and recorded order within each mod. Physics belongs after command execution and before snapshot/result publication. See `docs/SCRIPTING.md` for the complete protocol and pending-result API.

## Commands

```sh
bun run prepare:bun
bun run build:bun
bun run build:runtime
bun run build:host
bun run test:host
bun run test:network
bun run test:runtime
cargo build
cargo run
cargo run -- --mode server
cargo run -- --debug=all
cargo run -- --debug=ipc,modloader
cargo test
bun run format
bun run format:check

# Rebuild @whatever-engine/api bundle + declarations after editing runtime/src/
bun run build:runtime
```

The shipped core is metadata/assets only. Put engine diagnostic/demo tools in `mods/debug`; keep protocols/examples in `mods/network_example`. Headless stdin uses the same command registry as the client console.

## Adding a New Mod

1. Create `mods_user/<mod_id>/mod.toml` (copy from `mods/core/mod.toml` and edit)
2. Add assets to `mods_user/<mod_id>/assets/`
3. Optionally add `scripts/index.ts` and set `[script]` in `mod.toml`
4. `cargo run` — mod is discovered and loaded automatically

## Coding Conventions

- No `unwrap()` — use `?` or explicit error handling
- No `println!` — use `tracing::{info, warn, error}` or `DebugLogger`
- No game logic in engine code
- `mod.rs` files are re-exports only
- Add dependencies only when correct implementation would take more than a day or requires significant platform complexity

Mod JavaScript/TypeScript/JSON sources use the root Biome configuration: 2 spaces, 100 columns, LF. Run `bun run format` after editing mods or game mods, then `bun run format:check`. Keep assets and generated output out of formatting.

## Documentation

**When modifying any file under `runtime/src/`** (the `@whatever-engine/api` scripting API), always update
`docs/SCRIPTING.md` and `runtime/README.md` to reflect the changes (`docs/SCRIPTING.md` may contain detailed
type descriptions, while `runtime/README.md` should only have basic function headers).
The doc must stay in sync with the actual exported namespaces, types, and IPC messages.
After editing, run `bun run build:runtime` to rebuild `runtime/dist/`.

**When the project layout changes** (new module added, directory renamed or removed, new
top-level file), update the "Project Layout" block in `README.md` to match.
