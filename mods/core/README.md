# Example of the "core" mod

The `core` mod acts as the heart of any game made in Whatever. It is always loaded first and has no dependencies.

It also requires a special `meta.toml` file with the following content:

```toml
[game]
id   = "game_id"
name = "Game Name"
```

This file is used to set the game's ID and name, which are used in the window title and save file paths. The `core` mod can be thought of as the "base layer" of the engine, providing essential functionality and assets that other mods can build upon.

## Scripting

While the engine will support writing scripts using TypeScript with no compilation step, you can use Bun's bundler to automatically minify your scripts for production builds.

You can see an example of this here, where `src` contains the original TypeScript files, and `script` contains the bundled output.

The following command was used to bundle the scripts:

```bash
bun build src/index.ts --outfile scripts/index.js --minify --external=@whatever-engine/api
```