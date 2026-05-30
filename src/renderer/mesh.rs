use super::scene::Vertex;
use crate::vfs::{Vfs, VfsPath};
use anyhow::Context;
use serde::Deserialize;

/// Geometry ready to upload to the GPU.
pub struct CpuMesh {
    pub vertices: Vec<Vertex>,
    /// Triangle indices.  `u16` — maximum 65 535 unique vertices per mesh.
    pub indices: Vec<u16>,
}

/// Load a mesh from the VFS, dispatching on file extension.
///
/// Supported formats:
/// - `.json`        — `{"vertices":[[x,y,z,u,v],...], "indices":[...]}`
/// - `.obj`         — Wavefront OBJ; materials are silently ignored
/// - `.glb`/`.gltf` — glTF 2.0 (first mesh / first `TRIANGLES` primitive);
///                    only GLB and self-contained (base64-embedded) GLTF are supported;
///                    materials and animations are ignored
pub fn load_mesh_from_vfs(vfs: &dyn Vfs, path: &str) -> anyhow::Result<CpuMesh> {
    let ext = path.rsplit('.').next().unwrap_or("").to_lowercase();
    match ext.as_str() {
        "json" => load_json(vfs, path),
        "obj" => load_obj(vfs, path),
        "glb" | "gltf" => load_gltf(vfs, path),
        other => anyhow::bail!(
            "unsupported mesh format '.{other}'; supported: .json, .obj, .glb, .gltf"
        ),
    }
}

// --- JSON --------------------------------------------------------------------

#[derive(Deserialize)]
struct MeshJson {
    /// Each element is either `[x, y, z, u, v]` (5 floats, normal defaults to
    /// `[0, 0, 1]`) or `[x, y, z, u, v, nx, ny, nz]` (8 floats with explicit normal).
    vertices: Vec<Vec<f32>>,
    indices: Vec<u16>,
}

fn load_json(vfs: &dyn Vfs, path: &str) -> anyhow::Result<CpuMesh> {
    let bytes = read_vfs(vfs, path)?;
    let json: MeshJson =
        serde_json::from_slice(&bytes).with_context(|| format!("parsing mesh JSON '{path}'"))?;
    let vertices = json
        .vertices
        .iter()
        .enumerate()
        .map(|(i, v)| {
            anyhow::ensure!(
                v.len() >= 5,
                "mesh JSON '{path}': vertex {i} has fewer than 5 elements"
            );
            let nx = v.get(5).copied().unwrap_or(0.0);
            let ny = v.get(6).copied().unwrap_or(0.0);
            let nz = v.get(7).copied().unwrap_or(1.0);
            Ok(Vertex {
                position: [v[0], v[1], v[2]],
                tex_coords: [v[3], v[4]],
                normal: [nx, ny, nz],
            })
        })
        .collect::<anyhow::Result<Vec<_>>>()?;
    Ok(CpuMesh {
        vertices,
        indices: json.indices,
    })
}

// --- OBJ ---------------------------------------------------------------------

fn load_obj(vfs: &dyn Vfs, path: &str) -> anyhow::Result<CpuMesh> {
    let bytes = read_vfs(vfs, path)?;
    let cursor = std::io::Cursor::new(bytes);
    let load_options = tobj::LoadOptions {
        triangulate: true,
        single_index: true,
        ..Default::default()
    };
    let (models, _materials) =
        tobj::load_obj_buf(&mut std::io::BufReader::new(cursor), &load_options, |_| {
            Err(tobj::LoadError::OpenFileFailed)
        })
        .with_context(|| format!("parsing OBJ '{path}'"))?;

    let mut vertices: Vec<Vertex> = Vec::new();
    let mut indices: Vec<u16> = Vec::new();
    let mut index_offset: u16 = 0;

    for model in &models {
        let mesh = &model.mesh;
        let vert_count = mesh.positions.len() / 3;
        for i in 0..vert_count {
            let u = if mesh.texcoords.is_empty() { 0.0 } else { mesh.texcoords[i * 2] };
            // OBJ UV origin is bottom-left; flip V to match our top-left convention.
            let v = if mesh.texcoords.is_empty() {
                0.0
            } else {
                1.0 - mesh.texcoords[i * 2 + 1]
            };
            let nx = if mesh.normals.is_empty() { 0.0 } else { mesh.normals[i * 3] };
            let ny = if mesh.normals.is_empty() { 0.0 } else { mesh.normals[i * 3 + 1] };
            let nz = if mesh.normals.is_empty() { 1.0 } else { mesh.normals[i * 3 + 2] };
            vertices.push(Vertex {
                position: [
                    mesh.positions[i * 3],
                    mesh.positions[i * 3 + 1],
                    mesh.positions[i * 3 + 2],
                ],
                tex_coords: [u, v],
                normal: [nx, ny, nz],
            });
        }
        for &idx in &mesh.indices {
            indices.push(
                index_offset
                    .checked_add(u16::try_from(idx).context("OBJ index exceeds u16")?)
                    .context("OBJ index sum exceeds u16")?,
            );
        }
        index_offset = index_offset
            .checked_add(u16::try_from(vert_count).context("OBJ vertex count exceeds u16")?)
            .context("OBJ vertex count sum exceeds u16")?;
    }

    anyhow::ensure!(!vertices.is_empty(), "OBJ '{path}' contains no geometry");
    Ok(CpuMesh { vertices, indices })
}

// --- glTF / GLB --------------------------------------------------------------

fn load_gltf(vfs: &dyn Vfs, path: &str) -> anyhow::Result<CpuMesh> {
    let bytes = read_vfs(vfs, path)?;
    let (doc, buffers, _images) = gltf::import_slice(&bytes)
        .with_context(|| format!("parsing glTF/GLB '{path}'"))?;

    // Take the first mesh and the first TRIANGLES primitive.
    let mesh = doc
        .meshes()
        .next()
        .with_context(|| format!("glTF '{path}' has no meshes"))?;

    let prim = mesh
        .primitives()
        .find(|p| p.mode() == gltf::mesh::Mode::Triangles)
        .with_context(|| format!("glTF '{path}' has no TRIANGLES primitive"))?;

    let reader = prim.reader(|buf| buffers.get(buf.index()).map(|b| b.0.as_slice()));

    // Positions — required.
    let positions: Vec<[f32; 3]> = reader
        .read_positions()
        .with_context(|| format!("glTF '{path}' primitive has no POSITION accessor"))?
        .collect();

    // Texture coordinates — optional, default to [0, 0].
    let tex_coords: Vec<[f32; 2]> = reader
        .read_tex_coords(0)
        .map(|iter| iter.into_f32().collect())
        .unwrap_or_else(|| vec![[0.0, 0.0]; positions.len()]);

    // Surface normals — optional, default to [0, 0, 1].
    let normals: Vec<[f32; 3]> = reader
        .read_normals()
        .map(|iter| iter.collect())
        .unwrap_or_else(|| vec![[0.0, 0.0, 1.0]; positions.len()]);

    let vertices: Vec<Vertex> = positions
        .iter()
        .zip(tex_coords.iter())
        .zip(normals.iter())
        .map(|((pos, uv), n)| Vertex {
            position: *pos,
            tex_coords: *uv,
            normal: *n,
        })
        .collect();

    // Indices — required for TRIANGLES.
    let indices: Vec<u16> = reader
        .read_indices()
        .with_context(|| format!("glTF '{path}' primitive has no index accessor"))?
        .into_u32()
        .map(|i| u16::try_from(i).expect("glTF index exceeds u16 — mesh has > 65535 vertices"))
        .collect();

    anyhow::ensure!(!vertices.is_empty(), "glTF '{path}' contains no geometry");
    Ok(CpuMesh { vertices, indices })
}

// --- helpers -----------------------------------------------------------------

fn read_vfs(vfs: &dyn Vfs, path: &str) -> anyhow::Result<Vec<u8>> {
    let vfs_path =
        VfsPath::parse(path).ok_or_else(|| anyhow::anyhow!("invalid VFS path: '{path}'"))?;
    vfs.read(&vfs_path)
        .with_context(|| format!("reading mesh '{path}'"))
}
