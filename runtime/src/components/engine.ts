import { _getMode, _requireClient } from "../environment.ts";
import type { EventName, EventPayloads } from "../types.ts";
import { _handlers, _send, _EVENT_SUBSCRIBE } from "../shared.ts";

/** Core engine events and logging. */
export const Engine = {
  /** Runtime side, available before top-level mod code executes. */
  get mode() { return _getMode(); },
  /**
   * Subscribe to an engine event. The handler is called each time the event fires.
   * Tick commands are submitted automatically before awaiting user promises.
   * A handler still awaiting a result is skipped on later ticks until it finishes.
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

  /** Override the game tick rate. Takes effect after this tick's command phase. */
  setTickRate(ticks_per_second: number): void {
    _send({ type: "SetTickRate", ticks_per_second });
  },

  /** Set the FPS cap. Pass `null` to remove the cap (uncapped). */
  setFpsCap(fps: number | null): void {
    _requireClient("Engine.setFpsCap");
    _send({ type: "SetFpsCap", fps });
  },

  /** Enable or disable vertical sync. Takes effect immediately. */
  setVsync(enabled: boolean): void {
    _requireClient("Engine.setVsync");
    _send({ type: "SetVsync", enabled });
  },

  /**
   * Set the main camera entity.  The entity must have both `core:camera` and
   * `core:transform` components attached; the engine reads them each frame to
   * compute the view-projection matrix.
   *
   * Pass an empty string (`""`) or call `Engine.clearCamera()` to remove the
   * active camera — the renderer will clear to black and display a warning.
   *
   * Multiple cameras can exist in the scene; only the one designated here is
   * used for rendering.
   */
  setMainCamera(entity_id: string): void {
    _requireClient("Engine.setMainCamera");
    _send({ type: "SetMainCamera", entity_id });
  },

  /**
   * Remove the active camera.  The screen clears to black and shows a "no
   * active camera" warning until `Engine.setMainCamera` is called again.
   */
  clearCamera(): void {
    _requireClient("Engine.clearCamera");
    _send({ type: "SetMainCamera", entity_id: "" });
  },
};
