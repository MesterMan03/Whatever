# @whatever-engine/api

TypeScript scripting API for Whatever Engine mods. Abstracts the NDJSON IPC protocol between the engine and Bun script subprocesses.

## Usage

```ts
import { Engine, Window, File, Scene, Entity, Mods, Message, Console, Audio } from "@whatever-engine/api";

Engine.on("init", ({ mod_id }) => {
  Engine.log("info", `loaded as ${mod_id}`);
});

Engine.on("tick", async ({ delta_seconds }) => {
  // game logic — engine waits for this Promise before advancing
});
```

No install step needed — Bun resolves the package automatically for any script running within the engine directory tree.

## API

### `Engine`

- `Engine.on(event, handler)` — subscribe to an engine event (`init`, `exit`, `tick`, `mod_message`)
- `Engine.log(level, message)` — log through the engine logger (`"info"`, `"warn"`, `"error"`)
- `Engine.setTickRate(ticks_per_second)` — override the game tick rate at runtime
- `Engine.setFpsCap(fps | null)` — set FPS cap; `null` = uncapped
- `Engine.setVsync(enabled)` — enable or disable vertical sync
- `Engine.setMainCamera(entity_id)` — designate an entity as the active camera (needs `core:camera` + `core:transform`)
- `Engine.clearCamera()` — remove the active camera (screen goes black with a warning)

### `Window`

- `Window.setTitle(title)` — change the window title
- `Window.setSize(width, height)` — request a new inner size in physical pixels (no-op in fullscreen modes)
- `Window.setMode(mode)` — set display mode: `"windowed"`, `"borderless"`, or `"fullscreen"`

### `File`

Sandboxed per-mod file I/O. Paths must not contain `..`.

- `File.write(path, data)` → `Promise<void>`
- `File.read(path)` → `Promise<string>`
- `File.delete(path)` → `Promise<void>`

### `Entity`

A live entity in the scene. Returned by `Scene.createEntity`, `Scene.listEntities`, `Scene.query`, and `Scene.spawnSprite`. For built-in component types the data parameter/return is automatically typed; for custom component types a generic `T` can be supplied.

- `entity.id` — opaque entity ID string
- `entity.destroy()` — fire-and-forget
- `entity.setComponent(component_type, data)` / `entity.setComponent(component)` — fire-and-forget; typed for built-in components; single-argument form reads `id` from the component object
- `entity.removeComponent(component_type)` — fire-and-forget
- `entity.getComponent(component_type)` → `Promise<T | null>` — typed for built-in components
- `entity.move(position)` → `Promise<void>` — update `core:transform` position, preserve rotation/scale
- `entity.setParent(parent | null)` — attach to a parent entity or detach; fire-and-forget; transform becomes local-space relative to parent
- `entity.getParent()` → `Promise<Entity | null>` — return the parent entity
- `entity.getChildren()` → `Promise<Entity[]>` — return all direct children

### `Scene`

Entity and component management. Methods that accept a raw `entity_id` string are provided for cases where only an ID is available; prefer `Entity` methods when possible.

- `Scene.createEntity()` → `Promise<Entity>`
- `Scene.destroyEntity(entity_id)` — fire-and-forget
- `Scene.listEntities()` → `Promise<Entity[]>`
- `Scene.setComponent(entity_id, component_type, data)` — fire-and-forget; typed for built-in components
- `Scene.removeComponent(entity_id, component_type)` — fire-and-forget
- `Scene.getComponent(entity_id, component_type)` → `Promise<T | null>` — typed for built-in components
- `Scene.query(component_types)` → `Promise<QueryResult[]>` — each row has `entity: Entity` and `components`
- `Scene.spawnSprite(texture, position, scale?)` → `Promise<Entity>` — convenience
- `Scene.spawnMesh(mesh, shader, position, options?)` → `Promise<Entity>` — convenience
- `Scene.spawnText(text, position, options?)` → `Promise<Entity>` — convenience
- `Scene.moveEntity(entity_id, position)` → `Promise<void>` — convenience
- `Scene.addAmbientLight(color?, intensity?)` → `Promise<Entity>` — convenience
- `Scene.addDirectionalLight(direction, color?, intensity?)` → `Promise<Entity>` — convenience
- `Scene.addPointLight(position, color?, intensity?, range?)` → `Promise<Entity>` — convenience
- `Scene.setParent(entity_id, parent_id | null)` — fire-and-forget
- `Scene.getParent(entity_id)` → `Promise<string | null>`
- `Scene.getChildren(entity_id)` → `Promise<string[]>`

Built-in component types: `core:transform`, `core:sprite_renderer`, `core:mesh_renderer`, `core:text_renderer`, `core:camera`, `core:ambient_light`, `core:directional_light`, `core:point_light`. `getComponent` for built-in types returns a **live class instance** with methods — not a plain object.

### `BuiltInComponents.Transform`

Methods (all setters chainable, return `this`):

- `getX/Y/Z()`, `setX/Y/Z(v)`, `addX/Y/Z(v)` — individual position components
- `getPosition()` → `[x, y, z]`, `setPosition(x, y, z)` — full position
- `getScaleX/Y/Z()`, `setScaleX/Y/Z(v)` — individual scale components
- `getScale()` → `[x, y, z]`, `setScale(x, y, z)`, `setScaleUniform(s)` — full scale
- `getRotation()` → `[x, y, z, w]`, `setRotation(x, y, z, w)` — raw quaternion
- `getEulerRadians/Degrees()` → `[rx, ry, rz]` — intrinsic XYZ decomposition
- `setEulerRadians/Degrees(rx, ry, rz)` — set from intrinsic XYZ Euler angles
- `rotateX/Y/Z(degrees)` — incremental world-space rotation around axis
- `distance(other)` → `number` — Euclidean distance between positions

### `BuiltInComponents.SpriteRenderer`

- `getTexture()`, `setTexture(path)` — VFS texture path
- `getShader()`, `setShader(path)` — VFS shader path (default: `"core://shaders/sprite.wgsl"`; see `docs/SHADER.md`)

### `BuiltInComponents.MeshRenderer`

- `getMesh()`, `setMesh(path)` — VFS mesh path (`.json`, `.obj`, `.glb`, `.gltf`)
- `getShader()`, `setShader(path)` — VFS shader path (default: `"core://shaders/mesh_lit.wgsl"`; see `docs/SHADER.md`)
- `getTexture()`, `setTexture(path | null)` — VFS texture path; `null` = 1×1 white fallback

### `BuiltInComponents.TextRenderer`

- `getText()`, `setText(text)` — displayed string
- `getFont()`, `setFont(path)` — VFS path to TTF/OTF font (default: `"core://fonts/default.ttf"`)
- `getFontSize()`, `setFontSize(size)` — font size in logical pixels (default: `24`)
- `getColor()`, `setColor(r, g, b, a)` — RGBA colour, each channel `[0.0, 1.0]` (default: white)

### `BuiltInComponents.Camera`

Makes an entity usable as a scene camera. Requires `core:transform` on the same entity. Activate with `Engine.setMainCamera(entity.id)`.

- `getFov()`, `setFov(degrees)` — vertical FOV in degrees (default: `45`)
- `getZNear()`, `setZNear(v)` — near clip plane (default: `0.1`)
- `getZFar()`, `setZFar(v)` — far clip plane (default: `1000`)

### `BuiltInComponents.AmbientLight`

Uniform fill light. Multiple ambient lights are additive. No position needed.

- `getColor()`, `setColor(r, g, b)` — RGB colour, channels in `[0.0, 1.0]` (default: white)
- `getIntensity()`, `setIntensity(v)` — multiplier (default: `0.1`)

### `BuiltInComponents.DirectionalLight`

Infinitely distant light (sun-like). Max 4 active simultaneously.

- `getDirection()`, `setDirection(x, y, z)` — normalised world-space direction (default: `[0, -1, 0]`)
- `getColor()`, `setColor(r, g, b)` — RGB colour (default: white)
- `getIntensity()`, `setIntensity(v)` — multiplier (default: `1.0`)

### `BuiltInComponents.PointLight`

Omnidirectional point light. Position comes from `core:transform`. Max 8 active simultaneously.

- `getColor()`, `setColor(r, g, b)` — RGB colour (default: white)
- `getIntensity()`, `setIntensity(v)` — multiplier (default: `1.0`)
- `getRange()`, `setRange(v)` — attenuation radius in world units (default: `10`)

### `Audio`

Play audio files loaded from the VFS. Supported formats: WAV, OGG/Vorbis, MP3.

- `Audio.play({ path, seek?, speed?, volume? })` — fire-and-forget; engine frees the handle automatically when playback ends
- `Audio.load({ path, play?, closeStrategy?, volume?, speed?, loop? })` → `Promise<AudioHandle>` — load and optionally start playback; returns a controllable handle

`AudioHandle` methods:
- `handle.isStopped()` — `boolean` (sync): `true` after `stop()` or engine-initiated close
- `handle.play(opts?)` → `Promise<number>` — resume playback; returns position in ms; `opts` can override `volume` and `speed`
- `handle.pause()` → `Promise<number>` — pause; returns position in ms
- `handle.stop()` — void (sync): stop and free the handle; fires `close` handlers immediately
- `handle.position()` → `Promise<number>` — current position in ms
- `handle.isPlaying()` → `Promise<boolean>`
- `handle.volume()` → `Promise<number>`
- `handle.speed()` → `Promise<number>`
- `handle.isLooping()` → `Promise<boolean>`
- `handle.metadata()` → `Promise<AudioMetadata>` — `{ duration_ms, sample_rate, channels }`; cached after first call
- `handle.seekTo(ms)` → `Promise<number>` — seek to absolute position; returns previous position
- `handle.seek(offsetMs)` → `Promise<number>` — seek forward/backward; returns new position; clamped to `[0, duration-1]`, pauses if past end
- `handle.loop(enabled)` — void (sync): enable or disable looping; resets position to 0 when changed
- `handle.on("close", fn)` — fired on both manual `stop()` and engine-initiated close

`CloseStrategy`: `"Auto"` (default) — engine frees handle when playback ends; `"Manual"` — mod must call `stop()`.

### `Mods`

- `Mods.list()` → `Promise<ModManifest[]>` — all loaded mods in load order
- `Mods.get(id)` → `Promise<ModManifest>` — manifest for a specific mod

### `Message`

Inter-mod communication. Payloads must be JSON-serializable.

- `Message.sendAndForget(id, message)` — fire-and-forget
- `Message.send(id, message, timeout)` → `Promise<JsonValue>` — send and await a reply
- `Message.registerMessageHandler(handler)` — handle incoming messages; return a value to reply

### `Console`

- `Console.register(spec)` — register developer console commands with typed args and subcommands

## Development

```sh
# Regenerate type declarations after editing index.ts
bun run build:types
```

Full API documentation is in [`docs/SCRIPTING.md`](../docs/SCRIPTING.md) at the repo root.
