import { createInterface } from "node:readline";

// Internal IPC types — match Rust serde tags exactly, not part of the public API.
type _EngineMsg =
  | { type: "CommandInvoke"; request_id: string; command_path: string[]; args: JsonValue[] }
  | { type: "Init"; mod_id: string; engine_version: string }
  | { type: "Frame"; delta_seconds: number; frame_number: number }
  | { type: "Input"; keys_pressed: string[]; mouse_delta: [number, number] }
  | { type: "AssetResponse"; request_id: string; path: string; data_base64: string | null; error: string | null }
  | { type: "FileResponse"; request_id: string; data_base64: string | null; error: string | null }
  | { type: "ModListResponse"; request_id: string; mods: ModManifest[] }
  | { type: "ModGetResponse"; request_id: string; manifest: ModManifest | null; error: string | null }
  | { type: "ModMessageReceived"; source_mod_id: string; request_id: string | null; payload: JsonValue }
  | { type: "ModMessageReplyDelivered"; request_id: string; payload: JsonValue }
  | { type: "Shutdown"; exit_code: number };

type _ScriptMsg =
  | { type: "RegisterCommand"; name: string; description: string; subcommands: _CommandNodeSpec[]; args: _ArgSpec[]; has_handler: boolean }
  | { type: "CommandResponse"; request_id: string; output: string[]; error: string | null }
  | { type: "Subscribe"; events: string[] }
  | { type: "AssetRequest"; request_id: string; path: string }
  | { type: "SpawnSprite"; entity_id: string; texture: string; position: [number, number, number]; scale: [number, number, number] }
  | { type: "MoveEntity"; entity_id: string; position: [number, number, number] }
  | { type: "DestroyEntity"; entity_id: string }
  | { type: "Log"; level: "info" | "warn" | "error"; message: string }
  | { type: "SetWindowTitle"; title: string }
  | { type: "FileWrite"; request_id: string; path: string; data_base64: string }
  | { type: "FileRead"; request_id: string; path: string }
  | { type: "FileDelete"; request_id: string; path: string }
  | { type: "ModListRequest"; request_id: string }
  | { type: "ModGetRequest"; request_id: string; mod_id: string }
  | { type: "ModMessageSend"; target_mod_id: string; request_id: string | null; payload: JsonValue }
  | { type: "ModMessageReply"; request_id: string; payload: JsonValue };

/** Arbitrary JSON-serializable value used for inter-mod messages. */
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
  /** Fired every rendered frame. */
  frame: { delta_seconds: number; frame_number: number };
  /** Fired every frame with the current input state. */
  input: { keys_pressed: string[]; mouse_delta: [number, number] };
  /** Response to a prior `Assets.request` call. */
  asset_response: { request_id: string; path: string; data_base64: string | null; error: string | null };
  /**
   * Fired when another mod sends this mod a message via `Message.send`.
   * If `request_id` is present the sender is awaiting a reply — call `Message.reply(request_id, data)`.
   * Not fired for replies that arrive via the `Message.send(id, msg, timeout)` overload.
   */
  mod_message: { source_mod_id: string; message: JsonValue; request_id?: string };
};

export type EventName = keyof EventPayloads;

// Maps public event names → internal Rust message types.
// mod_message is absent: it is dispatched specially and needs no Subscribe.
const _EVENT_TYPE: Partial<Record<EventName, _EngineMsg["type"]>> = {
  init:           "Init",
  exit:           "Shutdown",
  frame:          "Frame",
  input:          "Input",
  asset_response: "AssetResponse",
};

// --- Shared internal IPC state ---

const _handlers = new Map<EventName, Set<(payload: any) => void>>();
const _fileCallbacks = new Map<string, { resolve: (v: string | null) => void; reject: (e: Error) => void }>();
const _modListCallbacks = new Map<string, { resolve: (v: ModManifest[]) => void; reject: (e: Error) => void }>();
const _modGetCallbacks = new Map<string, { resolve: (v: ModManifest) => void; reject: (e: Error) => void }>();
const _msgCallbacks = new Map<string, { resolve: (v: JsonValue) => void; reject: (e: Error) => void }>();
let _reqCounter = 0;

function _send(msg: _ScriptMsg): void {
  process.stdout.write(JSON.stringify(msg) + "\n");
}

function _dispatch(msg: _EngineMsg): void {
  if (msg.type === "CommandInvoke") {
    _handleCommandInvoke(msg);
    return;
  }

  if (msg.type === "FileResponse") {
    const cb = _fileCallbacks.get(msg.request_id);
    if (cb) {
      _fileCallbacks.delete(msg.request_id);
      msg.error ? cb.reject(new Error(msg.error)) : cb.resolve(msg.data_base64);
    }
    return;
  }

  if (msg.type === "ModListResponse") {
    const cb = _modListCallbacks.get(msg.request_id);
    if (cb) {
      _modListCallbacks.delete(msg.request_id);
      cb.resolve(msg.mods);
    }
    return;
  }

  if (msg.type === "ModGetResponse") {
    const cb = _modGetCallbacks.get(msg.request_id);
    if (cb) {
      _modGetCallbacks.delete(msg.request_id);
      msg.error ? cb.reject(new Error(msg.error)) : cb.resolve(msg.manifest!);
    }
    return;
  }

  if (msg.type === "ModMessageReplyDelivered") {
    const cb = _msgCallbacks.get(msg.request_id);
    if (cb) {
      _msgCallbacks.delete(msg.request_id);
      cb.resolve(msg.payload);
    }
    return;
  }

  if (msg.type === "ModMessageReceived") {
    const handlers = _handlers.get("mod_message");
    if (handlers) {
      const payload: EventPayloads["mod_message"] = {
        source_mod_id: msg.source_mod_id,
        message: msg.payload,
        ...(msg.request_id !== null && { request_id: msg.request_id }),
      };
      for (const fn_ of handlers) fn_(payload);
    }
    return;
  }

  for (const [event, msgType] of Object.entries(_EVENT_TYPE) as [EventName, _EngineMsg["type"]][]) {
    if (msg.type !== msgType) continue;
    const handlers = _handlers.get(event);
    if (handlers) {
      for (const fn_ of handlers) fn_(msg);
    }
    if (event === "exit") {
      process.exit((msg as Extract<_EngineMsg, { type: "Shutdown" }>).exit_code);
    }
  }
}

const _rl = createInterface({ input: process.stdin, terminal: false });
_rl.on("line", (line) => {
  try {
    _dispatch(JSON.parse(line) as _EngineMsg);
  } catch {
    // ignore malformed messages
  }
});
_rl.on("close", () => process.exit(0));

// --- Public API namespaces ---

/** Core engine events and logging. */
export const Engine = {
  /**
   * Subscribe to an engine event. The handler is called each time the event fires.
   * Registering a handler also sends a `Subscribe` message to the engine automatically
   * (except for `mod_message`, which the engine routes unconditionally).
   */
  on<E extends EventName>(event: E, handler: (payload: EventPayloads[E]) => void): void {
    if (!_handlers.has(event)) _handlers.set(event, new Set());
    _handlers.get(event)!.add(handler as (payload: any) => void);
    const msgType = _EVENT_TYPE[event];
    if (msgType) _send({ type: "Subscribe", events: [msgType] });
  },

  /** Log a message via the engine logger. Output includes timestamp and mod ID. */
  log(level: "info" | "warn" | "error", message: string): void {
    _send({ type: "Log", level, message });
  },
};

/** Window management. */
export const Window = {
  /** Set the title of the active window. */
  setTitle(title: string): void {
    _send({ type: "SetWindowTitle", title });
  },
};

/** Sandboxed per-mod file I/O. Paths must not contain `..`. */
export const File = {
  /** Write a UTF-8 string to a sandboxed file for this mod. */
  write(path: string, data: string): Promise<void> {
    return new Promise((resolve, reject) => {
      const request_id = String(++_reqCounter);
      _fileCallbacks.set(request_id, { resolve: () => resolve(), reject });
      _send({ type: "FileWrite", request_id, path, data_base64: Buffer.from(data, "utf8").toString("base64") });
    });
  },

  /** Read a sandboxed file for this mod and return its contents as a UTF-8 string. */
  read(path: string): Promise<string> {
    return new Promise((resolve, reject) => {
      const request_id = String(++_reqCounter);
      _fileCallbacks.set(request_id, {
        resolve: (b64) => resolve(Buffer.from(b64!, "base64").toString("utf8")),
        reject,
      });
      _send({ type: "FileRead", request_id, path });
    });
  },

  /** Delete a sandboxed file for this mod. */
  delete(path: string): Promise<void> {
    return new Promise((resolve, reject) => {
      const request_id = String(++_reqCounter);
      _fileCallbacks.set(request_id, { resolve: () => resolve(), reject });
      _send({ type: "FileDelete", request_id, path });
    });
  },
};

/** Scene entity management. */
export const Scene = {
  spawnSprite(entity_id: string, texture: string, position: [number, number, number], scale: [number, number, number] = [1, 1, 1]): void {
    _send({ type: "SpawnSprite", entity_id, texture, position, scale });
  },

  moveEntity(entity_id: string, position: [number, number, number]): void {
    _send({ type: "MoveEntity", entity_id, position });
  },

  destroyEntity(entity_id: string): void {
    _send({ type: "DestroyEntity", entity_id });
  },
};

/** Asset requests (VFS paths: `mod_id://relative/path`). */
export const Assets = {
  request(request_id: string, path: string): void {
    _send({ type: "AssetRequest", request_id, path });
  },
};

/** Query information about loaded mods. */
export const Mods = {
  /** Returns the manifests of all currently loaded mods in load order. */
  list(): Promise<ModManifest[]> {
    return new Promise((resolve, reject) => {
      const request_id = String(++_reqCounter);
      _modListCallbacks.set(request_id, { resolve, reject });
      _send({ type: "ModListRequest", request_id });
    });
  },

  /** Returns the manifest for a specific mod by ID. Rejects if the mod is not loaded. */
  get(id: string): Promise<ModManifest> {
    return new Promise((resolve, reject) => {
      const request_id = String(++_reqCounter);
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

// --- Console command types (internal) ---

type _ArgSpec = {
  name: string;
  type: "string" | "int" | "float" | "bool";
  required: boolean;
  description: string;
};

type _CommandNodeSpec = {
  name: string;
  description: string;
  subcommands: _CommandNodeSpec[];
  args: _ArgSpec[];
  has_handler: boolean;
};

// --- Inter-mod communication ---

/** Inter-mod communication. */
export const Message: _MessageNamespace = {
  sendAndForget<T extends JsonValue>(id: string, message: T): void {
    _send({ type: "ModMessageSend", target_mod_id: id, request_id: null, payload: message });
  },
  send<T extends JsonValue, U extends JsonValue>(id: string, message: T, timeout: number): Promise<U> {
    return new Promise<U>((resolve, reject) => {
      const request_id = String(++_reqCounter);
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

// --- Console command registration ---

/** Public arg type for Console.register(). */
export type ArgType = "string" | "int" | "float" | "bool";

/** Argument specification for a command. */
export type ArgSpec = {
  name: string;
  type: ArgType;
  required?: boolean;
  description?: string;
};

/** A command or subcommand specification. */
export type CommandSpec = {
  name: string;
  description?: string;
  subcommands?: CommandSpec[];
  args?: ArgSpec[];
  handler?: (args: Record<string, string | number | boolean>) => string | string[] | Promise<string | string[]>;
};

// Handlers and their arg specs, keyed by dot-joined command path (e.g. "mycmd" or "mycmd.sub")
const _cmdHandlers = new Map<string, NonNullable<CommandSpec["handler"]>>();
const _cmdArgSpecs = new Map<string, _ArgSpec[]>();

function _specToInternal(spec: CommandSpec, pathPrefix: string): _CommandNodeSpec {
  const path = pathPrefix ? `${pathPrefix}.${spec.name}` : spec.name;
  const mappedArgs: _ArgSpec[] = (spec.args ?? []).map((a) => ({
    name: a.name,
    type: a.type,
    required: a.required ?? false,
    description: a.description ?? "",
  }));
  if (spec.handler) {
    _cmdHandlers.set(path, spec.handler);
    _cmdArgSpecs.set(path, mappedArgs);
  }
  return {
    name: spec.name,
    description: spec.description ?? "",
    subcommands: (spec.subcommands ?? []).map((s) => _specToInternal(s, path)),
    args: mappedArgs,
    has_handler: !!spec.handler,
  };
}

// Handle CommandInvoke from the engine
// command_path includes the root name, e.g. ["myfoo"] or ["myfoo", "sub"]
function _handleCommandInvoke(msg: Extract<_EngineMsg, { type: "CommandInvoke" }>): void {
  const handlerKey = msg.command_path.join(".");
  const handler = _cmdHandlers.get(handlerKey);
  const argSpecs = _cmdArgSpecs.get(handlerKey) ?? [];

  // Build args keyed by name (falling back to index if spec is missing)
  const argsRecord: Record<string, string | number | boolean> = {};
  msg.args.forEach((v, i) => {
    const name = argSpecs[i]?.name ?? String(i);
    argsRecord[name] = v as string | number | boolean;
  });

  if (!handler) {
    _send({ type: "CommandResponse", request_id: msg.request_id, output: [], error: `no handler registered for '${handlerKey}'` });
    return;
  }

  Promise.resolve(handler(argsRecord))
    .then((result) => {
      const lines = Array.isArray(result) ? result : [result];
      _send({ type: "CommandResponse", request_id: msg.request_id, output: lines, error: null });
    })
    .catch((err: unknown) => {
      const msg2 = err instanceof Error ? err.message : String(err);
      _send({ type: "CommandResponse", request_id: msg.request_id, output: [], error: msg2 });
    });
}

/** Register a command that users can invoke from the developer console. */
export const Console = {
  register(spec: CommandSpec): void {
    if (!/^[a-z_]+$/.test(spec.name)) {
      throw new Error(`Console.register: command name '${spec.name}' must match [a-z_]+`);
    }
    const internal = _specToInternal(spec, "");
    _send({
      type: "RegisterCommand",
      name: internal.name,
      description: internal.description,
      subcommands: internal.subcommands,
      args: internal.args,
      has_handler: internal.has_handler,
    });
  },
};

