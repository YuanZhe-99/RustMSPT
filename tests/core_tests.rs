use rustmspt::config::parse_box_dimensions;
use rustmspt::geometry::{
    box_mesh, l2_norm, merge_meshes, mesh_signed_volume, mesh_volume,
    orient_components_to_positive_volume, split_mesh_into_granules, translate_mesh,
};
use rustmspt::types::Vec3;

#[test]
fn parse_box_dimensions_size_mode() {
    let bbox = parse_box_dimensions(&[10.0, 20.0, 30.0]).expect("box parse should succeed");
    assert_eq!(bbox.min, Vec3::new(0.0, 0.0, 0.0));
    assert_eq!(bbox.max, Vec3::new(10.0, 20.0, 30.0));
}

#[test]
fn box_mesh_volume_is_positive() {
    let bbox = parse_box_dimensions(&[0.0, 0.0, 0.0, 2.0, 3.0, 4.0]).expect("box parse should succeed");
    let mesh = box_mesh(bbox);
    let vol = mesh_volume(&mesh);
    assert!(vol > 0.0);
}

#[test]
fn l2_norm_works() {
    let a = [1.0, 2.0, 3.0];
    let b = [1.0, 2.0, 5.0];
    let v = l2_norm(&a, &b);
    assert!((v - 2.0).abs() < 1e-9);
}

#[test]
fn split_mesh_into_two_components() {
    let bbox = parse_box_dimensions(&[0.0, 0.0, 0.0, 1.0, 1.0, 1.0]).expect("box parse should succeed");
    let m1 = box_mesh(bbox);
    let mut m2 = box_mesh(bbox);
    translate_mesh(&mut m2, Vec3::new(10.0, 0.0, 0.0));

    let merged = merge_meshes(&[m1, m2]);
    let parts = split_mesh_into_granules(&merged);
    assert_eq!(parts.len(), 2);
}

#[test]
fn orientation_fix_makes_each_component_positive() {
    let bbox = parse_box_dimensions(&[0.0, 0.0, 0.0, 1.0, 1.0, 1.0]).expect("box parse should succeed");
    let mut a = box_mesh(bbox);
    for f in &mut a.faces {
        std::mem::swap(&mut f.b, &mut f.c);
    }

    let mut b = a.clone();
    translate_mesh(&mut b, Vec3::new(3.0, 0.0, 0.0));
    let merged = merge_meshes(&[a, b]);

    let before_parts = split_mesh_into_granules(&merged);
    let expected_flipped = before_parts
        .iter()
        .filter(|p| mesh_signed_volume(p) < 0.0)
        .count();

    let (fixed, flipped, total) = orient_components_to_positive_volume(&merged);
    assert_eq!(total, 2);
    assert_eq!(flipped, expected_flipped);

    let parts = split_mesh_into_granules(&fixed);
    assert_eq!(parts.len(), 2);
    for p in parts {
        assert!(mesh_signed_volume(&p) > 0.0);
    }
}
