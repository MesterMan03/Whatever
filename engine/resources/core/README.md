# Core

Core is the metadata and shared-asset layer of a Whatever game. It loads first and contains no diagnostic scripts or demonstration gameplay.

`meta.toml` selects the game's ID, name, tick rate and default runtime mode. Other mods use `core://shaders/sprite.wgsl`, `core://shaders/mesh_lit.wgsl` and `core://fonts/default.ttf`.

The first-party tools and demonstration camera are in [the debug suite](../debug/README.md). A game can replace core metadata/assets and omit the debug suite entirely.
