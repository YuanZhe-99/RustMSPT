use rustmspt::config::parse_box_dimensions;
use rustmspt::geometry::{box_mesh, split_mesh_into_granules};
use rustmspt::io::{
    load_raw_folder, load_stl, load_tiff_or_folder, save_stl, save_tiff_or_folder,
    save_tiff_or_folder_with_ext, sha256_bytes, sha256_file, ByteOrder, RawFolderSpec, Volume3D,
    VolumeNumericType,
};
use std::fs;

#[test]
fn binary_save_and_load_roundtrip() {
    let tmp = tempfile::tempdir().expect("tempdir should be created");
    let file = tmp.path().join("roundtrip_binary.stl");

    let bbox = parse_box_dimensions(&[0.0, 0.0, 0.0, 2.0, 2.0, 2.0]).expect("bbox parse should succeed");
    let mesh = box_mesh(bbox);

    save_stl(&file, &mesh, "roundtrip").expect("binary save should succeed");
    let loaded = load_stl(&file).expect("binary load should succeed");

    assert!(!loaded.vertices.is_empty());
    assert!(!loaded.faces.is_empty());
}

#[test]
fn ascii_is_auto_detected_on_load() {
    let tmp = tempfile::tempdir().expect("tempdir should be created");
    let file = tmp.path().join("simple_ascii.stl");

    let ascii = "solid simple\n  facet normal 0 0 0\n    outer loop\n      vertex 0 0 0\n      vertex 1 0 0\n      vertex 0 1 0\n    endloop\n  endfacet\nendsolid simple\n";
    fs::write(&file, ascii).expect("ascii stl write should succeed");

    let mesh = load_stl(&file).expect("ascii auto-detect load should succeed");
    assert_eq!(mesh.faces.len(), 1);
    assert_eq!(mesh.vertices.len(), 3);
}

#[test]
fn loaded_cube_splits_to_single_component() {
    let tmp = tempfile::tempdir().expect("tempdir should be created");
    let file = tmp.path().join("cube.stl");

    let bbox = parse_box_dimensions(&[0.0, 0.0, 0.0, 1.0, 1.0, 1.0]).expect("bbox parse should succeed");
    let mesh = box_mesh(bbox);
    save_stl(&file, &mesh, "cube").expect("cube save should succeed");

    let loaded = load_stl(&file).expect("cube load should succeed");
    let parts = split_mesh_into_granules(&loaded);
    assert_eq!(parts.len(), 1);
}

#[test]
fn raw_folder_loads_with_inclusive_range() {
    let tmp = tempfile::tempdir().expect("tempdir should be created");
    let folder = tmp.path().join("raw_slices");
    fs::create_dir_all(&folder).expect("folder should be created");

    fs::write(folder.join("000.raw"), vec![1u8, 2, 3, 4]).expect("slice write should succeed");
    fs::write(folder.join("001.raw"), vec![5u8, 6, 7, 8]).expect("slice write should succeed");
    fs::write(folder.join("002.raw"), vec![9u8, 10, 11, 12]).expect("slice write should succeed");

    let spec = RawFolderSpec {
        folder,
        width: 2,
        height: 2,
        bits: 8,
        signed: false,
        byte_order: ByteOrder::LittleEndian,
        slice_start: 1,
        slice_end: 2,
    };

    let vol = load_raw_folder(&spec).expect("raw folder load should succeed");
    assert_eq!(vol.width, 2);
    assert_eq!(vol.height, 2);
    assert_eq!(vol.depth, 2);
    assert_eq!(vol.numeric_type, VolumeNumericType::U8);
    assert_eq!(vol.data, vec![5, 6, 7, 8, 9, 10, 11, 12]);
}

#[test]
fn raw_folder_supports_big_endian() {
    let tmp = tempfile::tempdir().expect("tempdir should be created");
    let folder = tmp.path().join("raw_be");
    fs::create_dir_all(&folder).expect("folder should be created");

    fs::write(folder.join("000.raw"), vec![0x01u8, 0x02, 0x03, 0x04])
        .expect("slice write should succeed");

    let spec = RawFolderSpec {
        folder,
        width: 2,
        height: 1,
        bits: 16,
        signed: false,
        byte_order: ByteOrder::BigEndian,
        slice_start: -1,
        slice_end: -1,
    };

    let vol = load_raw_folder(&spec).expect("raw big-endian load should succeed");
    assert_eq!(vol.depth, 1);
    assert_eq!(vol.data, vec![0x0102, 0x0304]);
}

#[test]
fn tiff_file_and_folder_roundtrip() {
    let tmp = tempfile::tempdir().expect("tempdir should be created");
    let volume = Volume3D {
        width: 2,
        height: 2,
        depth: 3,
        data: vec![1, 2, 3, 4, 11, 12, 13, 14, 21, 22, 23, 24],
        numeric_type: VolumeNumericType::U16,
    };

    let single = tmp.path().join("stack.tiff");
    save_tiff_or_folder(&volume, &single, None).expect("single tiff save should succeed");
    let from_single = load_tiff_or_folder(&single).expect("single tiff load should succeed");
    assert_eq!(from_single.width, 2);
    assert_eq!(from_single.height, 2);
    assert_eq!(from_single.depth, 3);
    assert_eq!(from_single.numeric_type, VolumeNumericType::U16);
    assert_eq!(from_single.data, volume.data);

    let folder = tmp.path().join("tiff_slices");
    save_tiff_or_folder(&volume, &folder, Some("ct")).expect("folder tiff save should succeed");
    let from_folder = load_tiff_or_folder(&folder).expect("folder tiff load should succeed");
    assert_eq!(from_folder.width, 2);
    assert_eq!(from_folder.height, 2);
    assert_eq!(from_folder.depth, 3);
    assert_eq!(from_folder.numeric_type, VolumeNumericType::U16);
    assert_eq!(from_folder.data, volume.data);

    let folder_tif = tmp.path().join("tif_slices");
    save_tiff_or_folder_with_ext(&volume, &folder_tif, Some("ct"), Some("tif"))
        .expect("folder tif save should succeed");
    let from_tif_folder = load_tiff_or_folder(&folder_tif).expect("folder tif load should succeed");
    assert_eq!(from_tif_folder.width, 2);
    assert_eq!(from_tif_folder.height, 2);
    assert_eq!(from_tif_folder.depth, 3);
    assert_eq!(from_tif_folder.numeric_type, VolumeNumericType::U16);
    assert_eq!(from_tif_folder.data, volume.data);
}

// Published SHA-256 vectors (FIPS 180-4 / RFC 6234). Pinning them here means a
// wrong digest is caught by arithmetic, not by agreement with our own code.
#[test]
fn sha256_matches_the_published_vectors() {
    assert_eq!(
        sha256_bytes(b""),
        "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
    );
    assert_eq!(
        sha256_bytes(b"abc"),
        "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
    );
    assert_eq!(
        sha256_bytes(b"abcdbcdecdefdefgefghfghighijhijkijkljklmklmnlmnomnopnopq"),
        "248d6a61d20638b8e5c026930c3e6039a33ce45964ff2167f6ecedd419db06c1"
    );
    assert_eq!(sha256_bytes(b"abc").len(), 64);
    assert!(sha256_bytes(b"abc").chars().all(|c| c.is_ascii_hexdigit()));
}

// The streaming file hash must agree with the in-memory hash and report the
// number of bytes it actually hashed, including across its internal buffer
// boundary (64 KiB).
#[test]
fn sha256_file_agrees_with_sha256_bytes_across_the_buffer_boundary() {
    let tmp = tempfile::tempdir().expect("tempdir should be created");

    let empty = tmp.path().join("empty.bin");
    fs::write(&empty, b"").expect("write empty file");
    let (digest, bytes) = sha256_file(&empty).expect("hash empty file");
    assert_eq!(digest, sha256_bytes(b""));
    assert_eq!(bytes, 0);

    let small = tmp.path().join("small.bin");
    fs::write(&small, b"abc").expect("write small file");
    let (digest, bytes) = sha256_file(&small).expect("hash small file");
    assert_eq!(digest, sha256_bytes(b"abc"));
    assert_eq!(bytes, 3);

    // 200 000 bytes spans three reads of the 64 KiB buffer.
    let payload: Vec<u8> = (0..200_000u32).map(|i| (i % 251) as u8).collect();
    let large = tmp.path().join("large.bin");
    fs::write(&large, &payload).expect("write large file");
    let (digest, bytes) = sha256_file(&large).expect("hash large file");
    assert_eq!(digest, sha256_bytes(&payload));
    assert_eq!(bytes, payload.len() as u64);
}

// A missing file is an error, not a digest of nothing.
#[test]
fn sha256_file_errors_on_a_missing_file() {
    let tmp = tempfile::tempdir().expect("tempdir should be created");
    assert!(sha256_file(&tmp.path().join("absent.bin")).is_err());
}
