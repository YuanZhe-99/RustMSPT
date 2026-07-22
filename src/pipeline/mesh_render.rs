use super::Pipeline;
use crate::config::mesh_render::{FilterSpec, MeshRenderConfig, ViewSpec};
use crate::error::{Result, RustMsptError};
use crate::geometry::render::{
    build_render_camera, parse_render_projection, parse_render_vec3, RenderCameraSpec,
};
use crate::geometry::scene_render::{named_view, render_scene_cpu, SceneRenderSettings};
use crate::io::vtu::{load_vtu, ArrayData};
use crate::io::save_image;
use crate::meshgen::render_scene::{build_scene, ColorMode, SceneFilter, SceneSpec};
use crate::types::{BoundingBox, Mesh, Vec3};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

// AI-FUNC-SUMMARY: Pipeline wrapper for the mesh-render subcommand; holds the parsed MeshRenderConfig; side effects: none until run().
pub struct MeshRenderPipeline {
    pub config: MeshRenderConfig,
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

impl Pipeline for MeshRenderPipeline {
    // AI-FUNC-SUMMARY:
    // Purpose: Render a contract VTU to one PNG per configured view: load, extract a filtered RenderScene, build a camera per view (named preset or custom), CPU-composite, and save.
    // Inputs: self.config (input VTU path, views, image size/background, coloring, opacities, filters, overlays).
    // Returns: Ok(()) or the first configuration/IO error.
    // Side effects: Reads the VTU; creates output_dir; writes `<input_stem>_<view>.png` per view; prints scene stats and written paths.
    // Notes: Coloring by an integer cell array is categorical, by a float array sequential (viridis); "uniform" uses `uniform_color`. Camera framing uses the full document bbox so all views and filter variations frame identically.
    fn run(&self) -> Result<()> {
        let p = &self.config.mesh_render;
        let doc = load_vtu(Path::new(&p.input))?;

        let color_mode = if p.color_by.eq_ignore_ascii_case("uniform") {
            let c = match &p.uniform_color {
                Some(v) => parse_rgb("uniform_color", v, false)?,
                None => [140, 160, 190, 255],
            };
            ColorMode::Uniform([c[0], c[1], c[2]])
        } else {
            let arr = doc.cell_array(&p.color_by).ok_or_else(|| {
                RustMsptError::InvalidConfig(format!(
                    "mesh_render.color_by names cell array '{}', which is absent from this VTU (produced by mesh generation or mesh-verify --annotate)",
                    p.color_by
                ))
            })?;
            match arr.data {
                ArrayData::F32(_) | ArrayData::F64(_) => ColorMode::Scalar {
                    array: p.color_by.clone(),
                    min: p.scalar_min,
                    max: p.scalar_max,
                },
                _ => ColorMode::Categorical { array: p.color_by.clone() },
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
            max_wireframe_edges: 200_000,
            highlight_points,
        };

        let scene = build_scene(&doc, &spec)?;
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
        let settings = SceneRenderSettings { background, ambient: p.ambient };

        let stem = Path::new(&p.input)
            .file_stem()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_else(|| "mesh".to_string());
        let out_dir = PathBuf::from(&p.output_dir);
        std::fs::create_dir_all(&out_dir)?;

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
                ViewSpec::Custom { name, view_direction, focus_point, up_vector } => {
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
            let camera = build_render_camera(&corner_mesh, &camera_spec)?;
            let image = render_scene_cpu(&scene, &camera, p.width, p.height, &settings);
            let out_path = out_dir.join(format!("{stem}_{view_name}.png"));
            save_image(&out_path, &image)?;
            println!("[mesh-render] wrote {}", out_path.display());
        }

        Ok(())
    }
}
