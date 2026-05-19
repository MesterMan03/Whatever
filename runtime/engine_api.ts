import * as readline from "readline";

type EngineMessage =
  | { type: "Init"; mod_id: string; engine_version: string }
  | { type: "Frame"; delta_seconds: number; frame_number: number }
  | { type: "Input"; keys_pressed: string[]; mouse_delta: [number, number] }
  | { type: "AssetResponse"; request_id: string; path: string; data_base64: string | null; error: string | null }
  | { type: "Shutdown" };

type ScriptMessage =
  | { type: "Subscribe"; events: string[] }
  | { type: "AssetRequest"; request_id: string; path: string }
  | { type: "SpawnSprite"; entity_id: string; texture: string; position: [number, number, number]; scale: [number, number, number] }
  | { type: "MoveEntity"; entity_id: string; position: [number, number, number] }
  | { type: "DestroyEntity"; entity_id: string }
  | { type: "Log"; level: string; message: string }
  | { type: "SetWindowTitle"; title: string };

type EventMap = {
  [K in EngineMessage["type"]]: Extract<EngineMessage, { type: K }>;
};

type Handler<T extends EngineMessage["type"]> = (msg: EventMap[T]) => void;

class EngineApi {
  private handlers = new Map<string, Set<Function>>();

  constructor() {
    const rl = readline.createInterface({ input: process.stdin, terminal: false });
    rl.on("line", (line) => {
      try {
        const msg = JSON.parse(line) as EngineMessage;
        const set = this.handlers.get(msg.type);
        if (set) {
          for (const fn_ of set) fn_(msg);
        }
      } catch {
        // ignore malformed messages
      }
    });
    rl.on("close", () => process.exit(0));
  }

  on<T extends EngineMessage["type"]>(event: T, handler: Handler<T>): void {
    if (!this.handlers.has(event)) this.handlers.set(event, new Set());
    this.handlers.get(event)!.add(handler as Function);
    this.send({ type: "Subscribe", events: [event] });
  }

  private send(msg: ScriptMessage): void {
    process.stdout.write(JSON.stringify(msg) + "\n");
  }

  log(level: "info" | "warn" | "error", message: string): void {
    this.send({ type: "Log", level, message });
  }

  setWindowTitle(title: string): void {
    this.send({ type: "SetWindowTitle", title });
  }

  spawnSprite(entity_id: string, texture: string, position: [number, number, number], scale: [number, number, number] = [1, 1, 1]): void {
    this.send({ type: "SpawnSprite", entity_id, texture, position, scale });
  }

  moveEntity(entity_id: string, position: [number, number, number]): void {
    this.send({ type: "MoveEntity", entity_id, position });
  }

  destroyEntity(entity_id: string): void {
    this.send({ type: "DestroyEntity", entity_id });
  }

  requestAsset(request_id: string, path: string): void {
    this.send({ type: "AssetRequest", request_id, path });
  }
}

export const engine = new EngineApi();
