---
title: Client and server
description: Select script entries by runtime side and keep presentation separate from shared simulation.
---

Whatever runs a client with presentation or a headless server. Both use independent simulation ticking, ECS, and native physics. A server has no window, GPU, or audio output device.

## Choose entries per side

```toml
[mod]
id = "hello_mod"
name = "Hello Game"
version = "0.1.0"
sides = ["client", "server"]

[dependencies]
core = "*"

[script]
client = "scripts/client.ts"
server = "scripts/server.ts"
```

Use `[script].entry` for shared code, or side-specific entries for distinct bootstraps. Runtime-side selection and fallback rules are documented in the [API side reference](/reference/api/sides/). Project validation still requires the full dependency collection, including dependencies used on the other side.

## Guard presentation work

`Engine.mode` is available before top-level mod evaluation:

```ts
import { Engine, Window } from "@whatever-engine/api";

Engine.on("init", () => {
  Engine.setTickRate(60);
  if (Engine.mode === "client") {
    Window.setTitle("Hello Game");
    Engine.setFpsCap(120);
  }
});
```

Window/audio methods and client camera/presentation controls reject use on a server. Storing render component data in ECS remains possible; actual presentation is client work.

## Run each side

From a source checkout:

```sh
cargo run -- --project ./hello-game
cargo run -- --project ./hello-game --mode server
```

The server accepts the same registered console commands on stdin that the client console uses.

## Connect deliberately

Running a client and server does not automatically replicate a game world. Networking uses explicitly registered schemas and packets over native QUIC connections. Mods own their replication, authority, and gameplay rules.

Read the [network API](/reference/api/network/) for specification registration, typed packets, delivery modes, certificate trust, and callback behavior. The repository's [network example](https://github.com/MesterMan03/Whatever/tree/main/games/test/mods/network_example) contains complete client/server flows.
