mod camera;
mod context;
mod lighting;
mod mesh;
mod scene;
mod text;
mod texture;

pub use camera::view_proj_from_entity;
pub use context::WgpuContext;
pub use lighting::{GpuDirectionalLight, GpuLightingData, GpuPointLight};
pub use scene::{MeshUpdateTarget, RenderContext, Scene, SpriteUpdateTarget, Vertex};
pub use text::GlyphonText;

use crate::vfs::{Vfs, VfsPath};
use anyhow::{Context, anyhow};
use bytemuck::cast_slice;
use glam::{Mat4, Vec3};
use std::collections::{HashMap, HashSet};

const DEPTH_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Depth32Float;

fn create_depth_view(device: &wgpu::Device, width: u32, height: u32) -> wgpu::TextureView {
    device
        .create_texture(&wgpu::TextureDescriptor {
            label: Some("depth_texture"),
            size: wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: DEPTH_FORMAT,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            view_formats: &[],
        })
        .create_view(&Default::default())
}

pub struct EguiOutput {
    pub paint_jobs: Vec<egui::ClippedPrimitive>,
    pub textures_delta: egui::TexturesDelta,
    pub screen_descriptor: egui_wgpu::ScreenDescriptor,
}

pub struct Renderer {
    pub ctx: WgpuContext,
    /// Current surface aspect ratio (width / height).  Updated on resize.
    pub aspect: f32,
    pub scene: Scene,
    pub texture_bind_group_layout: wgpu::BindGroupLayout,
    /// Depth texture view — recreated on resize.
    depth_view: wgpu::TextureView,
    /// Cache of compiled render pipelines keyed by (shader VFS path, back_cull).
    pipeline_cache: HashMap<(String, bool), wgpu::RenderPipeline>,
    /// Keys that failed to compile — errors are logged once and then suppressed.
    pipeline_errors: HashSet<(String, bool)>,
    /// Camera uniform buffer — 80 bytes: 64 bytes view_proj + 16 bytes position+pad.
    camera_buffer: wgpu::Buffer,
    camera_bind_group: wgpu::BindGroup,
    /// Stored so that `get_or_build_pipeline` can construct new pipeline layouts.
    camera_bind_group_layout: wgpu::BindGroupLayout,
    /// Lighting uniform buffer (group 2, binding 0).
    lighting_buffer: wgpu::Buffer,
    lighting_bind_group: wgpu::BindGroup,
    lighting_bind_group_layout: wgpu::BindGroupLayout,
    egui_renderer: egui_wgpu::Renderer,
    pub text: GlyphonText,
}

impl Renderer {
    pub async fn new(ctx: WgpuContext) -> anyhow::Result<Self> {
        // 80 bytes: mat4x4 (64) + vec3 position (12) + f32 pad (4).
        let camera_buffer = ctx.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("camera_buffer"),
            size: 80,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let camera_bgl = ctx
            .device
            .create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("camera_bgl"),
                entries: &[wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::VERTEX | wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                }],
            });

        let camera_bind_group = ctx.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("camera_bg"),
            layout: &camera_bgl,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: camera_buffer.as_entire_binding(),
            }],
        });

        let texture_bgl = ctx
            .device
            .create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("texture_bgl"),
                entries: &[
                    wgpu::BindGroupLayoutEntry {
                        binding: 0,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Texture {
                            sample_type: wgpu::TextureSampleType::Float { filterable: true },
                            view_dimension: wgpu::TextureViewDimension::D2,
                            multisampled: false,
                        },
                        count: None,
                    },
                    wgpu::BindGroupLayoutEntry {
                        binding: 1,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                        count: None,
                    },
                ],
            });

        let lighting_bgl = ctx
            .device
            .create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("lighting_bgl"),
                entries: &[wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT | wgpu::ShaderStages::VERTEX,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                }],
            });

        let lighting_buffer = ctx.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("lighting_buffer"),
            size: size_of::<GpuLightingData>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let lighting_bind_group = ctx.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("lighting_bg"),
            layout: &lighting_bgl,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: lighting_buffer.as_entire_binding(),
            }],
        });

        let scene = Scene::new();
        let aspect = ctx.config.width as f32 / ctx.config.height.max(1) as f32;
        let depth_view = create_depth_view(&ctx.device, ctx.config.width, ctx.config.height);
        let egui_renderer =
            egui_wgpu::Renderer::new(&ctx.device, ctx.config.format, None, 1, false);
        let text = GlyphonText::new();

        Ok(Renderer {
            aspect,
            scene,
            texture_bind_group_layout: texture_bgl,
            depth_view,
            pipeline_cache: HashMap::new(),
            pipeline_errors: HashSet::new(),
            camera_buffer,
            camera_bind_group,
            camera_bind_group_layout: camera_bgl,
            lighting_buffer,
            lighting_bind_group,
            lighting_bind_group_layout: lighting_bgl,
            egui_renderer,
            text,
            ctx,
        })
    }

    /// Return a reference to the compiled pipeline for `shader_path`, building
    /// it on first use.  `back_cull = true` enables back-face culling (use for
    /// 3-D meshes); `false` disables it (use for flat sprites and text).
    /// Returns `None` if the shader has already failed (error was logged once).
    pub fn get_or_build_pipeline(
        &mut self,
        vfs: &dyn Vfs,
        shader_path: &str,
        back_cull: bool,
    ) -> Option<&wgpu::RenderPipeline> {
        let key = (shader_path.to_owned(), back_cull);
        if self.pipeline_errors.contains(&key) {
            return None;
        }
        if !self.pipeline_cache.contains_key(&key) {
            match self.compile_pipeline(vfs, shader_path, back_cull) {
                Ok(pipeline) => {
                    self.pipeline_cache.insert(key.clone(), pipeline);
                }
                Err(e) => {
                    tracing::error!(shader = shader_path, "shader compile error: {e}");
                    self.pipeline_errors.insert(key);
                    return None;
                }
            }
        }
        self.pipeline_cache.get(&key)
    }

    fn compile_pipeline(
        &self,
        vfs: &dyn Vfs,
        shader_path: &str,
        back_cull: bool,
    ) -> anyhow::Result<wgpu::RenderPipeline> {
        let vfs_path = VfsPath::parse(shader_path)
            .ok_or_else(|| anyhow!("invalid VFS path for shader '{shader_path}'"))?;
        let src = vfs.read(&vfs_path).context("read shader source")?;
        let src = String::from_utf8(src).context("shader source is not valid UTF-8")?;

        let shader = self
            .ctx
            .device
            .create_shader_module(wgpu::ShaderModuleDescriptor {
                label: Some(shader_path),
                source: wgpu::ShaderSource::Wgsl(src.into()),
            });

        let pipeline_layout =
            self.ctx
                .device
                .create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                    label: Some("pipeline_layout"),
                    bind_group_layouts: &[
                        &self.camera_bind_group_layout,
                        &self.texture_bind_group_layout,
                        &self.lighting_bind_group_layout,
                    ],
                    push_constant_ranges: &[],
                });

        let pipeline = self
            .ctx
            .device
            .create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some(shader_path),
                layout: Some(&pipeline_layout),
                vertex: wgpu::VertexState {
                    module: &shader,
                    entry_point: Some("vs_main"),
                    buffers: &[Vertex::desc()],
                    compilation_options: Default::default(),
                },
                fragment: Some(wgpu::FragmentState {
                    module: &shader,
                    entry_point: Some("fs_main"),
                    targets: &[Some(wgpu::ColorTargetState {
                        format: self.ctx.config.format,
                        blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                        write_mask: wgpu::ColorWrites::ALL,
                    })],
                    compilation_options: Default::default(),
                }),
                primitive: wgpu::PrimitiveState {
                    topology: wgpu::PrimitiveTopology::TriangleList,
                    cull_mode: if back_cull {
                        Some(wgpu::Face::Back)
                    } else {
                        None
                    },
                    ..Default::default()
                },
                depth_stencil: Some(wgpu::DepthStencilState {
                    format: DEPTH_FORMAT,
                    depth_write_enabled: true,
                    depth_compare: wgpu::CompareFunction::Less,
                    stencil: wgpu::StencilState::default(),
                    bias: wgpu::DepthBiasState::default(),
                }),
                multisample: wgpu::MultisampleState::default(),
                multiview: None,
                cache: None,
            });

        Ok(pipeline)
    }

    /// Render one frame.
    ///
    /// `camera_vp` — the combined view-projection matrix for this frame.
    ///   * `Some(mat)` — normal render with the provided camera.
    ///   * `None`      — no active camera; clears to black.
    pub fn render(
        &mut self,
        camera_vp: Option<Mat4>,
        camera_pos: Option<Vec3>,
        egui_output: Option<&EguiOutput>,
        vfs: &dyn Vfs,
    ) -> anyhow::Result<()> {
        let vp = camera_vp.unwrap_or(Mat4::IDENTITY);
        let pos = camera_pos.unwrap_or(Vec3::ZERO);
        // 80-byte camera uniform: [view_proj (64 bytes)] + [position (12 bytes)] + [pad (4 bytes)]
        let mut cam_data = [0u8; 80];
        cam_data[..64].copy_from_slice(cast_slice(&vp.to_cols_array()));
        cam_data[64..76].copy_from_slice(cast_slice(&pos.to_array()));
        self.ctx
            .queue
            .write_buffer(&self.camera_buffer, 0, &cam_data);

        let output = match self.ctx.surface.get_current_texture() {
            Ok(t) => t,
            Err(wgpu::SurfaceError::Lost | wgpu::SurfaceError::Outdated) => return Ok(()),
            Err(e) => return Err(e.into()),
        };
        let view = output.texture.create_view(&Default::default());
        let mut encoder = self
            .ctx
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("render_encoder"),
            });

        // Upload egui texture deltas before any render pass.
        let egui_staging = if let Some(egui) = egui_output {
            for (id, delta) in &egui.textures_delta.set {
                self.egui_renderer
                    .update_texture(&self.ctx.device, &self.ctx.queue, *id, delta);
            }
            self.egui_renderer.update_buffers(
                &self.ctx.device,
                &self.ctx.queue,
                &mut encoder,
                &egui.paint_jobs,
                &egui.screen_descriptor,
            )
        } else {
            Vec::new()
        };

        let clear_color = if camera_vp.is_some() {
            wgpu::Color {
                r: 0.1,
                g: 0.1,
                b: 0.15,
                a: 1.0,
            }
        } else {
            wgpu::Color {
                r: 0.0,
                g: 0.0,
                b: 0.0,
                a: 1.0,
            }
        };

        // Pre-warm any pipelines that are not yet compiled.  This must happen
        // outside the render pass (pipelines cannot be built mid-pass).
        // Collect (path, back_cull) pairs for every drawable that needs a pipeline.
        let mut pipeline_keys: Vec<(String, bool)> = self
            .scene
            .entity_sprites
            .values()
            .map(|q| (q.shader_path.clone(), false))
            .chain(self.text.quads().map(|q| (q.shader_path.clone(), false)))
            .chain(
                self.scene
                    .entity_meshes
                    .values()
                    .map(|d| (d.shader_path.clone(), true)),
            )
            .collect();
        pipeline_keys.sort_unstable();
        pipeline_keys.dedup();
        for (path, back_cull) in &pipeline_keys {
            self.get_or_build_pipeline(vfs, path, *back_cull);
        }

        // Main scene pass — sprites, meshes, and text share the same pass.
        {
            let mut rpass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("main_pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(clear_color),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: &self.depth_view,
                    depth_ops: Some(wgpu::Operations {
                        load: wgpu::LoadOp::Clear(1.0),
                        store: wgpu::StoreOp::Store,
                    }),
                    stencil_ops: None,
                }),
                timestamp_writes: None,
                occlusion_query_set: None,
            });

            rpass.set_bind_group(0, &self.camera_bind_group, &[]);
            rpass.set_bind_group(2, &self.lighting_bind_group, &[]);

            // --- Sprites grouped by shader ------------------------------------
            // Collect (shader_path, entity_idx) pairs so we can sort by shader.
            let mut sprite_by_shader: Vec<(&str, u32)> = self
                .scene
                .entity_sprites
                .iter()
                .map(|(&idx, q)| (q.shader_path.as_str(), idx))
                .collect();
            sprite_by_shader.sort_unstable_by_key(|(s, _)| *s);

            let mut current_shader: Option<&str> = None;
            for (shader, idx) in &sprite_by_shader {
                if current_shader != Some(shader) {
                    match self.pipeline_cache.get(&(shader.to_string(), false)) {
                        Some(pipeline) => {
                            rpass.set_pipeline(pipeline);
                            current_shader = Some(shader);
                        }
                        None => {
                            current_shader = None;
                            continue;
                        }
                    }
                }
                if current_shader.is_none() {
                    continue;
                }
                if let Some(quad) = self.scene.entity_sprites.get(idx) {
                    rpass.set_bind_group(1, &quad.bind_group, &[]);
                    rpass.set_vertex_buffer(0, quad.vertex_buffer.slice(..));
                    rpass.set_index_buffer(quad.index_buffer.slice(..), wgpu::IndexFormat::Uint16);
                    rpass.draw_indexed(0..6, 0, 0..1);
                }
            }

            // --- Meshes grouped by shader -------------------------------------
            let mut mesh_by_shader: Vec<(&str, u32)> = self
                .scene
                .entity_meshes
                .iter()
                .map(|(&idx, d)| (d.shader_path.as_str(), idx))
                .collect();
            mesh_by_shader.sort_unstable_by_key(|(s, _)| *s);

            current_shader = None;
            for (shader, idx) in &mesh_by_shader {
                if current_shader != Some(shader) {
                    match self.pipeline_cache.get(&(shader.to_string(), true)) {
                        Some(pipeline) => {
                            rpass.set_pipeline(pipeline);
                            current_shader = Some(shader);
                        }
                        None => {
                            current_shader = None;
                            continue;
                        }
                    }
                }
                if current_shader.is_none() {
                    continue;
                }
                if let Some(drawable) = self.scene.entity_meshes.get(idx) {
                    rpass.set_bind_group(1, &drawable.bind_group, &[]);
                    rpass.set_vertex_buffer(0, drawable.vertex_buffer.slice(..));
                    rpass.set_index_buffer(
                        drawable.index_buffer.slice(..),
                        wgpu::IndexFormat::Uint16,
                    );
                    rpass.draw_indexed(0..drawable.index_count, 0, 0..1);
                }
            }

            // --- Text quads grouped by shader (no back-cull) ------------------
            let mut text_by_shader: Vec<(&str, &scene::TexturedQuad)> = self
                .text
                .quads()
                .map(|q| (q.shader_path.as_str(), q))
                .collect();
            text_by_shader.sort_unstable_by_key(|(s, _)| *s);

            let mut current_text_shader: Option<&str> = None;
            for (shader, quad) in &text_by_shader {
                if current_text_shader != Some(shader) {
                    match self.pipeline_cache.get(&(shader.to_string(), false)) {
                        Some(pipeline) => {
                            rpass.set_pipeline(pipeline);
                            current_text_shader = Some(shader);
                        }
                        None => {
                            current_text_shader = None;
                            continue;
                        }
                    }
                }
                if current_text_shader.is_none() {
                    continue;
                }
                rpass.set_bind_group(1, &quad.bind_group, &[]);
                rpass.set_vertex_buffer(0, quad.vertex_buffer.slice(..));
                rpass.set_index_buffer(quad.index_buffer.slice(..), wgpu::IndexFormat::Uint16);
                rpass.draw_indexed(0..6, 0, 0..1);
            }
        }

        // egui overlay pass.
        if let Some(egui) = egui_output {
            let mut rpass = encoder
                .begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: Some("egui_pass"),
                    color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                        view: &view,
                        resolve_target: None,
                        ops: wgpu::Operations {
                            load: wgpu::LoadOp::Load,
                            store: wgpu::StoreOp::Store,
                        },
                    })],
                    depth_stencil_attachment: None,
                    timestamp_writes: None,
                    occlusion_query_set: None,
                })
                .forget_lifetime();
            self.egui_renderer
                .render(&mut rpass, &egui.paint_jobs, &egui.screen_descriptor);
        }

        let main_cmd = encoder.finish();
        self.ctx
            .queue
            .submit(egui_staging.into_iter().chain(std::iter::once(main_cmd)));

        if let Some(egui) = egui_output {
            for id in &egui.textures_delta.free {
                self.egui_renderer.free_texture(id);
            }
        }

        output.present();
        Ok(())
    }

    pub fn update_lighting(&mut self, data: &GpuLightingData) {
        self.ctx
            .queue
            .write_buffer(&self.lighting_buffer, 0, bytemuck::bytes_of(data));
    }

    pub fn resize(&mut self, width: u32, height: u32) {
        self.ctx.resize(width, height);
        self.aspect = width as f32 / height.max(1) as f32;
        self.depth_view = create_depth_view(&self.ctx.device, width, height);
    }
}
