import { Network } from "@whatever-engine/api";
const Position = Network.Struct({ x: Network.Type.F64, y: Network.Type.F64, z: Network.Type.F64 });
export const Game = Network.Specification("network_example:game", {
  login: {
    serverbound: { hello: Network.Packet({ name: Network.Type.String.with({ maxLength: 64 }) }) },
    clientbound: { welcome: Network.Packet({ playerId: Network.Type.I32.with({ min: 0 }) }) },
  },
  play: {
    serverbound: {
      move: Network.Packet({
        tick: Network.Type.VarInt.with({ min: 0 }),
        position: Position,
        sprinting: Network.Type.Optional(Network.Type.Bool),
      }),
    },
    clientbound: {
      players: Network.Packet({ positions: Network.Type.Array(Position, { maxLength: 256 }) }),
    },
  },
});
