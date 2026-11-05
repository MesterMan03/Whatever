# Whatever Starter

An editable starter library maintained with Whatever Engine. Import the .wasset
through Project > Import Asset Library, or choose an Official prefab from Scene >
Instantiate Prefab. Contents are copied into the receiving mod's libraries folder.
Reimporting preserves edited bundles by creating another copy; it never upgrades
existing scene objects. Prefab instances are independent hierarchies.

Contents: 33 meshes, 48 textures, 30 sprites, 8 shaders, 131 prefabs, 6 scripts, 16 audio, 1 fonts.

Primitives use unit dimensions except the original sphere/capsule. Modular pieces
are intended for meter-like units. Meshes use explicit normals and UVs. Materials
are supplied through shaders and textures because OBJ materials are not loaded.
Sprites have transparent backgrounds. Audio is original 22.05 kHz mono PCM WAV.
Physics Sandbox owns its world; its local references are remapped per instance.
Scripted prefabs need a script-enabled mod. The six exported components are loaded
by the normal sandboxed component host and expose editable fields in Inspector.

All content except Noto Sans is covered by LICENSE (Apache-2.0). Noto Sans is
Copyright 2022 The Noto Project Authors and is distributed under fonts/OFL.txt.
Run python tools/generate-starter.py from the source checkout to regenerate the
original content. Existing Noto Sans and its license are retained unchanged.

Library authoring paths use whatever_starter:// and whatever_starter:ComponentName.
Import rewrites those prefixes to the receiving mod and imported folder. Relative
JavaScript/TypeScript imports remain relative. Scripts do not run during import.
