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

### `Engine.setFpsCap(fps)`

Set a maximum frames-per-second limit. Pass `null` to remove the cap (uncapped).
When a cap is set the engine uses `ControlFlow::WaitUntil` for precise frame pacing;
without a cap the GPU present (vsync) is the only pacing mechanism.

```ts
Engine.setFpsCap(60);   // cap at 60 FPS
Engine.setFpsCap(null); // uncapped
```

### `Engine.setVsync(enabled)`

Enable or disable vertical sync. Takes effect immediately by reconfiguring the wgpu
surface present mode (`AutoVsync` / `AutoNoVsync`).

```ts
Engine.setVsync(true);  // on (default)
Engine.setVsync(false); // off
```

### `Engine.setMainCamera(entity_id)`

Designate an entity as the scene's active camera.  The entity must have both
`core:camera` and `core:transform` components attached; the engine reads them
every frame to produce the view-projection matrix.

```ts
const cam = await Scene.createEntity();
cam.setComponent(new BuiltInComponents.Transform({ position: [0, 5, 10] }));
cam.setComponent(new BuiltInComponents.Camera({ fovy_degrees: 60 }));
Engine.setMainCamera(cam.id);
```

Multiple camera entities can coexist in the scene; only the designated one is
rendered.  If no camera is set, or if the entity dies, the screen clears to
black and a warning overlay is displayed.

### `Engine.clearCamera()`

Remove the active camera.  The screen clears to black and shows a "no active
camera" warning until `Engine.setMainCamera` is called again.

```ts
Engine.clearCamera();
```

---

## `Window`

### `Window.setTitle(title)`

Change the title of the active window.

```ts
Window.setTitle("My Game — Level 1");
```

### `Window.setSize(width, height)`

Request a new inner window size in physical pixels. Has no effect when the window is in a fullscreen mode.

```ts
Window.setSize(1280, 720);
```

### `Window.setMode(mode)`

Set the window display mode.

```ts
Window.setMode("windowed");    // normal window
Window.setMode("borderless");  // borderless fullscreen (windowed fullscreen)
Window.setMode("fullscreen");  // exclusive fullscreen; falls back to borderless if unavailable
```

| Value | Description |
|---|---|
| `"windowed"` | Normal windowed mode |
| `"borderless"` | Borderless fullscreen using the current monitor |
| `"fullscreen"` | Exclusive fullscreen using the monitor's preferred video mode |

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

### `entity.setComponent(component_type, data)` / `entity.setComponent(component)`

Set a component on this entity. Fire-and-forget. Creates the component if absent,
overwrites if present. For built-in component types the compiler enforces the correct shape.

Accepts either a `(component_type, data)` pair or a single `Component` instance — the
component ID is read from the object's `id` field automatically.

```ts
// Plain object form
entity.setComponent("core:transform", {
  position: [0, 0, 0],
  rotation: [0, 0, 0, 1],
  scale: [1, 1, 1],
});
entity.setComponent("core:sprite_renderer", {
  texture: "my_mod://textures/player.png",
});

// Component instance form — id is inferred from the object
const t = new BuiltInComponents.Transform({ position: [1, 2, 3] });
entity.setComponent(t);

// Common pattern: fetch, mutate, push back
const t = await entity.getComponent("core:transform");
if (t) {
  t.addX(1);
  entity.setComponent(t); // no need to repeat the component type string
}
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

### `entity.setParent(parent)`

Attach this entity to a parent. `parent` can be an `Entity`, a raw entity ID
string, or `null` to detach. Fire-and-forget.

Once parented, `core:transform` values are **local** — relative to the parent's
world-space transform. Without a parent the local transform equals the world
transform.

```ts
entity.setParent(parentEntity);    // attach
entity.setParent("5:0");           // attach by raw ID
entity.setParent(null);            // detach
```
### `entity.getParent()` → `Promise<Entity | null>`

Return the parent `Entity`, or `null` if this entity has no parent.

```ts
const parent = await entity.getParent();
if (parent) Engine.log("info", `parent: ${parent.id}`);
```

### `entity.getChildren()` → `Promise<Entity[]>`

Return all direct children of this entity.

```ts
const kids = await entity.getChildren();
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

### `Scene.spawnText(text, position, options?)` → `Promise<Entity>`

Convenience: create an entity with `core:transform` and `core:text_renderer` pre-attached. Returns the entity.

```ts
const label = await Scene.spawnText("Hello, world!", [0, 0, 0]);

// With options:
const label = await Scene.spawnText("Score: 0", [0, 0, 2], {
  font: "my_mod://fonts/custom.ttf",   // optional, defaults to core://fonts/default.ttf
  font_size: 32,                        // optional, defaults to 24
  color: [1, 0.8, 0, 1],               // optional RGBA, defaults to [1, 1, 1, 1]
});
```

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

### `Scene.setParent(entity_id, parent_id)`

Attach an entity to a parent by raw IDs. Pass `null` as `parent_id` to detach.
Fire-and-forget. Prefer `entity.setParent()` when you have an `Entity` object.

```ts
Scene.setParent(childId, parentId);
Scene.setParent(childId, null); // detach
```

### `Scene.getParent(entity_id)` → `Promise<string | null>`

Return the parent entity ID as a string, or `null` if the entity has no parent.

### `Scene.getChildren(entity_id)` → `Promise<string[]>`

Return the IDs of all direct children of the given entity.

### Built-in component shapes and classes

`getComponent` for built-in component types returns a live **class instance** (not a plain object), so methods are available immediately.

#### `BuiltInComponents.Transform`

Returned by `getComponent("core:transform")`. Raw fields are still present for backwards compatibility; all setters are chainable and return `this`.

| Field | Type | Description |
|---|---|---|
| `position` | `[x, y, z]` | Local-space position (world-space when the entity has no parent) |
| `rotation` | `[x, y, z, w]` | Local-space unit quaternion (xyzw), identity = `[0, 0, 0, 1]` |
| `scale` | `[x, y, z]` | Local-space per-axis scale, uniform = `[1, 1, 1]` |

**Position helpers**

| Method | Returns | Description |
|---|---|---|
| `getX()` / `getY()` / `getZ()` | `number` | Individual position components |
| `setX(x)` / `setY(y)` / `setZ(z)` | `this` | Set one position component |
| `addX(x) / addY(y) / addZ(z)` | `this` | Add to one position component |
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

| Method | Returns | Description |
|---|---|---|
| `getTexture()` | `string` | Current texture path |
| `setTexture(path)` | `this` | Change the texture |

Sprites are rendered on the XZ plane; the Y axis is up. A sprite becomes visible as soon as the entity has **both** `core:transform` and `core:sprite_renderer` set.

#### `BuiltInComponents.TextRenderer`

Returned by `getComponent("core:text_renderer")`. Renders a UTF-8 string at the entity's world-space Transform position, projected to screen coordinates.

| Field | Type | Default | Description |
|---|---|---|---|
| `text` | `string` | `""` | The string to render |
| `font` | `string` | `"core://fonts/default.ttf"` | VFS path to a TTF or OTF font |
| `font_size` | `number` | `24` | Font size in logical pixels |
| `color` | `[r, g, b, a]` | `[1, 1, 1, 1]` | RGBA colour, each channel in `[0.0, 1.0]` |

| Method | Returns | Description |
|---|---|---|
| `getText()` | `string` | Current text string |
| `setText(text)` | `this` | Change the displayed text |
| `getFont()` | `string` | Current font VFS path |
| `setFont(path)` | `this` | Change the font |
| `getFontSize()` | `number` | Current font size |
| `setFontSize(size)` | `this` | Change the font size |
| `getColor()` | `[r, g, b, a]` | Copy of the colour tuple |
| `setColor(r, g, b, a)` | `this` | Change the colour |

Text is rendered on top of sprites. The engine ships Noto Sans as `core://fonts/default.ttf`; any mod can supply alternative fonts as TTF/OTF assets and pass the VFS path in the `font` field.

```ts
// Spawn a white label
const label = await Scene.spawnText("Hello!", [0, 0, 0], { font_size: 36 });

// Update it each tick
Engine.on("tick", async ({ tick_number }) => {
  const t = await label.getComponent("core:text_renderer");
  if (!t) return;
  t.setText(`Tick: ${tick_number}`);
  label.setComponent("core:text_renderer", t);
});
```

#### `BuiltInComponents.Camera`

Returned by `getComponent("core:camera")`.  Makes the entity usable as a scene
camera.  Combine with `core:transform` to control position and orientation, then
activate the camera with `Engine.setMainCamera(entity.id)`.

The camera looks in the **−Z** direction of its local frame.  Rotating the
entity via `core:transform` orbits/tilts the camera accordingly.

| Field | Type | Default | Description |
|---|---|---|---|
| `fovy_degrees` | `number` | `45` | Vertical field-of-view in degrees |
| `znear` | `number` | `0.1` | Near clip plane distance |
| `zfar` | `number` | `1000` | Far clip plane distance |

| Method | Returns | Description |
|---|---|---|
| `getFov()` | `number` | Current vertical FOV in degrees |
| `setFov(degrees)` | `this` | Change the vertical FOV |
| `getZNear()` | `number` | Current near clip distance |
| `setZNear(v)` | `this` | Change the near clip distance |
| `getZFar()` | `number` | Current far clip distance |
| `setZFar(v)` | `this` | Change the far clip distance |

```ts
// Minimal camera setup
Engine.on("init", async () => {
  const cam = await Scene.createEntity();
  cam.setComponent(new BuiltInComponents.Transform({ position: [0, 5, 10] }));
  cam.setComponent(new BuiltInComponents.Camera({ fovy_degrees: 60 }));
  Engine.setMainCamera(cam.id);
});

// Move camera each tick
Engine.on("tick", async ({ delta_seconds }) => {
  const t = await cam.getComponent("core:transform");
  if (!t) return;
  t.addZ(-5 * delta_seconds); // fly forward along -Z
  cam.setComponent(t);
});
```

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
| `suggest` | `(current: string) => string[] \| Promise<string[]>` | Optional. Called by the engine to provide autocomplete suggestions as the user types this argument. Receives the current raw text (empty string if nothing typed yet). Return a list of candidate strings; the engine sends the request via IPC and updates the completion dropdown when the promise resolves. |

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
| `EntityParentResponse` | `request_id`, `entity_id`, `parent_id` (string or null) |
| `EntityChildrenResponse` | `request_id`, `entity_id`, `child_ids` |
| `ComponentGetResponse` | `request_id`, `entity_id`, `component_type`, `data`, `error` |
| `ComponentQueryResponse` | `request_id`, `results` |
| `CommandInvoke` | `request_id`, `command_path`, `args` |
| `ArgSuggestRequest` | `request_id`, `command_path`, `arg_index`, `current` |
| `Shutdown` | `exit_code` |

### Script → Engine

| Message | Key fields |
|---|---|
| `Subscribe` | `events` |
| `TickDone` | `tick_number` |
| `Log` | `level`, `message` |
| `SetWindowTitle` | `title` |
| `SetWindowSize` | `width`, `height` |
| `SetWindowMode` | `mode` (`"windowed"` \| `"borderless"` \| `"fullscreen"`) |
| `SetTickRate` | `ticks_per_second` |
| `SetFpsCap` | `fps` (`number \| null`) |
| `SetVsync` | `enabled` |
| `SetMainCamera` | `entity_id` (empty string = clear) |
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
| `EntitySetParent` | `entity_id`, `parent_id` (string or null) |
| `EntityGetParent` | `request_id`, `entity_id` |
| `EntityGetChildren` | `request_id`, `entity_id` |
| `ComponentSet` | `entity_id`, `component_type`, `data` |
| `ComponentRemove` | `entity_id`, `component_type` |
| `ComponentGet` | `request_id`, `entity_id`, `component_type` |
| `ComponentQuery` | `request_id`, `component_types` |
| `RegisterCommand` | `name`, `description`, `subcommands`, `args`, `has_handler` |
| `CommandResponse` | `request_id`, `output`, `error` |
| `ArgSuggestResponse` | `request_id`, `suggestions` |
