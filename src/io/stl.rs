use crate::error::{Result, RustMsptError};
use crate::types::{Mesh, Triangle, Vec3};
use std::collections::HashMap;
use std::hash::{BuildHasherDefault, Hasher};

/// Vertex-weld map: quantized coordinates to the vertex index first assigned to them.
///
/// Only looked up and inserted, never iterated, so the hasher cannot affect which index a vertex gets;
/// SipHash cost more than the rest of a binary load (PLAN.Performance.md section 73).
type WeldMap = HashMap<(i64, i64, i64), usize, BuildHasherDefault<WeldHasher>>;

// AI-FUNC-SUMMARY: Multiply-xor hasher for quantized vertex keys with a 64-bit finalizer so the low bits the table indexes by depend on every input bit; returns the hash; side effects: none.
// Notes: Not collision-resistant against crafted input, which for a local mesh file costs only load time.
#[derive(Default)]
struct WeldHasher(u64);

impl Hasher for WeldHasher {
    fn write(&mut self, bytes: &[u8]) {
        for &b in bytes {
            self.write_u64(u64::from(b));
        }
    }
    fn write_u64(&mut self, value: u64) {
        self.0 = (self.0.rotate_left(5) ^ value).wrapping_mul(0x51_7c_c1_b7_27_22_0a_95);
    }
    fn write_i64(&mut self, value: i64) {
        self.write_u64(value as u64);
    }
    fn finish(&self) -> u64 {
        let mut h = self.0;
        h ^= h >> 33;
        h = h.wrapping_mul(0xff51_afd7_ed55_8ccd);
        h ^= h >> 33;
        h = h.wrapping_mul(0xc4ce_b9fe_1a85_ec53);
        h ^ (h >> 33)
    }
}
use std::fs;
use std::io::{BufRead, BufReader, BufWriter, Cursor, Read, Write};
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
    map: &mut WeldMap,
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

#[derive(Default)]
struct AsciiStlBuilder {
    vertices: Vec<Vec3>,
    faces: Vec<Triangle>,
    pending: Vec<Vec3>,
    map: WeldMap,
}

impl AsciiStlBuilder {
    // AI-FUNC-SUMMARY: Consume one raw ASCII STL line (terminator optional), applying the legacy lossy-UTF-8/trim/vertex rules and emitting a deduplicated face after every third vertex; side effects: mutates builder state.
    fn push_line(&mut self, raw: &[u8]) {
        let raw = raw.strip_suffix(b"\n").unwrap_or(raw);
        let text = String::from_utf8_lossy(raw);
        if let Some(v) = parse_ascii_vertex(text.trim()) {
            self.pending.push(v);
            if self.pending.len() == 3 {
                let a = dedup_vertex(&mut self.vertices, &mut self.map, self.pending[0]);
                let b = dedup_vertex(&mut self.vertices, &mut self.map, self.pending[1]);
                let c = dedup_vertex(&mut self.vertices, &mut self.map, self.pending[2]);
                self.pending.clear();
                self.faces.push(Triangle { a, b, c });
            }
        }
    }
}

// AI-FUNC-SUMMARY:
// Purpose: Stream an ASCII-sniffed STL line by line into a Mesh, falling back to binary parsing of the same bytes when no ASCII triangle is found.
// Inputs: buffered reader positioned at the first byte of the file (sniff prefix included) and source path for errors.
// Returns: Parsed Mesh (ASCII result if any triangle parsed, otherwise the legacy binary fallback result).
// Side effects: Reads the reader to EOF.
// Notes: Raw bytes are retained only until the first ASCII triangle completes, after which success is certain; a misdetected binary file is therefore still fully retained for fallback, exactly like the former whole-text buffer. Line splitting on b'\n' then per-line lossy UTF-8 and trim is equivalent to the former whole-text lossy conversion because 0x0A never occurs inside a UTF-8 sequence.
fn parse_ascii_stream_or_binary(mut reader: impl BufRead, path: &Path) -> Result<Mesh> {
    let mut builder = AsciiStlBuilder::default();
    let mut retained = Vec::new();
    let mut retaining = true;
    let mut line = Vec::new();
    loop {
        line.clear();
        if reader.read_until(b'\n', &mut line)? == 0 {
            break;
        }
        if retaining {
            retained.extend_from_slice(&line);
        }
        builder.push_line(&line);
        if retaining && !builder.faces.is_empty() {
            retaining = false;
            retained = Vec::new();
        }
    }
    if builder.faces.is_empty() {
        return parse_binary_stl(&retained, path);
    }
    Ok(Mesh {
        vertices: builder.vertices,
        faces: builder.faces,
    })
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
    // A closed triangle mesh has about half as many vertices as faces; the header's count is only a
    // hint for sizing, capped so a corrupt header cannot reserve memory the file does not back.
    let hint = (count as usize).min(1 << 24);
    let mut vertices = Vec::with_capacity(hint / 2 + 3);
    let mut faces = Vec::with_capacity(hint);
    let mut map = WeldMap::with_capacity_and_hasher(hint / 2 + 3, Default::default());
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

// AI-FUNC-SUMMARY: Load an STL from a forward reader, retaining legacy 512-byte ASCII sniff and binary fallback; ASCII is parsed line by line and binary by 50-byte records, neither buffering the whole file once an ASCII triangle is found.
pub fn load_stl_from_reader(mut reader: impl Read, path: &Path) -> Result<Mesh> {
    let mut prefix = Vec::with_capacity(512);
    reader.by_ref().take(512).read_to_end(&mut prefix)?;
    if looks_ascii_stl(&prefix) {
        return parse_ascii_stream_or_binary(
            BufReader::with_capacity(64 * 1024, Cursor::new(prefix).chain(reader)),
            path,
        );
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
