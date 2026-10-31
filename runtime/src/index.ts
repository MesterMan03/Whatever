// Side-effect import — starts the private IPC loop.
import "./ipc.ts";

export type { RuntimeMode, Component, JsonValue, ModManifest, EventPayloads, EventName, QueryResult } from "./types.ts";
export { Engine } from "./components/engine.ts";
export { Window } from "./components/window.ts";
export { File } from "./components/file.ts";
export { BuiltInComponents, Entity, PendingEntity, LoadedScene, Scene } from "./components/ecs.ts";
export type { Pending } from "./pending.ts";
export { EntityRef } from "./references.ts";
export { Physics } from "./components/physics.ts";
export type { ComponentRegistry, ComponentSetRegistry } from "./components/ecs.ts";
export type { EntityReference, ComponentValue } from "./references.ts";
export { Mods, Message } from "./components/mods.ts";
export { Console } from "./components/console.ts";
export type { ArgType, ArgSpec, CommandSpec } from "./components/console.ts";
export { Audio } from "./components/audio.ts";
export type { AudioHandle, AudioMetadata, CloseStrategy } from "./components/audio.ts";

export { Network } from "./components/network.ts";
