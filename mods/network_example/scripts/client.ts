import { Console, Engine, Network } from "@whatever-engine/api";
import { Game } from "./network.ts";
Engine.on("init", async () => {
  try {
    const client = await Network.registerClient(Game);
    client.on("login.welcome", (packet) =>
      Engine.log("info", `Assigned player ${packet.playerId}`),
    );
    const probe = async () => {
      const bytes = await client.encodePacket("login.hello", { name: "Whatever" });
      const decoded = await client.decodePacket(bytes);
      return `Client codec: ${bytes.length} bytes, ${decoded.packet}; ${client.fingerprint}`;
    };
    Console.register({
      name: "netdemo",
      description: "Network API example (offline)",
      subcommands: [
        { name: "codec", handler: probe },
        {
          name: "spec",
          handler: () => client.packets.map((p) => `${p.id}: ${p.direction} ${p.packet}`),
        },
      ],
    });
    Engine.log("info", await probe());
  } catch (error) {
    Engine.log("error", `Network example failed: ${error}`);
  }
});
