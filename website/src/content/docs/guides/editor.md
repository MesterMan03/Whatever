---
title: Work in the editor
description: Navigate the workspace, edit a scene, attach components, and inspect a running game.
---

The editor changes ordinary mod files. Opening a project does not evaluate its scripts. Play launches the normal sandboxed player.

## Open and inspect a scene

Open `whatever.wproject`, select a mod in the browser, and double-click a scene in its `scenes/` directory. The workspace combines a Hierarchy, scene viewport, Inspector, and resizable Project/Assets/Build Log browser.

| Action | Control |
| --- | --- |
| Orbit | Right-drag or Alt-left-drag |
| Pan | Middle-drag |
| Zoom | Scroll |
| Frame selected entity | F while over the viewport |
| Select / Move / Rotate / Scale | Q / W / E / R |
| Save | Ctrl+S |
| Undo / Redo | Ctrl+Z / Ctrl+Y |
| Open project | Ctrl+O |
| Build | Ctrl+B |

The grid, selection axes, and handles are editor overlays. They do not become game entities.

## Build a hierarchy

Drag near the top or bottom of a hierarchy row to insert a sibling. Drop in its middle to parent to that entity. Drop onto blank hierarchy space to move an entity to the scene root.

Reparenting preserves the world transform when the moved entity has a Transform. Cycles and zero-scale parents that prevent a valid conversion are rejected. Hierarchy changes participate in Undo and Redo.

## Edit components

Select an entity to edit its native components in the Inspector. Asset fields offer VFS choices and a Browse dialog; choosing an asset outside the collection imports it into the active mod.

Exported custom Component classes appear in **Add Component** with defaults, ranges, enum choices, and entity/asset selectors. Edit their public fields in the Inspector. Private state stays inside the script. Follow the [component guide](/guides/components/) to create one.

After source changes, refocus the editor or use **Reload component types** to refresh metadata. Restart Play to load the new code. The editor extracts metadata without running mod scripts.

## Play and live editing

Press Play to save and launch. On Windows, click the embedded player to focus it. The player and editor route input ownership so editor interactions don't replay accumulated mouse movement into the game.

During Play, the hierarchy and Inspector can display and edit live runtime entities. Live edits have temporary undo history. Stop returns to the authored scene; live changes are not automatically saved back into it.

Use a configured startup scene when you want the authored scene to exist before script initialization. Scripts can adopt it using `Scene.initialScene()`; see [scenes and assets](/guides/scenes-and-assets/).

## Save safely

Scene drafts validate before being applied. Invalid data stays editable and blocks saving. Missing component data is retained for repair, while unknown or invalid components block saving and execution. Leaving an unsaved scene offers Save, Discard, or Cancel.

For the complete project, live-editing, and portable-distribution behavior, see the [editor reference](/reference/editor/).
