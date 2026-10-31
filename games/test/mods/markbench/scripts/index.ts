import {
  BuiltInComponents as C,
  Console,
  Engine,
  File,
  Network,
  Physics,
  Profiler,
  Scene,
  type Entity,
  type TickTimings,
} from "@whatever-engine/api";
import { Counter, Payload } from "./components.ts";
import { distribution, geometricMean, score } from "./scoring.ts";

type Stage = {
  id: string;
  count: number;
  setup: () => void;
  step: (tick: number) => void;
  graphics?: boolean;
};
type Sample = {
  native: TickTimings;
  runtime: ReturnType<typeof Profiler.runtime>;
  workload_ms: number;
};
type Run = {
  preset: string;
  warmup: number;
  measured: number;
  stages: Stage[];
  index: number;
  frame: number;
  samples: Sample[][];
  scheduled: Map<number, { stage: number; workload_ms: number }>;
  roots: Entity[];
  entities: Entity[];
  root: Entity | null;
  previousCamera: Entity | null;
  previous: ReturnType<typeof Profiler.latest>;
  baseline: number;
  memory: ReturnType<typeof Profiler.memory>;
  started: number;
  lastFrame: number;
  frames: TickTimings["frame"][][];
  pending: number;
  codecs: number;
  failure: string | null;
};
let run: Run | null = null;
let report: any = null;
let checksum = 0;
const packet = Network.Packet({
  sequence: Network.Type.I32,
  points: Network.Type.Array(Network.Type.F64, { maxLength: 64 }),
});
const specification = Network.Specification("markbench:codec", {
  sample: { serverbound: { data: packet }, clientbound: { data: packet } },
});
let endpoint: {
  encodePacket(
    path: "sample.data",
    data: { sequence: number; points: number[] },
  ): Promise<Uint8Array>;
  decodePacket(bytes: Uint8Array): Promise<any>;
} | null = null;
Engine.on("init", async () => {
  endpoint =
    Engine.mode === "client"
      ? await Network.registerClient(specification)
      : await Network.registerServer(specification, {
          clientData: Network.Struct({}),
          initialClientData: {},
        });
  Engine.log("info", "Markbench ready: markbench run quick | standard");
});
function entity(): Entity {
  const r = run!;
  const e = Scene.createEntity();
  if (r.root) e.setParent(r.root);
  else r.roots.push(e);
  return e;
}
function transforms(count: number, component = false): void {
  const r = run!;
  for (let i = 0; i < count; i++) {
    const e = entity()
      .setComponent(new C.Transform({ position: [i % 64, Math.floor(i / 64), 0] }))
      .addTag("markbench:entity");
    if (component) e.setComponent(new Payload());
    r.entities.push(e);
  }
}
function stages(quick: boolean): Stage[] {
  const n = quick ? 64 : 2048;
  const result: Stage[] = [
    {
      id: "snapshot_reads",
      count: n * 8,
      setup: () => transforms(n, true),
      step: () => {
        const r = run!;
        for (let repeat = 0; repeat < 8; repeat++)
          for (const e of r.entities) {
            const data = e.getComponent(Payload);
            const transform = e.getComponent("core:transform");
            if (!e.exists || !data || !transform || !e.hasTag("markbench:entity"))
              throw new Error("Snapshot read failed");
            checksum += data.value + transform.position[0];
          }
        if (Scene.findByTag("markbench:entity").length !== n) throw new Error("Tag index mismatch");
        if (Scene.query([Payload, "core:transform"]).length !== n)
          throw new Error("Component index mismatch");
      },
    },
    {
      id: "sparse_deltas",
      count: Math.max(1, n / 16),
      setup: () => transforms(n, true),
      step: (tick) => {
        const r = run!;
        for (let i = 0; i < r.entities.length; i += 16)
          r.entities[i]!.editComponent(Payload, (draft) => {
            draft.value = tick;
          });
      },
    },
    {
      id: "transform_writes",
      count: n,
      setup: () => transforms(n),
      step: (tick) => {
        for (const e of run!.entities) e.setEulerDegrees(0, tick % 360, 0);
      },
    },
    {
      id: "hierarchy",
      count: n,
      setup: () => {
        const r = run!;
        for (let i = 0; i < 16; i++) {
          const parent = entity().setComponent(new C.Transform());
          r.entities.push(parent);
          for (let j = 0; j < n / 16; j++)
            entity()
              .setParent(parent)
              .setComponent(new C.Transform({ position: [j, i, 0] }));
        }
      },
      step: (tick) => {
        for (const e of run!.entities) e.move([Math.sin(tick * 0.01), 0, 0]);
      },
    },
    {
      id: "component_lifecycle",
      count: n,
      setup: () => {
        transforms(n);
        for (const e of run!.entities) e.setComponent(new Counter());
      },
      step: () => {
        for (const e of run!.entities)
          if (!e.getComponent(Counter)) throw new Error("Lifecycle snapshot missing");
      },
    },
    {
      id: "entity_churn",
      count: quick ? 8 : 128,
      setup: () => {},
      step: (tick) => {
        const r = run!;
        for (const e of r.entities) e.destroy();
        r.entities = [];
        transforms(quick ? 8 : 128, true);
        for (const e of r.entities) e.move([tick % 10, 1, 0]);
      },
    },
    {
      id: "physics_contacts",
      count: quick ? 16 : 256,
      setup: () => {
        const r = run!;
        const world = entity().setComponent(new Physics.World({ substeps: 2 }));
        entity()
          .setComponent(new C.Transform({ position: [0, -1, 0] }))
          .setComponent(Physics.RigidBody.fixed({ world }))
          .setComponent(new Physics.Collider({ shape: Physics.Shape.box({ size: [40, 1, 40] }) }));
        for (let i = 0; i < (quick ? 16 : 256); i++)
          r.entities.push(
            entity()
              .setComponent(
                new C.Transform({
                  position: [i % 8, 0.6 + Math.floor(i / 64) * 1.1, Math.floor(i / 8) % 8],
                }),
              )
              .setComponent(Physics.RigidBody.dynamic({ world, canSleep: false }))
              .setComponent(
                new Physics.Collider({ shape: Physics.Shape.box({ size: [0.9, 0.9, 0.9] }) }),
              ),
          );
      },
      step: () => {
        for (const e of run!.entities)
          checksum += e.getComponent("core:rigid_body_state")?.position[1] ?? 0;
      },
    },
    {
      id: "native_codecs",
      count: quick ? 4 : 64,
      setup: () => {
        if (!endpoint) throw new Error("Codec endpoint has not initialized");
      },
      step: (tick) => {
        const r = run!;
        const count = quick ? 4 : 64;
        if (r.pending > count * 4)
          throw new Error("Native codecs failed to drain their bounded pipeline");
        for (let i = 0; i < count; i++) {
          r.pending++;
          const sequence = (tick * count + i) >>> 0;
          void endpoint!
            .encodePacket("sample.data", {
              sequence,
              points: Array.from({ length: 32 }, (_, i) => i * 0.25),
            })
            .then((bytes) => endpoint!.decodePacket(bytes))
            .then((decoded) => {
              if (decoded.data.sequence !== sequence || decoded.data.points.length !== 32)
                throw new Error("Codec round trip mismatch");
              r.codecs++;
            })
            .catch((error) => {
              r.failure = String(error);
            })
            .finally(() => {
              r.pending--;
            });
        }
      },
    },
  ];
  if (Engine.mode === "client") {
    const graphics =
      (fill: boolean, text: boolean): (() => void) =>
      () => {
        const camera = entity()
          .setComponent(new C.Transform({ position: [0, 0, 30] }))
          .setComponent(new C.Camera({ zfar: 200 }));
        Engine.setMainCamera(camera);
        const count = text ? (quick ? 16 : 256) : fill ? (quick ? 16 : 192) : n;
        for (let i = 0; i < count; i++) {
          const position: [number, number, number] = fill
            ? [0, 0, -i * 0.001]
            : [(i % 32) - 16, Math.floor(i / 32) * 0.4 - 10, 0];
          const e = entity().setComponent(
            new C.Transform({ position, scale: fill ? [24, 24, 1] : [0.3, 0.3, 1] }),
          );
          if (text)
            e.setComponent(
              new C.TextRenderer({
                text: "Markbench 0123456789",
                font: "core://fonts/default.ttf",
                shader: "core://shaders/sprite.wgsl",
                font_size: 24,
              }),
            );
          else e.setComponent(new C.SpriteRenderer({ shader: "core://shaders/sprite.wgsl" }));
        }
      };
    result.push(
      {
        id: "render_draws",
        count: n,
        graphics: true,
        setup: graphics(false, false),
        step: () => {},
      },
      {
        id: "render_overdraw",
        count: quick ? 16 : 192,
        graphics: true,
        setup: graphics(true, false),
        step: () => {},
      },
      {
        id: "render_text",
        count: quick ? 16 : 256,
        graphics: true,
        setup: graphics(false, true),
        step: () => {},
      },
    );
  }
  return result;
}
function cleanup(r: Run): void {
  for (const root of r.roots) root.destroy();
  r.root = null;
  r.roots = [];
  r.entities = [];
}
function restore(r: Run): void {
  if (Engine.mode === "client") {
    Engine.setMainCamera(r.previousCamera ?? "");
    if (r.previous?.frame) {
      Engine.setVsync(r.previous.frame.vsync);
      Engine.setFpsCap(r.previous.frame.fps_cap);
    }
  }
}
function beginStage(r: Run): void {
  cleanup(r);
  if (r.index >= r.stages.length) {
    finish(r);
    return;
  }
  r.root = entity();
  const stage = r.stages[r.index]!;
  stage.setup();
  r.frame = -3;
  Engine.log("info", `Markbench stage ${stage.id} (${stage.count} work units/tick)`);
}
function summarize(r: Run) {
  const results = r.stages.map((stage, i) => {
    const samples = r.samples[i]!;
    const metric = (
      key:
        | "total_ms"
        | "scripts_ms"
        | "commands_ms"
        | "physics_ms"
        | "snapshot_ms"
        | "presentation_ms"
        | "snapshot_bytes",
    ) => distribution(samples.map((sample) => sample.native[key]));
    const native = {
      total_ms: metric("total_ms"),
      scripts_ms: metric("scripts_ms"),
      commands_ms: metric("commands_ms"),
      physics_ms: metric("physics_ms"),
      snapshot_ms: metric("snapshot_ms"),
      presentation_ms: metric("presentation_ms"),
      snapshot_bytes: metric("snapshot_bytes"),
    };
    const frames = r.frames[i]!.filter(
      (frame): frame is NonNullable<typeof frame> => frame !== null,
    );
    const frame_ms = distribution(frames.map((frame) => frame.frame_ms));
    return {
      id: stage.id,
      units_per_tick: stage.count,
      samples: samples.length,
      valid: samples.length === r.measured,
      native,
      workload_ms: distribution(samples.map((sample) => sample.workload_ms)),
      runtime_script_ms: distribution(samples.map((sample) => sample.runtime.script_ms)),
      runtime_snapshot_ms: distribution(samples.map((sample) => sample.runtime.snapshot_ms)),
      frame_ms,
      render_cpu_ms: distribution(frames.map((frame) => frame.cpu_ms)),
      surface_wait_ms: distribution(frames.map((frame) => frame.acquire_ms)),
      draw_calls: distribution(frames.map((frame) => frame.draw_calls)),
      score: score(native.total_ms.p95),
      presentation_score: stage.graphics ? score(frame_ms.p95) : null,
    };
  });
  const complete = results.every((stage) => stage.valid) && r.pending === 0 && !r.failure;
  return {
    schema: 1,
    suite: "markbench-1",
    preset: r.preset,
    mode: Engine.mode,
    complete,
    failure: r.failure,
    elapsed_ms: Profiler.now() - r.started,
    warmup_ticks: r.warmup,
    measured_ticks: r.measured,
    baseline_entities: r.baseline,
    environment: Profiler.latest(),
    memory_before: r.memory,
    memory_after: Profiler.memory(),
    codec_round_trips: r.codecs,
    checksum,
    results,
    score: complete
      ? geometricMean(results.filter((_, i) => !r.stages[i]!.graphics).map((stage) => stage.score))
      : null,
    presentation_score: complete
      ? geometricMean(results.map((stage) => stage.presentation_score))
      : null,
    score_note:
      "1000 = p95 within a 60 Hz budget. Compare the same preset, mode, build, resolution and mod set. GPU execution timestamps unavailable.",
  };
}
function finish(r: Run): void {
  restore(r);
  // Codec continuations drain before publishing a complete report.
  if (r.pending) {
    r.index = r.stages.length;
    return;
  }
  report = summarize(r);
  run = null;
  Engine.log("info", `MARKBENCH_RESULT ${JSON.stringify(report)}`);
  void File.write("markbench-latest.json", JSON.stringify(report, null, 2)).catch((error) =>
    Engine.log("error", "Markbench report write failed: " + error),
  );
}
function cancel(reason: string): string {
  if (!run) return "Markbench is idle";
  const r = run;
  r.failure = reason;
  cleanup(r);
  restore(r);
  report = summarize(r);
  run = null;
  return `Markbench stopped: ${reason}`;
}
Console.register({
  name: "markbench",
  description: "Deterministic engine stress suite",
  subcommands: [
    {
      name: "run",
      args: [{ name: "preset", type: "string", suggest: () => ["quick", "standard"] }],
      handler: ({ preset }) => {
        if (run) throw new Error("Markbench is already running");
        const name = String(preset ?? "standard");
        if (!["quick", "standard"].includes(name))
          throw new Error("Preset must be quick or standard");
        if (!Profiler.latest() || !endpoint)
          throw new Error("Wait for Markbench ready before running");
        const quick = name === "quick";
        const list = stages(quick);
        const r: Run = {
          preset: name,
          warmup: quick ? 4 : 60,
          measured: quick ? 12 : 240,
          stages: list,
          index: 0,
          frame: 0,
          samples: list.map(() => []),
          frames: list.map(() => []),
          scheduled: new Map(),
          roots: [],
          entities: [],
          root: null,
          previousCamera: Engine.mode === "client" ? Engine.getMainCamera() : null,
          previous: Profiler.latest(),
          baseline: Scene.listEntities().length,
          memory: Profiler.memory(),
          started: Profiler.now(),
          lastFrame: 0,
          pending: 0,
          codecs: 0,
          failure: null,
        };
        checksum = 0;
        run = r;
        if (Engine.mode === "client") {
          Engine.setVsync(false);
          Engine.setFpsCap(null);
        }
        beginStage(r);
        return `Markbench ${name} started; ${list.length} stages. Use markbench status or cancel.`;
      },
    },
    {
      name: "status",
      handler: () =>
        run
          ? `${run.preset}: ${run.stages[run.index]?.id ?? "draining"}, tick ${Math.max(0, run.frame)}/${run.warmup + run.measured}`
          : "Markbench idle",
    },
    { name: "cancel", handler: () => cancel("cancelled") },
    { name: "report", handler: () => (report ? JSON.stringify(report) : "No completed report") },
  ],
});
Engine.on("tick", ({ tick_number }) => {
  const r = run;
  if (!r) return;
  try {
    const sample = Profiler.latest();
    if (sample) {
      const scheduled = r.scheduled.get(sample.tick_number);
      if (scheduled) {
        r.samples[scheduled.stage]!.push({
          native: sample as TickTimings,
          runtime: Profiler.runtime(),
          workload_ms: scheduled.workload_ms,
        });
        r.scheduled.delete(sample.tick_number);
        if (sample.frame && sample.frame.frame_number !== r.lastFrame) {
          r.frames[scheduled.stage]!.push(sample.frame as NonNullable<TickTimings["frame"]>);
          r.lastFrame = sample.frame.frame_number;
        }
      }
    }
    if (r.failure) {
      cancel(r.failure);
      return;
    }
    if (r.index >= r.stages.length) {
      finish(r);
      return;
    }
    if (r.frame >= r.warmup + r.measured) {
      r.index++;
      beginStage(r);
      return;
    }
    if (r.frame >= 0) {
      const started = Profiler.now();
      r.stages[r.index]!.step(tick_number);
      if (r.frame >= r.warmup)
        r.scheduled.set(tick_number, { stage: r.index, workload_ms: Profiler.now() - started });
    }
    r.frame++;
  } catch (error) {
    Engine.log("error", cancel(String(error)));
  }
});
