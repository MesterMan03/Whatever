import { describe, expect, test } from "bun:test";
import { mkdtempSync, mkdirSync, writeFileSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { inspectComponents } from "../../runtime/compiler/components.ts";
import { Type, toWire, fromWire, assertNetworkSchema } from "../../runtime/src/schema.ts";

function inspect(source: string, owner = "fixture") {
  const root = mkdtempSync(join(tmpdir(), "whatever-components-"));
  try {
    mkdirSync(join(root, "scripts"));
    writeFileSync(join(root, "scripts/components.ts"), source);
    return inspectComponents(root, owner);
  } finally { rmSync(root, { recursive: true, force: true }); }
}

describe("component source metadata", () => {
  test("component fields preserve concrete types, forward links and explicit descriptors", () => {
    const definitions = inspect(`
      import {Component, Type, field} from "@whatever-engine/api";
      export class Pickup extends Component {
        session: Session | null = null;
        explicit = field(Type.Nullable(Type.Component(Session)), null);
      }
      export class Session extends Component {
        static readonly type = "fixture:game_session";
        score = 0;
        award(amount: number) { this.score += amount; }
        peer: Pickup | null = null;
      }
    `);
    const pickup = definitions.find((definition) => definition.id === "fixture:Pickup")!;
    for (const field of pickup.fields) expect(field.schema).toEqual({kind:"nullable",inner:{kind:"component",component_type:"fixture:game_session"}});
    expect(definitions.find((definition) => definition.id === "fixture:game_session")?.fields.find(field => field.name === "peer")?.schema)
      .toEqual({kind:"nullable",inner:{kind:"component",component_type:"fixture:Pickup"}});
    const schema = {kind:"component",component_type:"fixture:game_session"} as const;
    expect(toWire(schema, {$entity:"2:1",$component:"fixture:game_session"})).toEqual({$entity:"2:1",$component:"fixture:game_session"});
    expect(() => toWire(schema, {$entity:"2:1",$component:"fixture:Pickup"})).toThrow("component");
    expect(() => toWire(schema, {$entity:"2:1"})).toThrow("component");
    expect(() => assertNetworkSchema(schema)).toThrow("network");
  });
  test("ignores local trash and staging scripts when discovering live components", () => {
    const root = mkdtempSync(join(tmpdir(), "whatever-component-trash-"));
    try {
      mkdirSync(join(root, ".whatever", "trash"), { recursive: true });
      const source = 'import { Component } from "@whatever-engine/api"; export class Live extends Component {}';
      writeFileSync(join(root, "live.ts"), source);
      writeFileSync(join(root, ".whatever", "trash", "old.ts"), source);
      expect(inspectComponents(root, "fixture").map((definition) => definition.id)).toEqual(["fixture:Live"]);
    } finally { rmSync(root, { recursive: true, force: true }); }
  });
  test("the game core can export custom component classes", () => {
    const [definition] = inspect('import { Component } from "@whatever-engine/api"; export class Player extends Component { speed = 3; }', "core");
    expect(definition?.id).toBe("core:Player");
    expect(definition?.fields[0]?.name).toBe("speed");
  });
  test("extracts inherited fields and shared types without evaluating mod code", () => {
    const [definition] = inspect(`
      import { Component as C, Entity, Struct, Type as T, field as f } from "@whatever-engine/api";
      throw new Error("THIS MODULE MUST NEVER BE EVALUATED");
      const rank = T.I32.with({min:0});
      abstract class Base extends C { inherited = 4; }
      export class Spinner extends Base {
        static readonly type = "fixture:spinner";
        speed = f(T.F64.with({ min: 0, max: 360 }), 90, { label: "Speed" });
        /** Object to follow. */
        target: Entity | null = null;
        mode: "slow" | "fast" = "slow";
        axis: [number, number, number] = [0, 1, 0];
        points: number[] = [];
        nested = { label: "hello", active: true };
        ranked = f(Struct({rank:rank}), {rank:1});
        texture = f(T.Asset, "fixture://textures/test.png");
        private elapsed = 0;
        #scratch = 0;
        readonly cache = new Map();
        get computed() { return 3; }
      }`);
    expect(definition?.id).toBe("fixture:spinner");
    expect(definition?.source).toBe("scripts/components.ts");
    const fields = Object.fromEntries(definition!.fields.map(field => [field.name, field]));
    expect(fields.speed).toMatchObject({ schema: { kind: "f64", min: 0, max: 360 }, default: 90, editor: { label: "Speed" } });
    expect(fields.inherited?.default).toBe(4);
    expect(fields.target).toMatchObject({ schema: { kind: "nullable", inner: { kind: "entity" } }, default: null, editor: { tooltip: "Object to follow." } });
    expect(fields.mode?.schema.kind).toBe("enum");
    expect(fields.axis?.schema.kind).toBe("tuple");
    expect(fields.points?.schema).toEqual({ kind: "array", element: { kind: "f64" } });
    expect(fields.nested?.schema.kind).toBe("struct");
    expect(fields.ranked?.schema).toEqual({kind:"struct",fields:[{name:"rank",schema:{kind:"i32",min:0}}]});
    expect(fields.texture?.schema.kind).toBe("asset");
    for (const excluded of ["elapsed", "#scratch", "cache", "computed", "type"]) expect(fields[excluded]).toBeUndefined();
  });

  test("diagnoses dynamic defaults, incompatible defaults and recursive data", () => {
    expect(() => inspect('import {Component} from "@whatever-engine/api"; export class Bad extends Component { value = Date.now(); }')).toThrow("dynamic initialization in start()");
    expect(() => inspect('import {Component,Type,field} from "@whatever-engine/api"; export class Bad extends Component { value = field(Type.I32, "wrong"); }')).toThrow("finite number");
    expect(() => inspect('import {Component} from "@whatever-engine/api"; type Loop={next:Loop}; export class Bad extends Component { value: Loop = {} as Loop; }')).toThrow("recursive");
  });

  test("diagnoses required constructors and conflicting annotations", () => {
    expect(() => inspect('import {Component} from "@whatever-engine/api"; export class Bad extends Component {constructor(required:number){super();}}')).toThrow("without arguments");
    expect(() => inspect('import {Component,Type,field} from "@whatever-engine/api"; export class Bad extends Component {value:string=field(Type.I32,1);}')).toThrow("conflicts");
  });

  test("uses API symbol identity and rejects duplicate component IDs", () => {
    expect(inspect('class Component {} export class Ordinary extends Component { value = Date.now(); }')).toEqual([]);
    expect(() => inspect('import {Component} from "@whatever-engine/api"; export class A extends Component {static readonly type="fixture:same";} export class B extends Component {static readonly type="fixture:same";}')).toThrow("duplicate component");
  });
});

test("shared component types validate data and keep network codecs explicit", () => {
  const tuple = Type.Tuple(Type.F64, Type.Enum(["slow", "fast"]));
  expect(toWire(tuple.schema, [3, "slow"])).toEqual([3, "slow"]);
  expect(() => toWire(tuple.schema, [3, "wrong"])).toThrow("enum");
  expect(() => toWire(tuple.schema, [3])).toThrow("tuple");
  const nullable = Type.Nullable(Type.Entity);
  expect(fromWire(nullable.schema, toWire(nullable.schema, null))).toBeNull();
  expect(toWire(nullable.schema, { $entity: "1:0" })).toEqual({ $entity: "1:0" });
  expect(() => Type.Optional(nullable)).toThrow();
  expect(() => assertNetworkSchema(nullable.schema)).toThrow("network");
  expect(() => toWire(Type.Asset.schema, "outside.png")).toThrow("VFS");
});

test("shipped Markbench component defaults are statically extractable", () => {
  const definitions = inspectComponents(join(import.meta.dirname, "../../games/test/mods/markbench"), "markbench");
  expect(definitions.map(definition => definition.id).sort()).toEqual(["markbench:counter", "markbench:payload"]);
});
