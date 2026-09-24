use crate::error::{Result, RustMsptError};
use crate::types::{Mesh, Triangle, Vec3};
use std::collections::HashMap;
use std::fs;
use std::io::{BufReader, BufWriter, Cursor, Read, Write};
use std::path::{Path, PathBuf};

// AI-FUNC-SUMMARY: Parse one ASCII STL "vertex x y z" line into a Vec3; returns Some(Vec3) on valid format, None otherwise; side effects: None.
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

// AI-FUNC-SUMMARY: Quantize a vertex to a fixed-precision integer key for tolerant deduplication; returns (i64, i64, i64); side effects: None.
fn quantize_key(v: Vec3) -> (i64, i64, i64) {
    const SCALE: f64 = 1_000_000.0;
    (
        (v.x * SCALE).round() as i64,
        (v.y * SCALE).round() as i64,
        (v.z * SCALE).round() as i64,
    )
}

// AI-FUNC-SUMMARY: Deduplicate a vertex against existing list using quantized key matching; returns index of existing or newly inserted vertex; side effects: Mutates vertices vec and map.
fn dedup_vertex(
    vertices: &mut Vec<Vec3>,
    map: &mut HashMap<(i64, i64, i64), usize>,
    v: Vec3,
) -> usize {
    let key = quantize_key(v);
    if let Some(&idx) = map.get(&key) {
        return idx;
    }
    let idx = vertices.len();
    vertices.push(v);
    map.insert(key, idx);
    idx
}

// AI-FUNC-SUMMARY:
// Purpose: Parse an ASCII STL document into a Mesh with deduplicated vertices.
// Inputs: STL text content and source path (for error messages).
// Returns: Parsed Mesh.
// Side effects: None.
// Notes: Returns InvalidMesh error if header missing or no valid triangles found.
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
                faces.push(Triangle { a, b, c });
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

// AI-FUNC-SUMMARY: Parse little-endian f32 bytes and upcast to f64; returns f64; side effects: None.
fn parse_f32_le(bytes: &[u8]) -> f64 {
    let arr = [bytes[0], bytes[1], bytes[2], bytes[3]];
    f32::from_le_bytes(arr) as f64
}

// AI-FUNC-SUMMARY:
// Purpose: Parse binary STL bytes into a Mesh with deduplicated vertices.
// Inputs: raw file bytes and source path (for error messages).
// Returns: Parsed Mesh.
// Side effects: None.
// Notes: Returns InvalidMesh error if file too small, size mismatch, or vertex index overflow.
fn parse_binary_stl(bytes: &[u8], path: &Path) -> Result<Mesh> {
    parse_binary_reader(Cursor::new(bytes), path)
}

// AI-FUNC-SUMMARY: Parse binary STL from a reader using one 50-byte triangle record and incremental deduplication; drains trailing bytes, preserving raw-file hashing and old trailing-data acceptance.
fn parse_binary_reader(mut reader: impl Read, path: &Path) -> Result<Mesh> {
    let mut header = [0u8; 84];
    read_stl_record(&mut reader, &mut header, path)?;
    let count = u32::from_le_bytes(header[80..84].try_into().unwrap());
    let mut vertices = Vec::new();
    let mut faces = Vec::new();
    let mut map = HashMap::new();
    let mut record = [0u8; 50];
    for _ in 0..count {
        read_stl_record(&mut reader, &mut record, path)?;
        let mut indices = [0usize; 3];
        for (i, index) in indices.iter_mut().enumerate() {
            let offset = 12 + i * 12;
            let v = Vec3::new(
                parse_f32_le(&record[offset..offset + 4]),
                parse_f32_le(&record[offset + 4..offset + 8]),
                parse_f32_le(&record[offset + 8..offset + 12]),
            );
            *index = dedup_vertex(&mut vertices, &mut map, v);
        }
        faces.push(Triangle {
            a: indices[0],
            b: indices[1],
            c: indices[2],
        });
    }
    std::io::copy(&mut reader, &mut std::io::sink())?;
    Ok(Mesh { vertices, faces })
}

// AI-FUNC-SUMMARY: Read a complete STL header/triangle, reporting truncated files as InvalidMesh and propagating other read failures.
fn read_stl_record(reader: &mut impl Read, buffer: &mut [u8], path: &Path) -> Result<()> {
    reader.read_exact(buffer).map_err(|error| {
        if error.kind() == std::io::ErrorKind::UnexpectedEof {
            RustMsptError::InvalidMesh(format!("Binary STL size mismatch in {}", path.display()))
        } else {
            error.into()
        }
    })
}

// AI-FUNC-SUMMARY: Heuristically detect whether bytes represent ASCII STL (checks "solid" header + "facet"/"vertex" keywords); returns bool; side effects: None.
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

// AI-FUNC-SUMMARY:
// Purpose: Load an STL file with automatic ASCII/binary detection and parsing.
// Inputs: file path.
// Returns: Parsed Mesh.
// Side effects: Reads from disk.
// Notes: Tries ASCII first if header matches; falls back to binary. ASCII parse failure silently falls back to binary.
pub fn load_stl(path: &Path) -> Result<Mesh> {
    load_stl_from_reader(
        BufReader::with_capacity(64 * 1024, fs::File::open(path)?),
        path,
    )
}

// AI-FUNC-SUMMARY: Load an STL from a forward reader, retaining legacy ASCII sniff/fallback and streaming binary triangle records; ASCII retains its existing full-text buffer.
pub fn load_stl_from_reader(mut reader: impl Read, path: &Path) -> Result<Mesh> {
    let mut prefix = Vec::with_capacity(512);
    reader.by_ref().take(512).read_to_end(&mut prefix)?;
    if looks_ascii_stl(&prefix) {
        reader.read_to_end(&mut prefix)?;
        let text = String::from_utf8_lossy(&prefix);
        if let Ok(mesh) = parse_ascii_stl(&text, path) {
            return Ok(mesh);
        }
        return parse_binary_stl(&prefix, path);
    }
    parse_binary_reader(Cursor::new(prefix).chain(reader), path)
}

// AI-FUNC-SUMMARY: Parse and SHA-256 the exact same raw STL byte stream in one file pass; returns mesh, digest and byte count, including ignored binary trailers.
pub fn load_stl_hashed(path: &Path) -> Result<(Mesh, String, u64)> {
    let file = BufReader::with_capacity(64 * 1024, fs::File::open(path)?);
    let mut reader = super::hash::HashingReader::new(file);
    let mesh = load_stl_from_reader(&mut reader, path)?;
    let (digest, bytes) = reader.finish();
    Ok((mesh, digest, bytes))
}

// AI-FUNC-SUMMARY:
// Purpose: Load all STL files in a folder directory.
// Inputs: folder path.
// Returns: Vec of (file path, parsed mesh) pairs.
// Side effects: Reads from disk.
pub fn load_folder_stls(folder: &Path) -> Result<Vec<(PathBuf, Mesh)>> {
    use rayon::prelude::*;
    let paths = stl_paths(folder)?;
    let mut out = Vec::with_capacity(paths.len());
    for batch in paths.chunks(2) {
        let loaded: Vec<_> = batch.par_iter().map(|path| load_stl(path)).collect();
        for (path, mesh) in batch.iter().zip(loaded) { out.push((path.clone(), mesh?)); }
    }
    Ok(out)
}

// AI-FUNC-SUMMARY: Collect STL paths in the existing directory iteration order, retaining case-insensitive extension matching and error propagation.
fn stl_paths(folder: &Path) -> Result<Vec<PathBuf>> {
    let mut paths = Vec::new();
    for entry in fs::read_dir(folder)? {
        let path = entry?.path();
        if path.extension().is_some_and(|ext| ext.to_string_lossy().eq_ignore_ascii_case("stl")) { paths.push(path); }
    }
    Ok(paths)
}

// AI-FUNC-SUMMARY:
// Purpose: Load a single STL file or merge all STLs in a directory into one mesh.
// Inputs: file or directory path.
// Returns: Merged mesh for directories, or single loaded mesh for files.
// Side effects: Reads from disk.
// Notes: Returns InvalidConfig if directory contains no STL files.
pub fn load_stl_or_merge_folder(path: &Path) -> Result<Mesh> {
    if path.is_dir() {
        use rayon::prelude::*;
        let paths = stl_paths(path)?;
        if paths.is_empty() {
            return Err(RustMsptError::InvalidConfig(format!("No STL files found in directory: {}", path.display())));
        }
        let mut out = Mesh::empty();
        for batch in paths.chunks(2) {
            let loaded: Vec<_> = batch.par_iter().map(|path| load_stl(path)).collect();
            for mesh in loaded {
                let mesh = mesh?;
                let offset = out.vertices.len();
                out.vertices.extend(mesh.vertices);
                out.faces.extend(mesh.faces.into_iter().map(|f| Triangle { a: f.a + offset, b: f.b + offset, c: f.c + offset }));
            }
        }
        Ok(out)
    } else {
        load_stl(path)
    }
}

// AI-FUNC-SUMMARY:
// Purpose: Save a mesh as a binary STL file.
// Inputs: target path, mesh, and solid name for the 80-byte header.
// Returns: Ok(()) on success.
// Side effects: Creates parent directories; writes binary file to disk.
// Notes: Returns InvalidMesh error for empty mesh. Writes zero normals (recalc not performed). Converts f64 vertices to f32 for STL format.
pub fn save_stl(path: &Path, mesh: &Mesh, solid_name: &str) -> Result<()> {
    if mesh.is_empty() {
        return Err(RustMsptError::InvalidMesh(
            "Cannot write empty mesh".to_string(),
        ));
    }

    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }

    let mut file = BufWriter::with_capacity(64 * 1024, fs::File::create(path)?);

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

    file.flush()?;
    Ok(())
}
