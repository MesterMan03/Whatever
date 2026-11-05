---
title: Scenes and assets
description: Understand VFS paths, authored hierarchy, startup scenes, and immutable component reads.
---

## Use mod-qualified paths

Assets use `mod_id://relative/path`. The path is relative to the mod's configured asset root, normally `assets/`:

```text
core://textures/player.png  → mods/core/assets/textures/player.png
core://libraries/whatever_starter/meshes/cube.obj → mods/core/libraries/whatever_starter/meshes/cube.obj
```

Scenes use their mod's `scenes/` directory. `core://main`, `core://main.wscene`, and `core://scenes/main.wscene` resolve to the same scene file.

The game owns `mods/core`. **Scene > Instantiate Prefab** opens project and official templates. Choosing an official template imports its editable asset library under `libraries/`; empty projects contain no required starter assets. A scene may reference another mod's assets or custom components only when its owning mod declares a direct dependency on that mod. Missing referenced assets and undeclared dependencies reject saving, loading and game packaging.

## Asset libraries and prefabs

Share authoring content as `.wasset` ZIPs with creator/license metadata. Import them through **Project > Import Asset Library**. Prefabs are `.wprefab` JSON documents with one entity tree; use **Save Selected as Prefab** to author one. Every placed or spawned hierarchy is an independent copy. Imported scripts remain inside the normal mod sandbox. See the [library format and workflows](/reference/assets/) for namespace remapping, export and the official starter catalog.

```ts
import { Prefab } from "@whatever-engine/api";

const instance = await Prefab.spawn("core://prefabs/my_object.wprefab");
instance.root.setEulerDegrees(0, 90, 0);
```

## Load and address a scene

```ts
import { Engine, Scene } from "@whatever-engine/api";

Engine.on("init", async () => {
  const scene = await Scene.loadScene("core://main");
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
initial_scene = "core://main"

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
  const scene = Scene.initialScene() ?? await Scene.loadScene("core://main");
  if (Engine.mode === "client") Engine.setMainCamera(scene.get("camera"));
});
```

This assumes the project's startup scene is `core://main`. When writing a reusable mod for arbitrary projects, check `initial.path` before adopting it, as the scaffolded entry does.

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
