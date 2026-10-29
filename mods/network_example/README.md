# Network API example

One mod runs on both sides through `scripts/client.ts` and `scripts/server.ts`. Both import the immutable specification from `scripts/network.ts`. The server additionally registers a local client-data schema and initial values.

Run `cargo run` or `cargo run -- --mode server`. Initialization registers the protocol and logs an offline native codec roundtrip. Both sides report the same fingerprint and packet table:

| ID | Packet | Direction |
|---|---|---|
| 0 | login.hello | serverbound |
| 1 | login.welcome | clientbound |
| 2 | play.move | serverbound |
| 3 | play.players | clientbound |

`netdemo spec` lists that table; `netdemo codec` repeats the offline check, through F2 or server stdin. Encoding and decoding execute in Rust through the bundled API bridge.

No connections are opened. The server's typed hello handler demonstrates `client.data` and sending for the future transport stage; it is not called by the offline codec probe. `send` and `broadcast` reject while transport is unavailable.
