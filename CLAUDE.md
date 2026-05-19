# Whatever Engine

## Philosophy

The engine is a barebones backend. Every game is just a mod. The engine owns window (winit), GPU renderer (wgpu), virtual file system, mod loader, and TypeScript/Bun scripting host — nothing else. No game logic lives in engine code. This allows both players and developers to truly "do whatever" since even finished games using this engine have no special behavior, they just mods using the same rule book as other mods.

## Architecture

| Module | Purpose |
|---|---|
| `src/main.rs` | CLI args, Engine::new(), engine.run() |
| `src/engine.rs` | Engine struct; owns all subsystems; drives main loop |
| `src/debug.rs` | DebugConfig (CLI parsing) + DebugLogger (file writers) |
| `src/input.rs` | InputState: key + mouse delta accumulator |
| `src/vfs/` | VFS trait, layered VFS, disk-backed layer |
| `src/mods/` | Mod manifest, discovery, toposort, registry |
| `src/renderer/` | wgpu context, camera, texture loading, scene |
| `src/script/` | Bun subprocess host, NDJSON IPC, message dispatch |
| `runtime/engine_api.ts` | TypeScript IPC shim imported by mod scripts |

Every `mod.rs` is a thin re-export file only. Logic lives in named sibling files.

## VFS Path Convention

`mod_id://relative/path/to/asset`

- `core://shaders/sprite.wgsl` — engine shader
- `my_game://textures/player.png` — mod asset

## Mod System

Mods live in `mods/` (engine-shipped) and `mods_user/` (user-installed, git-ignored). Each mod directory must contain `mod.toml`. Mods are loaded in toposorted dependency order. The `core` mod always loads first.

## IPC Protocol

One Bun process per scripted mod. Transport: NDJSON on stdin/stdout. Stderr → engine logger with mod_id prefix.

Engine → Script: `Init`, `Frame`, `Input`, `AssetResponse`, `Shutdown`
Script → Engine: `Subscribe`, `AssetRequest`, `SpawnSprite`, `MoveEntity`, `DestroyEntity`, `Log`, `SetWindowTitle`

## Commands

```sh
cargo build
cargo run
cargo run -- --debug=all
cargo run -- --debug=ipc,modloader
cargo test
```

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