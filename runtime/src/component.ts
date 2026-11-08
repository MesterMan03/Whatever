import { committedComponents, canonical, type ComponentMembership } from "./scene-store.ts";
import { Entity, hydrateReferences } from "./components/ecs.ts";
import { DataType, toWire, fromWire, type Schema } from "./schema.ts";
import { _send } from "./shared.ts";
import {
  _native,
  _installRemovalObserver,
  _readComponentRow,
  _rememberComponentRow,
  type ComponentRow,
} from "./transport.ts";
import { liveValue } from "./live.ts";

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
  toJSON(): { $entity: string; $component: string } {
    return { $entity: this.entity.id, $component: this.id };
  }
  get id(): string {
    return definitionFor(this.constructor).id;
  }
  get entity(): Entity {
    const binding = bindings.get(this);
    if (!binding || binding.dead) throw new Error("Component is not attached to a live entity");
    return binding.entity;
  }
  start(): void | Promise<void> {}
  /**
   * Called with elapsed simulation time since this instance last ticked.
   * `Input` and `Time` are also available here and describe the latest engine tick.
   */
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
  disposing: boolean;
  faulted: boolean;
  delta: number;
  incarnation: string | null;
};
const definitions = new Map<Function, ComponentDefinition>();
const classes = new Map<string, ComponentClass>();
const bindings = new WeakMap<Component, Binding>();
const instances = new Map<string, Binding>();
const key = (entity: string, type: string) => JSON.stringify([entity, type]);
const equal = (a: unknown, b: unknown) => a === b || JSON.stringify(a) === JSON.stringify(b);

/** @internal Resolve lazily so cyclic and forward component links do not hydrate recursively. */
export function _componentReference(entityId: string, type: string): Component {
  const resolve = (): Component => {
    const id = canonical(entityId);
    const row = _readComponentRow(id, type);
    if (row === null) throw new Error(`Component reference '${type}' on '${id}' is unavailable`);
    const instance = _hydrateComponent(id, type, row);
    if (!instance) throw new Error(`Component reference '${type}' is not owned by this mod`);
    return instance;
  };
  return new Proxy({} as Component, {
    getPrototypeOf() {
      return Reflect.getPrototypeOf(resolve());
    },
    has(_target, property) {
      return property === "toJSON" || property in resolve();
    },
    get(_target, property) {
      if (property === "toJSON") return () => ({ $entity: canonical(entityId), $component: type });
      const instance = resolve();
      const value = Reflect.get(instance, property, instance);
      return typeof value === "function" && property !== "constructor"
        ? value.bind(instance)
        : value;
    },
    set(_target, property, value) {
      const instance = resolve();
      return Reflect.set(instance, property, value, instance);
    },
  });
}

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
export function _hydrateComponent(
  entityId: string,
  type: string,
  row?: ComponentRow,
): Component | null {
  const ctor = classes.get(type);
  if (!ctor) return null;
  entityId = canonical(entityId);
  const current = row ?? _readComponentRow(entityId, type);
  if (!current) return null;
  const id = key(entityId, type);
  let binding = instances.get(id);
  const incarnation = current.instance_id;
  if (binding && binding.incarnation !== incarnation) {
    dispose(binding);
    instances.delete(id);
    binding = undefined;
  }
  if (!binding) {
    const instance = new ctor();
    binding = {
      instance,
      entity: new Entity(entityId),
      definition: definitionFor(ctor),
      baseline: current.data,
      active: false,
      started: false,
      busy: false,
      dead: false,
      disposing: false,
      faulted: false,
      delta: 0,
      incarnation,
    };
    bindings.set(instance, binding);
    instances.set(id, binding);
    for (const field of binding.definition.fields) {
      const state = binding;
      const read = () => {
        if (state.dead) throw new Error("Component handle is stale");
        const row = _readComponentRow(state.entity.id, type);
        if (state.disposing) return (row?.data ?? state.baseline)[field.name];
        if (row === null || row.instance_id !== state.incarnation)
          throw new Error("Component handle is stale");
        state.baseline = row.data;
        return row.data[field.name];
      };
      const write = (value: any) => {
        const previous = read();
        const next = toWire(field.schema, value);
        if (equal(previous, next)) return;
        _native("component_patch", {
          id: state.entity.id,
          kind: type,
          data: { [field.name]: next },
        });
      };
      Object.defineProperty(instance, field.name, {
        enumerable: true,
        configurable: true,
        get: () => {
          if (field.schema.kind === "optional" && read() === null) return undefined;
          return liveValue(read, write);
        },
        set: write,
      });
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
  if (binding.dead || binding.disposing) return;
  binding.disposing = true;
  if (binding.active) invoke(binding, () => binding.instance.onDisable());
  invoke(binding, () => binding.instance.destroy());
  binding.dead = true;
  binding.disposing = false;
}

// Destruction is synchronous too: release listeners before the next engine call.
_installRemovalObserver(() => {
  for (const [id, binding] of [...instances]) {
    const row = _readComponentRow(binding.entity.id, binding.definition.id);
    if (!row || row.instance_id !== binding.incarnation) {
      instances.delete(id);
      dispose(binding);
    }
  }
});

/** @internal Authoritative membership and field updates arrive before each Tick. */
export function _componentSnapshot(rows: ComponentMembership[]): void {
  const present = new Map<string, ComponentMembership>();
  for (const row of rows) {
    present.set(key(row.entity_id, row.component_type), row);
    _rememberComponentRow(row.entity_id, row.component_type, row);
  }
  for (const [id, binding] of instances) {
    const row = present.get(id);
    if (!row || binding.incarnation !== row.instance_id) {
      dispose(binding);
      instances.delete(id);
    }
  }
  for (const row of rows) {
    const current = _readComponentRow(row.entity_id, row.component_type);
    if (!current || current.instance_id !== row.instance_id) continue;
    const instance = _hydrateComponent(row.entity_id, row.component_type, current);
    if (!instance) continue;
    const binding = bindings.get(instance)!;
    if (binding.active !== row.active) {
      binding.active = row.active;
      invoke(binding, () => (row.active ? instance.onEnable() : instance.onDisable()));
    }
  }
}

/** @internal Async hooks do not overlap; synchronous field edits are already committed. */
export function _tickComponents(delta: number): void {
  for (const binding of instances.values()) {
    if (!binding.active || binding.dead || binding.faulted) continue;
    binding.delta += delta;
    if (binding.busy) continue;
    if (!binding.started) {
      binding.started = true;
      invoke(binding, () => binding.instance.start());
    }
    if (binding.busy || binding.faulted || binding.dead || !binding.active) continue;
    const elapsed = binding.delta;
    binding.delta = 0;
    invoke(binding, () => binding.instance.tick(elapsed));
  }
}

/** @internal */
export function _shutdownComponents(): void {
  for (const binding of instances.values()) dispose(binding);
  instances.clear();
}

/** @internal Reconcile only this mod's class instances from native scene data. */
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
