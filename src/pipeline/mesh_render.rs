use super::Pipeline;
use crate::compute::backend::AccelerationMode;
use crate::compute::policy::configured_mode;
use crate::config::mesh_render::{FilterSpec, MeshRenderConfig, ViewSpec};
use crate::error::{Result, RustMsptError};
use crate::geometry::render::{
    build_render_camera, parse_render_projection, parse_render_vec3, RenderCameraSpec,
};
use crate::geometry::scene_render::{named_view, PreparedScene, SceneRenderSettings};
use crate::io::save_image;
use crate::io::vtu::{load_vtu, ArrayData};
use crate::meshgen::render_scene::{
    build_scene, ColorMode, SceneFilter, SceneSpec, DEFAULT_MAX_WIREFRAME_EDGES,
};
use crate::types::{BoundingBox, Mesh, Vec3};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

// AI-FUNC-SUMMARY: Pipeline wrapper for the mesh-render subcommand; holds the parsed MeshRenderConfig; side effects: none until run().
pub struct MeshRenderPipeline {
    pub config: MeshRenderConfig,
}

// AI-FUNC-SUMMARY: Consume ordered owned frames using a rendezvous writer when overlap is enabled; join/drain before returning, and report output errors separately from producer errors so GPU fallback never hides a write failure. At most one frame is being written and one is held by the producer.
fn consume_frames<T: Send>(
    overlap: bool,
    produce: impl FnOnce(
        &mut dyn FnMut(T) -> std::result::Result<(), String>,
    ) -> std::result::Result<(), String>,
    mut write: impl FnMut(T) -> Result<()> + Send,
) -> (std::result::Result<(), String>, Result<()>) {
    if !overlap {
        let mut output = Ok(());
        let result = produce(&mut |frame| match write(frame) {
            Ok(()) => Ok(()),
            Err(error) => {
                let message = error.to_string();
                output = Err(error);
                Err(message)
            }
        });
        return (result, output);
    }
    std::thread::scope(|scope| {
        let (sender, receiver) = std::sync::mpsc::sync_channel(0);
        let writer = scope.spawn(move || {
            for frame in receiver {
                write(frame)?;
            }
            Ok(())
        });
        let result = produce(&mut |frame| {
            sender
                .send(frame)
                .map_err(|_| "image writer stopped".to_string())
        });
        drop(sender);
        let output = writer.join().unwrap_or_else(|_| {
            Err(RustMsptError::Io(std::io::Error::other(
                "image writer panicked",
            )))
        });
        (result, output)
    })
}

// AI-FUNC-SUMMARY:
// Purpose: Render items in order and write each result, overlapping the write of item i-1 with the render of item i through rayon::join.
// Inputs: items, a render closure (may use nested Rayon parallelism), an ordered write closure receiving (index, frame).
// Returns: Ok after every frame is written, or the first write error.
// Side effects: Whatever render/write do.
// Notes: At most one finished frame waits for or undergoes writing while the next renders, so two frames are resident at most. Writes happen strictly in index order. A write error returns after the concurrently started render finishes; no later item is rendered or written. With one worker, join runs render then write inline, which is sequential.
fn render_and_write_overlapped<C: Sync, T: Send>(
    items: &[C],
    render: impl Fn(&C) -> T + Sync,
    mut write: impl FnMut(usize, T) -> Result<()> + Send,
) -> Result<()> {
    let mut pending: Option<(usize, T)> = None;
    for (index, item) in items.iter().enumerate() {
        let (frame, written) = match pending.take() {
            None => (render(item), Ok(())),
            Some((previous, image)) => rayon::join(|| render(item), || write(previous, image)),
        };
        written?;
        pending = Some((index, frame));
    }
    match pending {
        Some((index, frame)) => write(index, frame),
        None => Ok(()),
    }
}

fn parse_rgb(name: &str, values: &[u8], allow_alpha: bool) -> Result<[u8; 4]> {
    match (values.len(), allow_alpha) {
        (3, _) => Ok([values[0], values[1], values[2], 255]),
        (4, true) => Ok([values[0], values[1], values[2], values[3]]),
        _ => Err(RustMsptError::InvalidConfig(format!(
            "mesh_render.{name} must have 3 (RGB) {}elements",
            if allow_alpha { "or 4 (RGBA) " } else { "" }
        ))),
    }
}

fn to_vec3(name: &str, values: &[f64]) -> Result<Vec3> {
    let a = parse_render_vec3(name, values)?;
    Ok(Vec3::new(a[0], a[1], a[2]))
}

fn build_filters(specs: &[FilterSpec]) -> Result<Vec<SceneFilter>> {
    specs
        .iter()
        .map(|f| {
            Ok(match f {
                FilterSpec::CellKind { values } => SceneFilter::CellKind(values.clone()),
                FilterSpec::Component { values } => SceneFilter::Component(values.clone()),
                FilterSpec::RegionKey { values } => SceneFilter::RegionKey(values.clone()),
                FilterSpec::Partition { values } => SceneFilter::Partition(values.clone()),
                FilterSpec::Regime { values } => SceneFilter::Regime(values.clone()),
                FilterSpec::Background { keep } => {
                    if *keep {
                        SceneFilter::BackgroundOnly
                    } else {
                        SceneFilter::ExcludeBackground
                    }
                }
                FilterSpec::ArrayRange { array, min, max } => SceneFilter::ArrayRange {
                    array: array.clone(),
                    min: *min,
                    max: *max,
                },
                FilterSpec::Bbox { min, max } => SceneFilter::BBox(BoundingBox {
                    min: to_vec3("filters.bbox.min", min)?,
                    max: to_vec3("filters.bbox.max", max)?,
                }),
                FilterSpec::ClipPlane { origin, normal } => SceneFilter::ClipPlane {
                    origin: to_vec3("filters.clip_plane.origin", origin)?,
                    normal: to_vec3("filters.clip_plane.normal", normal)?,
                },
            })
        })
        .collect()
}

// AI-FUNC-SUMMARY:
// Purpose: Render every requested view through the GPU opaque preview in one batch.
// Inputs: extracted scene, named cameras, resolution, appearance settings.
// Returns: Ok after ordered consumption, or the first rendering/consumer error.
// Side effects: Initializes wgpu on the first call of the process.
// Notes: Compiled out without the `gpu` feature, where it always reports unavailability so
//   `backend: auto` degrades to the CPU renderer exactly as it does on a machine with no adapter.
#[cfg(feature = "gpu")]
fn render_views_gpu(
    scene: &crate::meshgen::render_scene::RenderScene,
    cameras: &[(String, crate::geometry::render::RenderCamera)],
    width: usize,
    height: usize,
    settings: &SceneRenderSettings,
    consume: impl FnMut(usize, crate::types::RenderedImage) -> std::result::Result<(), String>,
) -> std::result::Result<(), String> {
    let cams: Vec<crate::geometry::render::RenderCamera> =
        cameras.iter().map(|(_, c)| *c).collect();
    let options = crate::gpu::GpuSceneOptions::with_overlays();
    crate::gpu::GpuScenePipeline::new()?
        .render_views_to(scene, &cams, width, height, settings, &options, consume)
}

#[cfg(not(feature = "gpu"))]
fn render_views_gpu(
    _scene: &crate::meshgen::render_scene::RenderScene,
    _cameras: &[(String, crate::geometry::render::RenderCamera)],
    _width: usize,
    _height: usize,
    _settings: &SceneRenderSettings,
    _consume: impl FnMut(usize, crate::types::RenderedImage) -> std::result::Result<(), String>,
) -> std::result::Result<(), String> {
    Err("built without the `gpu` feature".to_string())
}

impl Pipeline for MeshRenderPipeline {
    // AI-FUNC-SUMMARY:
    // Purpose: Render a contract VTU to one PNG per configured view: load, extract a filtered RenderScene, build a camera per view (named preset or custom), CPU-composite, and save.
    // Inputs: self.config (input VTU path, views, image size/background, coloring, opacities, filters, overlays).
    // Returns: Ok(()) or the first configuration/IO error.
    // Side effects: Reads the VTU; creates output_dir; writes `<input_stem>_<view>.png` per view; prints scene stats and written paths.
    // Notes: Coloring by an integer cell array is categorical, by a float array sequential (viridis); "uniform" uses `uniform_color`. Camera framing uses the full document bbox so all views and filter variations frame identically.
    fn run(&self) -> Result<()> {
        self.with_worker_pool(|| self.run_in_pool())
    }
}

impl MeshRenderPipeline {
    // AI-FUNC-SUMMARY: Install all mesh-render work in a bounded Rayon pool; default/-1 uses available CPUs, other requests clamp to 1..available; propagate pool and operation errors.
    fn with_worker_pool<T: Send>(&self, operation: impl FnOnce() -> Result<T> + Send) -> Result<T> {
        let available = std::thread::available_parallelism()
            .map(|n| n.get())
            .unwrap_or(1);
        let requested = self.config.cpu_max.unwrap_or(-1);
        let workers = if requested == -1 {
            available
        } else {
            (requested.max(1) as usize).min(available)
        };
        let pool = rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .map_err(|e| {
                RustMsptError::InvalidConfig(format!(
                    "Failed to build mesh-render thread pool: {e}"
                ))
            })?;
        pool.install(|| {
            println!("[mesh-render] CPU setting: cpu_max={requested}, actual workers={} (available {available})", rayon::current_num_threads());
            operation()
        })
    }

    // AI-FUNC-SUMMARY: Load VTU, prepare one scene and render/save all views within the installed worker budget, including GPU fallback; preserve opaque-preview versus CPU transparency behavior; prints load/scene/cameras/gpu_render_write or cpu_prepare/cpu_render/encode_write timings (the last two are summed per-view times that overlap each other) plus cpu_render_write_wall, workers and peak RSS.
    fn run_in_pool(&self) -> Result<()> {
        let p = &self.config.mesh_render;
        let configured = match p.backend.trim().to_ascii_lowercase().as_str() {
            "cpu" => AccelerationMode::Cpu,
            "gpu" => AccelerationMode::Gpu,
            "auto" => AccelerationMode::Auto,
            other => {
                return Err(RustMsptError::InvalidConfig(format!(
                    "mesh_render.backend '{other}' must be cpu, gpu, or auto"
                )))
            }
        };
        let backend = configured_mode(&crate::config::AccelerationConfig {
            mode: configured,
            ..Default::default()
        })?;
        println!("[mesh-render] requested backend: {backend}");
        let mut timer = crate::pipeline::timing::StageTimer::start("mesh-render");
        let doc = load_vtu(Path::new(&p.input))?;
        timer.stage("load");

        let color_mode = if p.color_by.eq_ignore_ascii_case("uniform") {
            let c = match &p.uniform_color {
                Some(v) => parse_rgb("uniform_color", v, false)?,
                None => [140, 160, 190, 255],
            };
            ColorMode::Uniform([c[0], c[1], c[2]])
        } else {
            // Point fields (`separation_t`, `sizing_h`) colour a cell by the mean of
            // its points, so a stage snapshot with no cell field still renders.
            let arr = doc
                .cell_array(&p.color_by)
                .or_else(|| doc.point_array(&p.color_by))
                .ok_or_else(|| {
                    RustMsptError::InvalidConfig(format!(
                        "mesh_render.color_by names cell or point array '{}', which is absent from this VTU (produced by mesh generation or mesh-verify --annotate)",
                        p.color_by
                    ))
                })?;
            match arr.data {
                ArrayData::F32(_) | ArrayData::F64(_) => ColorMode::Scalar {
                    array: p.color_by.clone(),
                    min: p.scalar_min,
                    max: p.scalar_max,
                },
                _ => ColorMode::Categorical {
                    array: p.color_by.clone(),
                },
            }
        };

        let mut opacity_overrides = HashMap::new();
        for (k, v) in &p.opacity_overrides {
            let key: i64 = k.parse().map_err(|_| {
                RustMsptError::InvalidConfig(format!(
                    "mesh_render.opacity_overrides key '{k}' must be a region_key integer"
                ))
            })?;
            opacity_overrides.insert(key, v.clamp(0.0, 1.0));
        }

        let mut highlight_points = Vec::new();
        for (i, hp) in p.highlight_points.iter().enumerate() {
            highlight_points.push(to_vec3(&format!("highlight_points[{i}]"), hp)?);
        }

        let spec = SceneSpec {
            filters: build_filters(&p.filters)?,
            color_mode,
            volume_opacity: p.volume_opacity.clamp(0.0, 1.0),
            face_opacity: p.face_opacity.clamp(0.0, 1.0),
            opacity_overrides,
            show_faces: p.show_faces,
            show_curves: p.show_curves,
            wireframe: p.wireframe,
            max_wireframe_edges: DEFAULT_MAX_WIREFRAME_EDGES,
            highlight_points,
        };

        timer.restart();
        let scene = build_scene(&doc, &spec)?;
        timer.stage("scene");
        let bbox = scene.bbox.ok_or_else(|| {
            RustMsptError::InvalidMesh("mesh-render: the VTU contains no points".to_string())
        })?;
        println!(
            "[mesh-render] {}: {} cells -> {} triangles, {} segments, {} markers",
            p.input,
            doc.num_cells(),
            scene.tris.len(),
            scene.segments.len(),
            scene.markers.len()
        );
        if scene.wireframe_edges_emitted < scene.wireframe_edges_total {
            println!(
                "[mesh-render] WARNING: wireframe thinned to {} of {} edges (cap {}); the frame is a uniform stride sample, not the full mesh",
                scene.wireframe_edges_emitted, scene.wireframe_edges_total, spec.max_wireframe_edges
            );
        }

        let corner_mesh = Mesh {
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
        };
        let center = bbox.min.add(bbox.max).scale(0.5);
        let projection = parse_render_projection(&p.projection)?;
        let background = parse_rgb("background", &p.background, true)?;
        let settings = SceneRenderSettings {
            background,
            ambient: p.ambient,
        };

        let stem = Path::new(&p.input)
            .file_stem()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_else(|| "mesh".to_string());
        let out_dir = PathBuf::from(&p.output_dir);
        std::fs::create_dir_all(&out_dir)?;

        let mut cameras = Vec::with_capacity(p.views.len());
        for view in &p.views {
            let (view_name, direction, up, focus) = match view {
                ViewSpec::Named(name) => {
                    let (dir, up) = named_view(name).ok_or_else(|| {
                        RustMsptError::InvalidConfig(format!(
                            "mesh_render.views: unknown view preset '{name}' (expected front/back/left/right/top/bottom/iso_ne/iso_nw/iso_se/iso_sw or a custom camera block)"
                        ))
                    })?;
                    (name.clone(), dir, Some(up), center)
                }
                ViewSpec::Custom {
                    name,
                    view_direction,
                    focus_point,
                    up_vector,
                } => {
                    let dir = parse_render_vec3("views.view_direction", view_direction)?;
                    let up = match up_vector {
                        Some(u) => Some(parse_render_vec3("views.up_vector", u)?),
                        None => None,
                    };
                    let focus = match focus_point {
                        Some(f) => to_vec3("views.focus_point", f)?,
                        None => center,
                    };
                    (name.clone(), dir, up, focus)
                }
            };
            let camera_spec = RenderCameraSpec {
                focus_point: [focus.x, focus.y, focus.z],
                view_direction: direction,
                up_vector: up,
                projection,
                perspective_fov_degrees: p.perspective_fov_degrees,
                camera_distance: p.camera_distance,
                fit_padding: p.fit_padding,
                width: p.width,
                height: p.height,
            };
            cameras.push((view_name, build_render_camera(&corner_mesh, &camera_spec)?));
        }

        timer.stage("cameras");
        let gpu_preflight = || -> std::result::Result<(), String> {
            let plan = crate::compute::render_memory::SceneRenderMemory::plan(
                scene.tris.iter().filter(|t| !(t.alpha <= 0.0)).count(),
                scene.segments.len(),
                scene.markers.len(),
                p.width,
                p.height,
            )?;
            plan.check_buffers(u64::MAX)?;
            plan.check_budget(p.gpu_memory_limit_mb)?;
            println!(
                "[mesh-render] GPU planned peak: {} logical bytes, padded row {} bytes",
                plan.gpu_peak_bytes, plan.padded_row_bytes
            );
            Ok(())
        };
        let below_threshold = backend == AccelerationMode::Auto
            && p.width
                .checked_mul(p.height)
                .is_some_and(|pixels| pixels < p.gpu_min_pixels);
        if below_threshold {
            println!(
                "[mesh-render] CPU selected: below gpu_min_pixels={}",
                p.gpu_min_pixels
            );
        }

        let gpu_completed = match backend {
            AccelerationMode::Cpu => false,
            _ if below_threshold => false,
            AccelerationMode::Gpu | AccelerationMode::Auto => {
                let (render_result, output_result) = consume_frames(
                    rayon::current_num_threads() > 1 && cameras.len() > 1,
                    |consume| {
                        gpu_preflight().and_then(|()| {
                            render_views_gpu(
                                &scene,
                                &cameras,
                                p.width,
                                p.height,
                                &settings,
                                &mut |index, image| consume((index, image)),
                            )
                        })
                    },
                    |(index, image): (usize, crate::types::RenderedImage)| {
                        let out_path = out_dir.join(format!("{stem}_{}.png", cameras[index].0));
                        save_image(&out_path, &image)?;
                        println!("[mesh-render] wrote {}", out_path.display());
                        Ok(())
                    },
                );
                output_result?;
                match render_result {
                    Ok(()) => true,
                    Err(e) if backend == AccelerationMode::Auto => {
                        println!(
                            "[mesh-render] GPU preview unavailable, using the CPU renderer: {e}"
                        );
                        false
                    }
                    Err(e) => return Err(RustMsptError::Gpu(e)),
                }
            }
        };
        if !matches!(backend, AccelerationMode::Cpu) && !below_threshold {
            timer.stage("gpu_render_write");
        }
        if gpu_completed {
            println!("[mesh-render] GPU opaque preview (per-set opacity ignored; the CPU path is the transparency reference)");
        }

        if !gpu_completed {
            println!(
                "[mesh-render] CPU transparency renderer: workers={}, worker_index={:?}",
                rayon::current_num_threads(),
                rayon::current_thread_index()
            );
            timer.restart();
            let prepared = PreparedScene::new(&scene);
            timer.stage("cpu_prepare");
            let render_nanos = std::sync::atomic::AtomicU64::new(0);
            let mut write_seconds = 0.0;
            render_and_write_overlapped(
                &cameras,
                |(_, camera)| {
                    let started = std::time::Instant::now();
                    let image = prepared.render(camera, p.width, p.height, &settings);
                    render_nanos.fetch_add(
                        started.elapsed().as_nanos() as u64,
                        std::sync::atomic::Ordering::Relaxed,
                    );
                    image
                },
                |index, image| {
                    let started = std::time::Instant::now();
                    let out_path = out_dir.join(format!("{stem}_{}.png", cameras[index].0));
                    save_image(&out_path, &image)?;
                    write_seconds += started.elapsed().as_secs_f64();
                    println!("[mesh-render] wrote {}", out_path.display());
                    Ok(())
                },
            )?;
            timer.report(
                "cpu_render",
                render_nanos.load(std::sync::atomic::Ordering::Relaxed) as f64 * 1e-9,
            );
            timer.report("encode_write", write_seconds);
            timer.stage("cpu_render_write_wall");
        }
        timer.total("total_in_pool");
        timer.report_resources();

        Ok(())
    }
}

#[cfg(test)]
mod execution_tests {
    use super::*;
    use rayon::prelude::*;

    // AI-FUNC-SUMMARY: Compare sequential and overlapped PNG bytes and verify ordered draining even when rendering fails after delivering frames.
    #[test]
    fn frame_writer_preserves_pngs_and_drains_on_render_error() {
        let dir = tempfile::tempdir().unwrap();
        for overlap in [false, true] {
            let mut written = Vec::new();
            let (render, output) = consume_frames(
                overlap,
                |consume| {
                    for index in 0..5 {
                        consume((
                            index,
                            crate::types::RenderedImage {
                                width: 3,
                                height: 2,
                                rgba: vec![index as u8 * 31; 24],
                            },
                        ))?;
                    }
                    Err("injected render failure".to_string())
                },
                |(index, image)| {
                    written.push(index);
                    save_image(&dir.path().join(format!("{overlap}_{index}.png")), &image)
                },
            );
            assert_eq!(render.unwrap_err(), "injected render failure");
            output.unwrap();
            assert_eq!(written, vec![0, 1, 2, 3, 4]);
        }
        for index in 0..5 {
            assert_eq!(
                std::fs::read(dir.path().join(format!("false_{index}.png"))).unwrap(),
                std::fs::read(dir.path().join(format!("true_{index}.png"))).unwrap()
            );
        }
    }

    // AI-FUNC-SUMMARY: Verify a final-frame write error survives successful production and earlier failures stop ordered writing without a fallback-shaped error.
    #[test]
    fn frame_writer_propagates_first_and_last_output_errors() {
        for overlap in [false, true] {
            for fail_at in [0, 4] {
                let mut written = Vec::new();
                let (_, output) = consume_frames(
                    overlap,
                    |consume| {
                        for index in 0..5 {
                            consume(index)?;
                        }
                        Ok(())
                    },
                    |index| {
                        written.push(index);
                        if index == fail_at {
                            return Err(RustMsptError::InvalidConfig(
                                "injected write failure".into(),
                            ));
                        }
                        Ok(())
                    },
                );
                assert!(output
                    .unwrap_err()
                    .to_string()
                    .contains("injected write failure"));
                assert_eq!(written, (0..=fail_at).collect::<Vec<_>>());
            }
        }
    }

    // AI-FUNC-SUMMARY: Prove the writer can remain active while the producer starts its next frame, without timing-dependent performance assertions.
    #[test]
    fn frame_writer_overlaps_production() {
        let (next, started) = std::sync::mpsc::channel();
        let (render, output) = consume_frames(
            true,
            |consume| {
                consume(0)?;
                next.send(()).unwrap();
                consume(1)
            },
            move |index| {
                if index == 0 {
                    started.recv().unwrap();
                }
                Ok(())
            },
        );
        render.unwrap();
        output.unwrap();
    }

    // AI-FUNC-SUMMARY: Compare overlapped CPU render/write PNG bytes and order with a sequential loop at 1, 2 and 4 workers, including zero and one item.
    #[test]
    fn cpu_overlap_preserves_order_and_bytes() {
        let dir = tempfile::tempdir().unwrap();
        let frame = |i: &usize| crate::types::RenderedImage {
            width: 5,
            height: 3,
            rgba: (0..60).map(|k| (k * 7 + i * 13) as u8).collect(),
        };
        for count in [0usize, 1, 2, 7] {
            let items: Vec<usize> = (0..count).collect();
            for (i, item) in items.iter().enumerate() {
                save_image(&dir.path().join(format!("seq_{count}_{i}.png")), &frame(item)).unwrap();
            }
            for workers in [1, 2, 4] {
                let pool = rayon::ThreadPoolBuilder::new().num_threads(workers).build().unwrap();
                let mut order = Vec::new();
                pool.install(|| {
                    render_and_write_overlapped(&items, frame, |index, image| {
                        order.push(index);
                        save_image(&dir.path().join(format!("ovl_{count}_{workers}_{index}.png")), &image)
                    })
                })
                .unwrap();
                assert_eq!(order, items);
                for i in 0..count {
                    assert_eq!(
                        std::fs::read(dir.path().join(format!("seq_{count}_{i}.png"))).unwrap(),
                        std::fs::read(dir.path().join(format!("ovl_{count}_{workers}_{i}.png"))).unwrap()
                    );
                }
            }
        }
    }

    // AI-FUNC-SUMMARY: Verify a first, middle or last write error is returned, stops later writes, and renders at most the one item overlapping the failing write.
    #[test]
    fn cpu_overlap_write_error_stops_rendering() {
        for workers in [1, 2] {
            let pool = rayon::ThreadPoolBuilder::new().num_threads(workers).build().unwrap();
            for fail_at in [0usize, 2, 5] {
                let rendered = std::sync::atomic::AtomicUsize::new(0);
                let mut written = Vec::new();
                let items: Vec<usize> = (0..6).collect();
                let error = pool
                    .install(|| {
                        render_and_write_overlapped(
                            &items,
                            |&i| {
                                rendered.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                                i
                            },
                            |index, _| {
                                written.push(index);
                                if index == fail_at {
                                    return Err(RustMsptError::InvalidConfig("injected write failure".into()));
                                }
                                Ok(())
                            },
                        )
                    })
                    .unwrap_err();
                assert!(error.to_string().contains("injected write failure"));
                assert_eq!(written, (0..=fail_at).collect::<Vec<_>>());
                assert_eq!(rendered.load(std::sync::atomic::Ordering::SeqCst), (fail_at + 2).min(6));
            }
        }
    }

    // AI-FUNC-SUMMARY: Prove rendering item 1 does not wait for writing item 0: the write blocks until the next render has started (bounded wait turns a regression into a failure, not a hang).
    #[test]
    fn cpu_overlap_renders_next_view_while_writing() {
        let pool = rayon::ThreadPoolBuilder::new().num_threads(2).build().unwrap();
        let (started, wait) = std::sync::mpsc::channel();
        let wait = std::sync::Mutex::new(wait);
        let started = std::sync::Mutex::new(started);
        let items = [0usize, 1];
        pool.install(|| {
            render_and_write_overlapped(
                &items,
                |&i| {
                    if i == 1 {
                        started.lock().unwrap().send(()).unwrap();
                    }
                    i
                },
                |index, _| {
                    if index == 0 {
                        wait.lock()
                            .unwrap()
                            .recv_timeout(std::time::Duration::from_secs(20))
                            .map_err(|_| RustMsptError::InvalidConfig("render of view 1 never started".into()))?;
                    }
                    Ok(())
                },
            )
        })
        .unwrap();
    }

    // AI-FUNC-SUMMARY: Ignored release benchmark: median wall time of sequential render-then-save versus overlapped CPU render/PNG write over 8 views of a shaded icosphere at several sizes and worker counts.
    #[test]
    #[ignore]
    fn cpu_overlap_benchmark() {
        use crate::geometry::render::{render_mesh_cpu, RenderProjection, RenderSettings};
        let mesh = crate::geometry::icosphere_mesh(Vec3::new(0.0, 0.0, 0.0), 1.0, 5);
        let dir = tempfile::tempdir().unwrap();
        let settings = RenderSettings::default();
        for size in [256usize, 1024, 2048] {
            let cameras: Vec<_> = (0..8)
                .map(|i| {
                    let a = i as f64 * 0.7;
                    build_render_camera(
                        &mesh,
                        &RenderCameraSpec {
                            focus_point: [0.0, 0.0, 0.0],
                            view_direction: [a.cos(), a.sin(), -0.4],
                            up_vector: None,
                            projection: RenderProjection::Perspective,
                            perspective_fov_degrees: 40.0,
                            camera_distance: None,
                            fit_padding: 0.05,
                            width: size,
                            height: size,
                        },
                    )
                    .unwrap()
                })
                .collect();
            for workers in [1usize, 2, 4] {
                let pool = rayon::ThreadPoolBuilder::new().num_threads(workers).build().unwrap();
                let mut seq = Vec::new();
                let mut ovl = Vec::new();
                for round in 0..6 {
                    for overlapped in [round % 2 == 0, round % 2 != 0] {
                        let start = std::time::Instant::now();
                        pool.install(|| {
                            let render = |c: &crate::geometry::render::RenderCamera| render_mesh_cpu(&mesh, c, size, size, &settings);
                            let save = |i: usize, img: crate::types::RenderedImage| save_image(&dir.path().join(format!("b{i}.png")), &img);
                            if overlapped {
                                render_and_write_overlapped(&cameras, render, save).unwrap();
                            } else {
                                for (i, c) in cameras.iter().enumerate() {
                                    save(i, render(c)).unwrap();
                                }
                            }
                        });
                        if round > 0 {
                            let t = start.elapsed().as_secs_f64();
                            if overlapped { ovl.push(t) } else { seq.push(t) }
                        }
                    }
                }
                seq.sort_by(f64::total_cmp);
                ovl.sort_by(f64::total_cmp);
                println!(
                    "size {size} views 8 workers {workers} sequential_median_s {:.4} overlapped_median_s {:.4} ratio {:.3} seq_samples {:?} ovl_samples {:?}",
                    seq[2], ovl[2], ovl[2] / seq[2], seq, ovl
                );
            }
        }
    }

    // AI-FUNC-SUMMARY: Observe nested Rayon work under the same installation used by run, including defaults, clamping and error propagation.
    #[test]
    fn mesh_render_worker_budget_reaches_nested_work() {
        let available = std::thread::available_parallelism()
            .map(|n| n.get())
            .unwrap_or(1);
        for request in [None, Some(-1), Some(0), Some(-2), Some(1), Some(2), Some(8)] {
            let mut config: MeshRenderConfig =
                serde_yaml::from_str("mesh_render: { input: unused.vtu }").unwrap();
            config.cpu_max = request;
            let expected = match request {
                None | Some(-1) => available,
                Some(n) => (n.max(1) as usize).min(available),
            };
            let pipeline = MeshRenderPipeline { config };
            let observed = pipeline
                .with_worker_pool(|| {
                    Ok((0..128)
                        .into_par_iter()
                        .map(|_| {
                            (0..4)
                                .into_par_iter()
                                .map(|_| {
                                    (rayon::current_num_threads(), rayon::current_thread_index())
                                })
                                .collect::<Vec<_>>()
                        })
                        .flatten()
                        .collect::<Vec<_>>())
                })
                .unwrap();
            assert_eq!(observed.len(), 512);
            assert!(observed.iter().all(
                |&(workers, index)| workers == expected && index.is_some_and(|i| i < expected)
            ));
            let error = pipeline
                .with_worker_pool::<()>(|| {
                    Err(RustMsptError::InvalidConfig("operation failure".into()))
                })
                .unwrap_err();
            assert!(error.to_string().contains("operation failure"));
        }
    }
}
