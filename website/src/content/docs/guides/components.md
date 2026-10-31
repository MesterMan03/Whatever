---
title: Write custom components
description: Turn behavior into reusable, persisted classes with editor-visible fields.
---

A custom component combines persisted data, editor fields, and lifecycle hooks. Export a named class extending `Component` from a local source file. The host discovers and registers it before the script entry runs.

## Define a spinning component

Add this file to the tutorial mod:

```ts title="mods/hello_mod/scripts/spin.ts"
import { Component, Type, field } from "@whatever-engine/api";

export class Spin extends Component {
  static readonly type = "hello_mod:spin";

  degreesPerSecond = field(Type.F64.with({ min: -360, max: 360 }), 45, {
    label: "Rotation speed",
    tooltip: "Degrees per simulation second around the Y axis.",
  });

  override tick(deltaSeconds: number) {
    this.entity.editComponent("core:transform", (transform) => {
      transform.rotateY(this.degreesPerSecond * deltaSeconds);
    });
  }
}
```

This class has a stable ID in its owning mod's namespace. Its constructor needs no arguments. The `field` helper supplies a schema and inspector hints while retaining a normal typed field value.

## Attach it from a script

Import `Spin` in the entry script and attach it when creating the cube:

```ts
import { Engine, Scene } from "@whatever-engine/api";
import { Spin } from "./spin.ts";

Engine.on("init", () => {
  Scene.spawnMesh(
    "core://meshes/cube.obj",
    "core://shaders/mesh_lit.wgsl",
    [0, 1, 0],
  ).setComponent(new Spin());
});
```

This is an attachment example: keep the scene-loading and camera setup from the first tutorial. Remove its global rotation callback, so the same cube isn't rotated twice.

## Attach it in the editor

For an authored entity with a Transform and MeshRenderer, choose **Add Component → hello_mod:spin**. Set Rotation speed in the Inspector, save, and restart Play. Refresh component types if the class isn't listed yet.

## Choose persisted fields deliberately

Public writable fields are persisted. Numbers, booleans, strings, arrays, tuples, nested objects, enums, and entity references are supported. Static, readonly, private/protected fields, methods, and getters are excluded.

```ts
export class Counter extends Component {
  static readonly type = "hello_mod:counter";
  total = field(Type.I32.with({ min: 0 }), 0);
  private elapsed = 0;

  override tick(deltaSeconds: number) {
    this.elapsed += deltaSeconds;
    if (this.elapsed >= 1) {
      this.total += 1;
      this.elapsed -= 1;
    }
  }
}
```

`total` is component data. `elapsed` is private runtime state. Add the same API imports as the first example. Defaults must be supported literals or literal constants; arbitrary initializer calls are diagnosed during metadata extraction.

## Understand lifecycle and async work

An active instance receives `onEnable()`, `start()` once, then `tick(deltaSeconds)`. Disabling it or an ancestor pauses ticks and calls `onDisable()`. Reenabling calls `onEnable()` again. Removal and orderly shutdown dispose the instance with `destroy()`.

`start` and `tick` can return promises. Busy instances are skipped without overlapping calls; elapsed simulation time accumulates. Commands still flush each tick. A hook failure logs the component/entity and stops that instance's ticking.

Keep component modules free of top-level scene mutations. Put initialization in `start()` or an `init` event. See [schemas and components](/reference/api/components/) for extraction rules, validation, references, and field patch behavior.
