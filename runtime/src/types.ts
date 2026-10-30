/** The runtime side selected before mods are evaluated. */
export type RuntimeMode = "client" | "server";

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
  sides: RuntimeMode[];
  name: string;
  version: string;
  description: string;
  authors: string[];
  license: string;
  dependencies: Record<string, string>;
  load_order: { after: string[]; before: string[] };
  script?: { entry: string | null; client: string | null; server: string | null; runtime: string };
};

/** Payload types for each public event name. */
export type EventPayloads = {
  /** Fired once after the runtime is ready; client mode also waits for its window. */
  init: { mod_id: string; engine_version: string; mode: RuntimeMode };
  /** Fired when the game is shutting down. The process exits after all handlers return. */
  exit: { exit_code: number };
  /**
   * Fired on game ticks. A still-running async handler is not called again.
   * Commands flush once per tick, independently of async handler completion.
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
