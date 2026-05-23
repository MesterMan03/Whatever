use serde::{Deserialize, Serialize};

pub const COMPONENT_TRANSFORM: &str = "core:transform";
pub const COMPONENT_SPRITE_RENDERER: &str = "core:sprite_renderer";
pub const COMPONENT_TEXT_RENDERER: &str = "core:text_renderer";

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

fn default_font_path() -> String {
    "core://fonts/default.ttf".to_owned()
}
fn default_font_size() -> f32 {
    24.0
}
fn default_color() -> [f32; 4] {
    [1.0, 1.0, 1.0, 1.0]
}
