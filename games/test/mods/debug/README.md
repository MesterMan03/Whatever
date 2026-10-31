# Whatever Debug Suite

First-party diagnostics under the `dev` command. The client entry loads the demo camera and presentation probes; the server entry loads headless probes. Tests run on request, with no startup file writes, messaging broadcasts, tick spam or intentional failures.

Open F2 on the client or type commands into server stdin. Use `help dev`, `help dev render` or `help dev render light` for signatures.

- `dev echo "hello"`: argument parsing probe.
- `dev error`: deliberate command-handler failure.
- `dev ticks true` / `dev ticks false`: tick tracing.
- `dev watchdog 2000` / `dev watchdog 0`: slow-tick and shutdown probe; zero disables.
- `dev file`: per-mod broker roundtrip with cleanup.
- `dev message debug`: inter-mod request/reply to one loaded mod.
- `dev sandbox`: import/global allowlists and file-containment checks.
- `dev benchmark 128`: single-thread Bun Mandelbrot benchmark; replaces the old native `markbench` experiment.
- `dev sprite`: original sprite/transform probe, using `debug://textures/sample.png`.
- `dev text "hello"`: text renderer probe.
- `dev suggest value`: deterministic custom completion probe.
- `dev window resize 1280 720`, `dev window mode borderless`, `dev window title "Whatever"`.
- `dev render mesh triangle.glb mesh_lit 0 0 -6`: mesh/shader probe. Full VFS paths also work.
- `dev render sprite debug://textures/sample.png uv_debug`: custom sprite shader probe.
- `dev render light ambient 1 1 1 0.2` and `dev render light point 0 2 0 1 1 1 2 10`.
- `dev render clear`: remove entities created by the render probes.
- `dev render scene [path]`: instantiate a `.wscene`; defaults to `debug://editor`, also available in the native editor scene browser.

Presentation commands are client-only. The invisible free-camera rig uses WASD, Space/Shift and captured mouse movement only while its camera is active. Other mods can replace the main camera without moving the debug rig in the background. Sprites are created explicitly through `dev sprite` or the rendering probes. Shared engine introspection (`engine`, `mods`, `scene`, `vfs`) and logging switches (`debug`) remain native console services.
