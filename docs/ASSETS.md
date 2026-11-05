# Asset libraries and prefabs

Asset libraries are portable authoring content. Importing a library creates ordinary
editable mod files; games run without the original archive or the editor. Prefabs
are reusable entity templates. They use the same components, hierarchy and named
entity references as scenes. The engine does not define Cube, Sphere, Tree or any
other game object: those names and their contents come from prefab documents.

## Library format

A `.wasset` is an ordinary ZIP with `wasset.toml` at its root:

```toml
format_version = 1
id = "my_library"
name = "My Library"
version = "1.0.0"
authors = ["Creator name"]
license = "MIT"
description = "Reusable models and scripts"
homepage = "https://example.com"

# Optional external mod requirements. The receiving mod must declare these too.
[dependencies]
shared_tools = "^1.0"
```

Other files may be meshes, textures, shaders, audio, scripts, prefabs, attribution
or other data. Folder structure is preserved. Include all referenced files and
licenses. Archives do not contain install hooks and import never evaluates scripts.
An archive is not a mod and does not become a runtime dependency.

Inside a library, use `my_library://path/to/file` for its own assets and
`my_library:ComponentName` for its own custom component IDs. Imports map these
prefixes to the receiving mod and bundle directory. For example:

```text
my_library://textures/wood.png -> core://libraries/my_library/textures/wood.png
my_library:Spinner            -> core:my_library_Spinner
```

Namespace substitution applies to UTF-8 JSON, prefab/scene, TypeScript/JavaScript,
WGSL, TOML, Markdown and text files. It leaves binary files and the original
`wasset.toml` attribution unchanged. Relative script imports remain relative.
Give exported components explicit static `type` IDs using the library namespace.
Avoid hardcoding the eventual receiving mod ID.

Imports publish a complete bundle under `mods/<mod>/libraries/<library-id>` in one
directory rename. Identical imports are reused. If an existing bundle was edited,
the next import uses a suffix and remaps all its internal references together.
Component IDs also include that suffix, so edited copies can coexist. Existing
assets, scene objects and edited scripts are never overwritten or upgraded.
Imported bundles keep a local `.wasset-import.json` binding. Exporting that folder
uses it to restore portable library namespaces, so an edited library can be shared
and imported into a different mod. This local binding is omitted from the archive.

Scripts are discovered by the usual component inspector, and run in the usual
sandbox when the game starts. Scripted prefabs need a script-enabled receiving
mod. The library's external mod requirements must exist, satisfy its version
requirements, and be declared by the receiving mod before import.

Archives use stored or Deflate ZIP entries. Tooling rejects traversal, absolute
paths, Windows device names, links, duplicate/case-colliding names, file/directory
collisions and managed tooling directories. Imports are bounded to 10,000 entries,
128 MiB per file and 512 MiB unpacked. CRC checks happen before publication.

## Prefab format

Use `.wprefab` JSON with exactly one root `entity`:

```json
{
  "version": 1,
  "name": "Wooden Crate",
  "description": "A reusable editable crate",
  "category": "Props",
  "entity": {
    "name": "Crate",
    "script_id": "crate",
    "components": {
      "core:transform": {
        "position": [0, 0, 0],
        "rotation": [0, 0, 0, 1],
        "scale": [1, 1, 1]
      },
      "core:mesh_renderer": {
        "mesh": "my_library://meshes/crate.obj",
        "shader": "my_library://shaders/mesh_lit.wgsl"
      }
    },
    "children": []
  }
}
```

Root and child entities support names, enabled state, tags, script IDs, components
and children exactly as scenes do. `{"$entity":"child_name"}` refers to a script ID
inside that prefab. Prefabs must be self-contained: external entity references are
rejected. Each spawn allocates fresh runtime IDs, including internal references;
failed creation rolls back the entire hierarchy. Physics prefabs must include any
authored world/body they reference. The official Physics Sandbox demonstrates this.

Mods auto-discover `.wprefab` files anywhere in VFS. Their standard `prefabs/` and
`libraries/` folders are mounted automatically, alongside `scenes/`. Set
`[mounts] prefabs = "templates"` or `libraries = "shared_content"` in `mod.toml` to
change the physical directories. Library archives use the same prefab format.
Malformed documents, unknown components, missing assets and undeclared mod
dependencies are checked during project validation/build and runtime spawning.

Instances are independent copies. Changing a source prefab affects future
instances. This version has no inheritance, linked overrides or apply/revert.
Placed copies obey the scene-owning mod's dependency rules as ordinary entities.

## Editor and scripting

- **Scene > Instantiate Prefab** or Hierarchy **+ > Prefab** opens the searchable
  catalog. It includes project prefabs and the bundled official library.
- Choosing an official prefab imports its library on first use in a background
  job, then places the hierarchy after component inspection completes.
- Double-click a `.wprefab` in Assets to place a copy. Undo removes the placed
  hierarchy. Imported files remain available as normal authoring content.
- **Scene > Save Selected as Prefab** saves the selected entity and its children.
  Include referenced entities in the subtree; external references are diagnosed.
- **Project > Import Asset Library** imports `.wasset`; the Assets Create menu
  exposes the same command. **Project > Export Asset Library** packs a folder
  containing `wasset.toml`. Imports/exports show background progress in the status
  bar and preserve existing files.
- Moving or renaming assets through Assets updates typed references in scenes and
  prefabs. Literal paths inside scripts still need manual editing.

```ts
import { Prefab } from "@whatever-engine/api";

const instance = await Prefab.spawn("core://prefabs/crate.wprefab");
instance.root.editComponent("core:transform", (pose) => {
  pose.position = [2, 1, 0];
});
const child = instance.get("named_child");
```

`Prefab.spawn` submits a batched command and returns `Promise<PrefabInstance>`.
Instances expose `root`, `get(scriptId)`, `entities()`, `entityCount`, `path` and
`loadTime`. Native validation enforces the caller's direct dependency on the
prefab's mod and the prefab owner's own asset/component dependencies. No native
primitive shortcuts are involved.

## CLI and official pack

```sh
wmod library pack path/to/folder --output MyLibrary.wasset
wmod library inspect MyLibrary.wasset
wmod library import MyLibrary.wasset --project path/to/game --mod core
```

The official **Whatever Starter** source is `engine/resources/starter`. Its catalog
contains 131 prefabs, 33 meshes, 48 textures, 30 UI sprites, 8 shaders, 16 sounds,
6 reusable script components and Noto Sans with its OFL. It includes primitives,
modular building pieces, furniture, nature props, lighting rigs, material samples,
animated pickups, cameras and a self-contained physics sandbox. Content remains
game-owned after import. Original content follows the repository Apache-2.0
license; Noto Sans retains its separate OFL-1.1 attribution.

`python tools/generate-starter.py` reproduces the original generated content using
only Python's standard library. Portable editor kits include both its catalog and
`resources/libraries/WhateverStarter.wasset`, packed by the same public tooling.
