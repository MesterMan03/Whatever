import { Console, Engine, Network } from "@whatever-engine/api";
import { ClientData, Game } from "./network.ts";

Engine.on("init", async () => {
  try {
    const server = await Network.registerServer(Game, {
      clientData: ClientData,
      initialClientData: {},
    });
    const players = new Map<number, Network.Client<Network.Infer<typeof ClientData>>>();
    let nextPlayerId = 1;
    const roster = () => [...players.keys()].sort((a, b) => a - b);
    const refreshRoster = async () => {
      if (!server.listening) return;
      try {
        await server.broadcast("session.roster", { players: roster() });
      } catch (error) {
        Engine.log("warn", `Roster refresh failed: ${error}`);
      }
    };
    server.onConnect(async (client) => {
      if (nextPlayerId > 2_147_483_647) throw new Error("Player ID space exhausted");
      const playerId = nextPlayerId++;
      client.data.playerId = playerId;
      await client.flushData();
      players.set(playerId, client);
      await server.send(client, "session.welcome", { playerId });
      Engine.log("info", `Player #${playerId} joined (${client.id})`);
      await refreshRoster();
    });
    server.onDisconnect(async (client, reason) => {
      const playerId = client.data.playerId;
      if (playerId === undefined) return;
      players.delete(playerId);
      Engine.log("info", `Player #${playerId} left: ${reason}`);
      await refreshRoster();
    });
    server.on("session.list", async (_packet, client) => {
      await server.send(client, "session.roster", { players: roster() });
    });
    server.on("chat.tell", async (packet, sender) => {
      const recipient = players.get(packet.recipientId);
      if (!recipient?.connected) {
        await server.send(sender, "chat.error", {
          message: `Player #${packet.recipientId} is not connected`,
        });
        return;
      }
      await server.send(recipient, "chat.message", {
        senderId: sender.data.playerId!,
        recipientId: packet.recipientId,
        text: packet.text,
      });
    });
    server.on("chat.say", async (packet, sender) => {
      await server.broadcast("chat.message", {
        senderId: sender.data.playerId!,
        text: packet.text,
      });
    });
    server.on("chat.ping", async (packet, client, context) => {
      await server.send(client, "chat.pong", packet, { transport: context.transport });
    });
    const probe = async () => {
      const bytes = await server.encodePacket("session.welcome", { playerId: 1 });
      const decoded = await server.decodePacket(bytes);
      return `Server codec: ${bytes.length} bytes, ${decoded.packet}; ${server.fingerprint}`;
    };
    Console.register({
      name: "netdemo",
      description: "QUIC messenger server",
      subcommands: [
        {
          name: "host",
          description: "Listen on a UDP port (default 4444); zero chooses a free port",
          args: [
            { name: "port", type: "int" },
            { name: "max_clients", type: "int" },
          ],
          handler: async ({ port, max_clients }) => {
            const address = `0.0.0.0:${port ?? 4444}`;
            const info = await server.listen({ address, maxClients: Number(max_clients ?? 128) });
            return `Messenger listening on ${info.address}`;
          },
        },
        {
          name: "stop",
          handler: async () => {
            await server.stop();
            return "Messenger stopped";
          },
        },
        { name: "players", handler: () => roster().map((id) => `Player #${id}`) },
        { name: "codec", handler: probe },
        {
          name: "spec",
          handler: () => server.packets.map((p) => `${p.id}: ${p.direction} ${p.packet}`),
        },
      ],
    });
    Engine.log("info", await probe());
    Engine.log("info", "Messenger ready: netdemo host [port] [max_clients]");
  } catch (error) {
    Engine.log("error", `Network example failed: ${error}`);
  }
});
