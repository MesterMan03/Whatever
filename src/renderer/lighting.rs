use bytemuck::{Pod, Zeroable};

#[repr(C)]
#[derive(Copy, Clone, Pod, Zeroable)]
pub struct GpuDirectionalLight {
    pub direction: [f32; 3],
    pub _pad0: f32,
    pub color: [f32; 3],
    pub intensity: f32,
}

#[repr(C)]
#[derive(Copy, Clone, Pod, Zeroable)]
pub struct GpuPointLight {
    pub position: [f32; 3],
    pub range: f32,
    pub color: [f32; 3],
    pub intensity: f32,
}

#[repr(C)]
#[derive(Copy, Clone, Pod, Zeroable)]
pub struct GpuLightingData {
    pub ambient_color: [f32; 3],
    pub ambient_intensity: f32,
    pub dir_light_count: u32,
    pub point_light_count: u32,
    _pad: [u32; 2],
    pub dir_lights: [GpuDirectionalLight; 4],
    pub point_lights: [GpuPointLight; 8],
}
