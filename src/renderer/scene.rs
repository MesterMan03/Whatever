use super::mesh::{CpuMesh, load_mesh_from_vfs};
use super::texture::{GpuTexture, create_from_pixels, load_from_vfs};
use crate::ecs::{MeshRenderer, SpriteRenderer, Transform};
use crate::vfs::{Vfs, VfsPath};
use bytemuck::{Pod, Zeroable};
use glam::{Quat, Vec3};
use std::collections::HashMap;
use wgpu::util::DeviceExt;

#[repr(C)]
#[derive(Copy, Clone, Pod, Zeroable)]
pub struct Vertex {
    pub position: [f32; 3],
    pub tex_coords: [f32; 2],
    pub normal: [f32; 3],
}

impl Vertex {
    const ATTRIBS: [wgpu::VertexAttribute; 3] = wgpu::vertex_attr_array![
        0 => Float32x3,
        1 => Float32x2,
        2 => Float32x3,
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
    /// Shader VFS path; used to look up the right pipeline at draw time.
    pub shader_path: String,
}

pub struct MeshDrawable {
    pub vertex_buffer: wgpu::Buffer,
    pub index_buffer: wgpu::Buffer,
    pub index_count: u32,
    pub bind_group: wgpu::BindGroup,
    /// `None` means the fallback 1×1 white texture was used.
    pub texture_path: Option<String>,
    pub mesh_path: String,
    pub shader_path: String,
}

pub struct Scene {
    /// Entity index → GPU sprite resources.
    pub entity_sprites: HashMap<u32, TexturedQuad>,
    /// Entity index → GPU mesh resources.
    pub entity_meshes: HashMap<u32, MeshDrawable>,
    /// VFS path → parsed CPU-side mesh (avoids re-parsing on every transform change).
    mesh_cpu_cache: HashMap<String, CpuMesh>,
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

pub struct MeshUpdateTarget<'a> {
    pub entity_idx: u32,
    pub transform: &'a Transform,
    pub mesh_renderer: &'a MeshRenderer,
}

impl Scene {
    pub fn new() -> Self {
        Scene {
            entity_sprites: HashMap::new(),
            entity_meshes: HashMap::new(),
            mesh_cpu_cache: HashMap::new(),
        }
    }

    /// Create or update the sprite for `entity_idx`.
    ///
    /// - If the entity already has a quad and the texture / shader paths are
    ///   unchanged, only the vertex buffer is updated (cheap `queue.write_buffer`).
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
            quad.shader_path = sprite.shader.clone();
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
                shader_path: sprite.shader.clone(),
            },
        );
        Ok(())
    }

    pub fn remove_sprite(&mut self, entity_idx: u32) {
        self.entity_sprites.remove(&entity_idx);
    }

    /// Create or update the mesh drawable for `entity_idx`.
    ///
    /// The CPU mesh is cached by VFS path.  On a transform-only change the
    /// cached vertices are re-transformed and written to the existing buffer.
    pub fn update_mesh(
        &mut self,
        vfs: &dyn Vfs,
        target: MeshUpdateTarget,
        ctx: RenderContext,
    ) -> anyhow::Result<()> {
        let RenderContext { device, queue, bgl } = ctx;
        let MeshUpdateTarget {
            entity_idx,
            transform,
            mesh_renderer,
        } = target;

        let mesh_changed = self
            .entity_meshes
            .get(&entity_idx)
            .map_or(true, |d| d.mesh_path != mesh_renderer.mesh);

        let texture_changed = self.entity_meshes.get(&entity_idx).map_or(true, |d| {
            d.texture_path.as_deref() != mesh_renderer.texture.as_deref()
        });

        // Ensure the CPU mesh is cached.
        if mesh_changed || !self.mesh_cpu_cache.contains_key(&mesh_renderer.mesh) {
            let cpu = load_mesh_from_vfs(vfs, &mesh_renderer.mesh)
                .map_err(|e| anyhow::anyhow!("load mesh '{}': {e}", mesh_renderer.mesh))?;
            self.mesh_cpu_cache.insert(mesh_renderer.mesh.clone(), cpu);
        }

        let cpu = self
            .mesh_cpu_cache
            .get(&mesh_renderer.mesh)
            .expect("just inserted");

        // Apply transform to every vertex on the CPU (position + normal).
        let transformed: Vec<Vertex> = cpu
            .vertices
            .iter()
            .map(|v| Vertex {
                position: apply_transform(v.position, transform),
                tex_coords: v.tex_coords,
                normal: apply_transform_normal(v.normal, transform),
            })
            .collect();

        // Fast path: only transform changed — reuse buffers and bind group.
        if !mesh_changed && !texture_changed {
            if let Some(drawable) = self.entity_meshes.get_mut(&entity_idx) {
                queue.write_buffer(
                    &drawable.vertex_buffer,
                    0,
                    bytemuck::cast_slice(&transformed),
                );
                drawable.shader_path = mesh_renderer.shader.clone();
                return Ok(());
            }
        }

        // Full (re)build.
        let vertex_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("mesh_vb"),
            contents: bytemuck::cast_slice(&transformed),
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
        });
        let index_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("mesh_ib"),
            contents: bytemuck::cast_slice(&cpu.indices),
            usage: wgpu::BufferUsages::INDEX,
        });
        let index_count = cpu.indices.len() as u32;

        let (bind_group, texture_path) = match &mesh_renderer.texture {
            Some(tex_path) => {
                let tex = load_texture(device, queue, vfs, tex_path)?;
                (make_bind_group(device, bgl, &tex), Some(tex_path.clone()))
            }
            None => {
                // Clone the bind group reference — we store a fresh bind group
                // built from the fallback texture that was created at renderer init.
                // Since we can't clone wgpu::BindGroup, we keep using the renderer's
                // fallback directly; store None in texture_path as a sentinel.
                //
                // Build a 1×1 opaque-white fallback texture inline.
                let white = create_from_pixels(device, queue, &[255, 255, 255, 255], 1, 1);
                (make_bind_group(device, bgl, &white), None)
            }
        };

        self.entity_meshes.insert(
            entity_idx,
            MeshDrawable {
                vertex_buffer,
                index_buffer,
                index_count,
                bind_group,
                texture_path,
                mesh_path: mesh_renderer.mesh.clone(),
                shader_path: mesh_renderer.shader.clone(),
            },
        );
        Ok(())
    }

    pub fn remove_mesh(&mut self, entity_idx: u32) {
        self.entity_meshes.remove(&entity_idx);
    }
}

// --- helpers -----------------------------------------------------------------

/// Build the four vertices of a textured quad from a `Transform`.
fn build_vertices(transform: &Transform) -> [Vertex; 4] {
    let origin = Vec3::from(transform.position);
    let [sx, _, sz] = transform.scale;
    let hw = sx * 0.5;
    let hd = sz * 0.5;
    let [qx, qy, qz, qw] = transform.rotation;
    let rot = Quat::from_xyzw(qx, qy, qz, qw);

    // The sprite lies in the XZ plane; its face normal is +Z in local space.
    let baked_normal = (rot * Vec3::Z).to_array();

    let corners = [
        Vec3::new(-hw, 0.0, -hd),
        Vec3::new(hw, 0.0, -hd),
        Vec3::new(hw, 0.0, hd),
        Vec3::new(-hw, 0.0, hd),
    ]
    .map(|c| (rot * c + origin).to_array());

    [
        Vertex {
            position: corners[0],
            tex_coords: [0.0, 1.0],
            normal: baked_normal,
        },
        Vertex {
            position: corners[1],
            tex_coords: [1.0, 1.0],
            normal: baked_normal,
        },
        Vertex {
            position: corners[2],
            tex_coords: [1.0, 0.0],
            normal: baked_normal,
        },
        Vertex {
            position: corners[3],
            tex_coords: [0.0, 0.0],
            normal: baked_normal,
        },
    ]
}

/// Apply a `Transform` (position + rotation + scale) to a model-space vertex position.
fn apply_transform(pos: [f32; 3], transform: &Transform) -> [f32; 3] {
    let v = Vec3::from(pos);
    let scale = Vec3::from(transform.scale);
    let [qx, qy, qz, qw] = transform.rotation;
    let rot = Quat::from_xyzw(qx, qy, qz, qw);
    let origin = Vec3::from(transform.position);
    (rot * (scale * v) + origin).to_array()
}

/// Apply a `Transform` to a model-space surface normal.
///
/// The normal matrix is `R × S⁻¹` — this correctly handles non-uniform scale
/// so normals remain perpendicular to their surfaces after stretching.
fn apply_transform_normal(normal: [f32; 3], transform: &Transform) -> [f32; 3] {
    let n = Vec3::from(normal);
    let scale = Vec3::from(transform.scale);
    let [qx, qy, qz, qw] = transform.rotation;
    let rot = Quat::from_xyzw(qx, qy, qz, qw);
    (rot * (n / scale)).normalize_or_zero().to_array()
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
