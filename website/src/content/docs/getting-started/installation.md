---
title: Install and create a project
description: Install the editor, use a portable distribution, or build the tools from source.
---

## Use an editor distribution

Get an installer or portable archive for your operating system from [GitHub Releases](https://github.com/MesterMan03/Whatever/releases). Each includes the editor, `wmod`, player, shared libraries, SDK declarations, and stock core template.

| Platform | Installer | Terminal commands |
|---|---|---|
| Windows | Run the `.msi`; the default location is `Program Files/Whatever Editor` | Adds its `bin` directory to system PATH and creates a Start menu shortcut. |
| Ubuntu/Debian | Run `sudo apt install ./WhateverEditor-linux-x86_64.deb` | Installs `whatever-editor` and `wmod` links in `/usr/bin`. Requires Ubuntu 24.04 or a compatible glibc 2.39 system. |
| macOS | Open the `.pkg` to install `/Applications/Whatever Editor.app` | Registers its command directory through `/etc/paths.d`. |

Open a new terminal after installing. To use a portable archive instead, extract the
whole archive and keep its support files together. Windows/Linux start the actual
editor at `bin/whatever-editor` (`.exe` on Windows); macOS opens `Whatever Editor.app`.
The app's terminal tools live in `Contents/MacOS`. Portable archives leave PATH unchanged.

The installed editor includes the tools to run and package games. Use the native welcome screen to create a project, or run the included CLI from a terminal.

```sh
wmod project create ./hello-game --id hello_mod --name "Hello Game"
wmod project check ./hello-game
```

Open the resulting `hello-game/whatever.wproject` in the editor. If `wmod` is not on your PATH, run it from the extracted distribution using the appropriate executable path.

:::note[Match your engine and API]
Use SDK types from the same editor distribution, or the npm API release matching your engine. The SDK and VM are embedded with their matching engine version. These docs follow the current source tree, which may include changes newer than a released archive.
:::

## Build from source

Install [Rust and Cargo](https://rustup.rs/) and [Bun](https://bun.sh/) 1.4.2+. Bun runs development tooling; installed mods run in the engine's embedded Wasm VM. Native windowing, audio and Wasm build prerequisites depend on your platform. See the [sandbox reference](/reference/sandbox/#build-and-verification).

```sh
git clone https://github.com/MesterMan03/Whatever.git
cd Whatever
bun install --frozen-lockfile
rustup target add wasm32-unknown-unknown
bun run build:runtime
bun run build:guest
bun run build:native
```

The engine embeds its Wasm VM, SDK and TypeScript compiler. Installed games compile raw mod sources internally.

Create and open the tutorial project from the checkout:

```sh
cargo run -p wmod -- project create ./hello-game --id hello_mod --name "Hello Game"
cargo run -p whatever-editor -- ./hello-game
```

To explore the existing development collection instead:

```sh
cargo run -p whatever-editor -- games/test
```

## What was created

```text
hello-game/
  whatever.wproject
  mods/
    core/
      mod.toml
      meta.toml
      assets/
      package.json
      scripts/index.ts
      scenes/main.wscene
  tsconfig.json
  .whatever/tsconfig.sdk.json  (local, git-ignored)
```

The game lives in `mods/core`, including its entry script, camera, ambient light and initial scene. Assets start empty. Creating a primitive imports the needed editable resources from the editor's library. TypeScript configuration references the installed SDK without copying declarations. Opening the project repairs its local SDK binding after relocation; terminal users can run `wmod project bind-sdk ./hello-game`. Continue with [your first mod](/getting-started/first-mod/).
