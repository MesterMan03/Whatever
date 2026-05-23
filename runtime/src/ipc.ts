import { createInterface } from "node:readline";
import type { JsonValue, ModManifest, EventName, EventPayloads } from "./types.ts";
import {
  _handlers, _fileCallbacks, _modListCallbacks, _modGetCallbacks, _msgCallbacks,
  _entityCallbacks, _entityListCallbacks, _componentGetCallbacks, _componentQueryCallbacks,
  _EVENT_SUBSCRIBE,
} from "./shared.ts";
import { _handleCommandInvoke, _handleArgSuggestRequest } from "./components/console.ts";

// Internal IPC types — match Rust serde tags exactly, not part of the public API.
type _EngineMsg =
  | { type: "CommandInvoke"; request_id: string; command_path: string[]; args: JsonValue[] }
  | { type: "ArgSuggestRequest"; request_id: string; command_path: string[]; arg_index: number; current: string }
  | { type: "Init"; mod_id: string; engine_version: string }
  | { type: "Frame"; delta_seconds: number; frame_number: number }
  | { type: "Input"; keys_pressed: string[]; mouse_delta: [number, number] }
  | { type: "Tick"; tick_number: number; delta_seconds: number; keys_pressed: string[]; mouse_delta: [number, number] }
  | { type: "AssetResponse"; request_id: string; path: string; data_base64: string | null; error: string | null }
  | { type: "FileResponse"; request_id: string; data_base64: string | null; error: string | null }
  | { type: "ModListResponse"; request_id: string; mods: ModManifest[] }
  | { type: "ModGetResponse"; request_id: string; manifest: ModManifest | null; error: string | null }
  | { type: "ModMessageReceived"; source_mod_id: string; request_id: string | null; payload: JsonValue }
  | { type: "ModMessageReplyDelivered"; request_id: string; payload: JsonValue }
  | { type: "EntityCreated"; request_id: string; entity_id: string }
  | { type: "EntityListResponse"; request_id: string; entity_ids: string[] }
  | { type: "ComponentGetResponse"; request_id: string; entity_id: string; component_type: string; data: JsonValue | null; error: string | null }
  | { type: "ComponentQueryResponse"; request_id: string; results: Array<{ entity_id: string; components: Record<string, JsonValue> }> }
  | { type: "Shutdown"; exit_code: number };

function _dispatch(msg: _EngineMsg): void {
  if (msg.type === "CommandInvoke") {
    _handleCommandInvoke(msg);
    return;
  }

  if (msg.type === "ArgSuggestRequest") {
    _handleArgSuggestRequest(msg);
    return;
  }

  if (msg.type === "Tick") {
    const handlers = _handlers.get("tick");
    const promises: Promise<void>[] = [];
    if (handlers) {
      for (const fn_ of handlers) {
        const r = fn_(msg);
        if (r instanceof Promise) promises.push(r);
      }
    }
    Promise.all(promises).then(() =>
      process.stdout.write(JSON.stringify({ type: "TickDone", tick_number: msg.tick_number }) + "\n")
    );
    return;
  }

  if (msg.type === "FileResponse") {
    const cb = _fileCallbacks.get(msg.request_id);
    if (cb) {
      _fileCallbacks.delete(msg.request_id);
      msg.error ? cb.reject(new Error(msg.error)) : cb.resolve(msg.data_base64);
    }
    return;
  }

  if (msg.type === "ModListResponse") {
    const cb = _modListCallbacks.get(msg.request_id);
    if (cb) {
      _modListCallbacks.delete(msg.request_id);
      cb.resolve(msg.mods);
    }
    return;
  }

  if (msg.type === "ModGetResponse") {
    const cb = _modGetCallbacks.get(msg.request_id);
    if (cb) {
      _modGetCallbacks.delete(msg.request_id);
      msg.error ? cb.reject(new Error(msg.error)) : cb.resolve(msg.manifest!);
    }
    return;
  }

  if (msg.type === "ModMessageReplyDelivered") {
    const cb = _msgCallbacks.get(msg.request_id);
    if (cb) {
      _msgCallbacks.delete(msg.request_id);
      cb.resolve(msg.payload);
    }
    return;
  }

  if (msg.type === "ModMessageReceived") {
    const handlers = _handlers.get("mod_message");
    if (handlers) {
      const payload: EventPayloads["mod_message"] = {
        source_mod_id: msg.source_mod_id,
        message: msg.payload,
        ...(msg.request_id !== null && { request_id: msg.request_id }),
      };
      for (const fn_ of handlers) fn_(payload);
    }
    return;
  }

  if (msg.type === "EntityCreated") {
    const cb = _entityCallbacks.get(msg.request_id);
    if (cb) {
      _entityCallbacks.delete(msg.request_id);
      cb.resolve(msg.entity_id);
    }
    return;
  }

  if (msg.type === "EntityListResponse") {
    const cb = _entityListCallbacks.get(msg.request_id);
    if (cb) {
      _entityListCallbacks.delete(msg.request_id);
      cb.resolve(msg.entity_ids);
    }
    return;
  }

  if (msg.type === "ComponentGetResponse") {
    const cb = _componentGetCallbacks.get(msg.request_id);
    if (cb) {
      _componentGetCallbacks.delete(msg.request_id);
      msg.error ? cb.reject(new Error(msg.error)) : cb.resolve(msg.data);
    }
    return;
  }

  if (msg.type === "ComponentQueryResponse") {
    const cb = _componentQueryCallbacks.get(msg.request_id);
    if (cb) {
      _componentQueryCallbacks.delete(msg.request_id);
      cb.resolve(msg.results);
    }
    return;
  }

  // Init / Shutdown / any future broadcast-style messages.
  for (const [event, msgType] of Object.entries(_EVENT_SUBSCRIBE) as [EventName, string][]) {
    if (msg.type !== msgType) continue;
    const handlers = _handlers.get(event);
    if (handlers) {
      for (const fn_ of handlers) fn_(msg);
    }
    if (event === "exit") {
      process.exit((msg as Extract<_EngineMsg, { type: "Shutdown" }>).exit_code);
    }
  }
}

// Start reading from stdin — side-effect only, no exports needed.
const _rl = createInterface({ input: process.stdin, terminal: false });
_rl.on("line", (line) => {
  try {
    _dispatch(JSON.parse(line) as _EngineMsg);
  } catch {
    // ignore malformed messages
  }
});
_rl.on("close", () => process.exit(0));
