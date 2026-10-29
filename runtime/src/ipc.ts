import type { JsonValue, ModManifest, EventName, EventPayloads } from "./types.ts";
import {
  _handlers, _fileCallbacks, _modListCallbacks, _modGetCallbacks, _msgCallbacks,
  _entityCallbacks, _entityListCallbacks, _componentGetCallbacks, _componentQueryCallbacks,
  _entityParentCallbacks, _entityChildrenCallbacks,
  _EVENT_SUBSCRIBE, _send,
} from "./shared.ts";
import { _handleCommandInvoke, _handleArgSuggestRequest } from "./components/console.ts";
import {
  _audioLoadCallbacks, _audioStateCallbacks, _audioHandles, AudioHandle,
  type AudioMetadata,
} from "./components/audio.ts";

import { _exit, _stdin } from "./transport.ts";
const _decoder = new TextDecoder();

// Internal IPC types — match Rust serde tags exactly, not part of the public API.
type _EngineMsg =
  | { type: "CommandInvoke"; request_id: string; command_path: string[]; args: JsonValue[] }
  | { type: "ArgSuggestRequest"; request_id: string; command_path: string[]; arg_index: number; current: string }
  | { type: "Init"; mod_id: string; engine_version: string; mode: import("./types.ts").RuntimeMode }
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
  | { type: "EntityParentResponse"; request_id: string; entity_id: string; parent_id: string | null }
  | { type: "EntityChildrenResponse"; request_id: string; entity_id: string; child_ids: string[] }
  | { type: "AudioLoaded"; request_id: string; audio_id: string; duration_ms: number | null; sample_rate: number; channels: number; error: string | null }
  | { type: "AudioState"; request_id: string; audio_id: string; position_ms: number; volume: number; speed: number; is_playing: boolean; is_looping: boolean; error: string | null }
  | { type: "AudioClose"; audio_id: string }
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
    const fail = (error: unknown): never => {
      try { _send({type: "Log", level: "error", message: "Tick handler failed: " + String(error)}); }
      finally { return _exit(1); }
    };
    const promises: Promise<void>[] = [];
    try {
      for (const handler of _handlers.get("tick") ?? []) {
        const result = handler(msg);
        if (result instanceof Promise) promises.push(result);
      }
    } catch (error) { fail(error); }
    void Promise.all(promises).then(
      () => _send({ type: "TickDone", tick_number: msg.tick_number }),
      fail,
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

  if (msg.type === "EntityParentResponse") {
    const cb = _entityParentCallbacks.get(msg.request_id);
    if (cb) {
      _entityParentCallbacks.delete(msg.request_id);
      cb.resolve(msg.parent_id);
    }
    return;
  }

  if (msg.type === "EntityChildrenResponse") {
    const cb = _entityChildrenCallbacks.get(msg.request_id);
    if (cb) {
      _entityChildrenCallbacks.delete(msg.request_id);
      cb.resolve(msg.child_ids);
    }
    return;
  }

  if (msg.type === "AudioLoaded") {
    const cb = _audioLoadCallbacks.get(msg.request_id);
    if (cb) {
      _audioLoadCallbacks.delete(msg.request_id);
      if (msg.error) {
        cb.reject(new Error(msg.error));
      } else {
        const meta: AudioMetadata = {
          duration_ms: msg.duration_ms,
          sample_rate: msg.sample_rate,
          channels: msg.channels,
        };
        cb.resolve(new AudioHandle(msg.audio_id, meta));
      }
    }
    return;
  }

  if (msg.type === "AudioState") {
    const cb = _audioStateCallbacks.get(msg.request_id);
    if (cb) {
      _audioStateCallbacks.delete(msg.request_id);
      if (msg.error) {
        cb.reject(new Error(msg.error));
      } else {
        cb.resolve({
          position_ms: msg.position_ms,
          volume: msg.volume,
          speed: msg.speed,
          is_playing: msg.is_playing,
          is_looping: msg.is_looping,
        });
      }
    }
    return;
  }

  if (msg.type === "AudioClose") {
    const handle = _audioHandles.get(msg.audio_id);
    if (handle) handle._handleClose();
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
      _exit((msg as Extract<_EngineMsg, { type: "Shutdown" }>).exit_code);
    }
  }
}

// Acquire the pipe before lockdown; retain no public handle to it.
void (async () => {
  let remaining = "";
  for (;;) {
    const { value, done } = await _stdin.read();
    if (done) { _exit(0); }
    remaining += _decoder.decode(value, { stream: true });
    let newline: number;
    while ((newline = remaining.indexOf("\n")) >= 0) {
      const line = remaining.slice(0, newline);
      remaining = remaining.slice(newline + 1);
      try { _dispatch(JSON.parse(line) as _EngineMsg); }
      catch { /* Ignore malformed messages. */ }
    }
  }
})();
