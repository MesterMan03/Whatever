import type { EventName, JsonValue, ModManifest } from "./types.ts";

import { _write } from "./transport.ts";
const _stringify = JSON.stringify;

// --- Shared mutable IPC state ---

export const _handlers = new Map<EventName, Set<(payload: any) => void | Promise<void>>>();
export const _fileCallbacks = new Map<
  string,
  { resolve: (v: string | null) => void; reject: (e: Error) => void }
>();
export const _modListCallbacks = new Map<
  string,
  { resolve: (v: ModManifest[]) => void; reject: (e: Error) => void }
>();
export const _modGetCallbacks = new Map<
  string,
  { resolve: (v: ModManifest) => void; reject: (e: Error) => void }
>();
export const _msgCallbacks = new Map<
  string,
  { resolve: (v: JsonValue) => void; reject: (e: Error) => void }
>();
export const _entityCallbacks = new Map<
  string,
  { resolve: (v: string) => void; reject: (e: Error) => void }
>();
export const _entityListCallbacks = new Map<
  string,
  { resolve: (v: string[]) => void; reject: (e: Error) => void }
>();
export const _mainCameraCallbacks = new Map<
  string,
  { resolve: (value: string | null) => void; reject: (error: Error) => void }
>();
export const _componentGetCallbacks = new Map<
  string,
  { resolve: (v: JsonValue | null) => void; reject: (e: Error) => void }
>();
export const _componentQueryCallbacks = new Map<
  string,
  {
    resolve: (v: Array<{ entity_id: string; components: Record<string, JsonValue> }>) => void;
    reject: (e: Error) => void;
  }
>();
export const _entityParentCallbacks = new Map<
  string,
  { resolve: (v: string | null) => void; reject: (e: Error) => void }
>();
export const _entityChildrenCallbacks = new Map<
  string,
  { resolve: (v: string[]) => void; reject: (e: Error) => void }
>();

const _counter = { n: 0 };
export const _startupScene: { data: LoadedSceneData | null } = { data: null };
export interface LoadedSceneData {
  path?: string;
  entity_ids: string[];
  script_ids: Record<string, string>;
  load_time: number;
}
export const _sceneCallbacks = new Map<
  string,
  { resolve: (scene: LoadedSceneData) => void; reject: (error: Error) => void }
>();
/** Returns the next unique request ID string. */
export function nextReqId(): string {
  return String(++_counter.n);
}

let commands: string[] = [];

/** Record immediately, including a snapshot of mutable arguments, in call order. */
export function _send(msg: any): void {
  commands.push(_stringify(msg));
}

/** Private automatic tick listener. Even an empty buffer acknowledges submission. */
export function _flush(tick_number: number): void {
  const batch = commands;
  commands = [];
  _write(
    '{"type":"CommandBatch","tick_number":' +
      tick_number +
      ',"commands":[' +
      batch.join(",") +
      "]}\n",
  );
}

/** Maps public event names to the Rust IPC message type that triggers them.
 *  `mod_message` is absent — it is dispatched specially and needs no Subscribe. */
export const _EVENT_SUBSCRIBE: Partial<Record<EventName, string>> = {
  init: "Init",
  exit: "Shutdown",
  tick: "Tick",
  physics: "PhysicsEvent",
};
