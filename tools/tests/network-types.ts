import {Network} from "../../runtime/dist/index.js";
const hello = Network.Packet({name:Network.Type.String,flag:Network.Type.Optional(Network.Type.Bool)});
const game = Network.Specification("fixture:typed",{login:{serverbound:{hello},clientbound:{welcome:Network.Packet({id:Network.Type.I32})}}});
const data = Network.Struct({id:Network.Type.Optional(Network.Type.I32),tags:Network.Type.Array(Network.Type.String)});
async function check() {
  const client = await Network.registerClient(game);
  client.connect({address:"localhost:4444",trust:"tofu"});
  client.onConnect(info=>{const address:string=info.address; const max:number|null=info.maxDatagramSize;});
  client.onDisconnect(reason=>{const text:string=reason;});
  client.send("login.hello",{name:"Alex"},{transport:"datagram"});
  // @ts-expect-error: certificate validation cannot be disabled indiscriminately
  client.connect({address:"localhost:4444",trust:"insecure"});
  // @ts-expect-error: transport is QUIC stream or datagram
  client.send("login.hello",{name:"Alex"},{transport:"tcp"});
  client.encodePacket("login.hello",{name:"Alex"});
  client.on("login.welcome",packet=>{const n:number=packet.id;});
  // @ts-expect-error: wrong direction
  client.send("login.welcome",{id:1});
  // @ts-expect-error: wrong payload
  client.send("login.hello",{name:1});
  // @ts-expect-error: unknown packet
  client.on("login.unknown",()=>{});
  const server = await Network.registerServer(game,{clientData:data,initialClientData:{tags:[]}});
  server.listen({address:"0.0.0.0:4444",maxClients:64});
  server.on("login.hello",(packet,peer,context)=>{
    const transport:Network.TransportKind=context.transport;
    peer.data.id=1;peer.data.tags.push(packet.name);
    server.send(peer,"login.welcome",{id:peer.data.id});
    // @ts-expect-error: schema properties are inferred
    peer.data.missing=true;
    // @ts-expect-error: array type retained
    peer.data.tags.push(123);
    // @ts-expect-error: whole handle cannot be replaced
    peer.data={tags:[]};
  });
  // @ts-expect-error: defaults cannot widen schema
  Network.registerServer(game,{clientData:data,initialClientData:{tags:[1]}});
  // @ts-expect-error: server send needs a client
  server.send("login.welcome",{id:1});
  const decoded = await client.decodePacket(new Uint8Array());
  if (decoded.packet === "login.welcome") {const n:number=decoded.data.id;}
}
