# Custom Shaders

The engine owns the rendering *frame structure* — camera uniform, texture bindings,
vertex layout, and render pass setup.  Mods supply the WGSL shaders that decide
what pixels look like.  As long as a shader satisfies the contract below, the
engine can load and run it without any Rust changes.

---

## Standard Shader Contract

Every shader loaded by the engine **must** follow this interface exactly.

### Bind groups

```wgsl
// Group 0 — provided by the engine every frame (camera).
struct CameraUniform {
    view_proj: mat4x4<f32>,
}
@group(0) @binding(0) var<uniform> camera: CameraUniform;

// Group 1 — provided by the engine per draw call (texture).
// Always bound; may be a 1×1 opaque-white fallback when no texture is set.
@group(1) @binding(0) var t_diffuse: texture_2d<f32>;
@group(1) @binding(1) var s_diffuse: sampler;
```

### Vertex attributes

```wgsl
struct VertexInput {
    @location(0) position:   vec3<f32>,
    @location(1) tex_coords: vec2<f32>,
}
```

### Required entry points

The vertex entry point must be `vs_main`; the fragment entry point must be `fs_main`.

### Output

The fragment shader must write to `@location(0)` as `vec4<f32>` (RGBA).
Alpha blending is always enabled — the alpha channel is respected.

---

## Minimal working example

This is the built-in sprite shader (`core://shaders/sprite.wgsl`).
Copy it as a starting point:

```wgsl
struct CameraUniform {
    view_proj: mat4x4<f32>,
}

@group(0) @binding(0) var<uniform> camera: CameraUniform;

struct VertexInput {
    @location(0) position:   vec3<f32>,
    @location(1) tex_coords: vec2<f32>,
}

struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0)       tex_coords:    vec2<f32>,
}

@vertex
fn vs_main(in: VertexInput) -> VertexOutput {
    var out: VertexOutput;
    out.clip_position = camera.view_proj * vec4<f32>(in.position, 1.0);
    out.tex_coords    = in.tex_coords;
    return out;
}

@group(1) @binding(0) var t_diffuse: texture_2d<f32>;
@group(1) @binding(1) var s_diffuse: sampler;

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    return textureSample(t_diffuse, s_diffuse, in.tex_coords);
}
```

---

## Using a custom shader with `core:sprite_renderer`

Set the `shader` field when creating or updating a `SpriteRenderer`:

```typescript
import { BuiltInComponents, Scene } from "@whatever-engine/api";

const entity = await Scene.spawnSprite(
  "my_mod://textures/player.png",
  [0, 0, 0],
  [1, 1, 1],
);
// Switch to a custom shader at any time:
entity.setComponent(new BuiltInComponents.SpriteRenderer({
  texture: "my_mod://textures/player.png",
  shader:  "my_mod://shaders/outline.wgsl",
}));
```

When `shader` is omitted it defaults to `"core://shaders/sprite.wgsl"`.

---

## Using `core:mesh_renderer`

`MeshRenderer` renders arbitrary triangle geometry instead of a quad.

```typescript
import { Scene } from "@whatever-engine/api";

const entity = await Scene.spawnMesh(
  "my_mod://meshes/triangle.glb",      // mesh file path
  "my_mod://shaders/solid.wgsl",   // shader path
  [0, 0, -5],                      // world position
  { texture: "my_mod://textures/crate.png" },
);
```

Or via the low-level component API:

```typescript
entity.setComponent("core:mesh_renderer", {
  mesh:    "my_mod://meshes/cube.obj",
  shader:  "core://shaders/sprite.wgsl",
  texture: "my_mod://textures/crate.png",  // optional
});
```

---

## Mesh file formats

The format is detected automatically from the file extension.

### `.json` — custom simple format

```json
{
  "vertices": [
    [x, y, z, u, v],
    ...
  ],
  "indices": [0, 1, 2, ...]
}
```

- `vertices` — each entry is `[x, y, z, tex_u, tex_v]`
- `indices`  — triangle list; every three indices form one triangle
- Index type is `u16` — maximum **65 535** unique vertices per mesh

### `.obj` — Wavefront OBJ

- Standard triangulated OBJ geometry
- Materials (`.mtl` references) are silently ignored — geometry only
- UV coordinates are read from `vt` entries; if absent they default to `[0, 0]`
- Multiple objects/groups are merged into a single draw call

### `.glb` / `.gltf` — glTF 2.0

- GLB (binary glTF) and self-contained GLTF with base64-embedded buffers are supported
- The **first mesh** and its **first `TRIANGLES` primitive** are loaded
- `POSITION` (vec3) and `TEXCOORD_0` (vec2) accessors are read; others are ignored
- Materials, animations, skins, and cameras are ignored — geometry only
- **External `.bin` buffer references are not resolved** — export as GLB instead

---

## Common patterns

### Solid colour (ignore texture)

```wgsl
@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    return vec4<f32>(1.0, 0.0, 0.5, 1.0); // hot pink
}
```

### Tint the texture

```wgsl
const TINT: vec4<f32> = vec4<f32>(1.0, 0.4, 0.0, 1.0); // orange

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    return textureSample(t_diffuse, s_diffuse, in.tex_coords) * TINT;
}
```

### UV offset (scroll effect)

```wgsl
// Scroll offset must be baked in by the engine in v1 (no time uniform yet).
// As a static example, shift UVs by half:
@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    let uv = fract(in.tex_coords + vec2<f32>(0.5, 0.0));
    return textureSample(t_diffuse, s_diffuse, uv);
}
```

### Vertex displacement (wave effect)

```wgsl
// Displace Y by a sine wave based on X position.
// Without a time uniform the wave is static.
@vertex
fn vs_main(in: VertexInput) -> VertexOutput {
    var pos = in.position;
    pos.y += sin(pos.x * 3.14159) * 0.1;
    var out: VertexOutput;
    out.clip_position = camera.view_proj * vec4<f32>(pos, 1.0);
    out.tex_coords    = in.tex_coords;
    return out;
}
```

---

## v1 Limitations

- **No custom bind groups** beyond group 1.  There are no per-entity uniforms,
  no time uniform, and no material parameters accessible from WGSL in v1.
  Workarounds: bake constants into the shader source; use the texture to carry
  parameter data (palette / LUT texture).
- **No depth buffer** — draw order is determined by the order entities appear in
  the renderer's internal maps.
- **Transform baked into vertices** — the engine applies `core:transform` to mesh
  vertices on the CPU at upload time.  Changing a mesh entity's transform
  re-uploads the vertex buffer.
- **Pipeline compiled on first use** — a shader is compiled when the first entity
  with that shader path is registered.  This may cause a brief stall.  Use the
  test mod's `spawnmesh` command to trigger compilation before gameplay starts.
- **No hot-reload** — shader edits require an engine restart.
- **glTF** — only the first mesh and first primitive are loaded; external `.bin`
  buffers are not supported (use GLB); materials and animations are ignored.
- **Index limit** — all mesh formats use `u16` indices: maximum 65 535 unique
  vertices per mesh.

---

## Future extensions (planned)

- Per-entity model matrix uniform (group 2) to decouple transforms from vertices
- Time and frame-number uniforms for animated shaders
- Shader hot-reload via VFS file watcher
- Depth buffer toggle per pipeline
- Multi-texture support (group 1 slots 2–N)
- Instanced rendering
