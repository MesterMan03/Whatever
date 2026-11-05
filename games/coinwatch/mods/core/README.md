# Core

Core is the game developer's main mod. New projects add their entry script, initial scene and editable assets here. Additional mods are optional extensions.

`meta.toml` selects the game's ID, name, tick rate and default runtime mode. The editor's starter meshes, shaders and font live in its separate `resources/starter` library and are imported into the scene-owning mod only when used. Assets are ordinary VFS files and can be changed or deleted.

The source checkout's `games/test/mods/debug` contains optional diagnostic tools and demonstrations. New games do not include them.
