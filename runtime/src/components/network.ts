import * as schema from "../network/schema.ts";
import * as registration from "../network/registration.ts";

/** Declarative protocols and engine-owned offline codecs. Transport is a later stage. */
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
  export type ClientRegistration<S extends schema.States> = registration.ClientEndpoint<S>;
  export type ServerRegistration<S extends schema.States, D> = registration.ServerEndpoint<S, D>;
}
