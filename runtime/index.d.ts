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
    /** Response to a prior `requestAsset` call. */
    asset_response: {
        request_id: string;
        path: string;
        data_base64: string | null;
        error: string | null;
    };
};
export type EventName = keyof EventPayloads;
declare class EngineApi {
    private handlers;
    private _fileCallbacks;
    private _fileReqCounter;
    constructor();
    private _dispatch;
    /**
     * Subscribe to an engine event. The handler is called each time the event fires.
     * Registering a handler also sends a `Subscribe` message to the engine automatically.
     */
    on<E extends EventName>(event: E, handler: (payload: EventPayloads[E]) => void): void;
    private _send;
    /** Log a message via the engine logger. Output includes timestamp and mod ID. */
    log(level: "info" | "warn" | "error", message: string): void;
    /** Set the title of the active window. */
    setWindowTitle(title: string): void;
    spawnSprite(entity_id: string, texture: string, position: [number, number, number], scale?: [number, number, number]): void;
    moveEntity(entity_id: string, position: [number, number, number]): void;
    destroyEntity(entity_id: string): void;
    requestAsset(request_id: string, path: string): void;
    /** Write a UTF-8 string to a sandboxed file for this mod. Path must not contain `..`. */
    writeFile(path: string, data: string): Promise<void>;
    /** Read a sandboxed file for this mod and return its contents as a UTF-8 string. */
    readFile(path: string): Promise<string>;
    /** Delete a sandboxed file for this mod. */
    deleteFile(path: string): Promise<void>;
}
export declare const engine: EngineApi;
export {};
