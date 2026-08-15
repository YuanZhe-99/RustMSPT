//! GA-5 visual QA and committed image-regression baselines.
//!
//! CPU is the transparency reference. The committed matrix is one fixed
//! contract fixture (`good_cube.vtu`) x four appearance/filter variants x all
//! ten named views. GPU tests compare the three opaque variants against those
//! same references and skip cleanly without an adapter. Regenerate explicitly:
//! `RUSTMSPT_UPDATE_RENDER_BASELINES=1 cargo test --test mesh_visual_regression_tests`.

use rustmspt::geometry::render::{build_render_camera, RenderCameraSpec, RenderProjection};
use rustmspt::geometry::scene_render::{named_view, render_scene_cpu, SceneRenderSettings};
use rustmspt::io::{load_vtu, save_image, VtuDoc};
use rustmspt::meshgen::{build_scene, ColorMode, SceneFilter, SceneSpec};
use rustmspt::types::{BoundingBox, Mesh, RenderedImage, Vec3};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

const WIDTH: usize = 96;
const HEIGHT: usize = 96;
const VIEWS: [&str; 10] = [
    "front", "back", "left", "right", "top", "bottom", "iso_ne", "iso_nw", "iso_se", "iso_sw",
];

#[derive(Clone, Copy, Debug)]
enum Variant {
    Opaque,
    Filtered,
    Transparent,
    Clipped,
}

impl Variant {
    // AI-FUNC-SUMMARY: Stable baseline filename token; returns &'static str; side effects: none.
    fn name(self) -> &'static str {
        match self {
            Variant::Opaque => "opaque",
            Variant::Filtered => "filtered_region_1",
            Variant::Transparent => "transparent_exterior",
            Variant::Clipped => "clipped_x",
        }
    }

    // AI-FUNC-SUMMARY: True when CPU/GPU comparison is valid (GPU preview is opaque-only); returns bool; side effects: none.
    #[cfg(feature = "gpu")]
    fn gpu_comparable(self) -> bool {
        !matches!(self, Variant::Transparent)
    }

    // AI-FUNC-SUMMARY: Build the extraction/appearance specification for one visual-regression variant; returns SceneSpec; side effects: none.
    fn scene_spec(self) -> SceneSpec {
        let mut spec = SceneSpec {
            color_mode: ColorMode::Categorical {
                array: "region_key".to_string(),
            },
            show_faces: true,
            show_curves: true,
            ..SceneSpec::default()
        };
        match self {
            Variant::Opaque => {
                spec.wireframe = true;
            }
            Variant::Filtered => {
                spec.filters = vec![SceneFilter::RegionKey(vec![1])];
                spec.wireframe = true;
            }
            Variant::Transparent => {
                spec.volume_opacity = 0.30;
                spec.face_opacity = 1.0;
                spec.opacity_overrides = HashMap::from([(0, 0.12), (1, 0.35)]);
            }
            Variant::Clipped => {
                spec.filters = vec![SceneFilter::ClipPlane {
                    origin: Vec3::new(0.5, 0.5, 0.5),
                    normal: Vec3::new(1.0, 0.0, 0.0),
                }];
                spec.wireframe = true;
            }
        }
        spec
    }

    // AI-FUNC-SUMMARY: Scene-render settings for this variant; returns SceneRenderSettings; side effects: none.
    fn settings(self) -> SceneRenderSettings {
        SceneRenderSettings {
            background: if matches!(self, Variant::Transparent) {
                [255, 255, 255, 0]
            } else {
                [248, 249, 251, 255]
            },
            ambient: 0.28,
        }
    }
}

const VARIANTS: [Variant; 4] = [
    Variant::Opaque,
    Variant::Filtered,
    Variant::Transparent,
    Variant::Clipped,
];

// AI-FUNC-SUMMARY: Path to the committed GA-5 CPU reference PNG directory; returns PathBuf; side effects: none.
fn baseline_dir() -> PathBuf {
    PathBuf::from("data/fixtures/meshgen/render_baselines/cpu")
}

// AI-FUNC-SUMMARY: Stable path for one variant/view baseline PNG; returns PathBuf; side effects: none.
fn baseline_path(variant: Variant, view: &str) -> PathBuf {
    baseline_dir().join(format!("good_cube_{}_{}.png", variant.name(), view))
}

// AI-FUNC-SUMMARY: Convert a scene bbox to the corner-only Mesh expected by camera auto-fit; returns Mesh; side effects: none.
fn corner_mesh(bbox: BoundingBox) -> Mesh {
    Mesh {
        vertices: vec![
            bbox.min,
            Vec3::new(bbox.max.x, bbox.min.y, bbox.min.z),
            Vec3::new(bbox.min.x, bbox.max.y, bbox.min.z),
            Vec3::new(bbox.min.x, bbox.min.y, bbox.max.z),
            Vec3::new(bbox.max.x, bbox.max.y, bbox.min.z),
            Vec3::new(bbox.max.x, bbox.min.y, bbox.max.z),
            Vec3::new(bbox.min.x, bbox.max.y, bbox.max.z),
            bbox.max,
        ],
        faces: Vec::new(),
    }
}

// AI-FUNC-SUMMARY: Build one named orthographic camera with stable framing from the full-document bbox; returns RenderCamera; side effects: none.
fn camera_for(bbox: BoundingBox, view: &str) -> rustmspt::geometry::render::RenderCamera {
    let (direction, up) = named_view(view).expect("known GA-5 view");
    let center = bbox.min.add(bbox.max).scale(0.5);
    build_render_camera(
        &corner_mesh(bbox),
        &RenderCameraSpec {
            focus_point: [center.x, center.y, center.z],
            view_direction: direction,
            up_vector: Some(up),
            projection: RenderProjection::Orthographic,
            perspective_fov_degrees: 45.0,
            camera_distance: None,
            fit_padding: 0.08,
            width: WIDTH,
            height: HEIGHT,
        },
    )
    .expect("GA-5 camera builds")
}

// AI-FUNC-SUMMARY: Render one committed CPU-reference case; returns RenderedImage; side effects: none.
fn render_cpu_case(doc: &VtuDoc, variant: Variant, view: &str) -> RenderedImage {
    let scene = build_scene(doc, &variant.scene_spec()).expect("GA-5 scene extracts");
    let bbox = scene.bbox.expect("fixture has a bbox");
    let camera = camera_for(bbox, view);
    render_scene_cpu(&scene, &camera, WIDTH, HEIGHT, &variant.settings())
}

// AI-FUNC-SUMMARY: Load a PNG as the renderer's top-row-first RGBA8 contract; returns RenderedImage; side effects: reads disk.
fn load_png(path: &Path) -> RenderedImage {
    let image = image::open(path)
        .unwrap_or_else(|e| panic!("missing/invalid GA-5 baseline {}: {e}", path.display()))
        .to_rgba8();
    RenderedImage {
        width: image.width() as usize,
        height: image.height() as usize,
        rgba: image.into_raw(),
    }
}

// AI-FUNC-SUMMARY: Compare RGBA images with the frozen GA-5 tolerance (>2 LSB allowed on a bounded pixel share); returns none; side effects: panics on failure.
fn assert_image_matches(
    actual: &RenderedImage,
    expected: &RenderedImage,
    label: &str,
    allowed_percent: usize,
) {
    assert_eq!(
        (actual.width, actual.height),
        (expected.width, expected.height),
        "{label}: dimensions"
    );
    let mut mismatched = 0usize;
    let mut max_diff = 0i32;
    for (a, e) in actual
        .rgba
        .chunks_exact(4)
        .zip(expected.rgba.chunks_exact(4))
    {
        let diff = (0..4)
            .map(|k| (a[k] as i32 - e[k] as i32).abs())
            .max()
            .unwrap_or(0);
        max_diff = max_diff.max(diff);
        if diff > 2 {
            mismatched += 1;
        }
    }
    let total = actual.width * actual.height;
    assert!(
        mismatched * 100 <= total * allowed_percent,
        "{label}: {mismatched}/{total} pixels differ by >2 LSB (limit {allowed_percent}%, max channel diff {max_diff})"
    );
}

#[test]
fn cpu_images_match_committed_visual_baselines() {
    let doc = load_vtu(Path::new("data/fixtures/meshgen/good_cube.vtu")).expect("good_cube loads");
    let update = std::env::var_os("RUSTMSPT_UPDATE_RENDER_BASELINES").is_some();
    for variant in VARIANTS {
        for view in VIEWS {
            let actual = render_cpu_case(&doc, variant, view);
            let path = baseline_path(variant, view);
            if update {
                save_image(&path, &actual).expect("baseline writes");
            } else {
                let expected = load_png(&path);
                assert_image_matches(&actual, &expected, &format!("{} {view}", variant.name()), 2);
            }
        }
    }
}

#[test]
fn every_contract_fixture_extracts_and_renders_nonblank() {
    for entry in std::fs::read_dir("data/fixtures/meshgen").expect("fixture dir") {
        let path = entry.expect("dir entry").path();
        if path.extension().and_then(|e| e.to_str()) != Some("vtu") {
            continue;
        }
        let doc = load_vtu(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
        let scene = build_scene(&doc, &SceneSpec::default())
            .unwrap_or_else(|e| panic!("{} scene extraction: {e}", path.display()));
        let bbox = scene
            .bbox
            .unwrap_or_else(|| panic!("{} has no bbox", path.display()));
        let image = render_scene_cpu(
            &scene,
            &camera_for(bbox, "iso_ne"),
            WIDTH,
            HEIGHT,
            &SceneRenderSettings::default(),
        );
        let visible = image
            .rgba
            .chunks_exact(4)
            .filter(|p| p[..3] != [255, 255, 255])
            .count();
        assert!(
            visible > 10,
            "{} rendered blank ({visible} visible pixels)",
            path.display()
        );
    }
}

#[test]
fn visual_variants_are_materially_distinct() {
    let doc = load_vtu(Path::new("data/fixtures/meshgen/good_cube.vtu")).expect("good_cube loads");
    let images: Vec<_> = VARIANTS
        .iter()
        .map(|&variant| render_cpu_case(&doc, variant, "iso_ne"))
        .collect();
    for i in 0..images.len() {
        for j in (i + 1)..images.len() {
            let differing = images[i]
                .rgba
                .chunks_exact(4)
                .zip(images[j].rgba.chunks_exact(4))
                .filter(|(a, b)| a != b)
                .count();
            assert!(
                differing > WIDTH * HEIGHT / 100,
                "variants {i}/{j} are visually indistinct"
            );
        }
    }
}

#[test]
fn transparent_baseline_render_is_deterministic() {
    let doc = load_vtu(Path::new("data/fixtures/meshgen/good_cube.vtu")).expect("good_cube loads");
    let first = render_cpu_case(&doc, Variant::Transparent, "front");
    let second = render_cpu_case(&doc, Variant::Transparent, "front");
    assert_eq!(first.rgba, second.rgba);
}

#[test]
#[cfg(feature = "gpu")]
fn gpu_opaque_variants_match_committed_cpu_references() {
    let doc = load_vtu(Path::new("data/fixtures/meshgen/good_cube.vtu")).expect("good_cube loads");
    let mut pipeline = match rustmspt::gpu::GpuScenePipeline::new() {
        Ok(pipeline) => pipeline,
        Err(e) => {
            println!("Skipping GA-5 GPU baseline test (no adapter): {e}");
            return;
        }
    };
    for variant in VARIANTS.into_iter().filter(|v| v.gpu_comparable()) {
        let scene = build_scene(&doc, &variant.scene_spec()).expect("GA-5 scene extracts");
        let bbox = scene.bbox.expect("fixture has a bbox");
        for view in VIEWS {
            let camera = camera_for(bbox, view);
            let actual = pipeline
                .render(
                    &scene,
                    &camera,
                    WIDTH,
                    HEIGHT,
                    &variant.settings(),
                    &rustmspt::gpu::GpuSceneOptions::with_overlays(),
                )
                .expect("GPU visual-regression render");
            let expected = load_png(&baseline_path(variant, view));
            // Full fixture scenes include LineList/wireframe overlays. CPU and GPU
            // rasterize those edges differently, so GA-5 freezes a 5% edge-pixel
            // budget; the simpler no-overlay GA-3c parity test remains at 2%.
            assert_image_matches(
                &actual,
                &expected,
                &format!("GPU {} {view}", variant.name()),
                5,
            );
        }
    }
}
