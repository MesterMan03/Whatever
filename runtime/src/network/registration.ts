import {_send, nextReqId} from "../shared.ts";
import {_getMode} from "../environment.ts";
import {
  SpecificationDefinition, StructDefinition, packetSchema, toWire, fromWire,
  type SchemaValue, type States, type Fields, type Direction, type PacketPath, type PacketData, type DecodedPacket,
} from "./schema.ts";
import {dataProxy} from "./data.ts";

const pending = new Map<string, {resolve(value: any): void; reject(error: Error): void}>();
const endpoints = new Map<string, (message: NetworkEvent) => void>();
function request(value: object): Promise<any> {
  return new Promise((resolve, reject) => {
    const request_id = nextReqId();
    pending.set(request_id, {resolve, reject});
    try { _send({type: "NetworkRequest", request_id, request: value}); }
    catch (error) { pending.delete(request_id); reject(error); }
  });
}
export function handleResponse(message: {request_id: string; result: unknown; error: string | null}): void {
  const callback = pending.get(message.request_id);
  if (!callback) return;
  pending.delete(message.request_id);
  message.error ? callback.reject(new Error(message.error)) : callback.resolve(message.result);
}
export interface RegisteredPacketInfo { readonly id: number; readonly packet: string; readonly state: string; readonly direction: Direction }
type RegistrationInfo = {registration_id: string; specification_id: string; fingerprint: string; packets: RegisteredPacketInfo[]};
export interface ClientHandle<D> { readonly id: string; readonly connected: boolean; readonly data: D; flushData(): Promise<void> }
type Handler = (...args: any[]) => void | Promise<void>;
const logError = (error: unknown) => _send({type: "Log", level: "error", message: "Network callback/data update failed: " + String(error)});
function invoke(handlers: Iterable<Handler>, ...args: any[]): void {
  for (const handler of handlers) { try { Promise.resolve(handler(...args)).catch(logError); } catch (error) { logError(error); } }
}
class Peer<D> implements ClientHandle<D> {
  private connectedState = true;
  get connected(): boolean { return this.connectedState; }
  disconnect(): void { this.connectedState = false; }
  readonly data: D;
  private writes = new Set<Promise<unknown>>();
  private failure: unknown;
  constructor(readonly id: string, registration: string, data: unknown, schema: StructDefinition<Fields>) {
    Object.defineProperty(this, "id", {value: id, writable: false, configurable: false});
    this.data = dataProxy<D>(schema.schema, fromWire(schema.schema, data), wire => {
      if (!this.connected) throw new Error("Client is disconnected");
      const operation = request({op: "UpdateClientData", registration_id: registration, client_id: id, data: wire});
      this.writes.add(operation);
      void operation.then(() => this.writes.delete(operation), error => { this.writes.delete(operation); this.failure = error; logError(error); });
    });
    Object.defineProperty(this, "data", {value: this.data, writable: false, configurable: false});
  }
  async flushData(): Promise<void> { await Promise.all(this.writes); if (this.failure) throw this.failure; }
}
class Endpoint<S extends States> {
  readonly id: string;
  readonly specificationId: string;
  readonly fingerprint: string;
  readonly packets: readonly RegisteredPacketInfo[];
  protected active = true;
  protected handlers = new Map<string, Set<Handler>>();
  constructor(protected specification: SpecificationDefinition<S>, info: RegistrationInfo, protected outgoing: Direction) {
    this.id = info.registration_id; this.specificationId = info.specification_id; this.fingerprint = info.fingerprint;
    this.packets = Object.freeze(info.packets.map(packet => Object.freeze(packet)));
    endpoints.set(this.id, message => this.receive(message));
  }
  protected check(): void { if (!this.active) throw new Error("Network registration is closed"); }
  protected subscribe(direction: Direction, packet: string, handler: Handler): () => void {
    this.check(); packetSchema(this.specification, direction, packet);
    const handlers = this.handlers.get(packet) ?? new Set(); handlers.add(handler); this.handlers.set(packet, handlers);
    return () => handlers.delete(handler);
  }
  protected payload(packet: string, data: unknown): unknown { this.check(); return toWire(packetSchema(this.specification, this.outgoing, packet), data); }
  protected async encode(packet: string, data: unknown): Promise<Uint8Array> {
    const result = await request({op: "Encode", registration_id: this.id, packet, data: this.payload(packet, data)});
    return Uint8Array.from(Buffer.from(result.bytes, "base64"));
  }
  /** Offline codec inspection, accepting either packet direction. Includes the u16 ID, no stream length prefix. */
  async decodePacket(bytes: Uint8Array): Promise<DecodedPacket<S>> {
    this.check();
    if (!(bytes instanceof Uint8Array) || bytes.length > 1_048_576) throw new Error("Expected packet bytes within size limit");
    const result = await request({op: "Decode", registration_id: this.id, bytes: Buffer.from(bytes).toString("base64")});
    const schema = packetSchema(this.specification, result.direction, result.packet);
    return {...result, data: fromWire(schema, result.data)};
  }
  async close(): Promise<void> {
    this.check(); await request({op: "Close", registration_id: this.id});
    this.active = false; this.handlers.clear(); endpoints.delete(this.id);
  }
  protected receive(_message: NetworkEvent): void { /* Server/client overrides handle trusted native events. */ }
}
export class ClientEndpoint<S extends States> extends Endpoint<S> {
  constructor(spec: SpecificationDefinition<S>, info: RegistrationInfo) { super(spec, info, "serverbound"); }
  encodePacket<P extends PacketPath<S, "serverbound">>(packet: P, data: NoInfer<PacketData<S, "serverbound", P>>): Promise<Uint8Array> { return this.encode(packet, data); }
  on<P extends PacketPath<S, "clientbound">>(packet: P, handler: (data: PacketData<S, "clientbound", P>) => void | Promise<void>): () => void { return this.subscribe("clientbound", packet, handler); }
  async send<P extends PacketPath<S, "serverbound">>(packet: P, data: NoInfer<PacketData<S, "serverbound", P>>): Promise<void> {
    await request({op: "Send", registration_id: this.id, client_id: null, packet, data: this.payload(packet, data)});
  }
  protected override receive(message: NetworkEvent): void {
    if (message.event !== "packet") return;
    const schema = packetSchema(this.specification, "clientbound", message.packet);
    invoke(this.handlers.get(message.packet) ?? [], fromWire(schema, message.data));
  }
}
export class ServerEndpoint<S extends States, D> extends Endpoint<S> {
  private peers = new Map<string, Peer<D>>();
  private connects = new Set<Handler>();
  private disconnects = new Set<Handler>();
  constructor(spec: SpecificationDefinition<S>, info: RegistrationInfo, private dataDefinition: StructDefinition<Fields>) { super(spec, info, "clientbound"); }
  get clients(): readonly ClientHandle<D>[] { return Object.freeze([...this.peers.values()]); }
  getClient(id: string): ClientHandle<D> | undefined { return this.peers.get(id); }
  onConnect(handler: (client: ClientHandle<D>) => void | Promise<void>): () => void { this.check(); this.connects.add(handler); return () => this.connects.delete(handler); }
  onDisconnect(handler: (client: ClientHandle<D>) => void | Promise<void>): () => void { this.check(); this.disconnects.add(handler); return () => this.disconnects.delete(handler); }
  on<P extends PacketPath<S, "serverbound">>(packet: P, handler: (data: PacketData<S, "serverbound", P>, client: ClientHandle<D>) => void | Promise<void>): () => void { return this.subscribe("serverbound", packet, handler); }
  encodePacket<P extends PacketPath<S, "clientbound">>(packet: P, data: NoInfer<PacketData<S, "clientbound", P>>): Promise<Uint8Array> { return this.encode(packet, data); }
  async validateClientData(data: NoInfer<D>): Promise<D> {
    this.check(); const result = await request({op: "ValidateClientData", registration_id: this.id, data: toWire(this.dataDefinition.schema, data)});
    return fromWire(this.dataDefinition.schema, result);
  }
  async send<P extends PacketPath<S, "clientbound">>(client: ClientHandle<D>, packet: P, data: NoInfer<PacketData<S, "clientbound", P>>): Promise<void> {
    this.check(); if (this.peers.get(client.id) !== client || !client.connected) throw new Error("ClientHandle does not belong to this server or is disconnected");
    await request({op: "Send", registration_id: this.id, client_id: client.id, packet, data: this.payload(packet, data)});
  }
  async broadcast<P extends PacketPath<S, "clientbound">>(packet: P, data: NoInfer<PacketData<S, "clientbound", P>>): Promise<void> {
    this.payload(packet, data);
    throw new Error("Network transport is not implemented; use encodePacket for offline validation");
  }
  override async close(): Promise<void> { await super.close(); for (const peer of this.peers.values()) peer.disconnect(); this.peers.clear(); this.connects.clear(); this.disconnects.clear(); }
  protected override receive(message: NetworkEvent): void {
    if (message.event === "connected") {
      if (this.peers.has(message.client_id)) throw new Error("Duplicate client connection event");
      const peer = new Peer<D>(message.client_id, this.id, message.data, this.dataDefinition);
      this.peers.set(peer.id, peer); invoke(this.connects, peer); return;
    }
    const peer = message.client_id ? this.peers.get(message.client_id) : undefined;
    if (!peer) throw new Error("Network event references an unknown client");
    if (message.event === "disconnected") { peer.disconnect(); invoke(this.disconnects, peer); this.peers.delete(peer.id); return; }
    const schema = packetSchema(this.specification, "serverbound", message.packet);
    invoke(this.handlers.get(message.packet) ?? [], fromWire(schema, message.data), peer);
  }
}
export type NetworkEvent =
  | {event: "connected"; registration_id: string; client_id: string; data: unknown}
  | {event: "disconnected"; registration_id: string; client_id: string}
  | {event: "packet"; registration_id: string; client_id?: string; packet: string; data: unknown};
export function handleEvent(message: NetworkEvent): void { endpoints.get(message.registration_id)?.(message); }
export async function registerClientEndpoint<S extends States>(specification: SpecificationDefinition<S>): Promise<ClientEndpoint<S>> {
  if (_getMode() !== "client") throw new Error("registerClient requires client mode");
  if (!(specification instanceof SpecificationDefinition)) throw new Error("Expected Network.Specification");
  const result = await request({op: "Register", side: "client", specification: specification.manifest, client_data: null});
  return new ClientEndpoint(specification, result);
}
export async function registerServerEndpoint<S extends States, C extends StructDefinition<Fields>>(specification: SpecificationDefinition<S>, options: {clientData: C; initialClientData: NoInfer<SchemaValue<C>>}): Promise<ServerEndpoint<S, SchemaValue<C>>> {
  if (_getMode() !== "server") throw new Error("registerServer requires server mode");
  if (!(specification instanceof SpecificationDefinition) || !(options.clientData instanceof StructDefinition)) throw new Error("Expected specification and client-data struct");
  const result = await request({op: "Register", side: "server", specification: specification.manifest,
    client_data: {schema: options.clientData.schema, initial: toWire(options.clientData.schema, options.initialClientData)}});
  return new ServerEndpoint(specification, result, options.clientData);
}
