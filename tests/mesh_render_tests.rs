use rustmspt::config::MeshRenderConfig;
use rustmspt::geometry::render::{build_render_camera, RenderCameraSpec, RenderProjection};
use rustmspt::geometry::scene_render::{named_view, render_scene_cpu, SceneRenderSettings};
use rustmspt::io::vtu::{
    load_vtu, save_vtu, ArrayData, DataArray, VtuDoc, VtuEncoding, VTK_POLY_LINE, VTK_TETRA,
    VTK_TRIANGLE,
};
use rustmspt::meshgen::render_scene::{
    build_scene, RenderScene, SceneFilter, SceneSpec, SceneTri, SetKind,
};
use rustmspt::pipeline::mesh_render::MeshRenderPipeline;
use rustmspt::pipeline::Pipeline;
use rustmspt::types::{Mesh, Vec3};

fn contract_doc() -> VtuDoc {
    let points = vec![
        Vec3::new(0.0, 0.0, 0.0),
        Vec3::new(1.0, 0.0, 0.0),
        Vec3::new(0.0, 1.0, 0.0),
        Vec3::new(0.0, 0.0, 1.0),
        Vec3::new(1.0, 1.0, 1.0),
    ];
    let connectivity = vec![
        0, 1, 2, 3, // tet, region {1}
        1, 2, 3, 4, // tet, region {2}
        1, 2, 3, // tagged face on the shared face
        0, 4, // polyline
    ];
    let offsets = vec![4, 8, 11, 13];
    let types = vec![VTK_TETRA, VTK_TETRA, VTK_TRIANGLE, VTK_POLY_LINE];
    let cell_data = vec![
        DataArray::scalar("cell_kind", ArrayData::U8(vec![0, 0, 1, 2])),
        DataArray::scalar("region_key", ArrayData::I32(vec![1, 2, -1, -1])),
        DataArray::scalar("partition_id", ArrayData::I32(vec![1, 1, -1, -1])),
        DataArray::scalar("regime", ArrayData::U8(vec![0, 0, 255, 255])),
        DataArray::scalar("face_tag_key", ArrayData::I32(vec![-1, -1, 0, -1])),
        DataArray::scalar("curve_id", ArrayData::I32(vec![-1, -1, -1, 0])),
        DataArray::scalar("quality_demo", ArrayData::F32(vec![1.5, 12.0, -1.0, -1.0])),
    ];
    let point_data = vec![
        DataArray::scalar("n_id_key", ArrayData::I32(vec![0, 1, 1, 1, 2])),
        DataArray::scalar("constraint_kind", ArrayData::U8(vec![0, 1, 1, 1, 0])),
    ];
    let field_data = vec![
        DataArray::scalar("RegionSetOffsets", ArrayData::I64(vec![1, 2, 3])),
        DataArray::scalar("RegionSetComponents", ArrayData::I32(vec![0, 1, 2])),
        DataArray::scalar("RegionSetPriority", ArrayData::I32(vec![0, 1, 1])),
        DataArray::scalar("FaceTagOffsets", ArrayData::I64(vec![1])),
        DataArray::scalar("FaceTagComponents", ArrayData::I32(vec![1])),
        DataArray::scalar("FaceTagKind", ArrayData::U8(vec![0])),
        DataArray::scalar("CurveKind", ArrayData::U8(vec![2])),
        DataArray::scalar("CurveCompOffsets", ArrayData::I64(vec![2])),
        DataArray::scalar("CurveCompComponents", ArrayData::I32(vec![1, 2])),
        DataArray::scalar("SchemaVersion", ArrayData::I32(vec![1])),
    ];
    VtuDoc {
        points,
        connectivity,
        offsets,
        types,
        point_data,
        cell_data,
        field_data,
    }
}

#[test]
fn vtu_roundtrip_ascii_is_exact() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("mesh_ascii.vtu");
    let doc = contract_doc();
    save_vtu(&path, &doc, VtuEncoding::Ascii).unwrap();
    let loaded = load_vtu(&path).unwrap();
    assert_eq!(doc, loaded);
}

#[test]
fn vtu_roundtrip_appended_raw_is_exact() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("mesh_raw.vtu");
    let doc = contract_doc();
    save_vtu(&path, &doc, VtuEncoding::AppendedRaw).unwrap();
    let loaded = load_vtu(&path).unwrap();
    assert_eq!(doc, loaded);
}

#[test]
fn vtu_validate_rejects_out_of_range_connectivity() {
    let mut doc = contract_doc();
    doc.connectivity[0] = 99;
    assert!(doc.validate().is_err());
}

#[test]
fn extraction_counts_default_spec() {
    let doc = contract_doc();
    let scene = build_scene(&doc, &SceneSpec::default()).unwrap();
    // Two selected tets share one face: 2*4 - 2 = 6 boundary faces, plus 1 tagged face.
    assert_eq!(scene.tris.len(), 7);
    assert_eq!(
        scene.tris.iter().filter(|t| t.set == SetKind::Face).count(),
        1
    );
    // One 2-node polyline -> 1 segment.
    assert_eq!(scene.segments.len(), 1);
    assert!(scene.bbox.is_some());
}

#[test]
fn region_filter_exposes_interior_face() {
    let doc = contract_doc();
    let spec = SceneSpec {
        filters: vec![SceneFilter::RegionKey(vec![1])],
        ..SceneSpec::default()
    };
    let scene = build_scene(&doc, &spec).unwrap();
    // One selected tet -> all 4 of its faces are boundary now, plus the tagged face.
    assert_eq!(scene.tris.len(), 5);
}

#[test]
fn missing_array_filter_reports_array_name() {
    let doc = contract_doc();
    let spec = SceneSpec {
        filters: vec![SceneFilter::ArrayRange {
            array: "aspect_ratio".to_string(),
            min: 0.0,
            max: 10.0,
        }],
        ..SceneSpec::default()
    };
    let err = build_scene(&doc, &spec).unwrap_err().to_string();
    assert!(err.contains("aspect_ratio"), "error should name the array: {err}");
}

#[test]
fn all_named_views_resolve() {
    for name in [
        "front", "back", "left", "right", "top", "bottom", "iso_ne", "iso_nw", "iso_se", "iso_sw",
    ] {
        assert!(named_view(name).is_some(), "view {name} must resolve");
    }
    assert!(named_view("sideways").is_none());
    let (dir, up) = named_view("front").unwrap();
    assert_eq!(dir, [0.0, 1.0, 0.0]);
    assert_eq!(up, [0.0, 0.0, 1.0]);
}

fn quad(x: f64, color: [u8; 3], alpha: f64, set: SetKind) -> Vec<SceneTri> {
    let (lo, hi) = (-5.0, 5.0);
    let p = |y: f64, z: f64| Vec3::new(x, y, z);
    vec![
        SceneTri { a: p(lo, lo), b: p(hi, lo), c: p(hi, hi), color, alpha, set },
        SceneTri { a: p(lo, lo), b: p(hi, hi), c: p(lo, hi), color, alpha, set },
    ]
}

fn ortho_camera_along_x(width: usize, height: usize) -> rustmspt::geometry::render::RenderCamera {
    let corner_mesh = Mesh {
        vertices: vec![Vec3::new(0.0, -5.0, -5.0), Vec3::new(3.0, 5.0, 5.0)],
        faces: Vec::new(),
    };
    let spec = RenderCameraSpec {
        focus_point: [1.5, 0.0, 0.0],
        view_direction: [1.0, 0.0, 0.0],
        up_vector: Some([0.0, 0.0, 1.0]),
        projection: RenderProjection::Orthographic,
        perspective_fov_degrees: 45.0,
        camera_distance: None,
        fit_padding: 0.0,
        width,
        height,
    };
    build_render_camera(&corner_mesh, &spec).unwrap()
}

#[test]
fn transparency_compositing_matches_analytic_result() {
    // Front quad at x=1: red, alpha 0.5. Back quad at x=2: green, alpha 1.0.
    // Quad normals are parallel to the ray, so headlight intensity is exactly 1.
    // Expected center pixel: 0.5*red + 0.5*green.
    let mut scene = RenderScene::default();
    scene.tris.extend(quad(1.0, [200, 0, 0], 0.5, SetKind::Volume));
    scene.tris.extend(quad(2.0, [0, 100, 0], 1.0, SetKind::Volume));
    let (w, h) = (64, 64);
    let camera = ortho_camera_along_x(w, h);
    let settings = SceneRenderSettings { background: [0, 0, 255, 255], ambient: 0.25 };
    let image = render_scene_cpu(&scene, &camera, w, h, &settings);
    let center = ((h / 2) * w + w / 2) * 4;
    let px = &image.rgba[center..center + 4];
    assert!((px[0] as i32 - 100).abs() <= 2, "red {} != ~100", px[0]);
    assert!((px[1] as i32 - 50).abs() <= 2, "green {} != ~50", px[1]);
    assert_eq!(px[2], 0, "back quad is opaque; background must not leak");
    assert_eq!(px[3], 255);
}

#[test]
fn coincident_face_set_wins_over_volume_set() {
    // A Face-set triangle pair coincident with a Volume-set pair: the face color
    // must win and compositing must not double-count the coincident geometry.
    let mut scene = RenderScene::default();
    scene.tris.extend(quad(1.0, [10, 10, 10], 1.0, SetKind::Volume));
    scene.tris.extend(quad(1.0, [0, 200, 200], 1.0, SetKind::Face));
    let (w, h) = (32, 32);
    let camera = ortho_camera_along_x(w, h);
    let settings = SceneRenderSettings { background: [255, 255, 255, 255], ambient: 0.25 };
    let image = render_scene_cpu(&scene, &camera, w, h, &settings);
    let center = ((h / 2) * w + w / 2) * 4;
    let px = &image.rgba[center..center + 4];
    assert!(px[1] > 150 && px[2] > 150, "face color must win: {px:?}");
}

#[test]
fn transparent_background_alpha_is_preserved() {
    let scene = RenderScene::default();
    let (w, h) = (16, 16);
    let camera = ortho_camera_along_x(w, h);
    let settings = SceneRenderSettings { background: [0, 0, 0, 0], ambient: 0.25 };
    let image = render_scene_cpu(&scene, &camera, w, h, &settings);
    assert!(image.rgba.chunks(4).all(|p| p[3] == 0));
}

#[test]
fn mesh_render_pipeline_end_to_end() {
    let dir = tempfile::tempdir().unwrap();
    let vtu_path = dir.path().join("fixture.vtu");
    save_vtu(&vtu_path, &contract_doc(), VtuEncoding::AppendedRaw).unwrap();
    let out_dir = dir.path().join("render");

    let yaml = format!(
        r#"
mesh_render:
  input: {}
  output_dir: {}
  views:
    - iso_ne
    - front
    - name: oblique
      view_direction: [-1.0, -0.5, -0.25]
  width: 96
  height: 80
  background: [255, 255, 255, 0]
  color_by: region_key
  opacity_overrides: {{ "2": 0.5 }}
  filters:
    - {{ kind: cell_kind, values: [0, 1, 2] }}
  wireframe: true
"#,
        vtu_path.display(),
        out_dir.display()
    );
    let config: MeshRenderConfig = serde_yaml::from_str(&yaml).unwrap();
    MeshRenderPipeline { config }.run().unwrap();

    for view in ["iso_ne", "front", "oblique"] {
        let path = out_dir.join(format!("fixture_{view}.png"));
        let img = image::open(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
        let rgba = img.to_rgba8();
        assert_eq!((rgba.width(), rgba.height()), (96, 80));
        let colored = rgba.pixels().filter(|p| p.0[3] > 0).count();
        let transparent = rgba.pixels().filter(|p| p.0[3] == 0).count();
        assert!(colored > 50, "{view}: expected visible geometry, got {colored} px");
        assert!(transparent > 50, "{view}: expected transparent background, got {transparent} px");
    }
}

#[test]
fn mesh_render_pipeline_rejects_unknown_view() {
    let dir = tempfile::tempdir().unwrap();
    let vtu_path = dir.path().join("fixture.vtu");
    save_vtu(&vtu_path, &contract_doc(), VtuEncoding::Ascii).unwrap();
    let yaml = format!(
        "mesh_render:\n  input: {}\n  output_dir: {}\n  views: [sideways]\n",
        vtu_path.display(),
        dir.path().join("out").display()
    );
    let config: MeshRenderConfig = serde_yaml::from_str(&yaml).unwrap();
    let err = MeshRenderPipeline { config }.run().unwrap_err().to_string();
    assert!(err.contains("sideways"), "error should name the bad view: {err}");
}
