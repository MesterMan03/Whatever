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
  index.ts              Source
  index.d.ts            Generated type declarations (for IDE autocomplete)
src/
  main.rs               CLI args, Engine::new(), engine.run()
  engine.rs             Engine struct; owns all subsystems; drives main loop
  debug.rs              Debug flags + file loggers
  input.rs              Keyboard/mouse accumulator
  console/              Developer console (egui UI, command registry, tab completion)
    commands/           Built-in engine commands (version, markbench, mods, vfs)
    completer.rs        Tab-completion logic
    console.rs          DevConsole render + execute
    parser.rs           Command tokenizer and argument parser
    registry.rs         CommandRegistry + script-to-node conversion
    types.rs            Shared types (CommandNode, ArgSpec, …)
  script/               Bun subprocess host + NDJSON IPC
  renderer/             wgpu context, camera, sprites
  vfs/                  Layered virtual filesystem
  mods/                 Mod manifest, discovery, toposort, registry
docs/
  scripting-api.md      Full scripting API reference
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

## Debug flags

| Flag | What it logs |
|---|---|
| `all` | Everything |
| `ipc` | Every raw NDJSON message between engine and scripts |
| `modloader` | Mod discovery, dependency resolution, load order |
| `window` | Window lifecycle events |
| `vfs` | VFS events (file access, asset overrides) |

Logs write to `debug/<flag>.log` and to the dev console which can be opened with Ctrl + Alt + Enter.
