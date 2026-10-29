import { Console, Engine, Network } from "@whatever-engine/api";
import { Game } from "./network.ts";
const ClientData = Network.Struct({
  playerId: Network.Type.Optional(Network.Type.I32),
  name: Network.Type.String,
  authenticated: Network.Type.Bool,
  tags: Network.Type.Array(Network.Type.String),
});
Engine.on("init", async () => {
  try {
    const server = await Network.registerServer(Game, {
      clientData: ClientData,
      initialClientData: { name: "", authenticated: false, tags: [] },
    });
    server.on("login.hello", async (packet, client) => {
      client.data.name = packet.name;
      client.data.playerId = 1;
      client.data.authenticated = true;
      client.data.tags.push("player");
      await client.flushData();
      await server.send(client, "login.welcome", { playerId: client.data.playerId });
    });
    const probe = async () => {
      const data = await server.validateClientData({
        playerId: 1,
        name: "Offline probe",
        authenticated: true,
        tags: ["probe"],
      });
      const bytes = await server.encodePacket("login.welcome", { playerId: data.playerId! });
      const decoded = await server.decodePacket(bytes);
      return `Server codec: ${bytes.length} bytes, ${decoded.packet}; ${server.fingerprint}`;
    };
    Console.register({
      name: "netdemo",
      description: "Network API example (offline)",
      subcommands: [
        { name: "codec", handler: probe },
        {
          name: "spec",
          handler: () => server.packets.map((p) => `${p.id}: ${p.direction} ${p.packet}`),
        },
      ],
    });
    Engine.log("info", await probe());
  } catch (error) {
    Engine.log("error", `Network example failed: ${error}`);
  }
});
