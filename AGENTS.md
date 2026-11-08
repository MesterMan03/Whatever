# Whatever Engine

## Philosophy

The engine is a barebones backend. Every game is just a mod. The engine owns window (winit), GPU renderer (wgpu), virtual file system, mod loader, and embedded TypeScript/JavaScript Wasm runtime — nothing else. No game logic lives in engine code. This allows both players and developers to truly "do whatever" since even finished games using this engine have no special behavior, they just mods using the same rule book as other mods.

## Architecture

| Module | Purpose |
|---|---|
| `player/src/main.rs` | CLI args, Engine::new(), engine.run() |
| `engine/src/engine.rs` | Client presentation: window, graphics, input and world snapshots |
| `engine/src/runtime/` | Mode selection, independent simulation worker, tick clock and headless server |
| `engine/src/debug.rs` | DebugConfig (CLI parsing) + DebugLogger (file writers) |
| `engine/src/input.rs` | InputState: key + mouse delta accumulator |
| `engine/src/vfs/` | VFS trait, layered VFS, disk-backed layer |
| `engine/src/mods/` | Mod manifest, discovery, toposort, registry |
| `engine/src/renderer/` | wgpu context, camera, texture loading, scene |
| `engine/src/network/` | Native schemas/codecs, QUIC transport, TLS trust and per-client data |
| `engine/src/script/` | Source compiler and native dispatch |
| `script-vm/` | Shared Wasmtime/QuickJS library and embedded VM/compiler payload |
| `render-deps/` | Shared graphics dependencies; bounds Windows DLL exports |
| `engine/src/scene/` | Versioned scene files and atomic hierarchy instantiation |
| `engine/src/project/` | Shared project, scaffolding and deployment tooling |
| `editor/` | Native editor crate; shares engine and tooling services |
| `launcher/` | Small branded launcher, independent of engine code |
| `wmod/` | CLI development companion using shared project tooling |
| `runtime/` | `@whatever-engine/api` npm package — TypeScript scripting API for mods |

Every `mod.rs` is a thin re-export file only. Logic lives in named sibling files.

## VFS Path Convention

`mod_id://relative/path/to/asset`

- `core://shaders/sprite.wgsl` — engine shader
- `my_game://textures/player.png` — mod asset

## Mod System

Each game owns its `mods/` and optional `mods_user/` (git-ignored). The source checkout defaults to `games/test`; new projects own their game scripts, scenes and assets in `mods/core`. The editor ships a minimal core template from `engine/resources/core` and an on-demand asset library from `engine/resources/starter`. Each mod directory must contain `mod.toml`. Mods are loaded in toposorted dependency order with core preferred first. Scene references to another mod's assets or custom components require a direct dependency in `mod.toml`.

## Embedded Scripting

Each mod runs inside an isolated engine-owned QuickJS Wasm VM hosted by Wasmtime.
The native player/editor/CLI shares the VM, SDK, TypeScript compiler and declarations
through `whatever_script_vm`; all native libraries must match their executables.
Mods distribute raw JS/TS/JSON sources, never Wasm. Source discovery, static metadata
inspection and transpilation work on the destination machine without external tools.
Imports are limited to the embedded API and contained local sources. The VM exposes
only the engine's capabilities and has no WASI, OS globals or mod-controlled grants.
See docs/EMBEDDED_SCRIPTING.md and docs/SANDBOX.md.

Native calls execute synchronously on the simulation worker, in dependency/load
order across mods and call order within each mod. Live component setters/nested
edits affect later queries immediately. Physics integration follows scripts and
editor commands. Retained scene snapshots and Pending.get() are compatibility
helpers; no IPC barrier or script-side scene replica remains. Component references
resolve mod-local class instances; isolated mods use messaging for communication.

## Commands

```sh
bun install --frozen-lockfile
rustup target add wasm32-unknown-unknown
bun run build:runtime
bun run build:guest
bun run test:embedded
bun run test:guest
bun run test:network
bun run test:runtime
bun run build:native
cargo run
cargo run -- --mode server
cargo run -- --debug=all
cargo run -- --debug=ipc,modloader
bun run test:native
bun run format
bun run format:check

# Rebuild @whatever-engine/api bundle + declarations after editing runtime/src/
bun run build:runtime
bun run build:guest
```

Native builds use dynamic Rust libraries; `build:native` configures loader paths,
stages Rust std and rebuilds the root game launcher without private dependencies.
Linux/macOS native builds embed source-build library search paths. Editor distribution
CI builds and tests release only; `WHATEVER_TEST_PROFILE=release` selects those
binaries for the Bun integration tests. The game launcher overrides the workspace's
dynamic-linking flags through its Cargo environment and statically links Rust std.
Development uses stock Bun for package installation, builds, tests and documentation.
Bun is not part of the shipped mod runtime; mods execute in the embedded QuickJS Wasm VM.
Windows/Linux distributions keep real executables and private libraries once in
`bin/`, with data at the root. macOS uses `.app/Contents/MacOS`. Game exports keep
their branded launcher at the root and player/libraries in `bin/`. Packaging records
the player's dependency closure in `native-libraries.json`. Editor installers are
MSI (Windows system PATH), DEB (Linux `/usr/bin` links) and PKG (macOS paths.d).

The shipped core template is metadata only; project scaffolding adds its game entry script and initial scene. Object templates are data-driven `.wprefab` documents; `.wasset` ZIP libraries import editable content into a mod's `libraries/` mount. Never add engine-native Cube/Sphere/game object definitions. See `docs/ASSETS.md`. Put engine diagnostic/demo tools in `games/test/mods/debug`; keep protocols/examples in `games/test/mods/network_example`. Headless stdin uses the same command registry as the client console. Project autocomplete references the installed SDK through an ignored `.whatever/tsconfig.sdk.json`; never copy API declarations into new projects.

## Versioning

We skipped releasing `0.5.0` before `0.6.0`, and the joke is now policy: Whatever
release versions must never contain the digit `5`, including prerelease and build
identifiers. Always skip from `4` to `6`: `0.4.0` → `0.6.0`, `0.14.0` → `0.16.0`,
`1.4.0` → `1.6.0`, and `0.6.4` → `0.6.6`.

This applies to the shared engine, editor, launcher, CLI and scripting API release
version and matching release tags. Game, mod, dependency and protocol/schema
versions remain independent. `bun run check:versions` enforces the release rule.

## Adding a New Mod

1. Create `games/test/mods_user/<mod_id>/mod.toml` (copy from `engine/resources/core/mod.toml` and edit)
2. Add assets to `games/test/mods_user/<mod_id>/assets/`
3. Optionally add `scripts/index.ts` and set `[script]` in `mod.toml`
4. Add the mod to `games/test/whatever.wproject` through the editor, then `cargo run`.

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
