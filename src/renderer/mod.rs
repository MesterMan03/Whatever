mod camera;
mod context;
mod scene;
mod text;
mod texture;

pub use camera::view_proj_from_entity;
pub use context::WgpuContext;
pub use scene::{RenderContext, Scene, SpriteUpdateTarget, Vertex};
pub use text::GlyphonText;

use crate::vfs::{Vfs, VfsPath};
use anyhow::{Context, anyhow};
use bytemuck::cast_slice;
use glam::Mat4;

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
    pipeline: wgpu::RenderPipeline,
    camera_buffer: wgpu::Buffer,
    camera_bind_group: wgpu::BindGroup,
    egui_renderer: egui_wgpu::Renderer,
    pub text: GlyphonText,
}

impl Renderer {
    pub async fn new(ctx: WgpuContext, vfs: &dyn Vfs) -> anyhow::Result<Self> {
        let shader_path = VfsPath::parse("core://shaders/sprite.wgsl")
            .ok_or_else(|| anyhow!("invalid VFS path for sprite shader"))?;
        let shader_src = vfs.read(&shader_path).context("read sprite shader")?;
        let shader_src = String::from_utf8(shader_src).context("shader utf8")?;

        let shader = ctx
            .device
            .create_shader_module(wgpu::ShaderModuleDescriptor {
                label: Some("sprite_shader"),
                source: wgpu::ShaderSource::Wgsl(shader_src.into()),
            });

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

        let pipeline_layout = ctx
            .device
            .create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("pipeline_layout"),
                bind_group_layouts: &[&camera_bgl, &texture_bgl],
                push_constant_ranges: &[],
            });

        let pipeline = ctx
            .device
            .create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some("sprite_pipeline"),
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
                        format: ctx.config.format,
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

        let aspect = ctx.config.width as f32 / ctx.config.height.max(1) as f32;

        let egui_renderer =
            egui_wgpu::Renderer::new(&ctx.device, ctx.config.format, None, 1, false);

        let text = GlyphonText::new(vfs).context("init text system")?;

        Ok(Renderer {
            ctx,
            aspect,
            scene,
            texture_bind_group_layout: texture_bgl,
            pipeline,
            camera_buffer,
            camera_bind_group,
            egui_renderer,
            text,
        })
    }

    /// Render one frame.
    ///
    /// `camera_vp` — the combined view-projection matrix for this frame.
    ///   * `Some(mat)` — normal render with the provided camera.
    ///   * `None`      — no active camera; clears to black.  The caller is
    ///     expected to add a "no camera" notice via the egui overlay.
    pub fn render(
        &mut self,
        camera_vp: Option<Mat4>,
        egui_output: Option<&EguiOutput>,
    ) -> anyhow::Result<()> {
        // Upload whichever matrix we got (identity if no camera — the scene is
        // invisible anyway since we clear to black and nothing meaningful is drawn).
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

        // Upload egui texture deltas and staging buffers before any render pass
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

        // Clear colour: black when there is no camera, dark-blue otherwise.
        let clear_color = if camera_vp.is_some() {
            wgpu::Color { r: 0.1, g: 0.1, b: 0.15, a: 1.0 }
        } else {
            wgpu::Color { r: 0.0, g: 0.0, b: 0.0, a: 1.0 }
        };

        // Main scene pass — sprites then text quads share the same pipeline
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

            rpass.set_pipeline(&self.pipeline);
            rpass.set_bind_group(0, &self.camera_bind_group, &[]);

            for quad in self.scene.entity_sprites.values() {
                rpass.set_bind_group(1, &quad.bind_group, &[]);
                rpass.set_vertex_buffer(0, quad.vertex_buffer.slice(..));
                rpass.set_index_buffer(quad.index_buffer.slice(..), wgpu::IndexFormat::Uint16);
                rpass.draw_indexed(0..6, 0, 0..1);
            }

            for quad in self.text.quads() {
                rpass.set_bind_group(1, &quad.bind_group, &[]);
                rpass.set_vertex_buffer(0, quad.vertex_buffer.slice(..));
                rpass.set_index_buffer(quad.index_buffer.slice(..), wgpu::IndexFormat::Uint16);
                rpass.draw_indexed(0..6, 0, 0..1);
            }
        }

        // egui overlay pass (LoadOp::Load to draw on top of the scene)
        // forget_lifetime() converts RenderPass<'encoder> → RenderPass<'static> as required by egui-wgpu
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

        // Free released egui textures after submission
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
