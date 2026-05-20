# Scripting API

Mods with a `[script]` section in `mod.toml` run as a Bun subprocess. The engine
communicates with each script over NDJSON on stdin/stdout. The `@whatever/api`
package abstracts the wire protocol.

```ts
import { Engine, Window, Scene, Assets, File, Mods, Message, Console } from "@whatever/api";
```

The package is provided by the engine's workspace (`runtime/`). No install step
needed — Bun resolves it automatically for any script running within the engine
directory tree.

---

## `Engine`

### `Engine.on(event, handler)`

Subscribe to an engine event. Registering a handler also sends a `Subscribe`
message to the engine automatically (except `mod_message`, which is routed
unconditionally).

```ts
Engine.on("init", ({ mod_id, engine_version }) => {
  Engine.log("info", `loaded as ${mod_id}, engine v${engine_version}`);
});

Engine.on("frame", ({ delta_seconds, frame_number }) => {
  // per-frame logic
});

Engine.on("input", ({ keys_pressed, mouse_delta }) => {
  if (keys_pressed.includes("Space")) { /* jump */ }
});

Engine.on("exit", ({ exit_code }) => {
  Engine.log("info", `shutting down (code ${exit_code})`);
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
| `frame` | `{ delta_seconds, frame_number }` | Fired every rendered frame |
| `input` | `{ keys_pressed, mouse_delta }` | Frame snapshot of keyboard + mouse state |
| `asset_response` | `{ request_id, path, data_base64, error }` | Response to a prior `Assets.request` call |
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

---

## `Window`

### `Window.setTitle(title)`

Change the title of the active window.

```ts
Window.setTitle("My Game — Level 1");
```

---

## `Scene`

### `Scene.spawnSprite(entity_id, texture, position, scale?)`

Spawn a textured sprite in the scene.

```ts
Scene.spawnSprite("player", "my_mod://textures/player.png", [0, 0, 0]);
Scene.spawnSprite("player", "my_mod://textures/player.png", [0, 0, 0], [2, 2, 1]);
```

| Parameter | Type | Description |
|---|---|---|
| `entity_id` | `string` | Unique identifier for this entity |
| `texture` | `string` | VFS path to the texture (`mod_id://relative/path.png`) |
| `position` | `[x, y, z]` | World-space position |
| `scale` | `[x, y, z]` | Scale multiplier, defaults to `[1, 1, 1]` |

### `Scene.moveEntity(entity_id, position)`

Move an existing entity to a new world-space position.

### `Scene.destroyEntity(entity_id)`

Remove an entity from the scene.

---

## `Assets`

### `Assets.request(request_id, path)`

Request raw asset bytes from the VFS. The result arrives as an `asset_response`
event. The `request_id` is echoed back so concurrent requests can be matched.

```ts
Assets.request("my-req-1", "my_mod://textures/player.png");

Engine.on("asset_response", ({ request_id, data_base64, error }) => {
  if (request_id !== "my-req-1") return;
  if (error) { Engine.log("error", error); return; }
  // decode data_base64 and use the asset
});
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
| `Frame` | `delta_seconds`, `frame_number` |
| `Input` | `keys_pressed`, `mouse_delta` |
| `AssetResponse` | `request_id`, `path`, `data_base64`, `error` |
| `FileResponse` | `request_id`, `data_base64`, `error` |
| `ModListResponse` | `request_id`, `mods` |
| `ModGetResponse` | `request_id`, `manifest`, `error` |
| `ModMessageReceived` | `source_mod_id`, `request_id`, `payload` |
| `ModMessageReplyDelivered` | `request_id`, `payload` |
| `CommandInvoke` | `request_id`, `command_path`, `args` |
| `Shutdown` | `exit_code` |

### Script → Engine

| Message | Key fields |
|---|---|
| `Subscribe` | `events` |
| `Log` | `level`, `message` |
| `SetWindowTitle` | `title` |
| `SpawnSprite` | `entity_id`, `texture`, `position`, `scale` |
| `MoveEntity` | `entity_id`, `position` |
| `DestroyEntity` | `entity_id` |
| `AssetRequest` | `request_id`, `path` |
| `FileWrite` | `request_id`, `path`, `data_base64` |
| `FileRead` | `request_id`, `path` |
| `FileDelete` | `request_id`, `path` |
| `ModListRequest` | `request_id` |
| `ModGetRequest` | `request_id`, `mod_id` |
| `ModMessageSend` | `target_mod_id`, `request_id`, `payload` |
| `ModMessageReply` | `request_id`, `payload` |
| `RegisterCommand` | `name`, `description`, `subcommands`, `args`, `has_handler` |
| `CommandResponse` | `request_id`, `output`, `error` |
