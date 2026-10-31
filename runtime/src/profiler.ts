import { _now, _memory } from "./transport.ts";
import { immutable, type DeepReadonly } from "./scene-store.ts";
export interface FrameTimings {
  frame_number: number;
  frame_ms: number;
  cpu_ms: number;
  acquire_ms: number;
  encode_ms: number;
  submit_ms: number;
  present_ms: number;
  draw_calls: number;
  width: number;
  height: number;
  vsync: boolean;
  fps_cap: number | null;
  adapter: string;
  gpu_ms: number | null;
}
export interface TickTimings {
  tick_number: number;
  tick_rate: number;
  total_ms: number;
  scripts_ms: number;
  commands_ms: number;
  physics_ms: number;
  presentation_ms: number;
  snapshot_ms: number;
  snapshot_bytes: number;
  commands: number;
  entities: number;
  hosts: number;
  build: string;
  platform: string;
  frame: FrameTimings | null;
}
let latest: DeepReadonly<TickTimings> | null = null;
let runtime = { tick_number: 0, snapshot_ms: 0, script_ms: 0 };
export function _performance(sample: TickTimings): void {
  latest = immutable(sample);
}
export function _runtimePerformance(sample: typeof runtime): void {
  runtime = Object.freeze(sample);
}
/** Native engine CPU measurements and a captured monotonic nanosecond clock. */
export const Profiler = {
  now: _now,
  latest(): DeepReadonly<TickTimings> | null {
    return latest;
  },
  runtime(): Readonly<typeof runtime> {
    return runtime;
  },
  memory(): Readonly<{ heap_used: number; rss: number }> {
    return Object.freeze(_memory());
  },
};
