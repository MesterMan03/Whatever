import type { EntityReference } from "./references.ts";
import { _componentId, type Component, type ComponentClass } from "./component.ts";
/** Serializable schema grammar shared with the native registration/codec service. */
export type Schema =
  | { kind: "i32" | "f64" | "varint"; min?: number; max?: number }
  | { kind: "bool" }
  | { kind: "string"; max_length?: number }
  | { kind: "array"; element: Schema; max_length?: number }
  | { kind: "optional"; inner: Schema }
  | { kind: "struct"; fields: readonly Field[] }
  | { kind: "nullable"; inner: Schema }
  | { kind: "tuple"; elements: readonly Schema[] }
  | { kind: "enum"; values: readonly string[] }
  | { kind: "entity" }
  | { kind: "component"; component_type: string }
  | { kind: "asset" };
export type Field = { readonly name: string; readonly schema: Schema };
export type Bounds = { min?: number; max?: number };
export type LengthLimit = { maxLength?: number };
export const MAX_BYTES = 1_048_576;
export const MAX_ITEMS = 65_536;
const encoder = new TextEncoder();

function freeze<T>(value: T): T {
  if (value && typeof value === "object" && !Object.isFrozen(value)) {
    for (const child of Object.values(value)) freeze(child);
    Object.freeze(value);
  }
  return value;
}
export function assertName(name: string): void {
  if (
    name.length > 128 ||
    !/^[a-zA-Z_][a-zA-Z0-9_]*$/.test(name) ||
    ["__proto__", "prototype", "constructor"].includes(name)
  ) {
    throw new Error(`Invalid schema name '${name}'`);
  }
}
export declare const valueType: unique symbol;
export class DataType<T> {
  declare readonly [valueType]: T;
  constructor(readonly schema: Schema) {
    const queue: { schema: Schema; depth: number }[] = [{ schema, depth: 0 }];
    let nodes = 0;
    while (queue.length) {
      const item = queue.pop()!;
      if (++nodes > 100_000 || item.depth > 32)
        throw new Error("Schema exceeds nesting/node limit");
      if (!item.schema || typeof item.schema !== "object")
        throw new Error("Invalid schema descriptor");
      const child = (schema: Schema) => queue.push({ schema, depth: item.depth + 1 });
      if (item.schema.kind === "array") child(item.schema.element);
      else if (item.schema.kind === "optional" || item.schema.kind === "nullable")
        child(item.schema.inner);
      else if (item.schema.kind === "tuple")
        for (const element of item.schema.elements) child(element);
      else if (item.schema.kind === "struct")
        for (const field of item.schema.fields) child(field.schema);
    }
    freeze(schema);
  }
}
export type SchemaValue<T> = T extends DataType<infer V> ? V : never;
export type Fields = Record<string, DataType<unknown>>;
type OptionalKeys<F extends Fields> = {
  [K in keyof F]: undefined extends SchemaValue<F[K]> ? K : never;
}[keyof F];
export type ObjectData<F extends Fields> = {
  -readonly [K in Exclude<keyof F, OptionalKeys<F>>]: SchemaValue<F[K]>;
} & { -readonly [K in OptionalKeys<F>]?: SchemaValue<F[K]> };

class NumericType extends DataType<number> {
  with(bounds: Bounds): NumericType {
    if (!["i32", "f64", "varint"].includes(this.schema.kind))
      throw new Error("Expected numeric schema");
    const schema = { ...this.schema, ...bounds } as Extract<
      Schema,
      { kind: "i32" | "f64" | "varint" }
    >;
    for (const value of [schema.min, schema.max]) {
      if (
        value !== undefined &&
        (!Number.isFinite(value) ||
          (schema.kind !== "f64" &&
            (!Number.isInteger(value) || value < -2147483648 || value > 2147483647)))
      ) {
        throw new Error("Invalid numeric schema bounds");
      }
    }
    if (schema.min !== undefined && schema.max !== undefined && schema.min > schema.max)
      throw new Error("min exceeds max");
    return Object.freeze(new NumericType(schema));
  }
}
function length(value: number | undefined, max: number): void {
  if (value !== undefined && (!Number.isInteger(value) || value < 0 || value > max))
    throw new Error(`Length limit must be between 0 and ${max}`);
}
class StringType extends DataType<string> {
  with({ maxLength }: LengthLimit): StringType {
    length(maxLength, MAX_BYTES);
    return Object.freeze(new StringType({ kind: "string", max_length: maxLength }));
  }
}
export class StructDefinition<F extends Fields> extends DataType<ObjectData<F>> {
  readonly fields: F;
  constructor(fields: F) {
    const entries = Object.entries(fields);
    if (Array.isArray(fields) || entries.length > 100_000) throw new Error("Invalid struct fields");
    const copy: Record<string, DataType<unknown>> = Object.create(null);
    for (const [name, type] of entries) {
      assertName(name);
      if (!(type instanceof DataType)) throw new Error(`Field '${name}' needs a Type descriptor`);
      copy[name] = type;
    }
    super({
      kind: "struct",
      fields: entries.map(([name, type]) => ({ name, schema: type.schema })),
    });
    this.fields = Object.freeze(copy) as F;
  }
}
export interface TypeNamespace {
  readonly I32: NumericType;
  readonly F64: NumericType;
  readonly VarInt: NumericType;
  readonly Bool: DataType<boolean>;
  readonly String: StringType;
  readonly Entity: DataType<EntityReference>;
  readonly Asset: DataType<string>;
  Component<T extends Component>(kind: ComponentClass<T>): DataType<T>;
  Nullable<T>(inner: DataType<T>): DataType<T | null>;
  Enum<const V extends readonly [string, ...string[]]>(values: V): DataType<V[number]>;
  Tuple<const T extends readonly DataType<unknown>[]>(
    ...elements: T
  ): DataType<{ -readonly [K in keyof T]: SchemaValue<T[K]> }>;
  Array<T>(element: DataType<T>, options?: LengthLimit): DataType<T[]>;
  Optional<T>(inner: DataType<T>): DataType<T | undefined>;
}
export const Type: TypeNamespace = Object.freeze({
  Entity: Object.freeze(new DataType<EntityReference>({ kind: "entity" })),
  Asset: Object.freeze(new DataType<string>({ kind: "asset" })),
  Component<T extends Component>(kind: ComponentClass<T>): DataType<T> {
    return Object.freeze(
      new DataType<T>({ kind: "component", component_type: _componentId(kind) }),
    );
  },
  Nullable<T>(inner: DataType<T>): DataType<T | null> {
    if (!(inner instanceof DataType) || ["nullable", "optional"].includes(inner.schema.kind))
      throw new Error("Nullable needs a required non-nullable descriptor");
    return Object.freeze(new DataType<T | null>({ kind: "nullable", inner: inner.schema }));
  },
  Enum<const V extends readonly [string, ...string[]]>(values: V): DataType<V[number]> {
    if (
      !values.length ||
      values.length > 1024 ||
      values.some((v) => typeof v !== "string") ||
      new Set(values).size !== values.length
    )
      throw new Error("Enum needs 1–1024 unique strings");
    return Object.freeze(new DataType<V[number]>({ kind: "enum", values: [...values] }));
  },
  Tuple<const T extends readonly DataType<unknown>[]>(
    ...elements: T
  ): DataType<{ -readonly [K in keyof T]: SchemaValue<T[K]> }> {
    if (!elements.length || elements.length > 32 || elements.some((e) => !(e instanceof DataType)))
      throw new Error("Tuple needs 1–32 descriptors");
    return Object.freeze(
      new DataType<{ -readonly [K in keyof T]: SchemaValue<T[K]> }>({
        kind: "tuple",
        elements: elements.map((e) => e.schema),
      }),
    );
  },
  I32: Object.freeze(new NumericType({ kind: "i32" })),
  F64: Object.freeze(new NumericType({ kind: "f64" })),
  VarInt: Object.freeze(new NumericType({ kind: "varint" })),
  Bool: Object.freeze(new DataType<boolean>({ kind: "bool" })),
  String: Object.freeze(new StringType({ kind: "string" })),
  Array<T>(element: DataType<T>, options: LengthLimit = {}): DataType<T[]> {
    if (!(element instanceof DataType)) throw new Error("Array needs an element descriptor");
    length(options.maxLength, MAX_ITEMS);
    return Object.freeze(
      new DataType<T[]>({ kind: "array", element: element.schema, max_length: options.maxLength }),
    );
  },
  Optional<T>(inner: DataType<T>): DataType<T | undefined> {
    if (!(inner instanceof DataType) || ["optional", "nullable"].includes(inner.schema.kind))
      throw new Error("Optional needs a non-optional descriptor");
    return Object.freeze(new DataType<T | undefined>({ kind: "optional", inner: inner.schema }));
  },
});
/** Validate and snapshot values for the JSON bridge. Optional absence is represented by null only inside IPC. */
export function toWire(
  schema: Schema,
  value: unknown,
  budget = { nodes: 100_000 },
  depth = 0,
): any {
  if (--budget.nodes < 0 || depth > 32) throw new Error("Value exceeds nesting/node limit");
  const fail = (message: string): never => {
    throw new Error(message);
  };
  switch (schema.kind) {
    case "nullable":
      return value === null ? null : toWire(schema.inner, value, budget, depth + 1);
    case "tuple":
      if (!Array.isArray(value) || value.length !== schema.elements.length)
        return fail("Expected fixed-size tuple");
      return schema.elements.map((element, index) =>
        toWire(element, value[index], budget, depth + 1),
      );
    case "enum":
      return typeof value === "string" && schema.values.includes(value)
        ? value
        : fail("Expected an enum choice");
    case "entity": {
      const reference =
        value &&
        typeof value === "object" &&
        "toJSON" in value &&
        typeof value.toJSON === "function"
          ? value.toJSON()
          : value;
      if (
        !reference ||
        typeof reference !== "object" ||
        Object.keys(reference).length !== 1 ||
        typeof (reference as any).$entity !== "string" ||
        !(reference as any).$entity ||
        (reference as any).$entity.length > 256
      )
        return fail("Expected entity reference");
      return { $entity: (reference as any).$entity };
    }
    case "component": {
      const reference =
        value &&
        typeof value === "object" &&
        "toJSON" in value &&
        typeof value.toJSON === "function"
          ? value.toJSON()
          : value;
      if (
        reference &&
        typeof reference === "object" &&
        Object.keys(reference).length === 2 &&
        (reference as any).$component === schema.component_type &&
        typeof (reference as any).$entity === "string" &&
        (reference as any).$entity.length > 0 &&
        (reference as any).$entity.length <= 256
      )
        return { $entity: (reference as any).$entity, $component: schema.component_type };
      if (
        value &&
        typeof value === "object" &&
        "entity" in value &&
        "id" in value &&
        value.id === schema.component_type
      ) {
        const entity = (value as Component).entity;
        return { $entity: entity.id, $component: schema.component_type };
      }
      return fail(`Expected reference to component '${schema.component_type}'`);
    }
    case "asset":
      return typeof value === "string" && (value === "" || /^[a-z0-9_-]+:\/\/[^\\]+$/.test(value))
        ? value
        : fail("Expected VFS asset path");
    case "optional":
      return value === undefined ? null : toWire(schema.inner, value, budget, depth + 1);
    case "i32":
    case "varint":
    case "f64": {
      if (typeof value !== "number" || !Number.isFinite(value))
        return fail("Expected finite number");
      if (
        schema.kind !== "f64" &&
        (!Number.isInteger(value) || value < -2147483648 || value > 2147483647)
      )
        return fail("Expected i32 integer");
      if ((schema.min != null && value < schema.min) || (schema.max != null && value > schema.max))
        return fail("Number outside declared bounds");
      return value;
    }
    case "bool":
      if (typeof value !== "boolean") return fail("Expected bool");
      return value;
    case "string":
      if (typeof value !== "string") return fail("Expected string");
      if (!value.isWellFormed()) return fail("String contains an unpaired UTF-16 surrogate");
      if (encoder.encode(value).length > (schema.max_length ?? MAX_BYTES))
        return fail("UTF-8 string exceeds length limit");
      return value;
    case "array": {
      if (!Array.isArray(value) || value.length > (schema.max_length ?? MAX_ITEMS))
        return fail("Expected array within length limit");
      return Array.from({ length: value.length }, (_, i) => {
        if (!Object.hasOwn(value, i)) return fail("Sparse arrays are not supported");
        return toWire(schema.element, value[i], budget, depth + 1);
      });
    }
    case "struct": {
      if (!value || typeof value !== "object" || Array.isArray(value))
        return fail("Expected object");
      const input = value as Record<string, unknown>;
      const names = new Set(schema.fields.map((f) => f.name));
      if (Object.keys(input).some((k) => !names.has(k)))
        return fail("Object contains undeclared fields");
      const output: Record<string, unknown> = Object.create(null);
      for (const field of schema.fields)
        output[field.name] = toWire(field.schema, input[field.name], budget, depth + 1);
      return output;
    }
  }
}
export function fromWire(schema: Schema, value: any): any {
  if (schema.kind === "optional") return value === null ? undefined : fromWire(schema.inner, value);
  if (schema.kind === "nullable") return value === null ? null : fromWire(schema.inner, value);
  if (schema.kind === "tuple")
    return schema.elements.map((element, index) => fromWire(element, value[index]));
  if (schema.kind === "array") return value.map((v: unknown) => fromWire(schema.element, v));
  if (schema.kind === "struct") {
    const output: Record<string, unknown> = Object.create(null);
    for (const field of schema.fields)
      output[field.name] = fromWire(field.schema, value[field.name]);
    return output;
  }
  return value;
}

/** Network codecs support an explicit subset of the common value grammar. */
export function assertNetworkSchema(schema: Schema): void {
  if (schema.kind === "array") assertNetworkSchema(schema.element);
  else if (schema.kind === "optional") assertNetworkSchema(schema.inner);
  else if (schema.kind === "struct")
    for (const field of schema.fields) assertNetworkSchema(field.schema);
  else if (!["i32", "f64", "varint", "bool", "string"].includes(schema.kind))
    throw new Error(`Type '${schema.kind}' is not supported in network schemas`);
}

export function defineStruct<const F extends Fields>(fields: F): StructDefinition<F> {
  return Object.freeze(new StructDefinition(fields));
}
