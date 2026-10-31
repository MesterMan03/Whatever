import type { Component, ComponentData } from "./component.ts";
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
export interface SceneDelta {
  revision: number;
  base_revision: number;
  tick_number: number;
  reset: boolean;
  camera: string | null;
  created: EntityRecord[];
  removed: string[];
  changed: {
    id: string;
    metadata?: EntityMetadata;
    components: Record<string, any | null>;
    instances: Record<string, string | null>;
  }[];
}
export type DeepReadonly<T> = T extends (...args: any[]) => any
  ? T
  : T extends object
    ? { readonly [K in keyof T]: DeepReadonly<T[K]> }
    : T;
export type ComponentSnapshot<T> = DeepReadonly<
  T extends Component
    ? ComponentData<T> & { readonly id: string }
    : { [K in keyof T as T[K] extends (...args: any[]) => any ? never : K]: T[K] }
>;

const parse = JSON.parse;
const stringify = JSON.stringify;
const base = new Map<string, EntityRecord>();
const overlay = new Map<string, EntityRecord | null>();
const aliases = new Map<string, string>();
const failed = new Set<string>();
const byType = new Map<string, Set<string>>();
const byTag = new Map<string, Set<string>>();
let operations: { command: any; tick: number | null }[] = [];
let revision = 0;
let camera: string | null = null;
let stagedCamera: string | null | undefined;

export function immutable<T>(value: T): DeepReadonly<T> {
  if (value && typeof value === "object" && !Object.isFrozen(value)) {
    for (const item of Object.values(value)) immutable(item);
    Object.freeze(value);
  }
  return value as DeepReadonly<T>;
}
export function canonical(id: string): string {
  return aliases.get(id) ?? id;
}
export function hasSceneRevision(): boolean {
  return revision > 0;
}
export function sceneRevision(): number {
  return revision;
}
export function sceneCamera(): string | null {
  return stagedCamera === undefined ? camera : stagedCamera;
}
export function readEntity(id: string): EntityRecord | null {
  id = canonical(id);
  return overlay.has(id) ? (overlay.get(id) ?? null) : (base.get(id) ?? null);
}
function index(map: Map<string, Set<string>>, key: string, id: string, add: boolean): void {
  if (add) {
    if (!map.has(key)) map.set(key, new Set());
    map.get(key)!.add(id);
  } else {
    const values = map.get(key);
    values?.delete(id);
    if (!values?.size) map.delete(key);
  }
}
function indexEntity(entity: EntityRecord, add: boolean): void {
  for (const kind of Object.keys(entity.components)) index(byType, kind, entity.id, add);
  for (const tag of entity.metadata.tags) index(byTag, tag, entity.id, add);
}
export function applySceneDelta(delta: SceneDelta): void {
  if (!delta.reset && delta.base_revision !== revision)
    throw new Error(`Scene revision gap: expected ${revision}, received ${delta.base_revision}`);
  if (delta.reset) {
    base.clear();
    byType.clear();
    byTag.clear();
  }
  for (const id of delta.removed) {
    const old = base.get(id);
    if (old) indexEntity(old, false);
    base.delete(id);
  }
  for (const entity of delta.created) {
    const old = base.get(entity.id);
    if (old) indexEntity(old, false);
    immutable(entity);
    base.set(entity.id, entity);
    indexEntity(entity, true);
  }
  for (const change of delta.changed) {
    const old = base.get(change.id);
    if (!old) throw new Error(`Scene delta references missing entity '${change.id}'`);
    const components = { ...old.components };
    const instances = { ...old.instances };
    for (const [kind, value] of Object.entries(change.components)) {
      const had = Object.hasOwn(components, kind);
      if (value === null) delete components[kind];
      else components[kind] = immutable(value);
      if (had !== (value !== null)) index(byType, kind, old.id, value !== null);
    }
    for (const [kind, value] of Object.entries(change.instances)) {
      if (value === null) delete instances[kind];
      else instances[kind] = value;
    }
    if (change.metadata) {
      for (const tag of old.metadata.tags) index(byTag, tag, old.id, false);
      for (const tag of change.metadata.tags) index(byTag, tag, old.id, true);
    }
    base.set(
      old.id,
      immutable({
        id: old.id,
        metadata: change.metadata ?? old.metadata,
        components,
        instances,
      }) as EntityRecord,
    );
  }
  revision = delta.revision;
  camera = delta.camera;
  operations = operations.filter((op) => op.tick === null || op.tick > delta.tick_number);
  rebuildOverlay();
}
function blank(id: string): EntityRecord {
  return {
    id,
    metadata: { name: "", enabled: true, active: true, parent: null, children: [], tags: [] },
    components: {},
    instances: {},
  };
}
function update(id: string, change: (entity: EntityRecord) => void): void {
  id = canonical(id);
  const old = readEntity(id);
  if (!old) return;
  const entity = {
    id,
    metadata: {
      ...old.metadata,
      tags: [...old.metadata.tags],
      children: [...old.metadata.children],
    },
    components: { ...old.components },
    instances: { ...old.instances },
  };
  change(entity);
  overlay.set(id, immutable(entity) as EntityRecord);
}
function applyLocal(command: any): boolean {
  const id = command.entity_id && canonical(command.entity_id);
  switch (command.type) {
    case "SetMainCamera":
      stagedCamera = command.entity_id ? canonical(command.entity_id) : null;
      return true;
    case "EntityCreate": {
      const id = canonical("pending:" + command.request_id);
      if (!failed.has(id) && !base.has(id)) overlay.set(id, immutable(blank(id)) as EntityRecord);
      return true;
    }
    case "EntityClone": {
      const source = readEntity(command.entity_id);
      if (!source) return true;
      const copy = parse(stringify(source));
      copy.id = canonical("pending:" + command.request_id);
      copy.metadata.name += " (Clone)";
      copy.metadata.enabled = command.enabled ?? source.metadata.enabled;
      copy.metadata.children = [];
      overlay.set(copy.id, immutable(copy) as EntityRecord);
      return true;
    }
    case "ComponentSet":
      update(id, (entity) => {
        entity.components[command.component_type] = command.data;
      });
      return true;
    case "ComponentPatch":
      update(id, (entity) => {
        const old = entity.components[command.component_type];
        if (old) entity.components[command.component_type] = { ...old, ...command.data };
      });
      return true;
    case "ComponentRemove":
      update(id, (entity) => {
        delete entity.components[command.component_type];
        delete entity.instances[command.component_type];
      });
      return true;
    case "EntityMove":
      update(id, (entity) => {
        entity.components["core:transform"] = {
          rotation: [0, 0, 0, 1],
          scale: [1, 1, 1],
          ...entity.components["core:transform"],
          position: command.position,
        };
      });
      return true;
    case "EntitySetEnabled":
      update(id, (entity) => {
        entity.metadata.enabled = command.enabled;
      });
      return true;
    case "EntityTagAdd":
      update(id, (entity) => {
        if (!entity.metadata.tags.includes(command.tag)) {
          if (entity.metadata.tags.length >= 32)
            throw new Error("An entity can have at most 32 tags");
          entity.metadata.tags.push(command.tag);
        }
      });
      return true;
    case "EntityTagRemove":
      update(id, (entity) => {
        entity.metadata.tags = entity.metadata.tags.filter((tag) => tag !== command.tag);
      });
      return true;
    case "EntitySetParent": {
      const parent = command.parent_id === null ? null : canonical(command.parent_id);
      if (parent === id) throw new Error("An entity cannot parent itself");
      let cursor = parent;
      const visited = new Set<string>();
      while (cursor) {
        if (cursor === id || visited.has(cursor)) throw new Error("Parenting would create a cycle");
        visited.add(cursor);
        cursor = readEntity(cursor)?.metadata.parent ?? null;
      }
      const old = readEntity(id)?.metadata.parent;
      if (old)
        update(old, (entity) => {
          entity.metadata.children = entity.metadata.children.filter((child) => child !== id);
        });
      if (parent)
        update(parent, (entity) => {
          if (!entity.metadata.children.includes(id)) entity.metadata.children.push(id);
        });
      update(id, (entity) => {
        entity.metadata.parent = parent;
      });
      return true;
    }
    case "EntityDestroy": {
      const parent = readEntity(id)?.metadata.parent;
      if (parent)
        update(parent, (entity) => {
          entity.metadata.children = entity.metadata.children.filter((child) => child !== id);
        });
      const pending = [id];
      const seen = new Set<string>();
      while (pending.length) {
        const next = pending.pop()!;
        if (seen.has(next)) continue;
        seen.add(next);
        pending.push(...(readEntity(next)?.metadata.children ?? []));
        overlay.set(next, null);
      }
      return true;
    }
    default:
      return false;
  }
}
export function stageCommand(encoded: string): void {
  const command = parse(encoded);
  if (applyLocal(command)) operations.push({ command, tick: null });
}
export function submitSceneCommands(tick: number): void {
  for (const operation of operations) if (operation.tick === null) operation.tick = tick;
}
function rebuildOverlay(): void {
  overlay.clear();
  stagedCamera = undefined;
  for (const operation of operations) {
    try {
      applyLocal(operation.command);
    } catch {
      /* Native commit reconciles rejected edits. */
    }
  }
}
export function resolveEntityAlias(alias: string, id: string): void {
  aliases.set(alias, id);
  rebuildOverlay();
}
export function rejectEntityAlias(alias: string): void {
  failed.add(alias);
  overlay.delete(alias);
}
export function resolveCommandReferences(encoded: string): string {
  if (!encoded.includes("pending:")) return encoded;
  const command = parse(encoded);
  for (const key of ["entity_id", "parent_id"])
    if (typeof command[key] === "string") command[key] = canonical(command[key]);
  function refs(value: any): void {
    if (!value || typeof value !== "object") return;
    if (Object.keys(value).length === 1 && typeof value.$entity === "string")
      value.$entity = canonical(value.$entity);
    else for (const item of Object.values(value)) refs(item);
  }
  if (["ComponentSet", "ComponentPatch"].includes(command.type)) refs(command.data);
  if (command.type === "PhysicsRequest") refs(command.request);
  return stringify(command);
}
export function activeEntity(id: string, read = readEntity): boolean {
  const seen = new Set<string>();
  for (
    let entity = read(id);
    entity;
    entity = entity.metadata.parent ? read(entity.metadata.parent) : null
  ) {
    if (seen.has(entity.id) || !entity.metadata.enabled) return false;
    seen.add(entity.id);
    if (!entity.metadata.parent) return true;
  }
  return false;
}
function ordered(ids: Iterable<string>): string[] {
  return [...new Set(ids)]
    .filter((id) => !!readEntity(id))
    .sort((a, b) => {
      const left = Number(a.split(":")[0]),
        right = Number(b.split(":")[0]);
      return Number.isFinite(left) && Number.isFinite(right) ? left - right : a.localeCompare(b);
    });
}
export function listEntityIds(): string[] {
  return ordered([...base.keys(), ...overlay.keys()]);
}
export function queryEntityIds(kinds: readonly string[]): string[] {
  if (!kinds.length) return [];
  if (kinds.every((kind) => kind === "core:entity" || kind === "core:world_transform"))
    return listEntityIds().filter((id) => activeEntity(id));
  const shortest = kinds
    .filter((kind) => kind !== "core:entity" && kind !== "core:world_transform")
    .map((kind) => byType.get(kind) ?? new Set<string>())
    .sort((a, b) => a.size - b.size)[0]!;
  return ordered([...shortest, ...overlay.keys()]).filter(
    (id) =>
      activeEntity(id) &&
      kinds.every(
        (kind) =>
          kind === "core:entity" ||
          kind === "core:world_transform" ||
          readEntity(id)?.components[kind] !== undefined,
      ),
  );
}
export function queryTagIds(query: {
  all?: readonly string[];
  any?: readonly string[];
  none?: readonly string[];
  includeDisabled?: boolean;
}): string[] {
  const all = query.all ?? [],
    any = query.any ?? [],
    none = query.none ?? [];
  const candidates = all.length
    ? (byTag.get(all[0]!) ?? [])
    : any.length
      ? any.flatMap((tag) => [...(byTag.get(tag) ?? [])])
      : base.keys();
  return ordered([...candidates, ...overlay.keys()]).filter((id) => {
    const tags = readEntity(id)!.metadata.tags;
    return (
      (query.includeDisabled || activeEntity(id)) &&
      all.every((tag) => tags.includes(tag)) &&
      (!any.length || any.some((tag) => tags.includes(tag))) &&
      none.every((tag) => !tags.includes(tag))
    );
  });
}
export function worldTransform(id: string): any {
  const entity = readEntity(id);
  if (!entity) return null;
  if (!overlay.size) return entity.components["core:world_transform"];
  const chain: EntityRecord[] = [];
  const seen = new Set<string>();
  for (
    let current: EntityRecord | null = entity;
    current;
    current = current.metadata.parent ? readEntity(current.metadata.parent) : null
  ) {
    if (seen.has(current.id)) return null;
    seen.add(current.id);
    chain.push(current);
  }
  let position = [0, 0, 0],
    rotation = [0, 0, 0, 1],
    scale = [1, 1, 1];
  for (const current of chain.reverse()) {
    const local = current.components["core:transform"];
    if (!local) continue;
    const v = local.position.map((n: number, i: number) => n * scale[i]!);
    const [x, y, z, w] = rotation as [number, number, number, number];
    const cross = [2 * (y * v[2] - z * v[1]), 2 * (z * v[0] - x * v[2]), 2 * (x * v[1] - y * v[0])];
    const rotated = [
      v[0] + w * cross[0]! + y * cross[2]! - z * cross[1]!,
      v[1] + w * cross[1]! + z * cross[0]! - x * cross[2]!,
      v[2] + w * cross[2]! + x * cross[1]! - y * cross[0]!,
    ];
    position = position.map((n, i) => n + rotated[i]!);
    const [a, b, c, d] = local.rotation;
    rotation = [
      w * a + x * d + y * c - z * b,
      w * b - x * c + y * d + z * a,
      w * c + x * b - y * a + z * d,
      w * d - x * a - y * b - z * c,
    ];
    const length = Math.hypot(...rotation);
    rotation = rotation.map((n) => n / length);
    scale = scale.map((n, i) => n * local.scale[i]);
  }
  return { position, rotation, scale };
}
export function committedComponents(types: Iterable<string>): {
  entity_id: string;
  component_type: string;
  instance_id?: string;
  data: any;
  active: boolean;
}[] {
  const rows = [];
  for (const kind of types)
    for (const id of byType.get(kind) ?? []) {
      const entity = base.get(id)!;
      rows.push({
        entity_id: id,
        component_type: kind,
        instance_id: entity.instances[kind],
        data: entity.components[kind],
        active: entity.metadata.active,
      });
    }
  return rows;
}

/** Explicit retained snapshot: records share immutable component values across revisions. */
export class SceneSnapshot {
  readonly revision: number;
  readonly #records: Map<string, EntityRecord>;
  constructor() {
    this.revision = revision;
    this.#records = new Map(base);
    Object.freeze(this);
  }
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
