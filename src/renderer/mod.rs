mod camera;
mod context;
mod mesh;
mod scene;
mod text;
mod texture;

pub use camera::view_proj_from_entity;
pub use context::WgpuContext;
pub use scene::{MeshUpdateTarget, RenderContext, Scene, SpriteUpdateTarget, Vertex};
pub use text::GlyphonText;

use crate::vfs::{Vfs, VfsPath};
use anyhow::{Context, anyhow};
use bytemuck::cast_slice;
use glam::Mat4;
use std::collections::{HashMap, HashSet};

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
    /// Cache of compiled render pipelines keyed by shader VFS path.
    pipeline_cache: HashMap<String, wgpu::RenderPipeline>,
    /// Shader paths that failed to compile — errors are logged once and then
    /// suppressed to avoid per-frame spam.
    pipeline_errors: HashSet<String>,
    camera_buffer: wgpu::Buffer,
    camera_bind_group: wgpu::BindGroup,
    /// Stored so that `get_or_build_pipeline` can construct new pipeline layouts.
    camera_bind_group_layout: wgpu::BindGroupLayout,
    egui_renderer: egui_wgpu::Renderer,
    pub text: GlyphonText,
}

impl Renderer {
    pub async fn new(ctx: WgpuContext, vfs: &dyn Vfs) -> anyhow::Result<Self> {
        let camera_buffer = ctx.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("camera_buffer"),
            size: size_of::<[f32; 16]>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let camera_bgl = ctx
            .device
            .create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("camera_bgl"),
                entries: &[wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::VERTEX,
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

        let scene = Scene::new();
        let aspect = ctx.config.width as f32 / ctx.config.height.max(1) as f32;
        let egui_renderer =
            egui_wgpu::Renderer::new(&ctx.device, ctx.config.format, None, 1, false);
        let text = GlyphonText::new(vfs).context("init text system")?;

        Ok(Renderer {
            aspect,
            scene,
            texture_bind_group_layout: texture_bgl,
            pipeline_cache: HashMap::new(),
            pipeline_errors: HashSet::new(),
            camera_buffer,
            camera_bind_group,
            camera_bind_group_layout: camera_bgl,
            egui_renderer,
            text,
            ctx,
        })
    }

    /// Return a reference to the compiled pipeline for `shader_path`, building
    /// it on first use.  Returns `None` if the shader has already failed
    /// (error was logged at compile time) or if compilation fails now.
    pub fn get_or_build_pipeline(
        &mut self,
        vfs: &dyn Vfs,
        shader_path: &str,
    ) -> Option<&wgpu::RenderPipeline> {
        if self.pipeline_errors.contains(shader_path) {
            return None;
        }
        if !self.pipeline_cache.contains_key(shader_path) {
            match self.compile_pipeline(vfs, shader_path) {
                Ok(pipeline) => {
                    self.pipeline_cache.insert(shader_path.to_owned(), pipeline);
                }
                Err(e) => {
                    tracing::error!(shader = shader_path, "shader compile error: {e}");
                    self.pipeline_errors.insert(shader_path.to_owned());
                    return None;
                }
            }
        }
        self.pipeline_cache.get(shader_path)
    }

    fn compile_pipeline(
        &self,
        vfs: &dyn Vfs,
        shader_path: &str,
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
                    ],
                    push_constant_ranges: &[],
                });

        let pipeline =
            self.ctx
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
                        cull_mode: None,
                        ..Default::default()
                    },
                    depth_stencil: None,
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
        egui_output: Option<&EguiOutput>,
        vfs: &dyn Vfs,
    ) -> anyhow::Result<()> {
        let vp = camera_vp.unwrap_or(Mat4::IDENTITY);
        let vp_arr: [f32; 16] = vp.to_cols_array();
        self.ctx
            .queue
            .write_buffer(&self.camera_buffer, 0, cast_slice(&vp_arr));

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
            wgpu::Color { r: 0.1, g: 0.1, b: 0.15, a: 1.0 }
        } else {
            wgpu::Color { r: 0.0, g: 0.0, b: 0.0, a: 1.0 }
        };

        // Pre-warm any pipelines that are not yet compiled.  This must happen
        // outside the render pass (pipelines cannot be built mid-pass).
        const TEXT_SHADER: &str = "core://shaders/sprite.wgsl";
        let sprite_shaders: Vec<String> = self
            .scene
            .entity_sprites
            .values()
            .map(|q| q.shader_path.clone())
            .chain(std::iter::once(TEXT_SHADER.to_owned()))
            .collect();
        let mesh_shaders: Vec<String> = self
            .scene
            .entity_meshes
            .values()
            .map(|d| d.shader_path.clone())
            .collect();
        for path in sprite_shaders.iter().chain(mesh_shaders.iter()) {
            if !self.pipeline_cache.contains_key(path.as_str())
                && !self.pipeline_errors.contains(path.as_str())
            {
                self.get_or_build_pipeline(vfs, path);
            }
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
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
            });

            rpass.set_bind_group(0, &self.camera_bind_group, &[]);

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
                    match self.pipeline_cache.get(*shader) {
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
                    rpass.set_index_buffer(
                        quad.index_buffer.slice(..),
                        wgpu::IndexFormat::Uint16,
                    );
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
                    match self.pipeline_cache.get(*shader) {
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

            // --- Text quads (always use the sprite shader) --------------------
            if let Some(pipeline) = self.pipeline_cache.get(TEXT_SHADER) {
                rpass.set_pipeline(pipeline);
                for quad in self.text.quads() {
                    rpass.set_bind_group(1, &quad.bind_group, &[]);
                    rpass.set_vertex_buffer(0, quad.vertex_buffer.slice(..));
                    rpass.set_index_buffer(
                        quad.index_buffer.slice(..),
                        wgpu::IndexFormat::Uint16,
                    );
                    rpass.draw_indexed(0..6, 0, 0..1);
                }
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

    pub fn resize(&mut self, width: u32, height: u32) {
        self.ctx.resize(width, height);
        self.aspect = width as f32 / height.max(1) as f32;
    }
}
