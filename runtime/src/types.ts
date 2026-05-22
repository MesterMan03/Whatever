/** Base interface for all components. */
export interface Component {
  id: string;
}

/** Arbitrary JSON-serializable value used for inter-mod messages and components. */
export type JsonValue =
  | string
  | number
  | boolean
  | null
  | JsonValue[]
  | { [key: string]: JsonValue };

/** Metadata about a loaded mod, mirroring mod.toml. */
export type ModManifest = {
  id: string;
  name: string;
  version: string;
  description: string;
  authors: string[];
  license: string;
  dependencies: Record<string, string>;
  load_order: { after: string[]; before: string[] };
  script?: { entry: string; runtime: string };
};

/** Payload types for each public event name. */
export type EventPayloads = {
  /** Fired once after the engine has initialised and the window is ready. */
  init: { mod_id: string; engine_version: string };
  /** Fired when the game is shutting down. The process exits after all handlers return. */
  exit: { exit_code: number };
  /**
   * Fired every game tick. All async handlers are awaited before the engine advances.
   * Subscribe by calling `Engine.on("tick", handler)`.
   */
  tick: { tick_number: number; delta_seconds: number; keys_pressed: string[]; mouse_delta: [number, number] };
  /**
   * Fired when another mod sends this mod a message via `Message.send`.
   * If `request_id` is present the sender is awaiting a reply — call `Message.reply(request_id, data)`.
   * Not fired for replies that arrive via the `Message.send(id, msg, timeout)` overload.
   */
  mod_message: { source_mod_id: string; message: JsonValue; request_id?: string };
};

export type EventName = keyof EventPayloads;

/** One row returned by `Scene.query`. */
export type QueryResult<E> = { entity: E; components: Record<string, JsonValue> };
