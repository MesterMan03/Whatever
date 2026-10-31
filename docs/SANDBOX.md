# Mod runtime policy

Each scripted mod runs in an engine-owned `whatever-mod-host` subprocess. The executable contains the pinned Whatever Bun fork, the trusted bootstrap, and `@whatever-engine/api`. The engine never searches PATH for Bun and never falls back to stock Bun.

The submodule tracks the `codex/whatever-runtime` branch of [Whatever-mod-runtime](https://github.com/MesterMan03/Whatever-mod-runtime), with the upstream Bun repository retained locally as `upstream`. It is pinned to `12ad6221f3a931151c0be16f10c556eb15b4030c` (Bun v1.4.4) in `vendor/bun`, including the committed native policy and Windows build adjustments. `vendor/patches/bun-whatever.patch` exports those changes relative to `f7607a3d350b9c9488d41eab6a18d6cac74794e6` for review and reproduction. Run `bun run prepare:bun` to initialize and verify the pinned fork. Preparation refuses a different revision or conflicting policy edits.

The patch also keeps code-generation source arguments relative to the checkout to fit Windows command limits and pins builtin-table manifests to LF endings. Preparation normalizes existing CRLF copies of those manifests without changing their data.

## Imports

The policy starts with one public package on the allowlist:

- `@whatever-engine/api`, supplied by the host before mod evaluation.

Local JavaScript, TypeScript and JSON imports are allowed inside the current mod directory. Supported extensions are `.js`, `.jsx`, `.mjs`, `.cjs`, `.ts`, `.tsx`, `.mts`, `.cts` and `.json`. Paths are canonicalized before loading. A path outside the mod root, a symlink to an outside file, any `node_modules` component, and a native addon are denied. Non-source loader overrides, including SQLite, file and text loaders, are also denied.

All other package names, `node:` and `bun:` modules, package subpaths, package import aliases, and network/data/blob module URLs are denied. Checks cover static import records before optimization, ESM resolution and fetching, dynamic imports, and CommonJS resolution before consulting its cache. A mod's package.json, tsconfig aliases or bundled dependencies cannot grant runtime privileges. Pure JavaScript copied into a mod's own local source is still mod code and runs under the same restrictions.

Unix policy path checks use Bun's caller-buffer realpath wrapper, avoiding libc-owned allocations crossing Bun's allocator boundary.

The trusted bootstrap registers the engine API before enabling the native policy. A mod-local copy of that package cannot replace it. Mod bundlers should leave this package external.

There are no elevated permissions or mod-configurable grants yet. Future allowed modules and permissions must be added explicitly; an unknown module stays denied.

## Globals

The native transition retains an explicit allowlist of ordinary JavaScript language globals, typed arrays and Buffer, text encoding, URL parsing, timers, microtasks, structured cloning and the host's restricted console. The complete list is `whateverGlobalAllowed` in the fork's `src/jsc/bindings/ZigGlobalObject.cpp`.

Other globals are replaced with immutable undefined values. In particular, mods receive no `Bun`, `process`, `fetch`, `WebSocket`, `Worker` or `ShadowRealm`. Unknown globals added by future Bun releases are denied unless reviewed and added to the list.

The bootstrap captures native stdin/stdout pipe handles for its private IPC transport before this transition, clears the CommonJS cache of trusted bootstrap modules, and hides CommonJS parent references. Cache enumeration and direct lookups also enforce the native mod-root allowlist, including the ESM registry fallback and cache proxies created before lockdown. Embedded executable paths are hidden on every platform. Policy activation is irreversible for the process. Scripts cannot widen or reset it.

## Host startup

1. The engine locates the host beside its executable. Debug builds may also use `runtime/dist/whatever-mod-host` from the source checkout.
2. It checks the host identity, pinned Bun version, native policy version, IPC protocol and API version.
3. The host runs with a cleared environment, retaining only Windows system-directory and temporary-directory variables when present, plus a fixed `BUN_DEBUG_QUIET_LOGS=1` to keep native debug diagnostics off IPC stdout. It receives no inherited PATH, Bun/Node options or application secrets.
4. Compiled hosts disable automatic dotenv, bunfig, package.json and tsconfig loading. Runtime package installation, macros and native addons are disabled.
5. The bootstrap registers the embedded API, installs its console, activates the native policy, and loads the mod.
6. After evaluation, it sends `RuntimeReady`. The engine waits for the active hosts' handshakes and sends `Init` in dependency/load order; client mode also waits for its window. Host protocol 8 provides `client` or `server` before top-level mod evaluation, includes it in `Init`, and accepts application commands only inside per-tick `CommandBatch` envelopes.

A missing or incompatible host is an engine startup error. A host built with stock Bun refuses to load any mod.

## Files and assets

Persistent files go through the Rust `File` broker, under the OS local-data directory at `Whatever/<game_id>/<mod_id>/`. IDs are restricted to lowercase letters, digits, underscores and hyphens to avoid case aliases between data directories. Relative paths reject parent traversal, absolute paths, drive prefixes, backslashes and alternate-stream syntax. Existing path ancestors are canonicalized to reject symlink escapes, including dangling symlinks.

Mod IDs must be unique, and asset roots must remain inside their mod directory. VFS disk reads apply the same containment check to each mod's asset root. Recursive asset listing skips symlinks. These checks protect operations performed by the engine on behalf of a script; hiding Bun's filesystem APIs alone would not protect that broker.

## OS restrictions

The existing OS restrictions remain a second layer.

- Linux applies Landlock for filesystem/network access, seccomp for sensitive syscalls and network sockets, and capability reduction. It grants the bundled host directory read/execute access, the mod directory read access and temporary storage for runtime caches. It no longer searches for a user Bun installation or grants access to the engine source tree or persistent save directories.
- macOS installs a Seatbelt profile around the exact host executable and mod root. It denies networking and restricts writes to temporary storage.
- Windows uses a Job Object for process limits and cleanup. It does not provide filesystem or network isolation by itself.

OS setup currently degrades with diagnostics when a facility is unavailable. The native policy remains mandatory. Runtime policy is not a replacement for kernel isolation against native-code vulnerabilities. General CPU/memory quotas and a permission-grant system remain future work; engine-to-mod IPC now has bounded nonblocking writer queues.

## Build and verification

Install the native prerequisites described in the Bun checkout's build documentation, including its pinned Rust toolchain, LLVM 23.1.x, CMake, Ninja, Perl and NASM where applicable. Windows also needs Visual Studio C++ tools with ATL; the build helper initializes the Visual Studio environment automatically. Portable LLVM/CMake/NASM tools under `target/build-tools` are recognized.

```sh
bun install --frozen-lockfile
bun run prepare:bun
bun run build:bun
bun run build:runtime
bun run build:host
bun run test:host
cargo test --workspace --locked
```

For a debug fork use `bun run build:bun --debug`. Then use `bun run build:host --debug-runtime` explicitly for a development host. The default host builder requires the release fork and never silently falls back to debug. Deployments reject debug hosts. The host builder also accepts `--bun <path-to-built-fork>` and verifies its native hook before compiling. It does not accept stock Bun.

The native resolver regression cases are in `vendor/bun/test/js/bun/resolve/resolve.test.ts` under `Whatever native module policy`. Run them with `bun bd test test/js/bun/resolve/resolve.test.ts -t Whatever` inside that checkout. Background transpilation, bytecode caches and transpilation caches are disabled for locked mod sources so cached transforms cannot skip the policy. The end-to-end host suite runs the compiled host outside the engine tree with an empty PATH.

Linux checks can be run under WSL. Keep Linux Cargo outputs in a separate target directory so Windows and Linux never share compiled artifacts. Run sandboxed Linux game/mod files from WSL's Linux filesystem: the tested WSL kernel denied even allowlisted files on the Windows-mounted `/mnt/c` filesystem under Landlock. The kernel probe uses Linux-filesystem fixtures and verifies an allowed read plus denied engine-file and network access.
The Network API submits schemas and transport requests to an engine-owned QUIC service. Native registration checks runtime side/mod namespace; every later request checks registration/peer ownership. Binary decoders enforce bounds, packet direction and frame limits before callbacks. The service and per-peer queues are bounded and callback acknowledgements prevent unbounded delivery into a slow mod. Server TLS keys and strict/TOFU trust records live outside mod-accessible directories; only operator CLI settings select certificate, key or additional-CA files. TOFU still verifies TLS handshake signatures and commits a key only after TLS and schema negotiation succeed. Known key changes and PKI downgrades fail. Mods retain no raw sockets, networking globals, package imports or trust-reset capability. Client-data templates remain server-local.


The native editor may launch a player with `WHATEVER_EDITOR_ENDPOINT` and a
cryptographically random `WHATEVER_EDITOR_TOKEN`. This is a separate, loopback-only
control channel, authenticated before any snapshot or edit. Framed messages are
size limited and the GUI uses a latest-state snapshot mailbox. Entity mutations
are validated atomically on the simulation worker before physics. Mod hosts
retain their cleared environment and receive neither variable; this adds no
script import, network, global or native-module permissions.
