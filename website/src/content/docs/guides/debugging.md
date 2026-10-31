---
title: Debug your mod
description: Log through the engine, register a console command, and reason about tick results.
---

## Start with logs

```ts
import { Engine } from "@whatever-engine/api";

Engine.on("init", ({ mod_id, mode }) => {
  Engine.log("info", `${mod_id} initialized on ${mode}`);
});
```

Logs include the mod identity. For more native diagnostics in a source checkout:

```sh
cargo run -- --project ./hello-game --debug=all
cargo run -- --project ./hello-game --debug=ipc,modloader
```

## Add a small diagnostic command

```ts
import { Console, Scene } from "@whatever-engine/api";

Console.register({
  name: "entity_count",
  description: "Count entities in the current scene snapshot",
  handler: () => `Entities: ${Scene.listEntities().length}`,
});
```

The engine uses the same command registry for the client console and server stdin. Commands can expose typed arguments, subcommands, and completion; see the [console reference](/reference/api/console/).

## Know what a read means

`entity.getComponent()` reads an immutable local view. To ask the engine for a value at a particular point in command order:

```ts
import { BuiltInComponents, Engine, Scene } from "@whatever-engine/api";

Engine.on("init", async () => {
  const entity = Scene.createEntity()
    .setComponent(new BuiltInComponents.Transform());
  const transform = await entity.requestComponent("core:transform").get();
  Engine.log("info", `Native position: ${transform?.position.join(", ")}`);
});
```

`.get()` waits for an already recorded result. It does not flush or rerun a command. Async handlers can span ticks; a busy tick handler is skipped, and elapsed simulation time accumulates. Use the [timing reference](/reference/api/timing/) when diagnosing apparent stale reads or delayed continuations.

## Inspect live state

During Play, use the editor's runtime hierarchy and Inspector to examine entities and component fields. Live edits are temporary. For performance, use `Profiler.latest()` for native tick CPU phases and `Profiler.runtime()` for host measurements. GPU execution timing is not currently measured; CPU submit time does not establish GPU cost.

## Common causes

| Problem | Likely cause |
| --- | --- |
| API import resolves in the editor but fails in a script | A bundle included a forbidden dependency or built-in; leave the engine API external |
| Component is missing from Add Component | Class isn't a named export, metadata extraction failed, or types need refreshing |
| Component starts ticking then stops | A lifecycle hook threw; inspect the component/entity log |
| Camera or window call fails on server | Presentation API used without a client guard |
| Scene appears twice | Script loaded a scene that was already configured as the startup scene |
| Edits disappear after stopping | They were live runtime edits; author the saved scene instead |
