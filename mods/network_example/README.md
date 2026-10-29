# Network Messenger

A shared typed protocol with separate client/server entries. The server mod assigns increasing player IDs, keeps them in `client.data.playerId`, and routes private messages and broadcasts. Reconnecting gets a fresh ID; IDs are not reused during the server mod's lifetime. Gameplay stays entirely in this mod.

Build the engine and bundled host, then start a server:

```sh
cargo run -- --mode server
```

In its terminal:

```text
netdemo host 4444
```

Start several clients with `cargo run` (or the built engine executable). Open F2 in each:

```text
netdemo connect localhost:4444
netdemo players
netdemo tell 2 "hello there"
netdemo say "hello everyone"
netdemo ping
netdemo status
netdemo inbox
netdemo disconnect
```

`netdemo connect <address> [strict|tofu]` defaults to **TOFU for this development example**. The public API defaults to strict certificate validation. Servers generate and persist their development identity automatically; players never enter certificate values. TOFU remembers the first server key and rejects later key changes. Its first connection cannot independently detect an impersonator. A properly certified server can be joined with `netdemo connect game.example:4444 strict`.

Server commands are `netdemo host [port] [max_clients]`, `netdemo stop` and `netdemo players`. Port zero chooses a free port. Both sides retain `netdemo spec` and `netdemo codec` for schema/codec inspection.

Chat uses the reliable ordered QUIC stream. `ping` uses QUIC datagrams in both directions. Message text is limited to 1024 UTF-8 bytes, and the client retains the latest 100 messages for `inbox`. The server derives sender IDs from the connection's local data; the client cannot supply a sender ID. Unknown recipients produce an error packet.

Operators can pass `--tls-cert <chain.pem> --tls-key <key.pem>` to the engine. Clients use OS certificate roots and can add a private authority with `--tls-ca <authority.pem>`. Native identity/trust files are separate from mod-accessible files; `--network-data <directory>` isolates them for tests or separate instances.

After building the bundled host, run the real mod integration demo without windows or audio hardware:

```sh
cargo test --locked network_messenger -- --ignored
```

It runs one server and three bundled client mod hosts, checks strict rejection of a development certificate, IDs 1/2/3, private-message isolation, broadcasts, missing-recipient errors, datagram ping and reconnect as player 4.
