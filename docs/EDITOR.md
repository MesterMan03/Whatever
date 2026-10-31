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
cargo run -p whatever-editor -- .
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

The editor does not yet provide viewport object picking, interactive transform
gizmos, docking, prefabs or live script reload.

## Build Game

Choose **Build Game** in the toolbar or Build menu, select an output folder, and
wait for the build log to report completion. The editor packages the player,
separate pinned mod runtime, native libraries and selected mods, then generates
the named launcher with its icon and Windows product/version metadata. Project
Settings controls the name, company, version and optional custom icon. A default
Whatever icon is supplied automatically. Builds run in the background.

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
