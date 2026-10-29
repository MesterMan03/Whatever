// uv_debug.wgsl — visualises UV coordinates as a red/green gradient.
// U → red channel, V → green channel.  Useful for checking mesh UV layout.

struct CameraUniform {
    view_proj: mat4x4<f32>,
}

@group(0) @binding(0) var<uniform> camera: CameraUniform;

struct VertexInput {
    @location(0) position:   vec3<f32>,
    @location(1) tex_coords: vec2<f32>,
    @location(2) normal:     vec3<f32>,
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
    // Red = U, Green = V, Blue = 0, Alpha = 1
    return vec4<f32>(in.tex_coords.x, in.tex_coords.y, 0.0, 1.0);
}
