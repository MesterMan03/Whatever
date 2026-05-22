// Side-effect import — starts the readline IPC loop.
import "./ipc.ts";

export type { Component, JsonValue, ModManifest, EventPayloads, EventName, QueryResult } from "./types.ts";
export { Engine } from "./components/engine.ts";
export { Window } from "./components/window.ts";
export { File } from "./components/file.ts";
export { BuiltInComponents, Entity, Scene } from "./components/ecs.ts";
export { Mods, Message } from "./components/mods.ts";
export { Console } from "./components/console.ts";
export type { ArgType, ArgSpec, CommandSpec } from "./components/console.ts";
