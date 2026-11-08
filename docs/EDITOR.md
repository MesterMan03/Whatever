# Editor, projects and deployment

Custom component fields may reference a component directly, for example
`session: RunSession | null = null`. The field picker lists only scene entities
containing that component type. The reference preserves the entity's authored ID
and required component type when saving, duplicating or instantiating a prefab.

Whatever has a shared Rust engine crate (`engine/`), an ordinary player application
(`player/`) and a separate native editor (`editor/`). The editor authors mod files
without evaluating scripts. Play starts the normal sandboxed player. `wmod/` is
the CLI companion; both tools use the same project, scene and build code. Native
applications dynamically share `whatever_engine`, `whatever_script_vm` and
`whatever_render_deps`. The VM/compiler payload appears once in the scripting
library. These Rust libraries must ship with executables from the same build.

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
Assets/Log browser. Opening a project selects the core mod's initial scene when
configured. Select a mod in Assets and double-click a scene to open it. Use
**Refresh** to update files and component types after external changes; returning
focus to the editor also refreshes both. Drag in the viewport to orbit, middle-drag to pan, scroll to
zoom, and press F over the viewport to frame the selected entity. The scene has no default grid. Transform handles and camera frustums are clipped editor overlays, not authored game entities.

Drag hierarchy rows to reorder or reparent entities. Dropping near a row's top or
bottom inserts a sibling; dropping in its middle makes a child. Blank hierarchy
space moves an entity to the scene root. Drop indicators distinguish insertion,
parenting and invalid cycles. Reparenting preserves the existing world transform
when the moved entity has a Transform; a zero-scale parent is rejected. Undo and
Redo include hierarchy changes.

The Inspector provides forms for Transform (including Euler rotation), cameras,
mesh/sprite/text rendering, all lights, physics worlds, rigid bodies, colliders
and joints. Asset fields offer VFS choices and a Browse dialog; selecting an asset
outside the collection imports it into the scene-owning mod. Adding a rigid body uses
an authored physics world, creating one if necessary. Exported custom Component classes appear in Add Component with schema-generated fields, class defaults, ranges, enum choices and entity/asset selectors. Metadata refreshes when the editor regains focus or through Reload component types; restart Play after source changes. The editor inspects sources without evaluating mod code. Each entity has an editable set of up to 32 tags, including live edits and undo/redo. Missing component data is preserved for repair; unknown or invalid components block saving and execution. Advanced native fields retain explicit JSON editing.

Scene drafts validate before applying. Invalid drafts stay editable and block
saving. Saves replace the scene through a temporary file; leaving an unsaved scene
offers Save, Discard or Cancel. Ctrl+S saves, Ctrl+Z/Ctrl+Y undo/redo, Ctrl+O opens
a project, and Ctrl+B builds. Play saves the scene and project before launching
the player. Scripts remain ordinary files for a code editor.

Component metadata reloads run on a background worker so the editor keeps
responding after script changes. The bottom status bar shows a spinner, the
current mod and overall mod progress. Repeated reload requests are coalesced;
results from a closed or replaced project are discarded. Existing component
types and unsaved edits stay available until a complete reload succeeds. Failures
keep the previous types and offer **View log** for details. Saving, Play and Build
wait for component types to finish loading successfully; scene saves reuse those
loaded types instead of launching another inspection. Play, Build and Mod Settings
validation also run in background jobs with status-bar progress. A host self-check
times out after 10 seconds, and an individual mod's source inspection times out
after 30 seconds, terminating and reaping a stalled host.

The viewport supports mesh-surface picking and Move/Rotate/Scale handles (Q/W/E/R).
All three tools follow the selected object's world rotation, including parents. Rotate uses three rings perpendicular to the local axes and applies signed, snapped rotation. Cameras show their oriented frustums; Look through camera previews a selected camera without changing the game's active camera. Every overlay is clipped to the scene rectangle.
Right-drag or Alt-left-drag orbits, and middle-drag pans. Drag colored handles to
transform a selected entity; the white center moves in the view plane or scales
uniformly. Each gesture is one undo step. Docking and live script reload
remain future work.

## Build Game

Choose **Build Game** in the toolbar or Build menu, select an output folder, and
wait for the build log to report completion. The editor packages the player with
its embedded VM, native libraries and selected mods, then generates
the named launcher with its icon and Windows product/version metadata. Project
Settings controls the name, company, version and optional custom icon. A default
Whatever icon is supplied automatically. Builds run in the background.
On Windows the launcher delegates foreground activation to its player. The player
shows and focuses its standalone window after GPU initialization; embedded Play
continues to use click-to-focus.

Installed editor distributions include their engine support payload. They require
no development toolchain or manually assembled runtime directory for
game builds. From a source checkout, Build automatically compiles the release
player and launcher with the already-built embedded guest; the normal engine
development prerequisites apply. Play automatically prepares a debug player.
Builds currently target the editor's host platform.

```text
My Game.exe                    Small launcher with the game's name and icon
bin/
  Whatever.exe                 Native player
  whatever_engine.dll          Shared engine services
  whatever_script_vm.dll       Embedded VM, SDK and compiler
  whatever_render_deps.dll     Graphics dependencies
  native-libraries.json        Required private library closure
  ...                          Matching Rust/MSVC support libraries
branding/
  icon.png                     Icon used by the player window
mods/
  core/
  my_game/
whatever.wproject              Portable mod collection and build settings
build.json                     Format, engine version, target and mod versions
LICENSE
RUNTIME-LICENSES.md
```

Linux uses the same layout without `.exe`; PE icon/version resources apply to
Windows launchers. The launcher resolves the engine beside itself, forwards
arguments and returns its exit status. The player finds game data beside its
installation, independently of the working directory. Each scripted mod has an isolated Wasm VM inside the player. Its embedded compiler
loads distributed JS/TS/JSON sources without external build tools.

The launcher filename sanitizes invalid filename characters and Windows reserved
device names. Game window titles, modes and tick rates remain ordinary
`core/meta.toml` metadata. Game-specific installers and code signing remain the
game publisher's responsibility; the installers below distribute the editor.

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
script, `package.json` and `tsconfig.json`. New games contain one scripted `mods/core` with their main scene and game assets. Additional mods do not automatically depend on core; declare dependencies when referencing its assets or custom components. Existing multi-mod projects remain supported and are not merged automatically.

Autocomplete references the editor's `sdk/index.d.ts` through a small git-ignored `.whatever/tsconfig.sdk.json`. Opening a project repairs this local binding after relocating the editor. `wmod project bind-sdk <project>` provides terminal setup. Legacy copied types are preserved until explicitly migrated. Game builds exclude local bindings and trash. The engine embeds matching API metadata for its VM.

Release maintainers use `wmod runtime-kit` to assemble the support payload from
the player and launcher binaries. CI adds the editor and CLI to that distribution.
`package --runtime <distribution>` remains an advanced override.

Run `bun run test:native`, `bun run test:server` and `bun run test:project`
after changing this workflow. The project integration test checks automatic
installed-payload discovery, launcher metadata, safe rebuilding and a relocated
game running with an empty PATH. The physics demo loads its authored playground
through the same scene API tested on the headless server.

## Project identity, startup and build selection

The project owns `id` and `name`. Play uses them for game identity and window title;
packaging writes the same identity into the exported core/meta.toml and project
manifest. The game mod ID remains `core`; custom `core:ClassName` components are supported while exact native IDs such as `core:transform` remain reserved.
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

The creation menu offers Empty Entity, Prefab, cameras and lights;
object templates come from `.wprefab` documents. Scene > Instantiate Prefab opens
the searchable project/official catalog. Choosing an official template imports
its editable `.wasset` library into the scene-owning mod's `libraries` folder.
Identical imports are reused; conflicts get a separate coherent bundle. Scene >
Save Selected as Prefab saves a self-contained subtree. Instances are independent
copies. See [Asset libraries and prefabs](ASSETS.md) for formats and tooling.
The asset browser has folder navigation,
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
projects persist in the user's Whatever/editor.json configuration. The Assets browser lists
mod source files; double-click opens a source in that editor. Project â†’ New
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

The native player embeds the SDK, TypeScript compiler and QuickJS Wasm VM. Source
maintainers rebuild these with `bun run build:runtime` and `bun run build:guest`
before compiling engine binaries. Distributed editor and game builds compile mods
internally.

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

## Editor distributions and installers

`.github/workflows/editor.yml` builds, tests and packages native editor
archives and installers for Windows/Linux x86_64 and macOS arm64. It runs for pull requests, main pushes and
manual dispatch. Version releases promote its exact artifacts only after all three
platforms pass on the tagged commit on `main`. Missing or expired artifacts block
publication; the release workflow also supports retrying an existing tag through
manual dispatch. The Wasm guest is built before native engine checks. Optimized Rust outputs
are cached by platform, compiler and Cargo configuration immediately after the
release build, before packaging starts. Artifacts include:

- `bin/whatever-editor`, `bin/wmod`, `bin/Whatever` and `bin/whatever-launcher`.
- Shared engine, VM, graphics, Rust standard library and required native libraries in `bin/`.
- `mods/core` stock template, `sdk/index.d.ts`, licenses and usage instructions.
- `resources/starter` editor-owned assets imported into games on demand.

Windows/Linux place data folders at the archive root. Start the actual editor in
`bin/`; there are no root editor/CLI wrapper executables. macOS puts executables,
libraries and support data in `Whatever Editor.app/Contents/MacOS`, with the
editor as the app's direct executable. Archives preserve executable permissions.
Linux bundles non-system linked libraries with relative loader paths and targets
Ubuntu 24.04's glibc baseline; display/GPU drivers remain system components. The
macOS app is ad hoc signed and is not notarized. Windows follows PE imports to
include required MSVC redistributables. Every private library has one copy beside
the executables. The dependency collector rejects basename collisions and writes
`native-libraries.json`; game exports copy only the player's required closure.
The branded game launcher is self-contained and remains at each exported game's root.

Packaging copies the distribution to a temporary directory, creates a project
using its installed core and SDK, validates it through the installed editor, and
builds a game and runs its branded launcher and player headlessly with an empty
PATH before archiving.
The result is a self-contained editor distribution.
`bun run package:editor` packages a release build. For a local smoke test, run
`bun run package:editor -- --profile debug --output ./dist/editor-smoke`.

Build native binaries with `bun run build:native -- --release` before packaging.
The helper configures the dynamic Rust library path for build tools and stages
the standard library for direct executable launches. Linux/macOS source builds
also embed library search paths. The branded launcher is built with static Rust
runtime linkage independently of the shared engine libraries. Native libraries and
executables form a single versioned distribution; replace them together.
Distribution CI builds and tests the release profile throughout, including native
unit tests and game integrations, so it does not also compile the dev profile.
Set `WHATEVER_TEST_PROFILE=release` to run the integration scripts against a local
release build. Source executables in the checkout's Cargo output directory can use
the checkout SDK/core/starter resources in either profile; relocated executables
must use their installed payload.

Generate an installer from that validated portable payload:

```sh
bun run package:editor
bun run package:installers -- --source dist/WhateverEditor-windows-x86_64
```

Use the matching platform/architecture source directory on Linux or macOS. The
installer command runs on the destination platform and requires its native tools:
WiX 5.0.2 with `WixToolset.UI.wixext/5.0.2` on Windows, `dpkg-deb` on Linux, or
`pkgbuild`/`pkgutil` on macOS. Windows CI installs WiX with the .NET tool CLI.

| Platform | Package and install location | Terminal registration |
|---|---|---|
| Windows | `.msi`, per-machine under `Program Files/Whatever Editor` | Appends `bin/` to system PATH; uninstall removes that entry. Also creates a Start menu shortcut. |
| Ubuntu/Debian | `.deb`, `/opt/whatever-editor` | Package-owned `/usr/bin/whatever-editor` and `/usr/bin/wmod` symlinks; removal deletes them. |
| macOS | `.pkg`, `/Applications/Whatever Editor.app` | `/etc/paths.d/whatever-engine` exposes the app's command directory to login shells. |

Open a new terminal after installation, then run `whatever-editor` or `wmod`.
Windows supports repair and major upgrades, including rebuilding the same release.
CI checks install, repair, upgrade and PATH cleanup on a disposable Windows runner,
plus command registration on Linux/macOS. The macOS package has no uninstaller;
remove the app and `/etc/paths.d/whatever-engine`, then use
`sudo pkgutil --forget engine.whatever.editor` to discard its receipt.
MSI/PKG signing is not configured by default. macOS maintainers may set
`WHATEVER_INSTALLER_IDENTITY` to sign the PKG with an installed installer certificate.

Packaged applications write logs under the OS local-data directory in
`Whatever/logs/<process-id>/`, so installed directories need no write access.

The repository's test project lives in `games/test`; `cargo run` selects it by
default in source builds. Open it with `cargo run -p whatever-editor -- games/test`.
The engine's project-independent minimal core template lives in `engine/resources/core`; its editor starter asset library lives in `engine/resources/starter`.

Assets and Log share one resizable panel height and padded headers. Assets displays scenes, scripts, ordinary assets and mod configuration in one folder browser, including empty folders. Create adds folders/scripts/scenes; the context menu moves, renames or deletes files and folders. Operations stay within their owning mod and run in the background. Moves update authored scene VFS references and manifest script/initial-scene entry points, preserve edited files and reject collisions; JS/TS imports and string literals need manual updates. Deletion moves items into ignored local `.whatever/trash`; managed root folders and mod configuration are protected.

Mod Settings exposes sides, direct dependencies and semver requirements, initial scene, script entries, asset root and load order. Asset/custom-component selectors respect the scene-owning mod's dependencies. Shared validation checks authored scene references on save, runtime load and packaging, including schema-defined nested asset fields. Build exclusions cannot remove a dependency of any included mod; the settings UI explains which mods require it. Raw TOML remains available for advanced configuration.

Embedded Play avoids synchronizing or drawing a second scene behind the native player
window. Returning to Scene rebuilds the deferred GPU scene from the latest world.
Live inspection snapshots are limited to roughly 30 Hz with latest-state mailboxes;
inspecting-state changes are coalesced. The editor frame loop is paced at 60 Hz,
and uses an unsynchronized parent swapchain while hosting the player. Child
visibility uses `ShowWindowAsync`, and repeated focus/layout calls are avoided.
Live document cloning/diffing runs only on editing input and actual dirty changes.
See [Microsoft's ShowWindowAsync documentation](https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-showwindowasync).
