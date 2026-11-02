---
title: Work in the editor
description: Navigate the workspace, edit a scene, attach components, and inspect a running game.
---

The editor changes ordinary mod files. Opening a project does not evaluate its scripts. Play launches the normal sandboxed player.

## Open and inspect a scene

Open `whatever.wproject`, select a mod in Assets, and double-click a scene in its `scenes/` directory. The workspace combines a Hierarchy, scene viewport, Inspector, and resizable Assets/Log browser. Both tabs retain the panel's height, including an empty Log.

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

The viewport has no default grid. Transform handles follow the object's world rotation. Rotate uses three axis-aligned rings; cameras show their orientation and viewing frustum. Select a camera and choose **Look through camera** to preview it. All overlays are clipped to the scene view and do not become game entities.

## Manage files and dependencies

Assets combines scenes, scripts and asset files. Use **Create** for folders, scripts and scenes, or right-click a file/folder to rename, move or delete it. Destinations are relative to the selected mod. Rename/move updates authored scene VFS references and manifest script/initial-scene paths; update JS/TS imports and path strings yourself. Deletion moves the item to the mod's ignored `.whatever/trash` for recovery. Existing references remain visible as missing-asset errors.

**Mod Settings** exposes runtime sides, dependency checkboxes with semver requirements, the initial scene, entry scripts, asset folder and load order. Scene asset selectors list the owning mod and its direct dependencies. Cross-mod custom components and asset references are checked again on save, load and packaging. The Build settings prevent excluding a mod required by another included mod and explain the dependency.

## Build a hierarchy

Drag near the top or bottom of a hierarchy row to insert a sibling. Drop in its middle to parent to that entity. Drop onto blank hierarchy space to move an entity to the scene root.

Reparenting preserves the world transform when the moved entity has a Transform. Cycles and zero-scale parents that prevent a valid conversion are rejected. Hierarchy changes participate in Undo and Redo.

## Edit components

Select an entity to edit its native components in the Inspector. Asset fields offer VFS choices and a Browse dialog; choosing an asset outside the collection imports it into the active mod.

Exported custom Component classes appear in **Add Component** with defaults, ranges, enum choices, and entity/asset selectors. Edit their public fields in the Inspector. Private state stays inside the script. Follow the [component guide](/guides/components/) to create one.

After source changes, refocus the editor or use **Reload component types** to refresh metadata. Restart Play to load the new code. The editor extracts metadata without running mod scripts.

Reloading runs in the background. The bottom bar shows a spinner, the current mod,
and overall progress while you continue editing. Repeated requests are combined,
and old-project results are ignored. Failed reloads retain the previous component
types; use **View log** for details, fix the source, and reload again. Saving, Play,
and Build require a successful reload. Scene saves reuse the loaded types, and
Play, Build, and Mod Settings validation also report background activity there.

## Play and live editing

Press Play to save and launch. On Windows, click the embedded player to focus it. The player and editor route input ownership so editor interactions don't replay accumulated mouse movement into the game.

During Play, the hierarchy and Inspector can display and edit live runtime entities. Live edits have temporary undo history. Stop returns to the authored scene; live changes are not automatically saved back into it.

Use a configured startup scene when you want the authored scene to exist before script initialization. Scripts can adopt it using `Scene.initialScene()`; see [scenes and assets](/guides/scenes-and-assets/).

## Save safely

Scene drafts validate before being applied. Invalid data stays editable and blocks saving. Missing component data is retained for repair, while unknown or invalid components block saving and execution. Leaving an unsaved scene offers Save, Discard, or Cancel.

For the complete project, live-editing, and portable-distribution behavior, see the [editor reference](/reference/editor/).
