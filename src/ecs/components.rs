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
