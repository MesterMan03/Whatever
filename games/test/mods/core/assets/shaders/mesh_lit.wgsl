struct CameraUniform {
    view_proj: mat4x4<f32>,
    position: vec3<f32>,
    _pad: f32,
}

@group(0) @binding(0) var<uniform> camera: CameraUniform;

struct DirectionalLight {
    direction: vec3<f32>,
    _pad0: f32,
    color: vec3<f32>,
    intensity: f32,
}

struct PointLight {
    position: vec3<f32>,
    range: f32,
    color: vec3<f32>,
    intensity: f32,
}

struct LightingUniform {
    ambient_color: vec3<f32>,
    ambient_intensity: f32,
    dir_light_count: u32,
    point_light_count: u32,
    _pad: vec2<u32>,
    dir_lights: array<DirectionalLight, 4>,
    point_lights: array<PointLight, 8>,
}

@group(2) @binding(0) var<uniform> lighting: LightingUniform;

struct VertexInput {
    @location(0) position:   vec3<f32>,
    @location(1) tex_coords: vec2<f32>,
    @location(2) normal:     vec3<f32>,
}

struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0)       tex_coords:    vec2<f32>,
    @location(1)       world_pos:     vec3<f32>,
    @location(2)       world_normal:  vec3<f32>,
}

@vertex
fn vs_main(in: VertexInput) -> VertexOutput {
    var out: VertexOutput;
    out.clip_position = camera.view_proj * vec4<f32>(in.position, 1.0);
    out.tex_coords   = in.tex_coords;
    out.world_pos    = in.position;
    out.world_normal = in.normal;
    return out;
}

@group(1) @binding(0) var t_diffuse: texture_2d<f32>;
@group(1) @binding(1) var s_diffuse: sampler;

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    let base = textureSample(t_diffuse, s_diffuse, in.tex_coords);
    let N = normalize(in.world_normal);
    let V = normalize(camera.position - in.world_pos);

    var color = lighting.ambient_color * lighting.ambient_intensity * base.rgb;

    for (var i = 0u; i < lighting.dir_light_count; i++) {
        let L = normalize(lighting.dir_lights[i].direction);
        let H = normalize(L + V);
        let light = lighting.dir_lights[i].color * lighting.dir_lights[i].intensity;
        color += light * (max(dot(N, L), 0.0) * base.rgb + pow(max(dot(N, H), 0.0), 32.0) * 0.3);
    }

    for (var i = 0u; i < lighting.point_light_count; i++) {
        let to_l = lighting.point_lights[i].position - in.world_pos;
        let atten = pow(saturate(1.0 - length(to_l) / lighting.point_lights[i].range), 2.0);
        let L = normalize(to_l);
        let H = normalize(L + V);
        let light = lighting.point_lights[i].color * lighting.point_lights[i].intensity * atten;
        color += light * (max(dot(N, L), 0.0) * base.rgb + pow(max(dot(N, H), 0.0), 32.0) * 0.3);
    }

    return vec4<f32>(color, base.a);
}
