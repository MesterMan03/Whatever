mod camera;
mod context;
mod lighting;
mod mesh;
mod scene;
mod text;
mod texture;
mod world;

pub use world::{apply_render_command, collect_lights, sync_render_transforms};

pub use camera::view_proj_from_entity;
pub use context::WgpuContext;
pub use lighting::{GpuDirectionalLight, GpuLightingData, GpuPointLight};
pub use mesh::{CpuMesh, load_mesh_from_vfs};
pub use scene::{MeshUpdateTarget, RenderContext, Scene, SpriteUpdateTarget, Vertex};
pub use text::GlyphonText;

mod render;
pub use render::{EguiOutput, Renderer};
