use serde::{Deserialize, Serialize};

pub const COMPONENT_TRANSFORM: &str = "core:transform";
pub const COMPONENT_SPRITE_RENDERER: &str = "core:sprite_renderer";
pub const COMPONENT_TEXT_RENDERER: &str = "core:text_renderer";
pub const COMPONENT_CAMERA: &str = "core:camera";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Transform {
    pub position: [f32; 3],
    /// Quaternion in xyzw order.
    pub rotation: [f32; 4],
    pub scale: [f32; 3],
}

impl Default for Transform {
    fn default() -> Self {
        Transform {
            position: [0.0, 0.0, 0.0],
            rotation: [0.0, 0.0, 0.0, 1.0],
            scale: [1.0, 1.0, 1.0],
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SpriteRenderer {
    /// VFS path to the texture, e.g. `"my_mod://textures/player.png"`.
    pub texture: String,
    /// VFS path to a WGSL shader that satisfies the engine shader contract.
    /// Defaults to `"core://shaders/sprite.wgsl"`.
    #[serde(default = "default_sprite_shader")]
    pub shader: String,
}

fn default_sprite_shader() -> String {
    "core://shaders/sprite.wgsl".to_owned()
}

pub const COMPONENT_MESH_RENDERER: &str = "core:mesh_renderer";

/// Renders arbitrary geometry loaded from a mesh file.
///
/// The mesh file format is detected from the file extension:
/// - `.json` — `{"vertices":[[x,y,z,u,v],...], "indices":[...]}`
/// - `.obj`  — Wavefront OBJ (materials are ignored)
/// - `.glb` / `.gltf` — glTF 2.0 (first mesh/primitive; materials and animations ignored;
///   only GLB and self-contained GLTF with embedded buffers are supported)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MeshRenderer {
    /// VFS path to the mesh file.
    pub mesh: String,
    /// VFS path to a WGSL shader that satisfies the engine shader contract.
    pub shader: String,
    /// Optional VFS path to a texture.  When absent the engine binds a 1×1 white fallback.
    #[serde(default)]
    pub texture: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TextRenderer {
    pub text: String,
    /// VFS path to a TTF/OTF font. Defaults to `"core://fonts/default.ttf"`.
    #[serde(default = "default_font_path")]
    pub font: String,
    #[serde(default = "default_font_size")]
    pub font_size: f32,
    /// RGBA colour, each channel in `[0.0, 1.0]`. Defaults to opaque white.
    #[serde(default = "default_color")]
    pub color: [f32; 4],
}

/// Camera component.  The entity whose ID is passed to `Engine.setMainCamera`
/// is used as the scene camera.  The camera's position and orientation are
/// read from the entity's `core:transform` each frame.
///
/// `fovy_degrees` — vertical field-of-view in degrees (default 45).
/// `znear` / `zfar` — near/far clip planes (defaults 0.1 / 1000.0).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CameraComponent {
    #[serde(default = "default_fovy_degrees")]
    pub fovy_degrees: f32,
    #[serde(default = "default_znear")]
    pub znear: f32,
    #[serde(default = "default_zfar")]
    pub zfar: f32,
}

impl Default for CameraComponent {
    fn default() -> Self {
        CameraComponent {
            fovy_degrees: default_fovy_degrees(),
            znear: default_znear(),
            zfar: default_zfar(),
        }
    }
}

fn default_fovy_degrees() -> f32 {
    45.0
}
fn default_znear() -> f32 {
    0.1
}
fn default_zfar() -> f32 {
    1000.0
}

fn default_font_path() -> String {
    "core://fonts/default.ttf".to_owned()
}
fn default_font_size() -> f32 {
    24.0
}
fn default_color() -> [f32; 4] {
    [1.0, 1.0, 1.0, 1.0]
}
