---
title: Your first mod
description: Load the scaffolded scene, spawn a cube, and rotate it with tick input.
---

This walkthrough starts with the `hello-game` project from [installation](/getting-started/installation/). You'll run a scene, add a cube, and give it a small piece of behavior.

## 1. Check the manifest

Open `mods/hello_mod/mod.toml`. The scaffold creates a shared script entry:

```toml title="mods/hello_mod/mod.toml"
[mod]
id = "hello_mod"
name = "Hello Game"
version = "0.1.0"

[dependencies]
core = "*"

[script]
entry = "scripts/index.ts"

[assets]
root = "assets"
```

The project manifest selects `mods/core` and `mods/hello_mod`. A mod existing on disk is not enough when a project explicitly selects its collection.

## 2. Load the scene and add a cube

Replace the scaffolded entry script with this complete example. Leave the project's startup scene unset for this walkthrough; this script loads it once.

```ts title="mods/hello_mod/scripts/index.ts"
import { Engine, Scene, Window } from "@whatever-engine/api";

Engine.on("init", async ({ mod_id }) => {
  const scene = await Scene.loadScene("hello_mod://main");

  if (Engine.mode === "client") {
    Window.setTitle("Hello, Whatever!");
    Engine.setMainCamera(scene.get("camera"));
  }

  Scene.spawnMesh(
    "core://meshes/cube.obj",
    "core://shaders/mesh_lit.wgsl",
    [0, 1, 0],
  ).addTag("tutorial.cube");

  Engine.log("info", `${mod_id} is ready`);
});

Engine.on("tick", ({ delta_seconds }) => {
  for (const cube of Scene.findByTag("tutorial.cube")) {
    cube.editComponent("core:transform", (transform) => {
      transform.rotateY(45 * delta_seconds);
    });
  }
});
```

`hello_mod://main` resolves to `mods/hello_mod/scenes/main.wscene`. The scaffold supplies a camera with script ID `camera` and an ambient light. The cube uses stock core assets, so you don't need to import a mesh yet.

`spawnMesh` returns a pending entity. You can chain mutations such as `addTag` immediately; they stay ordered with its creation. You only need `.get()` when you need to wait for a native result.

## 3. Press Play

Open `main.wscene` in the editor and press **Play**. Play saves the scene and project before launching the normal player. On Windows it is embedded in the editor viewport; Shift-click Play opens a separate window. Other platforms currently use a separate window.

You should see a rotating cube in the running game. The cube is created by the script during Play, so it is not an authored entity in the saved scene. Stopping Play discards runtime changes.

## 4. Try keyboard input

Replace the tick callback with this version, which also reads `keys_pressed`:

```ts
Engine.on("tick", ({ delta_seconds, keys_pressed }) => {
  const speed = keys_pressed.includes("Space") ? 180 : 45;

  for (const cube of Scene.findByTag("tutorial.cube")) {
    cube.editComponent("core:transform", (transform) => {
      transform.rotateY(speed * delta_seconds);
    });
  }
});
```

Restart Play, focus the player, and hold Space. Angular speed uses simulation seconds, so changing the presentation FPS cap doesn't change the intended rotation speed.

## If something looks wrong

| Symptom | Check |
| --- | --- |
| Black screen / no active camera | The loaded scene has the scaffolded camera and its `script_id` is `camera` |
| Mod never starts | The project selects its directory; `[script].entry` points to the file you edited |
| Mesh fails to load | Both VFS paths include `core://` and the core template's assets are present |
| Code changes do nothing | Stop and restart Play; there is no live script reload |
| Import is denied | Use the embedded API or local source inside the mod root |

Next, move behavior into an [editor-visible component](/guides/components/), or learn the [editor workflow](/guides/editor/).
