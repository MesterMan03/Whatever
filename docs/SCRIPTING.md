# Scripting API

Mods with a `[script]` section in `mod.toml` run as a Bun subprocess. The engine
communicates with each script over NDJSON on stdin/stdout. The `@whatever-engine/api`
package abstracts the wire protocol.

`Prefab.spawn(path): Promise<PrefabInstance>` instantiates a `.wprefab` entity tree
in the next command batch. The result exposes `root: Entity`, `get(scriptId)`,
`entities()`, `entityCount`, `path` and `loadTime`, with fresh IDs and local named
references for every instance. Creation is atomic and enforces direct mod
dependencies. Instances are independent copies. See [Asset libraries and
prefabs](ASSETS.md) for the JSON format, editor catalog and `.wasset` imports.

```ts
import { Engine, Input, Time, Window, File, Scene, EntityRef, Physics, Mods, Message, Console, Audio } from "@whatever-engine/api";
```

The engine-owned mod host embeds the API and registers `@whatever-engine/api` before
loading mod code. It resolves independently of the mod's directory or node_modules.
Leave this import external in mod bundles; install the npm package only if needed
for editor types. Other packages and all Node/Bun built-in modules are denied by
default. Local JS/TS/JSON modules must resolve inside the current mod directory.
See [the runtime policy](SANDBOX.md) for the module and global allowlists.

The trusted host captures fixed stdin/stdout operations before activating the policy.
Its NDJSON reader uses asynchronous descriptor reads and its writer completes every
UTF-8 batch, including partial writes, before returning. These private operations
work with the ordinary OS pipes created by the Rust player and do not expose file
descriptors or built-in modules to mods.

---

## `Engine`

### `Engine.mode`

Read-only `"client" | "server"`, available during top-level mod evaluation as well as event handlers. The host sets it before importing the mod. `init` includes the same `mode` alongside `mod_id` and `engine_version`.

Simulation ticks run on a separate engine worker. Each mod submits one command buffer per tick, independently of promises returned by its handlers. The server runs this worker without a window, GPU or audio output device. FPS and vsync affect presentation only. Invalid tick rates (nonpositive, nonfinite or unrepresentable) are logged and ignored.

Window and audio methods, `Engine.setFpsCap`, `setVsync`, `setMainCamera`, `getMainCamera` and `clearCamera` reject use in server mode. ECS component data, native physics and entity operations remain available there, including components a client may later render.

### `Engine.on(event, handler)`

Subscribe to an engine event. Registering a handler also sends a `Subscribe`
message to the engine automatically (except `mod_message`, which is routed
unconditionally). Every mod receives private ticks for buffer submission, even without
a public tick listener. Async tick handlers may span ticks; a still-running handler
is skipped until it finishes. Its next invocation accumulates simulation time and
mouse deltas from skipped ticks, with the latest held keys and global tick number.

```ts
Engine.on("init", ({ mod_id, engine_version, mode }) => {
  Engine.log("info", `loaded as ${mod_id}, engine v${engine_version}`);
});

Engine.on("exit", ({ exit_code }) => {
  Engine.log("info", `shutting down (code ${exit_code})`);
});

Engine.on("tick", async () => {
  // Input and Time are available in any function, including component hooks.
  // Record commands here. Awaiting a result does not stop buffer submission.
});
```

The host sends an internal `RuntimeReady` message after mod evaluation completes.
The engine sends `Init` after this handshake and runtime readiness (including window initialization on clients), so top-level event registration is in place before `init` fires.

### Events

| Event | Payload | Description |
|---|---|---|
| `init` | `{ mod_id, engine_version, mode }` | Fired once after engine initialization; server has no window |
| `exit` | `{ exit_code }` | Fired on shutdown; process exits after all handlers return |
| `tick` | `{ tick_number, delta_seconds, keys_pressed, mouse_delta, mouse_captured }` | Runs on ticks when this handler is free; no overlapping invocations |
| `mod_message` | `{ source_mod_id, message, request_id? }` | Message from another mod (see `Message`) |
| `physics` | `{ event: Physics.Event }` | Native collision/contact-force event after physics |

### Tick keyboard input: `PressedKeys` and `KeyCode`

`Input.keysPressed` (also `tick.keys_pressed`) is an immutable `PressedKeys` snapshot. Its query methods accept
the exported `KeyCode` string union, so TypeScript suggests supported names and
rejects typos. The union covers all 194 physical key codes emitted by the engine's
current winit version: letters (`KeyA`–`KeyZ`), digits (`Digit0`–`Digit9`), arrows,
punctuation, left/right modifiers, navigation keys, numpad, `F1`–`F35`, international,
media and system keys. Codes describe physical positions, independent of the
active keyboard layout; `KeyW` is the key at the QWERTY W position.

```ts
Engine.on("tick", () => {
  const forward = Input.isAnyPressed("KeyW", "ArrowUp");
  const sprinting = forward && Input.isShiftPressed();
  const controlS = Input.isControlPressed() && Input.isPressed("KeyS");
  const leftControlS = Input.areAllPressed("ControlLeft", "KeyS");
});
```

| Member | Result |
|---|---|
| `isPressed(key: KeyCode)` | Whether the key is held |
| `isAnyPressed(...keys: KeyCode[])` | Whether at least one key is held; false with no arguments |
| `areAllPressed(...keys: KeyCode[])` | Whether every key is held; true with no arguments |
| `isShiftPressed()` | Either `ShiftLeft` or `ShiftRight` |
| `isControlPressed()` | Either `ControlLeft` or `ControlRight` |
| `isAltPressed()` | Either `AltLeft` or `AltRight`, including the physical AltGr position |
| `isSuperPressed()` | Either `SuperLeft` or `SuperRight` (Windows / Command) |
| `includes(key: KeyCode)` | Compatibility alias for `isPressed(key)` |
| `size` | Number of distinct held keys |
| `toArray()` | Frozen `readonly KeyCode[]` |
| `[Symbol.iterator]()` | Iterate held codes with `for...of` or `[...keys]` |
| `toJSON()` | Serialize as an array of key codes |

These methods report held state, so they stay true on successive ticks while a
key remains down. They do not detect a new press or release. Each snapshot stays
unchanged across awaits; a busy async handler receives the latest held keys on
its next invocation. Server ticks contain an empty snapshot.

Existing `keys_pressed.includes("KeyW")` checks keep working and gain autocomplete.
The payload is no longer an array: use `keys.size` for its count,
`keys.toArray()` for read-only array operations, or `[...keys]` for a mutable copy.
`new PressedKeys(codes)` also creates an independent snapshot from a
`readonly KeyCode[]` (default: empty). The private NDJSON `Tick` still sends a
`keys_pressed` array; the API wraps it before delivering public tick handlers.

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

Host protocol 10 requires revisioned scene deltas and native performance samples; older hosts are rejected and rebuilt.
Mouse motion is relative and only delivered while the player owns mouse capture.
`mouse_captured` is false and `mouse_delta` is `[0, 0]` while uncaptured, while the
console is open, or while the editor owns input. Release also clears accumulated
motion for busy async tick handlers; it cannot be replayed after console/editor
interaction. Held keys retain their normal focus/console behavior.

Rendering interpolates adjacent tick poses for the camera, objects and lights.
This adds roughly one tick of visual delay; it never changes ECS queries, physics
or tick timing. FPS caps are maximum presentation rates, not guaranteed throughput;
vsync, display refresh, GPU load and OS scheduling can keep measured FPS below them.

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

### `Engine.setMainCamera(entityOrId)`

Designate an entity as the scene's active camera.  The entity must have both
`core:camera` and `core:transform` components attached; the engine reads them
every frame to produce the view-projection matrix.

Accepts `Entity`, `PendingEntity`, `EntityRef`, or an opaque ID string. Pending cameras
can be created, configured and selected in the same batch.

```ts
const cam = await Scene.createEntity().get();
cam.setComponent(new BuiltInComponents.Transform({ position: [0, 5, 10] }));
cam.setComponent(new BuiltInComponents.Camera({ fovy_degrees: 60 }));
Engine.setMainCamera(cam);
```

Multiple camera entities can coexist in the scene; only the designated one is
rendered.  If no camera is set, or if the entity dies, the screen clears to
black and a warning overlay is displayed.

### `Engine.getMainCamera()` → `Entity | null`

Read the current camera from the committed revision plus local camera commands. Useful for restoring the
previous view after a mod temporarily installs a camera. Available on the client only.

### `Engine.clearCamera()`

Remove the active camera.  The screen clears to black and shows a "no active
camera" warning until `Engine.setMainCamera` is called again.

```ts
Engine.clearCamera();
```

---

## `Input`

Import `Input` from `@whatever-engine/api` to read the latest tick's keyboard and
mouse state from any function, including component lifecycle hooks. No
`Engine.on("tick")` subscription is required. The runtime updates input before
component synchronization, `onEnable`, `start`, component ticks and public tick
handlers. The object and all exposed snapshots are read-only at runtime and in
TypeScript.

| Member | Result |
|---|---|
| `Input.keysPressed` | Immutable `PressedKeys` snapshot for the current tick |
| `Input.isPressed(key: KeyCode)` | Whether the physical key is held |
| `Input.isAnyPressed(...keys: KeyCode[])` | Whether any supplied key is held |
| `Input.areAllPressed(...keys: KeyCode[])` | Whether every supplied key is held |
| `Input.isShiftPressed()` / `Input.isControlPressed()` | Whether either side of the modifier is held |
| `Input.isAltPressed()` / `Input.isSuperPressed()` | Whether either side of the modifier is held |
| `Input.mouseDelta` | Frozen `readonly [number, number]` for this tick's relative motion |
| `Input.mouseCaptured` | Whether the player owns mouse capture |
| `Input.mouseButtons` | Frozen held `MouseButtonCode[]` (`Left`, `Right`, `Middle`, `Back`, `Forward`, `Other${number}`) |
| `Input.mouseButtonsPressed` | Frozen press edges collected since the preceding tick, including quick clicks |
| `Input.isMouseButtonDown(button)` | Whether the button is held |
| `Input.wasMouseButtonPressed(button)` | Whether a press occurred during this tick |

Mouse buttons are empty while uncaptured, on a headless server, or while the console is open. Capture loss clears held buttons and pending presses. Retain an array snapshot before awaiting; press edges last one tick, so component hooks can consume them synchronously.

The key queries share `PressedKeys`' typed key names and held-state semantics.
Before the first tick, keys are empty, mouse motion is `[0, 0]`, and capture is
false. Headless server input stays empty and uncaptured. Uncaptured ticks always
report zero mouse motion.

`Input` is a shared live view. After an `await`, it reports the latest received
tick. To retain the original input, save `const keys = Input.keysPressed` and
`const mouse = Input.mouseDelta` before awaiting; those snapshots never change.
Mouse deltas in this object cover one tick, while a busy tick handler's existing
`mouse_delta` argument still accumulates motion from skipped ticks.

## `Time`

Import `Time` from `@whatever-engine/api` to read simulation timing anywhere.
It updates alongside `Input` before component hooks and tick handlers, even
without a tick listener. Both properties are read-only.

| Member | Result |
|---|---|
| `Time.tickNumber` | Latest engine tick number; zero before the first tick |
| `Time.deltaSeconds` | Simulation interval for this tick in seconds; zero before the first tick |

Timing advances on clients and headless servers. `deltaSeconds` uses the current
tick's configured interval; a tick-rate change appears with the next tick. It
does not measure wall-clock time and does not accumulate skipped async ticks.
Existing `tick.delta_seconds` and `Component.tick(deltaSeconds)` arguments retain
their accumulated-time behavior. Save timing values before an `await` when they
must stay associated with that invocation.

```ts
import { Component, Input, Time } from "@whatever-engine/api";

export class Movement extends Component {
  override tick() {
    if (!Input.isPressed("KeyW")) return;
    const speed = Input.isShiftPressed() ? 6 : 3;
    this.entity.editComponent("core:transform", (transform) => {
      transform.position[2] -= speed * Time.deltaSeconds;
    });
  }
}
```

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

## Tick order and command batches

Each tick has one ordered pipeline:

1. Gather input and queued external work: console invocations, network events and completed audio notifications. Input is sampled once with held keys in stable sorted order; events arriving after collection wait until a later tick.
2. Send a tick to every initialized mod. The SDK refreshes `Input` and `Time`, synchronizes and ticks components, calls available tick handlers in registration order, then flushes its command buffer immediately, without awaiting user promises. This private flush happens even when there are no tick handlers or all handlers are awaiting results.
3. Collect exactly one batch per live mod for the current tick. Execute complete batches in dependency/load order, and commands within each batch in recording order. No command executes while other batches are still being collected.
4. Reconcile physics components, step each native world in stable entity order using this tick's sampled delta, then write poses and derived state back to ECS. World substeps/timeScale affect only that world's integration. Position-based kinematic targets interpolate across its substeps.
5. Publish the world/render snapshot and buffered command results. Result callbacks and async continuations record new commands for a later tick.

All API namespaces share this buffer, including logging, window commands, files, ECS, physics, console and networking. Arguments are serialized when recorded so later JavaScript mutations cannot change earlier commands. Queries capture the value at their position in the command sequence. Native physics operations/queries reconcile preceding component changes without advancing simulation time. Physics events and buffered results publish after the physics phase. Changing the tick rate affects the next tick; the current tick keeps the delta already delivered to mods. This is ordered execution, not an atomic transaction: commands do not roll back together on failure.

Mods are ordered by dependencies and explicit load constraints, with `core` preferred first and alphabetical mod-ID tie-breaking among eligible mods. Startup waits for host handshakes and sends `Init` in this order before the first tick. Pipe arrival order cannot change command execution order. A process exit removes it from the submission barrier; shutdown interrupts that barrier. A synchronously blocked mod can still delay submission, while an asynchronously waiting handler continues submitting empty or otherwise queued buffers.

This establishes a predictable execution order, not automatic deterministic gameplay. Network/input timing, timers and arbitrary asynchronous JavaScript still need controlled inputs for reproducible runs. Do not await engine API requests during top-level module evaluation: the startup handshake happens after evaluation; perform that work in `init` instead.

## Scene revisions and synchronous reads

Every mod receives a full scene revision before `init`, followed by deltas after
commands and physics. `Entity` handles resolve into that local scene store:
`exists`, `name`, `hasComponent`, `getComponent`, `getTags`, `hasTag`, `getEnabled`,
`getActive`, `getParent`, and `getChildren` are synchronous. `Scene.find`,
`listEntities`, `query`, `findByTag`, and `findByTags` use the same store. Component
and tag indices narrow queries; no IPC read is recorded for these methods.

`getComponent` returns `ComponentSnapshot<T> | null`: deeply immutable data with
no mutable class helpers or lifecycle methods. Custom snapshots expose declared
fields and `id`; they are distinct from a component's mutable lifecycle `this`.
Use `entity.editComponent(kind, draft => { ... })` to edit a detached class draft;
the callback must be synchronous. Only changed top-level fields are patched.
`entity.setEulerDegrees(x, y, z)` patches rotation without replacing position or scale.

```ts
const transform = this.entity.getComponent("core:transform");
if (transform) this.entity.setEulerDegrees(0, this.angle, 0);
this.entity.editComponent(Health, draft => { draft.value = 80; });
```

The committed revision stays fixed during a tick. Each mod overlays its recorded
writes so it can read its own pending creates, removals, fields, hierarchy and tags
immediately. Other mods' writes become visible at the next committed revision.
World transforms compose local overlay edits using the engine's TRS convention.
Rejected native commands reconcile back to authoritative data on commit. Retained
component values never change. `Scene.snapshot()` returns a retained committed
`SceneSnapshot` with `revision`, `has(id)`, `get(id)` and `entities()`; it excludes
local overlays and shares immutable values with future revisions. A revision gap
faults the host rather than allowing scripts to continue on stale state.

`Scene.revision` reads the committed revision. IDs include generations; stale IDs
never resolve to recycled entities. `Scene.find(id)` returns null for missing or
locally destroyed entities. Full native component data is visible to every mod,
just as native ECS queries were; private lifecycle fields are never published.

## `Pending<T>` and explicit ordered requests

`Scene.createEntity()` and spawn/clone helpers return `PendingEntity`. It is usable
immediately; `.id` is a provisional `pending:...` identifier until confirmed.
Calling `.get()` optionally waits for a native `Entity`. References and later
commands resolve provisional aliases automatically. Recursive clone descendants
and remapped references become authoritative when creation is confirmed; provisional
clone reads expose the copied root only. Pending handles are not promises.

Use `entity.requestComponent(kind)`, `requestParent()`, `requestChildren()`,
`Scene.requestComponent(id, kind)`, `Scene.requestQuery(kinds)`, or
`Engine.requestMainCamera()` when the result must reflect a particular position in
the ordered native command buffer. These return `Pending<T>`; `.get()` returns
the same promise without submitting or repeating the command. Result continuations
record work for a later batch. Ordered component requests return detached mutable
native class data; custom requests retain their owning lifecycle instance behavior.

```ts
const entity = Scene.createEntity().setComponent(new BuiltInComponents.Transform());
const before = entity.requestComponent("core:transform");
entity.move([3, 0, 0]);
const immediate = entity.getComponent("core:transform"); // position [3, 0, 0]
const earlier = await before.get(); // position [0, 0, 0] at request position
```

Native publication consumes mutation journals, compares only touched values, and
sends creates, generation-aware tombstones, changed component values and metadata.
Custom instance IDs detect remove/re-add even when values are equal. Each delta
is encoded once and its bytes shared across native pipe writers. Each Bun process
currently holds its own decoded store; OS shared read-only mappings are not part
of this implementation. No unmeasured shared-memory performance claim is made.

## `Profiler`

- `Profiler.now(): number` — monotonic milliseconds from captured native `hrtime.bigint`.
- `Profiler.latest(): DeepReadonly<TickTimings> | null` — previous completed native tick.
- `Profiler.runtime()` — this host's previous tick number, snapshot application and synchronous script work in milliseconds.
- `Profiler.memory()` — this host's native `heap_used` and `rss` in bytes.

`TickTimings` contains `tick_number`, `tick_rate`, `total_ms`, `scripts_ms`
(the barrier waiting for all hosts), `commands_ms`, `physics_ms`, `snapshot_ms`,
`presentation_ms`, `snapshot_bytes` (one encoded delta, before fan-out), command,
entity and host counts, `build`, `platform`, and optional `frame`.
`total_ms` includes scene publication/result overhead and excludes scheduled idle time
and the telemetry message's own encoding;
individual phases may not sum exactly to it. Initialization is excluded by benchmark warmup.
`FrameTimings` carries `frame_number`, `frame_ms` (wall interval between successful presentations), `cpu_ms`,
`acquire_ms`, `encode_ms`, `submit_ms`, `present_ms`, `draw_calls` (scene only),
resolution, adapter/backend, vsync and FPS cap. The most recent frame may repeat
across several simulation ticks; consumers must deduplicate by frame number.
`gpu_ms` is null: GPU timestamp queries are not implemented, and CPU submission
measurements must not be described as GPU execution time. Server `frame` is null.
Trusted clock/memory captures expose no process, file descriptor, or built-in module.
See [Markbench](../games/test/mods/markbench/README.md) for workload and score definitions.

## `Type`, `Struct`, and custom `Component` classes

The common schema system is shared by components and networking. `Network.Type` and
`Network.Struct` remain aliases. Numeric descriptors are `Type.I32`, `Type.F64`, and
`Type.VarInt`; `.with({ min?, max? })` returns an immutable constrained descriptor.
Other descriptors are `Type.Bool`, `Type.String.with({ maxLength? })`,
`Type.Array(element, { maxLength? })`, `Type.Optional(inner)`, and `Struct(fields)`.
`Infer<typeof descriptor>` obtains the TypeScript value type.

Components also support `Type.Nullable(inner)`, `Type.Enum(["a", "b"])`,
`Type.Tuple(...elements)`, `Type.Entity`, and `Type.Asset`. Entity values are explicit
references; asset values are VFS strings (an empty string represents no asset).
Network packets and client-data definitions accept the original codec-supported
subset; component-only types are rejected during network registration. Existing
packet bytes, field order, and fingerprints are unchanged.

Optional fields use `undefined` in JavaScript and the private IPC null marker;
nullable fields use an explicit `null`. Optional and nullable wrappers cannot be
combined. Bounds, lengths, tuples, enum choices and undeclared struct keys are
validated in both the SDK and native engine.

```ts
import { Component, Entity, Type, field } from "@whatever-engine/api";

export class Health extends Component {
  static readonly type = "game:health";
  value = field(Type.I32.with({ min: 0, max: 100 }), 100, {
    label: "Health", tooltip: "Current hit points.",
  });
  target: Entity | null = null;
  private elapsed = 0;

  override start() { /* this.entity is available here */ }
  override tick(deltaSeconds: number) { this.elapsed += deltaSeconds; }
  override onEnable() {}
  override onDisable() {}
  override destroy() {}
}
```

Exported named class declarations are discovered in the owning mod's local JS/TS source files, excluding assets and node_modules.
A stable `static readonly type = "mod_id:name"` is recommended; otherwise the ID is
`mod_id:ClassName`. Component IDs belong to their mod and must be unique. The
engine/editor invoke the bundled host's read-only `--component-metadata` tool to
extract schemas without evaluating mod code or running project configuration.
The host loads discovered class modules under its ordinary native sandbox before
the script entry. Keep component modules free of top-level scene mutations;
initialize game state in `start()` or an `init` event.

Public writable instance fields are persisted and exposed in the inspector. The
extractor supports inherited fields, numbers, booleans, strings, string-literal
choices, arrays, fixed tuples, plain nested objects, optional fields, and nullable
entity references. `field(type, default, { label?, tooltip? })` overrides an inferred
schema and supplies inspector hints; it returns the normal typed field value.
Static, readonly, private/protected and `#private` fields, methods and getters are
excluded. Defaults must be literals or supported literal constants; arbitrary
calls and recursive data are diagnosed with a source location. Constructors must
work without arguments. `id`, `entity`, and lifecycle names are reserved.

Rust validates schemas before scene loading and validates every component write,
including editor edits and class field updates. Missing fields receive class
defaults; unknown fields and unregistered component names are rejected. Scene
loading rolls back invalid instances atomically. Class instances stay in their
owning mod process; other processes exchange validated data, not methods.

Each entity/component pair has one persistent instance. Committed membership and
field snapshots arrive before the next tick. An active instance receives
`onEnable()`, then `start()` once before its first `tick(deltaSeconds)`. Inactive
entities (including disabled ancestors) pause ticking and receive `onDisable()`;
reenabling calls `onEnable()` without repeating `start()`. Removal/re-attachment also creates a fresh instance when both happen in one batch. Removal, recursive
entity destruction and orderly shutdown dispose the instance once with
`destroy()` (after `onDisable()` when active). Crashes cannot guarantee cleanup.

`start()` and `tick()` may return promises. A busy instance is skipped without
overlapping calls, and elapsed tick time accumulates. The command batch always
flushes before awaiting native results. Hook failures log the component and entity
and stop that instance's ticking. Declared field changes, including nested arrays,
are validated and collected into one field patch per instance per tick. Private
state stays local. Component hooks run before global `Engine.on("tick")` handlers.
`Input` and `Time` already describe the current tick when these hooks run; a
parameterless `tick()` can use `Input.isPressed("KeyW")` and `Time.deltaSeconds`
directly. Its optional use of the existing `deltaSeconds` argument remains useful
when elapsed time should include skipped async ticks.
Native component instances retain their explicit fetch/mutate/set workflow.

The editor lists registered classes under Add Component, renders schema-based
fields with defaults/reset controls, ranges, dropdowns and entity/asset selectors,
and refreshes metadata in the background on regaining focus or Reload component
types. The bottom bar reports mod progress and errors; previous types and unsaved
edits remain available until the new types are ready. Restart Play
to load source/schema changes; code hot reload is not provided.

## `Entity`

Entities use opaque generational IDs, `"index:generation"`. Built-in component
lookups use their known `core:` IDs; custom component lookups use their class.

### Typed references in component data

`Entity`, `PendingEntity`, `Scene.ref(entityOrId)`, and `new EntityRef(entityOrId)`
implement `EntityReference`. Native storage resolves pending references within the
originating batch and remaps references during scene instantiation and cloning.
Reads hydrate references to entities. Ordinary strings remain data; `$entity` is
reserved for the private wire representation.

### `entity.id`

The opaque generation-aware ID, provisional until native creation is confirmed.

### `entity.destroy()`

Destroy this entity and its descendants. Fire-and-forget.

### `entity.setComponent(component)` → `this`

Attach or replace a registered component class instance. Both native instances and
custom `Component` subclasses are accepted. The raw `(name, data)` overload and
`Scene.setComponent` have been removed; plain objects cannot spoof registration.

```ts
const entity = Scene.createEntity()
  .setComponent(new BuiltInComponents.Transform({ position: [1, 2, 3] }))
  .setComponent(new Health());
const health = entity.getComponent(Health);
if (health) entity.editComponent(Health, draft => { draft.value = 80; });
entity.editComponent("core:transform", draft => { draft.addX(1); });
```

### `entity.removeComponent(componentIdOrClass)` → `this`

Remove a known native component by ID or a custom component by class.

### `entity.getComponent(componentIdOrClass)` → `ComponentSnapshot<T> | null`

Read immutable typed fields immediately. `entity.editComponent(kind, callback)`
provides a mutable detached class draft; `requestComponent(kind).get()` explicitly
requests data at the command position. Lifecycle class instances remain local to
the owning mod; snapshot reads never expose their private state or mutable identity.

### Entity tags

- `entity.addTag(tag)` / `entity.removeTag(tag)` → `this`
- `entity.getTags()` → `readonly string[]`
- `entity.hasTag(tag)` → `boolean`
- `Scene.findByTag(tag, { includeDisabled? })` → `readonly Entity[]`
- `Scene.findByTags({ all?, any?, none?, includeDisabled? })` → `readonly Entity[]`

Each entity holds at most 32 unique, case-sensitive strings. A tag must contain
1–128 UTF-8 bytes, have nonblank text, and contain no control characters. Duplicate
additions and removing an absent tag are harmless. A 33rd unique tag is rejected
without changing membership. Filters accept at most 32 unique strings per group.
All `all` tags must match, at least one `any` tag must match when supplied, and no
`none` tags may match. An empty filter returns all active entities. Disabled
entities/descendants are excluded unless `includeDisabled: true`. Results use
stable entity allocation order. Tag methods also work on pending entities.

Tags persist in `.wscene` entity `tags` arrays and follow deep cloning, editor
undo/redo and live editing. Destruction clears membership before an ID is reused.
Use tags for categories instead of empty custom component objects.

```ts
const enemy = Scene.createEntity().addTag("enemy").addTag("flying");
const enemies = Scene.findByTags({ all: ["enemy"], none: ["dead"] });
```

### `entity.move(position)` → `this`

Convenience: update the `position` field of this entity's `core:transform` while
preserving existing rotation and scale. Initialises rotation/scale to identity/`[1,1,1]`
if the component is not yet set.

```ts
entity.move([x, y, z]);
```

### `entity.setParent(parent)` → `this`

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
### `entity.getParent()` → `Entity | null`

Return the parent `Entity`, or `null` if this entity has no parent.

```ts
const parent = entity.getParent();
if (parent) Engine.log("info", `parent: ${parent.id}`);
```

### `entity.getChildren()` → `readonly Entity[]`

Return all direct children of this entity.

```ts
const kids = entity.getChildren();
```

---

## `Scene`

Entity and component management. Entities are identified by opaque string IDs of
the form `"index:generation"`. Built-in components use the `core:` namespace.

Most callers should work with `Entity` objects returned by `Scene.createEntity`,
`Scene.listEntities`, `Scene.spawnSprite`, and `Scene.query` rather than calling
component helpers with raw IDs.

### `Scene.loadScene(path)` → `Promise<LoadedScene>`

The calling mod may load its own scenes or scenes from directly declared dependencies.
Authored assets and custom component types are validated against the scene-owning
mod's direct dependencies before anything is instantiated. Native component IDs
remain available without a mod dependency. Typed asset fields, including nested
custom component fields, must resolve to existing VFS assets. Ordinary text fields
are not interpreted as asset references.

Instantiate a versioned JSON `.wscene` from a loaded mod. `my_mod://level` and
`my_mod://scenes/level.wscene` both load `my_mod/scenes/level.wscene`. Nested
paths such as `my_mod://levels/intro` are supported. Scene files are ordinary
mod resources, available in both client and server modes.

```json
{
  "version": 1,
  "entities": [
    {
      "name": "Root",
      "script_id": "root",
      "components": {
        "core:transform": {
          "position": [0, 0, 0],
          "rotation": [0, 0, 0, 1],
          "scale": [1, 1, 1]
        }
      },
      "children": [
        {
          "name": "Child",
          "script_id": "child",
          "components": {
            "my_mod:target": {"entity": {"$entity": "root"}}
          }
        }
      ]
    }
  ]
}
```

`children` encodes parentage; transforms are local to the parent. `name` is an
optional editor label. `script_id` is an optional, nonempty name unique within
the file. There are no predefined runtime entity IDs. Components use their
registered data schemas and may belong to any loaded mod. An optional `tags` array stores up to 32 unique tags. Explicit `{"$entity":"name"}`
references are resolved against this scene's script IDs, including forward
references. Ordinary strings are preserved. Native derived state such as
`core:rigid_body_state` cannot be authored.

Authored physics components use the same native validation and defaults as script
commands. A scene rigid body must explicitly reference a scene entity with
`core:physics_world`, for example `"world":{"$entity":"world"}`. Standalone
colliders (`"body":null`) must also name that world. An omitted collider body
attaches to its own entity; explicit body references support compound colliders.
Scene loading never creates a hidden default world outside the scene instance.

```ts
const scene = await Scene.loadScene("my_mod://level");
const root: Entity = scene.get("root");
const all: Entity[] = scene.entities();
Engine.log("info", `${scene.entityCount} entities loaded in ${scene.loadTime} ms`);
```

`LoadedScene` is exported from `@whatever-engine/api`. `get(scriptId)` throws
if the name does not exist in this instance. `entities()` returns a new array
of entity wrappers in parent-first file order; `entityCount` is its original
size and `loadTime` measures native reading, parsing and instantiation in
milliseconds. These describe the instance at load time; destroying an entity
later does not remove it from this metadata.

The load command executes inside the calling mod's next command batch, in
recorded order. Its Promise resolves with the normal command results, after
native simulation. Each call creates an independent instance with fresh
generational IDs. Duplicate names, unresolved references, invalid built-in
components, unknown document fields/versions, excessive hierarchy depth or
missing files reject the Promise. Failed instances are fully rolled back;
existing entities remain alive. The native hierarchy limit is 64 levels and
the entity limit is 100,000, in addition to the JSON parser's nesting limits.

Loading does not replace the world or select a main camera. Scripts control
those actions, for example `Engine.setMainCamera(scene.get("camera").id)`.
Render resources are invalidated for the new hierarchy and authored physics
components join the normal native simulation phase. See [editor and project
workflows](EDITOR.md).

The VFS preserves existing asset paths such as `core://shaders/sprite.wgsl`.
It also mounts a mod's `scenes/` folder at `mod_id://scenes/`. Additional
contained folders can be declared with `[mounts]` in `mod.toml`, for example
`data = "data"`; this exposes `mod_id://data/...`. `scenes = "levels"` changes
the physical scene folder while retaining the virtual `scenes/` prefix.
Mounts share VFS override resolution and cannot escape the mod root.

### `Scene.createEntity()` → `PendingEntity`

Create a new entity and return it.

```ts
const entity = await Scene.createEntity().get();
```

### `Scene.destroyEntity(entity_id)`

Destroy an entity by raw ID and all its components. Fire-and-forget.
Prefer `entity.destroy()` when you already have an `Entity` object.

### `Scene.listEntities()` → `readonly Entity[]`

Return all living entities synchronously, including pending local creations.

### `Scene.removeComponent(entity_id, component_type)`

Remove a component from an entity. Fire-and-forget.

### `Scene.getComponent(entity_id, component_type)` → `ComponentSnapshot<T> | null`

Get a component's data by raw ID. Return type is automatically inferred for
built-in component types. Returns `null` if the component is not set.

```ts
const t = await Scene.requestComponent(id, "core:transform").get();
// t is BuiltInComponents.Transform | null
```

### `Scene.query(component_types)` → `readonly { entity: Entity; components: Readonly<Record<string, any>> }[]`

Return active entities that have every listed native component ID or custom component class, along with the requested component data.

```ts
const results = Scene.query(["core:transform", "core:sprite_renderer"]);
for (const { entity, components } of results) {
  const t = components["core:transform"] as BuiltInComponents.Transform;
  entity.move([t.position[0] + 1, t.position[1], t.position[2]]);
}
```

Synchronous rows contain immutable component data. `Scene.requestQuery` keeps the ordered pending result form.

### `Scene.spawnText(text, position, options)` → `PendingEntity`

Convenience: create an entity with `core:transform` and `core:text_renderer` pre-attached. Returns the entity.

```ts
const label = await Scene.spawnText("Hello, world!", [0, 0, 0], {
  font: "core://fonts/default.ttf", shader: "core://shaders/sprite.wgsl",
}).get();

// With options:
const label = await Scene.spawnText("Score: 0", [0, 0, 2], {
  font: "my_mod://fonts/custom.ttf",
  shader: "core://shaders/sprite.wgsl",
  font_size: 32,                        // optional, defaults to 24
  color: [1, 0.8, 0, 1],               // optional RGBA, defaults to [1, 1, 1, 1]
}).get();
```

### `Scene.spawnSprite(texture, shader, position, scale?)` → `PendingEntity`

Convenience: create an entity with `core:transform` and `core:sprite_renderer`
pre-attached. Pass the shader's VFS path explicitly.

```ts
const entity = await Scene.spawnSprite(
  "my_mod://textures/player.png",
  "core://shaders/sprite.wgsl",
  [0, 0, 0],
  [1, 1, 1],   // optional, defaults to [1, 1, 1]
).get();
```

### `Scene.spawnMesh(mesh, shader, position, options?)` → `PendingEntity`

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
).get();
```

### `Scene.moveEntity(entity_id, position)` → `void`

Convenience: update the `position` field of an entity's `core:transform` while
preserving its rotation and scale.

```ts
Scene.moveEntity(id, [x, y, z]);
```

### `Scene.addAmbientLight(color?, intensity?)` → `PendingEntity`

Convenience: create an entity and attach `core:ambient_light`. Multiple ambient lights are additive.

```ts
await Scene.addAmbientLight([1, 0.9, 0.7], 0.15).get();
```

### `Scene.addDirectionalLight(direction, color?, intensity?)` → `PendingEntity`

Convenience: create an entity and attach `core:directional_light`.

```ts
await Scene.addDirectionalLight([0, -1, -0.3], [1, 1, 1], 1.0).get();
```

### `Scene.addPointLight(position, color?, intensity?, range?)` → `PendingEntity`

Convenience: create an entity with `core:transform` + `core:point_light`. The light is emitted from `position` in all directions.

```ts
await Scene.addPointLight([0, 3, 0], [1, 0.5, 0.1], 2.0, 15).get();
```

### `Scene.setParent(entity_id, parent_id)`

Attach an entity to a parent by raw IDs. Pass `null` as `parent_id` to detach.
Fire-and-forget. Prefer `entity.setParent()` when you have an `Entity` object.

```ts
Scene.setParent(childId, parentId);
Scene.setParent(childId, null); // detach
```

### `Scene.getParent(entity_id)` → `string | null`

Return the parent entity ID as a string, or `null` if the entity has no parent.

### `Scene.getChildren(entity_id)` → `readonly string[]`

Return the IDs of all direct children of the given entity.

### Built-in component shapes and classes

`editComponent` drafts and explicit `requestComponent` results use native data classes with the helpers below. Synchronous `getComponent` returns immutable fields without these methods.

#### `BuiltInComponents.Transform`

Mutable drafts for `editComponent("core:transform")`. Raw fields are still present for backwards compatibility; all setters are chainable and return `this`.

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
  const t = await entity.requestComponent("core:transform").get();
  if (!t) return;
  t.rotateY(45).setX(t.getX() + 0.1);
  entity.setComponent(t);
});
```

The constructor `new BuiltInComponents.Transform({ position?, rotation?, scale? })` creates a fresh instance with identity defaults.

#### `BuiltInComponents.SpriteRenderer`

Mutable drafts for `editComponent("core:sprite_renderer")`.

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

Mutable drafts for `editComponent("core:mesh_renderer")`. Renders arbitrary triangle geometry from a mesh file.

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

Mutable drafts for `editComponent("core:text_renderer")`. Renders a UTF-8 string at the entity's world-space Transform position, projected to screen coordinates.

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

Text is rendered on top of sprites. The official starter library includes Noto Sans;
after import its path is `<mod>://libraries/whatever_starter/fonts/default.ttf`.
Mods may supply other TTF/OTF fonts and pass their VFS path in the `font` field.

```ts
// Spawn a white label
const label = await Scene.spawnText("Hello!", [0, 0, 0], { font_size: 36 }).get();

// Update it each tick
Engine.on("tick", async ({ tick_number }) => {
  const t = await label.requestComponent("core:text_renderer").get();
  if (!t) return;
  t.setText(`Tick: ${tick_number}`);
  label.setComponent(t);
});
```

#### `BuiltInComponents.Camera`

Mutable drafts for `editComponent("core:camera")`.  Makes the entity usable as a scene
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
  const cam = await Scene.createEntity().get();
  cam.setComponent(new BuiltInComponents.Transform({ position: [0, 5, 10] }));
  cam.setComponent(new BuiltInComponents.Camera({ fovy_degrees: 60 }));
  Engine.setMainCamera(cam.id);
});

// Move camera each tick
Engine.on("tick", async ({ delta_seconds }) => {
  const t = await cam.requestComponent("core:transform").get();
  if (!t) return;
  t.addZ(-5 * delta_seconds); // fly forward along -Z
  cam.setComponent(t);
});
```

#### `BuiltInComponents.AmbientLight`

Mutable drafts for `editComponent("core:ambient_light")`. Contributes a uniform fill light to the scene. Multiple ambient lights are **additive**.

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
await Scene.addAmbientLight([1, 0.9, 0.7], 0.15).get();
```

#### `BuiltInComponents.DirectionalLight`

Mutable drafts for `editComponent("core:directional_light")`. Simulates an infinitely distant light source (e.g. the sun). Up to **4** directional lights are active simultaneously; extras are silently ignored.

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
await Scene.addDirectionalLight([0, -1, -0.3], [1, 0.95, 0.8], 1.2).get();
```

#### `BuiltInComponents.PointLight`

Mutable drafts for `editComponent("core:point_light")`. Emits light in all directions from the entity's `core:transform` position. Attenuation is quadratic — falls off to zero at `range` units. Up to **8** point lights are active simultaneously; extras are silently ignored.

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
await Scene.addPointLight([0, 3, 0], [1, 0.5, 0.1], 2.0, 15).get();
```

---

## `Physics`

Native Rapier 3D 0.36.1 runs on the simulation worker. Configure it with ordinary ECS
components; `Physics.body(reference)` binds operations without implicitly adding a body.
See [the full physics reference](PHYSICS.md) for shapes, compound bodies, joint frames,
lifecycle, timing, all native option names and supported compile-time capabilities.

```ts
const ball = Scene.createEntity().move([0, 5, 0])
  .setComponent(Physics.RigidBody.dynamic({ ccd: true }))
  .setComponent(new Physics.Collider({ shape: Physics.Shape.sphere(0.5), mass: 2 }));
Physics.body(ball).applyImpulse([0, 3, 0]);
const state = await Physics.body(ball).state().get();
```

Component constructors/factories:

- `new Physics.World(WorldOptions?)`: `core:physics_world`. `defaultWorld` is a lazy
  symbolic entity reference; `await Physics.defaultWorld.get()` returns its real entity.
- `new Physics.RigidBody({ type?, ...BodyOptions })`, or factories `dynamic`, `fixed`,
  `kinematicPosition`, `kinematicVelocity`: `core:rigid_body`.
- `new Physics.Collider(ColliderOptions)`: `core:collider`. Omitted `body` means same
  entity; explicit body references create compound objects; `body: null` is standalone.
- `new Physics.Joint({ type?, ...JointOptions })`, or factories `fixed`, `revolute`,
  `prismatic`, `spherical`, `generic`: `core:joint` on an independent joint entity.
- Read-only views: `core:rigid_body_state` (`BodyState`) and `core:physics_status` (`Status`).
- `Physics.Shape`: `sphere`, `box`, `capsule`, `cylinder`, `cone`, `plane`, `convexHull`,
  `triMesh`, `compound`. All sizes are full dimensions; capsule height includes its caps.

`Physics.BodyHandle` operations return `this`: `applyImpulse`, `applyTorqueImpulse`,
`addForce`, `addTorque`, `resetForces`, `resetTorques`, `setLinearVelocity`,
`setAngularVelocity`, `teleport`, `setKinematicTarget`, `sleep`, `wake`.
`body.get(): Promise<void>` waits for recorded mutations and reports errors;
`body.state(): Pending<BodyState>` selects state. Force/impulse options include world-space
`point` and `wake`. `setLinearVelocity` accepts selected axes to preserve the others.

`Physics.world(reference?)` returns a `WorldHandle` with
`castRay(origin, direction, RayOptions): Pending<RayHit | null>`. Directions normalize;
options include distance, solidity, groups, native filter flags and body/collider exclusions.
`Physics.onEvent(handler)` observes collision/force events after physics. The same events
are available through `Engine.on("physics", ({ event }) => ...)`. Enable events on colliders.

Public types under `Physics`: `Vec3`, `Quat`, `EntityInput`, `WorldOptions`, `BodyOptions`,
`ColliderOptions`, `JointOptions`, `IntegrationOptions`, `BodyRapierOptions`,
`ColliderRapierOptions`, `JointRapierOptions`, `ShapeData`, `BodyState`, `Status`, `Event`,
`RayHit`, `RayOptions`, `Groups`, `MassProperties`, `Motor`, `Limits`, `JointAxis`,
`LockedAxis`, `CombineRule`, `MotorModel`. Runtime constants include `capabilities`,
`QueryFilterFlag`, `CollisionType`, `TriMeshFlag`, and the six component ID constants.

This build exposes 3D rigid-body physics with enhanced determinism and serial execution.
Compile-time limitations are explicit in `capabilities`; there are no soft-body ECS
components or synchronous JavaScript physics hooks. Collision/solver groups are native.

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

Unique root commands use their plain name. When multiple sources register the same
root name, **every** participant is renamed to `<namespace>:<name>`; the plain name
becomes ambiguous and is rejected. Built-ins use `core`, and mod commands use the
mod ID. For example, a mod named `tools` registering `scene` produces `core:scene`
and `tools:scene`, independent of registration order. Subcommand paths stay intact,
and mod handlers/suggesters continue using their original, unqualified paths.
Registering the same root twice within one namespace is a fatal registration error.
In particular, a core mod command that clashes with a built-in prevents the game
from continuing to load, since both would be `core:<name>`.

The built-in `scene load <path>` instantiates any loaded mod's scene using the same
atomic loader as `Scene.loadScene`, without replacing existing entities. For example,
`scene load debug://editor` and `scene load debug://scenes/editor.wscene` load the same
file. It runs on the simulation worker before physics and publishes render updates;
invalid loads report a console error without leaving partial entities. If another
mod registers `scene`, use `core:scene load <path>` instead. The debug mod no longer
registers its own scene loader.

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

Definitions are immutable snapshots. Structs and packets use ordered, named fields; arrays accept any descriptor, including structs and optional values. States/packet names are ASCII identifiers, without dots; unsafe property names are rejected. A specification ID must use the registering mod's namespace. Define shared schemas in a local module imported by both side entries, as in `games/test/mods/network_example/scripts/network.ts`.

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

Calling `listen` again after `stop` waits for the previous listener's tasks and UDP socket to retire before binding, so reopening the same port needs no caller-side delay or retry.

`connect` accepts `host:port` or `[IPv6]:port`, resolves DNS in native code, and completes after TLS and specification negotiation. It returns `Network.ConnectionInfo {address, maxDatagramSize}`; the datagram limit is the initial negotiated limit and is checked again against the current limit on every send. `client.connected` and `client.connection` expose state. `client.disconnect()` permits reconnecting the same registration. Both sides provide `onConnect` and `onDisconnect` subscriptions; the client connect callback receives connection info, the client disconnect callback a reason, and the server disconnect callback receives the peer and reason. Reconnection creates fresh native peer handles/defaults. Old handles fail sends and writes.

`send` and `broadcast` accept optional `Network.SendOptions {transport?: "stream" | "datagram"}`. The default is one reliable, ordered application stream per connection; independent application streams are not yet exposed. Stream frames are `[unsigned ULEB128 length][u16 big-endian packet ID][payload]`, where length counts ID and payload and excludes its own prefix. Datagram frames are just `[u16 packet ID][payload]`. Datagrams are unordered, may be lost, and must fit the current QUIC limit; oversized datagrams fail without automatic fragmentation or fallback. Reliable frames are capped at 1 MiB including the ID. Strings/arrays use unsigned ULEB128 lengths; standalone VarInt remains ZigZag + ULEB128.

Each connection binds one specification. A reserved control stream exchanges `WHATEVER/1\0` plus its 32-byte SHA-256 digest before mod events are emitted. The server opens one application stream with a one-time one-byte kind marker with value `0`; subsequent bytes are the frames above. Protocol states remain metadata, with IDs unique across every state and direction within the specification. Unknown IDs, wrong directions, invalid payloads, noncanonical/oversized framing, and schema mismatches terminate the connection.

Successful sends mean validation and acceptance into a bounded local queue; they do not acknowledge remote application delivery. Each peer has 64 outgoing packet slots and a 4 MiB byte budget. Broadcast reserves capacity for every current peer before enqueueing, or fails without partial enqueue. The service uses bounded command/event queues. Connect callbacks finish before incoming packets are dispatched, and packet callbacks are acknowledged/serialized per peer; other peers continue independently. Awaiting a reply to an incoming packet inside its own handler would hold that peer's receive queue: send the request and return, then handle the reply separately. Callback errors are logged; failed connect callbacks close the peer. Disconnect/close is immediate and does not flush queued messages. Closing registrations, stopping the engine or reaping a mod closes its network resources.

### Certificates and TOFU

`trust: "strict"` validates the certificate chain, validity and server name against native OS roots. `trust: "tofu"` first attempts the same validation. If this is a new untrusted server, it accepts and remembers the server's SHA-256 public-key identity after TLS proves key possession and the specification handshake succeeds. Future connections must match that key. Servers previously CA-validated while using TOFU policy continue requiring valid CA authentication; they never silently downgrade to TOFU. Certificate renewal with the same pinned key works. The first TOFU connection has no independent protection against server impersonation.

The native engine stores server identities and trust records under the OS local-data directory `WhateverNetwork/<game_id>`, outside every mod's broker-accessible directory. Mods cannot choose certificate/key paths or rewrite the trust database. Development servers generate a persistent self-signed identity automatically. Operators can provide `--tls-cert <chain.pem> --tls-key <key.pem>` and optional client `--tls-ca <authority.pem>`. `--network-data <directory>` selects a separate native identity/trust directory. Trust entries are scoped to the normalized server host and port, independent of specification changes. An unexpected key change fails the connection; explicit trust-management UI is a later addition. No certificate fingerprint is required in the ordinary client API or commands.

## IPC protocol reference

The complete [native mod-host wire contract](IPC.md) documents compatible API/host
implementations, startup and tick sequencing, scene synchronization, nested
physics/network payloads, limits, and errors. The documentation website also
generates an exhaustive message-field catalogue from the native Rust definitions.

This section documents the raw wire format. Normal mod code should use the API
namespaces above rather than speaking the protocol directly.

Network IPC adds `NetworkRequest { request_id, request }` and `NetworkResponse { request_id, result, error }`. Requests cover Register, Encode, Decode, ValidateClientData, UpdateClientData, Listen, Connect, Stop, Disconnect, Send, Broadcast, EventDone and Close; byte buffers use base64 inside this private bridge. The native engine owns codec execution. Host protocol is 10. Native `NetworkEvent` messages carry connected/disconnected/packet events, registration and peer IDs, and bounded per-peer callback acknowledgement IDs.

**Transport:** NDJSON (one JSON object per line) on stdin/stdout per process. Script application commands must be inside `CommandBatch { tick_number, commands }`; raw commands and nested batches are rejected. The only startup envelopes are `RuntimeReady { protocol: 10 }` and `RuntimeFailed { message }`. Engine command replies and mod messages are grouped into `CommandResults { tick_number, messages }` after execution. Shutdown may flush a final buffer for diagnostic logs; state-changing commands from that final buffer are discarded.
Stderr lines are forwarded to the engine logger with a `[mod_id]` prefix. Engine-to-mod pipe writes run on a dedicated bounded writer thread (256 queued messages, 64 MiB including the in-flight write). A full/broken writer terminates the affected mod rather than blocking the simulation or silently losing RPCs.

### Engine → Script

| Message | Key fields |
|---|---|
| `CommandResults` | `tick_number`, `messages` (the replies below) |
| `NetworkResponse` | `request_id`, `result`, `error` |
| `PhysicsResponse` | `request_id`, `result`, `error` |
| `PhysicsEvent` | `event` (generational references, collision/force data, tick and substep) |
| `MainCameraResponse` | `request_id`, `entity_id` (string or null) |
| `NetworkEvent` | `event`, `registration_id`, `client_id`, `event_id` when acknowledged, event-specific data |
| `Init` | `mod_id`, `engine_version`, `mode` |
| `Tick` | `tick_number`, `delta_seconds`, `keys_pressed` (key-code array on the wire), `mouse_delta`, `mouse_captured`, `mouse_buttons`, `mouse_buttons_pressed` |
| `FileResponse` | `request_id`, `data_base64`, `error` |
| `ModListResponse` | `request_id`, `mods` |
| `ModGetResponse` | `request_id`, `manifest`, `error` |
| `ModMessageReceived` | `source_mod_id`, `request_id`, `payload` |
| `ModMessageReplyDelivered` | `request_id`, `payload` |
| `EntityCreated` | `request_id`, `entity_id` |
| `SceneLoaded` | `request_id`, `scene` (`entity_ids`, `script_ids`, `load_time`) or null, `error` |
| `EntityListResponse` | `request_id`, `entity_ids` |
| `EntityParentResponse` | `request_id`, `entity_id`, `parent_id` (string or null) |
| `EntityChildrenResponse` | `request_id`, `entity_id`, `child_ids` |
| `SceneDelta` | `delta`: revision, base_revision, tick_number, reset, camera, created records, removed IDs, changed metadata/components/instances |
| `Performance` | `sample`: native `TickTimings` after commit |
| `EntityTagsResponse` | `request_id`, `tags`, `error` |
| `ComponentGetResponse` | `request_id`, `entity_id`, `component_type`, `data`, `error` |
| `ComponentQueryResponse` | `request_id`, `results` |
| `CommandInvoke` | `request_id`, `command_path`, `args` |
| `ArgSuggestRequest` | `request_id`, `command_path`, `arg_index`, `current` |
| `Shutdown` | `exit_code` |

### Script → Engine

| Message | Key fields |
|---|---|
| `NetworkRequest` | `request_id`, `request` (operation DTO) |
| `PhysicsRequest` | `request_id`, `request` (`worldEntity`, `state`, `body`, `castRay` operation DTO) |
| `GetMainCamera` | `request_id` |
| `Subscribe` | `events` |
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
| `SceneLoad` | `request_id`, `path` (VFS scene path) |
| `EntityDestroy` | `entity_id` |
| `EntityMove` | `entity_id`, `position` (preserves rotation/scale in native code) |
| `EntityListRequest` | `request_id` |
| `EntitySetParent` | `entity_id`, `parent_id` (string or null) |
| `EntityGetParent` | `request_id`, `entity_id` |
| `EntityGetChildren` | `request_id`, `entity_id` |
| `ComponentSet` | `entity_id`, `component_type`, `data` (native or registered custom schema only) |
| `ComponentPatch` | `entity_id`, `component_type`, `data` (changed top-level custom fields) |
| `EntityTagAdd` / `EntityTagRemove` | `entity_id`, `tag` |
| `EntityTagsGet` | `request_id`, `entity_id` |
| `EntityTagQuery` | `request_id`, `query` (`all`, `any`, `none`, `includeDisabled`) |
| `ComponentRemove` | `entity_id`, `component_type` |
| `ComponentGet` | `request_id`, `entity_id`, `component_type` |
| `ComponentQuery` | `request_id`, `component_types` |
| `RegisterCommand` | `name`, `description`, `subcommands`, `args`, `has_handler` |
| `CommandResponse` | `request_id`, `output`, `error` |
| `ArgSuggestResponse` | `request_id`, `suggestions` |

## Mod sides and script entries

The `[mod].sides` list defaults to both `client` and `server`. Inactive mods are removed before dependency validation. The `[script]` section may contain optional `entry`, `client` and `server` paths: a side entry overrides `entry`; without either, that mod has no script on that side. `Mods.list()` and `Mods.get()` expose only active mods, with `sides: RuntimeMode[]` and nullable script entry paths. `RuntimeMode` is exported as a public type.


A synchronous throw or rejected promise from a tick handler is queued for logging and terminates that mod process after its next buffer submission. The runtime removes exited processes from the submission barrier so other mods can continue. Shutdown interrupts a waiting tick and cleans up remaining subprocesses.

## Entity lifecycle and startup scenes

`Entity.clone(options?: { enabled?: boolean; recursive?: boolean }): PendingEntity`
copies authored components and descendants with fresh IDs, remapping internal entity
references. Native physics state is rebuilt. The original enabled state is preserved
unless overridden. `Entity.setEnabled(enabled: boolean): this` changes local enabled
state; `Entity.getEnabled(): boolean` reads it. `core:entity` is read-only
metadata containing `enabled` and inherited `active` flags. Disabled ancestors make
their descendants inactive: no rendering, lighting, camera or physics simulation.
Mod tick callbacks are global; they continue running, so scripts can enable entities.

`Scene.initialScene(): LoadedScene | null` returns the scene preloaded before Init
by the core mod's top-level `initial_scene = "my_game://main"` setting in mod.toml.
`LoadedScene.path` identifies its source. Mods can attach behavior to those IDs
instead of instantiating the scene again. Startup chooses its first enabled camera;
scripts can select another camera normally. Init now includes optional initial_scene
metadata; the matching host protocol is 7.

New joints belong on their first rigid body. Construct `Physics.Joint.fixed({
connectedBody: otherBody })` (or revolute/prismatic/spherical/generic) and attach it
to the owning body. Local A anchors describe the owner; B anchors describe the
connected body. Legacy bodyA/bodyB descriptors remain readable for older mods.

Component queries omit inactive entities. Direct component reads and scene-local lookups still access disabled templates.

`Entity.getComponent("core:world_transform"): ComponentSnapshot<BuiltInComponents.Transform> | null` reads the inherited world transform. This derived component is read-only.
