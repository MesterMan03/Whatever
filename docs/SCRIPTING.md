# Scripting API

Mods with a `[script]` section in `mod.toml` run as a Bun subprocess. The engine
communicates with each script over NDJSON on stdin/stdout. The `@whatever-engine/api`
package abstracts the wire protocol.

```ts
import { Engine, Window, File, Scene, Mods, Message, Console, Audio } from "@whatever-engine/api";
```

The engine-owned mod host embeds the API and registers `@whatever-engine/api` before
loading mod code. It resolves independently of the mod's directory or node_modules.
Leave this import external in mod bundles; install the npm package only if needed
for editor types. Other packages and all Node/Bun built-in modules are denied by
default. Local JS/TS/JSON modules must resolve inside the current mod directory.
See [the runtime policy](SANDBOX.md) for the module and global allowlists.

---

## `Engine`

### `Engine.mode`

Read-only `"client" | "server"`, available during top-level mod evaluation as well as event handlers. The host sets it before importing the mod. `init` includes the same `mode` alongside `mod_id` and `engine_version`.

Simulation ticks run on a separate engine worker. Tick handlers are awaited without blocking the client renderer. The server runs this worker without a window, GPU or audio output device. FPS and vsync affect presentation only. Invalid tick rates (nonpositive, nonfinite or unrepresentable) are logged and ignored.

Window and audio methods, `Engine.setFpsCap`, `setVsync`, `setMainCamera` and `clearCamera` reject use in server mode. ECS component data and entity operations remain available there, including components a client may later render.

### `Engine.on(event, handler)`

Subscribe to an engine event. Registering a handler also sends a `Subscribe`
message to the engine automatically (except `mod_message`, which is routed
unconditionally). For `tick`, all async handlers are awaited before the engine
advances the simulation.

```ts
Engine.on("init", ({ mod_id, engine_version, mode }) => {
  Engine.log("info", `loaded as ${mod_id}, engine v${engine_version}`);
});

Engine.on("exit", ({ exit_code }) => {
  Engine.log("info", `shutting down (code ${exit_code})`);
});

Engine.on("tick", async ({ tick_number, delta_seconds, keys_pressed }) => {
  // game logic here — engine waits for this Promise to resolve
});
```

The host sends an internal `RuntimeReady` message after mod evaluation completes.
The engine sends `Init` after this handshake and runtime readiness (including window initialization on clients), so top-level event registration is in place before `init` fires.

### Events

| Event | Payload | Description |
|---|---|---|
| `init` | `{ mod_id, engine_version, mode }` | Fired once after engine initialization; server has no window |
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
pre-attached. The `shader` field defaults to `"core://shaders/sprite.wgsl"`;
use `entity.setComponent(new BuiltInComponents.SpriteRenderer({ ..., shader: "..." }))` to change it after spawning.

```ts
const entity = await Scene.spawnSprite(
  "my_mod://textures/player.png",
  [0, 0, 0],
  [1, 1, 1],   // optional, defaults to [1, 1, 1]
);
```

### `Scene.spawnMesh(mesh, shader, position, options?)` → `Promise<Entity>`

Convenience: create an entity with `core:transform` and `core:mesh_renderer` pre-attached.

```ts
const entity = await Scene.spawnMesh(
  "my_mod://meshes/triangle.glb",       // mesh path
  "my_mod://shaders/tint.wgsl",     // shader path
  [0, 0, -5],                       // world position
  {
    texture: "my_mod://textures/crate.png",  // optional
    scale:   [2, 2, 2],                      // optional, defaults to [1, 1, 1]
  },
);
```

### `Scene.moveEntity(entity_id, position)` → `Promise<void>`

Convenience: update the `position` field of an entity's `core:transform` while
preserving its rotation and scale.

```ts
await Scene.moveEntity(id, [x, y, z]);
```

### `Scene.addAmbientLight(color?, intensity?)` → `Promise<Entity>`

Convenience: create an entity and attach `core:ambient_light`. Multiple ambient lights are additive.

```ts
await Scene.addAmbientLight([1, 0.9, 0.7], 0.15);
```

### `Scene.addDirectionalLight(direction, color?, intensity?)` → `Promise<Entity>`

Convenience: create an entity and attach `core:directional_light`.

```ts
await Scene.addDirectionalLight([0, -1, -0.3], [1, 1, 1], 1.0);
```

### `Scene.addPointLight(position, color?, intensity?, range?)` → `Promise<Entity>`

Convenience: create an entity with `core:transform` + `core:point_light`. The light is emitted from `position` in all directions.

```ts
await Scene.addPointLight([0, 3, 0], [1, 0.5, 0.1], 2.0, 15);
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

| Field | Type | Default | Description |
|---|---|---|---|
| `texture` | `string` | — | VFS path `mod_id://path/to/texture` |
| `shader` | `string` | `"core://shaders/sprite.wgsl"` | VFS path to a WGSL shader; see `docs/SHADER.md` |

| Method | Returns | Description |
|---|---|---|
| `getTexture()` | `string` | Current texture path |
| `setTexture(path)` | `this` | Change the texture |
| `getShader()` | `string` | Current shader VFS path |
| `setShader(path)` | `this` | Change the shader |

Sprites are rendered on the XZ plane; the Y axis is up. A sprite becomes visible as soon as the entity has **both** `core:transform` and `core:sprite_renderer` set.

#### `BuiltInComponents.MeshRenderer`

Returned by `getComponent("core:mesh_renderer")`. Renders arbitrary triangle geometry from a mesh file.

| Field | Type | Default | Description |
|---|---|---|---|
| `mesh` | `string` | — | VFS path to mesh file (`.json`, `.obj`, `.glb`, `.gltf`) |
| `shader` | `string` | `"core://shaders/mesh_lit.wgsl"` | VFS path to a WGSL shader |
| `texture` | `string \| null` | `null` | VFS path to texture; `null` uses a 1×1 white fallback |

| Method | Returns | Description |
|---|---|---|
| `getMesh()` | `string` | Current mesh VFS path |
| `setMesh(path)` | `this` | Change the mesh |
| `getShader()` | `string` | Current shader VFS path |
| `setShader(path)` | `this` | Change the shader |
| `getTexture()` | `string \| null` | Current texture path or `null` |
| `setTexture(path)` | `this` | Change the texture |

A mesh entity becomes visible as soon as it has **both** `core:transform` and `core:mesh_renderer` set. The engine applies the transform (position, rotation, scale) to the mesh vertices on the CPU at upload time. See `docs/SHADER.md` for the supported mesh formats and shader contract.

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

#### `BuiltInComponents.AmbientLight`

Returned by `getComponent("core:ambient_light")`. Contributes a uniform fill light to the scene. Multiple ambient lights are **additive**.

| Field | Type | Default | Description |
|---|---|---|---|
| `color` | `[r, g, b]` | `[1, 1, 1]` | RGB colour, each channel in `[0.0, 1.0]` |
| `intensity` | `number` | `0.1` | Multiplier for the colour |

| Method | Returns | Description |
|---|---|---|
| `getColor()` | `[r, g, b]` | Copy of the colour tuple |
| `setColor(r, g, b)` | `this` | Change the colour |
| `getIntensity()` | `number` | Current intensity |
| `setIntensity(v)` | `this` | Change the intensity |

```ts
// Soft warm ambient fill
await Scene.addAmbientLight([1, 0.9, 0.7], 0.15);
```

#### `BuiltInComponents.DirectionalLight`

Returned by `getComponent("core:directional_light")`. Simulates an infinitely distant light source (e.g. the sun). Up to **4** directional lights are active simultaneously; extras are silently ignored.

| Field | Type | Default | Description |
|---|---|---|---|
| `direction` | `[x, y, z]` | `[0, -1, 0]` | Normalised direction the light travels **towards** (world space) |
| `color` | `[r, g, b]` | `[1, 1, 1]` | RGB colour, each channel in `[0.0, 1.0]` |
| `intensity` | `number` | `1.0` | Multiplier for the colour |

| Method | Returns | Description |
|---|---|---|
| `getDirection()` | `[x, y, z]` | Copy of the direction tuple |
| `setDirection(x, y, z)` | `this` | Change the direction |
| `getColor()` | `[r, g, b]` | Copy of the colour tuple |
| `setColor(r, g, b)` | `this` | Change the colour |
| `getIntensity()` | `number` | Current intensity |
| `setIntensity(v)` | `this` | Change the intensity |

```ts
// Sun shining down and slightly forward
await Scene.addDirectionalLight([0, -1, -0.3], [1, 0.95, 0.8], 1.2);
```

#### `BuiltInComponents.PointLight`

Returned by `getComponent("core:point_light")`. Emits light in all directions from the entity's `core:transform` position. Attenuation is quadratic — falls off to zero at `range` units. Up to **8** point lights are active simultaneously; extras are silently ignored.

| Field | Type | Default | Description |
|---|---|---|---|
| `color` | `[r, g, b]` | `[1, 1, 1]` | RGB colour, each channel in `[0.0, 1.0]` |
| `intensity` | `number` | `1.0` | Multiplier for the colour |
| `range` | `number` | `10` | Distance (world units) at which the light reaches zero |

| Method | Returns | Description |
|---|---|---|
| `getColor()` | `[r, g, b]` | Copy of the colour tuple |
| `setColor(r, g, b)` | `this` | Change the colour |
| `getIntensity()` | `number` | Current intensity |
| `setIntensity(v)` | `this` | Change the intensity |
| `getRange()` | `number` | Current range |
| `setRange(v)` | `this` | Change the range |

Requires a `core:transform` on the same entity to provide the position.

```ts
// Orange glow above the origin
await Scene.addPointLight([0, 3, 0], [1, 0.5, 0.1], 2.0, 15);
```

---

## `Audio`

Play audio files loaded from the VFS. Supported formats: WAV, OGG/Vorbis, MP3.

```ts
import { Audio, CloseStrategy } from "@whatever-engine/api";
import type { AudioHandle, AudioMetadata } from "@whatever-engine/api";
```

### `Audio.play(opts)`

Fire-and-forget playback. The engine loads the file and plays it once; the handle is freed automatically when done. No `AudioHandle` is returned.

```ts
Audio.play({ path: "mymod://sounds/explosion.wav" });
Audio.play({ path: "mymod://music/theme.ogg", volume: 0.5, speed: 1.2 });
```

| Option | Type | Default | Description |
|---|---|---|---|
| `path` | `string` | — | VFS path to the audio file |
| `volume` | `number` | `1.0` | Playback volume (`0.0`–`1.0+`) |
| `speed` | `number` | `1.0` | Playback speed multiplier |

### `Audio.load(opts)` → `Promise<AudioHandle>`

Load an audio file and return a controllable `AudioHandle`. Rejects if the path is invalid or the file cannot be decoded.

```ts
const handle = await Audio.load({ path: "mymod://music/loop.ogg", loop: true, play: true, closeStrategy: "Manual" });
```

| Option | Type | Default | Description |
|---|---|---|---|
| `path` | `string` | — | VFS path to the audio file |
| `play` | `boolean` | `false` | Start playback immediately |
| `volume` | `number` | `1.0` | Initial volume |
| `speed` | `number` | `1.0` | Initial speed |
| `loop` | `boolean` | `false` | Loop playback |
| `closeStrategy` | `CloseStrategy` | `"Auto"` | When to free the handle (see below) |

**`CloseStrategy`**

- `"Auto"` — engine frees the handle and fires the `close` event when playback naturally ends.
- `"Manual"` — mod must call `handle.stop()` to free the resource; useful for sound effects replayed multiple times without reloading.

---

### `AudioHandle`

#### `handle.isStopped()` → `boolean` (sync)

Returns `true` immediately after `stop()` is called, or after the engine fires a `close` event. Safe to call at any time. Use as a guard before any async method:

```ts
if (!handle.isStopped()) {
  await handle.pause();
}
```

#### `handle.play(opts?)` → `Promise<number>`

Resume (or start) playback. Returns the position in ms at the time playback resumed. If already playing, updates `volume`/`speed` and returns the current position.

```ts
const pos = await handle.play({ volume: 0.9, speed: 1.5 });
```

#### `handle.pause()` → `Promise<number>`

Pause playback. Returns the position in ms at the time of pausing.

#### `handle.stop()` → `void` (sync)

Stop playback and free the handle. Fires all `close` handlers immediately. Calling any async method after `stop()` throws.

#### `handle.position()` → `Promise<number>`

Current playback position in ms.

#### `handle.isPlaying()` → `Promise<boolean>`

#### `handle.volume()` → `Promise<number>`

#### `handle.speed()` → `Promise<number>`

#### `handle.isLooping()` → `Promise<boolean>`

#### `handle.metadata()` → `Promise<AudioMetadata>`

Returns `{ duration_ms: number | null, sample_rate: number, channels: number }`. The result is cached locally after the first call; subsequent calls return immediately.

#### `handle.seekTo(ms)` → `Promise<number>`

Jump to an absolute position in ms. Returns the **previous** position.

#### `handle.seek(offsetMs)` → `Promise<number>`

Move forward (positive) or backward (negative) by `offsetMs` ms. Returns the **new** position. Clamped to `[0, duration-1]`; seeking past the end pauses at the last ms without closing the handle.

#### `handle.loop(enabled)` → `void` (sync)

Enable or disable looping. Changing this resets the playback position to 0.

#### `handle.on("close", fn)` → `void`

Register a callback fired when the handle closes — either from `stop()` or when the engine auto-closes it (playback ended + `closeStrategy: "Auto"`).

```ts
const sfx = await Audio.load({ path: "mymod://sounds/hit.wav", closeStrategy: "Manual" });
sfx.on("close", () => console.log("sfx freed"));

await sfx.play();
// later...
sfx.stop();
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
| `type` | `"string" \| "int" \| "float" \| "bool" \| "mod_id" \| "entity_id"` | Parsed and validated argument type |
| `required` | `boolean?` | Defaults to `false` |
| `description` | `string?` | Shown in `help <command>` |
| `suggest` | `(current: string) => string[] \| Promise<string[]>` | Optional. Called by the engine to provide autocomplete suggestions as the user types this argument. Receives the current raw text (empty string if nothing typed yet). Return a list of candidate strings; the engine sends the request via IPC and updates the completion dropdown when the promise resolves. Ignored for `mod_id` and `entity_id`, which are completed by the engine. |

Engine-provided argument types work for both built-in and mod commands:

| Type | Completion and validation | Handler value |
|---|---|---|
| `mod_id` | Prefix matches from the mods loaded on the current runtime side; rejects IDs that are not loaded | `string` |
| `entity_id` | Prefix matches from living entities in the current world snapshot; rejects malformed, dead or stale-generation IDs | `string` in `index:generation` format |
| `bool` | Suggests `false` and `true` when no custom suggester is provided; also accepts `1/0` and `yes/no` | `boolean` |

```ts
Console.register({
  name: "target_mod",
  args: [{ name: "target", type: "mod_id", required: true }],
  handler: ({ target }) => `Selected loaded mod: ${target}`,
});
```

The engine validates arguments before invoking either an engine handler or a mod handler. Invalid IDs produce a console error; no command invocation is sent to the mod. Engine-provided suggestions use current engine state without a mod-side callback or suggestion IPC. Entities can still change after validation, so an entity command should handle an entity disappearing before an asynchronous operation finishes.

The console opens with **F2**. Suggestions are alphabetically sorted, and Up/Down cycles through the entire list. The selected suffix appears as dim preview text; Tab or clicking accepts it, while Enter runs the entered text. Help renders signatures and descriptions on separate, wrapping lines.

The handler receives a `Record<string, string | number | boolean>` where keys are
the `name` fields from the `ArgSpec` list. Missing optional args are absent from
the record (check with `arg in args` or `args.arg ?? default`).

Handlers may return a plain string, a string array (multiple output lines), or a
`Promise` resolving to either. The console shows a "waiting" indicator while a
promise is pending.

If a command name conflicts with an already-registered command, the engine will
register it as `mod_id:name` instead and log a warning.

---

## `Network`

The engine owns QUIC transport, TLS, schema validation and binary codecs. Registration validates a specification without opening sockets; explicit `listen` or `connect` starts transport. Mod code uses the bundled API broker and receives no raw sockets or direct network globals.

```ts
const Hello = Network.Packet({name: Network.Type.String.with({maxLength: 64})});
const Game = Network.Specification("my_mod:game", {
  login: {
    serverbound: {hello: Hello},
    clientbound: {welcome: Network.Packet({playerId: Network.Type.I32})},
  },
});
type HelloData = Network.Infer<typeof Hello>;
const client = await Network.registerClient(Game); // client entry
const bytes = await client.encodePacket("login.hello", {name: "Alex"});
const decoded = await client.decodePacket(bytes); // discriminated union: packet, direction, state, data
```

Definitions are immutable snapshots. Structs and packets use ordered, named fields; arrays accept any descriptor, including structs and optional values. States/packet names are ASCII identifiers, without dots; unsafe property names are rejected. A specification ID must use the registering mod's namespace. Define shared schemas in a local module imported by both side entries, as in `mods/network_example/scripts/network.ts`.

| Descriptor | JavaScript value | Wire representation |
|---|---|---|
| `Type.I32` | integer number, signed 32-bit | Four bytes, big endian |
| `Type.F64` | finite number | Eight IEEE-754 bytes, big endian |
| `Type.VarInt` | integer number, signed 32-bit | ZigZag followed by ULEB128 |
| `Type.Bool` | boolean | One byte; decoding treats nonzero as true |
| `Type.String` | string | Unsigned ULEB128 UTF-8 byte length, then UTF-8 |
| `Type.Array(type, {maxLength?})` | array | Unsigned ULEB128 element count, then elements |
| `Type.Optional(type)` | value or undefined | Presence byte, then value when present |
| `Network.Struct(fields)` | object | Field values in declared order |

Optional object fields can be omitted. Undefined is represented as null only inside private JSON IPC; it is restored to undefined by the SDK. Public values do not accept null as a substitute. Nested optionals, sparse arrays, unknown object fields, out-of-range numbers and malformed UTF-8 are rejected. Numeric `.with({min?, max?})` bounds affect validation and specification metadata, with no extra wire bytes. String `.with({maxLength})` bounds count UTF-8 bytes.

Native limits: 1 MiB per encoded packet including its two-byte ID, up to 65,536 array elements, 32 schema nesting levels, 100,000 schema/value nodes and 1,024 states. A protocol may contain 65,536 packets. IDs increment across states and both directions, in state declaration order, serverbound then clientbound within each state. Mod load order has no effect. States group packets as metadata; no connection state machine is imposed.

Registration checks the runtime side, ownership, names, schema constraints and server defaults before returning an engine handle. Conflicting/repeated registrations fail; `close()` releases the registration, and mod process exit removes its registrations. The fingerprint is SHA-256 of the ordered canonical specification; native conflict checks also compare the full manifest, and QUIC negotiation checks the digest before delivering mod events.

Both registration handles expose `id`, `specificationId`, `fingerprint`, readonly `packets` (ID/path/state/direction), `decodePacket(bytes)` and `close()`. Client `encodePacket/send/on` and server equivalents infer names/payloads from direction. A client sends serverbound and receives clientbound packets; a server does the reverse. `decodePacket` is an offline inspection method for either direction and does not dispatch receive handlers. Its bytes contain `[u16 packet ID][payload]`, independent of stream framing. The fingerprint is SHA-256 of the ordered canonical specification; client-data definitions are excluded.

Server registration also declares per-client data:

```ts
const ClientData = Network.Struct({
  playerId: Network.Type.Optional(Network.Type.I32),
  tags: Network.Type.Array(Network.Type.String),
});
const server = await Network.registerServer(Game, {
  clientData: ClientData,
  initialClientData: {tags: []},
});
server.on("login.hello", async (packet, client) => {
  client.data.playerId = 42;
  client.data.tags.push(packet.name);
  await client.flushData();
  await server.send(client, "login.welcome", {playerId: client.data.playerId});
});
```

Required client-data fields need initial values. Native connection allocation copies defaults independently for each peer; data remains server/mod-local and is excluded from the wire fingerprint. The SDK proxy validates assignments, nested edits and array mutations before submitting ordered snapshots to native storage. `flushData()` awaits outstanding writes and reports rejected updates. Disconnect makes the data view read-only for cleanup; reconnect gets fresh defaults. `server.validateClientData(value)` validates and copies data offline without inventing a connected peer.

`on(packet, handler)` returns unsubscribe; client handlers receive data, server handlers receive data and `Network.Client<D>`. `server.onConnect/onDisconnect` likewise return unsubscribe. `server.clients` and `getClient(id)` return typed handles. Server `send(client, packet, data)` requires a client belonging to that registration. `broadcast(packet, data)` is explicit. Callback failures and asynchronous data-update failures are logged. Server handlers additionally receive a `Network.PacketContext` as their third argument; client packet handlers receive it as their second argument. Its `transport` is `"stream"` or `"datagram"`.

### QUIC connections and delivery

Install handlers before starting transport:

```typescript
// Server entry; both sides have already registered the shared Game specification.
server.onConnect(async client => {
  // Defaults are independent per connection. These callbacks gate incoming packets.
  client.data.playerId = nextPlayerId++;
  await client.flushData();
});
const listener = await server.listen({address: "0.0.0.0:4444", maxClients: 128});

// Client entry. Strict certificate validation is the API default.
// This local development example explicitly enables TOFU fallback:
await client.connect({address: "localhost:4444", trust: "tofu"});
await client.send("login.hello", {name: "Alex"});
```

`listen` requires a numeric IP and port; port zero chooses a free port. It returns `Network.ListenerInfo {address}`; `server.listener` and `server.listening` expose listener state. `maxClients` defaults to 128 and accepts 1–65536, counting pending TLS handshakes as well as connected clients. `server.stop()` stops the listener and disconnects peers while preserving registration. `server.disconnect(client, reason?)` disconnects one owned client.

`connect` accepts `host:port` or `[IPv6]:port`, resolves DNS in native code, and completes after TLS and specification negotiation. It returns `Network.ConnectionInfo {address, maxDatagramSize}`; the datagram limit is the initial negotiated limit and is checked again against the current limit on every send. `client.connected` and `client.connection` expose state. `client.disconnect()` permits reconnecting the same registration. Both sides provide `onConnect` and `onDisconnect` subscriptions; the client connect callback receives connection info, the client disconnect callback a reason, and the server disconnect callback receives the peer and reason. Reconnection creates fresh native peer handles/defaults. Old handles fail sends and writes.

`send` and `broadcast` accept optional `Network.SendOptions {transport?: "stream" | "datagram"}`. The default is one reliable, ordered application stream per connection; independent application streams are not yet exposed. Stream frames are `[unsigned ULEB128 length][u16 big-endian packet ID][payload]`, where length counts ID and payload and excludes its own prefix. Datagram frames are just `[u16 packet ID][payload]`. Datagrams are unordered, may be lost, and must fit the current QUIC limit; oversized datagrams fail without automatic fragmentation or fallback. Reliable frames are capped at 1 MiB including the ID. Strings/arrays use unsigned ULEB128 lengths; standalone VarInt remains ZigZag + ULEB128.

Each connection binds one specification. A reserved control stream exchanges `WHATEVER/1\0` plus its 32-byte SHA-256 digest before mod events are emitted. The server opens one application stream with a one-time one-byte kind marker with value `0`; subsequent bytes are the frames above. Protocol states remain metadata, with IDs unique across every state and direction within the specification. Unknown IDs, wrong directions, invalid payloads, noncanonical/oversized framing, and schema mismatches terminate the connection.

Successful sends mean validation and acceptance into a bounded local queue; they do not acknowledge remote application delivery. Each peer has 64 outgoing packet slots and a 4 MiB byte budget. Broadcast reserves capacity for every current peer before enqueueing, or fails without partial enqueue. The service uses bounded command/event queues. Connect callbacks finish before incoming packets are dispatched, and packet callbacks are acknowledged/serialized per peer; other peers continue independently. Awaiting a reply to an incoming packet inside its own handler would hold that peer's receive queue: send the request and return, then handle the reply separately. Callback errors are logged; failed connect callbacks close the peer. Disconnect/close is immediate and does not flush queued messages. Closing registrations, stopping the engine or reaping a mod closes its network resources.

### Certificates and TOFU

`trust: "strict"` validates the certificate chain, validity and server name against native OS roots. `trust: "tofu"` first attempts the same validation. If this is a new untrusted server, it accepts and remembers the server's SHA-256 public-key identity after TLS proves key possession and the specification handshake succeeds. Future connections must match that key. Servers previously CA-validated while using TOFU policy continue requiring valid CA authentication; they never silently downgrade to TOFU. Certificate renewal with the same pinned key works. The first TOFU connection has no independent protection against server impersonation.

The native engine stores server identities and trust records under the OS local-data directory `WhateverNetwork/<game_id>`, outside every mod's broker-accessible directory. Mods cannot choose certificate/key paths or rewrite the trust database. Development servers generate a persistent self-signed identity automatically. Operators can provide `--tls-cert <chain.pem> --tls-key <key.pem>` and optional client `--tls-ca <authority.pem>`. `--network-data <directory>` selects a separate native identity/trust directory. Trust entries are scoped to the normalized server host and port, independent of specification changes. An unexpected key change fails the connection; explicit trust-management UI is a later addition. No certificate fingerprint is required in the ordinary client API or commands.

## IPC protocol reference

This section documents the raw wire format. Normal mod code should use the API
namespaces above rather than speaking the protocol directly.

Network IPC adds `NetworkRequest { request_id, request }` and `NetworkResponse { request_id, result, error }`. Requests cover Register, Encode, Decode, ValidateClientData, UpdateClientData, Listen, Connect, Stop, Disconnect, Send, Broadcast, EventDone and Close; byte buffers use base64 inside this private bridge. The native engine owns codec execution. Host protocol is 4. Native `NetworkEvent` messages carry connected/disconnected/packet events, registration and peer IDs, and bounded per-peer callback acknowledgement IDs.

**Transport:** NDJSON (one JSON object per line) on stdin/stdout per process.
Stderr lines are forwarded to the engine logger with a `[mod_id]` prefix. Engine-to-mod pipe writes run on a dedicated bounded writer thread (256 queued messages, 64 MiB including the in-flight write). A full/broken writer terminates the affected mod rather than blocking the simulation or silently losing RPCs.

### Engine → Script

| Message | Key fields |
|---|---|
| `NetworkResponse` | `request_id`, `result`, `error` |
| `NetworkEvent` | `event`, `registration_id`, `client_id`, `event_id` when acknowledged, event-specific data |
| `Init` | `mod_id`, `engine_version`, `mode` |
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
| `NetworkRequest` | `request_id`, `request` (operation DTO) |
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

## Mod sides and script entries

The `[mod].sides` list defaults to both `client` and `server`. Inactive mods are removed before dependency validation. The `[script]` section may contain optional `entry`, `client` and `server` paths: a side entry overrides `entry`; without either, that mod has no script on that side. `Mods.list()` and `Mods.get()` expose only active mods, with `sides: RuntimeMode[]` and nullable script entry paths. `RuntimeMode` is exported as a public type.


A synchronous throw or rejected promise from a tick handler is logged and terminates that mod process. The runtime removes exited subscribers from the current tick barrier so other mods can continue. Shutdown interrupts a waiting tick and cleans up remaining subprocesses.
