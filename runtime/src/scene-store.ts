import type { Component } from "./component.ts";
/** Private revisioned scene store. Protocol data never contains script instances. */
export interface EntityMetadata {
  name: string;
  enabled: boolean;
  active: boolean;
  parent: string | null;
  children: string[];
  tags: string[];
}
export interface EntityRecord {
  id: string;
  metadata: EntityMetadata;
  components: Record<string, any>;
  instances: Record<string, string>;
}
export type DeepReadonly<T> = T extends Component
  ? T
  : T extends (...args: any[]) => any
    ? T
    : T extends object
      ? { readonly [K in keyof T]: DeepReadonly<T[K]> }
      : T;
/** @deprecated Ordinary component access now returns live, mutable handles. */
export type ComponentSnapshot<T> = T;

import { _native, _readComponentRow, type ComponentRow } from "./transport.ts";
const aliases = new Map<string, string>();
export function immutable<T>(value: T): DeepReadonly<T> {
  if (value && typeof value === "object" && !Object.isFrozen(value)) {
    for (const child of Object.values(value)) immutable(child);
    Object.freeze(value);
  }
  return value as DeepReadonly<T>;
}
export const canonical = (id: string): string => aliases.get(id) ?? id;
export const hasSceneRevision = (): boolean => true;
export const sceneRevision = (): number => _native("scene_info").revision;
export const sceneCamera = (): string | null => _native("scene_info").camera;
export const readEntity = (id: string): EntityRecord | null =>
  _native("entity_record", { id: canonical(id) });
export const readMetadata = (id: string): EntityMetadata | null =>
  _native("entity_metadata", { id: canonical(id) });
export function resolveEntityAlias(alias: string, id: string): void {
  aliases.set(alias, id);
}
export function rejectEntityAlias(alias: string): void {
  aliases.delete(alias);
}
export const listEntityIds = (): string[] => _native("entity_list");
export function queryEntityIds(kinds: readonly string[]): string[] {
  return _native("query_entities", { kinds });
}
export function queryTagIds(query: {
  all?: readonly string[];
  any?: readonly string[];
  none?: readonly string[];
  includeDisabled?: boolean;
}): string[] {
  return _native("query_tags", { query });
}
export const worldTransform = (id: string): any =>
  _readComponentRow(canonical(id), "core:world_transform")?.data ?? null;

export type ComponentMembership = ComponentRow & {
  entity_id: string;
  component_type: string;
  active: boolean;
};
export function committedComponents(types: Iterable<string>): ComponentMembership[] {
  return _native("component_rows", { kinds: [...types] });
}
/** An explicitly requested retained snapshot; ordinary entity access is live. */
export class SceneSnapshot {
  readonly revision = sceneRevision();
  readonly #records = new Map(listEntityIds().map((id) => [id, immutable(readEntity(id)!)]));
  has(id: string): boolean {
    return this.#records.has(canonical(id));
  }
  get(id: string): DeepReadonly<EntityRecord> | null {
    return this.#records.get(canonical(id)) ?? null;
  }
  entities(): readonly DeepReadonly<EntityRecord>[] {
    return Object.freeze([...this.#records.values()]);
  }
}
