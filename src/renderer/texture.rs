use crate::vfs::{Vfs, VfsPath};
use anyhow::Context;

pub struct GpuTexture {
    pub view: wgpu::TextureView,
    pub sampler: wgpu::Sampler,
}

pub fn load_from_vfs(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    vfs: &dyn Vfs,
    path: &VfsPath,
) -> anyhow::Result<GpuTexture> {
    let bytes = vfs
        .read(path)
        .with_context(|| format!("reading {}", path.to_string()))?;
    let img = image::load_from_memory(&bytes).context("decode image")?;
    let rgba = img.to_rgba8();
    let (width, height) = rgba.dimensions();

    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some(&path.to_string()),
        size: wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba8UnormSrgb,
        usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
        view_formats: &[],
    });

    queue.write_texture(
        wgpu::ImageCopyTexture {
            texture: &texture,
            mip_level: 0,
            origin: wgpu::Origin3d::ZERO,
            aspect: wgpu::TextureAspect::All,
        },
        &rgba,
        wgpu::ImageDataLayout {
            offset: 0,
            bytes_per_row: Some(4 * width),
            rows_per_image: Some(height),
        },
        wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        },
    );

    let view = texture.create_view(&Default::default());
    let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
        address_mode_u: wgpu::AddressMode::ClampToEdge,
        address_mode_v: wgpu::AddressMode::ClampToEdge,
        mag_filter: wgpu::FilterMode::Nearest,
        min_filter: wgpu::FilterMode::Linear,
        ..Default::default()
    });

    Ok(GpuTexture { view, sampler })
}
