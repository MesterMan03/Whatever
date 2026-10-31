import {
  assertNetworkSchema,
  DataType,
  StructDefinition,
  defineStruct,
  assertName,
  type Fields,
  type SchemaValue,
  type Field,
  type Schema,
} from "../schema.ts";
export {
  DataType,
  StructDefinition,
  defineStruct,
  Type,
  toWire,
  fromWire,
  MAX_BYTES,
  MAX_ITEMS,
} from "../schema.ts";
export type { Schema, Field, Fields, SchemaValue, TypeNamespace, ObjectData } from "../schema.ts";

function freeze<T>(value: T): T {
  if (value && typeof value === "object" && !Object.isFrozen(value)) {
    for (const child of Object.values(value)) freeze(child);
    Object.freeze(value);
  }
  return value;
}
export class PacketDefinition<F extends Fields> extends StructDefinition<F> {}
export function definePacket<const F extends Fields>(fields: F): PacketDefinition<F> {
  const packet = new PacketDefinition(fields);
  assertNetworkSchema(packet.schema);
  return Object.freeze(packet);
}
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
