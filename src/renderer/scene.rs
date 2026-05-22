use super::texture::{GpuTexture, load_from_vfs};
use crate::ecs::{SpriteRenderer, Transform};
use crate::vfs::{Vfs, VfsPath};
use bytemuck::{Pod, Zeroable};
use std::collections::HashMap;
use wgpu::util::DeviceExt;

#[repr(C)]
#[derive(Copy, Clone, Pod, Zeroable)]
pub struct Vertex {
    pub position: [f32; 3],
    pub tex_coords: [f32; 2],
}

impl Vertex {
    const ATTRIBS: [wgpu::VertexAttribute; 2] = wgpu::vertex_attr_array![
        0 => Float32x3,
        1 => Float32x2,
    ];

    pub fn desc() -> wgpu::VertexBufferLayout<'static> {
        wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<Self>() as u64,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: &Self::ATTRIBS,
        }
    }
}

pub struct TexturedQuad {
    pub vertex_buffer: wgpu::Buffer,
    pub index_buffer: wgpu::Buffer,
    pub bind_group: wgpu::BindGroup,
    /// Cached VFS path; used to skip texture reloads when only the transform changed.
    pub texture_path: String,
}

pub struct Scene {
    /// Entity index → GPU sprite resources.
    pub entity_sprites: HashMap<u32, TexturedQuad>,
}

pub struct RenderContext<'a> {
    pub device: &'a wgpu::Device,
    pub queue: &'a wgpu::Queue,
    pub bgl: &'a wgpu::BindGroupLayout,
}

pub struct SpriteUpdateTarget<'a> {
    pub entity_idx: u32,
    pub transform: &'a Transform,
    pub sprite: &'a SpriteRenderer,
}

impl Scene {
    pub fn new() -> Self {
        Scene {
            entity_sprites: HashMap::new(),
        }
    }

    /// Create or update the sprite for `entity_idx`.
    ///
    /// - If the entity already has a quad and the texture path is unchanged,
    ///   only the vertex buffer is updated (cheap `queue.write_buffer`).
    /// - If the texture path changed or the entity is new, GPU resources are
    ///   (re)created.
    pub fn update_sprite(
        &mut self,
        vfs: &dyn Vfs,
        target: SpriteUpdateTarget,
        ctx: RenderContext,
    ) -> anyhow::Result<()> {
        let RenderContext { device, queue, bgl } = ctx;
        let SpriteUpdateTarget {
            entity_idx,
            transform,
            sprite,
        } = target;
        let verts = build_vertices(transform);

        if let Some(quad) = self.entity_sprites.get_mut(&entity_idx) {
            queue.write_buffer(&quad.vertex_buffer, 0, bytemuck::cast_slice(&verts));
            if quad.texture_path != sprite.texture {
                let tex = load_texture(device, queue, vfs, &sprite.texture)?;
                quad.bind_group = make_bind_group(device, bgl, &tex);
                quad.texture_path = sprite.texture.clone();
            }
            return Ok(());
        }

        let tex = load_texture(device, queue, vfs, &sprite.texture)?;
        let indices: [u16; 6] = [0, 1, 2, 0, 2, 3];
        let vertex_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("quad_vb"),
            contents: bytemuck::cast_slice(&verts),
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
        });
        let index_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("quad_ib"),
            contents: bytemuck::cast_slice(&indices),
            usage: wgpu::BufferUsages::INDEX,
        });
        self.entity_sprites.insert(
            entity_idx,
            TexturedQuad {
                vertex_buffer,
                index_buffer,
                bind_group: make_bind_group(device, bgl, &tex),
                texture_path: sprite.texture.clone(),
            },
        );
        Ok(())
    }

    pub fn remove_sprite(&mut self, entity_idx: u32) {
        self.entity_sprites.remove(&entity_idx);
    }
}

// --- helpers -----------------------------------------------------------------

fn build_vertices(transform: &Transform) -> [Vertex; 4] {
    let [x, y, z] = transform.position;
    let [sx, _, sz] = transform.scale;
    let hw = sx * 0.5;
    let hd = sz * 0.5;
    [
        Vertex {
            position: [x - hw, y, z - hd],
            tex_coords: [0.0, 1.0],
        },
        Vertex {
            position: [x + hw, y, z - hd],
            tex_coords: [1.0, 1.0],
        },
        Vertex {
            position: [x + hw, y, z + hd],
            tex_coords: [1.0, 0.0],
        },
        Vertex {
            position: [x - hw, y, z + hd],
            tex_coords: [0.0, 0.0],
        },
    ]
}

fn load_texture(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    vfs: &dyn Vfs,
    texture_path: &str,
) -> anyhow::Result<GpuTexture> {
    let vfs_path = VfsPath::parse(texture_path)
        .ok_or_else(|| anyhow::anyhow!("invalid VFS texture path: {texture_path}"))?;
    load_from_vfs(device, queue, vfs, &vfs_path)
}

fn make_bind_group(
    device: &wgpu::Device,
    bgl: &wgpu::BindGroupLayout,
    texture: &GpuTexture,
) -> wgpu::BindGroup {
    device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("texture_bg"),
        layout: bgl,
        entries: &[
            wgpu::BindGroupEntry {
                binding: 0,
                resource: wgpu::BindingResource::TextureView(&texture.view),
            },
            wgpu::BindGroupEntry {
                binding: 1,
                resource: wgpu::BindingResource::Sampler(&texture.sampler),
            },
        ],
    })
}
