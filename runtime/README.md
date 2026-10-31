# @whatever-engine/api

The npm package contains the scripting bundle and TypeScript declarations. The native mod host is supplied separately by the editor/player distribution.

TypeScript scripting API for Whatever Engine mods. Abstracts the NDJSON IPC protocol between the engine and Bun script subprocesses.

The bundled host delivers complete UTF-8 batches over ordinary engine pipes; its transport handles remain private.

## Usage

```ts
import { Engine, Window, File, Scene, Entity, Physics, Mods, Message, Console, Audio } from "@whatever-engine/api";

Engine.on("init", ({ mod_id }) => {
  Engine.log("info", `loaded as ${mod_id}`);
});

Engine.on("tick", async ({ delta_seconds }) => {
  // Record commands. Awaiting results does not stop automatic batch submission.
});
```

The engine-owned mod host contains this API and resolves `@whatever-engine/api` before loading the mod. No runtime install or workspace ancestry is required. Leave this import external when bundling mods. The npm package remains available for editor types.

Other package imports, including `node:` and `bun:` modules, are denied initially. Mods may import local JS/TS/JSON source inside their own directory. Direct Bun/process/network globals are unavailable; use the engine API for file I/O and logging. See [`docs/SANDBOX.md`](../docs/SANDBOX.md) for the native allowlists and build instructions.

## API

### `Engine`

- `Engine.mode` — read-only `"client" | "server"`, available before top-level evaluation
- `Engine.on(event, handler)` — subscribe to an engine event (`init`, `exit`, `tick`, `mod_message`, `physics`)
- `Engine.log(level, message)` — log through the engine logger (`"info"`, `"warn"`, `"error"`)
- `Engine.setTickRate(ticks_per_second)` — override the game tick rate at runtime
- `Engine.setFpsCap(fps | null)` — set FPS cap; `null` = uncapped
- `Engine.setVsync(enabled)` — enable or disable vertical sync
- `Engine.setMainCamera(entityOrId)` — designate a live or pending entity as the active camera
- `Engine.getMainCamera()` → `Pending<Entity | null>`
- `Engine.clearCamera()` — remove the active camera (screen goes black with a warning)

Tick payloads include `mouse_captured`; `mouse_delta` is `[0, 0]` while uncaptured.
Pending mouse motion is discarded on release, including for busy async handlers.

### `Window`

- `Window.setTitle(title)` — change the window title
- `Window.setSize(width, height)` — request a new inner size in physical pixels (no-op in fullscreen modes)
- `Window.setMode(mode)` — set display mode: `"windowed"`, `"borderless"`, or `"fullscreen"`

### `File`

Sandboxed per-mod file I/O. Paths must not contain `..`.

- `File.write(path, data)` → `Promise<void>`
- `File.read(path)` → `Promise<string>`
- `File.delete(path)` → `Promise<void>`

### `Pending<T>` / `PendingEntity`

- `pending.get()` → `Promise<T>` — wait for the recorded result; never flushes or repeats work
- `PendingEntity` — an entity reference with chainable mutations before its native ID exists; queries return their own `Pending<T>`

All API commands are submitted automatically once per tick. See [tick timing and migration](../docs/SCRIPTING.md#tick-order-and-command-batches).

### `Entity`

A live entity in the scene. Returned by `Scene.createEntity`, `Scene.listEntities`, `Scene.query`, and `Scene.spawnSprite`. For built-in component types the data parameter/return is automatically typed; for custom component types a generic `T` can be supplied.

- `entity.id` — opaque entity ID string
- `entity.destroy()` — fire-and-forget
- `entity.setComponent(component_type, data)` / `entity.setComponent(component)` → `this` — typed for built-in components
- `entity.removeComponent(component_type)` → `this`
- `entity.getComponent(component_type)` → `Pending<T | null>` — typed for built-in components
- `entity.move(position)` → `this` — update `core:transform` position, preserve rotation/scale
- `entity.setParent(parent | null)` → `this` — accepts entities, references or IDs
- `entity.getParent()` → `Pending<Entity | null>` — return the parent entity
- `entity.getChildren()` → `Pending<Entity[]>` — return all direct children

### `EntityRef` / `EntityReference`

- `new EntityRef(entityOrId)` / `Scene.ref(entityOrId)` — typed references in nested component data
- `reference.id` — opaque ID; unavailable while its target is pending
- `ComponentValue` — component JSON with typed entity references
- `ComponentRegistry` / `ComponentSetRegistry` — augment custom component read/write types

### `Scene`

Entity and component management. Methods that accept a raw `entity_id` string are provided for cases where only an ID is available; prefer `Entity` methods when possible.

- `Scene.createEntity()` → `PendingEntity`
- `Scene.loadScene(path)` → `Promise<LoadedScene>`
- `Scene.ref(entityOrId)` → `EntityRef`
- `Scene.destroyEntity(entity_id)` — fire-and-forget
- `Scene.listEntities()` → `Pending<Entity[]>`
- `Scene.setComponent(entity_id, component_type, data)` — fire-and-forget; typed for built-in components
- `Scene.removeComponent(entity_id, component_type)` — fire-and-forget
- `Scene.getComponent(entity_id, component_type)` → `Pending<T | null>` — typed for built-in components
- `Scene.query(component_types)` → `Pending<QueryResult<Entity>[]>` — each row has `entity: Entity` and `components`
- `Scene.spawnSprite(texture, shader, position, scale?)` → `PendingEntity` — convenience
- `Scene.spawnMesh(mesh, shader, position, options?)` → `PendingEntity` — convenience
- `Scene.spawnText(text, position, options)` → `PendingEntity` — convenience
- `Scene.moveEntity(entity_id, position)` → `void` — convenience
- `Scene.addAmbientLight(color?, intensity?)` → `PendingEntity` — convenience
- `Scene.addDirectionalLight(direction, color?, intensity?)` → `PendingEntity` — convenience
- `Scene.addPointLight(position, color?, intensity?, range?)` → `PendingEntity` — convenience
- `Scene.setParent(entity_id, parent_id | null)` — fire-and-forget
- `Scene.getParent(entity_id)` → `Pending<string | null>`
- `Scene.getChildren(entity_id)` → `Pending<string[]>`

Built-in component types: `core:transform`, `core:sprite_renderer`, `core:mesh_renderer`, `core:text_renderer`, `core:camera`, `core:ambient_light`, `core:directional_light`, `core:point_light`. `getComponent` for built-in types returns a **live class instance** with methods — not a plain object.

### `LoadedScene`

- `scene.get(scriptId)` → `Entity`
- `scene.entities()` → `Entity[]`
- `scene.entityCount` → `number`
- `scene.loadTime` → `number` (milliseconds)

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

### `Physics`

Native Rapier 3D; see the [full physics reference](../docs/PHYSICS.md).

- `new Physics.World(options?)` — `core:physics_world`
- `Physics.defaultWorld.get()` → `Promise<Entity>`; `Physics.capabilities`
- `Physics.RigidBody.dynamic(options?)`, `.fixed(options?)`, `.kinematicPosition(options?)`, `.kinematicVelocity(options?)` — `core:rigid_body`
- `new Physics.Collider(options)` — `core:collider`
- `Physics.Joint.fixed(options)`, `.revolute(options)`, `.prismatic(options)`, `.spherical(options)`, `.generic(options)` — `core:joint`
- `Physics.Shape.sphere(radius)`, `.box(options)`, `.capsule(options)`, `.cylinder(options)`, `.cone(options)`, `.plane(normal?)`, `.convexHull(points)`, `.triMesh(vertices, indices, flags?)`, `.compound(parts)`
- `Physics.body(entityOrId)` → `BodyHandle`
- `body.applyImpulse(vector, options?)`, `.applyTorqueImpulse(vector, wake?)`, `.addForce(vector, options?)`, `.addTorque(vector, wake?)`
- `body.resetForces(wake?)`, `.resetTorques(wake?)`, `.setLinearVelocity(vector, options?)`, `.setAngularVelocity(vector, wake?)`
- `body.teleport(position, options?)`, `.setKinematicTarget(position, rotation?)`, `.sleep()`, `.wake()`
- `body.get()` → `Promise<void>`; `body.state()` → `Pending<Physics.BodyState>`
- `Physics.world(entityOrId?)` → `WorldHandle`; `world.castRay(origin, direction, options)` → `Pending<Physics.RayHit | null>`
- `Physics.onEvent(handler)`; `Physics.QueryFilterFlag`, `Physics.CollisionType`, `Physics.TriMeshFlag`
- Read-only ECS views: `core:rigid_body_state` / `Physics.BodyState`, `core:physics_status` / `Physics.Status`
- Typed expert settings: `Physics.IntegrationOptions`, `BodyRapierOptions`, `ColliderRapierOptions`, `JointRapierOptions`

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

Unique commands use plain names. Collisions qualify all participants as `mod_id:name` (built-ins use `core:name`). Duplicate commands within one namespace, including a core mod clashing with a built-in, stop loading.

- `Console.register(spec)` — register developer console commands with typed args and subcommands. Argument types: `string`, `int`, `float`, `bool`, `mod_id`, `entity_id`. The engine completes and validates loaded mod IDs and living entity IDs; handlers receive ID strings. `bool` suggests `false`/`true` by default. Open the console with F2; Up/Down previews suggestions and Tab accepts.

### `Network`

- `Network.Type` — `I32`, `F64`, signed 32-bit ZigZag `VarInt`, `Bool`, `String`, `Array(type, options?)`, `Optional(type)`
- `Network.Type.I32/F64/VarInt.with({ min?, max? })` — immutable numeric constraints
- `Network.Type.String.with({ maxLength })` — UTF-8 byte limit; array `maxLength` counts elements
- `Network.Struct(fields)` / `Network.Packet(fields)` / `Network.Specification(id, states)` — immutable definitions
- `Network.Infer<typeof descriptor>` — inferred payload/client-data type
- `Network.registerClient(spec)` / `Network.registerServer(spec, { clientData, initialClientData })` — await native validation/registration
- `registration.packets` / `id` / `specificationId` / `fingerprint` — engine-assigned metadata
- `registration.encodePacket(name, payload)` / `decodePacket(bytes)` — native offline codecs; packet ID and payload, no stream length prefix
- `registration.on(name, handler)` — typed receive subscription; returns unsubscribe
- `registration.send(...)` / `server.broadcast(...)` — validated bounded-queue sends; optional `{transport: "stream" | "datagram"}`
- `server.onConnect(handler)` / `onDisconnect(handler)` / `clients` / `getClient(id)` — typed connection lifecycle and owned clients
- `server.validateClientData(data)` — offline native validation/copy
- `client.data` / `client.flushData()` — typed per-peer mutation proxy and native update acknowledgement
- `client.connect({address, trust?: "strict" | "tofu"})` / `disconnect()` / `connected` / `connection` — QUIC lifecycle; strict by default
- `client.onConnect(handler)` / `onDisconnect(handler)` — lifecycle subscriptions
- `server.listen({address, maxClients?})` / `stop()` / `listening` / `listener` — native QUIC listener
- `server.disconnect(client, reason?)` — disconnect an owned peer
- `Network.PacketContext` — receive transport metadata (client second argument, server third)
- `Network.ConnectOptions` / `ListenOptions` / `SendOptions` / `TrustPolicy` — public option types
- `registration.close()` — disconnect peers and release native registration

## Development

```sh
# Regenerate type declarations after editing index.ts
bun run build:types
```

Full API documentation is in [`docs/SCRIPTING.md`](../docs/SCRIPTING.md) at the repo root.

The host captures private native pipe handles before lockdown; mods do not receive stdin, stdout or process objects. The public TypeScript declarations remain independent of Bun internals.


Init payloads include `mode`. Simulation ticks run independently of rendering. Window/audio operations and presentation settings reject server use. Mod manifests expose supported `sides` and optional shared/client/server script entries; only mods active on the current side appear in `Mods.list()` and `Mods.get()`.


Tick handler failures are logged and stop the faulty mod; exited processes are removed from the command-submission barrier. Host protocol 7 uses `CommandBatch` and `CommandResults` envelopes.

## Entity lifecycle and startup scenes

- `Entity.clone(options?: CloneOptions): PendingEntity`
- `Entity.setEnabled(enabled: boolean): this`
- `Entity.getEnabled(): Pending<boolean>`
- `Scene.initialScene(): LoadedScene | null`
- `LoadedScene.path: string`
- `Physics.Joint.fixed({ connectedBody, ...options }): Physics.Joint`
- `Physics.Joint.revolute({ connectedBody, ...options }): Physics.Joint`
- `Physics.Joint.prismatic({ connectedBody, ...options }): Physics.Joint`
- `Physics.Joint.spherical({ connectedBody, ...options }): Physics.Joint`
- `Physics.Joint.generic({ connectedBody, ...options }): Physics.Joint`

- `Entity.getComponent("core:world_transform"): Pending<BuiltInComponents.Transform | null>` — read-only inherited transform
