import { committedComponents } from "./scene-store.ts";
import { Entity, hydrateReferences } from "./components/ecs.ts";
import { DataType, toWire, fromWire, type Schema } from "./schema.ts";
import { _send } from "./shared.ts";

export interface FieldEditor {
  label?: string;
  tooltip?: string;
}

/** Optional schema override and inspector hints for a normal class field. */
export function field<T>(type: DataType<T>, initial: NoInfer<T>, editor: FieldEditor = {}): T {
  if (!(type instanceof DataType)) throw new Error("field needs a Type descriptor");
  return hydrateReferences(fromWire(type.schema, toWire(type.schema, initial)));
}

export interface ComponentDefinition {
  id: string;
  source: string;
  export: string;
  fields: { name: string; schema: Schema; default: unknown; editor?: FieldEditor }[];
}

export type ComponentClass<T extends Component = Component> = new () => T;
export type ComponentData<T> = {
  [K in keyof T as K extends keyof Component
    ? never
    : T[K] extends (...args: any[]) => any
      ? never
      : K]: T[K];
};

/** Extend this class to create a custom, persisted component with lifecycle hooks. */
export abstract class Component {
  get id(): string {
    return definitionFor(this.constructor).id;
  }
  get entity(): Entity {
    const binding = bindings.get(this);
    if (!binding || binding.dead) throw new Error("Component is not attached to a live entity");
    return binding.entity;
  }
  start(): void | Promise<void> {}
  tick(deltaSeconds: number): void | Promise<void> {}
  onEnable(): void {}
  onDisable(): void {}
  destroy(): void {}
}

type Binding = {
  instance: Component;
  entity: Entity;
  definition: ComponentDefinition;
  baseline: Record<string, unknown>;
  active: boolean;
  started: boolean;
  busy: boolean;
  dead: boolean;
  faulted: boolean;
  delta: number;
  incarnation?: string;
};
const definitions = new Map<Function, ComponentDefinition>();
const classes = new Map<string, ComponentClass>();
const bindings = new WeakMap<Component, Binding>();
const instances = new Map<string, Binding>();
const key = (entity: string, type: string) => JSON.stringify([entity, type]);
const equal = (a: unknown, b: unknown) => JSON.stringify(a) === JSON.stringify(b);

function definitionFor(ctor: Function): ComponentDefinition {
  const definition = definitions.get(ctor);
  if (!definition)
    throw new Error("Component class is not registered; export it from a mod source file");
  return definition;
}

/** @internal Called by the trusted host after source inspection. */
export function _registerComponent(ctor: ComponentClass, definition: ComponentDefinition): void {
  if (typeof ctor !== "function" || !(ctor.prototype instanceof Component))
    throw new Error(`Invalid component class '${definition.id}'`);
  if (classes.has(definition.id)) throw new Error(`Duplicate component '${definition.id}'`);
  definitions.set(ctor, definition);
  classes.set(definition.id, ctor);
}

/** @internal */
export function _componentId(ctor: ComponentClass): string {
  return definitionFor(ctor).id;
}

/** @internal Serialize only declared persistent fields, including nested values. */
export function _serializeComponent(instance: Component): Record<string, unknown> {
  const definition = definitionFor(instance.constructor);
  const data: Record<string, unknown> = Object.create(null);
  for (const field of definition.fields) {
    data[field.name] = toWire(field.schema, (instance as any)[field.name]);
  }
  return data;
}

/** @internal */
export function _hydrateComponent(entityId: string, type: string, data: any): Component | null {
  const ctor = classes.get(type);
  if (!ctor) return null;
  const id = key(entityId, type);
  let binding = instances.get(id);
  if (!binding) {
    const instance = new ctor();
    binding = {
      instance,
      entity: new Entity(entityId),
      definition: definitionFor(ctor),
      baseline: {},
      active: false,
      started: false,
      busy: false,
      dead: false,
      faulted: false,
      delta: 0,
    };
    bindings.set(instance, binding);
    instances.set(id, binding);
    for (const field of binding.definition.fields) {
      (instance as any)[field.name] = hydrateReferences(fromWire(field.schema, data[field.name]));
    }
    binding.baseline = JSON.parse(JSON.stringify(data));
  } else {
    try {
      const local = _serializeComponent(binding.instance);
      for (const field of binding.definition.fields) {
        if (equal(local[field.name], binding.baseline[field.name]))
          (binding.instance as any)[field.name] = hydrateReferences(
            fromWire(field.schema, data[field.name]),
          );
      }
      binding.baseline = JSON.parse(JSON.stringify(data));
    } catch (error) {
      fail(binding, error);
    }
  }
  return binding.instance;
}

function invoke(binding: Binding, callback: () => void | Promise<void>): void {
  try {
    const result = callback();
    if (result && typeof result.then === "function") {
      binding.busy = true;
      void Promise.resolve(result).then(
        () => {
          binding.busy = false;
        },
        (error) => fail(binding, error),
      );
    }
  } catch (error) {
    fail(binding, error);
  }
}
function fail(binding: Binding, error: unknown): void {
  binding.busy = false;
  binding.faulted = true;
  _send({
    type: "Log",
    level: "error",
    message: `Component ${binding.definition.id} on ${binding.entity.id} failed: ${String(error)}`,
  });
}
function dispose(binding: Binding): void {
  if (binding.dead) return;
  if (binding.active) invoke(binding, () => binding.instance.onDisable());
  invoke(binding, () => binding.instance.destroy());
  binding.dead = true;
}

/** @internal Authoritative membership and field updates arrive before each Tick. */
export function _componentSnapshot(
  rows: {
    entity_id: string;
    component_type: string;
    instance_id?: string;
    data: any;
    active: boolean;
  }[],
): void {
  const present = new Map(rows.map((row) => [key(row.entity_id, row.component_type), row]));
  for (const [id, binding] of instances) {
    const row = present.get(id);
    if (
      !row ||
      (binding.incarnation !== undefined &&
        row.instance_id !== undefined &&
        binding.incarnation !== row.instance_id)
    ) {
      dispose(binding);
      instances.delete(id);
    }
  }
  for (const row of rows) {
    const instance = _hydrateComponent(row.entity_id, row.component_type, row.data);
    if (!instance) continue;
    const binding = bindings.get(instance)!;
    binding.incarnation = row.instance_id;
    if (binding.active !== row.active) {
      binding.active = row.active;
      invoke(binding, () => (row.active ? instance.onEnable() : instance.onDisable()));
    }
  }
}

/** @internal Never wait on a promise before the current command batch is submitted. */
export function _tickComponents(delta: number): void {
  for (const binding of instances.values()) {
    if (!binding.active || binding.dead || binding.faulted) continue;
    binding.delta += delta;
    if (binding.busy) continue;
    if (!binding.started) {
      binding.started = true;
      invoke(binding, () => binding.instance.start());
    }
    if (binding.busy || binding.faulted) continue;
    const elapsed = binding.delta;
    binding.delta = 0;
    invoke(binding, () => binding.instance.tick(elapsed));
  }
}

/** @internal Batch changed fields once per tick, preserving unrelated fields. */
export function _flushComponents(): void {
  for (const binding of instances.values()) {
    if (
      binding.dead ||
      binding.faulted ||
      !binding.entity.hasComponent(binding.definition.id as any)
    )
      continue;
    try {
      const current = _serializeComponent(binding.instance);
      const patch = Object.fromEntries(
        Object.entries(current).filter(([name, value]) => !equal(value, binding.baseline[name])),
      );
      if (Object.keys(patch).length)
        _send({
          type: "ComponentPatch",
          entity_id: binding.entity.id,
          component_type: binding.definition.id,
          data: patch,
        });
      binding.baseline = current;
    } catch (error) {
      fail(binding, error);
    }
  }
}

/** @internal */
export function _shutdownComponents(): void {
  for (const binding of instances.values()) dispose(binding);
  instances.clear();
}

/** @internal Reconcile only this mod's class instances from committed scene data. */
export function _syncSceneComponents(): void {
  _componentSnapshot(committedComponents(classes.keys()));
}
/** @internal Create a detached editable copy, without attaching a lifecycle instance. */
export function _componentDraft<T extends Component>(ctor: ComponentClass<T>, data: any): T {
  const instance = new ctor();
  for (const field of definitionFor(ctor).fields)
    (instance as any)[field.name] = hydrateReferences(fromWire(field.schema, data[field.name]));
  return instance;
}
/** @internal */
export function _componentFields(ctor: ComponentClass, data: any): Record<string, unknown> {
  return Object.fromEntries(
    definitionFor(ctor).fields.map((field) => [
      field.name,
      hydrateReferences(fromWire(field.schema, data[field.name])),
    ]),
  );
}
