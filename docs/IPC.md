# Mod host IPC protocol

This is the engine-to-mod-host protocol for implementing a compatible scripting
API or trusted host integration. The current host protocol is **10**. It is
independent of the engine release version, scene file version, and QUIC network
protocol.

For the supplied TypeScript implementation, see [Scripting API](SCRIPTING.md).
For the execution boundary, see [Mod runtime policy](SANDBOX.md).

## What can be replaced today

`@whatever-engine/api` implements this protocol inside an engine-owned
`whatever-mod-host` process. It adds convenient entities, pending results,
component lifecycle callbacks, synchronous scene reads, and typed networking.
Those JavaScript abstractions are not the wire format.

The current engine does **not** let a mod choose an arbitrary executable or gain
access to raw stdin/stdout. The bundled host embeds the API and captures its
private transport before enabling the native sandbox. A normal mod can import
the embedded API and local JS/TS/JSON source; it cannot import Node/Bun modules,
replace the embedded package, or acquire native pipe handles. A local wrapper
around the public API works under the existing rules. Replacing the wire client
itself requires an engine-owned, compatible host integration and rebuild; this
document does not introduce a permission grant or runtime-selection mechanism.

## Canonical definitions

Use these definitions together. The main IPC enum deliberately delegates some
payloads to native subsystems.

| Contract | Source |
| --- | --- |
| Envelopes, commands, responses, console DTOs, mod manifest DTO | [engine/src/script/ipc.rs](../engine/src/script/ipc.rs) |
| Host discovery, metadata validation and protocol constant | [engine/src/script/runtime.rs](../engine/src/script/runtime.rs) |
| Process pipes, decoding, deferred response grouping | [engine/src/script/host.rs](../engine/src/script/host.rs) |
| Bounded engine-to-host writer | [engine/src/script/writer.rs](../engine/src/script/writer.rs) |
| Startup, tick barrier and publication order | [engine/src/runtime/simulation.rs](../engine/src/runtime/simulation.rs) |
| Command execution, storage and inter-mod routing | [engine/src/script/api.rs](../engine/src/script/api.rs) |
| Scene snapshots and deltas | [engine/src/ecs/snapshot.rs](../engine/src/ecs/snapshot.rs) |
| Entity references and tag queries | [references.rs](../engine/src/ecs/references.rs), [tags.rs](../engine/src/ecs/tags.rs) |
| Shared schema wire representation | [engine/src/schema/value.rs](../engine/src/schema/value.rs) |
| Physics request operations and events | [engine/src/physics/registry.rs](../engine/src/physics/registry.rs) |
| Network operations and event delivery | [registry.rs](../engine/src/network/registry.rs), [lifecycle.rs](../engine/src/network/lifecycle.rs) |
| Scene-load result DTO | [engine/src/scene/document.rs](../engine/src/scene/document.rs) |
| Performance notification DTOs | [engine/src/runtime/performance.rs](../engine/src/runtime/performance.rs) |
| Supplied host bootstrap and API implementation | [runtime/host/index.ts](../runtime/host/index.ts), [runtime/src/ipc.ts](../runtime/src/ipc.ts) |

## Transport and encoding

Each scripted mod has one host process. The engine writes messages to the
process's stdin and reads packets from its stdout. The engine knows the sending
mod from the process; a packet does not authenticate itself with a `mod_id`.

Transport is UTF-8 newline-delimited JSON (NDJSON): one complete JSON object per
line, terminated by `\n`. `type` is a case-sensitive string discriminator.
Properties use the exact names in the native definitions, usually `snake_case`.
Embedded newlines must be JSON escapes. A pipe read may contain part of a line,
many lines, or a split UTF-8 code point; decoding and framing must preserve all
three cases. Flush complete packets promptly. Do not pretty-print JSON across
physical lines, add banners, or emit ordinary logs on stdout.

Use a `Log` command for normal logging. Host stderr is forwarded to the engine
logger with the mod identity. The supplied host replaces `console` methods with
API logging before evaluating mod code.

Rust integer fields retain their declared ranges: tick/revision counters are
`u64`, for example. Do not silently round counters in an implementation with a
smaller exact integer range. JSON values cannot represent NaN or infinities.
Vectors and quaternions are fixed-length JSON arrays. Optional native response
fields generally serialize as explicit `null`; fields marked
`skip_serializing_if` may be omitted. `null` and missing are not interchangeable
for every payload, especially scene changes.

## Host startup

Before launching mods, the engine runs the selected bundled executable with
`--runtime-info`. Its stdout is a single JSON metadata object, followed by a
newline. It has **no `type` field**:

```json
{"runtime":"whatever-mod-host","protocol":10,"native_policy":1,"api_version":"0.7.0","bun_version":"1.4.4"}
```

The API version in that example is illustrative: it must equal the engine's
actual release version. The engine also checks the runtime identity, protocol,
native policy and pinned Bun version. Release deployment rejects a debug Bun
build. This metadata preflight is separate from the in-process ready handshake.

The engine launches the host with the script entry, mod root, runtime mode
(`client` or `server`), and mod ID as arguments. The trusted bootstrap sets the
mode **before evaluating mod modules**, inspects and registers component
definitions, installs the embedded API and restricted console, enables native
lockdown, then evaluates the entry. The process environment is cleared except
for the engine's fixed runtime settings and supported OS/temp variables.

On successful evaluation, the host emits:

```json
{"type":"RuntimeReady","protocol":10}
```

On startup failure, it emits and exits:

```json
{"type":"RuntimeFailed","message":"Mod host startup failed: explanation"}
```

The engine waits for the active hosts to complete their startup handshakes. A
wrong protocol, an early command batch, or another unexpected startup packet
terminates that host. After readiness, the engine sends an initial resetting
`SceneDelta` at tick zero, then sends `Init` in mod dependency/load order:

```json
{"type":"SceneDelta","delta":{"revision":1,"base_revision":0,"tick_number":0,"reset":true,"camera":null,"created":[],"removed":[],"changed":[]}}
{"type":"Init","initial_scene":null,"mod_id":"my_mod","engine_version":"0.7.0","mode":"client"}
```

`initial_scene` is either `null` or the scene-load result described below. It is
not the world snapshot: the preceding `SceneDelta` supplies that snapshot. The
world may already contain an authored startup scene. Client initialization also
waits for client readiness, including its window; the simulation worker owns
the protocol independently of redraws.

Do not wait for an engine-backed result during top-level module evaluation.
Commands queued during evaluation or `Init` can only execute after the first
`Tick`; awaiting them before `RuntimeReady` deadlocks startup.

## One tick, one batch

The engine sends every ready host a `Tick`:

```json
{"type":"Tick","tick_number":1,"delta_seconds":0.016666666666666666,"keys_pressed":["KeyW"],"mouse_delta":[2,-1],"mouse_captured":true}
```

`delta_seconds` is the configured simulation interval, not the duration of a
window redraw. Input is sampled by the engine. Use `mouse_captured` when deciding
whether to apply mouse motion. A headless server has no client window input.
`keys_pressed` stays an array of physical key-code strings on the wire; the API
converts it into an immutable `PressedKeys` snapshot for public tick handlers.
Before component hooks and public tick handlers run, it also updates the shared
read-only `Input` and `Time` API objects, even without a public tick listener.
These objects describe one current tick; existing handler arguments still
accumulate elapsed time and mouse motion when async handlers skip ticks.

Each host must answer with **exactly one** batch with the same tick number:

```json
{"type":"CommandBatch","tick_number":1,"commands":[]}
```

An empty batch still acknowledges the tick. Only `RuntimeReady`,
`RuntimeFailed`, and `CommandBatch` are top-level script packets. Application
commands such as `Log`, `EntityCreate`, and `NetworkRequest` are valid only in
`commands`; startup packets and nested batches are not valid commands.

The engine collects all ready hosts' batches, then executes them in mod
dependency/load order, independent of pipe arrival order. Within one mod, it
executes commands in array order. A batch is an ordered command list, **not an
atomic transaction**: a failed command does not roll back earlier commands.

The tick proceeds through these phases:

1. Gather input, pending controls, network events and finished-audio events.
2. Send `Tick` and collect one matching batch from each ready host.
3. Execute mod batches in dependency/load order, then queued editor mutations.
4. Step native physics and collect physics events.
5. Publish the committed `SceneDelta` to ready hosts.
6. Publish presentation state and flush deferred `CommandResults` per mod.
7. Send a `Performance` sample.

An engine request issued during a handler therefore cannot resolve before that
handler's current batch is submitted. Keep the transport responsive while user
promises await results. The supplied API submits a batch without awaiting user
tick-handler promises; a busy async handler is not reentered, and elapsed tick
delta and captured mouse motion accumulate for its next invocation. This
scheduling policy is an API implementation choice, not an extra wire message.

Commands recorded by result callbacks, timers or async continuations belong to
a subsequent batch. Never submit a second batch for the completed tick.

## Requests, results and execution time

Request IDs are opaque strings chosen by the sending host. Keep outstanding IDs
unique within that host, retain a pending-result map, and match replies by
`request_id`. Numeric strings from a monotonic counter are a convention of the
supplied API, not a wire restriction.

Many commands have a dedicated result type. For example:

```json
{"type":"CommandBatch","tick_number":2,"commands":[{"type":"EntityCreate","request_id":"create-1"},{"type":"ComponentSet","entity_id":"pending:create-1","component_type":"core:transform","data":{"position":[0,1,0],"rotation":[0,0,0,1],"scale":[1,1,1]}},{"type":"ComponentGet","request_id":"read-1","entity_id":"pending:create-1","component_type":"core:transform"}]}
```

After physics, the engine publishes a scene delta, then a result envelope:

```json
{"type":"CommandResults","tick_number":2,"messages":[{"type":"EntityCreated","request_id":"create-1","entity_id":"0:0"},{"type":"ComponentGetResponse","request_id":"read-1","entity_id":"pending:create-1","component_type":"core:transform","data":{"position":[0,1,0],"rotation":[0,0,0,1],"scale":[1,1,1]},"error":null}]}
```

The allocated entity ID is illustrative. `messages` contains normal
engine-message objects, dispatched in order. The engine sends a
`CommandResults` envelope only to owners with deferred messages; do not wait
for an empty envelope after every batch. Notifications and asynchronous
responses may also arrive outside an envelope.

Publishing replies after physics does **not** make every reply a post-physics
query. `ComponentGet` and physics requests are evaluated at their own position
in command execution. Their queued response records that result, while the
subsequent scene delta describes the committed post-physics world.

There is no universal success acknowledgment. Fire-and-forget commands can
reject invalid data with a native diagnostic, or do nothing. Commands with an
`error` field normally report failure as a non-null string. Missing component
data with `error:null` is a valid query result. Do not infer that a `null` result
always means failure.

## Entity identity and references

Native entity IDs are strings in `index:generation` form, such as `12:3`.
Generation is part of identity; a freed slot reused for a new entity is not the
old entity. Keep IDs opaque outside a native-ID parser.

`EntityCreate` and successful `EntityClone` register `pending:<request_id>` in
a table scoped to **the current mod's current batch**. Later commands in that
same batch can use that alias. Resolve it from `EntityCreated` before referring
to the entity in later batches. The engine does not retain pending aliases
across ticks or share them across mods.

Entity references inside component or physics JSON use an explicit marker:

```json
{"target":{"$entity":"pending:create-1"},"label":"pending:create-1"}
```

Only the tagged reference is resolved; the label remains an ordinary string.
A reference object must contain exactly one `$entity` key whose value is a
string. The native reference resolver traverses nested arrays and objects.
`default:physics_world` is a special engine-resolved reference that lazily
creates the default physics-world entity when supported commands need it.

`ComponentSet` supplies a complete component value and runs native validation.
`ComponentPatch` shallowly merges top-level fields into an existing component
before applying the normal set path; it is not a recursive JSON merge patch.
Unknown, missing or read-only component types can be rejected. Schemas and
custom component definitions are registered by engine-owned source inspection,
not by a permission-bearing IPC command from the mod.

## Revisioned scene synchronization

`SceneDelta` is broadcast to ready hosts after command execution and physics,
even when there are no entity changes. Its shape is:

```ts
interface SceneDelta {
  revision: number;
  base_revision: number;
  tick_number: number;
  reset: boolean;
  camera: string | null;
  created: EntitySnapshot[];
  removed: string[];
  changed: EntityDelta[];
}
interface EntityMetadata {
  name: string;
  enabled: boolean;
  active: boolean;
  parent: string | null;
  children: string[];
  tags: string[];
}
interface EntitySnapshot {
  id: string;
  metadata: EntityMetadata;
  components: Record<string, unknown>;
  instances: Record<string, string>;
}
interface EntityDelta {
  id: string;
  metadata?: EntityMetadata;
  components: Record<string, unknown | null>;
  instances: Record<string, string | null>;
}
```

The TypeScript `number` spelling above illustrates JSON shapes; native counter
ranges remain `u64`. `components` maps namespaced component IDs to data.
`instances` maps component types to native instance IDs, not JavaScript objects.

Apply a resetting delta by clearing the previous scene and installing the
supplied state. Otherwise require `base_revision` to equal the locally applied
revision. Remove the IDs in `removed`, install `created` records, then apply
`changed` records. In a change, omitted metadata means unchanged metadata;
present metadata replaces that record. A missing component/instance key means
unchanged; an explicit `null` deletes the entry; another value replaces it.
`camera` always supplies the current main-camera ID or `null`.

Keep native committed state separate from any local prediction. The supplied
API overlays staged commands for immediate synchronous reads and retires
submitted operations once the committed delta reaches their tick. This overlay,
immutable snapshots, `PendingEntity`, and `.get()` helpers are library
features. A simpler client can read only committed state and still speak the
same protocol. Do not report predicted mutations as native acknowledgments.

There is no public IPC command requesting a replacement full snapshot after a
revision gap. Detect a gap as a synchronization failure; do not silently apply
an incompatible delta or invent a resynchronization packet.

## Command families and unsolicited messages

The native enum defines the exact fields for every entry. This table explains
how the families interact; it does not replace those field definitions.

| Family | Commands | Results and notifications |
| --- | --- | --- |
| Lifecycle and diagnostics | `Subscribe`, `Log` | `Init`, `Tick`, `Shutdown`, `Performance` |
| Scene and camera | `SceneLoad`, `SetMainCamera`, `GetMainCamera` | `SceneLoaded`, `MainCameraResponse`, `SceneDelta` |
| Entities and hierarchy | `EntityCreate`, `EntityClone`, `EntityDestroy`, `EntityMove`, `EntitySetEnabled`, `EntityListRequest`, `EntitySetParent`, `EntityGetParent`, `EntityGetChildren` | `EntityCreated`, `EntityCreateFailed`, `EntityListResponse`, `EntityParentResponse`, `EntityChildrenResponse` |
| Tags | `EntityTagAdd`, `EntityTagRemove`, `EntityTagsGet`, `EntityTagQuery` | `EntityTagsResponse`, `EntityListResponse` |
| Components | `ComponentSet`, `ComponentPatch`, `ComponentRemove`, `ComponentGet`, `ComponentQuery` | `ComponentGetResponse`, `ComponentQueryResponse`, `SceneDelta` |
| Persistent files | `FileWrite`, `FileRead`, `FileDelete` | `FileResponse` |
| Mod discovery | `ModListRequest`, `ModGetRequest` | `ModListResponse`, `ModGetResponse` |
| Inter-mod messages | `ModMessageSend`, `ModMessageReply` | `ModMessageReceived`, `ModMessageReplyDelivered` |
| Console | `RegisterCommand`, `CommandResponse`, `ArgSuggestResponse` | `CommandInvoke`, `ArgSuggestRequest` |
| Physics | `PhysicsRequest` | `PhysicsResponse`, `PhysicsEvent` |
| Network | `NetworkRequest` | `NetworkResponse`, `NetworkEvent` |
| Audio | `AudioLoad`, `AudioPlay`, `AudioPause`, `AudioStop`, `AudioSeekTo`, `AudioSeek`, `AudioSetLoop`, `AudioQuery` | `AudioLoaded`, `AudioState`, `AudioClose` |
| Client presentation and pacing | `SetWindowTitle`, `SetWindowSize`, `SetWindowMode`, `SetTickRate`, `SetFpsCap`, `SetVsync` | No general success response |

Important payload details:

- `Subscribe.events` contains native names such as `"PhysicsEvent"`, not public
  API names such as `"physics"`. The current dispatcher uses it to enable physics
  notifications. Core tick and scene publication do not require subscribing.
- `SceneLoaded.scene` contains `path`, `entity_ids`, `script_ids`, and `load_time`.
  `script_ids` maps authored scene identifiers to allocated runtime IDs. Scene
  files have their own version and are loaded atomically by the scene service.
- Tag queries use `all`, `any`, `none`, and `includeDisabled`. Tags are limited
  to 32 per entity, each 1–128 UTF-8 bytes with no control characters.
- `ComponentQueryResponse.results` contains rows with `entity_id` and a
  `components` map. Do not confuse this with the supplied API's entity objects.
- Files use standard Base64 in `data_base64`. Paths address the current mod's
  persistent storage broker, not arbitrary paths or VFS assets. A successful
  write/delete has `data_base64:null` and `error:null`.
- Inter-mod payloads are JSON. For a request expecting a reply, the engine
  replaces the recipient's `request_id` with a namespaced routing key. Reply
  using that key unchanged; the sender receives its original ID. A null request
  ID means no reply was requested. Unknown targets can be logged without a
  response. Timeouts in `Message.send` are supplied-library behavior.
- Console command trees use `name`, `description`, `subcommands`, `args`, and
  `has_handler`. An argument's discriminator is `type`, with `required`,
  `description`, and `has_suggest`. Respond to console invocation/suggestion
  notifications through a subsequent command batch.
- Audio wire keys include **`loop_`** and `close_strategy`; they are not the
  friendly TypeScript option names. Time positions and offsets use milliseconds.
  `AudioClose` can occur asynchronously when a sound finishes.
- Clearing the main camera uses `SetMainCamera` with `entity_id:""`.
  `SetFpsCap.fps:null` removes the cap. Window changes are unavailable in server
  mode; use the mode supplied by the trusted host before creating client UI.

### Physics nested requests

`PhysicsRequest` is an application command containing `request_id:string` and
`request:object`. Its nested `op` discriminator uses **camelCase**. Unknown
request fields are rejected. In the tables below, `?` means the field can be
omitted, `= ...` gives its omission default, `Ref` means `{"$entity":"ID"}`,
`Vec3` means three finite f32 numbers, and `Quat` means four finite f32 numbers
in xyzw order. Nonzero quaternions are normalized by the engine.

| `op` | Fields besides `op` | Successful `PhysicsResponse.result` |
| --- | --- | --- |
| `worldEntity` | `world:Ref` | `Ref` for the active physics-world entity |
| `state` | `entity:Ref` | `BodyState` below; fails without an active rigid body |
| `body` | `entity:Ref`, `action:string`, `vector?:Vec3\|null = null`, `point?:Vec3\|null = null`, `rotation?:Quat\|null = null`, `wake?:boolean = true`, `axes?:string[]\|null = null` | `null` |
| `castRay` | `world:Ref`, `origin:Vec3`, `direction:Vec3`, `maxDistance:f32`, `solid?:boolean = true`, `groups?:Groups\|null = null`, `flags?:u32 = 0`, `excludeBody?:Ref\|null = null`, `excludeCollider?:Ref\|null = null` | `RayHit` below, or `null` when nothing is hit |

Every response is `{type:"PhysicsResponse",request_id,result,error}`. On
success, `error:null`; on failure, `result:null` and `error:string`. A ray miss
and a successful body action therefore both have `result:null,error:null`.
These are not error responses. Requests synchronize earlier physics component
changes before evaluation, without advancing the simulation clock.

Body action requirements:

| `action` | Required data and behavior |
| --- | --- |
| `applyImpulse` | `vector`; optional `point` applies the impulse at a world point |
| `addForce` | `vector`; optional `point` applies the force at a world point |
| `applyTorqueImpulse` | `vector` |
| `addTorque` | `vector` |
| `resetForces`, `resetTorques` | No additional data |
| `setLinearVelocity` | `vector`; optional `axes` contains only `"x"`, `"y"`, `"z"`; unlisted velocity axes retain their current values |
| `setAngularVelocity` | `vector` |
| `teleport` | Optional `vector` supplies position, optional `rotation` supplies orientation; omitted parts retain the current pose |
| `setKinematicTarget` | Same pose fields as `teleport`; requires a `kinematicPosition` body |
| `sleep` | No additional data; puts the body to sleep |
| `wake` | No additional data; wakes the body regardless of the optional `wake` field |

Except for the explicit sleep/wake actions, the `wake` field is passed to the
applicable native mutation. All body actions require an active body in an
active world. Supplying an invalid non-null vector/quaternion is rejected even
if that action does not use the field.

```ts
interface BodyState {
  world: Ref;
  position: Vec3;
  rotation: Quat;
  linearVelocity: Vec3;
  angularVelocity: Vec3;
  sleeping: boolean;
  mass: number;
  centerOfMass: Vec3;
  tickNumber: number; // u64: last completed physics tick for this world
}
interface RayHit {
  collider: Ref;
  body: Ref | null;
  distance: number;
  point: Vec3;
  normal: Vec3;
}
interface Groups {
  memberships?: number; // u32; default 4294967295
  filter?: number; // u32; default 4294967295
  test_mode?: "And" | "Or"; // default "And"
}
```

`Groups` rejects unknown fields. Ray direction must have squared length greater
than `1e-12` and is normalized; `maxDistance` must be finite and nonnegative in
world units. Excluded entities must be active in the queried world. `flags`
combines exclusion bits: fixed `1`, kinematic `2`, dynamic `4`, sensors `8`,
solids `16`. The body-type-only combinations are dynamic `3`, kinematic `5`,
fixed `6`; unknown bits are rejected.

For example, inside a batch:

```json
{"type":"PhysicsRequest","request_id":"ray-1","request":{"op":"castRay","world":{"$entity":"default:physics_world"},"origin":[0,10,0],"direction":[0,-1,0],"maxDistance":20,"flags":8}}
```

Physics notifications have outer shape `{type:"PhysicsEvent",event:...}`.
Subscribe using `Subscribe.events:["PhysicsEvent"]` and enable the appropriate
collider event flags. Every event has `world:Ref`, `colliderA:Ref`,
`colliderB:Ref`, `tickNumber:u64`, `substep:u32`, and `phase:"step"`, plus:

| `event.kind` | Additional fields |
| --- | --- |
| `started`, `stopped` | `sensor:boolean`, `removed:boolean` |
| `contactForce` | `totalForce:Vec3`, `totalForceMagnitude:f32`, `maxForceDirection:Vec3`, `maxForceMagnitude:f32` |

See [Physics](PHYSICS.md) for component configuration and event ordering.

### Network nested requests and callback acknowledgments

`NetworkRequest` is an application command containing `request_id:string` and
`request:object`. The nested discriminator is `op` with **PascalCase** values.
Unknown request fields are rejected. Fields marked `?` may be omitted and their
defaults are shown. `Json` means a schema-validated JSON value. Every operation
except `Register` requires `registration_id:string` in addition to the listed
fields.

| `op` | Other request fields | Successful `NetworkResponse.result` |
| --- | --- | --- |
| `Register` | `specification:Specification`, `side:"client"\|"server"`, `client_data?:ClientDataDefinition\|null = null` | `RegistrationInfo` below |
| `Encode` | `packet:string`, `data:Json` | `{bytes:string}` containing standard Base64 packet bytes |
| `Decode` | `bytes:string` containing standard Base64 | `{packet:string,state:string,direction:"serverbound"\|"clientbound",data:Json}` |
| `ValidateClientData` | `data:Json` | Normalized client-data object |
| `UpdateClientData` | `client_id:string`, `data:Json` | `null`; replaces that client's complete server-local data object |
| `Send` | `client_id?:string\|null = null`, `delivery?:"stream"\|"datagram" = "stream"`, `packet:string`, `data:Json` | `null` |
| `Listen` | `address:string`, `max_clients?:usize = 128` | `{address:string}` with the bound local address; asynchronous |
| `Connect` | `address:string`, `trust?:"strict"\|"tofu" = "strict"` | `ConnectionInfo` below; asynchronous |
| `Stop` | No additional fields | `null`; stops the endpoint and disconnects peers, keeping the registration |
| `Disconnect` | `client_id?:string\|null = null`, `reason?:string\|null = null` | `null`; disconnects the selected peer, also stopping a client endpoint |
| `Broadcast` | `packet:string`, `data:Json`, `delivery?:"stream"\|"datagram" = "stream"` | `null` |
| `EventDone` | `client_id:string`, `event_id:string`, `error?:string\|null = null` | `null` |
| `Close` | No additional fields | `null`; stops the endpoint and removes the registration |

Every response has `{type:"NetworkResponse",request_id,result,error}`. Success
sets `error:null`; errors set `result:null,error:string`. `Listen` and `Connect`
may answer in a later tick outside the submitting tick's `CommandResults`.
Other successful operations can legitimately return `result:null`.

```ts
interface RegistrationInfo {
  registration_id: string;
  specification_id: string;
  fingerprint: string; // "sha256:" followed by the native canonical schema hash
  side: "client" | "server";
  packets: {
    id: number; // u16
    packet: string; // "StateName.PacketName"
    state: string;
    direction: "serverbound" | "clientbound";
  }[];
}
interface ConnectionInfo {
  address: string; // remote socket address
  maxDatagramSize: number | null;
}
interface Specification {
  id: string; // "mod_id:specification_name"
  states: {
    name: string;
    serverbound: { name: string; fields: Field[] }[];
    clientbound: { name: string; fields: Field[] }[];
  }[];
}
interface ClientDataDefinition {
  schema: Schema; // must have kind:"struct"
  initial: Json;
}
interface Field {
  name: string;
  schema: Schema;
}
```

Specification, state, packet, client-data and field objects reject unknown
fields and require their listed arrays even when empty. A specification must
contain at least one packet. Its namespace must match the sending mod;
`side` must match the runtime mode. Client registrations cannot define client
data. A server accepting peers must supply a non-null client-data definition;
an empty struct with `initial:{}` is valid. The engine clones its normalized
initial value for each peer; this data is server-local, not replicated to the
client.

All later operations verify registration ownership. Server sends require an
active `client_id`, while client sends require it to be null/omitted. Outgoing
packets must have the correct side's direction and use the full
`"StateName.PacketName"` path. `Broadcast` requires a listening server.
`Listen` requires an owned server registration and literal IP:port address;
`max_clients` must be 1–65,536. `Connect` requires an owned client registration;
the nonempty address is at most 512 UTF-8 bytes. A starting/running endpoint
cannot start again without stopping. Disconnect reasons are at most 512 UTF-8
bytes; null selects the native `"disconnected"` reason. Datagrams must fit the
connection's current `maxDatagramSize`; a null maximum means they are
unavailable. Stream/datagram queue reservation can fail with a normal response
error, rather than block the simulation.

`ValidateClientData` and `UpdateClientData` require a registered client-data
schema; updates additionally require an active peer. Update/initial JSON is
limited to 1 MiB, and updates replace the whole normalized object. `Decode`
rejects Base64 input larger than `ceil(1048576 / 3) * 4` characters, decoded
packets above 1 MiB, invalid schemas, truncation, unknown IDs and trailing bytes.

Network events are **flattened**, with these exact current native shapes:

```ts
type NetworkNotification =
  | {
      type: "NetworkEvent";
      event: "connected";
      registration_id: string;
      client_id: string;
      data: Json; // server: normalized per-client data; client: null
      event_id: string;
      connection: ConnectionInfo;
    }
  | {
      type: "NetworkEvent";
      event: "packet";
      registration_id: string;
      client_id: string;
      event_id: string;
      packet: string;
      data: Json;
      transport: "stream" | "datagram";
    }
  | {
      type: "NetworkEvent";
      event: "disconnected";
      registration_id: string;
      client_id: string;
      reason: string;
    };
```

For example:

```json
{"type":"NetworkEvent","event":"packet","registration_id":"network:1","client_id":"peer-id","event_id":"event:2","packet":"Play.Move","data":{"x":1},"transport":"stream"}
```

There is no `payload` wrapper. After finishing `connected` or `packet`
callbacks, submit `EventDone` with the registration, client, event ID and
nullable error through a command batch. Its event ID must match the outstanding
acknowledgment for that peer. This releases per-peer delivery backpressure.
Failing a connected callback closes the peer; a packet callback error still
releases its acknowledgment. Forgetting acknowledgments stalls further
delivery. `disconnected` events have no event ID and require no acknowledgment.

`delivery` is `"stream"` or `"datagram"` in requests; the event calls this field
`transport`. Trust is `"strict"` or `"tofu"`, selected through the native
connection request. The engine manages certificates, trust records, sockets
and TLS keys. Mod IPC is separate from the binary QUIC packet protocol.

### Shared schema objects

`Schema` objects reject unknown fields and use `kind` in snake_case. These are
the exact data keys; `?` fields accept omitted or null values:

| `kind` | Additional keys | Accepted values | Supported in network schemas |
| --- | --- | --- | --- |
| `i32` | `min?:number\|null`, `max?:number\|null` | Signed 32-bit integer within optional inclusive bounds | Yes |
| `f64` | `min?:number\|null`, `max?:number\|null` | Finite number within optional inclusive bounds | Yes |
| `varint` | `min?:number\|null`, `max?:number\|null` | Signed 32-bit integer within optional inclusive bounds | Yes |
| `bool` | None | JSON boolean | Yes |
| `string` | `max_length?:usize\|null` | String; maximum UTF-8 bytes, default 1,048,576 | Yes |
| `array` | `element:Schema`, `max_length?:usize\|null` | Array; maximum item count, default 65,536 | Yes, when the element is supported |
| `optional` | `inner:Schema` | Null or an inner value; a missing struct field normalizes to null | Yes, when the inner kind is supported |
| `struct` | `fields:Field[]` | Object with exactly the declared fields; optional fields may be omitted | Yes, when every field is supported |
| `nullable` | `inner:Schema` | Null or an inner value | No |
| `tuple` | `elements:Schema[]` | Array with exactly those ordered elements | No |
| `enum` | `values:string[]` | One of the listed strings | No |
| `entity` | None | Object with only `$entity:string` (1–256 UTF-8 bytes) | No |
| `asset` | None | Empty string or a valid `mod_id://...` VFS path | No |

For numeric schemas, null/omitted bounds mean unbounded within the underlying
type. Integer bounds must themselves fit i32; all bounds are finite and
`min <= max`. Tuple definitions contain 1–32 elements; enum definitions contain
1–1,024 unique strings. `optional`/`nullable` cannot directly wrap another
`optional`/`nullable`. Schema nesting is at most 32 levels; definitions and
normalized values consume a 100,000-node budget. Declared string/array maxima
cannot exceed the defaults above.

State, packet, specification suffix and field names contain 1–128 ASCII bytes,
start with a letter or underscore, and continue with letters, digits or
underscores. `__proto__`, `constructor` and `prototype` are reserved. State names
are unique within the specification; packet names are unique per state and
direction; field names are unique per struct. At most 1,024 states and 65,536
packets are accepted. Packet IDs are assigned in declaration order: each
state's serverbound packets, then its clientbound packets. Reordering arrays
therefore changes the compiled contract and fingerprint.

Normalization rejects undeclared object keys. Every declared output field is
present: an omitted optional input becomes a key with null value. The shared
schema supports more kinds than the current native network codec; in
particular, `nullable`, tuple, enum and entity/asset references are valid for
other engine data but rejected for packets and client-data definitions.

This command registers a minimal client-side specification and can be included
in `CommandBatch.commands`:

```json
{"type":"NetworkRequest","request_id":"network-register-1","request":{"op":"Register","side":"client","client_data":null,"specification":{"id":"my_mod:play","states":[{"name":"Play","serverbound":[{"name":"Move","fields":[{"name":"x","schema":{"kind":"f64","min":-100,"max":100}}]}],"clientbound":[]}]}}}
```

TypeScript `Struct` and `Network.Specification` classes build these plain DTOs;
the classes and handlers themselves never cross the pipe.

### Declared but inactive messages

`Frame` and `Input` remain declared engine-message variants, but the current
simulation publishes input through `Tick`; it does not generate standalone
frame/input events. `AssetRequest` is currently a logging stub and does not
produce a usable `AssetResponse`. Do not implement asset loading by waiting on
that response. The presence of a variant in a schema is not a guarantee that
the current engine emits or services it.

## Bounds, failures and shutdown

The engine's current pipe queues have these implementation bounds:

| Boundary | Current behavior |
| --- | --- |
| Engine-to-host messages | Nonblocking writer queue: 256 queued messages, 64 MiB queued UTF-8 bytes per host; a full/closed queue or broken pipe terminates the host |
| Host-to-engine collection | Shared channel capacity of 4,096 lines; nonblocking drain processes at most 4,096 lines per call |
| Host-to-engine line size | No dedicated maximum line length in the current line reader; queue capacity is not a byte-size guarantee |
| Shared schema values | Native schema limits include 1 MiB payload bytes, 65,536 array items, 100,000 nodes, and 32 nesting levels; these are subsystem limits, not a universal IPC line limit |

Keep reading engine messages while work is pending and bound your own outgoing
data. Do not assume a 64 KiB read buffer is a packet-size limit or that the
64 MiB queue is a per-message entitlement.

Malformed JSON, an unknown top-level packet, invalid command structure,
unsolicited batches, mismatched tick numbers and duplicate tick batches cause
native rejection and host termination. The tick collection barrier waits for
each pending host to submit or exit. It has no per-mod timeout that
automatically skips a live host; the watchdog reports slow/hung ticks. A host
that keeps running without answering can stall simulation. The window redraw
loop does not advance or bypass that barrier.

On shutdown, the engine sends:

```json
{"type":"Shutdown","exit_code":0}
```

The current engine grants a brief 100 ms drain before killing remaining hosts.
Its final drain processes only `Log` commands from final batches. Do not depend
on shutdown to save files, create entities, finish RPCs, or commit application
mutations. Complete persistent writes during normal ticks. The supplied API
flushes its remaining buffer on exit and terminates with the provided code;
that does not change the engine's final-drain restriction. An EOF should close
the client cleanly rather than spin waiting for another message.

## Compatibility and conformance

Use protocol 10 for the current handshake and preflight. Do not invent a
version property on every message: only the defined metadata and handshake
carry protocol version. There is currently no capability negotiation or range
negotiation. Exact host/API compatibility is checked before mod evaluation.
When changing the engine-owned wire contract, update both native definitions
and the host implementation, keep documentation synchronized, and review the
host protocol version. Independent game, mod, scene and network-schema versions
are not substitutes for host compatibility.

A compatible implementation should verify these behaviors:

- Preflight metadata and `RuntimeReady` report the expected versions; startup
  errors emit `RuntimeFailed` and exit.
- A streaming UTF-8 decoder handles partial and coalesced lines without leaking
  log output into stdout.
- The initial reset is applied before `Init`; client/server mode is available
  before mod evaluation; startup does not await engine results.
- Every `Tick` produces one matching batch, including empty ticks; async
  callbacks do not submit extra or nested batches.
- Command order is preserved, request IDs are correlated, and result dispatch
  accepts both envelopes and standalone notifications/responses.
- Same-batch aliases resolve; resolved IDs retain generations; tagged nested
  references are distinct from ordinary strings.
- Scene revisions, removals, component tombstones, instance IDs and complete
  metadata replacements are applied correctly.
- Query results are understood at command execution time and predictions are
  reconciled with committed post-physics state.
- Network callback acknowledgments release delivery; slow readers and pending
  operations do not block transport processing.
- EOF, rejected packets, host termination and shutdown reject or cancel pending
  library operations without relying on a final mutation batch.

The supplied implementation can be studied without treating its conveniences
as protocol requirements: [shared command buffer](../runtime/src/shared.ts),
[message dispatch](../runtime/src/ipc.ts),
[private transport](../runtime/src/transport.ts), and
[scene mirror and prediction](../runtime/src/scene-store.ts).
