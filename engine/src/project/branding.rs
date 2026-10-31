use super::Project;
use anyhow::{Context, ensure};
use image::{DynamicImage, ImageFormat, Rgba, RgbaImage};
use std::path::Path;

/// Vector-like engine mark rasterized at the requested icon size, without external tools.
pub fn default_icon_rgba(size: u32) -> Vec<u8> {
    let points = [
        (0.21_f32, 0.28_f32),
        (0.32, 0.72),
        (0.5, 0.44),
        (0.68, 0.72),
        (0.79, 0.28),
    ];
    let mut image = RgbaImage::new(size, size);
    for (x, y, pixel) in image.enumerate_pixels_mut() {
        let mut sum = [0_f32; 4];
        for sy in 0..4 {
            for sx in 0..4 {
                let px = (x as f32 + (sx as f32 + 0.5) / 4.0) / size as f32;
                let py = (y as f32 + (sy as f32 + 0.5) / 4.0) / size as f32;
                let dx = (px - 0.5).abs() - 0.30;
                let dy = (py - 0.5).abs() - 0.30;
                let inside = (dx.max(0.0).powi(2) + dy.max(0.0).powi(2)).sqrt()
                    + dx.max(dy).min(0.0)
                    <= 0.18;
                if !inside {
                    continue;
                }
                let mark = points.windows(2).any(|pair| {
                    let (ax, ay) = pair[0];
                    let (bx, by) = pair[1];
                    let t = (((px - ax) * (bx - ax) + (py - ay) * (by - ay))
                        / ((bx - ax).powi(2) + (by - ay).powi(2)))
                    .clamp(0.0, 1.0);
                    ((px - ax - t * (bx - ax)).powi(2) + (py - ay - t * (by - ay)).powi(2)).sqrt()
                        < 0.037
                });
                let color = if mark {
                    [93.0 + py * 70.0, 217.0 - py * 80.0, 255.0, 255.0]
                } else {
                    [21.0 + py * 17.0, 28.0 + px * 11.0, 47.0 + py * 34.0, 255.0]
                };
                for channel in 0..4 {
                    sum[channel] += color[channel] / 16.0;
                }
            }
        }
        *pixel = Rgba(sum.map(|c| c.round() as u8));
    }
    image.into_raw()
}

pub(super) fn brand(project: &Project, executable: &Path, output: &Path) -> anyhow::Result<()> {
    semver::Version::parse(&project.build.version)
        .context("invalid game version in Build Settings")?;
    let image = if let Some(icon) = &project.build.icon {
        let path = project
            .path
            .parent()
            .context("project directory")?
            .join(icon);
        image::open(&path)
            .with_context(|| format!("read game icon {}", super::display_path(&path)))?
    } else {
        DynamicImage::ImageRgba8(
            RgbaImage::from_raw(256, 256, default_icon_rgba(256)).context("default icon")?,
        )
    };
    ensure!(
        image.width() > 0 && image.height() > 0,
        "game icon is empty"
    );
    let image = image.resize_exact(256, 256, image::imageops::FilterType::Lanczos3);
    std::fs::create_dir_all(output.join("branding"))?;
    image.save_with_format(output.join("branding/icon.png"), ImageFormat::Png)?;
    #[cfg(windows)]
    windows::apply(project, executable, &image)?;
    #[cfg(not(windows))]
    {
        let _ = executable;
    }
    Ok(())
}

#[cfg(windows)]
mod windows {
    use super::*;
    use ::windows::Win32::System::LibraryLoader::{
        BeginUpdateResourceW, EndUpdateResourceW, UpdateResourceW,
    };
    use ::windows::core::PCWSTR;
    use std::io::Cursor;
    use std::os::windows::ffi::OsStrExt;

    // Win32 resource names may encode a numeric ID instead of an address.
    fn resource_id(id: u16) -> PCWSTR {
        PCWSTR(std::ptr::without_provenance(usize::from(id)))
    }
    fn wide(text: &str) -> Vec<u8> {
        text.encode_utf16()
            .chain([0])
            .flat_map(u16::to_le_bytes)
            .collect()
    }
    fn align(bytes: &mut Vec<u8>) {
        while !bytes.len().is_multiple_of(4) {
            bytes.push(0);
        }
    }
    fn node(key: &str, kind: u16, value_len: u16, value: &[u8], children: &[Vec<u8>]) -> Vec<u8> {
        let mut result = vec![0, 0];
        result.extend(value_len.to_le_bytes());
        result.extend(kind.to_le_bytes());
        result.extend(wide(key));
        align(&mut result);
        result.extend(value);
        for child in children {
            align(&mut result);
            result.extend(child);
        }
        let len = result.len() as u16;
        result[..2].copy_from_slice(&len.to_le_bytes());
        result
    }
    fn version(project: &Project) -> anyhow::Result<Vec<u8>> {
        let version = semver::Version::parse(&project.build.version)?;
        ensure!(
            [version.major, version.minor, version.patch]
                .iter()
                .all(|n| *n <= 65535),
            "game version fields must fit in 16 bits"
        );
        let ms = ((version.major as u32) << 16) | version.minor as u32;
        let ls = (version.patch as u32) << 16;
        let fixed: [u32; 13] = [
            0xfeef04bd, 0x10000, ms, ls, ms, ls, 0x3f, 0, 0x40004, 1, 0, 0, 0,
        ];
        let strings = [
            ("FileDescription", project.name.as_str()),
            ("ProductName", project.name.as_str()),
            ("CompanyName", project.build.company.as_str()),
            ("FileVersion", project.build.version.as_str()),
            ("ProductVersion", project.build.version.as_str()),
        ]
        .iter()
        .map(|(key, value)| {
            let value = wide(value);
            node(key, 1, (value.len() / 2) as u16, &value, &[])
        })
        .collect::<Vec<_>>();
        let table = node("040904B0", 1, 0, &[], &strings);
        let strings = node("StringFileInfo", 1, 0, &[], &[table]);
        let translation = node("Translation", 0, 4, &[9, 4, 0xb0, 4], &[]);
        let vars = node("VarFileInfo", 1, 0, &[], &[translation]);
        Ok(node(
            "VS_VERSION_INFO",
            0,
            52,
            &fixed
                .into_iter()
                .flat_map(u32::to_le_bytes)
                .collect::<Vec<_>>(),
            &[strings, vars],
        ))
    }
    pub fn apply(project: &Project, executable: &Path, image: &DynamicImage) -> anyhow::Result<()> {
        let path: Vec<u16> = executable.as_os_str().encode_wide().chain([0]).collect();
        let update = unsafe { BeginUpdateResourceW(PCWSTR(path.as_ptr()), false)? };
        let result = (|| -> anyhow::Result<()> {
            let sizes = [16_u32, 24, 32, 48, 64, 128, 256];
            let mut group = vec![0, 0, 1, 0, sizes.len() as u8, 0];
            for (index, size) in sizes.iter().enumerate() {
                let mut png = Cursor::new(Vec::new());
                image
                    .resize_exact(*size, *size, image::imageops::FilterType::Lanczos3)
                    .write_to(&mut png, ImageFormat::Png)?;
                let png = png.into_inner();
                let id = (index + 1) as u16;
                unsafe {
                    UpdateResourceW(
                        update,
                        resource_id(3),
                        resource_id(id),
                        0,
                        Some(png.as_ptr().cast()),
                        png.len() as u32,
                    )?;
                }
                group.extend([*size as u8, *size as u8, 0, 0]);
                group.extend(1_u16.to_le_bytes());
                group.extend(32_u16.to_le_bytes());
                group.extend((png.len() as u32).to_le_bytes());
                group.extend(id.to_le_bytes());
            }
            let version = version(project)?;
            unsafe {
                UpdateResourceW(
                    update,
                    resource_id(14),
                    resource_id(1),
                    0,
                    Some(group.as_ptr().cast()),
                    group.len() as u32,
                )?;
                UpdateResourceW(
                    update,
                    resource_id(16),
                    resource_id(1),
                    0x409,
                    Some(version.as_ptr().cast()),
                    version.len() as u32,
                )?;
            }
            Ok(())
        })();
        unsafe {
            EndUpdateResourceW(update, result.is_err())?;
        }
        result
    }
}
