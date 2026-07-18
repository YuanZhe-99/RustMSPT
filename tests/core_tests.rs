use rustmspt::config::parse_box_dimensions;
use rustmspt::geometry::{
    box_mesh, l2_norm, merge_meshes, mesh_metrics, mesh_signed_volume, mesh_volume,
    orient_components_to_positive_volume, scale_mesh_to_equivalent_diameter,
    split_mesh_into_granules, translate_mesh,
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
    let bbox =
        parse_box_dimensions(&[0.0, 0.0, 0.0, 2.0, 3.0, 4.0]).expect("box parse should succeed");
    let mesh = box_mesh(bbox);
    let vol = mesh_volume(&mesh);
    assert!(vol > 0.0);
}

#[test]
fn mesh_metrics_match_known_box_values() {
    let bbox =
        parse_box_dimensions(&[0.0, 0.0, 0.0, 1.0, 1.0, 1.0]).expect("box parse should succeed");
    let mesh = box_mesh(bbox);
    let metrics = mesh_metrics(&mesh).expect("closed box should have valid metrics");
    assert!((metrics.volume - 1.0).abs() < 1e-12);
    assert!((metrics.surface_area - 6.0).abs() < 1e-12);
    assert!(
        (metrics.equivalent_diameter - (6.0 / std::f64::consts::PI).powf(1.0 / 3.0)).abs() < 1e-12
    );
    assert!((metrics.sphericity - (std::f64::consts::PI / 6.0).powf(1.0 / 3.0)).abs() < 1e-12);
}

#[test]
fn equivalent_diameter_scaling_preserves_sphericity() {
    let bbox =
        parse_box_dimensions(&[0.0, 0.0, 0.0, 1.0, 1.0, 1.0]).expect("box parse should succeed");
    let mut mesh = box_mesh(bbox);
    let before = mesh_metrics(&mesh).expect("closed box should have valid metrics");
    let target = before.equivalent_diameter * 2.5;
    let factor = scale_mesh_to_equivalent_diameter(&mut mesh, before, target)
        .expect("positive finite target should scale");
    assert!((factor - 2.5).abs() < 1e-12);
    let after = mesh_metrics(&mesh).expect("scaled box should have valid metrics");
    assert!((after.equivalent_diameter - target).abs() < 1e-12);
    assert!((after.volume - before.volume * factor.powi(3)).abs() < 1e-12);
    assert!((after.surface_area - before.surface_area * factor.powi(2)).abs() < 1e-12);
    assert!((after.sphericity - before.sphericity).abs() < 1e-12);
}

#[test]
fn mesh_metrics_reject_degenerate_mesh() {
    let mesh = rustmspt::types::Mesh {
        vertices: vec![Vec3::new(0.0, 0.0, 0.0)],
        faces: vec![rustmspt::types::Triangle { a: 0, b: 0, c: 0 }],
    };
    assert!(mesh_metrics(&mesh).is_none());
}

#[test]
fn mesh_metrics_reject_open_mesh() {
    let mesh = rustmspt::types::Mesh {
        vertices: vec![
            Vec3::new(0.0, 0.0, 0.0),
            Vec3::new(1.0, 0.0, 0.0),
            Vec3::new(0.0, 1.0, 0.0),
        ],
        faces: vec![rustmspt::types::Triangle { a: 0, b: 1, c: 2 }],
    };
    assert!(mesh_metrics(&mesh).is_none());
}

#[test]
fn mesh_metrics_reject_back_to_back_sheet_attached_to_closed_mesh() {
    let bbox =
        parse_box_dimensions(&[0.0, 0.0, 0.0, 1.0, 1.0, 1.0]).expect("box parse should succeed");
    let mut mesh = box_mesh(bbox);
    mesh.vertices.push(Vec3::new(-1.0, 0.0, 0.0));
    mesh.vertices.push(Vec3::new(0.0, -1.0, 0.0));
    mesh.faces
        .push(rustmspt::types::Triangle { a: 0, b: 8, c: 9 });
    mesh.faces
        .push(rustmspt::types::Triangle { a: 0, b: 9, c: 8 });
    assert!(mesh_metrics(&mesh).is_none());
}

#[test]
fn mesh_metrics_reject_zero_volume_closed_shell_attached_at_vertex() {
    let bbox =
        parse_box_dimensions(&[0.0, 0.0, 0.0, 1.0, 1.0, 1.0]).expect("box parse should succeed");
    let mut mesh = box_mesh(bbox);
    mesh.vertices.push(Vec3::new(2.0, 0.0, 0.0));
    mesh.vertices.push(Vec3::new(0.0, 2.0, 0.0));
    mesh.vertices.push(Vec3::new(2.0, 2.0, 0.0));
    mesh.faces.extend([
        rustmspt::types::Triangle { a: 0, b: 9, c: 8 },
        rustmspt::types::Triangle { a: 0, b: 8, c: 10 },
        rustmspt::types::Triangle { a: 8, b: 9, c: 10 },
        rustmspt::types::Triangle { a: 9, b: 0, c: 10 },
    ]);
    assert!(mesh_metrics(&mesh).is_none());
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
    let bbox =
        parse_box_dimensions(&[0.0, 0.0, 0.0, 1.0, 1.0, 1.0]).expect("box parse should succeed");
    let m1 = box_mesh(bbox);
    let mut m2 = box_mesh(bbox);
    translate_mesh(&mut m2, Vec3::new(10.0, 0.0, 0.0));

    let merged = merge_meshes(&[m1, m2]);
    let parts = split_mesh_into_granules(&merged);
    assert_eq!(parts.len(), 2);
}

#[test]
fn orientation_fix_makes_each_component_positive() {
    let bbox =
        parse_box_dimensions(&[0.0, 0.0, 0.0, 1.0, 1.0, 1.0]).expect("box parse should succeed");
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
