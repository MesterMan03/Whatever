/** Private synchronous Wasm boundary for engine capabilities. */
type Native = (payload: string) => string;
const native = (globalThis as unknown as { __engineCall?: Native }).__engineCall;
let deliver: (message: any) => void = () => {};
let afterRemoval: () => void = () => {};
export function _installRemovalObserver(callback: () => void): void {
  afterRemoval = callback;
}
// During a delivery, the world changes only through synchronous host calls.
const reads = new Map<string, any>();
export type ComponentRow = { data: any; instance_id: string | null };
const componentRows = new Map<string, Map<string, ComponentRow | null>>();
let turn = 0;
export const _turn = (): number => turn;
const readOperations = new Set([
  "entity_metadata",
  "entity_record",
  "entity_list",
  "query_entities",
  "query_tags",
  "scene_info",
]);
export function _beginTurn(): void {
  turn++;
  reads.clear();
  componentRows.clear();
}
function rowsFor(kind: string): Map<string, ComponentRow | null> {
  let rows = componentRows.get(kind);
  if (!rows) {
    rows = new Map();
    componentRows.set(kind, rows);
  }
  return rows;
}
export function _rememberComponentRow(id: string, kind: string, row: ComponentRow): void {
  rowsFor(kind).set(id, row);
}
export function _readComponentRow(id: string, kind: string): ComponentRow | null {
  const rows = rowsFor(kind);
  const cached = rows.get(id);
  if (cached !== undefined) return cached;
  const row = _native("component_row", { id, kind }) as ComponentRow | null;
  rows.set(id, row);
  return row;
}
function invalidateWrite(op: string, args: object, payload: string): void {
  reads.clear();
  const call = args as { kind?: string; message?: { type: string; component_type?: string } };
  const kind =
    op === "component_patch"
      ? call.kind
      : call.message?.type === "ComponentPatch"
        ? call.message.component_type
        : undefined;
  // Physics normalization can create a default world. Structural commands
  // and other effects fall back to clearing all component reads.
  if (
    !kind ||
    kind === "core:rigid_body" ||
    kind === "core:collider" ||
    payload.includes('"default:physics_world"')
  ) {
    componentRows.clear();
    return;
  }
  componentRows.delete(kind);
  if (kind === "core:transform") componentRows.delete("core:world_transform");
}
export function _installDispatcher(callback: (message: any) => void): void {
  deliver = callback;
}
export function _native(op: string, args: object = {}): any {
  if (!native) throw new Error("The API requires the embedded Whatever runtime");
  const payload = JSON.stringify({ op, ...args });
  const cacheable = readOperations.has(op);
  if (cacheable && reads.has(payload)) return reads.get(payload);
  if (op === "command" || op === "component_patch") invalidateWrite(op, args, payload);
  const result = JSON.parse(native(payload));
  if (result.error) throw new Error(result.error);
  if (cacheable) reads.set(payload, result.value);
  return result.value;
}
export function _command(message: any): void {
  const messages = _native("command", { message }) ?? [];
  if (message.type === "EntityDestroy" || message.type === "ComponentRemove") afterRemoval();
  for (const message of messages) deliver(message);
}
export function _requestNow(message: any): any {
  const messages = _native("command", { message }) ?? [];
  const response = messages.find((candidate: any) => candidate.request_id === message.request_id);
  for (const candidate of messages) if (candidate !== response) deliver(candidate);
  if (!response) throw new Error("Engine operation did not return synchronously");
  if (response.error) throw new Error(response.error);
  return response;
}
export const _now = (): number => _native("clock");
export const _memory = (): { heap_used: number; rss: number } => _native("memory");
export function _exit(_code: number): never {
  throw new Error("Mod execution stopped");
}
