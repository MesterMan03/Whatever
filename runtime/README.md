# @whatever-engine/api

TypeScript scripting API for Whatever Engine mods. Abstracts the NDJSON IPC protocol between the engine and Bun script subprocesses.

## Usage

```ts
import { Engine, Window, File, Scene, Entity, Mods, Message, Console } from "@whatever-engine/api";

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
- `Scene.spawnText(text, position, options?)` → `Promise<Entity>` — convenience
- `Scene.moveEntity(entity_id, position)` → `Promise<void>` — convenience
- `Scene.setParent(entity_id, parent_id | null)` — fire-and-forget
- `Scene.getParent(entity_id)` → `Promise<string | null>`
- `Scene.getChildren(entity_id)` → `Promise<string[]>`

Built-in component types: `core:transform`, `core:sprite_renderer`, `core:text_renderer`. `getComponent` for built-in types returns a **live class instance** with methods — not a plain object.

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

### `BuiltInComponents.TextRenderer`

- `getText()`, `setText(text)` — displayed string
- `getFont()`, `setFont(path)` — VFS path to TTF/OTF font (default: `"core://fonts/default.ttf"`)
- `getFontSize()`, `setFontSize(size)` — font size in logical pixels (default: `24`)
- `getColor()`, `setColor(r, g, b, a)` — RGBA colour, each channel `[0.0, 1.0]` (default: white)

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

Full API documentation is in [`docs/scripting-api.md`](../docs/scripting-api.md) at the repo root.
