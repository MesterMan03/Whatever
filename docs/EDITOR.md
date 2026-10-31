# Editor, projects and deployment

Whatever has a shared Rust engine crate (`engine/`), an ordinary player application
(`player/`) and a separate native editor (`editor/`). The editor authors mod files
without evaluating scripts. Play starts the normal sandboxed player. `wmod/` is
the CLI companion; both tools use the same project, scene and build code.

## Projects are mod collections

`whatever.wproject` is a versioned TOML manifest:

```toml
version = 1
name = "My Game"
mods = ["mods/core", "mods/my_game", "../shared_mod"]

[build]
icon = "branding/game-icon.png" # Optional PNG or ICO, relative to the project
company = "My Studio"
version = "1.0.0"
```

Each mod directory contains an ordinary `mod.toml`. Paths are relative to the
manifest; external directories are supported. A collection must contain its mods'
dependencies, including dependencies used only on the other runtime side. Build
settings are optional; older projects use the default Whatever icon, an empty
company and version `0.1.0`. The display name also determines the launcher name.

Opening a game folder without a manifest discovers `mods/` and `mods_user/`.
Deployed games include the same project manifest and mod sources, so the editor
can open a finished game directly. The player also uses an adjacent manifest;
`--project <file-or-directory>` selects another and cannot be combined with `--mods`.

## Editor workspace

```sh
cargo run -p whatever-editor -- games/test
```

File, Edit, Scene, Project and Build commands live in the native menu on Windows
and macOS. Other platforms use an in-window menu. Opening projects, game folders,
scenes, assets, icons and build destinations uses native dialogs. Paths shown in
the workspace omit Windows' internal verbatim prefix. Drop a project or scene
file onto the window to open it.

The workspace has a Hierarchy, central scene preview, Inspector and resizable
Project/Assets/Build Log browser. Select a mod in the browser and double-click a
scene to open it. Drag in the viewport to orbit, middle-drag to pan, scroll to
zoom, and press F over the viewport to frame the selected entity. The grid and
selection axes are editor overlays, not authored game entities.

Drag hierarchy rows to reorder or reparent entities. Dropping near a row's top or
bottom inserts a sibling; dropping in its middle makes a child. Blank hierarchy
space moves an entity to the scene root. Drop indicators distinguish insertion,
parenting and invalid cycles. Reparenting preserves the existing world transform
when the moved entity has a Transform; a zero-scale parent is rejected. Undo and
Redo include hierarchy changes.

The Inspector provides forms for Transform (including Euler rotation), cameras,
mesh/sprite/text rendering, all lights, physics worlds, rigid bodies, colliders
and joints. Asset fields offer VFS choices and a Browse dialog; selecting an asset
outside the collection imports it into the active mod. Adding a rigid body uses
an authored physics world, creating one if necessary. Custom mod components can
be added by namespaced type. Advanced fields and explicit JSON editing preserve
arbitrary mod data.

Scene drafts validate before applying. Invalid drafts stay editable and block
saving. Saves replace the scene through a temporary file; leaving an unsaved scene
offers Save, Discard or Cancel. Ctrl+S saves, Ctrl+Z/Ctrl+Y undo/redo, Ctrl+O opens
a project, and Ctrl+B builds. Play saves the scene and project before launching
the player. Scripts remain ordinary files for a code editor.

The viewport supports mesh-surface picking and Move/Rotate/Scale handles (Q/W/E/R).
Right-drag or Alt-left-drag orbits, and middle-drag pans. Drag colored handles to
transform a selected entity; the white center moves in the view plane or scales
uniformly. Each gesture is one undo step. Docking, prefabs and live script reload
remain future work.

## Build Game

Choose **Build Game** in the toolbar or Build menu, select an output folder, and
wait for the build log to report completion. The editor packages the player,
separate pinned mod runtime, native libraries and selected mods, then generates
the named launcher with its icon and Windows product/version metadata. Project
Settings controls the name, company, version and optional custom icon. A default
Whatever icon is supplied automatically. Builds run in the background.
On Windows the launcher delegates foreground activation to its player. The player
shows and focuses its standalone window after GPU initialization; embedded Play
continues to use click-to-focus.

Installed editor distributions include their engine support payload. They require
no Rust toolchain, Bun installation or manually assembled runtime directory for
game builds. From a source checkout, Build automatically compiles the release
player and launcher and prepares the pinned host if needed; the normal engine
development prerequisites apply. Play automatically prepares a debug player.
Builds currently target the editor's host platform.

```text
My Game.exe                    Small launcher with the game's name and icon
engine/
  Whatever.exe                 Native player and engine services
  runtime.exe                  Separate pinned Bun host with the scripting API
  lib/                         Native runtime support libraries
branding/
  icon.png                     Icon used by the player window
mods/
  core/
  my_game/
whatever.wproject              Portable mod collection and build settings
build.json                     Format, engine version, target and mod versions
LICENSE
BUN-LICENSE.md
```

Linux uses the same layout without `.exe`; PE icon/version resources apply to
Windows launchers. The launcher resolves the engine beside itself, forwards
arguments and returns its exit status. The player finds game data beside its
installation, independently of the working directory. Each scripted mod retains
its own sandboxed runtime process. Bun is not embedded in the player or launcher.

The launcher filename sanitizes invalid filename characters and Windows reserved
device names. Game window titles, modes and tick rates remain ordinary
`core/meta.toml` metadata. Installer generation and code signing are not provided.

Packaging remaps external mods to `mods/<id>`, preserves their sources, scenes,
assets and relative imports, and omits `.git`, `node_modules` and logs. It validates
both runtime sides and scene documents, rejects symlinks and overlapping source
paths, and stages output before publishing it. Choose a new or empty directory,
or an existing Whatever build to rebuild. Unrelated nonempty directories are
rejected. A previous build is backed up during publication and restored if the
final rename fails.

## CLI companion

Installed releases include `wmod`. From source, use `cargo run -p wmod --`:

```sh
wmod project create ./my-game --id my_game --name "My Game"
wmod project check ./my-game
wmod mod create ./my-game/mods/weather --id weather --project ./my-game
wmod project add ./my-game ../shared_mod
wmod package ./my-game --output ./My-Game-build
```

`project create` uses the installed core template; `--core <directory>` overrides
it. `mod create` accepts `--name` and `--no-scripts`. `wmod create <directory>`
retains interactive mod creation. Scripted templates include a scene, entry
script, `package.json` and `tsconfig.json`. Local SDK declarations are copied when
available; otherwise the package pins `@whatever-engine/api` as a development
dependency. Installed npm packages do not confer runtime permissions.

Release maintainers use `wmod runtime-kit` to assemble the support payload from
built player/host/launcher binaries. CI adds the editor and CLI to that same
distribution. This is distribution tooling, not a game developer setup step.
`package --runtime <distribution>` remains an advanced override.

Run `cargo test --workspace`, `bun run test:server` and `bun run test:project`
after changing this workflow. The project integration test checks automatic
installed-payload discovery, launcher metadata, safe rebuilding and a relocated
game running with an empty PATH. The physics demo loads its authored playground
through the same scene API tested on the headless server.

## Project identity, startup and build selection

The project owns `id` and `name`. Play uses them for game identity and window title;
packaging writes the same identity into the exported core/meta.toml and project
manifest. The core mod ID remains `core`, since it is the shared asset namespace.
Older manifests derive an ID from the project name when opened. New projects use
the ID entered at creation. Project Settings exposes a stable editable Game ID.

`[build] exclude_mods = ["debug", "network_example"]` removes development mods only
from exports. The editor and Play retain them. Core cannot be excluded; missing
dependencies and a startup scene removed by an exclusion reject the build.

The core mod may set a top-level `initial_scene = "my_game://main"` in mod.toml,
before `[mod]`. It is instantiated before script Init; its first enabled camera is
selected. A scene owned by a mod excluded on the current runtime side is skipped
on that side. An unknown namespace or missing scene is an error. Mods may adopt
`Scene.initialScene()` to attach behavior without making another instance.

Play embeds the ordinary player process in the viewport on Windows. Shift-click
Play keeps it in a separate window. Other platforms currently use separate-window
Play. The player retains its console (F2), input, audio and sandboxed hosts.

The creation menu offers Empty Entity, Cube, Sphere, Bean, cameras and lights;
mesh presets use shared core meshes. The asset browser has folder navigation,
breadcrumbs, search, compact tiles and portable type icons for meshes, shaders,
fonts, text, JSON, images and scenes. It does not depend on platform icon fonts.

Entities expose Enabled in the Inspector. Disabled parents also deactivate their
descendants. Joint components belong on the first body and expose Connected Body,
with a scene entity picker and hierarchy drag-and-drop.


## Welcome, scripts and editing controls

Starting the editor without a path opens Welcome: create a project, open a project,
open a deployed game folder, or choose a recent project. File â†’ Welcome / Close
Project returns here after checking unsaved changes. An explicit project argument
still opens it directly.

Edit â†’ Editor Settings selects Visual Studio Code, Notepad, Notepad++, VSCodium,
Sublime Text or a custom text-editor executable. Preferences and ten recent
projects persist in the user's Whatever/editor.json configuration. Scripts opens
mod source folders; double-click opens a source in that editor. Project â†’ New
Script creates a source file inside the active mod. Project â†’ Edit Active Mod
edits its manifest and script entry points, with full TOML available for advanced
settings. The mod namespace remains stable because references use that ID.

Click the hierarchy to target entity shortcuts: Ctrl+C/X/V copy, cut and paste a
subtree, Ctrl+D duplicates it, Delete removes it, and F2 focuses its name. Copies
receive fresh script IDs and remap references within the copied subtree. Text
fields retain normal text editing shortcuts. Undo and redo retain the selected
entity when it exists, including across hierarchy changes.

Move/Rotate/Scale support these temporary snapping steps; no modifier means free
movement unless the toolbar Snap checkbox enables the adjacent manual value:

| Held keys | Move (world units) | Rotate (degrees) | Scale (ratio increment) |
|---|---:|---:|---:|
| Shift | 1 | 45 | 0.5 |
| Ctrl | 0.5 | 15 | 0.1 |
| Shift+Ctrl | 0.25 | 10 | 0.05 |
| Alt | 0.1 | 5 | 0.025 |
| Shift+Alt | 0.05 | 1 | 0.01 |
| Ctrl+Alt | 0.01 | 0.5 | 0.005 |
| Shift+Ctrl+Alt | 0.001 | 0.1 | 0.001 |

Hover highlights and enlarges the active axis. Right-drag orbits and middle-drag
pans, leaving Alt available for snapping. Collider cards offer Edit in Scene;
joints offer owner/connected anchor handles. These temporary visual handles use
the normal tools to edit local component data while leaving the entity transform
alone. Collider scale edits shape dimensions; rotating a revolute/prismatic joint handle edits its
axis; other joint handles edit their frame rotation. Done or Escape returns to the entity. F frames the active handle.

Settings dialogs block workspace clicks, raw viewport gestures and shortcuts.
Embedded Windows Play confines the captured cursor to the child viewport and
releases it on console opening, focus loss or window destruction. Child cursor
messages explicitly restore visibility when capture ends.

The mod host builder defaults to the pinned release Bun fork. A debug host needs
`bun run build:host --debug-runtime` explicitly and cannot be packaged as a final
game. Source builds prepare the release fork automatically when needed. Debug
Bun includes debug checks and a much larger runtime; the compiled host embeds that
whole runtime rather than a small interpreter wrapper.


## Live Play editing

Play launches the normal player in a separate process and connects a private,
authenticated loopback editor channel. The hierarchy switches to the running
world, including entities created by scripts, disabled templates and children.
Runtime IDs include their generation, so a deleted entity's replacement is never
mistaken for the original. Names remain editable; runtime IDs are read-only.

The Scene tab uses the editor camera to pick, frame, move, rotate and scale live
entities or component handles. The Play tab shows the actual embedded player on
Windows. Shift-Play keeps its window separate; other platforms use that separate
window and show its camera in the editor's Play preview. Click the embedded player
to capture input; Escape releases it. Scene view and dialogs release player input
without stopping simulation. Runtime state panels expose physics status and body
state as read-only values.

Inspector edits, enable toggles, component changes, hierarchy moves, creation,
duplication and deletion target the running world. Changes are atomic at a tick
boundary before physics; leaf patches preserve fields that scripts or physics
updated after the last snapshot. Undo/redo applies only session edits. Source
scene saves and scene-file switching are blocked during Play. Stop restores the
original document, selection and authored history, discarding runtime edits.

The control token is generated per Play session and never reaches sandboxed mod
hosts. Snapshots use a bounded latest-state mailbox; socket reads/writes run on
background threads. Normal games without editor connection variables open no
editor listener or control endpoint.

## Portable editor distributions

`.github/workflows/editor.yml` builds, tests and packages native editor
archives for Windows/Linux x86_64 and macOS arm64. It runs for pull requests, main pushes and
manual dispatch. Version releases promote its exact archives only after all three
platforms pass on the tagged commit on `main`. Missing or expired artifacts block
publication; the release workflow also supports retrying an existing tag through
manual dispatch. The pinned native Bun binary is
cached by fork revision, platform and build configuration. Artifacts include:

- Native editor, wmod and launcher template.
- `engine/Whatever`, the separate `engine/runtime` mod host, required libraries.
- `mods/core` stock template, `sdk/index.d.ts`, licenses and usage instructions.

macOS wraps these files in `Whatever Editor.app/Contents/MacOS`; the other
platforms put them at the archive root. Archives preserve executable permissions.
Linux bundles non-system linked libraries with relative loader paths and targets
Ubuntu 24.04's glibc baseline; display/GPU drivers remain system components. The
macOS app is ad hoc signed and is not notarized. Windows packages available MSVC
CRT redistributable DLLs beside the executables.

Packaging copies the distribution to a temporary directory, creates a project
using its installed core and SDK, validates it through the installed editor, and
builds a game and runs its branded launcher/player/host headlessly with an empty
PATH before archiving.
No engine source checkout, Rust toolchain or system Bun is needed by the result.
`bun run package:editor` reproduces this locally; `--profile debug --output <new
folder>` is useful for a local smoke test, while CI always packages release.

The repository's test project lives in `games/test`; `cargo run` selects it by
default in source builds. Open it with `cargo run -p whatever-editor -- games/test`.
The engine's project-independent template lives in `engine/resources/core`.
