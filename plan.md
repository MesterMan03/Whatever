# Whatever Engine — Architecture Plan

## Context
New cross-platform game engine in Rust. Core philosophy: **the engine is a barebones backend; every game is just a mod**. Engine owns window (winit), GPU renderer (wgpu), virtual file system, mod loader, and TypeScript/Bun scripting host — nothing else. First deliverable is a prototype renderer that loads all mods, finds every `.png` in the VFS, and renders them as quads in a 3D scene with a movable camera.

---

## 1. Source Tree

```
Whatever/
├── Cargo.toml
├── CLAUDE.md
├── mods/                          # engine-shipped mods (committed)
│   └── core/
│       ├── mod.toml
│       └── assets/shaders/
│           ├── sprite.wgsl
│           └── skybox.wgsl
├── mods_user/                     # user-installed mods (git-ignored)
├── runtime/                       # TypeScript SDK (shipped alongside binary)
│   └── engine_api.ts              # NDJSON IPC shim; every mod script imports this
└── src/
    ├── main.rs                    # CLI args, Engine::new(), engine.run()
    ├── engine.rs                  # Engine struct; owns all subsystems; drives main loop
    ├── input.rs                   # InputState: key + mouse delta accumulator
    ├── debug.rs                   # DebugConfig (CLI parsing) + DebugLogger (file writers)
    ├── vfs/
    │   ├── mod.rs                 # Vfs trait, VfsPath, VfsError, VfsHandle = Arc<dyn Vfs>
    │   ├── layered.rs             # LayeredVfs: stack of VfsLayer; last-pushed = highest priority
    │   └── disk.rs                # DiskLayer: reads from a real directory
    ├── mods/
    │   ├── mod.rs                 # re-exports; ModManager entry point
    │   ├── manifest.rs            # ModManifest serde struct (mod.toml schema)
    │   ├── loader.rs              # directory scan + Kahn's toposort dependency resolution
    │   └── registry.rs            # LoadedMod records; mod_id → metadata map
    ├── renderer/
    │   ├── mod.rs                 # re-exports; Renderer struct
    │   ├── context.rs             # WgpuContext: instance, device, queue, surface
    │   ├── camera.rs              # Camera + CameraController (WASD + mouse look)
    │   ├── texture.rs             # load_from_vfs() → wgpu::Texture
    │   └── scene.rs               # Scene: list of TexturedQuad; prototype grid layout
    └── script/
        ├── mod.rs                 # re-exports; ScriptHost struct
        ├── host.rs                # spawns one `bun run` per scripted mod; owns pipes
        ├── ipc.rs                 # EngineMessage + ScriptMessage enums + serde
        └── api.rs                 # dispatch: ScriptMessage → engine state mutations
```

> **Rule:** every `mod.rs` is a thin re-export file only. Logic lives in named sibling files.

---

## 2. Cargo.toml Dependencies to Add

```toml
winit          = "0.30"
wgpu           = "22"
bytemuck       = { version = "1", features = ["derive"] }
glam           = "0.29"
image          = { version = "0.25", default-features = false, features = ["png"] }
serde          = { version = "1", features = ["derive"] }
toml           = "0.8"
serde_json     = "1"
tokio          = { version = "1", features = ["full"] }
anyhow         = "1"
tracing        = "0.1"
tracing-subscriber = { version = "0.3", features = ["env-filter"] }

[profile.dev]
opt-level = 1   # wgpu is unusably slow at opt-level 0
```

---

## 3. Mod Structure

### Sample mod directory
```
mods/my_game/
├── mod.toml
├── assets/
│   ├── textures/player.png
│   └── data/config.json
└── scripts/
    └── index.ts               # entry point declared in mod.toml [script]
```

### `mod.toml` full schema
```toml
[mod]
id          = "my_game"         # snake_case, globally unique; MUST match directory name
name        = "My Game"
version     = "0.1.0"           # semver
description = ""                # optional
authors     = []                # optional
license     = ""                # optional SPDX

[dependencies]
# mod_id = "semver_req"   e.g.  core = ">=0.1"
# Missing dep = load failure with clear error

[load_order]
after  = []    # soft; mod IDs to load after (ignored if mod absent)
before = []

[script]
# Omit section entirely if mod has no TypeScript logic
entry   = "scripts/index.ts"   # relative to mod root
runtime = "bun"                 # only valid value for now

[assets]
root = "assets"                 # subtree mounted at mod_id:// in VFS; default "assets"

[overrides]
# Explicit path overrides (highest priority):
# "other_mod://textures/enemy.png" = "textures/my_enemy.png"
# Namespace-level shadowing:
# namespaces = ["other_mod"]
```

The built-in `core` mod (`id = "core"`) has no `[script]` and no `[dependencies]`. It always loads first.

---

## 4. VFS Design

**Path format:** `mod_id://relative/path/to/asset`

**Layering:** `LayeredVfs` holds a `Vec<VfsLayer>`. Layers are pushed in load order; last-pushed = highest priority. A read for `mod_id://path` walks layers top-to-bottom; first match wins.

**Overrides:** Explicit `[overrides]` mappings beat namespace-level layers.

```rust
// Critical interface (src/vfs/mod.rs)
pub trait Vfs: Send + Sync {
    fn read(&self, path: &VfsPath) -> Result<Vec<u8>, VfsError>;
    fn exists(&self, path: &VfsPath) -> bool;
    fn list(&self, mod_id: &str, prefix: &str) -> Result<Vec<String>, VfsError>;
}
pub type VfsHandle = Arc<dyn Vfs>;
```

---

## 5. Mod Loading Algorithm

1. Scan `mods/` then `mods_user/` — any subdirectory containing `mod.toml`
2. Parse each `mod.toml` into `ModManifest`
3. **Dependency validation:** every `[dependencies]` entry must exist and satisfy the semver req → error otherwise
4. **Toposort (Kahn's):** build dep-edge graph; process mods with in-degree 0 first; detect cycles → error
5. For each mod in toposorted order:
    - Push its `assets/` dir as a `VfsLayer` (last-pushed = highest priority)
    - Register explicit `[overrides]` mappings
    - If `[script]` present: register for script startup
6. After VFS is fully built: `ScriptHost::start_all()` — spawn Bun for each scripted mod, send `Init`

---

## 6. TypeScript / Bun IPC

**Transport:** newline-delimited JSON (NDJSON) on child process stdin/stdout. One Bun process per mod (crash isolation). Stderr → engine logger with mod_id prefix.

**Engine → Script:**
```jsonc
{ "type": "Init",     "mod_id": "my_game", "engine_version": "0.1.0" }
{ "type": "Frame",    "delta_seconds": 0.016, "frame_number": 42 }
{ "type": "Input",    "keys_pressed": ["KeyW"], "mouse_delta": [1.5, -0.3] }
{ "type": "AssetResponse", "request_id": "r1", "path": "...", "data_base64": "...", "error": null }
{ "type": "Shutdown" }
```

**Script → Engine:**
```jsonc
{ "type": "Subscribe",     "events": ["Frame", "Input"] }
{ "type": "AssetRequest",  "request_id": "r1", "path": "my_game://data/config.json" }
{ "type": "SpawnSprite",   "entity_id": "hero", "texture": "my_game://textures/player.png", "position": [0,0,0], "scale": [1,1,1] }
{ "type": "MoveEntity",    "entity_id": "hero", "position": [1,0,0] }
{ "type": "DestroyEntity", "entity_id": "hero" }
{ "type": "Log",           "level": "info", "message": "ready" }
```

`runtime/engine_api.ts` wraps `process.stdin/stdout` with NDJSON framing and an event-emitter API. It lives at `core://scripts/engine_api.ts` and is loaded via VFS — no npm install.

---

## 7. Prototype Renderer Boot Sequence

```
1. main() parses CLI → creates Engine
2. Engine::init():
   a. winit EventLoop + Window
   b. WgpuContext (instance, adapter, device, queue, surface)
   c. ModManager::discover_and_load() → LayeredVfs populated, ModRegistry filled
   d. ScriptHost::start_all() → Bun processes spawned
   e. Renderer::new(context, vfs) → compile core://shaders/sprite.wgsl
3. Prototype scan:
   for mod_id in registry:
       for path in vfs.list(mod_id, ""):
           if path ends with ".png":
               texture = load_from_vfs(device, queue, vfs, path)
               scene.add_sprite(texture, grid_pos(index))
               // grid: position = Vec3(col * 2.5, 0.0, row * 2.5), 8 cols
4. engine.run() → winit event loop (main thread; macOS requirement)
   - Tokio runtime on background thread pool for async IPC
   - Each frame: drain ScriptHost messages, update camera, render scene
```

**Camera:** WASD moves horizontally, mouse rotates yaw/pitch. Click to grab/release mouse. Camera uniform = `mat4x4` view-projection uploaded each frame via `queue.write_buffer`.

**Sprites:** each PNG = one quad (4 verts + index buffer) + one bind group (sampler + texture view). One draw call per sprite.

---

## 8. CLAUDE.md (to be created at repo root)

Contents:
- Project philosophy ("engine is barebones backend; every game is just a mod")
- Architecture overview (module purposes, one line each)
- Mod system description + VFS path convention
- IPC protocol summary
- Key commands (`cargo build`, `cargo run`, `cargo test`)
- "Adding a new mod" quick-start (4 steps)
- Coding conventions (no `unwrap`, no `println!`, no game logic in engine, `mod.rs` = re-exports only)
- Dependency philosophy (add only when correct impl > 1 day or platform complexity)

---

## 10. First Mod API — `setWindowTitle`

The very first engine capability exposed to mods. Validates the full round-trip: TypeScript call → IPC message → engine receives → winit window mutation.

### New IPC message (Script → Engine)
```jsonc
{ "type": "SetWindowTitle", "title": "My Awesome Game" }
```
Added to `ScriptMessage` enum in `src/script/ipc.rs`. Dispatch in `src/script/api.rs` calls `window.set_title(&title)` on the winit `Window`.

### TypeScript SDK (`runtime/engine_api.ts`)
```typescript
engine.setWindowTitle("My Awesome Game");
// Internally sends: { "type": "SetWindowTitle", "title": "My Awesome Game" }
```

### Test mod usage (in `scripts/index.ts`)
```typescript
import { engine } from "core://scripts/engine_api.ts";
engine.on("Init", () => {
    engine.setWindowTitle("Hello from Whatever!");
});
```

---

## 11. Debug CLI System

### Argument format
```
--debug=<type>[,<type>...]
```
Examples:
- `--debug=all` — enable all debug types
- `--debug=ipc,modloader` — enable only IPC and mod loader debug logs
- (multiple `--debug` flags are also accepted and merged)

### Debug types
| Type | What it logs |
|---|---|
| `window` | Window creation, resize, focus, title changes |
| `modloader` | Mod discovery, manifest parsing, dependency resolution steps, load order |
| `ipc` | Every NDJSON message sent and received, per mod_id, with direction arrow |
| `all` | All of the above |

### Log files
Written to `<CWD>/debug/<type>.log` (e.g., `debug/ipc.log`). Directory created on startup if it does not exist. Files are overwritten (not appended) each run — avoids unbounded growth during development.

### Implementation (`src/debug.rs`)
```rust
pub struct DebugConfig {
    pub window:     bool,
    pub modloader:  bool,
    pub ipc:        bool,
}

impl DebugConfig {
    pub fn from_args(args: &[String]) -> Self { ... }  // parses --debug= flags
}

pub struct DebugLogger {
    pub window:    Option<BufWriter<File>>,
    pub modloader: Option<BufWriter<File>>,
    pub ipc:       Option<BufWriter<File>>,
}

impl DebugLogger {
    pub fn new(config: &DebugConfig, cwd: &Path) -> anyhow::Result<Self> { ... }
    pub fn window(&mut self, msg: &str)    { self.write(&mut self.window, msg); }
    pub fn modloader(&mut self, msg: &str) { self.write(&mut self.modloader, msg); }
    pub fn ipc(&mut self, mod_id: &str, direction: &str, msg: &str) { ... }
    // format: "[mod_id] → { ... }" or "[mod_id] ← { ... }"
}
```

`DebugLogger` is owned by `Engine` and passed (mutably borrowed) to each subsystem on relevant operations. This keeps debug output completely separate from the `tracing` log stream.

---

## 12. Implementation Order

1. `Cargo.toml` — add all dependencies
2. `CLAUDE.md` — initialize
3. `src/debug.rs` — `DebugConfig` + `DebugLogger` *(parse CLI args early; needed by all subsystems)*
4. `src/vfs/` — `Vfs` trait + `VfsPath` + `LayeredVfs` + `DiskLayer` *(everything else depends on this)*
5. `src/mods/` — `ModManifest` serde + `ModManager` (scan + toposort + load) + `DebugLogger` calls
6. `mods/core/mod.toml` + placeholder shader files
7. `src/renderer/` — `WgpuContext` + `Camera` + `Texture::load_from_vfs` + `Scene` + `sprite.wgsl`
8. `src/input.rs` — `InputState`
9. `src/script/` — `ipc.rs` (include `SetWindowTitle` + all messages) + `ScriptHost` + `api.rs` dispatch + IPC debug logging
10. `runtime/engine_api.ts` — TypeScript IPC shim including `engine.setWindowTitle()`
11. `src/engine.rs` + `src/main.rs` — wire everything together; parse `--debug=` flags

---

## Verification

- `cargo build` compiles clean with no warnings
- Create `mods_user/test_mod/` with `mod.toml` + a `.png` in `assets/` → `cargo run` shows the PNG as a quad
- Create a second mod that overrides the first mod's PNG → the override wins in the rendered scene
- Add a `scripts/index.ts` that logs "hello" → message appears in engine log with mod_id prefix
- Camera moves with WASD; mouse look works after clicking window
