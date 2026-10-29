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
| [Bun](https://bun.sh) | Development/build tooling; shipped games use the bundled mod host |

## Build & Run

```sh
# Prepare and build the engine-owned runtime (native Bun build prerequisites required)
bun install --frozen-lockfile
bun run prepare:bun
bun run build:bun
bun run build:runtime
bun run build:host

# Build the engine
cargo build

# Run the client (or use core/meta.toml default_mode)
cargo run

# Run a headless server
cargo run -- --mode server

# Run with debug logging
cargo run -- --debug=all
cargo run -- --debug=ipc,modloader

# Load mods from a custom directory instead of mods/ and mods_user/
cargo run -- --mods /path/to/my/mods

# Run tests
cargo test
```

## Project Layout

```
.gitattributes          Keep native fork patches in LF format
mods/                   Engine-shipped mods (tracked in git)
  core/                 Built-in core mod (shaders, base assets, dev commands)
    src/                TypeScript source for the core script
    scripts/            Compiled JS entry point (built from src/ via bun build:core)
  script_test/          Scripting API integration test
  shader_test/          Custom shader and mesh renderer verification mod
mods_user/              User-installed mods (gitignored)
vendor/
  bun/                  Pinned Bun source submodule
  patches/              Reproducible native runtime fork patch
tools/                  Fork preparation, host builds and end-to-end security tests
runtime/                @whatever-engine/api — TypeScript scripting API package
  host/                 Trusted bootstrap compiled with the API into whatever-mod-host
  src/                  Source
  dist/
    index.js            Compiled API bundle (built from src/ via bun build:runtime)
    index.d.ts          Type declarations for the API
src/
  main.rs               CLI args, Engine::new(), engine.run()
  engine.rs             Client window, rendering, input and presentation snapshots
  runtime/              Mode selection, shared simulation worker, tick clock and server runner
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
  script/               Bundled runtime discovery + subprocess host + NDJSON IPC
  sandbox/              Native-host OS restrictions and broker path containment
  audio/                Audio playback and handle management
  watchdog.rs           Slow-tick diagnostics
  renderer/             wgpu context, camera, sprites
  vfs/                  Layered virtual filesystem
  mods/                 Mod manifest, discovery, toposort, registry
  ecs/                  Entity-component system
docs/
  SCRIPTING.md          Full scripting API reference
  SHADER.md             Custom shader contract, mesh formats, and patterns
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

### Client and server modes

Set `default_mode = "client"` or `"server"` in the `[game]` section of core's `meta.toml`. The CLI `--mode client|server` overrides it; old metadata defaults to client mode.

The simulation owns ECS state and mod IPC on a worker thread with its own tick deadlines. Rendering runs on the window thread and consumes the latest world snapshot. A slow tick does not block rendering; FPS caps, vsync and missing redraws do not schedule ticks. Missed deadlines after a slow tick are skipped to avoid a catch-up spiral. Published snapshots currently copy the world; pending GPU updates coalesce by entity and renderer type while frames are paused.

Server mode creates no window, graphics context, egui context or audio output device. It uses the same simulation worker with empty input. Stop it with Ctrl+C, SIGTERM on Unix, or `quit` / `stop` on stdin. The executable still links desktop libraries; a smaller server-only build is separate future work.

Mods may set `sides = ["client"]`, `["server"]` or `["client", "server"]` inside `[mod]`. Omission supports both sides. Inactive mods are filtered before dependency checks and load ordering; active dependencies must exist on the selected side.

A script may provide a shared fallback and side overrides:

```toml
[script]
entry = "scripts/shared.ts" # optional fallback
client = "scripts/client.ts" # optional override
server = "scripts/server.ts" # optional override
```

Only one entry executes per mod. A mod with no matching entry still contributes its assets and metadata. `Engine.mode` is available before top-level mod evaluation. Window, audio and presentation settings reject server use; ECS, files and inter-mod messages remain available. The shipped core script and visual demo mods run on the client.

Run `bun run test:server` after building the engine and host to check headless startup and shutdown. The additional `cargo test client_ticks_without_redraws -- --ignored` integration case requires the bundled host and verifies client ticks with a 1 FPS cap and no renderer consuming snapshots.

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

Scripted mods run in a bundled `whatever-mod-host` process per mod. The engine communicates over NDJSON on stdin/stdout. Shipped games do not need Bun in PATH or a runtime npm installation.

The host contains `@whatever-engine/api` and resolves that exact package for every mod, including mods outside the engine directory. Leave it external when bundling a mod; the npm package is optional for editor types. Other package imports are denied by default. See [`docs/SANDBOX.md`](docs/SANDBOX.md) for the initial module and global allowlists.
It contains all the necessary abstractions for the IPC messaging so that devs can work with properly typed structures without extra boilerplate.

See [`docs/SCRIPTING.md`](docs/SCRIPTING.md) for the full reference.

### Regenerating type declarations

After editing `runtime/src/`, regenerate `index.d.ts`:

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

Stdout mirrors the log with ANSI colours (WARN yellow, ERROR red). The in-game developer console (F2) also shows INFO and above in real time.

Suggestions are sorted alphabetically; Up/Down cycles through the full list and previews the selected completion as dim text in the input. Tab or a click accepts it, while Enter runs only the text already entered. Escape dismisses suggestions first, then closes the console. The input stays anchored when suggestions appear. Help lists command usage and descriptions on separate, wrapping lines.

By default only events from this crate are captured. Set `RUST_LOG` to override:

```sh
RUST_LOG=debug cargo run                        # all levels from this crate
RUST_LOG=wgpu=warn,Whatever=debug cargo run     # also include wgpu warnings
```

## Debug flags

Pass `--debug=<flags>` to enable verbose category logging (comma-separated). 
Lines are written to `logs/latest.log` alongside normal output and appear in the dev console (F2).

| Flag | What it logs |
|---|---|
| `all` | All categories below |
| `ipc` | Every raw NDJSON message between engine and scripts |
| `modloader` | Mod discovery, dependency resolution, load order |
| `window` | Window lifecycle events |
| `vfs` | VFS reads, overrides, and cache misses |
