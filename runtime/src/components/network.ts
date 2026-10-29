import * as schema from "../network/schema.ts";
import * as registration from "../network/registration.ts";

/** Typed protocols, QUIC connections, native codecs and per-client server data. */
export namespace Network {
  export const Type: schema.TypeNamespace = schema.Type;
  export const Struct = schema.defineStruct;
  export const Packet = schema.definePacket;
  export const Specification = schema.defineSpecification;
  export const registerClient = registration.registerClientEndpoint;
  export const registerServer = registration.registerServerEndpoint;
  export type Infer<T> = schema.SchemaValue<T>;
  export type Client<D> = registration.ClientHandle<D>;
  export type PacketInfo = registration.RegisteredPacketInfo;
  export type TrustPolicy = registration.ClientTrustPolicy;
  export type TransportKind = registration.DeliveryKind;
  export type ConnectOptions = registration.ClientConnectOptions;
  export type ListenOptions = registration.ServerListenOptions;
  export type SendOptions = registration.PacketSendOptions;
  export type ConnectionInfo = registration.PeerConnectionInfo;
  export type ListenerInfo = registration.ServerListenerInfo;
  export type PacketContext = registration.PacketReceiveContext;
  export type ClientRegistration<S extends schema.States> = registration.ClientEndpoint<S>;
  export type ServerRegistration<S extends schema.States, D> = registration.ServerEndpoint<S, D>;
}
