/** Serializable schema grammar shared with the native registration/codec service. */
export type Schema =
  | { kind: "i32" | "f64" | "varint"; min?: number; max?: number }
  | { kind: "bool" }
  | { kind: "string"; max_length?: number }
  | { kind: "array"; element: Schema; max_length?: number }
  | { kind: "optional"; inner: Schema }
  | { kind: "struct"; fields: readonly Field[] };
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
    throw new Error(`Invalid network schema name '${name}'`);
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
        throw new Error("Network schema exceeds nesting/node limit");
      if (!item.schema || typeof item.schema !== "object")
        throw new Error("Invalid network schema descriptor");
      const child = (schema: Schema) => queue.push({ schema, depth: item.depth + 1 });
      if (item.schema.kind === "array") child(item.schema.element);
      else if (item.schema.kind === "optional") child(item.schema.inner);
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
      if (!(type instanceof DataType))
        throw new Error(`Field '${name}' needs a Network.Type descriptor`);
      copy[name] = type;
    }
    super({
      kind: "struct",
      fields: entries.map(([name, type]) => ({ name, schema: type.schema })),
    });
    this.fields = Object.freeze(copy) as F;
  }
}
export class PacketDefinition<F extends Fields> extends StructDefinition<F> {}
export function defineStruct<const F extends Fields>(fields: F): StructDefinition<F> {
  return Object.freeze(new StructDefinition(fields));
}
export function definePacket<const F extends Fields>(fields: F): PacketDefinition<F> {
  return Object.freeze(new PacketDefinition(fields));
}
export interface TypeNamespace {
  readonly I32: NumericType;
  readonly F64: NumericType;
  readonly VarInt: NumericType;
  readonly Bool: DataType<boolean>;
  readonly String: StringType;
  Array<T>(element: DataType<T>, options?: LengthLimit): DataType<T[]>;
  Optional<T>(inner: DataType<T>): DataType<T | undefined>;
}
export const Type: TypeNamespace = Object.freeze({
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
    if (!(inner instanceof DataType) || inner.schema.kind === "optional")
      throw new Error("Optional needs a non-optional descriptor");
    return Object.freeze(new DataType<T | undefined>({ kind: "optional", inner: inner.schema }));
  },
});
export type PacketMap = Record<string, PacketDefinition<Fields>>;
export type States = Record<string, { serverbound?: PacketMap; clientbound?: PacketMap }>;
export type Direction = "serverbound" | "clientbound";
type PacketsAt<S extends States, K extends keyof S, D extends Direction> = D extends keyof S[K]
  ? NonNullable<S[K][D]>
  : {};
export type PacketPath<S extends States, D extends Direction> = {
  [K in keyof S & string]: `${K}.${keyof PacketsAt<S, K, D> & string}`;
}[keyof S & string];
export type PacketData<
  S extends States,
  D extends Direction,
  P extends string,
> = P extends `${infer K}.${infer N}`
  ? K extends keyof S
    ? N extends keyof PacketsAt<S, K, D>
      ? SchemaValue<PacketsAt<S, K, D>[N]>
      : never
    : never
  : never;
export type DecodedPacket<S extends States> = {
  [D in Direction]: {
    [P in PacketPath<S, D>]: { packet: P; state: string; direction: D; data: PacketData<S, D, P> };
  }[PacketPath<S, D>];
}[Direction];
export class SpecificationDefinition<S extends States> {
  readonly states: S;
  readonly manifest: {
    id: string;
    states: readonly {
      name: string;
      serverbound: readonly { name: string; fields: readonly Field[] }[];
      clientbound: readonly { name: string; fields: readonly Field[] }[];
    }[];
  };
  constructor(
    readonly id: string,
    states: S,
  ) {
    if (!/^[a-z_][a-z0-9_]*:[a-zA-Z_][a-zA-Z0-9_]*$/.test(id) || id.length > 256)
      throw new Error("Specification ID must be mod_id:name");
    const copy: Record<string, { serverbound: PacketMap; clientbound: PacketMap }> =
      Object.create(null);
    let count = 0;
    for (const [name, state] of Object.entries(states)) {
      assertName(name);
      const groups = { serverbound: Object.create(null), clientbound: Object.create(null) };
      for (const direction of ["serverbound", "clientbound"] as const) {
        for (const [key, packet] of Object.entries(state[direction] ?? {})) {
          assertName(key);
          if (!(packet instanceof PacketDefinition))
            throw new Error(`'${name}.${key}' needs Network.Packet`);
          groups[direction][key] = packet;
          if (++count > 65536) throw new Error("Specification exceeds 65536 packets");
        }
      }
      copy[name] = groups;
    }
    if (!count || Object.keys(copy).length > 1024)
      throw new Error("Specification needs packets and at most 1024 states");
    this.states = freeze(copy) as S;
    this.manifest = freeze({
      id,
      states: Object.entries(copy).map(([name, state]) => ({
        name,
        serverbound: Object.entries(state.serverbound).map(([name, p]) => ({
          name,
          fields: (p.schema as Extract<Schema, { kind: "struct" }>).fields,
        })),
        clientbound: Object.entries(state.clientbound).map(([name, p]) => ({
          name,
          fields: (p.schema as Extract<Schema, { kind: "struct" }>).fields,
        })),
      })),
    });
    Object.freeze(this);
  }
}
export function defineSpecification<const S extends States>(
  id: string,
  states: S,
): SpecificationDefinition<S> {
  return new SpecificationDefinition(id, states);
}

/** Validate and snapshot values for the JSON bridge. Optional absence is represented by null only inside IPC. */
export function toWire(
  schema: Schema,
  value: unknown,
  budget = { nodes: 100_000 },
  depth = 0,
): any {
  if (--budget.nodes < 0 || depth > 32) throw new Error("Network value exceeds nesting/node limit");
  const fail = (message: string): never => {
    throw new Error(message);
  };
  switch (schema.kind) {
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
      if (
        (schema.min !== undefined && value < schema.min) ||
        (schema.max !== undefined && value > schema.max)
      )
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
  if (schema.kind === "array") return value.map((v: unknown) => fromWire(schema.element, v));
  if (schema.kind === "struct") {
    const output: Record<string, unknown> = Object.create(null);
    for (const field of schema.fields)
      output[field.name] = fromWire(field.schema, value[field.name]);
    return output;
  }
  return value;
}
export function packetSchema(
  spec: SpecificationDefinition<States>,
  direction: Direction,
  path: string,
): Schema {
  const [state, name, extra] = path.split(".");
  const packet = state && name && !extra ? spec.states[state]?.[direction]?.[name] : undefined;
  if (!packet) throw new Error(`Unknown ${direction} packet '${path}'`);
  return packet.schema;
}
