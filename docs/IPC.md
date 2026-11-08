# Internal scripting ABI

The filename is retained for existing documentation links. Scripts now run inside
the engine; there is no stdin/stdout protocol or independently versioned host.
The VM, SDK and source compiler are built and embedded together with the engine.

## Source loading

Distribute source JS/TS/JSON files and a mod.toml entry. The engine discovers and
transforms these on each destination machine, using the embedded compiler. It
loads the SDK and discovered component classes before the side-specific entry,
sets `Engine.mode` before module evaluation, and delivers `Init` when native
runtime/window readiness permits. Modules must finish evaluating before `Init`;
move work waiting for later events/timers into an init handler.

## Wasm boundary

The engine-owned guest exports `initialize`, `allocate`, `deallocate`,
`compile_sources`, `load_package`, `evaluate`, `drain_jobs`, `result_pointer` and
`result_length`. Its only imports are checked `whatever.request`,
`whatever.response`, and the QuickJS clock callback. There are no WASI imports.
Memory ranges and buffer sizes are checked at the host boundary. Serialized
native Wasmtime caches or QuickJS bytecode supplied by mods are never accepted.

The private SDK calls `__engineCall(JSON.stringify({op,...args}))`; responses are
JSON `{value}` or `{error}`. This does not expose Rust pointers. A scoped callback
borrows the active simulation worker and binds mod authority independently of
payload fields. Direct reads cover entity metadata and records, entity listings,
queries, and component rows with their data and incarnation. `component_patch`
validates and commits fields synchronously. `command` dispatches the typed `ScriptMessage` enum into
native ECS, physics, files, networking, audio, console and presentation services,
then returns immediate `EngineMessage` responses to that mod. Async networking and
cross-mod replies arrive in later deliveries. Existing message names identify
internal operation DTOs; they are no longer transport envelopes.

The [generated message catalogue](/protocol/messages/) follows
`engine/src/script/ipc.rs`; physics/network payloads follow their native schemas.
Regular mods should use the public SDK rather than this private ABI.

## Ordering and failure

The simulation samples input and external events, executes mods in dependency/load
order, applies editor commands, integrates native physics, then publishes native
presentation/editor state and queues events/metrics. Calls within a VM execute in
call order; later commands and mods observe successful changes immediately. No
CommandBatch barrier, replicated script scene or command-result tick wait remains.
Query results are available without awaiting a tick. An error does not undo earlier
successful writes. Async hooks do not overlap their own pending invocation.

Fuel covers a guest invocation and its microtasks. Memory, stack, calls, buffers
and delivery queues are bounded. Traps and unhandled errors stop the affected
instance, while engine shutdown and healthy mods continue. See the
[runtime policy](SANDBOX.md) and [complete API](SCRIPTING.md).
