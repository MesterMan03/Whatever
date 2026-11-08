# Embedded scripting

Every scripted mod runs inside the engine, in its own QuickJS Wasm instance hosted
by Wasmtime. Games distribute ordinary JavaScript, TypeScript and JSON sources.

## Loading on an end-user machine

The maintainer build embeds one engine-owned Wasm VM, the SDK, TypeScript compiler,
SDK declarations and the compiler's standard library into the shared native
`whatever_script_vm` library. Player, editor and CLI load that same library;
the payload is included once per distribution.
The engine discovers contained mod sources, extracts component schemas without
executing the mod, and transpiles TypeScript inside a compiler Wasm instance.
QuickJS compiles the resulting modules to its internal bytecode when loading them.
Mods distribute source. The installed editor, player and CLI load new mods with
their embedded tooling. Supported systems use their platform's native engine build.

Transforms are cached as bounded JSON in the OS cache directory under
`Whatever/script-cache`. The key includes every source path/content, mod identity,
and the bundled VM/compiler/SDK bytes. Changed sources or engine builds invalidate
the cache. A missing or unwritable cache only requires recompilation. The engine
never deserializes mod-supplied native Wasmtime code caches.

Only `@whatever-engine/api` and local source imports are available. ESM supports
relative static/dynamic imports, JSON and top-level await that completes during
module initialization. CommonJS helpers support literal `require()` dependencies;
these use the same contained resolver. Runtime package/config/plugin resolution,
host built-ins and external source URLs are unavailable. Component classes use
named ESM exports. Put asynchronous startup that needs later ticks in `init`.

## Direct engine access

A VM turn runs synchronously on the simulation worker. Scoped host calls borrow
that worker's authoritative engine services; guests never hold Rust pointers.
Entity operations, live component field edits (including nested arrays/objects),
scene/prefab loading and physics queries execute before returning. Later commands
and later mods see those writes immediately. Physics integration remains a separate
phase after scripts. Successful writes are not rolled back by a later exception.

Ordinary component reads return live checked handles. `editComponent` and legacy
`Pending.get()` requests remain compatibility helpers; neither is necessary for
ordinary reads/writes. `Scene.snapshot()` explicitly requests an immutable retained
copy. Turn-local reads reuse component data from lifecycle reconciliation.
Component patches invalidate affected entries, including world transforms for
local transform patches. Structural writes and message delivery clear the
component cache. There is no replicated script scene or write
prediction. The initial private ABI still uses bounded JSON buffers within the
process; bulk binary access is a possible optimization. Removing IPC does not
establish a performance win over a JIT JavaScript engine.

Each mod has an isolated VM and heap. Native calls derive authority from the
engine's current mod identity, never a caller-supplied identity. Wasm has no WASI,
filesystem, environment, sockets or processes. Fuel covers execution and promise
jobs; stack, memory, source/result buffers and calls are limited. See
[SANDBOX.md](SANDBOX.md) for the limits and broker policy.

## Component references

```ts
export class Pickup extends Component {
  session: RunSession | null = null;
  collect(): void { this.session?.award(10); }
}
```

The annotation selects the exact concrete component type. The explicit descriptor
is `Type.Component(RunSession)`, composable with nullable/array/struct descriptors.
Persisted links contain `{"$entity":"session","$component":"core:RunSession"}`.
The inspector filters eligible entities. Scene, prefab and clone operations remap
the entity part. Atomic scene creation validates links after all components exist,
allowing forward/cyclic references and rejecting missing targets. Runtime access
resolves lazily to the current mod-local instance, preserving methods/private
state; missing targets throw. Handles check entity generation and component
incarnation. Separate mods cannot share JavaScript class instances or call each
other's methods; use mod messaging for that. Network schemas exclude these links.
Coinwatch uses RunSession component links for central callbacks.

## Maintainer build

Install stock Bun 1.4.2+, Rust's `wasm32-unknown-unknown` target and
a Wasm-capable Clang/LLVM toolchain. Windows builds recognize LLVM
under `target/build-tools` and use copied WASI headers to avoid extended-path bugs.

```sh
bun install --frozen-lockfile
rustup target add wasm32-unknown-unknown
bun run build:runtime
bun run build:guest
bun run build:native
bun run test:embedded
bun run test:guest
```

Rebuild the guest after changes to the SDK/compiler or guest sources. The ignored
`runtime/dist/whatever-script-guest.wasm` is an input to native engine compilation,
not a per-mod deliverable. CI builds it before checking or linking Rust targets.
Portable editor/game packaging ships the runtime in a private native library
beside its executables and includes its notices in `RUNTIME-LICENSES.md`.
