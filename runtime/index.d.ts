/** Arbitrary JSON-serializable value used for inter-mod messages. */
export type JsonValue = string | number | boolean | null | JsonValue[] | {
    [key: string]: JsonValue;
};
/** Metadata about a loaded mod, mirroring mod.toml. */
export type ModManifest = {
    id: string;
    name: string;
    version: string;
    description: string;
    authors: string[];
    license: string;
    dependencies: Record<string, string>;
    load_order: {
        after: string[];
        before: string[];
    };
    script?: {
        entry: string;
        runtime: string;
    };
};
/** Payload types for each public event name. */
export type EventPayloads = {
    /** Fired once after the engine has initialised and the window is ready. */
    init: {
        mod_id: string;
        engine_version: string;
    };
    /** Fired when the game is shutting down. The process exits after all handlers return. */
    exit: {
        exit_code: number;
    };
    /** Fired every rendered frame. */
    frame: {
        delta_seconds: number;
        frame_number: number;
    };
    /** Fired every frame with the current input state. */
    input: {
        keys_pressed: string[];
        mouse_delta: [number, number];
    };
    /** Response to a prior `Assets.request` call. */
    asset_response: {
        request_id: string;
        path: string;
        data_base64: string | null;
        error: string | null;
    };
    /**
     * Fired when another mod sends this mod a message via `Message.send`.
     * If `request_id` is present the sender is awaiting a reply — call `Message.reply(request_id, data)`.
     * Not fired for replies that arrive via the `Message.send(id, msg, timeout)` overload.
     */
    mod_message: {
        source_mod_id: string;
        message: JsonValue;
        request_id?: string;
    };
};
export type EventName = keyof EventPayloads;
/** Core engine events and logging. */
export declare const Engine: {
    /**
     * Subscribe to an engine event. The handler is called each time the event fires.
     * Registering a handler also sends a `Subscribe` message to the engine automatically
     * (except for `mod_message`, which the engine routes unconditionally).
     */
    on<E extends EventName>(event: E, handler: (payload: EventPayloads[E]) => void): void;
    /** Log a message via the engine logger. Output includes timestamp and mod ID. */
    log(level: "info" | "warn" | "error", message: string): void;
};
/** Window management. */
export declare const Window: {
    /** Set the title of the active window. */
    setTitle(title: string): void;
};
/** Sandboxed per-mod file I/O. Paths must not contain `..`. */
export declare const File: {
    /** Write a UTF-8 string to a sandboxed file for this mod. */
    write(path: string, data: string): Promise<void>;
    /** Read a sandboxed file for this mod and return its contents as a UTF-8 string. */
    read(path: string): Promise<string>;
    /** Delete a sandboxed file for this mod. */
    delete(path: string): Promise<void>;
};
/** Scene entity management. */
export declare const Scene: {
    spawnSprite(entity_id: string, texture: string, position: [number, number, number], scale?: [number, number, number]): void;
    moveEntity(entity_id: string, position: [number, number, number]): void;
    destroyEntity(entity_id: string): void;
};
/** Asset requests (VFS paths: `mod_id://relative/path`). */
export declare const Assets: {
    request(request_id: string, path: string): void;
};
/** Query information about loaded mods. */
export declare const Mods: {
    /** Returns the manifests of all currently loaded mods in load order. */
    list(): Promise<ModManifest[]>;
    /** Returns the manifest for a specific mod by ID. Rejects if the mod is not loaded. */
    get(id: string): Promise<ModManifest>;
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
export declare const Message: _MessageNamespace;
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
/** Register a command that users can invoke from the developer console. */
export declare const Console: {
    register(spec: CommandSpec): void;
};
export {};
