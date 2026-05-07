use crate::error::{Result, RustMsptError};
use crate::types::{Mesh, Triangle, Vec3};
use std::collections::HashMap;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

/// Parse one ASCII STL vertex line.
/// Inputs: a trimmed text line.
/// Outputs: parsed Vec3 when line format is "vertex x y z", else None.
fn parse_ascii_vertex(line: &str) -> Option<Vec3> {
    let parts: Vec<&str> = line.split_whitespace().collect();
    if parts.len() != 4 || parts[0] != "vertex" {
        return None;
    }
    let x = parts[1].parse::<f64>().ok()?;
    let y = parts[2].parse::<f64>().ok()?;
    let z = parts[3].parse::<f64>().ok()?;
    Some(Vec3::new(x, y, z))
}

/// Quantize a vertex for tolerant deduplication.
/// Inputs: vertex coordinate.
/// Outputs: integer key with fixed precision.
fn quantize_key(v: Vec3) -> (i64, i64, i64) {
    const SCALE: f64 = 1_000_000.0;
    (
        (v.x * SCALE).round() as i64,
        (v.y * SCALE).round() as i64,
        (v.z * SCALE).round() as i64,
    )
}

/// Deduplicate a vertex in-place and return index.
/// Inputs: mutable vertex list, key map, and candidate vertex.
/// Outputs: index of existing or newly inserted vertex.
fn dedup_vertex(vertices: &mut Vec<Vec3>, map: &mut HashMap<(i64, i64, i64), usize>, v: Vec3) -> usize {
    let key = quantize_key(v);
    if let Some(&idx) = map.get(&key) {
        return idx;
    }
    let idx = vertices.len();
    vertices.push(v);
    map.insert(key, idx);
    idx
}

/// Parse an ASCII STL document into Mesh.
/// Inputs: STL text and source path (for error context).
/// Outputs: parsed Mesh with deduplicated vertices.
fn parse_ascii_stl(content: &str, path: &Path) -> Result<Mesh> {
    let trimmed = content.trim_start();
    if !trimmed.starts_with("solid") {
        return Err(RustMsptError::InvalidMesh(format!(
            "ASCII STL header missing in {}",
            path.display()
        )));
    }

    let mut vertices: Vec<Vec3> = Vec::new();
    let mut faces: Vec<Triangle> = Vec::new();
    let mut face_buffer: Vec<Vec3> = Vec::new();
    let mut vertex_map: HashMap<(i64, i64, i64), usize> = HashMap::new();

    for line in content.lines().map(str::trim) {
        if let Some(v) = parse_ascii_vertex(line) {
            face_buffer.push(v);
            if face_buffer.len() == 3 {
                let a_v = face_buffer.remove(0);
                let b_v = face_buffer.remove(0);
                let c_v = face_buffer.remove(0);

                let a = dedup_vertex(&mut vertices, &mut vertex_map, a_v);
                let b = dedup_vertex(&mut vertices, &mut vertex_map, b_v);
                let c = dedup_vertex(&mut vertices, &mut vertex_map, c_v);
                faces.push(Triangle {
                    a,
                    b,
                    c,
                });
            }
        }
    }

    if vertices.is_empty() || faces.is_empty() {
        return Err(RustMsptError::InvalidMesh(format!(
            "No valid triangles parsed from {}",
            path.display()
        )));
    }

    Ok(Mesh { vertices, faces })
}

/// Parse little-endian f32 bytes and upcast to f64.
/// Inputs: 4-byte slice.
/// Outputs: decoded floating-point value.
fn parse_f32_le(bytes: &[u8]) -> f64 {
    let arr = [bytes[0], bytes[1], bytes[2], bytes[3]];
    f32::from_le_bytes(arr) as f64
}

/// Parse binary STL bytes into Mesh.
/// Inputs: raw file bytes and source path (for error context).
/// Outputs: parsed Mesh with deduplicated vertices.
fn parse_binary_stl(bytes: &[u8], path: &Path) -> Result<Mesh> {
    if bytes.len() < 84 {
        return Err(RustMsptError::InvalidMesh(format!(
            "Binary STL too small: {}",
            path.display()
        )));
    }

    let tri_count = u32::from_le_bytes([bytes[80], bytes[81], bytes[82], bytes[83]]) as usize;
    let expected = 84usize + tri_count.saturating_mul(50usize);
    if bytes.len() < expected {
        return Err(RustMsptError::InvalidMesh(format!(
            "Binary STL size mismatch in {}",
            path.display()
        )));
    }

    let mut vertices = Vec::with_capacity(tri_count * 3);
    let mut faces = Vec::with_capacity(tri_count);
    let mut vertex_map: HashMap<(i64, i64, i64), usize> = HashMap::new();

    let mut offset = 84usize;
    for _ in 0..tri_count {
        offset += 12;

        let v1 = Vec3::new(
            parse_f32_le(&bytes[offset..offset + 4]),
            parse_f32_le(&bytes[offset + 4..offset + 8]),
            parse_f32_le(&bytes[offset + 8..offset + 12]),
        );
        offset += 12;

        let v2 = Vec3::new(
            parse_f32_le(&bytes[offset..offset + 4]),
            parse_f32_le(&bytes[offset + 4..offset + 8]),
            parse_f32_le(&bytes[offset + 8..offset + 12]),
        );
        offset += 12;

        let v3 = Vec3::new(
            parse_f32_le(&bytes[offset..offset + 4]),
            parse_f32_le(&bytes[offset + 4..offset + 8]),
            parse_f32_le(&bytes[offset + 8..offset + 12]),
        );
        offset += 12;

        let a = dedup_vertex(&mut vertices, &mut vertex_map, v1);
        let b = dedup_vertex(&mut vertices, &mut vertex_map, v2);
        let c = dedup_vertex(&mut vertices, &mut vertex_map, v3);
        faces.push(Triangle {
            a,
            b,
            c,
        });

        offset += 2;
    }

    Ok(Mesh { vertices, faces })
}

/// Heuristically detect whether bytes likely represent ASCII STL.
/// Inputs: raw file bytes.
/// Outputs: true when header/text markers resemble ASCII STL.
fn looks_ascii_stl(bytes: &[u8]) -> bool {
    if bytes.len() < 5 {
        return false;
    }
    if &bytes[0..5] != b"solid" {
        return false;
    }
    let sniff_len = bytes.len().min(512);
    let sniff = &bytes[..sniff_len];
    let maybe_text = String::from_utf8_lossy(sniff).to_ascii_lowercase();
    maybe_text.contains("facet") && maybe_text.contains("vertex")
}

/// Load STL from path with ASCII/Binary auto-detection.
/// Inputs: file path.
/// Outputs: Mesh parsed from STL.
pub fn load_stl(path: &Path) -> Result<Mesh> {
    let bytes = fs::read(path)?;

    if looks_ascii_stl(&bytes) {
        let text = String::from_utf8_lossy(&bytes);
        if let Ok(mesh) = parse_ascii_stl(&text, path) {
            return Ok(mesh);
        }
    }

    parse_binary_stl(&bytes, path)
}

/// Load all STL files in a folder.
/// Inputs: folder path.
/// Outputs: vector of (file path, parsed mesh).
pub fn load_folder_stls(folder: &Path) -> Result<Vec<(PathBuf, Mesh)>> {
    let mut out = Vec::new();
    for entry in fs::read_dir(folder)? {
        let entry = entry?;
        let path = entry.path();
        if path
            .extension()
            .map(|e| e.to_string_lossy().to_ascii_lowercase() == "stl")
            .unwrap_or(false)
        {
            let mesh = load_stl(&path)?;
            out.push((path, mesh));
        }
    }
    Ok(out)
}

/// Load one STL file, or merge all STL files under a directory.
/// Inputs: file or directory path.
/// Outputs: merged mesh for directory input, or single loaded mesh.
pub fn load_stl_or_merge_folder(path: &Path) -> Result<Mesh> {
    if path.is_dir() {
        let meshes = load_folder_stls(path)?;
        if meshes.is_empty() {
            return Err(RustMsptError::InvalidConfig(format!(
                "No STL files found in directory: {}",
                path.display()
            )));
        }

        let mut out = Mesh::empty();
        for (_, m) in meshes {
            let offset = out.vertices.len();
            out.vertices.extend(m.vertices.iter().copied());
            out.faces.extend(m.faces.iter().map(|f| Triangle {
                a: f.a + offset,
                b: f.b + offset,
                c: f.c + offset,
            }));
        }
        Ok(out)
    } else {
        load_stl(path)
    }
}

/// Save mesh as Binary STL.
/// Inputs: target path, mesh, and STL header name.
/// Outputs: writes file to disk or returns error.
pub fn save_stl(path: &Path, mesh: &Mesh, solid_name: &str) -> Result<()> {
    if mesh.is_empty() {
        return Err(RustMsptError::InvalidMesh(
            "Cannot write empty mesh".to_string(),
        ));
    }

    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }

    let mut file = fs::File::create(path)?;

    let mut header = [0u8; 80];
    let name_bytes = solid_name.as_bytes();
    let copy_len = name_bytes.len().min(80);
    header[..copy_len].copy_from_slice(&name_bytes[..copy_len]);
    file.write_all(&header)?;

    let tri_count = mesh.faces.len() as u32;
    file.write_all(&tri_count.to_le_bytes())?;

    for face in &mesh.faces {
        let a = mesh.vertices[face.a];
        let b = mesh.vertices[face.b];
        let c = mesh.vertices[face.c];

        for normal_component in [0f32, 0f32, 0f32] {
            file.write_all(&normal_component.to_le_bytes())?;
        }

        for vertex in [a, b, c] {
            file.write_all(&(vertex.x as f32).to_le_bytes())?;
            file.write_all(&(vertex.y as f32).to_le_bytes())?;
            file.write_all(&(vertex.z as f32).to_le_bytes())?;
        }

        file.write_all(&0u16.to_le_bytes())?;
    }

    Ok(())
}
