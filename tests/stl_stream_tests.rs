use rustmspt::geometry::box_mesh;
use rustmspt::io::{load_stl, load_stl_from_reader, load_stl_hashed, save_stl, sha256_bytes};
use rustmspt::types::{BoundingBox, Vec3};
use std::io::{Cursor, Read};

struct ShortReads<R>(R);
impl<R: Read> Read for ShortReads<R> {
    // AI-FUNC-SUMMARY: Restrict every read to three bytes to exercise non-seekable short-read parsing.
    fn read(&mut self, b: &mut [u8]) -> std::io::Result<usize> {
        let n = b.len().min(3);
        self.0.read(&mut b[..n])
    }
}

// AI-FUNC-SUMMARY: Verify ASCII/binary sniff and fallback, short reads, exact raw-byte hashing including ignored trailers, and truncation refusal without count-sized allocation.
#[test]
fn stream_and_digest_cover_identical_bytes() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("shape.stl");
    let source = box_mesh(BoundingBox::from_size(Vec3::new(1.0, 2.0, 3.0)));
    save_stl(&path, &source, "solid facet vertex ambiguous binary").unwrap();
    let mut binary = std::fs::read(&path).unwrap();
    binary.extend_from_slice(b"trailer outside triangle records\0\xff");
    let ascii = b"solid one\nfacet normal 0 0 0\nouter loop\nvertex 0 0 0\nvertex 1 0 0\nvertex 0 1 0\nendloop\nendfacet\nendsolid one\n".to_vec();
    for bytes in [binary.clone(), ascii] {
        std::fs::write(&path, &bytes).unwrap();
        let (mesh, digest, count) = load_stl_hashed(&path).unwrap();
        assert_eq!(digest, sha256_bytes(&bytes));
        assert_eq!(count, bytes.len() as u64);
        let plain = load_stl(&path).unwrap();
        let short = load_stl_from_reader(ShortReads(Cursor::new(&bytes)), &path).unwrap();
        assert_eq!(mesh.vertices, plain.vertices);
        assert_eq!(mesh.vertices, short.vertices);
        assert_eq!(mesh.faces.len(), short.faces.len());
    }
    for len in [0, 4, 83, 84, 100, 683] {
        assert!(load_stl_from_reader(Cursor::new(&binary[..len]), &path).is_err());
    }
    let mut corrupt = vec![0u8; 84];
    corrupt[80..84].copy_from_slice(&u32::MAX.to_le_bytes());
    assert!(load_stl_from_reader(Cursor::new(corrupt), &path).is_err());
}

// AI-FUNC-SUMMARY: Compare bounded folder loading/merge with a sequential directory-order reference, including face offsets and uppercase extensions.
#[test]
fn folder_batches_preserve_geometry_order() {
    use rustmspt::io::{load_folder_stls, load_stl_or_merge_folder};
    use rustmspt::geometry::merge_meshes;
    let temp = tempfile::tempdir().unwrap();
    for i in 0..7 {
        let path = temp.path().join(format!("shape-{i}.STL"));
        save_stl(&path, &box_mesh(BoundingBox { min: Vec3::new(i as f64 * 3.0, 0.0, 0.0), max: Vec3::new(i as f64 * 3.0 + 1.0, 2.0, 3.0) }), "shape").unwrap();
    }
    let paths: Vec<_> = std::fs::read_dir(temp.path()).unwrap().map(|p| p.unwrap().path()).collect();
    let expected: Vec<_> = paths.iter().map(|p| load_stl(p).unwrap()).collect();
    let individual = load_folder_stls(temp.path()).unwrap();
    assert_eq!(individual.iter().map(|(p, _)| p).collect::<Vec<_>>(), paths.iter().collect::<Vec<_>>());
    for ((_, actual), reference) in individual.iter().zip(&expected) { assert_eq!(actual.vertices, reference.vertices); }
    let merged = load_stl_or_merge_folder(temp.path()).unwrap();
    let reference = merge_meshes(&expected);
    assert_eq!(merged.vertices, reference.vertices);
    assert_eq!(merged.faces.iter().map(|f| (f.a, f.b, f.c)).collect::<Vec<_>>(), reference.faces.iter().map(|f| (f.a, f.b, f.c)).collect::<Vec<_>>());
}

use std::alloc::{GlobalAlloc, Layout, System};
use std::sync::atomic::{AtomicUsize, Ordering};

struct PeakAlloc;
static CURRENT: AtomicUsize = AtomicUsize::new(0);
static PEAK: AtomicUsize = AtomicUsize::new(0);

unsafe impl GlobalAlloc for PeakAlloc {
    // AI-FUNC-SUMMARY: Allocate through the system allocator while tracking live and peak bytes for the ignored ASCII memory benchmark.
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        let ptr = unsafe { System.alloc(layout) };
        if !ptr.is_null() {
            let now = CURRENT.fetch_add(layout.size(), Ordering::Relaxed) + layout.size();
            PEAK.fetch_max(now, Ordering::Relaxed);
        }
        ptr
    }
    // AI-FUNC-SUMMARY: Free through the system allocator and decrement the live-byte counter.
    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        unsafe { System.dealloc(ptr, layout) };
        CURRENT.fetch_sub(layout.size(), Ordering::Relaxed);
    }
}

#[global_allocator]
static GLOBAL: PeakAlloc = PeakAlloc;

type LegacyMesh = (Vec<[u64; 3]>, Vec<(usize, usize, usize)>);

// AI-FUNC-SUMMARY: Reproduce the former whole-text ASCII STL parse (lossy UTF-8 of the full file, lines, trim, vertex tokens, quantized dedup); returns bit-exact vertices and faces, or None when no triangle is found.
fn legacy_ascii(bytes: &[u8]) -> Option<LegacyMesh> {
    let sniff = String::from_utf8_lossy(&bytes[..bytes.len().min(512)]).to_ascii_lowercase();
    assert!(bytes.starts_with(b"solid") && sniff.contains("facet") && sniff.contains("vertex"));
    let text = String::from_utf8_lossy(bytes);
    let mut vertices: Vec<[u64; 3]> = Vec::new();
    let mut map = std::collections::HashMap::new();
    let mut faces = Vec::new();
    let mut pending: Vec<[f64; 3]> = Vec::new();
    for line in text.lines().map(str::trim) {
        let parts: Vec<&str> = line.split_whitespace().collect();
        if parts.len() != 4 || parts[0] != "vertex" {
            continue;
        }
        let parsed: Vec<f64> = match parts[1..].iter().map(|p| p.parse::<f64>()).collect() {
            Ok(v) => v,
            Err(_) => continue,
        };
        pending.push([parsed[0], parsed[1], parsed[2]]);
        if pending.len() == 3 {
            let mut idx = [0usize; 3];
            for (slot, v) in idx.iter_mut().zip(pending.drain(..)) {
                let key = ((v[0] * 1e6).round() as i64, (v[1] * 1e6).round() as i64, (v[2] * 1e6).round() as i64);
                *slot = *map.entry(key).or_insert_with(|| {
                    vertices.push([v[0].to_bits(), v[1].to_bits(), v[2].to_bits()]);
                    vertices.len() - 1
                });
            }
            faces.push((idx[0], idx[1], idx[2]));
        }
    }
    if faces.is_empty() { None } else { Some((vertices, faces)) }
}

// AI-FUNC-SUMMARY: Convert a parsed Mesh to bit-exact vertex/face tuples for comparison with the legacy oracle.
fn as_bits(mesh: &rustmspt::types::Mesh) -> LegacyMesh {
    (
        mesh.vertices.iter().map(|v| [v.x.to_bits(), v.y.to_bits(), v.z.to_bits()]).collect(),
        mesh.faces.iter().map(|f| (f.a, f.b, f.c)).collect(),
    )
}

// AI-FUNC-SUMMARY: Build a deterministic ASCII STL of `facets` triangles mixing CRLF, tabs, exponents and signs, optionally omitting endsolid.
fn generated_ascii(facets: usize, crlf: bool, endsolid: bool) -> Vec<u8> {
    let nl = if crlf { "\r\n" } else { "\n" };
    let mut out = format!("solid generated{nl}");
    for i in 0..facets {
        let a = (i as f64) * 0.37 - 11.0;
        let b = ((i * 7919) % 1000) as f64 * 1e-3;
        out.push_str(&format!("  facet normal 0 0 1{nl}    outer loop{nl}"));
        out.push_str(&format!("\tvertex {a:e} {b} -{}{nl}", i % 13));
        out.push_str(&format!("      vertex\t{} {:E} 1.5e+{}{nl}", a + 1.0, b * 3.0, i % 3));
        out.push_str(&format!("vertex {} {} {:.9}   {nl}", b, a, (i % 5) as f64 / 3.0));
        out.push_str(&format!("    endloop{nl}  endfacet{nl}"));
    }
    if endsolid {
        out.push_str(&format!("endsolid generated{nl}"));
    }
    out.into_bytes()
}

// AI-FUNC-SUMMARY: Check that streaming ASCII parsing equals the whole-text legacy parse (and its binary fallback) across CRLF, tabs, exponents, missing endsolid, unicode whitespace, invalid UTF-8, lone CR, partial facets and short reads, and that the hashing path digests every byte.
#[test]
fn ascii_stream_matches_whole_text_oracle() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("ascii.stl");
    let header = b"solid x\nfacet normal 0 0 0\nouter loop\n".to_vec();
    let mut cases: Vec<Vec<u8>> = vec![
        generated_ascii(40, false, true),
        generated_ascii(40, true, true),
        generated_ascii(40, false, false),
        generated_ascii(40, true, false),
        generated_ascii(3000, true, true),
        [header.clone(), b"vertex 1 2 3\nvertex 4 5 6\nvertex 7 8 9".to_vec()].concat(),
        [header.clone(), b"vertex 1 2 3\r\nvertex 4 5 6\r\nvertex 7 8 9\r".to_vec()].concat(),
        [header.clone(), "vertex\u{a0}1 2 3\nvertex 4\u{2003}5 6\nvertex 7 8 9\nvertex 1 1 1\n".as_bytes().to_vec()].concat(),
        [header.clone(), b"bad \xff\xfe utf8\nvertex 1e-3 -2E+2 +3\nvertex 1 0 0 extra\nvertex 0 1 0\nvertex 1 1 0\nvertex 0 0 1\n\n\n".to_vec()].concat(),
        [header.clone(), b"vertex 1 2 3\rvertex 4 5 6\rvertex 7 8 9\r".to_vec()].concat(),
        [header.clone(), b"VERTEX 1 2 3\nvertex 4 5 6\nvertex 7 8 9\n".to_vec()].concat(),
        [header.clone(), b"vertex 0 0 0\nvertex 0.0000001 0 0\nvertex 0 0.0000004 0\nvertex 1 1 1\nvertex 1 1 1\nvertex 1 1 1\n".to_vec()].concat(),
        [b"solid  \t\n\n".to_vec(), header[8..].to_vec(), b" vertex 1 2 3 \n vertex 4 5 6 \n vertex 7 8 9 \n".to_vec()].concat(),
    ];
    let long_line = [header.clone(), b"vertex tag\n".to_vec(), vec![b'a'; 200_000], b"\nvertex 1 2 3\nvertex 4 5 6\nvertex 7 8 9".to_vec()].concat();
    cases.push(long_line);
    for (index, bytes) in cases.iter().enumerate() {
        std::fs::write(&path, bytes).unwrap();
        let expected = legacy_ascii(bytes);
        let loaded = load_stl(&path);
        let short = load_stl_from_reader(ShortReads(Cursor::new(bytes)), &path);
        let hashed = load_stl_hashed(&path);
        match expected {
            Some(expected) => {
                assert_eq!(as_bits(loaded.as_ref().unwrap()), expected, "case {index}");
                assert_eq!(as_bits(short.as_ref().unwrap()), expected, "case {index}");
                let (mesh, digest, count) = hashed.unwrap();
                assert_eq!(as_bits(&mesh), expected, "case {index}");
                assert_eq!(digest, sha256_bytes(bytes));
                assert_eq!(count, bytes.len() as u64);
            }
            None => {
                assert!(loaded.is_err() && short.is_err() && hashed.is_err(), "case {index}");
            }
        }
    }
    let source = box_mesh(BoundingBox::from_size(Vec3::new(1.0, 2.0, 3.0)));
    save_stl(&path, &source, "solid facet vertex\nvertex 1 2 3\nvertex 4 5 6").unwrap();
    let binary = std::fs::read(&path).unwrap();
    assert!(legacy_ascii(&binary).is_none());
    let (mesh, digest, count) = load_stl_hashed(&path).unwrap();
    assert_eq!(mesh.faces.len(), source.faces.len());
    assert_eq!(digest, sha256_bytes(&binary));
    assert_eq!(count, binary.len() as u64);
    assert_eq!(as_bits(&load_stl_from_reader(ShortReads(Cursor::new(&binary)), &path).unwrap()), as_bits(&mesh));
}

// AI-FUNC-SUMMARY: Ignored release benchmark: compare peak live heap bytes and wall time of the former whole-text ASCII parse against streaming load_stl on a generated file.
#[test]
#[ignore]
fn ascii_stream_memory_benchmark() {
    let facets: usize = std::env::var("RUSTMSPT_ASCII_BENCH_FACETS").ok().and_then(|v| v.parse().ok()).unwrap_or(200_000);
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("bench.stl");
    std::fs::write(&path, generated_ascii(facets, true, true)).unwrap();
    let size = std::fs::metadata(&path).unwrap().len();
    for round in 0..5 {
        let base = CURRENT.load(Ordering::Relaxed);
        PEAK.store(base, Ordering::Relaxed);
        let start = std::time::Instant::now();
        let legacy = legacy_ascii(&std::fs::read(&path).unwrap()).unwrap();
        let legacy_time = start.elapsed().as_secs_f64();
        let legacy_peak = PEAK.load(Ordering::Relaxed) - base;
        drop(legacy);
        let base = CURRENT.load(Ordering::Relaxed);
        PEAK.store(base, Ordering::Relaxed);
        let start = std::time::Instant::now();
        let mesh = load_stl(&path).unwrap();
        let stream_time = start.elapsed().as_secs_f64();
        let stream_peak = PEAK.load(Ordering::Relaxed) - base;
        println!("round {round} file_bytes {size} facets {facets} vertices {} legacy_peak_bytes {legacy_peak} legacy_s {legacy_time:.4} stream_peak_bytes {stream_peak} stream_s {stream_time:.4}", mesh.vertices.len());
    }
}
