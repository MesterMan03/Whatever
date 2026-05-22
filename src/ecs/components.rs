use serde::{Deserialize, Serialize};

pub const COMPONENT_TRANSFORM: &str = "core:transform";
pub const COMPONENT_SPRITE_RENDERER: &str = "core:sprite_renderer";

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
    pub z_index: i32,
}
