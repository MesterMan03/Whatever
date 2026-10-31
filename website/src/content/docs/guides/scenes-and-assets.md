---
title: Scenes and assets
description: Understand VFS paths, authored hierarchy, startup scenes, and immutable component reads.
---

## Use mod-qualified paths

Assets use `mod_id://relative/path`. The path is relative to the mod's configured asset root, normally `assets/`:

```text
hello_mod://textures/player.png  → mods/hello_mod/assets/textures/player.png
core://meshes/cube.obj           → mods/core/assets/meshes/cube.obj
```

Scenes use their mod's `scenes/` directory. `hello_mod://main`, `hello_mod://main.wscene`, and `hello_mod://scenes/main.wscene` resolve to the same scene file.

## Load and address a scene

```ts
import { Engine, Scene } from "@whatever-engine/api";

Engine.on("init", async () => {
  const scene = await Scene.loadScene("hello_mod://main");
  const camera = scene.get("camera");
  if (Engine.mode === "client") Engine.setMainCamera(camera);
  Engine.log("info", `Loaded ${scene.entityCount} entities in ${scene.loadTime} ms`);
});
```

`script_id` is an authored scene lookup key. It differs from an entity's native runtime ID, which is opaque and generation-aware. Loading a scene creates a new hierarchy; repeated calls create repeated instances.

Scene loading validates and instantiates atomically. An invalid component or reference rejects the load instead of leaving a partly created scene.

## Choose a startup scene

In `mods/core/mod.toml`, add the property **before** `[mod]`, at the TOML root:

```toml title="mods/core/mod.toml"
initial_scene = "hello_mod://main"

[mod]
id = "core"
name = "Core"
version = "0.1.0"

[assets]
root = "assets"
```

The engine instantiates it before script Init and selects its first enabled camera. Adopt the existing instance instead of loading a second copy:

```ts
Engine.on("init", async () => {
  const scene = Scene.initialScene() ?? await Scene.loadScene("hello_mod://main");
  if (Engine.mode === "client") Engine.setMainCamera(scene.get("camera"));
});
```

This assumes the project's startup scene is `hello_mod://main`. When writing a reusable mod for arbitrary projects, check `initial.path` before adopting it, as the scaffolded entry does.

## Edit data explicitly

Synchronous reads return immutable committed data plus your local command overlay. Modify components through a draft or set a new class instance:

```ts
import { BuiltInComponents, Scene } from "@whatever-engine/api";

const entity = Scene.createEntity()
  .setComponent(new BuiltInComponents.Transform({ position: [0, 2, 0] }));

entity.editComponent("core:transform", (transform) => {
  transform.setScaleUniform(2);
});
```

Use `requestComponent(...).get()` for an explicit ordered native query. A plain snapshot read does not wait for the engine to execute pending commands.

For file shapes, hierarchy, clone behavior, tags, references, and built-in component options, use the [scene reference](/reference/api/scenes/) and [entity reference](/reference/api/entities/). Custom WGSL and supported mesh formats are in the [shader reference](/reference/shaders/).
