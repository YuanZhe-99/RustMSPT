use rustmspt::config::parse_box_dimensions;
use rustmspt::geometry::{box_mesh, split_mesh_into_granules};
use rustmspt::io::{load_stl, save_stl};
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
