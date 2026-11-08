![GitHub License](https://img.shields.io/github/license/MesterMan03/Whatever)
[![CI](https://github.com/MesterMan03/Whatever/actions/workflows/ci.yml/badge.svg)](https://github.com/MesterMan03/Whatever/actions/workflows/ci.yml)
![GitHub Release](https://img.shields.io/github/v/release/MesterMan03/Whatever?include_prereleases)
[![NPM Version](https://img.shields.io/npm/v/%40whatever-engine%2Fapi)](https://www.npmjs.com/package/@whatever-engine/api)
![Rust](https://img.shields.io/badge/made%20with-Rust-f74c00)
![TypeScript](https://img.shields.io/badge/made%20with-TypeScript-3178c6)
![GitHub code size in bytes](https://img.shields.io/github/languages/code-size/MesterMan03/Whatever)


# Whatever Engine

A barebones game engine where every game is just a mod. The engine owns the window, GPU renderer, virtual file system, mod loader, ECS, native services such as physics and networking, and embedded TypeScript/JavaScript Wasm runtime. Gameplay lives in mods.

This enables the core philosophy of "you can do whatever": both developers and players are allowed to freely dissect and modify every part of the engine.

## Prerequisites

| Tool | Purpose |
|---|---|
| [Rust + Cargo](https://rustup.rs) | Building the engine |
| [Bun](https://bun.sh/) 1.4.2+ | Development dependencies, builds, tests and documentation |

## Build & Run

```sh
# Build the engine-owned VM (Clang/LLVM with Wasm support required)
bun install --frozen-lockfile
rustup target add wasm32-unknown-unknown
bun run build:runtime
bun run build:guest

# Build native executables and stage their shared libraries
bun run build:native

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
bun run test:native
```

Building the launcher from source for Windows MSVC requires Open Watcom 2.0
configured for DOS development, with `wcl` and `wlink` on PATH. The launcher
build compiles the Snake stub automatically. [Open Watcom downloads](https://github.com/open-watcom/open-watcom-v2/releases)

The native build helper configures the Rust library search path and stages the
matching Rust standard library. Use `bun run build:native -- --release` for an
optimized build. Player, editor and CLI share engine, scripting and graphics
libraries; keep binaries and libraries from the same build together.

## Project Layout

```text
Cargo.toml              Workspace; plain cargo run/build selects the player
.cargo/config.toml      Dynamic linking for native targets; Wasm builds stay independent
bun.lock                Reproducible Bun development dependencies
AGENTS.md               Architecture and contributor instructions
.gitattributes          LF rules for SDK and mod sources
.editorconfig           Editor defaults for mod sources
biome.json              Shared mod-source formatter configuration
engine/                 Shared whatever-engine native library (own Cargo.toml)
  src/
    lib.rs              Shared engine services and tooling
    engine.rs           Client window, graphics, input and world snapshots
    runtime/            Simulation worker, tick clock, modes and headless runner
      editor.rs         Atomic runtime edits and hierarchy projection
      editor_transport.rs  Private authenticated editor/player connection
      performance.rs    Native CPU phase and frame timings
    scene/              Versioned scene documents and atomic instantiation
      prefab.rs         Named single-root templates and independent runtime spawning
      references.rs     Scene asset ownership and direct mod dependency validation
    project/            Mod collections, scaffolding and managed game builds
      build.rs          Installed support discovery and automatic source builds
      installation.rs   Distribution paths and private native-library manifests
      branding.rs       Game icons and Windows launcher resources
      paths.rs          Portable paths and native presentation
      files.rs          Contained file operations, reference updates and local trash
      sdk.rs            Local TypeScript bindings to the installed editor SDK
      starter.rs        On-demand imports from the editor starter asset library
      library.rs        Asset library metadata, atomic imports and namespace remapping
      library_archive.rs Portable ZIP pack/read validation and bounded extraction
      library_tests.rs  Archive, import, prefab and official-content integration checks
      workflow_tests.rs Project, asset, SDK and dependency regression tests
    renderer/           wgpu, cameras, meshes, sprites, text and shared world rendering
      render.rs         Renderer, pipelines and render passes
      world.rs          Shared ECS-to-render updates for player and editor
    schema/             Shared value schemas and native validation
    ecs/                Generational entities, typed components, hierarchy, references and tags
      registry.rs       Registered custom component schemas and defaults
      snapshot.rs       Mutation journals and revisioned scene deltas
      tags.rs           Bounded entity tags and queries
      lifecycle.rs      Inherited enabled state and deep entity cloning
    presentation/       Frame deadlines, measured FPS and render-only pose interpolation
      frame_clock.rs    Phase-preserving FPS cap and time-weighted frame counter
      motion.rs         Adjacent tick poses, hierarchy interpolation and camera cuts
    physics/            Native Rapier configuration, state and lifecycle
    network/            QUIC, schemas/codecs, TLS and per-client data
    script/             Source compiler, native capabilities and synchronous dispatch
    sandbox/            Native file/VFS broker containment
    mods/               Manifests, discovery, deterministic ordering and registry
    vfs/                Layered asset and folder mounts
      types.rs          VFS trait, paths, errors and handle type
    console/            Developer console, parser, completion and command registry
    audio/              Audio playback and handle management
    debug.rs            Debug switches and category logging
    logging.rs          Log files, rotation and tracing setup
    input.rs            Keyboard/mouse accumulator
    cursor.rs           Embedded player cursor confinement and visibility
    watchdog.rs         Slow-tick diagnostics
  resources/RUNTIME-LICENSES.md  Embedded scripting third-party notices
  resources/core/       Stock core template packaged with the editor
  resources/starter/    Official .wasset source: prefabs, meshes, surfaces, UI, audio and scripts
script-vm/              Shared Wasmtime/QuickJS library; one embedded VM payload
  src/runtime.rs        Wasmtime instances, execution budgets and synchronous turns
  src/tests.rs          JavaScript/Wasm isolation and live-call regression tests
render-deps/            Shared graphics dependencies; keeps Windows DLL exports bounded
player/                 Whatever executable; depends on shared engine crate
  src/main.rs           CLI, game loading, client/server entry point
editor/                 Separate native editor application (own Cargo.toml)
  src/app.rs            Window, renderer, input and background build jobs
  src/actions.rs        Project/scene commands and native file dialogs
  src/menus.rs          Native menus and keyboard shortcuts
  src/preferences.rs    Text editor preferences, recent projects and welcome screen
  src/mod_editor.rs     Mod metadata and script creation
  src/component_fields.rs  Schema-generated custom component inspectors
  src/component_reload.rs  Background metadata inspection and status-bar progress
  src/component_handles.rs  Collider and joint handles in the scene viewer
  src/ui.rs             Workspace panels, browser, viewport and project settings
  src/inspector.rs      Typed core components and custom component editing
  src/entity_picker.rs  Entity references and hierarchy drops
  src/presets.rs        Native component helpers; object templates remain data
  src/libraries.rs      Searchable prefab catalog and background library import/export
  src/browser.rs        Folder navigation and typed asset tiles
  src/asset_actions.rs  Create folder, move, rename and recoverable delete dialogs
  src/viewport.rs       Surface picking and transform handles
  src/viewport_geometry.rs Clipped overlays, rotated rings and camera frustums
  src/player.rs         Embedded player window hosting
  src/live.rs           Runtime hierarchy, live edits and temporary undo history
  src/hierarchy.rs      Tree, drag reordering and reparenting
  src/document.rs       Scene editing, undo/redo and safe saves
launcher/               Small branded game launcher; no engine dependencies
  build.rs              Builds and links the Snake stub for Windows MSVC
dosstub/                Standalone DOS Snake source and Watcom linker directives
wmod/                   CLI companion using the same shared project tooling
games/                  Ordinary game mod collections
  test/                 Development/test game; default player project in source builds
    whatever.wproject   Test project identity, mod selection and build settings
    mods/               Its own core, debug, markbench, network_example and physics_demo mods
    mods/markbench/     Deterministic workloads, scoring and reports
    mods_user/          User-local development mods (gitignored)
  graviton/             Separate game collection
  coinwatch/            Starter-library FPS survival game with script navigation
    whatever.wproject   Editable single-core game project
    mods/core/          Arena, game components and imported Whatever Starter assets
runtime/                @whatever-engine/api TypeScript package
  guest/                Engine-owned minimal QuickJS Wasm guest (independent Cargo build)
    polyfills.js        Explicit text, buffer, timer and microtask services
    src/lib.rs          Guest ABI, JavaScript evaluation and bounded promise-job pump
  compiler/             Embedded TypeScript transformation and static component metadata
    components.ts       Read-only class/field metadata extraction
    environment.ts      Contained virtual source filesystem
  embedded/             SDK entry for engine deliveries and class registration
  src/                  Public namespaces, direct calls, compatibility requests and references
    dispatch.ts         Engine delivery and component/tick scheduling
    live.ts             Checked nested component field handles
    input.ts            Current tick input, physical key codes and immutable keyboard snapshots
    time.ts             Read-only simulation tick number and interval
    schema.ts           Shared Type descriptors and value validation
    component.ts        Custom Component classes and lifecycle scheduling
    scene-store.ts      Native scene reads and explicit retained snapshots
    profiler.ts         Native tick/frame measurements and monotonic clock
  dist/                 Built API, declarations and embedded Wasm guest (gitignored)
tools/                  VM builds, generated assets and integration tests
  build-script-assets.ts Embedded SDK/compiler bundle generation
  build-script-guest.ts Reproducible QuickJS Wasm guest build and Windows LLVM support
  build-native.ts       Shared native build, loader setup and standalone game launcher
  native-libraries.ts   Native dependency closure, deduplication and relative loader paths
  check-versions.ts     Shared release version and lockfile audit
  package-editor.ts     Portable editor assembly, dependency bundling and relocation smoke
  package-installers.ts MSI, DEB and PKG assembly with editor command registration
  test-msi.ps1          Disposable-runner install, repair, upgrade and uninstall checks
  process.ts            Native child-process capture for packaging and tests
  tests/native.ts       Shared debug/release binary selection for integration tests
  generate-starter.py   Reproducible official library content generation
.github/workflows/
  editor.yml            Build/test portable archives and installers on Windows/Linux/macOS
  release.yml           Publish the same tested distributions on version tags
  docs.yml              Build/check modding docs and deploy main to GitHub Pages
docs/
  releases/             Curated GitHub release notes
  EDITOR.md             Projects, native editor, CLI and branded deployment layout
  ASSETS.md             .wasset libraries, independent prefabs and official starter content
  EMBEDDED_SCRIPTING.md Embedded VM/compiler, live calls and component references
  SCRIPTING.md          Public API, scene format and IPC reference
  PHYSICS.md            Native physics configuration and lifecycle
  SHADER.md             Shader contract, mesh formats and patterns
  SANDBOX.md            Wasm isolation, capabilities and broker containment
  IPC.md                Internal scripting ABI, source loading and execution order
website/                Astro Starlight modding documentation site
  src/content/docs/     Authored tutorials and guides; references generated from docs/
  scripts/              Reference generation and built-site link verification
  astro.config.mjs      Theme, navigation and configurable deployment base
```

## Modding documentation website

The [Astro Starlight](https://starlight.astro.build/) site includes a first-mod tutorial,
editor and component guides, the full scripting reference, and the native IPC
specification with a message catalogue generated from the Rust definitions.

```sh
bun run docs:install
bun run docs:dev
# Build static HTML, the search index, and verify internal links:
bun run docs:build
```

The existing `docs/*.md` references remain the source of truth. See
[website/README.md](website/README.md) for authoring and GitHub Pages setup.
The docs workflow checks pull requests and deploys `main` after Pages is enabled
with **GitHub Actions** as its source.

## Editor and game builds

Run `bun run build:native`, then `cargo run -p whatever-editor -- games/test` to open the current mod collection. A project is a `whatever.wproject` manifest selecting ordinary mod directories. Scenes are versioned JSON `.wscene` files in each mod's `scenes/` folder; scripts instantiate them with `await Scene.loadScene("my_mod://main")`.

`wmod` creates projects and mods, validates collections and packages games through the same shared code as the editor. Deployed games contain a branded launcher, a `bin/` directory with the player and shared libraries, and ordinary mod sources. See [the editor and deployment guide](docs/EDITOR.md) for commands and the folder contract.

Choose **Build Game** and an output folder in the editor, or run `wmod package <project> --output <folder>`. The tools supply the player with its embedded VM and the branded launcher. Project settings control the game name, icon, company and version. Installed editor releases include their support binaries. A source checkout builds them when needed.

## Mod formatting

Biome formats JavaScript, TypeScript and JSON source under `mods/`, `mods_user/` and `games/` with **2 spaces**, a **100-column** line width and LF endings. Assets, dependencies and `dist/` output are excluded. Gitignored local mods are included deliberately. The root `biome.json` is the shared configuration; `.editorconfig` provides matching editor indentation. Editors with Biome support can use this configuration for format-on-save.

```sh
bun run format        # Format mod sources
bun run format:check  # Check formatting without writing (also runs in CI)
bun run test:runtime  # Check pending-result API types and shipped debug scripts
```

Biome is a pinned development dependency. Run `bun install --frozen-lockfile` after pulling the setup. Formatting does not enable lint fixes or import organization.

## Mod System

Each game owns its `mods/` and optional `mods_user/` (gitignored). The source checkout uses `games/test`. Each mod is a directory containing `mod.toml`.

### mod.toml

```toml
[mod]
id          = "my_game"
name        = "My Game"
version     = "0.1.0"
description = "A game built on the Whatever engine"

[script]
entry = "scripts/index.ts"     # optional â€” omit if the mod has no scripts

[assets]
root = "assets"                 # default
```

Mods are loaded in topological dependency order. `core` always loads first.

### Client and server modes

Set `default_mode = "client"` or `"server"` in the `[game]` section of core's `meta.toml`. The CLI `--mode client|server` overrides it; old metadata defaults to client mode.

The simulation owns ECS and mod VMs on a worker with independent tick deadlines. It samples input/events, executes synchronous mod callbacks in dependency order, applies editor commands, advances native physics, then publishes presentation/editor state. Later calls and mods see preceding writes immediately. Async hooks do not overlap their own pending invocation. See [tick semantics](docs/SCRIPTING.md#tick-order-and-direct-calls).

Rendering runs on the window thread and consumes the latest world snapshot and adjacent pose samples. Camera, geometry and moving lights interpolate local transforms between ticks for smooth presentation (roughly one tick of visual delay), without changing simulation/query/editor state. Reparenting, entity replacement and camera cuts snap to the new state. FPS caps use fixed deadlines, skip missed slots and do not accumulate wake-up delay. A slow tick does not block rendering; FPS caps, vsync and missing redraws do not schedule ticks. The simulation catches up after short overruns with fixed-size steps and drops excess backlog after a long stall. Published snapshots currently copy the world; pending GPU updates coalesce by entity and renderer type while frames are paused.

The player's F1 overlay shows presented frames and completed simulation ticks per second, both measured against wall time. With VSync on, the player requests a one-frame presentation queue and waits for the next buffer on a worker. The window thread keeps processing input and uses the latest simulation snapshot once the buffer is ready.

Server mode creates no window, graphics context, egui context or audio output device. It uses the same simulation worker with empty input. Enter built-in or mod console commands on stdin (`help`, `help dev`, `netdemo codec`). Stop it with Ctrl+C, SIGTERM on Unix, or `quit` / `stop`. The executable still links desktop libraries; a smaller server-only build is separate future work.

Mods may set `sides = ["client"]`, `["server"]` or `["client", "server"]` inside `[mod]`. Omission supports both sides. Inactive mods are filtered before dependency checks and load ordering; active dependencies must exist on the selected side.

A script may provide a shared fallback and side overrides:

```toml
[script]
entry = "scripts/shared.ts" # optional fallback
client = "scripts/client.ts" # optional override
server = "scripts/server.ts" # optional override
```

Only one entry executes per mod. A mod with no matching entry still contributes its assets and metadata. `Engine.mode` is available before top-level mod evaluation. Window, audio and presentation settings reject server use; ECS, files and inter-mod messages remain available. The shipped core script and visual demo mods run on the client.

Run `bun run test:server` after building the embedded guest and engine to check headless startup and shutdown. The additional `cargo test --workspace client_ticks_without_redraws -- --ignored` integration case uses the embedded VM and verifies client ticks with a 1 FPS cap and no renderer consuming snapshots.

### Physics

Rapier 3D runs natively on the simulation worker after mod commands and before world
snapshots/results are published. Mods configure ordinary ECS world/body/collider/joint
components, with typed entity references and native tuning options. See the
[physics API](docs/PHYSICS.md).

To try the shipped client playground, open the **F2** console and enter
`physicsdemo start`. Use WASD to walk, Shift to sprint, the mouse to orbit, and R to
respawn. `physicsdemo cubes [count]` adds cubes; `physicsdemo stop` removes the scene
and restores the previous camera. The mod creates nothing until activated.

### VFS paths

Assets are accessed via `mod_id://relative/path`:

```
core://shaders/sprite.wgsl
my_game://textures/player.png
```

### Adding a mod

1. Create `games/test/mods_user/<mod_id>/mod.toml`
2. Add assets to `games/test/mods_user/<mod_id>/assets/`
3. Optionally add `scripts/index.ts` and set `[script]` in `mod.toml`
4. `cargo run` â€” the mod is discovered and loaded automatically

## Scripting

Scripted mods run in isolated QuickJS Wasm instances inside Wasmtime. The engine discovers raw JS/TS/JSON sources, extracts metadata and compiles them on the destination machine using its embedded TypeScript compiler. Installed games are self-contained and mods distribute source. Entity/component operations and physics queries execute immediately against native state. See [embedded scripting](docs/EMBEDDED_SCRIPTING.md).

The VM embeds `@whatever-engine/api`. Leave it external in mod bundles. Installing the package is optional for editor types. The VM resolves only this package and contained local sources. See [runtime policy](docs/SANDBOX.md).

See [`docs/SCRIPTING.md`](docs/SCRIPTING.md) for the full reference.

### Regenerating type declarations

After editing `runtime/src/`, regenerate `index.d.ts`:

```sh
bun run build:runtime
```

## Debug suite and network API example

New games own their main scripts, scenes and assets in `core`. The editor imports editable starter assets from its separate library when needed. The source checkout's test game keeps its own shaders and font. Diagnostic scripts and sample assets live in [`games/test/mods/debug`](games/test/mods/debug/README.md). Open F2 and enter `help dev`; nested help works too, such as `help dev render light`. Tests that delay ticks, log every tick or exercise the sandbox run only when requested. The client debug suite provides the demonstration camera; the server entry imports only headless tools. [Markbench](games/test/mods/markbench/README.md) replaces the Mandelbrot test with deterministic engine workloads, native timings and automatic scores. Run `markbench run quick` or `markbench run standard`.

[`games/test/mods/network_example`](games/test/mods/network_example/README.md) is a QUIC messenger with separate client/server entries and a shared typed specification. Start a headless server and run `netdemo host 4444`; in each client (F2), run `netdemo connect localhost:4444`. Clients receive increasing player IDs and can use `netdemo tell 2 "hello"`, `netdemo say "hello everyone"`, `netdemo players` and `netdemo ping` (QUIC datagram). The demo defaults to TOFU; add `strict` after the address to require a trusted certificate. See [Network API](docs/SCRIPTING.md#network) for transport, trust and client-data details.

TLS keys and saved trust live in the engine-owned local-data directory `WhateverNetwork/<game_id>`, outside mod file access. Servers generate a persistent development identity automatically, or operators can supply `--tls-cert <chain.pem> --tls-key <key.pem>`. Clients use the OS trust roots, optionally extended with `--tls-ca <authority.pem>`. `--network-data <directory>` selects an isolated identity/trust directory, useful for tests and separate server instances. Players provide only a server address.

The integration demo can run without windows or audio devices after building the embedded guest and engine: `cargo test --workspace --locked network_messenger -- --ignored`. Native QUIC/TLS tests also run under WSL with a separate Linux Cargo target directory.

`bun run test:network` runs schema/proxy tests and compiler checks against the generated public declarations. Native codec and registration tests run with `cargo test --workspace`. `bun run test:server` also exercises native network calls and stdin mod commands through embedded VMs.

## Logging

Every run writes to `logs/latest.log`. On startup the previous file is gzip-archived as `logs/yyyy-mm-dd_hh-mm-ss.log.gz` (timestamp from its creation date). Both paths are gitignored.

Log line format:

```
[HH:MM:SS.mmm] [LEVEL]    message     â€” engine events  (INFO / WARN / ERROR)
[HH:MM:SS.mmm] [category] message     â€” debug category (ipc / modloader / window / vfs)
```

Stdout mirrors the log with ANSI colours (WARN yellow, ERROR red). The in-game developer console (F2) also shows INFO and above in real time.

Suggestions are sorted alphabetically; Up/Down cycles through the full list and previews the selected completion as dim text in the input. Tab or a click accepts it, while Enter runs only the text already entered. Escape dismisses suggestions first, then closes the console. The input stays anchored when suggestions appear. Help lists command usage and descriptions on separate, wrapping lines.

Console command collisions qualify every participant as `<namespace>:<command>`
(engine built-ins use `core`); the ambiguous plain name stops resolving. Duplicate
registrations within a namespace are errors, including a core mod shadowing a built-in.
Use `scene load debug://editor` to instantiate a scene from a loaded mod, or
`core:scene load debug://editor` if another mod also registers `scene`.

By default only events from this crate are captured. Set `RUST_LOG` to override:

```sh
RUST_LOG=debug cargo run                        # all levels from this crate
RUST_LOG=wgpu=warn,whatever_engine=debug cargo run     # also include wgpu warnings
```

## Debug flags

Pass `--debug=<flags>` to enable verbose category logging (comma-separated). 
Lines are written to `logs/latest.log` alongside normal output and appear in the dev console (F2).

| Flag | What it logs |
|---|---|
| `all` | All categories below |
| `ipc` | Script diagnostics (legacy category name retained) |
| `modloader` | Mod discovery, dependency resolution, load order |
| `window` | Window lifecycle events |
| `vfs` | VFS reads, overrides, and cache misses |


Portable editor CI runs on Windows 2025, Ubuntu 24.04 and macOS 15 (Apple Silicon). Each job
builds and tests the workspace in the release profile, then assembles a release
archive and platform installer and runs relocation and installation checks. Windows
uses MSI, Ubuntu/Debian uses DEB, and macOS uses PKG. Installers register
`whatever-editor` and `wmod` for terminal use. Version tags publish the
exact distributions from successful main-branch CI for the tagged commit; all three platforms
must pass and their artifacts must still exist. Run `bun run package:editor` after `bun run build:native -- --release`
to reproduce packaging locally. See [EDITOR.md](docs/EDITOR.md) for distribution
contents, platform requirements and live Scene/Play editing.

Rust components inherit `[workspace.package].version`. `bun run check:versions`
checks them, their lockfile entries and the npm workspace/API versions together;
release tags must match too. Game and mod versions are independent.

We skipped releasing `0.5.0` on the way from `0.4.0` to `0.6.0`, so now it's a
tradition: Whatever release versions never contain the digit `5`, including
prerelease and build identifiers. We always go from `4` to `6`: `0.14.0` jumps to
`0.16.0`, `1.4.0` to `1.6.0`, and `0.6.4` to `0.6.6`. The version check enforces
this for the shared engine, tools and scripting API releases.
