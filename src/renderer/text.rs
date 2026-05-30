use super::scene::{TexturedQuad, Vertex};
use super::texture::create_from_pixels;
use crate::ecs::{TextRenderer as TextRendererComp, Transform};
use crate::renderer::RenderContext;
use crate::vfs::{Vfs, VfsPath};
use anyhow::Context;
use glam::{Quat, Vec3};
use glyphon::{Attrs, Buffer, Family, FontSystem, Metrics, Shaping, SwashCache, fontdb};
use std::collections::HashMap;
use wgpu::util::DeviceExt;

/// How many rasterised pixels equal one world unit.
const PIXELS_PER_UNIT: f32 = 64.0;

struct TextEntry {
    quad: TexturedQuad,
    world_w: f32,
    world_h: f32,
    // cached component values — used to skip re-rasterisation on transform-only changes
    cached_text: String,
    cached_font: String,
    cached_font_size: f32,
    cached_color: [u32; 4], // stored as u8-cast bits to avoid f32 == pitfalls
}

pub struct GlyphonText {
    pub font_system: FontSystem,
    swash_cache: SwashCache,
    /// VFS font path → family name registered in fontdb.
    font_cache: HashMap<String, String>,
    entity_texts: HashMap<u32, TextEntry>,
}

impl GlyphonText {
    pub fn new() -> Self {
        GlyphonText {
            font_system: FontSystem::new_with_locale_and_db(
                "en-US".into(),
                fontdb::Database::new(),
            ),
            swash_cache: SwashCache::new(),
            font_cache: HashMap::new(),
            entity_texts: HashMap::new(),
        }
    }

    fn load_font_if_needed(&mut self, vfs: &dyn Vfs, font_path: &str) -> anyhow::Result<String> {
        if let Some(name) = self.font_cache.get(font_path) {
            return Ok(name.clone());
        }
        let vfs_path = VfsPath::parse(font_path)
            .ok_or_else(|| anyhow::anyhow!("invalid font VFS path: {font_path}"))?;
        let bytes = vfs
            .read(&vfs_path)
            .with_context(|| format!("reading font {font_path}"))?;

        let before: std::collections::HashSet<fontdb::ID> =
            self.font_system.db().faces().map(|f| f.id).collect();
        self.font_system.db_mut().load_font_data(bytes);

        let family_name = self
            .font_system
            .db()
            .faces()
            .find(|f| !before.contains(&f.id))
            .and_then(|f| f.families.first().map(|(n, _)| n.clone()))
            .unwrap_or_else(|| font_path.to_owned());

        self.font_cache
            .insert(font_path.to_owned(), family_name.clone());
        Ok(family_name)
    }

    pub fn upsert_text(
        &mut self,
        ctx: RenderContext,
        vfs: &dyn Vfs,
        entity_idx: u32,
        transform: &Transform,
        comp: &TextRendererComp,
    ) -> anyhow::Result<()> {
        let RenderContext { device, queue, bgl } = ctx;
        let color_bits = color_bits(comp.color);

        // If only the transform (or shader) changed, skip re-rasterisation.
        if let Some(entry) = self.entity_texts.get_mut(&entity_idx)
            && entry.cached_text == comp.text
            && entry.cached_font == comp.font
            && entry.cached_font_size == comp.font_size
            && entry.cached_color == color_bits
        {
            entry.quad.shader_path = comp.shader.clone();
            let verts = build_text_vertices(transform, entry.world_w, entry.world_h);
            queue.write_buffer(&entry.quad.vertex_buffer, 0, bytemuck::cast_slice(&verts));
            return Ok(());
        }

        // Full re-rasterise.
        let family_name = self.load_font_if_needed(vfs, &comp.font)?;

        let metrics = Metrics::new(comp.font_size, comp.font_size * 1.2);
        let mut buffer = Buffer::new(&mut self.font_system, metrics);
        buffer.set_size(&mut self.font_system, None, None);
        buffer.set_text(
            &mut self.font_system,
            &comp.text,
            Attrs::new().family(Family::Name(&family_name)),
            Shaping::Advanced,
        );
        buffer.shape_until_scroll(&mut self.font_system, false);

        let (pixels, bitmap_w, bitmap_h) = rasterize_buffer(
            &buffer,
            &mut self.font_system,
            &mut self.swash_cache,
            comp.color,
        );

        if bitmap_w == 0 || bitmap_h == 0 {
            // Nothing to draw; remove any existing quad silently.
            self.entity_texts.remove(&entity_idx);
            return Ok(());
        }

        let world_w = bitmap_w as f32 / PIXELS_PER_UNIT;
        let world_h = bitmap_h as f32 / PIXELS_PER_UNIT;
        let verts = build_text_vertices(transform, world_w, world_h);
        let indices: [u16; 6] = [0, 1, 2, 0, 2, 3];

        let vertex_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("text_quad_vb"),
            contents: bytemuck::cast_slice(&verts),
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
        });
        let index_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("text_quad_ib"),
            contents: bytemuck::cast_slice(&indices),
            usage: wgpu::BufferUsages::INDEX,
        });

        let tex = create_from_pixels(device, queue, &pixels, bitmap_w, bitmap_h);
        let bind_group = make_bind_group(device, bgl, &tex);

        self.entity_texts.insert(
            entity_idx,
            TextEntry {
                quad: TexturedQuad {
                    vertex_buffer,
                    index_buffer,
                    bind_group,
                    texture_path: String::new(), // not VFS-backed
                    shader_path: comp.shader.clone(),
                },
                world_w,
                world_h,
                cached_text: comp.text.clone(),
                cached_font: comp.font.clone(),
                cached_font_size: comp.font_size,
                cached_color: color_bits,
            },
        );
        Ok(())
    }

    pub fn remove_text(&mut self, entity_idx: u32) {
        self.entity_texts.remove(&entity_idx);
    }

    /// Iterate quads for the caller to draw via the sprite pipeline.
    pub fn quads(&self) -> impl Iterator<Item = &TexturedQuad> {
        self.entity_texts.values().map(|e| &e.quad)
    }
}

// --- helpers ------------------------------------------------------------------

fn color_bits(color: [f32; 4]) -> [u32; 4] {
    color.map(|c| c.to_bits())
}

fn build_text_vertices(transform: &Transform, world_w: f32, world_h: f32) -> [Vertex; 4] {
    let origin = Vec3::from(transform.position);
    let [sx, _, sz] = transform.scale;
    let hw = world_w * sx * 0.5;
    let hd = world_h * sz * 0.5;
    let [qx, qy, qz, qw] = transform.rotation;
    let rot = Quat::from_xyzw(qx, qy, qz, qw);

    let corners = [
        Vec3::new(-hw, 0.0, -hd),
        Vec3::new(hw, 0.0, -hd),
        Vec3::new(hw, 0.0, hd),
        Vec3::new(-hw, 0.0, hd),
    ]
    .map(|c| (rot * c + origin).to_array());

    let baked_normal = (rot * glam::Vec3::Z).to_array();
    [
        Vertex { position: corners[0], tex_coords: [0.0, 0.0], normal: baked_normal },
        Vertex { position: corners[1], tex_coords: [1.0, 0.0], normal: baked_normal },
        Vertex { position: corners[2], tex_coords: [1.0, 1.0], normal: baked_normal },
        Vertex { position: corners[3], tex_coords: [0.0, 1.0], normal: baked_normal },
    ]
}

/// Rasterise `buffer` to a straight-alpha RGBA bitmap.
///
/// Returns `(pixels, width, height)`. Returns an empty vec if the text is blank.
fn rasterize_buffer(
    buffer: &Buffer,
    font_system: &mut FontSystem,
    swash_cache: &mut SwashCache,
    color: [f32; 4],
) -> (Vec<u8>, u32, u32) {
    // --- Pass 1: collect glyph layout positions without holding image borrows ---
    struct GlyphSpot {
        draw_x: i32,
        draw_y: i32,
        img_w: u32,
        img_h: u32,
        data: Vec<u8>,
        is_color: bool, // true = RGBA (emoji), false = mask (normal text)
    }

    let mut spots: Vec<GlyphSpot> = Vec::new();
    let mut max_x = 0i32;
    let mut max_y = 0i32;

    for run in buffer.layout_runs() {
        for glyph in run.glyphs.iter() {
            let physical = glyph.physical((0.0, 0.0), 1.0);
            if let Some(image) = swash_cache.get_image(font_system, physical.cache_key) {
                let w = image.placement.width;
                let h = image.placement.height;
                if w == 0 || h == 0 {
                    continue;
                }
                let dx = physical.x + image.placement.left;
                let dy = run.line_y as i32 - physical.y - image.placement.top;
                let bpp = image.data.len() / (w * h) as usize;
                spots.push(GlyphSpot {
                    draw_x: dx,
                    draw_y: dy,
                    img_w: w,
                    img_h: h,
                    data: image.data.clone(),
                    is_color: bpp == 4,
                });
                max_x = max_x.max(dx + w as i32);
                max_y = max_y.max(dy + h as i32);
            }
        }
    }

    if max_x <= 0 || max_y <= 0 || spots.is_empty() {
        return (vec![], 0, 0);
    }

    // --- Pass 2: blit glyphs into RGBA bitmap ---
    let width = max_x as u32;
    let height = max_y as u32;
    let mut pixels = vec![0u8; (width * height * 4) as usize];

    let [cr, cg, cb, ca] = color;

    for spot in &spots {
        for row in 0..spot.img_h as i32 {
            for col in 0..spot.img_w as i32 {
                let px = spot.draw_x + col;
                let py = spot.draw_y + row;
                if px < 0 || py < 0 || px >= width as i32 || py >= height as i32 {
                    continue;
                }
                let dst = (py as u32 * width + px as u32) as usize * 4;
                let src = (row as usize * spot.img_w as usize + col as usize)
                    * if spot.is_color { 4 } else { 1 };

                if spot.is_color {
                    // Color glyph (emoji): tint by the component color.
                    let sr = spot.data[src] as f32 / 255.0;
                    let sg = spot.data[src + 1] as f32 / 255.0;
                    let sb = spot.data[src + 2] as f32 / 255.0;
                    let sa = spot.data[src + 3] as f32 / 255.0;
                    pixels[dst] = (sr * cr * 255.0) as u8;
                    pixels[dst + 1] = (sg * cg * 255.0) as u8;
                    pixels[dst + 2] = (sb * cb * 255.0) as u8;
                    pixels[dst + 3] = (sa * ca * 255.0) as u8;
                } else {
                    // Mask glyph: alpha from mask, colour from component.
                    let mask = spot.data[src] as f32 / 255.0;
                    pixels[dst] = (cr * 255.0) as u8;
                    pixels[dst + 1] = (cg * 255.0) as u8;
                    pixels[dst + 2] = (cb * 255.0) as u8;
                    pixels[dst + 3] = (mask * ca * 255.0) as u8;
                }
            }
        }
    }

    (pixels, width, height)
}

fn make_bind_group(
    device: &wgpu::Device,
    bgl: &wgpu::BindGroupLayout,
    tex: &super::texture::GpuTexture,
) -> wgpu::BindGroup {
    device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("text_bg"),
        layout: bgl,
        entries: &[
            wgpu::BindGroupEntry {
                binding: 0,
                resource: wgpu::BindingResource::TextureView(&tex.view),
            },
            wgpu::BindGroupEntry {
                binding: 1,
                resource: wgpu::BindingResource::Sampler(&tex.sampler),
            },
        ],
    })
}
