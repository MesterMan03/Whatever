// normal_map_debug.wgsl — encodes world-space vertex normals as RGB colour.
// Since the standard vertex layout has no normal attribute, this shader
// approximates normals from the position's XZ plane (Y=0 flat surface assumption).
// Replace with a real normal-map shader once per-entity uniforms are available.

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
    // Flat XZ-plane normal: (0, 1, 0) → maps to RGB (0.5, 1.0, 0.5) in normal-map convention.
    // This is a static approximation — real normal mapping requires a normals accessor.
    let n = vec3<f32>(0.0, 1.0, 0.0);
    return vec4<f32>(n * 0.5 + 0.5, 1.0);
}
