# Mod runtime policy

Mods execute in independent engine-owned QuickJS Wasm instances hosted by Wasmtime.
The engine owns the VM, SDK and TypeScript compiler. Source compilation and
metadata inspection happen on the destination machine without evaluating mod code
or loading project plugins. See [embedded scripting](EMBEDDED_SCRIPTING.md).

## Imports and globals

Only `@whatever-engine/api` and JS/TS/JSON source within the current mod are
resolvable. The engine enumerates bounded source files; symlinks, node_modules,
assets, logs and development metadata are excluded. Canonical paths must stay in
the mod root. The guest's module resolver rejects traversal, absolute mod imports,
backslashes, bare packages other than the SDK, built-ins, native addons and URL
imports. Static/dynamic ESM and literal CommonJS helper imports share this policy.
Project package/TypeScript configuration cannot widen it.

QuickJS supplies ordinary JavaScript language globals. The engine explicitly adds
restricted logging, UTF-8 TextEncoder/TextDecoder, Buffer UTF-8/base64 operations,
microtasks and simulation-driven timers. Date reads the host wall clock;
`Profiler.now()` is monotonic. Mods have no process, fetch, WebSocket, Worker,
filesystem, environment, raw socket, native extension or WASI access. Features
are added explicitly; there is no unrestricted resolver or permission grant.

## Execution limits

Each VM has a 256 MiB QuickJS heap limit within a 384 MiB Wasm memory limit, a
256 KiB JavaScript stack and 1 MiB Wasm stack. Ordinary evaluations have 2 billion
Wasm fuel units and at most 100,000 host calls, shared with promise-job draining.
Package initialization has a larger finite fuel allowance; static TypeScript
compilation has its own bounded instance and allowance. Recursively scheduled
microtasks and infinite loops exhaust fuel. Unhandled rejections and traps fault
the affected mod; remaining mods and engine shutdown continue. Reaping releases
its VM and native network registrations.

ABI source/results are at most 64 MiB. Mod sources are at most 4 MiB per file,
32 MiB total and 10,000 source files; traversal is bounded by depth and entry count.
Pending per-mod delivery queues have at most 4,096 messages. Native operations
also validate schemas, references, scene limits, ownership and subsystem bounds.
Wasm fuel does not meter Rust host work, and guest memory limits do not cap native
ECS allocations. This is an in-process sandbox, not a separate kernel security
boundary against vulnerabilities in Wasmtime or engine capability implementations.

## Files and assets

`File` uses the Rust broker under the OS local-data directory
`Whatever/<game_id>/<mod_id>/`. Storage IDs allow only lowercase ASCII letters,
digits, underscores and hyphens. Relative paths reject traversal, absolute paths,
drive prefixes, backslashes and alternate-stream syntax. Existing ancestors are
canonicalized to reject symlink escapes, including dangling links. VFS reads apply
containment to asset roots and recursive listings skip links. VM code receives
neither native paths nor file descriptors.

## Networking and editor control

Networking goes through the engine-owned QUIC service. Native registration checks
runtime side and mod namespace; later calls check registration/peer ownership.
Binary decoding, frame limits, bounded per-peer queues, callback acknowledgements,
TLS and strict/TOFU trust remain native. Certificate paths, keys, trust records and
trust-reset controls are unavailable to scripts. Mod messages route between
isolated instances rather than sharing JavaScript objects.

Editor live control is a separate authenticated loopback connection. Its random
token and endpoint are read by the native player and are never exported into a VM.
Scene edits run on the simulation worker. No mod receives an editor socket or OS
permission through this connection.

## Build and verification

The maintainer build needs Cargo, the wasm32-unknown-unknown target and Clang/LLVM.
Installed games/editor builds need none of these. Build and test instructions are
in [embedded scripting](EMBEDDED_SCRIPTING.md#maintainer-build). The portable suite
runs a copied player with an empty PATH and raw TS/JS/JSON mods, checks cache
invalidation and immediate mutation, rejects external imports, and verifies fuel
faults do not prevent engine shutdown. Native tests cover memory, calls, promise
jobs, VM isolation and compilation without source evaluation. Broker containment
and native network tests remain part of the workspace suite.
