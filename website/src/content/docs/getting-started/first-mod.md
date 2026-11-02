---
title: Your first mod
description: Load the scaffolded scene, spawn a cube, and rotate it with tick input.
---

This walkthrough starts with the `hello-game` project from [installation](/getting-started/installation/). You'll run a scene, add a cube, and give it a small piece of behavior.

## 1. Check the manifest

Open `mods/core/mod.toml`. Core is your game's main mod. The scaffold configures an initial scene and shared script entry:

```toml title="mods/core/mod.toml"
initial_scene = "core://main"

[mod]
id = "core"
name = "Hello Game"
version = "0.1.0"

[script]
entry = "scripts/index.ts"

[assets]
root = "assets"
```

The project manifest initially selects only `mods/core`. Other mods are optional. Scenes may reference their own mod's assets and assets from directly declared dependencies.

## 2. Load the scene and add a cube

Open `scenes/main.wscene` in Assets and create a **Cube** from the Hierarchy menu. The editor imports its mesh and shader into `core/assets/starter`; the cube has script ID `cube`. Save the scene, then replace the entry script with this example. Keep the configured initial scene.

```ts title="mods/core/scripts/index.ts"
import { Engine, Input, Scene, Time, Window } from "@whatever-engine/api";

Engine.on("init", async ({ mod_id }) => {
  const scene = Scene.initialScene();
  if (!scene) throw new Error("Configure core://main as the initial scene");

  if (Engine.mode === "client") {
    Window.setTitle("Hello, Whatever!");
    Engine.setMainCamera(scene.get("camera"));
  }

  scene.get("cube").addTag("tutorial.cube");

  Engine.log("info", `${mod_id} is ready`);
});

Engine.on("tick", () => {
  for (const cube of Scene.findByTag("tutorial.cube")) {
    cube.editComponent("core:transform", (transform) => {
      transform.rotateY(45 * Time.deltaSeconds);
    });
  }
});
```

`core://main` resolves to `mods/core/scenes/main.wscene`. The engine loads it before script initialization. Its camera has script ID `camera`; the cube's imported assets are ordinary game-owned files you can edit.

`Scene.initialScene()` adopts that existing scene without loading it twice. Script IDs find authored entities without depending on runtime IDs.

## 3. Press Play

Open `main.wscene` in the editor and press **Play**. Play saves the scene and project before launching the normal player. On Windows it is embedded in the editor viewport; Shift-click Play opens a separate window. Other platforms currently use a separate window.

You should see the authored cube rotate in the running game. Stopping Play discards its runtime rotation and restores the authored scene.

## 4. Try keyboard input

Replace the tick callback with this version, which reads the shared `Input` object:

```ts
Engine.on("tick", () => {
  const speed = Input.isPressed("Space") ? 180 : 45;

  for (const cube of Scene.findByTag("tutorial.cube")) {
    cube.editComponent("core:transform", (transform) => {
      transform.rotateY(speed * Time.deltaSeconds);
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
| Mesh fails to load | The cube's imported mesh and shader still exist under `assets/starter` |
| Code changes do nothing | Stop and restart Play; there is no live script reload |
| Import is denied | Use the embedded API or local source inside the mod root |

Next, move behavior into an [editor-visible component](/guides/components/), or learn the [editor workflow](/guides/editor/).
