import type { JsonValue, ModManifest, EventPayloads } from "../types.ts";
import { _modListCallbacks, _modGetCallbacks, _msgCallbacks, _send, nextReqId } from "../shared.ts";
import { Engine } from "./engine.ts";

/** Query information about loaded mods. */
export const Mods = {
  /** Returns the manifests of all currently loaded mods in load order. */
  list(): Promise<ModManifest[]> {
    return new Promise((resolve, reject) => {
      const request_id = nextReqId();
      _modListCallbacks.set(request_id, { resolve, reject });
      _send({ type: "ModListRequest", request_id });
    });
  },

  /** Returns the manifest for a specific mod by ID. Rejects if the mod is not loaded. */
  get(id: string): Promise<ModManifest> {
    return new Promise((resolve, reject) => {
      const request_id = nextReqId();
      _modGetCallbacks.set(request_id, { resolve, reject });
      _send({ type: "ModGetRequest", request_id, mod_id: id });
    });
  },
};

interface _MessageNamespace {
  /** Send a fire-and-forget message to another mod. */
  sendAndForget<T extends JsonValue>(id: string, message: T): void;
  /**
   * Send a message to another mod and wait for a reply.
   * The receiving mod's handler must return a non-null value within `timeout` ms.
   * Rejects with a timeout error if no reply arrives in time.
   */
  send<T extends JsonValue, U extends JsonValue>(id: string, message: T, timeout: number): Promise<U>;
  /**
   * Register a handler for incoming mod messages.
   * Return a `JsonValue` to reply (only meaningful when the sender used `send`); return `null` to ignore.
   * The `request_id` in the payload is an opaque engine token — do not inspect or store it.
   */
  registerMessageHandler(handler: (payload: EventPayloads["mod_message"]) => JsonValue | null): void;
}

/** Inter-mod communication. */
export const Message: _MessageNamespace = {
  sendAndForget<T extends JsonValue>(id: string, message: T): void {
    _send({ type: "ModMessageSend", target_mod_id: id, request_id: null, payload: message });
  },
  send<T extends JsonValue, U extends JsonValue>(id: string, message: T, timeout: number): Promise<U> {
    return new Promise<U>((resolve, reject) => {
      const request_id = nextReqId();
      const timer = setTimeout(() => {
        _msgCallbacks.delete(request_id);
        reject(new Error(`Message to '${id}' timed out after ${timeout}ms`));
      }, timeout);
      _msgCallbacks.set(request_id, {
        resolve: (v) => { clearTimeout(timer); resolve(v as U); },
        reject,
      });
      _send({ type: "ModMessageSend", target_mod_id: id, request_id, payload: message });
    });
  },
  registerMessageHandler(handler: (payload: EventPayloads["mod_message"]) => JsonValue | null): void {
    Engine.on("mod_message", (payload) => {
      const result = handler(payload);
      if (payload.request_id !== undefined && result !== null) {
        _send({ type: "ModMessageReply", request_id: payload.request_id, payload: result });
      }
    });
  },
};
