import { Network } from "@whatever-engine/api";

const T = Network.Type;

const PlayerId = T.VarInt.with({ min: 1 });
const Text = T.String.with({ maxLength: 1024 });
export const ClientData = Network.Struct({ playerId: T.Optional(PlayerId) });
export const Game = Network.Specification("network_example:game", {
  session: {
    serverbound: { list: Network.Packet({}) },
    clientbound: {
      welcome: Network.Packet({ playerId: PlayerId }),
      roster: Network.Packet({ players: T.Array(PlayerId) }),
    },
  },
  chat: {
    serverbound: {
      tell: Network.Packet({ recipientId: PlayerId, text: Text }),
      say: Network.Packet({ text: Text }),
      ping: Network.Packet({ sequence: T.VarInt.with({ min: 0 }) }),
    },
    clientbound: {
      message: Network.Packet({
        senderId: PlayerId,
        recipientId: T.Optional(PlayerId),
        text: Text,
      }),
      error: Network.Packet({ message: Text }),
      pong: Network.Packet({ sequence: T.VarInt.with({ min: 0 }) }),
    },
  },
});
