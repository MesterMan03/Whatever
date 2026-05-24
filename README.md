![GitHub License](https://img.shields.io/github/license/MesterMan03/Whatever)
[![CI](https://github.com/MesterMan03/Whatever/actions/workflows/ci.yml/badge.svg)](https://github.com/MesterMan03/Whatever/actions/workflows/ci.yml)
![GitHub Release](https://img.shields.io/github/v/release/MesterMan03/Whatever?include_prereleases)
[![NPM Version](https://img.shields.io/npm/v/%40whatever-engine%2Fapi)](https://www.npmjs.com/package/@whatever-engine/api)
![Rust](https://img.shields.io/badge/made%20with-Rust-f74c00)
![TypeScript](https://img.shields.io/badge/made%20with-TypeScript-3178c6)
![GitHub code size in bytes](https://img.shields.io/github/languages/code-size/MesterMan03/Whatever)


# Whatever Engine

A barebones game engine where every game is just a mod. The engine owns the window, GPU renderer, virtual file system, mod loader, and TypeScript/Bun scripting host, nothing else. No game logic lives in engine code.

This enables the core philosophy of "you can do whatever": both developers and players are allowed to freely dissect and modify every part of the engine.

## Prerequisites

| Tool | Purpose |
|---|---|
| [Rust + Cargo](https://rustup.rs) | Building the engine |
| [Bun](https://bun.sh) | Running mod scripts |

## Build & Run

```sh
# Build
cargo build

# Run
cargo run

# Run with debug logging
cargo run -- --debug=all
cargo run -- --debug=ipc,modloader

# Run tests
cargo test
```

## Project Layout

```
mods/                   Engine-shipped mods (tracked in git)
  core/                 Built-in core mod (shaders, base assets, dev commands)
    src/                TypeScript source for the core script
    scripts/            Compiled JS entry point (built from src/ via bun build:core)
  script_test/          Scripting API integration test
mods_user/              User-installed mods (gitignored)
runtime/                @whatever-engine/api — TypeScript scripting API package
  src/                  Source
  dist/
    index.js            Compiled API bundle (built from src/ via bun build:runtime)
    index.d.ts          Type declarations for the API
src/
  main.rs               CLI args, Engine::new(), engine.run()
  engine.rs             Engine struct; owns all subsystems; drives main loop
  debug.rs              DebugConfig (CLI parsing) + DebugLogger (debug-category writes)
  logging.rs            Log file init, rotation, tracing subscriber setup
  input.rs              Keyboard/mouse accumulator
  console/              Developer console (egui UI, command registry, tab completion)
    commands/           Built-in engine commands (version, markbench, mods, vfs)
    completer.rs        Tab-completion logic
    parser.rs           Command tokenizer and argument parser
    registry.rs         CommandRegistry + script-to-node conversion
    types.rs            Shared types (CommandNode, ArgSpec, …)
    widget.rs           DevConsole render + execute
  script/               Bun subprocess host + NDJSON IPC
  renderer/             wgpu context, camera, sprites
  vfs/                  Layered virtual filesystem
  mods/                 Mod manifest, discovery, toposort, registry
  ecs/                  Entity-component system
docs/
  scripting-api.md      Full scripting API reference
  SANDBOX.md            Security sandbox design and implementation details
```

## Mod System

Mods live in `mods/` (engine-shipped) or `mods_user/` (user-installed, gitignored). Each mod is a directory containing `mod.toml`.

### mod.toml

```toml
[mod]
id          = "my_game"
name        = "My Game"
version     = "0.1.0"
description = "A game built on the Whatever engine"

[script]
entry = "scripts/index.ts"     # optional — omit if the mod has no scripts

[assets]
root = "assets"                 # default
```

Mods are loaded in topological dependency order. `core` always loads first.

### VFS paths

Assets are accessed via `mod_id://relative/path`:

```
core://shaders/sprite.wgsl
my_game://textures/player.png
```

### Adding a mod

1. Create `mods_user/<mod_id>/mod.toml`
2. Add assets to `mods_user/<mod_id>/assets/`
3. Optionally add `scripts/index.ts` and set `[script]` in `mod.toml`
4. `cargo run` — the mod is discovered and loaded automatically

## Scripting

Scripted mods run as a Bun subprocess per mod. The engine communicates over NDJSON on stdin/stdout.

The `@whatever-engine/api` package is provided by the engine's workspace — no install step needed for mods running within the engine directory. 
It contains all the necessary abstractions for the IPC messaging so that devs can work with properly typed structures without extra boilerplate.

See [`docs/scripting-api.md`](docs/scripting-api.md) for the full reference.

### Regenerating type declarations

After editing `runtime/index.ts`, regenerate `index.d.ts`:

```sh
bun run build:runtime
```

## Logging

Every run writes to `logs/latest.log`. On startup the previous file is gzip-archived as `logs/yyyy-mm-dd_hh-mm-ss.log.gz` (timestamp from its creation date). Both paths are gitignored.

Log line format:

```
[HH:MM:SS.mmm] [LEVEL]    message     — engine events  (INFO / WARN / ERROR)
[HH:MM:SS.mmm] [category] message     — debug category (ipc / modloader / window / vfs)
```

Stdout mirrors the log with ANSI colours (WARN yellow, ERROR red). The in-game developer console (Ctrl+Alt+Enter) also shows INFO and above in real time.

By default only events from this crate are captured. Set `RUST_LOG` to override:

```sh
RUST_LOG=debug cargo run                        # all levels from this crate
RUST_LOG=wgpu=warn,Whatever=debug cargo run     # also include wgpu warnings
```

## Debug flags

Pass `--debug=<flags>` to enable verbose category logging (comma-separated). 
Lines are written to `logs/latest.log` alongside normal output and appear in the dev console (Ctrl + Alt + Enter).

| Flag | What it logs |
|---|---|
| `all` | All categories below |
| `ipc` | Every raw NDJSON message between engine and scripts |
| `modloader` | Mod discovery, dependency resolution, load order |
| `window` | Window lifecycle events |
| `vfs` | VFS reads, overrides, and cache misses |
