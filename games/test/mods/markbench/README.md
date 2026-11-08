# Markbench

Run `markbench run standard` from F2 or headless stdin. `markbench run quick`
is a smoke test. `markbench status`, `markbench cancel`, and `markbench report`
control the run. Nothing runs automatically on loading this mod.

Markbench replaces `dev benchmark` (Mandelbrot). All work is ordinary mod code.
It is included in the development project and excluded from deployment builds.

| Stage | Standard work per tick | Coverage |
|---|---:|---|
| snapshot_reads | 2048 entities, 8 read passes | live component handles, tags, native queries |
| sparse_deltas | 128 changed of 2048 components | unchanged cold arrays and immediate sparse native patches |
| transform_writes | 2048 rotations | ordered narrow patches, native ECS, transport |
| hierarchy | 16 moving parents, 2048 descendants | inherited transform invalidation |
| component_lifecycle | 2048 instances | reconciliation, synchronous tick hooks, immediate field writes |
| entity_churn | 128 creates and previous generation removals | aliases, tags, component registration, tombstones |
| physics_contacts | 256 dynamic boxes, 2 substeps | Rapier contacts, state and transform publication |
| native_codecs | 64 encode/decode requests | shared schema validation, native codec capability calls |
| render_draws (client) | 2048 sprites | draw submission and resource binding |
| render_overdraw (client) | 192 overlapping large quads | fragment/fill and presentation pressure |
| render_text (client) | 256 text labels | font atlas and text rendering |

Each standard stage settles for 3 ticks, warms up for 60 ticks and records 240
ticks. Quick uses 4 warmup and 12 measured ticks, with smaller counts. Setup,
cleanup and transition ticks are excluded. Commands and native timing samples
are associated by tick number; repeated frame samples are deduplicated. The
codec pipeline has a bounded number of outstanding operations, validates round
trips and drains before final reporting. Exceptions cancel the run and clean up.

The suite creates one owned root per stage, destroys only its own hierarchy, and
restores the previous main camera, FPS cap and vsync after finishing or cancelling.
The existing scene continues running: its entity and host counts are reported.
For comparisons, launch a dedicated project containing core and markbench only.
Keep the tick rate, build, hardware, runtime side, mod set and viewport resolution
identical. Standard and quick scores are separate measurements. Run several times;
thermal throttling, other applications and background mods affect the result.

Each result contains sample count, mean/p50/p95/p99/max for native tick phases,
script workload, compatibility snapshot metrics (zero with embedded VMs), rendering CPU
and surface waits, and scene draw calls. It includes guest memory before and after (compatibility heap/RSS field names),
platform, debug/release build, tick rate, adapter/backend, resolution, vsync and cap.
The JSON is logged with `MARKBENCH_RESULT` and saved as `markbench-latest.json`
through this mod's File broker. `markbench report` prints the most recent report.

Per-stage score is `1000 * (1000 / 60) / p95(native_total_ms)`. Thus 1000 means the
workload's p95 tick cost fits a 60 Hz budget. The engine score is an equal-weight
geometric mean of the eight simulation stages. Missing samples or failed codec
checks invalidate the aggregate. The client presentation score uses p95 wall frame
intervals for the three graphics stages, with vsync and FPS cap temporarily disabled.
Headless runs have no presentation score. Scores are engineering indices with an
explicit formula, not calibrated comparisons to another engine or hardware model.

Native renderer measurements cover CPU acquisition, encoding, submission and
presentation. **GPU execution timestamps are unavailable** (`gpu_ms: null`). A fast
CPU submission is not a GPU timing. The embedded backend reads authoritative
native state and replicates no scene to scripts: snapshot bytes/application and
publication fields remain zero for comparison with historical reports. `hosts`
now counts isolated VMs. Compare the same engine build and workload configuration.

`npm run test:markbench` runs all eight headless stages automatically. Set
`WHATEVER_TEST_PLAYER` to a freshly built player, and optionally
`WHATEVER_MARKBENCH_PRESET=standard` for the full stress pass. `test:snapshots`
checks scoring math; live-state and retained-snapshot checks are in `test:embedded`.
