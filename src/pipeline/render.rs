use crate::compute::policy::{configured_mode, resolve_execution};
use crate::config::RenderConfig;
use crate::error::Result;
use crate::geometry::{
    build_render_camera, parse_render_projection, parse_render_vec3, render_mesh_cpu,
    RenderCameraSpec, RenderSettings,
};
use crate::io::{load_stl_or_merge_folder, save_image};
use crate::pipeline::Pipeline;
use crate::types::RenderedImage;
use rayon::ThreadPoolBuilder;
use std::path::Path;

pub struct RenderPipeline {
    pub config: RenderConfig,
}

impl Pipeline for RenderPipeline {
    // AI-FUNC-SUMMARY:
    // Purpose: Render an STL mesh to an image from a configured viewpoint (CPU ray casting or GPU rasterization).
    // Inputs: RenderConfig with stl_path, focus_point, view_direction, optional up vector,
    //         projection, framing options, resolution, acceleration policy, and output path.
    // Returns: Ok(()) or error.
    // Side effects: Reads STL from disk; writes the rendered image; prints [Info]/[Warning] lines.
    // Notes: Focus point is the image-center target; view direction points from the camera toward
    // it. Orthographic auto-fits the mesh bbox in the image plane; perspective auto-places the
    // camera so the bbox fits the FOV (camera_distance overrides). GPU failures fall back to CPU.
    fn run(&self) -> Result<()> {
        let params = &self.config.render;

        let available_cores = std::thread::available_parallelism()
            .map(|n| n.get())
            .unwrap_or(1);
        let cpu_max = params.cpu_max.unwrap_or(-1);
        let thread_count = if cpu_max == -1 {
            available_cores
        } else {
            (cpu_max.max(1) as usize).min(available_cores)
        };
        let thread_pool = ThreadPoolBuilder::new()
            .num_threads(thread_count)
            .build()
            .map_err(|e| {
                crate::error::RustMsptError::InvalidConfig(format!(
                    "Failed to build thread pool: {e}"
                ))
            })?;
        println!(
            "[Info] CPU setting: cpu_max={} -> using {} worker threads (available {}).",
            cpu_max, thread_count, available_cores
        );

        thread_pool.install(|| self.run_in_pool())
    }
}

impl RenderPipeline {
    // AI-FUNC-SUMMARY: Load, select, render and save inside the configured pool, enforcing fallback policy; prints load/prepare/render_and_backend/encode_write/total_in_pool timings, workers and peak RSS.
    fn run_in_pool(&self) -> Result<()> {
        let params = &self.config.render;
        let requested = configured_mode(&params.acceleration)?;
        let mut timer = crate::pipeline::timing::StageTimer::start("render");
        let mesh = load_stl_or_merge_folder(Path::new(&params.stl_path))?;
        timer.stage("load");
        println!(
            "[Info] STL file(s) loaded from: {} ({} vertices, {} faces)",
            params.stl_path,
            mesh.vertices.len(),
            mesh.faces.len()
        );

        let focus = parse_render_vec3("focus_point", &params.focus_point)?;
        let direction = parse_render_vec3("view_direction", &params.view_direction)?;
        let up = match params.up_vector.as_ref() {
            Some(v) => Some(parse_render_vec3("up_vector", v)?),
            None => None,
        };
        let projection = parse_render_projection(&params.projection)?;
        let camera = build_render_camera(
            &mesh,
            &RenderCameraSpec {
                focus_point: focus,
                view_direction: direction,
                up_vector: up,
                projection,
                perspective_fov_degrees: params.perspective_fov_degrees,
                camera_distance: params.camera_distance,
                fit_padding: params.fit_padding,
                width: params.width,
                height: params.height,
            },
        )?;
        println!(
            "[Info] Camera: projection={:?}, eye=({:.4},{:.4},{:.4}), forward=({:.4},{:.4},{:.4}), resolution={}x{}",
            projection,
            camera.eye.x, camera.eye.y, camera.eye.z,
            camera.forward.x, camera.forward.y, camera.forward.z,
            params.width, params.height
        );

        let pixel_count = params.width.checked_mul(params.height).ok_or_else(|| {
            crate::error::RustMsptError::InvalidConfig("render pixel count overflows".into())
        })?;
        let accel = &params.acceleration;
        // Three 24-byte vertices per face, two 4-byte textures, padded readback and uniforms.
        let estimated_bytes = (|| {
            let vertices = u64::try_from(mesh.faces.len()).ok()?.checked_mul(72)?;
            let pixels = u64::try_from(pixel_count).ok()?.checked_mul(8)?;
            let row = u64::try_from(params.width)
                .ok()?
                .checked_mul(4)?
                .checked_add(255)?
                / 256
                * 256;
            let staging = row.checked_mul(u64::try_from(params.height).ok()?)?;
            vertices
                .checked_add(pixels)?
                .checked_add(staging)?
                .checked_add(128)
        })();
        let selection = resolve_execution(
            accel,
            requested,
            pixel_count,
            accel.gpu_min_pixels,
            true,
            estimated_bytes,
        )?;
        println!(
            "[Info] Acceleration: requested={}, effective={}",
            requested, selection.backend
        );
        if let Some(ref fb) = selection.fallback {
            println!("[Info] Acceleration fallback: {}", fb.reason);
        }

        let settings = RenderSettings::default();
        timer.stage("prepare");

        #[allow(unused_mut)]
        let mut gpu_image: Option<RenderedImage> = None;
        #[cfg(feature = "gpu")]
        if selection.backend.is_gpu() {
            match crate::gpu::GpuRenderPipeline::new()
                .and_then(|mut p| p.render(&mesh, &camera, params.width, params.height, &settings))
            {
                Ok(image) => {
                    println!("[Info] Rendered with GPU backend (wgpu offscreen rasterization)");
                    gpu_image = Some(image);
                }
                Err(e) if !accel.cpu_fallback => {
                    return Err(crate::error::RustMsptError::Gpu(format!("render: {e}")))
                }
                Err(e) => {
                    println!("[Warning] GPU render failed: {e}, falling back to CPU");
                }
            }
        }

        let image = match gpu_image {
            Some(image) => image,
            None => {
                println!("[Info] Rendering with CPU backend (ray casting)");
                render_mesh_cpu(&mesh, &camera, params.width, params.height, &settings)
            }
        };

        timer.stage("render_and_backend");
        println!(
            "[Info] Render execution: workers={}, worker_index={:?}",
            rayon::current_num_threads(),
            rayon::current_thread_index()
        );
        timer.restart();
        save_image(Path::new(&params.output_path), &image)?;
        timer.stage("encode_write");
        println!(
            "[Info] Rendered image ({}x{}) saved to: {}",
            image.width, image.height, params.output_path
        );
        timer.total("total_in_pool");
        timer.report_resources();

        Ok(())
    }
}
