import { Console, Engine, Network } from "@whatever-engine/api";
import { Game } from "./network.ts";

Engine.on("init", async () => {
  try {
    const client = await Network.registerClient(Game);
    let playerId: number | undefined;
    let players: number[] = [];
    let sequence = 0;
    let lastPong = "none";
    const inbox: string[] = [];
    const remember = (line: string) => {
      inbox.push(line);
      if (inbox.length > 100) inbox.shift();
      Engine.log("info", line);
    };
    client.onConnect((connection) => Engine.log("info", `Connected to ${connection.address}`));
    client.onDisconnect((reason) => {
      playerId = undefined;
      players = [];
      Engine.log("info", `Disconnected: ${reason}`);
    });
    client.on("session.welcome", (packet) => {
      playerId = packet.playerId;
      Engine.log("info", `You are player #${playerId}`);
    });
    client.on("session.roster", (packet) => {
      players = packet.players;
      Engine.log("info", `Players online: ${players.map((id) => `#${id}`).join(", ")}`);
    });
    client.on("chat.message", (packet) => {
      const destination = packet.recipientId === undefined ? "everyone" : `#${packet.recipientId}`;
      remember(`[#${packet.senderId} -> ${destination}] ${packet.text}`);
    });
    client.on("chat.error", (packet) => remember(`Server: ${packet.message}`));
    client.on("chat.pong", (packet, context) => {
      lastPong = `${packet.sequence} (${context.transport})`;
      Engine.log("info", `Pong ${lastPong}`);
    });
    const probe = async () => {
      const bytes = await client.encodePacket("chat.tell", { recipientId: 1, text: "Whatever" });
      const decoded = await client.decodePacket(bytes);
      return `Client codec: ${bytes.length} bytes, ${decoded.packet}; ${client.fingerprint}`;
    };
    Console.register({
      name: "netdemo",
      description: "QUIC messenger client",
      subcommands: [
        {
          name: "connect",
          description: "Join a server; trust defaults to tofu for this development demo",
          args: [
            { name: "address", type: "string", required: true },
            { name: "trust", type: "string", suggest: () => ["strict", "tofu"] },
          ],
          handler: async ({ address, trust }) => {
            const policy = String(trust ?? "tofu");
            if (policy !== "strict" && policy !== "tofu")
              throw new Error("Trust must be strict or tofu");
            const connection = await client.connect({ address: String(address), trust: policy });
            return `Connected to ${connection.address} (${policy})`;
          },
        },
        {
          name: "disconnect",
          handler: async () => {
            await client.disconnect();
            return "Disconnected";
          },
        },
        {
          name: "players",
          handler: async () => {
            await client.send("session.list", {});
            return `Players: ${players.map((id) => `#${id}`).join(", ")}`;
          },
        },
        {
          name: "tell",
          args: [
            { name: "player_id", type: "int", required: true, suggest: () => players.map(String) },
            { name: "message", type: "string", required: true },
          ],
          handler: async ({ player_id, message }) => {
            await client.send("chat.tell", {
              recipientId: Number(player_id),
              text: String(message),
            });
            return `Queued message to #${player_id}`;
          },
        },
        {
          name: "say",
          args: [{ name: "message", type: "string", required: true }],
          handler: async ({ message }) => {
            await client.send("chat.say", { text: String(message) });
            return "Queued message to everyone";
          },
        },
        {
          name: "ping",
          handler: async () => {
            await client.send("chat.ping", { sequence: ++sequence }, { transport: "datagram" });
            return `Queued datagram ping ${sequence}`;
          },
        },
        {
          name: "status",
          handler: () =>
            `${client.connected ? "Connected" : "Disconnected"}; player=${playerId ?? "none"}; players=${players.join(",")}; pong=${lastPong}`,
        },
        { name: "inbox", handler: () => (inbox.length ? [...inbox] : ["No messages yet."]) },
        { name: "codec", handler: probe },
        {
          name: "spec",
          handler: () => client.packets.map((p) => `${p.id}: ${p.direction} ${p.packet}`),
        },
      ],
    });
    Engine.log("info", await probe());
    Engine.log("info", "Messenger ready: netdemo connect localhost:4444");
  } catch (error) {
    Engine.log("error", `Network example failed: ${error}`);
  }
});
