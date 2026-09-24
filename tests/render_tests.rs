use rustmspt::config::{RenderConfig, RenderParams};
use rustmspt::geometry::{
    box_mesh, build_render_camera, parse_render_projection, parse_render_vec3, render_mesh_cpu,
    RenderCameraSpec, RenderProjection, RenderSettings,
};
use rustmspt::io::{load_stl, save_image, save_stl};
use rustmspt::pipeline::render::RenderPipeline;
use rustmspt::pipeline::Pipeline;
use rustmspt::types::{BoundingBox, RenderedImage, Vec3};

fn unit_box() -> rustmspt::types::Mesh {
    box_mesh(BoundingBox {
        min: Vec3::new(0.0, 0.0, 0.0),
        max: Vec3::new(1.0, 1.0, 1.0),
    })
}

fn spec(projection: RenderProjection) -> RenderCameraSpec {
    RenderCameraSpec {
        focus_point: [0.5, 0.5, 0.5],
        view_direction: [0.0, 0.0, -1.0],
        up_vector: None,
        projection,
        perspective_fov_degrees: 45.0,
        camera_distance: None,
        fit_padding: 0.05,
        width: 64,
        height: 64,
    }
}

fn default_camera(
    mesh: &rustmspt::types::Mesh,
    projection: RenderProjection,
) -> rustmspt::geometry::RenderCamera {
    build_render_camera(mesh, &spec(projection)).expect("camera should build")
}

fn pixel(image: &RenderedImage, x: usize, y: usize) -> [u8; 4] {
    let i = (y * image.width + x) * 4;
    [
        image.rgba[i],
        image.rgba[i + 1],
        image.rgba[i + 2],
        image.rgba[i + 3],
    ]
}

#[test]
fn parse_vec3_requires_three_elements() {
    assert!(parse_render_vec3("focus_point", &[1.0, 2.0, 3.0]).is_ok());
    assert!(parse_render_vec3("focus_point", &[1.0, 2.0]).is_err());
    assert!(parse_render_vec3("focus_point", &[1.0, 2.0, f64::NAN]).is_err());
}

#[test]
fn parse_projection_accepts_known_names() {
    assert_eq!(
        parse_render_projection("orthographic").unwrap(),
        RenderProjection::Orthographic
    );
    assert_eq!(
        parse_render_projection(" Perspective ").unwrap(),
        RenderProjection::Perspective
    );
    assert!(parse_render_projection("isometric").is_err());
}

#[test]
fn camera_rejects_invalid_inputs() {
    let mesh = unit_box();
    assert!(
        build_render_camera(
            &mesh,
            &RenderCameraSpec {
                view_direction: [0.0, 0.0, 0.0],
                ..spec(RenderProjection::Orthographic)
            }
        )
        .is_err(),
        "zero view direction must be rejected"
    );
    assert!(
        build_render_camera(
            &mesh,
            &RenderCameraSpec {
                width: 0,
                ..spec(RenderProjection::Orthographic)
            }
        )
        .is_err(),
        "zero width must be rejected"
    );
    assert!(
        build_render_camera(
            &mesh,
            &RenderCameraSpec {
                perspective_fov_degrees: 0.0,
                ..spec(RenderProjection::Perspective)
            }
        )
        .is_err(),
        "zero FOV must be rejected"
    );
    assert!(
        build_render_camera(
            &mesh,
            &RenderCameraSpec {
                camera_distance: Some(-1.0),
                ..spec(RenderProjection::Perspective)
            }
        )
        .is_err(),
        "negative camera distance must be rejected"
    );
    assert!(
        build_render_camera(
            &rustmspt::types::Mesh::empty(),
            &spec(RenderProjection::Orthographic)
        )
        .is_err(),
        "empty mesh must be rejected"
    );
}

#[test]
fn camera_auto_up_fallback_for_parallel_up() {
    let mesh = unit_box();
    let camera = build_render_camera(
        &mesh,
        &RenderCameraSpec {
            up_vector: Some([0.0, 0.0, 1.0]),
            ..spec(RenderProjection::Orthographic)
        },
    )
    .expect("camera should fall back to an alternate up axis");
    let cross = camera.forward.cross(camera.up);
    assert!(rustmspt::geometry::vec_norm(cross) > 0.9);
}

#[test]
fn camera_looks_along_view_direction_at_focus() {
    let mesh = unit_box();
    let camera = default_camera(&mesh, RenderProjection::Orthographic);
    let to_eye = Vec3::new(0.5, 0.5, 0.5).sub(camera.eye);
    let n = rustmspt::geometry::vec_norm(to_eye);
    let cos = to_eye.scale(1.0 / n).dot(camera.forward);
    assert!(cos > 0.999, "eye must lie behind the focus along -forward");
}

#[test]
fn cpu_render_orthographic_hits_center_misses_corners() {
    let mesh = unit_box();
    let settings = RenderSettings::default();
    let camera = default_camera(&mesh, RenderProjection::Orthographic);
    let image = render_mesh_cpu(&mesh, &camera, 64, 64, &settings);

    assert_eq!(image.width, 64);
    assert_eq!(image.height, 64);
    assert_eq!(image.rgba.len(), 64 * 64 * 4);

    let center = pixel(&image, 32, 32);
    assert_ne!(
        center,
        [
            settings.background[0],
            settings.background[1],
            settings.background[2],
            255
        ],
        "center pixel should hit the box"
    );

    let corner = pixel(&image, 0, 0);
    assert_eq!(
        corner,
        [
            settings.background[0],
            settings.background[1],
            settings.background[2],
            255
        ],
        "corner pixel should be background for a centered unit box"
    );
}

#[test]
fn cpu_render_perspective_hits_center() {
    let mesh = unit_box();
    let settings = RenderSettings::default();
    let camera = default_camera(&mesh, RenderProjection::Perspective);
    let image = render_mesh_cpu(&mesh, &camera, 64, 64, &settings);

    let center = pixel(&image, 32, 32);
    assert_ne!(
        center,
        [
            settings.background[0],
            settings.background[1],
            settings.background[2],
            255
        ],
        "center pixel should hit the box in perspective"
    );
}

#[test]
fn cpu_render_front_face_shaded_brighter_than_ambient() {
    let mesh = unit_box();
    let settings = RenderSettings::default();
    let camera = default_camera(&mesh, RenderProjection::Orthographic);
    let image = render_mesh_cpu(&mesh, &camera, 64, 64, &settings);

    let center = pixel(&image, 32, 32);
    let ambient_floor = (settings.base_color[0] as f64 * settings.ambient * 0.99) as u8;
    assert!(
        center[0] > ambient_floor,
        "front-facing surface should exceed ambient-only shading"
    );
}

#[test]
fn cpu_render_empty_mesh_yields_background() {
    let settings = RenderSettings::default();
    let empty = rustmspt::types::Mesh::empty();
    let proxy = unit_box();
    let camera = default_camera(&proxy, RenderProjection::Orthographic);
    let image = render_mesh_cpu(&empty, &camera, 16, 16, &settings);
    assert!(image.rgba.chunks_exact(4).all(|px| px
        == [
            settings.background[0],
            settings.background[1],
            settings.background[2],
            255
        ]));
}

#[test]
fn save_image_writes_png_and_rejects_bad_extension() {
    let tmp = tempfile::tempdir().expect("tempdir should be created");
    let settings = RenderSettings::default();
    let image = RenderedImage::filled(8, 6, [10, 20, 30, 255]);

    let png_path = tmp.path().join("out").join("img.png");
    save_image(&png_path, &image).expect("png save should succeed");
    assert!(png_path.exists());

    let loaded = ::image::open(&png_path).expect("png should decode");
    assert_eq!(loaded.width(), 8);
    assert_eq!(loaded.height(), 6);
    let rgba = loaded.to_rgba8();
    assert_eq!(rgba.get_pixel(0, 0).0, [10, 20, 30, 255]);

    let bad = RenderedImage {
        width: 4,
        height: 4,
        rgba: vec![0u8; 3],
    };
    assert!(
        save_image(&png_path, &bad).is_err(),
        "size mismatch must error"
    );

    let bmp_path = tmp.path().join("img.bmp");
    assert!(
        save_image(&bmp_path, &image).is_err(),
        "unsupported extension must error"
    );
    let _ = settings;
}

#[test]
fn render_pipeline_smoke_cpu() {
    let tmp = tempfile::tempdir().expect("tempdir should be created");
    let input = tmp.path().join("input.stl");
    let output = tmp.path().join("render.png");
    save_stl(&input, &unit_box(), "sample").expect("sample mesh save should succeed");

    let pipeline = RenderPipeline {
        config: RenderConfig {
            render: RenderParams {
                stl_path: input.to_string_lossy().to_string(),
                output_path: output.to_string_lossy().to_string(),
                focus_point: vec![0.5, 0.5, 0.5],
                view_direction: vec![1.0, 1.0, -1.0],
                up_vector: None,
                projection: "orthographic".to_string(),
                perspective_fov_degrees: 45.0,
                camera_distance: None,
                fit_padding: 0.05,
                width: 64,
                height: 64,
                cpu_max: Some(2),
                acceleration: rustmspt::config::AccelerationConfig {
                    mode: rustmspt::compute::AccelerationMode::Cpu,
                    ..Default::default()
                },
            },
        },
    };

    pipeline.run().expect("render pipeline should run");
    assert!(output.exists());

    let loaded = ::image::open(&output).expect("output png should decode");
    assert_eq!(loaded.width(), 64);
    assert_eq!(loaded.height(), 64);
}

#[test]
#[cfg(feature = "gpu")]
fn gpu_render_matches_cpu_within_tolerance() {
    let mesh = unit_box();
    let settings = RenderSettings::default();
    let (w, h) = (128usize, 128usize);

    for projection in [
        RenderProjection::Orthographic,
        RenderProjection::Perspective,
    ] {
        let camera = build_render_camera(
            &mesh,
            &RenderCameraSpec {
                view_direction: [0.4, 0.3, -1.0],
                width: w,
                height: h,
                ..spec(projection)
            },
        )
        .expect("camera should build");

        let cpu = render_mesh_cpu(&mesh, &camera, w, h, &settings);
        let mut pipeline = match rustmspt::gpu::GpuRenderPipeline::new() {
            Ok(pipeline) => pipeline,
            Err(e) => {
                println!("Skipping GPU render comparison (no GPU available): {e}");
                return;
            }
        };

        assert!(pipeline.render(&mesh, &camera, usize::MAX, h, &settings).is_err());
        assert!(pipeline.render(&mesh, &camera, 0, h, &settings).is_err());
        let gpu = pipeline.render(&mesh, &camera, w, h, &settings).expect("initialized GPU must render successfully");
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
fn stl_render_fixture_loads() {
    let tmp = tempfile::tempdir().expect("tempdir should be created");
    let input = tmp.path().join("input.stl");
    save_stl(&input, &unit_box(), "sample").expect("save should succeed");
    let mesh = load_stl(&input).expect("load should succeed");
    assert!(!mesh.is_empty());
}
