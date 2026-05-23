import type { EventName, EventPayloads } from "../types.ts";
import { _handlers, _send, _EVENT_SUBSCRIBE } from "../shared.ts";

/** Core engine events and logging. */
export const Engine = {
  /**
   * Subscribe to an engine event. The handler is called each time the event fires.
   * For `tick`, async handlers are awaited before the engine advances the simulation.
   * Registering a handler also sends a `Subscribe` message to the engine automatically
   * (except `mod_message`, which the engine routes unconditionally).
   */
  on<E extends EventName>(event: E, handler: (payload: EventPayloads[E]) => void | Promise<void>): void {
    if (!_handlers.has(event)) _handlers.set(event, new Set());
    _handlers.get(event)!.add(handler as (payload: any) => void | Promise<void>);
    const msgType = _EVENT_SUBSCRIBE[event];
    if (msgType) _send({ type: "Subscribe", events: [msgType] });
  },

  /** Log a message via the engine logger. Output includes timestamp and mod ID. */
  log(level: "info" | "warn" | "error", message: string): void {
    _send({ type: "Log", level, message });
  },

  /** Override the game tick rate. Takes effect immediately. */
  setTickRate(ticks_per_second: number): void {
    _send({ type: "SetTickRate", ticks_per_second });
  },

  /** Set the FPS cap. Pass `null` to remove the cap (uncapped). */
  setFpsCap(fps: number | null): void {
    _send({ type: "SetFpsCap", fps });
  },

  /** Enable or disable vertical sync. Takes effect immediately. */
  setVsync(enabled: boolean): void {
    _send({ type: "SetVsync", enabled });
  },
};
