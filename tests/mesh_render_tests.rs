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
    assert!(
        err.contains("aspect_ratio"),
        "error should name the array: {err}"
    );
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
        SceneTri {
            a: p(lo, lo),
            b: p(hi, lo),
            c: p(hi, hi),
            color,
            alpha,
            set,
        },
        SceneTri {
            a: p(lo, lo),
            b: p(hi, hi),
            c: p(lo, hi),
            color,
            alpha,
            set,
        },
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
    scene
        .tris
        .extend(quad(1.0, [200, 0, 0], 0.5, SetKind::Volume));
    scene
        .tris
        .extend(quad(2.0, [0, 100, 0], 1.0, SetKind::Volume));
    let (w, h) = (64, 64);
    let camera = ortho_camera_along_x(w, h);
    let settings = SceneRenderSettings {
        background: [0, 0, 255, 255],
        ambient: 0.25,
    };
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
    scene
        .tris
        .extend(quad(1.0, [10, 10, 10], 1.0, SetKind::Volume));
    scene
        .tris
        .extend(quad(1.0, [0, 200, 200], 1.0, SetKind::Face));
    let (w, h) = (32, 32);
    let camera = ortho_camera_along_x(w, h);
    let settings = SceneRenderSettings {
        background: [255, 255, 255, 255],
        ambient: 0.25,
    };
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
    let settings = SceneRenderSettings {
        background: [0, 0, 0, 0],
        ambient: 0.25,
    };
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
        assert!(
            colored > 50,
            "{view}: expected visible geometry, got {colored} px"
        );
        assert!(
            transparent > 50,
            "{view}: expected transparent background, got {transparent} px"
        );
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
    assert!(
        err.contains("sideways"),
        "error should name the bad view: {err}"
    );
}

// ---------------------------------------------------------------- GA-3c: GPU preview
//
// The GPU path is an *opaque* preview by design (PLAN §9.5): exact transparency lives in
// the CPU reference. These tests therefore compare on opaque scenes, and check the GPU-only
// features (per-vertex colour, LineList overlay, clip-plane discard, batch views) directly.

#[cfg(feature = "gpu")]
fn opaque_scene() -> RenderScene {
    // two axis-aligned quads at different depths, distinct colours, fully opaque
    let mut tris = quad(0.0, [220, 60, 40], 1.0, SetKind::Volume);
    tris.extend(quad(2.0, [40, 90, 220], 1.0, SetKind::Face));
    RenderScene {
        tris,
        segments: Vec::new(),
        markers: Vec::new(),
        bbox: Some(rustmspt::types::BoundingBox {
            min: Vec3::new(0.0, -5.0, -5.0),
            max: Vec3::new(2.0, 5.0, 5.0),
        }),
    }
}

#[cfg(feature = "gpu")]
fn box_scene() -> RenderScene {
    // closed axis-aligned box, one colour per face pair, so every named view shows
    // something and no two views coincide
    let (lo, hi) = (Vec3::new(0.0, 0.0, 0.0), Vec3::new(2.0, 1.0, 1.5));
    let v = |i: usize| {
        Vec3::new(
            if i & 1 == 0 { lo.x } else { hi.x },
            if i & 2 == 0 { lo.y } else { hi.y },
            if i & 4 == 0 { lo.z } else { hi.z },
        )
    };
    let faces: [([usize; 4], [u8; 3]); 6] = [
        ([0, 2, 6, 4], [220, 60, 40]),
        ([1, 5, 7, 3], [40, 90, 220]),
        ([0, 4, 5, 1], [60, 180, 75]),
        ([2, 3, 7, 6], [240, 200, 40]),
        ([0, 1, 3, 2], [150, 60, 200]),
        ([4, 6, 7, 5], [40, 200, 200]),
    ];
    let mut tris = Vec::new();
    for (q, color) in faces {
        for (a, b, c) in [(q[0], q[1], q[2]), (q[0], q[2], q[3])] {
            tris.push(SceneTri {
                a: v(a),
                b: v(b),
                c: v(c),
                color,
                alpha: 1.0,
                set: SetKind::Volume,
            });
        }
    }
    RenderScene {
        tris,
        segments: Vec::new(),
        markers: Vec::new(),
        bbox: Some(rustmspt::types::BoundingBox { min: lo, max: hi }),
    }
}

#[cfg(feature = "gpu")]
fn try_gpu() -> Option<rustmspt::gpu::GpuScenePipeline> {
    match rustmspt::gpu::GpuScenePipeline::new() {
        Ok(p) => Some(p),
        Err(e) => {
            println!("Skipping GPU scene test (no GPU available): {e}");
            None
        }
    }
}

#[test]
#[cfg(feature = "gpu")]
fn gpu_scene_matches_cpu_within_tolerance() {
    let scene = opaque_scene();
    let settings = SceneRenderSettings::default();
    let options = rustmspt::gpu::GpuSceneOptions::default();
    let (w, h) = (128usize, 128usize);

    for projection in [
        RenderProjection::Orthographic,
        RenderProjection::Perspective,
    ] {
        let mesh = Mesh {
            vertices: vec![Vec3::new(0.0, -5.0, -5.0), Vec3::new(2.0, 5.0, 5.0)],
            faces: Vec::new(),
        };
        let camera = build_render_camera(
            &mesh,
            &RenderCameraSpec {
                focus_point: [1.0, 0.0, 0.0],
                view_direction: [0.4, 0.3, -1.0],
                up_vector: Some([0.0, 0.0, 1.0]),
                projection,
                perspective_fov_degrees: 45.0,
                camera_distance: None,
                fit_padding: 0.1,
                width: w,
                height: h,
            },
        )
        .expect("camera should build");

        let cpu = render_scene_cpu(&scene, &camera, w, h, &settings);
        let Some(mut gpu_pipeline) = try_gpu() else {
            return;
        };
        let gpu = gpu_pipeline
            .render(&scene, &camera, w, h, &settings, &options)
            .expect("gpu render should succeed");

        assert_eq!(cpu.rgba.len(), gpu.rgba.len());
        let mut mismatched = 0usize;
        let mut max_channel_diff = 0i32;
        for (c, g) in cpu.rgba.chunks_exact(4).zip(gpu.rgba.chunks_exact(4)) {
            let diff = (0..4)
                .map(|k| (c[k] as i32 - g[k] as i32).abs())
                .max()
                .unwrap_or(0);
            max_channel_diff = max_channel_diff.max(diff);
            if diff > 2 {
                mismatched += 1;
            }
        }
        let total = w * h;
        assert!(
            mismatched * 100 <= total * 2,
            "{projection:?}: {mismatched}/{total} pixels differ by >2 (max channel diff {max_channel_diff})"
        );
    }
}

#[test]
#[cfg(feature = "gpu")]
fn gpu_per_vertex_color_reproduces_scene_colors() {
    // one quad per colour, side by side: the preview must paint each with its own colour
    // rather than a single uniform base colour.
    let scene = opaque_scene();
    let settings = SceneRenderSettings::default();
    let (w, h) = (96usize, 96usize);
    let camera = ortho_camera_along_x(w, h);
    let Some(mut gpu_pipeline) = try_gpu() else {
        return;
    };
    let gpu = gpu_pipeline
        .render(
            &scene,
            &camera,
            w,
            h,
            &settings,
            &rustmspt::gpu::GpuSceneOptions::default(),
        )
        .expect("gpu render should succeed");

    // the near quad (red) faces the camera and must dominate the image centre
    let centre = ((h / 2) * w + w / 2) * 4;
    let (r, g, b) = (gpu.rgba[centre], gpu.rgba[centre + 1], gpu.rgba[centre + 2]);
    assert!(
        r > g && r > b,
        "expected the red quad at the centre, got ({r}, {g}, {b})"
    );
    assert!(r > 100, "expected a lit red, got {r}");
}

#[test]
#[cfg(feature = "gpu")]
fn gpu_line_pipeline_draws_overlay_segments() {
    let mut scene = opaque_scene();
    scene.segments.push(SceneSegment {
        a: Vec3::new(-1.0, -4.0, 0.0),
        b: Vec3::new(-1.0, 4.0, 0.0),
        color: [0, 255, 0],
    });
    let settings = SceneRenderSettings::default();
    let (w, h) = (96usize, 96usize);
    let camera = ortho_camera_along_x(w, h);
    let Some(mut gpu_pipeline) = try_gpu() else {
        return;
    };

    let with_lines = gpu_pipeline
        .render(
            &scene,
            &camera,
            w,
            h,
            &settings,
            &rustmspt::gpu::GpuSceneOptions::with_overlays(),
        )
        .expect("gpu render should succeed");
    let without = gpu_pipeline
        .render(
            &scene,
            &camera,
            w,
            h,
            &settings,
            &rustmspt::gpu::GpuSceneOptions::default(),
        )
        .expect("gpu render should succeed");

    let green = |img: &rustmspt::types::RenderedImage| {
        img.rgba
            .chunks_exact(4)
            .filter(|p| p[1] > 200 && p[0] < 100 && p[2] < 100)
            .count()
    };
    assert!(green(&with_lines) > 0, "the LineList pass drew nothing");
    assert_eq!(
        green(&without),
        0,
        "segments must be off when not requested"
    );
}

#[test]
#[cfg(feature = "gpu")]
fn gpu_clip_plane_discards_the_positive_side() {
    let scene = opaque_scene();
    let settings = SceneRenderSettings::default();
    let (w, h) = (96usize, 96usize);
    let camera = ortho_camera_along_x(w, h);
    let Some(mut gpu_pipeline) = try_gpu() else {
        return;
    };

    let full = gpu_pipeline
        .render(
            &scene,
            &camera,
            w,
            h,
            &settings,
            &rustmspt::gpu::GpuSceneOptions::default(),
        )
        .expect("gpu render should succeed");
    // clip everything with y > 0
    let clipped = gpu_pipeline
        .render(
            &scene,
            &camera,
            w,
            h,
            &settings,
            &rustmspt::gpu::GpuSceneOptions {
                clip_plane: Some(rustmspt::gpu::GpuClipPlane {
                    origin: Vec3::new(0.0, 0.0, 0.0),
                    normal: Vec3::new(0.0, 1.0, 0.0),
                }),
                ..Default::default()
            },
        )
        .expect("gpu render should succeed");

    let background = |img: &rustmspt::types::RenderedImage| {
        img.rgba
            .chunks_exact(4)
            .filter(|p| p[0] == 255 && p[1] == 255 && p[2] == 255)
            .count()
    };
    let (b_full, b_clipped) = (background(&full), background(&clipped));
    assert!(
        b_clipped > b_full,
        "clipping must expose background: {b_full} -> {b_clipped}"
    );
    // roughly half the covered pixels should survive; allow a wide band for framing
    let covered_full = w * h - b_full;
    let covered_clipped = w * h - b_clipped;
    assert!(
        covered_clipped * 4 < covered_full * 3 && covered_clipped > covered_full / 8,
        "expected about half the coverage to survive: {covered_full} -> {covered_clipped}"
    );
}

#[test]
#[cfg(feature = "gpu")]
fn gpu_batch_views_match_individual_renders() {
    let scene = box_scene();
    let settings = SceneRenderSettings::default();
    let options = rustmspt::gpu::GpuSceneOptions::default();
    let (w, h) = (64usize, 64usize);
    let mesh = Mesh {
        vertices: vec![Vec3::new(0.0, 0.0, 0.0), Vec3::new(2.0, 1.0, 1.5)],
        faces: Vec::new(),
    };
    let cameras: Vec<_> = ["front", "top", "iso_ne"]
        .iter()
        .map(|name| {
            let (dir, up) = named_view(name).unwrap();
            build_render_camera(
                &mesh,
                &RenderCameraSpec {
                    focus_point: [1.0, 0.5, 0.75],
                    view_direction: dir,
                    up_vector: Some(up),
                    projection: RenderProjection::Orthographic,
                    perspective_fov_degrees: 45.0,
                    camera_distance: None,
                    fit_padding: 0.05,
                    width: w,
                    height: h,
                },
            )
            .unwrap()
        })
        .collect();

    let Some(mut gpu_pipeline) = try_gpu() else {
        return;
    };
    let batch = gpu_pipeline
        .render_views(&scene, &cameras, w, h, &settings, &options)
        .expect("batch render should succeed");
    assert_eq!(batch.len(), cameras.len());

    for (i, camera) in cameras.iter().enumerate() {
        let single = gpu_pipeline
            .render(&scene, camera, w, h, &settings, &options)
            .expect("single render should succeed");
        assert_eq!(
            batch[i].rgba, single.rgba,
            "batch view {i} differs from the same camera rendered alone"
        );
    }
    // the three views must not all be identical, or the batch is not varying the camera
    assert_ne!(batch[0].rgba, batch[1].rgba);
}

#[test]
fn mesh_render_backend_selection() {
    let dir = tempfile::tempdir().unwrap();
    let vtu_path = dir.path().join("fixture.vtu");
    save_vtu(&vtu_path, &contract_doc(), VtuEncoding::AppendedRaw).unwrap();

    let config_for = |backend: &str, out: &std::path::Path| -> MeshRenderConfig {
        let yaml = format!(
            r#"
mesh_render:
  input: {}
  output_dir: {}
  views: [front]
  width: 32
  height: 32
  backend: {}
"#,
            vtu_path.display(),
            out.display(),
            backend
        );
        serde_yaml::from_str(&yaml).unwrap()
    };

    // the default is the CPU reference renderer
    let default_cfg: MeshRenderConfig = serde_yaml::from_str(&format!(
        "mesh_render:\n  input: {}\n  output_dir: {}\n",
        vtu_path.display(),
        dir.path().join("d").display()
    ))
    .unwrap();
    assert_eq!(default_cfg.mesh_render.backend, "cpu");

    // `auto` always produces an image: it uses the GPU when one is available and
    // silently falls back to the CPU renderer when it is not
    let auto_out = dir.path().join("auto");
    MeshRenderPipeline {
        config: config_for("auto", &auto_out),
    }
    .run()
    .expect("auto backend must always succeed");
    assert!(auto_out.join("fixture_front.png").exists());

    // an unknown backend is a config error naming the accepted values
    let bad_out = dir.path().join("bad");
    let err = MeshRenderPipeline {
        config: config_for("quantum", &bad_out),
    }
    .run()
    .expect_err("unknown backend must be rejected");
    let msg = err.to_string();
    assert!(
        msg.contains("quantum") && msg.contains("cpu"),
        "unhelpful error: {msg}"
    );
}
