# @whatever/api

TypeScript scripting API for Whatever Engine mods. Abstracts the NDJSON IPC protocol between the engine and Bun script subprocesses.

## Usage

```ts
import { Engine, Window, Scene, Assets, File, Mods, Message, Console } from "@whatever/api";

Engine.on("init", ({ mod_id }) => {
  Engine.log("info", `loaded as ${mod_id}`);
});

Engine.on("frame", ({ delta_seconds }) => {
  // per-frame logic
});
```

No install step needed — Bun resolves the package automatically for any script running within the engine directory tree.

## API

### `Engine`

- `Engine.on(event, handler)` — subscribe to an engine event (`init`, `exit`, `frame`, `input`, `asset_response`, `mod_message`)
- `Engine.log(level, message)` — log through the engine logger (`"info"`, `"warn"`, `"error"`)

### `Window`

- `Window.setTitle(title)` — change the window title

### `Scene`

- `Scene.spawnSprite(entity_id, texture, position, scale?)` — spawn a textured sprite
- `Scene.moveEntity(entity_id, position)` — move an entity to a new world-space position
- `Scene.destroyEntity(entity_id)` — remove an entity from the scene

### `Assets`

- `Assets.request(request_id, path)` — request raw asset bytes from the VFS; result arrives as an `asset_response` event

### `File`

Sandboxed per-mod file I/O. Paths must not contain `..`.

- `File.write(path, data)` → `Promise<void>`
- `File.read(path)` → `Promise<string>`
- `File.delete(path)` → `Promise<void>`

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
