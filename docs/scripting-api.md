# Scripting API

Mods with a `[script]` section in `mod.toml` run as a Bun subprocess. The engine
communicates with each script over NDJSON on stdin/stdout. The `@whatever-engine/api`
package abstracts the wire protocol.

```ts
import { Engine, Window, File, Scene, Mods, Message, Console } from "@whatever-engine/api";
```

The package is provided by the engine's workspace (`runtime/`). No install step
needed — Bun resolves it automatically for any script running within the engine
directory tree.

---

## `Engine`

### `Engine.on(event, handler)`

Subscribe to an engine event. Registering a handler also sends a `Subscribe`
message to the engine automatically (except `mod_message`, which is routed
unconditionally). For `tick`, all async handlers are awaited before the engine
advances the simulation.

```ts
Engine.on("init", ({ mod_id, engine_version }) => {
  Engine.log("info", `loaded as ${mod_id}, engine v${engine_version}`);
});

Engine.on("exit", ({ exit_code }) => {
  Engine.log("info", `shutting down (code ${exit_code})`);
});

Engine.on("tick", async ({ tick_number, delta_seconds, keys_pressed }) => {
  // game logic here — engine waits for this Promise to resolve
});
```

**Why there is no chicken-and-egg problem:** Bun runs the script's top-level code
synchronously before blocking on stdin. Any `Engine.on(...)` calls are registered
while the script is starting up. The engine only sends `Init` after the renderer
is ready — by which point the script is already listening.

### Events

| Event | Payload | Description |
|---|---|---|
| `init` | `{ mod_id, engine_version }` | Fired once after engine init and window ready |
| `exit` | `{ exit_code }` | Fired on shutdown; process exits after all handlers return |
| `tick` | `{ tick_number, delta_seconds, keys_pressed, mouse_delta }` | Fired every game tick; engine waits for async handlers |
| `mod_message` | `{ source_mod_id, message, request_id? }` | Message from another mod (see `Message`) |

### `Engine.log(level, message)`

Send a log message through the engine's logger. Output includes timestamp and mod ID.

```ts
Engine.log("info",  "everything is fine");
Engine.log("warn",  "something looks off");
Engine.log("error", "something broke");
```

| Parameter | Type | Values |
|---|---|---|
| `level` | `string` | `"info"` \| `"warn"` \| `"error"` |
| `message` | `string` | Arbitrary log text |

### `Engine.setTickRate(ticks_per_second)`

Override the game tick rate at runtime. Takes effect on the next tick.

```ts
Engine.setTickRate(60); // 60 ticks per second
```

The default tick rate is set by `tick_rate` in `core/meta.toml` (default: 60).

---

## `Window`

### `Window.setTitle(title)`

Change the title of the active window.

```ts
Window.setTitle("My Game — Level 1");
```

---

## `File`

Sandboxed per-mod file I/O. Paths must not contain `..`. Files are stored in a
mod-specific directory managed by the engine.

### `File.write(path, data)` → `Promise<void>`

Write a UTF-8 string to a sandboxed file.

```ts
await File.write("save.json", JSON.stringify(state));
```

### `File.read(path)` → `Promise<string>`

Read a sandboxed file and return its contents as a UTF-8 string. Rejects if the
file does not exist.

```ts
const data = await File.read("save.json");
```

### `File.delete(path)` → `Promise<void>`

Delete a sandboxed file.

---

## `Entity`

A live entity in the scene. All methods that create or enumerate entities
(`Scene.createEntity`, `Scene.listEntities`, `Scene.query`, `Scene.spawnSprite`)
return `Entity` objects rather than raw ID strings. Entities are identified
internally by an opaque string of the form `"index:generation"`.

For built-in component types (`core:transform`, `core:sprite_renderer`) the
TypeScript compiler automatically narrows the data parameter and return type.
For custom component types you can supply a generic `T`:

```ts
const comp = await entity.getComponent<MyComponent>("mymod:mycomp");
```

### `entity.id`

The underlying opaque entity ID string.

### `entity.destroy()`

Destroy this entity and all its components. Fire-and-forget.

### `entity.setComponent(component_type, data)`

Set a component on this entity. Fire-and-forget. Creates the component if absent,
overwrites if present. For built-in component types the compiler enforces the correct shape.

```ts
entity.setComponent("core:transform", {
  position: [0, 0, 0],
  rotation: [0, 0, 0, 1],
  scale: [1, 1, 1],
});
entity.setComponent("core:sprite_renderer", {
  texture: "my_mod://textures/player.png",
  z_index: 0,
});
```

A sprite becomes visible as soon as the entity has **both** `core:transform` and
`core:sprite_renderer` set.

### `entity.removeComponent(component_type)`

Remove a component from this entity. Fire-and-forget.

### `entity.getComponent(component_type)` → `Promise<T | null>`

Get a component's data. Returns `null` if the component is not set. Return type is
automatically inferred for built-in component types.

```ts
const t = await entity.getComponent("core:transform");
// t is BuiltInComponents.Transform | null — a live class instance
if (t) {
  Engine.log("info", `pos: ${t.getX()}, ${t.getY()}, ${t.getZ()}`);
  t.setX(t.getX() + 1);
  entity.setComponent("core:transform", t);
}
```

### `entity.move(position)` → `Promise<void>`

Convenience: update the `position` field of this entity's `core:transform` while
preserving existing rotation and scale. Initialises rotation/scale to identity/`[1,1,1]`
if the component is not yet set.

```ts
await entity.move([x, y, z]);
```

---

## `Scene`

Entity and component management. Entities are identified by opaque string IDs of
the form `"index:generation"`. Built-in components use the `core:` namespace.

Most callers should work with `Entity` objects returned by `Scene.createEntity`,
`Scene.listEntities`, `Scene.spawnSprite`, and `Scene.query` rather than calling
`Scene.setComponent` / `Scene.getComponent` with raw IDs.

### `Scene.createEntity()` → `Promise<Entity>`

Create a new entity and return it.

```ts
const entity = await Scene.createEntity();
```

### `Scene.destroyEntity(entity_id)`

Destroy an entity by raw ID and all its components. Fire-and-forget.
Prefer `entity.destroy()` when you already have an `Entity` object.

### `Scene.listEntities()` → `Promise<Entity[]>`

Return all living entities.

### `Scene.setComponent(entity_id, component_type, data)`

Set a component on an entity by raw ID. Fire-and-forget. For built-in component
types the compiler enforces the correct data shape. Prefer `entity.setComponent`
when you already have an `Entity` object.

```ts
Scene.setComponent(id, "core:transform", {
  position: [0, 0, 0],
  rotation: [0, 0, 0, 1],
  scale: [1, 1, 1],
});
```

### `Scene.removeComponent(entity_id, component_type)`

Remove a component from an entity. Fire-and-forget.

### `Scene.getComponent(entity_id, component_type)` → `Promise<T | null>`

Get a component's data by raw ID. Return type is automatically inferred for
built-in component types. Returns `null` if the component is not set.

```ts
const t = await Scene.getComponent(id, "core:transform");
// t is BuiltInComponents.Transform | null
```

### `Scene.query(component_types)` → `Promise<QueryResult[]>`

Return all entities that have every listed component type, along with the requested
component data.

```ts
const results = await Scene.query(["core:transform", "core:sprite_renderer"]);
for (const { entity, components } of results) {
  const t = components["core:transform"] as BuiltInComponents.Transform;
  await entity.move([t.position[0] + 1, t.position[1], t.position[2]]);
}
```

`QueryResult` shape: `{ entity: Entity; components: Record<string, JsonValue> }`.

### `Scene.spawnSprite(texture, position, scale?)` → `Promise<Entity>`

Convenience: create an entity with `core:transform` and `core:sprite_renderer`
pre-attached. Returns the entity.

```ts
const entity = await Scene.spawnSprite(
  "my_mod://textures/player.png",
  [0, 0, 0],
  [1, 1, 1],   // optional, defaults to [1, 1, 1]
);
```

### `Scene.moveEntity(entity_id, position)` → `Promise<void>`

Convenience: update the `position` field of an entity's `core:transform` while
preserving its rotation and scale.

```ts
await Scene.moveEntity(id, [x, y, z]);
```

### Built-in component shapes and classes

`getComponent` for built-in component types returns a live **class instance** (not a plain object), so methods are available immediately.

#### `BuiltInComponents.Transform`

Returned by `getComponent("core:transform")`. Raw fields are still present for backwards compatibility; all setters are chainable and return `this`.

| Field | Type | Description |
|---|---|---|
| `position` | `[x, y, z]` | World-space position |
| `rotation` | `[x, y, z, w]` | Unit quaternion (xyzw), identity = `[0, 0, 0, 1]` |
| `scale` | `[x, y, z]` | Per-axis scale, uniform = `[1, 1, 1]` |

**Position helpers**

| Method | Returns | Description |
|---|---|---|
| `getX()` / `getY()` / `getZ()` | `number` | Individual position components |
| `setX(x)` / `setY(y)` / `setZ(z)` | `this` | Set one position component |
| `getPosition()` | `[number, number, number]` | Copy of the position tuple |
| `setPosition(x, y, z)` | `this` | Set all three position components |

**Scale helpers**

| Method | Returns | Description |
|---|---|---|
| `getScaleX()` / `getScaleY()` / `getScaleZ()` | `number` | Individual scale components |
| `setScaleX(x)` / `setScaleY(y)` / `setScaleZ(z)` | `this` | Set one scale component |
| `getScale()` | `[number, number, number]` | Copy of the scale tuple |
| `setScale(x, y, z)` | `this` | Set all three scale components |
| `setScaleUniform(s)` | `this` | Set all three scale components to the same value |

**Distance**

| Method | Returns | Description |
|---|---|---|
| `distance(other)` | `number` | Euclidean distance between two transform positions |

**Rotation — raw quaternion**

| Method | Returns | Description |
|---|---|---|
| `getRotation()` | `[x, y, z, w]` | Copy of the quaternion |
| `setRotation(x, y, z, w)` | `this` | Set the quaternion directly |

**Rotation — Euler angles (intrinsic XYZ)**

Angles are applied in order: first around X, then around the new Y, then around the new Z. Near gimbal-lock (pitch ≈ ±90°) the decomposed X and Z values may be unstable.

| Method | Returns | Description |
|---|---|---|
| `getEulerRadians()` | `[rx, ry, rz]` | Decompose quaternion to radians |
| `setEulerRadians(rx, ry, rz)` | `this` | Build quaternion from radians |
| `getEulerDegrees()` | `[rx, ry, rz]` | Decompose quaternion to degrees |
| `setEulerDegrees(x, y, z)` | `this` | Build quaternion from degrees |

**Rotation — incremental world-space rotation**

Left-multiplies an axis rotation onto the current quaternion (world-space composition).

| Method | Returns | Description |
|---|---|---|
| `rotateX(degrees)` | `this` | Rotate by degrees around the world X axis |
| `rotateY(degrees)` | `this` | Rotate by degrees around the world Y axis |
| `rotateZ(degrees)` | `this` | Rotate by degrees around the world Z axis |

```ts
// Spin a sprite 45° per tick around Y, translate it, and push the change back.
Engine.on("tick", async () => {
  const t = await entity.getComponent("core:transform");
  if (!t) return;
  t.rotateY(45).setX(t.getX() + 0.1);
  entity.setComponent("core:transform", t);
});
```

The constructor `new BuiltInComponents.Transform({ position?, rotation?, scale? })` creates a fresh instance with identity defaults.

#### `BuiltInComponents.SpriteRenderer`

Returned by `getComponent("core:sprite_renderer")`.

| Field | Type | Description |
|---|---|---|
| `texture` | `string` | VFS path `mod_id://path/to/texture` |
| `z_index` | `number` | Draw order; higher = drawn on top |

| Method | Returns | Description |
|---|---|---|
| `getTexture()` | `string` | Current texture path |
| `setTexture(path)` | `this` | Change the texture |
| `getZIndex()` | `number` | Current z-index |
| `setZIndex(z)` | `this` | Change the z-index |

Sprites are rendered on the XZ plane; the Y axis is up. A sprite becomes visible as soon as the entity has **both** `core:transform` and `core:sprite_renderer` set.

---

## `Mods`

### `Mods.list()` → `Promise<ModManifest[]>`

Returns the manifests of all currently loaded mods in load order.

```ts
const mods = await Mods.list();
Engine.log("info", mods.map(m => m.id).join(", "));
```

### `Mods.get(id)` → `Promise<ModManifest>`

Returns the manifest for a specific mod by ID. Rejects if the mod is not loaded.

```ts
const core = await Mods.get("core");
Engine.log("info", `core version: ${core.version}`);
```

**`ModManifest` shape:**

```ts
type ModManifest = {
  id: string;
  name: string;
  version: string;
  description: string;
  authors: string[];
  license: string;
  dependencies: Record<string, string>;
  load_order: { after: string[]; before: string[] };
  script?: { entry: string; runtime: string };
};
```

---

## `Message`

Inter-mod communication. All payloads must be `JsonValue`
(`string | number | boolean | null | JsonValue[] | Record<string, JsonValue>`).

### `Message.sendAndForget(id, message)`

Send a fire-and-forget message to another mod.

```ts
Message.sendAndForget("other_mod", { type: "ping" });
```

### `Message.send(id, message, timeout)` → `Promise<JsonValue>`

Send a message and wait for a reply. Rejects with a timeout error if the
receiving mod does not reply within `timeout` ms.

```ts
const reply = await Message.send("other_mod", { type: "query" }, 5000);
```

### `Message.registerMessageHandler(handler)`

Register a handler for incoming messages from other mods. Return a `JsonValue`
to send a reply (only meaningful when the sender used `Message.send`); return
`null` to not reply.

```ts
Message.registerMessageHandler((payload) => {
  Engine.log("info", `message from ${payload.source_mod_id}`);
  return "pong"; // reply if sender used Message.send
});
```

The `request_id` in the payload is an opaque engine token — do not inspect or store it.

---

## `Console`

Register developer console commands that users can invoke at runtime.

### `Console.register(spec)`

```ts
Console.register({
  name: "mymod",           // must match [a-z_]+
  description: "My mod commands",
  subcommands: [
    {
      name: "greet",
      description: "Print a greeting",
      args: [
        { name: "name", type: "string", required: true, description: "Name to greet" },
      ],
      handler: ({ name }) => `Hello, ${name}!`,
    },
    {
      name: "count",
      description: "Print a number",
      args: [
        { name: "n", type: "int", required: false, description: "Number (default 42)" },
      ],
      handler: ({ n }) => `Count: ${n ?? 42}`,
    },
  ],
});
```

**`CommandSpec` fields:**

| Field | Type | Description |
|---|---|---|
| `name` | `string` | Command name, must match `[a-z_]+` |
| `description` | `string?` | Shown in `help` |
| `subcommands` | `CommandSpec[]?` | Nested subcommands |
| `args` | `ArgSpec[]?` | Positional arguments |
| `handler` | `(args) => string \| string[] \| Promise<...>` | Called on invocation; return one or more output lines. A node may have both `subcommands` and a `handler` — the handler runs when the node itself is invoked with no matching subcommand token. |

**`ArgSpec` fields:**

| Field | Type | Description |
|---|---|---|
| `name` | `string` | Argument name — used as the key in the handler's `args` object |
| `type` | `"string" \| "int" \| "float" \| "bool"` | Parsed type |
| `required` | `boolean?` | Defaults to `false` |
| `description` | `string?` | Shown in `help <command>` |

The handler receives a `Record<string, string | number | boolean>` where keys are
the `name` fields from the `ArgSpec` list. Missing optional args are absent from
the record (check with `arg in args` or `args.arg ?? default`).

Handlers may return a plain string, a string array (multiple output lines), or a
`Promise` resolving to either. The console shows a "waiting" indicator while a
promise is pending.

If a command name conflicts with an already-registered command, the engine will
register it as `mod_id:name` instead and log a warning.

---

## IPC protocol reference

This section documents the raw wire format. Normal mod code should use the API
namespaces above rather than speaking the protocol directly.

**Transport:** NDJSON (one JSON object per line) on stdin/stdout per process.
Stderr lines are forwarded to the engine logger with a `[mod_id]` prefix.

### Engine → Script

| Message | Key fields |
|---|---|
| `Init` | `mod_id`, `engine_version` |
| `Tick` | `tick_number`, `delta_seconds`, `keys_pressed`, `mouse_delta` |
| `FileResponse` | `request_id`, `data_base64`, `error` |
| `ModListResponse` | `request_id`, `mods` |
| `ModGetResponse` | `request_id`, `manifest`, `error` |
| `ModMessageReceived` | `source_mod_id`, `request_id`, `payload` |
| `ModMessageReplyDelivered` | `request_id`, `payload` |
| `EntityCreated` | `request_id`, `entity_id` |
| `EntityListResponse` | `request_id`, `entity_ids` |
| `ComponentGetResponse` | `request_id`, `entity_id`, `component_type`, `data`, `error` |
| `ComponentQueryResponse` | `request_id`, `results` |
| `CommandInvoke` | `request_id`, `command_path`, `args` |
| `Shutdown` | `exit_code` |

### Script → Engine

| Message | Key fields |
|---|---|
| `Subscribe` | `events` |
| `TickDone` | `tick_number` |
| `Log` | `level`, `message` |
| `SetWindowTitle` | `title` |
| `SetTickRate` | `ticks_per_second` |
| `FileWrite` | `request_id`, `path`, `data_base64` |
| `FileRead` | `request_id`, `path` |
| `FileDelete` | `request_id`, `path` |
| `ModListRequest` | `request_id` |
| `ModGetRequest` | `request_id`, `mod_id` |
| `ModMessageSend` | `target_mod_id`, `request_id`, `payload` |
| `ModMessageReply` | `request_id`, `payload` |
| `EntityCreate` | `request_id` |
| `EntityDestroy` | `entity_id` |
| `EntityListRequest` | `request_id` |
| `ComponentSet` | `entity_id`, `component_type`, `data` |
| `ComponentRemove` | `entity_id`, `component_type` |
| `ComponentGet` | `request_id`, `entity_id`, `component_type` |
| `ComponentQuery` | `request_id`, `component_types` |
| `RegisterCommand` | `name`, `description`, `subcommands`, `args`, `has_handler` |
| `CommandResponse` | `request_id`, `output`, `error` |
