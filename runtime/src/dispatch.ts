import { _performance, _runtimePerformance, type TickTimings } from "./profiler.ts";
import { _now, _beginTurn } from "./transport.ts";
import { resolveEntityAlias, rejectEntityAlias } from "./scene-store.ts";
import { _syncSceneComponents, _tickComponents, _shutdownComponents } from "./component.ts";
import {
  handleResponse as handleNetworkResponse,
  handleEvent as handleNetworkEvent,
  type NetworkEvent,
} from "./network/registration.ts";
import { _handlePhysicsResponse } from "./components/physics.ts";
import { hydrateReferences } from "./components/ecs.ts";
import type { JsonValue, ModManifest, EventName, EventPayloads } from "./types.ts";
import {
  _startupScene,
  _tagCallbacks,
  _handlers,
  _fileCallbacks,
  _modListCallbacks,
  _modGetCallbacks,
  _msgCallbacks,
  _entityCallbacks,
  _entityListCallbacks,
  _componentGetCallbacks,
  _componentQueryCallbacks,
  _entityParentCallbacks,
  _entityChildrenCallbacks,
  _mainCameraCallbacks,
  _sceneCallbacks,
  _EVENT_SUBSCRIBE,
  _send,
} from "./shared.ts";
import { _handleCommandInvoke, _handleArgSuggestRequest } from "./components/console.ts";
import {
  _audioLoadCallbacks,
  _audioStateCallbacks,
  _audioHandles,
  AudioHandle,
  type AudioMetadata,
} from "./components/audio.ts";

import { _exit, _installDispatcher } from "./transport.ts";
import { Input, _updateInput, type KeyCode } from "./input.ts";
import { _updateTime } from "./time.ts";

// Internal delivery types — match Rust serde tags exactly, not part of the public API.
type _EngineMsg =
  | { type: "Performance"; sample: TickTimings }
  | { type: "EntityTagsResponse"; request_id: string; tags: string[]; error: string | null }
  | {
      type: "SceneLoaded";
      request_id: string;
      scene: import("./shared.ts").LoadedSceneData | null;
      error: string | null;
    }
  | { type: "MainCameraResponse"; request_id: string; entity_id: string | null }
  | { type: "PhysicsResponse"; request_id: string; result: unknown; error: string | null }
  | { type: "PhysicsEvent"; event: unknown }
  | ({ type: "NetworkEvent" } & NetworkEvent)
  | { type: "NetworkResponse"; request_id: string; result: unknown; error: string | null }
  | { type: "CommandInvoke"; request_id: string; command_path: string[]; args: JsonValue[] }
  | {
      type: "ArgSuggestRequest";
      request_id: string;
      command_path: string[];
      arg_index: number;
      current: string;
    }
  | {
      type: "Init";
      initial_scene?: import("./shared.ts").LoadedSceneData | null;
      mod_id: string;
      engine_version: string;
      mode: import("./types.ts").RuntimeMode;
    }
  | { type: "Frame"; delta_seconds: number; frame_number: number }
  | { type: "Input"; keys_pressed: KeyCode[]; mouse_delta: [number, number] }
  | {
      type: "Tick";
      tick_number: number;
      delta_seconds: number;
      keys_pressed: KeyCode[];
      mouse_delta: [number, number];
      mouse_captured: boolean;
      mouse_buttons?: import("./input.ts").MouseButtonCode[];
      mouse_buttons_pressed?: import("./input.ts").MouseButtonCode[];
    }
  | {
      type: "AssetResponse";
      request_id: string;
      path: string;
      data_base64: string | null;
      error: string | null;
    }
  | { type: "FileResponse"; request_id: string; data_base64: string | null; error: string | null }
  | { type: "ModListResponse"; request_id: string; mods: ModManifest[] }
  | {
      type: "ModGetResponse";
      request_id: string;
      manifest: ModManifest | null;
      error: string | null;
    }
  | {
      type: "ModMessageReceived";
      source_mod_id: string;
      request_id: string | null;
      payload: JsonValue;
    }
  | { type: "ModMessageReplyDelivered"; request_id: string; payload: JsonValue }
  | { type: "EntityCreateFailed"; request_id: string; error: string }
  | { type: "EntityCreated"; request_id: string; entity_id: string }
  | { type: "EntityListResponse"; request_id: string; entity_ids: string[] }
  | {
      type: "ComponentGetResponse";
      request_id: string;
      entity_id: string;
      component_type: string;
      data: JsonValue | null;
      error: string | null;
    }
  | {
      type: "ComponentQueryResponse";
      request_id: string;
      results: Array<{ entity_id: string; components: Record<string, JsonValue> }>;
    }
  | {
      type: "EntityParentResponse";
      request_id: string;
      entity_id: string;
      parent_id: string | null;
    }
  | { type: "EntityChildrenResponse"; request_id: string; entity_id: string; child_ids: string[] }
  | {
      type: "AudioLoaded";
      request_id: string;
      audio_id: string;
      duration_ms: number | null;
      sample_rate: number;
      channels: number;
      error: string | null;
    }
  | {
      type: "AudioState";
      request_id: string;
      audio_id: string;
      position_ms: number;
      volume: number;
      speed: number;
      is_playing: boolean;
      is_looping: boolean;
      error: string | null;
    }
  | { type: "AudioClose"; audio_id: string }
  | { type: "Shutdown"; exit_code: number };

const tickStates = new Map<
  (payload: any) => void | Promise<void>,
  {
    busy: boolean;
    delta: number;
    mouse: [number, number];
  }
>();
let initialized = false;
let failed = false;

export function _dispatch(msg: _EngineMsg): void {
  _beginTurn();
  if (msg.type === "Performance") {
    _performance(msg.sample);
    return;
  }
  if (msg.type === "EntityTagsResponse") {
    const callback = _tagCallbacks.get(msg.request_id);
    _tagCallbacks.delete(msg.request_id);
    if (callback) msg.error ? callback.reject(new Error(msg.error)) : callback.resolve(msg.tags);
    return;
  }
  if (msg.type === "SceneLoaded") {
    const callback = _sceneCallbacks.get(msg.request_id);
    if (callback) {
      _sceneCallbacks.delete(msg.request_id);
      if (msg.error || !msg.scene) callback.reject(new Error(msg.error ?? "Scene load failed"));
      else callback.resolve(msg.scene);
    }
    return;
  }
  if (msg.type === "MainCameraResponse") {
    const callback = _mainCameraCallbacks.get(msg.request_id);
    if (callback) {
      _mainCameraCallbacks.delete(msg.request_id);
      callback.resolve(msg.entity_id);
    }
    return;
  }
  if (msg.type === "PhysicsResponse") {
    _handlePhysicsResponse(msg);
    return;
  }
  if (msg.type === "PhysicsEvent") {
    for (const handler of _handlers.get("physics") ?? [])
      handler({ event: hydrateReferences(msg.event) });
    return;
  }
  if (msg.type === "NetworkResponse") {
    handleNetworkResponse(msg);
    return;
  }
  if (msg.type === "NetworkEvent") {
    handleNetworkEvent(msg);
    return;
  }
  if (msg.type === "CommandInvoke") {
    _handleCommandInvoke(msg);
    return;
  }

  if (msg.type === "ArgSuggestRequest") {
    _handleArgSuggestRequest(msg);
    return;
  }

  if (msg.type === "Tick") {
    (globalThis as any).__advanceTimers?.(msg.delta_seconds * 1000);
    const scriptStart = _now();
    _updateTime(msg.tick_number, msg.delta_seconds);
    _updateInput(
      msg.keys_pressed,
      msg.mouse_delta,
      msg.mouse_captured,
      msg.mouse_buttons,
      msg.mouse_buttons_pressed,
    );
    const fail = (error: unknown): void => {
      _send({ type: "Log", level: "error", message: "Tick handler failed: " + String(error) });
      failed = true;
    };
    if (initialized && !failed) {
      _syncSceneComponents();
      _tickComponents(msg.delta_seconds);
      for (const handler of _handlers.get("tick") ?? []) {
        const state = tickStates.get(handler) ?? {
          busy: false,
          delta: 0,
          mouse: [0, 0] as [number, number],
        };
        tickStates.set(handler, state);
        state.delta += msg.delta_seconds;
        if (msg.mouse_captured) {
          state.mouse[0] += msg.mouse_delta[0];
          state.mouse[1] += msg.mouse_delta[1];
        } else {
          state.mouse = [0, 0];
        }
        if (state.busy) continue;
        const payload: EventPayloads["tick"] = {
          ...msg,
          keys_pressed: Input.keysPressed,
          delta_seconds: state.delta,
          mouse_delta: [...state.mouse],
        };
        state.delta = 0;
        state.mouse = [0, 0];
        try {
          const result = handler(payload);
          if (result && typeof result.then === "function") {
            state.busy = true;
            void Promise.resolve(result).then(
              () => {
                state.busy = false;
              },
              (error) => {
                state.busy = false;
                fail(error);
              },
            );
          }
        } catch (error) {
          fail(error);
          break;
        }
      }
    }
    // Hooks run synchronously; only genuinely asynchronous work spans turns.
    _runtimePerformance({
      tick_number: msg.tick_number,
      snapshot_ms: 0,
      script_ms: _now() - scriptStart,
    });
    if (failed) throw new Error("Mod tick failed");
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

  if (msg.type === "EntityCreateFailed") {
    rejectEntityAlias("pending:" + msg.request_id);
    const cb = _entityCallbacks.get(msg.request_id);
    _entityCallbacks.delete(msg.request_id);
    cb?.reject(new Error(msg.error));
    return;
  }
  if (msg.type === "EntityCreated") {
    resolveEntityAlias("pending:" + msg.request_id, msg.entity_id);
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

  if (msg.type === "Init") {
    _startupScene.data = msg.initial_scene ?? null;
    initialized = true;
  }
  if (msg.type === "Shutdown") _shutdownComponents();
  // Init / Shutdown / any future broadcast-style messages.
  for (const [event, msgType] of Object.entries(_EVENT_SUBSCRIBE) as [EventName, string][]) {
    if (msg.type !== msgType) continue;
    const handlers = _handlers.get(event);
    if (handlers) {
      for (const fn_ of handlers) fn_(msg);
    }
  }
}

_installDispatcher(_dispatch);
