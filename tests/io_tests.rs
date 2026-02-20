use rustmspt::config::parse_box_dimensions;
use rustmspt::geometry::{box_mesh, split_mesh_into_granules};
use rustmspt::io::{
    load_raw_folder, load_stl, load_tiff_or_folder, save_stl, save_tiff_or_folder,
    save_tiff_or_folder_with_ext, ByteOrder, RawFolderSpec, Volume3D, VolumeNumericType,
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
