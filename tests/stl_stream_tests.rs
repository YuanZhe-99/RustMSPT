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
