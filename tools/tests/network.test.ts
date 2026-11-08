import {describe, expect, test} from "bun:test";
import {Type, defineStruct, definePacket, defineSpecification, toWire, fromWire} from "../../runtime/src/network/schema.ts";
import {dataProxy} from "../../runtime/src/network/data.ts";
describe("network specification SDK", () => {
  test("definitions snapshot mutable input and retain ordered fields", () => {
    const fields = {z: Type.VarInt, a: Type.Bool};
    const packet = definePacket(fields);
    fields.z = Type.I32;
    expect(packet.schema).toEqual({kind:"struct",fields:[{name:"z",schema:{kind:"varint"}},{name:"a",schema:{kind:"bool"}}]});
    const spec = defineSpecification("fixture:game", {login:{serverbound:{hello:packet}}});
    expect(Object.isFrozen(spec.states.login.serverbound)).toBe(true);
    expect(() => defineStruct({constructor:Type.String})).toThrow();
    expect(() => Type.I32.with({min: 0.5})).toThrow();
    expect(() => Type.F64.with({min: 2,max: 1})).toThrow();
    expect(() => Type.Optional(Type.Optional(Type.Bool))).toThrow();
  });
  test("optional IPC normalization is explicit and independent of public JS values", () => {
    const schema = defineStruct({a:Type.Optional(Type.Bool), values:Type.Array(Type.Optional(Type.VarInt))}).schema;
    const wire = toWire(schema,{values:[undefined,-1]});
    expect(wire).toEqual({a:null,values:[null,-1]});
    expect(fromWire(schema,wire)).toEqual({a:undefined,values:[undefined,-1]});
    expect(() => toWire(schema,{a:null,values:[]})).toThrow();
    expect(() => toWire(Type.String.schema,"\ud800")).toThrow();
    expect(() => toWire(Type.String.with({maxLength:1}).schema,"é")).toThrow();
    expect(() => toWire(Type.Array(Type.Bool).schema,new Array(1))).toThrow();
  });
  test("client-data proxies validate nested writes and atomic array mutations", () => {
    const schema = defineStruct({id:Type.Optional(Type.I32), nested:defineStruct({flag:Type.Bool}), tags:Type.Array(Type.String,{maxLength:3})}).schema;
    const writes: unknown[] = [];
    const data = dataProxy<{id?:number;nested:{flag:boolean};tags:string[]}>(schema,{nested:{flag:false},tags:[]}, value => writes.push(value));
    data.id=4;data.nested.flag=true;data.tags.push("b","a");data.tags.sort().push("c");
    expect(data.tags.map(x=>x)).toEqual(["a","b","c"]);
    expect(Object.keys(data)).toEqual(["id","nested","tags"]);
    expect(Array.isArray(data.tags)).toBe(true);
    expect(() => data.tags.push("overflow")).toThrow();
    expect(data.tags.length).toBe(3);
    expect(() => {data.nested.flag="wrong" as any;}).toThrow();
    expect(data.nested.flag).toBe(true);
    Object.getOwnPropertyDescriptor(data,"nested")!.value.flag = false;
    expect(data.nested.flag).toBe(false);
    delete data.id; expect(data.id).toBeUndefined();
    expect(() => {delete (data as any).nested;}).toThrow();
    expect(() => Object.defineProperty(data,"id",{value:9})).toThrow();
    expect(writes.length).toBeGreaterThan(4);
    const another=dataProxy<any>(schema,{nested:{flag:false},tags:[]},()=>{});
    expect(another.tags.length).toBe(0);
  });
});
