---
title: How Whatever works
description: The engine owns native services. Mods own the game.
---

Whatever is a small native engine with a simple rule: **every game is a mod collection**. There is no privileged game layer hidden inside the player. A finished game consists of ordinary mods using the same services available to other mods.

## What the engine provides

The native engine owns the window, GPU rendering, input, virtual file system, entity storage, physics, networking, mod loading, and embedded script VMs. Your mods define movement, combat, levels, inventory, rules, and UI.

| Piece | What you work with |
| --- | --- |
| Project | A `whatever.wproject` file selecting mod directories and build settings |
| Mod | A directory with a `mod.toml` manifest, assets, scenes, and optional scripts |
| Scene | A versioned `.wscene` JSON document containing entities and their components |
| Entity | An identity with a hierarchy, tags, enabled state, and components |
| Component | Native data such as a Transform, or an exported custom class with fields and lifecycle hooks |
| Script VM | One isolated, embedded QuickJS Wasm instance per scripted mod on each active runtime side |

## Choose your path

- **New to modding:** follow [installation](/getting-started/installation/) and [your first mod](/getting-started/first-mod/).
- **Working in the editor:** start with the [editor walkthrough](/guides/editor/) and [components guide](/guides/components/).
- **Looking up a function:** use the [scripting reference](/reference/api/overview/) or search with Ctrl+K.
- **Building a compatible API:** read the [native IPC protocol](/protocol/specification/), then its [message catalogue](/protocol/messages/).

## A few rules to know early

`core` loads first and supplies metadata and stock assets. Other mods load in dependency order. Each game owns its collection; the source checkout uses `games/test` by default.

Scripts import the engine-provided `@whatever-engine/api`. A development dependency can provide editor types. The embedded VM resolves the SDK and local JS/TS/JSON sources within your mod. See the [sandbox policy](/reference/sandbox/).

The TypeScript API records commands. The engine applies them in batches during simulation ticks. That distinction matters for reads and promises; the [timing reference](/reference/api/timing/) explains exactly when work becomes visible.
