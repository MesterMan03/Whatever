---
title: Install and create a project
description: Start with a portable editor distribution or build the tools from source.
---

## Use an editor distribution

Get an editor archive for your operating system from [GitHub Releases](https://github.com/MesterMan03/Whatever/releases). Extract the whole archive and keep its support files together. The portable distribution includes the editor, `wmod`, player, pinned mod host, SDK declarations, and stock core template.

The installed editor can run and package games without a separate Rust or Bun installation. Use the native welcome screen to create a project, or run the included CLI from a terminal:

```sh
wmod project create ./hello-game --id hello_mod --name "Hello Game"
wmod project check ./hello-game
```

Open the resulting `hello-game/whatever.wproject` in the editor. If `wmod` is not on your PATH, run it from the extracted distribution using the appropriate executable path.

:::note[Match your engine and API]
Use SDK types from the same editor distribution, or the npm API release matching your engine. The native host verifies its embedded API version. These docs follow the current source tree, which may include changes newer than a released archive.
:::

## Build from source

Install [Rust and Cargo](https://rustup.rs/) and [Bun](https://bun.sh/). Native windowing/audio development libraries and the Bun fork's native build prerequisites depend on your platform; the runtime build details live in the [sandbox reference](/reference/sandbox/#build-and-verification).

```sh
git clone --recurse-submodules https://github.com/MesterMan03/Whatever.git
cd Whatever
bun install --frozen-lockfile
bun run prepare:bun
bun run build:bun
bun run build:runtime
bun run build:host
cargo build --workspace
```

`prepare:bun` verifies the committed engine-owned fork. Mods run in the bundled `whatever-mod-host`; the system Bun installation is for development tooling.

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
    hello_mod/
      mod.toml
      package.json
      tsconfig.json
      scripts/index.ts
      scenes/main.wscene
      assets/
```

The scaffold includes a camera, ambient light, scene-loading entry script, and TypeScript configuration. SDK declarations are copied when available; otherwise the scaffold pins an API development dependency. Continue with [your first mod](/getting-started/first-mod/).
