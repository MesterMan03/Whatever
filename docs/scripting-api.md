# Scripting API

Mods with a `[script]` section in `mod.toml` run as a Bun subprocess. The engine
communicates with each script over NDJSON on stdin/stdout. The `@whatever/api`
package abstracts the wire protocol — import it and use the `engine` singleton.

```ts
import { engine } from "@whatever/api";
```

The package is provided by the engine's workspace (`runtime/`). No install step
needed — Bun resolves it automatically for any script running within the engine
directory tree.

---

## Events

Subscribe to engine events with `engine.on(eventName, handler)`. Registering a
handler also sends a `Subscribe` message to the engine automatically. Multiple
handlers for the same event are all called in registration order.

### `init`

Fired once after the engine has initialised, the window is open, and the renderer
is ready. This is the right place to set up initial state.

```ts
engine.on("init", ({ mod_id, engine_version }) => {
  engine.log("info", `loaded as ${mod_id}, engine v${engine_version}`);
});
```

**Why there is no chicken-and-egg problem:** Bun runs the script's top-level code
synchronously before blocking on stdin. Any `engine.on("init", ...)` calls are
registered while the script is starting up. The engine only sends `Init` after the
renderer is ready — by which point the script is already listening.

| Field | Type | Description |
|---|---|---|
| `mod_id` | `string` | The mod's own ID as defined in `mod.toml` |
| `engine_version` | `string` | Engine version string (semver) |

---

### `exit`

Fired when the engine is shutting down. All handlers run, then the process exits
with the provided code. Use this to flush any state or log a farewell message.

```ts
engine.on("exit", ({ exit_code }) => {
  engine.log("info", `shutting down (code ${exit_code})`);
});
```

| Field | Type | Description |
|---|---|---|
| `exit_code` | `number` | The exit code the engine is closing with (normally `0`) |

---

### `frame`

Fired every rendered frame. Use for per-frame game logic.

```ts
engine.on("frame", ({ delta_seconds, frame_number }) => {
  // update game state
});
```

| Field | Type | Description |
|---|---|---|
| `delta_seconds` | `number` | Time elapsed since the previous frame, in seconds |
| `frame_number` | `number` | Monotonically increasing frame counter starting at 1 |

---

### `input`

Fired every frame with a snapshot of current input state.

```ts
engine.on("input", ({ keys_pressed, mouse_delta }) => {
  if (keys_pressed.includes("Space")) { /* jump */ }
});
```

| Field | Type | Description |
|---|---|---|
| `keys_pressed` | `string[]` | List of physical key names held this frame (e.g. `"KeyW"`, `"Space"`) |
| `mouse_delta` | `[number, number]` | Mouse movement since last frame `[dx, dy]` in pixels |

---

### `asset_response`

Fired in response to a prior `engine.requestAsset()` call. Use `request_id` to
match the response to the original request.

```ts
engine.requestAsset("my-req-1", "script-test://textures/player.png");

engine.on("asset_response", ({ request_id, data_base64, error }) => {
  if (request_id !== "my-req-1") return;
  if (error) { engine.log("error", error); return; }
  // decode data_base64 and use the asset
});
```

| Field | Type | Description |
|---|---|---|
| `request_id` | `string` | Echoed back from the original `requestAsset` call |
| `path` | `string` | The VFS path that was requested |
| `data_base64` | `string \| null` | Base64-encoded file bytes on success |
| `error` | `string \| null` | Error message on failure |

---

## Methods

### `engine.log(level, message)`

Send a log message through the engine's logger. The output includes the current
timestamp and the mod's ID automatically.

```ts
engine.log("info",  "everything is fine");
engine.log("warn",  "something looks off");
engine.log("error", "something broke");
```

| Parameter | Type | Values |
|---|---|---|
| `level` | `string` | `"info"` \| `"warn"` \| `"error"` |
| `message` | `string` | Arbitrary log text |

---

### `engine.setWindowTitle(title)`

Change the title of the active window.

```ts
engine.setWindowTitle("My Game — Level 1");
```

---

### `engine.spawnSprite(entity_id, texture, position, scale?)`

Spawn a textured sprite in the scene.

```ts
engine.spawnSprite("player", "my-mod://textures/player.png", [0, 0, 0]);
engine.spawnSprite("player", "my-mod://textures/player.png", [0, 0, 0], [2, 2, 1]);
```

| Parameter | Type | Description |
|---|---|---|
| `entity_id` | `string` | Unique identifier for this entity |
| `texture` | `string` | VFS path to the texture (`mod_id://relative/path.png`) |
| `position` | `[x, y, z]` | World-space position |
| `scale` | `[x, y, z]` | Scale multiplier, defaults to `[1, 1, 1]` |

---

### `engine.moveEntity(entity_id, position)`

Move an existing entity to a new world-space position.

---

### `engine.destroyEntity(entity_id)`

Remove an entity from the scene.

---

### `engine.requestAsset(request_id, path)`

Request raw asset bytes from the VFS. The result arrives as an `asset_response`
event. The `request_id` is echoed back so concurrent requests can be matched.

---

## IPC protocol reference

This section documents the raw wire format for engine internals or debugging.
Normal mod code should use `engine_api.ts` rather than speaking the protocol
directly.

**Transport:** NDJSON (one JSON object per line) on stdin/stdout per process.

### Engine → Script

| Message | Fields |
|---|---|
| `Init` | `mod_id`, `engine_version` |
| `Frame` | `delta_seconds`, `frame_number` |
| `Input` | `keys_pressed`, `mouse_delta` |
| `AssetResponse` | `request_id`, `path`, `data_base64`, `error` |
| `Shutdown` | `exit_code` |

### Script → Engine

| Message | Fields |
|---|---|
| `Subscribe` | `events` (list of internal message type strings) |
| `Log` | `level`, `message` |
| `SetWindowTitle` | `title` |
| `SpawnSprite` | `entity_id`, `texture`, `position`, `scale` |
| `MoveEntity` | `entity_id`, `position` |
| `DestroyEntity` | `entity_id` |
| `AssetRequest` | `request_id`, `path` |
