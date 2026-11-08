---
title: Package and share a game
description: Validate a mod collection and export a relocatable game with the editor or CLI.
---

## Check the project

```sh
wmod project check ./hello-game
```

The project manifest selects the mod collection and optional build settings:

```toml title="hello-game/whatever.wproject"
version = 1
id = "hello_mod"
name = "Hello Game"
mods = ["mods/core"]

[build]
company = "My Studio"
version = "0.1.0"
```

Add optional `icon = "branding/icon.png"` only after adding that file. Build settings also support `exclude_mods` for development-only mods; exclusions must preserve required dependencies and the startup scene.

## Build in the editor

Set the name, company, version, and optional icon in Project Settings. Choose **Build Game**, select a new or empty output folder, and wait for Log to report completion. Required mods cannot be excluded while their dependents remain included. Builds run in the background and currently target the editor's host platform.

Installed editor distributions contain their build support payload. Source checkouts prepare the required player, launcher, and shared libraries using the normal native development prerequisites.

## Build with the CLI

```sh
wmod package ./hello-game --output ./Hello-Game-build
```

From a source checkout, use `cargo run -p wmod -- package ...`.

## Share the whole output

The output contains a branded launcher at the root, `bin/` with the native player and shared libraries, a project manifest, and mod sources and assets. Keep the whole folder together. Each private library ships once. The game resolves its data beside the installation.

The exported mod sources remain editable in the editor. Each scripted mod runs in its own isolated Wasm VM inside the player.

Packaging supports rebuilding a previous Whatever output and rejects unrelated nonempty destinations. It stages output and validates selected mods and scenes before publishing. See the [editor/deployment reference](/reference/editor/#build-game) for the exact layout and rebuild contract.
