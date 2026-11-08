import { hydrateReferences } from "./components/ecs.ts";
const copy = (value: any): any => JSON.parse(JSON.stringify(value));

/** A nested value whose edits are committed synchronously through its owning component. */
export function liveValue(read: () => any, write: (value: any) => void): any {
  const initial = read();
  if (initial === null || typeof initial !== "object") return initial;
  if (typeof initial.$entity === "string") return hydrateReferences(initial);
  const target = Array.isArray(initial) ? [] : {};
  return new Proxy(target, {
    get(_target, key, receiver) {
      if (key === "toJSON") return () => read();
      const current = read();
      if (key === Symbol.iterator && Array.isArray(current))
        return function* () {
          for (let i = 0; i < read().length; i++)
            yield liveValue(
              () => read()[i],
              (value) => {
                const data = copy(read());
                data[i] = value;
                write(data);
              },
            );
        };
      const value = current[key];
      if (typeof value === "function") return value.bind(receiver);
      return liveValue(
        () => read()[key],
        (value) => {
          const data = copy(read());
          data[key] = value;
          write(data);
        },
      );
    },
    set(_target, key, value) {
      const data = copy(read());
      data[key] = value;
      write(data);
      return true;
    },
    deleteProperty(_target, key) {
      const data = copy(read());
      delete data[key];
      write(data);
      return true;
    },
    ownKeys() {
      return Reflect.ownKeys(read());
    },
    getOwnPropertyDescriptor(_target, key) {
      if (key === "length" && Array.isArray(target))
        return { value: read().length, writable: true, enumerable: false, configurable: false };
      return { enumerable: true, configurable: true };
    },
  });
}
