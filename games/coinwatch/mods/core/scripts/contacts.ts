import { Physics } from "@whatever-engine/api";
/** One native subscription, with disposable per-entity listeners. */
const listeners = new Set<(event: Physics.Event) => void>();
let subscribed = false;
export function onContact(handler: (event: Physics.Event) => void): () => void {
  if (!subscribed) {
    subscribed = true;
    Physics.onEvent((event) => {
      for (const listener of [...listeners]) listener(event);
    });
  }
  listeners.add(handler);
  return () => {
    listeners.delete(handler);
  };
}
