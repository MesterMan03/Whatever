import { createInterface } from "node:readline";

// Internal IPC types — match Rust serde tags exactly, not part of the public API.
type _EngineMsg =
  | { type: "Init"; mod_id: string; engine_version: string }
  | { type: "Frame"; delta_seconds: number; frame_number: number }
  | { type: "Input"; keys_pressed: string[]; mouse_delta: [number, number] }
  | { type: "AssetResponse"; request_id: string; path: string; data_base64: string | null; error: string | null }
  | { type: "FileResponse"; request_id: string; data_base64: string | null; error: string | null }
  | { type: "Shutdown"; exit_code: number };

type _ScriptMsg =
  | { type: "Subscribe"; events: string[] }
  | { type: "AssetRequest"; request_id: string; path: string }
  | { type: "SpawnSprite"; entity_id: string; texture: string; position: [number, number, number]; scale: [number, number, number] }
  | { type: "MoveEntity"; entity_id: string; position: [number, number, number] }
  | { type: "DestroyEntity"; entity_id: string }
  | { type: "Log"; level: "info" | "warn" | "error"; message: string }
  | { type: "SetWindowTitle"; title: string }
  | { type: "FileWrite"; request_id: string; path: string; data_base64: string }
  | { type: "FileRead"; request_id: string; path: string }
  | { type: "FileDelete"; request_id: string; path: string };

/** Payload types for each public event name. */
export type EventPayloads = {
  /** Fired once after the engine has initialised and the window is ready. */
  init: { mod_id: string; engine_version: string };
  /** Fired when the game is shutting down. The process exits after all handlers return. */
  exit: { exit_code: number };
  /** Fired every rendered frame. */
  frame: { delta_seconds: number; frame_number: number };
  /** Fired every frame with the current input state. */
  input: { keys_pressed: string[]; mouse_delta: [number, number] };
  /** Response to a prior `requestAsset` call. */
  asset_response: { request_id: string; path: string; data_base64: string | null; error: string | null };
};

export type EventName = keyof EventPayloads;

// Maps public event names → internal Rust message types.
const EVENT_TYPE: Record<EventName, _EngineMsg["type"]> = {
  init:           "Init",
  exit:           "Shutdown",
  frame:          "Frame",
  input:          "Input",
  asset_response: "AssetResponse",
};

class EngineApi {
  private handlers = new Map<EventName, Set<(payload: any) => void>>();
  private _fileCallbacks = new Map<string, { resolve: (v: string | null) => void; reject: (e: Error) => void }>();
  private _fileReqCounter = 0;

  constructor() {
    const rl = createInterface({ input: process.stdin, terminal: false });
    rl.on("line", (line) => {
      try {
        const msg = JSON.parse(line) as _EngineMsg;
        this._dispatch(msg);
      } catch {
        // ignore malformed messages
      }
    });
    rl.on("close", () => process.exit(0));
  }

  private _dispatch(msg: _EngineMsg): void {
    if (msg.type === "FileResponse") {
      const cb = this._fileCallbacks.get(msg.request_id);
      if (cb) {
        this._fileCallbacks.delete(msg.request_id);
        msg.error ? cb.reject(new Error(msg.error)) : cb.resolve(msg.data_base64);
      }
      return;
    }
    for (const [event, msgType] of Object.entries(EVENT_TYPE) as [EventName, _EngineMsg["type"]][]) {
      if (msg.type !== msgType) continue;
      const handlers = this.handlers.get(event);
      if (handlers) {
        for (const fn_ of handlers) fn_(msg);
      }
      if (event === "exit") {
        process.exit((msg as Extract<_EngineMsg, { type: "Shutdown" }>).exit_code);
      }
    }
  }

  /**
   * Subscribe to an engine event. The handler is called each time the event fires.
   * Registering a handler also sends a `Subscribe` message to the engine automatically.
   */
  on<E extends EventName>(event: E, handler: (payload: EventPayloads[E]) => void): void {
    if (!this.handlers.has(event)) this.handlers.set(event, new Set());
    this.handlers.get(event)!.add(handler as (payload: any) => void);
    this._send({ type: "Subscribe", events: [EVENT_TYPE[event]] });
  }

  private _send(msg: _ScriptMsg): void {
    process.stdout.write(JSON.stringify(msg) + "\n");
  }

  /** Log a message via the engine logger. Output includes timestamp and mod ID. */
  log(level: "info" | "warn" | "error", message: string): void {
    this._send({ type: "Log", level, message });
  }

  /** Set the title of the active window. */
  setWindowTitle(title: string): void {
    this._send({ type: "SetWindowTitle", title });
  }

  spawnSprite(entity_id: string, texture: string, position: [number, number, number], scale: [number, number, number] = [1, 1, 1]): void {
    this._send({ type: "SpawnSprite", entity_id, texture, position, scale });
  }

  moveEntity(entity_id: string, position: [number, number, number]): void {
    this._send({ type: "MoveEntity", entity_id, position });
  }

  destroyEntity(entity_id: string): void {
    this._send({ type: "DestroyEntity", entity_id });
  }

  requestAsset(request_id: string, path: string): void {
    this._send({ type: "AssetRequest", request_id, path });
  }

  /** Write a UTF-8 string to a sandboxed file for this mod. Path must not contain `..`. */
  writeFile(path: string, data: string): Promise<void> {
    return new Promise((resolve, reject) => {
      const request_id = String(++this._fileReqCounter);
      this._fileCallbacks.set(request_id, { resolve: () => resolve(), reject });
      this._send({ type: "FileWrite", request_id, path, data_base64: Buffer.from(data, "utf8").toString("base64") });
    });
  }

  /** Read a sandboxed file for this mod and return its contents as a UTF-8 string. */
  readFile(path: string): Promise<string> {
    return new Promise((resolve, reject) => {
      const request_id = String(++this._fileReqCounter);
      this._fileCallbacks.set(request_id, {
        resolve: (b64) => resolve(Buffer.from(b64!, "base64").toString("utf8")),
        reject,
      });
      this._send({ type: "FileRead", request_id, path });
    });
  }

  /** Delete a sandboxed file for this mod. */
  deleteFile(path: string): Promise<void> {
    return new Promise((resolve, reject) => {
      const request_id = String(++this._fileReqCounter);
      this._fileCallbacks.set(request_id, { resolve: () => resolve(), reject });
      this._send({ type: "FileDelete", request_id, path });
    });
  }
}

export const engine = new EngineApi();
