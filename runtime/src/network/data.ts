import {fromWire, toWire, type Schema} from "./schema.ts";

/** Mutable SDK view of engine-owned data. Commit whole validated snapshots so array
 * mutators never expose half-applied edits to native validation. */
export function dataProxy<T>(schema: Schema, initial: unknown, changed: (wire: unknown) => void): T {
  let root = fromWire(schema, toWire(schema, initial));
  const cache = new Map<string, any>();
  const read = (path: PropertyKey[]) => path.reduce((value, key) => value[key], root);
  const clone = () => fromWire(schema, toWire(schema, root));
  const commit = (next: unknown) => {
    const wire = toWire(schema, next);
    changed(wire);
    root = fromWire(schema, wire);
  };
  const wrap = (path: PropertyKey[]): any => {
    const id = JSON.stringify(path);
    if (cache.has(id)) return cache.get(id);
    const array = Array.isArray(read(path));
    const proxy = new Proxy(array ? [] : Object.create(null), {
      get(_target, key, receiver) {
        const value = read(path);
        if (array && typeof key === "string" && ["push", "pop", "shift", "unshift", "splice", "sort", "reverse", "fill", "copyWithin"].includes(key)) {
          return (...args: unknown[]) => {
            const next = clone();
            const target = path.reduce((value, key) => value[key], next);
            const result = (Array.prototype as any)[key].apply(target, args);
            commit(next);
            return ["sort", "reverse", "fill", "copyWithin"].includes(key) ? receiver : result;
          };
        }
        const child = Reflect.get(value, key, receiver);
        return child && typeof child === "object" ? wrap([...path, key]) : child;
      },
      set(_target, key, value) {
        if (typeof key !== "string" || ["__proto__", "constructor", "prototype"].includes(key)) throw new Error("Invalid client-data property");
        const next = clone();
        const target = path.reduce((value, key) => value[key], next);
        Reflect.set(target, key, value);
        commit(next); return true;
      },
      deleteProperty(_target, key) {
        const next = clone();
        const target = path.reduce((value, key) => value[key], next);
        Reflect.deleteProperty(target, key);
        commit(next); return true;
      },
      ownKeys() { return Reflect.ownKeys(read(path)); },
      has(_target, key) { return Reflect.has(read(path), key); },
      getOwnPropertyDescriptor(_target, key) {
        const descriptor = Reflect.getOwnPropertyDescriptor(read(path), key);
        if (descriptor && "value" in descriptor && descriptor.value && typeof descriptor.value === "object") {
          return {...descriptor, value: wrap([...path, key])};
        }
        return descriptor;
      },
      defineProperty() { throw new Error("Client data supports assignments, not property definitions"); },
      setPrototypeOf() { throw new Error("Client-data prototypes cannot be changed"); },
      preventExtensions() { throw new Error("Client-data proxies cannot be frozen"); },
    });
    cache.set(id, proxy); return proxy;
  };
  return wrap([]);
}
