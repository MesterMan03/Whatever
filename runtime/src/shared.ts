import type { EventName, JsonValue, ModManifest } from "./types.ts";

// --- Shared mutable IPC state ---

export const _handlers = new Map<EventName, Set<(payload: any) => void | Promise<void>>>();
export const _fileCallbacks = new Map<string, { resolve: (v: string | null) => void; reject: (e: Error) => void }>();
export const _modListCallbacks = new Map<string, { resolve: (v: ModManifest[]) => void; reject: (e: Error) => void }>();
export const _modGetCallbacks = new Map<string, { resolve: (v: ModManifest) => void; reject: (e: Error) => void }>();
export const _msgCallbacks = new Map<string, { resolve: (v: JsonValue) => void; reject: (e: Error) => void }>();
export const _entityCallbacks = new Map<string, { resolve: (v: string) => void; reject: (e: Error) => void }>();
export const _entityListCallbacks = new Map<string, { resolve: (v: string[]) => void; reject: (e: Error) => void }>();
export const _componentGetCallbacks = new Map<string, { resolve: (v: JsonValue | null) => void; reject: (e: Error) => void }>();
export const _componentQueryCallbacks = new Map<string, { resolve: (v: Array<{ entity_id: string; components: Record<string, JsonValue> }>) => void; reject: (e: Error) => void }>();

const _counter = { n: 0 };
/** Returns the next unique request ID string. */
export function nextReqId(): string {
  return String(++_counter.n);
}

/** Serialise a message to stdout as NDJSON. Typed as `any` to avoid a circular
 *  dependency between this shared module and the ipc.ts type definitions. */
export function _send(msg: any): void {
  process.stdout.write(JSON.stringify(msg) + "\n");
}

/** Maps public event names to the Rust IPC message type that triggers them.
 *  `mod_message` is absent — it is dispatched specially and needs no Subscribe. */
export const _EVENT_SUBSCRIBE: Partial<Record<EventName, string>> = {
  init: "Init",
  exit: "Shutdown",
  tick: "Tick",
};
