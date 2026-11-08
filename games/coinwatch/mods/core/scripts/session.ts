import type { RunSession } from "./RunSession.ts";
import { RunState as RunStateComponent } from "./RunState.ts";
import { Entity, Scene, Time, type Physics, type ComponentData } from "@whatever-engine/api";
export type Ref = { readonly id: string; toJSON(): { $entity: string } };
export const id = (ref: Ref | null): string => ref?.toJSON().$entity ?? "";
export const resolve = (ref: Ref | null): Entity | null => (ref ? Scene.find(id(ref)) : null);
export const position = (entity: Entity): Physics.Vec3 => [
  ...(entity.getComponent("core:world_transform")?.position ?? [0, 0, 0]),
];
export const library = "core://libraries/whatever_starter/";
export type RunState = ComponentData<RunStateComponent>;
export type GameEvent =
  | { type: "award"; amount: number; kind: "coin" | "kill" }
  | { type: "death" }
  | { type: "restart" }
  | { type: "fired" }
  | { type: "damage"; target: string; amount: number }
  | { type: "projectileEnd"; distance: number; reason: "range" | "collider" };
const listeners = new Set<(session: string, event: GameEvent) => void>();
export const run = (session: RunSession | null): Readonly<RunState> | undefined =>
  session?.entity.getComponent(RunStateComponent) ?? undefined;
export const playing = (session: RunSession | null): boolean => {
  const state = run(session);
  return !!state && !state.over && Time.tickNumber > state.readyAfter;
};
export function emit(session: RunSession | null, event: GameEvent): void {
  const key = session?.entity.id ?? "";
  session?.handle(event);
  if (key) for (const listener of [...listeners]) listener(key, event);
}
/** Resolve the reference at delivery time: freshly spawned prefab bindings can arrive after start. */
export function listen(
  session: () => RunSession | null,
  handler: (event: GameEvent) => void,
): () => void {
  const listener = (key: string, event: GameEvent): void => {
    if (key === session()?.entity.id) handler(event);
  };
  listeners.add(listener);
  return () => {
    listeners.delete(listener);
  };
}
