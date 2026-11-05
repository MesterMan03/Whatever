// Side-effect import — starts the private IPC loop.
import "./ipc.ts";

export type {
  RuntimeMode,
  JsonValue,
  ModManifest,
  EventPayloads,
  EventName,
  QueryResult,
} from "./types.ts";
export { Component, field } from "./component.ts";
export type { ComponentClass, ComponentData, FieldEditor } from "./component.ts";
export { Type, DataType, defineStruct as Struct } from "./schema.ts";
export type { Schema, SchemaValue as Infer } from "./schema.ts";
export { Engine } from "./components/engine.ts";
export { Input, PressedKeys } from "./input.ts";
export type { KeyCode } from "./input.ts";
export { Time } from "./time.ts";
export { Window } from "./components/window.ts";
export { File } from "./components/file.ts";
export { BuiltInComponents, Entity, PendingEntity, LoadedScene, Scene } from "./components/ecs.ts";
export { Prefab, PrefabInstance } from "./components/ecs.ts";
export type { Pending } from "./pending.ts";
export { EntityRef } from "./references.ts";
export { Physics } from "./components/physics.ts";
export type {
  CloneOptions,
  TagQuery,
  ComponentRegistry,
  ComponentSetRegistry,
} from "./components/ecs.ts";
export type { EntityReference, ComponentValue } from "./references.ts";
export { Mods, Message } from "./components/mods.ts";
export { Console } from "./components/console.ts";
export type { ArgType, ArgSpec, CommandSpec } from "./components/console.ts";
export { Audio } from "./components/audio.ts";
export type { AudioHandle, AudioMetadata, CloseStrategy } from "./components/audio.ts";

export { Network } from "./components/network.ts";

export { SceneSnapshot } from "./scene-store.ts";
export type { ComponentSnapshot, DeepReadonly } from "./scene-store.ts";

export { Profiler } from "./profiler.ts";
export type { TickTimings, FrameTimings } from "./profiler.ts";
