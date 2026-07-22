use super::collision::to_parry_trimesh;
use super::render::{RenderCamera, RenderProjection};
use crate::meshgen::render_scene::{RenderScene, SetKind};
use crate::types::{Mesh, RenderedImage, Triangle, Vec3};
use parry3d_f64::math::{Point, Vector};
use parry3d_f64::query::visitors::RayIntersectionsVisitor;
use parry3d_f64::query::{Ray, RayCast};
use rayon::prelude::*;

// AI-FUNC-SUMMARY: Appearance settings for scene rendering; unlike RenderSettings, the background carries a real alpha channel (0 = fully transparent PNG background); side effects: none.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SceneRenderSettings {
    pub background: [u8; 4],
    pub ambient: f64,
}

impl Default for SceneRenderSettings {
    // AI-FUNC-SUMMARY: Default scene appearance (opaque white background, 0.25 ambient); returns SceneRenderSettings; side effects: none.
    fn default() -> Self {
        Self { background: [255, 255, 255, 255], ambient: 0.25 }
    }
}

struct SceneShape {
    shape: parry3d_f64::shape::TriMesh,
    color: Vec<[u8; 3]>,
    alpha: Vec<f64>,
    set: Vec<SetKind>,
}

fn build_scene_shape(scene: &RenderScene) -> Option<SceneShape> {
    if scene.tris.is_empty() {
        return None;
    }
    let mut mesh = Mesh { vertices: Vec::with_capacity(scene.tris.len() * 3), faces: Vec::with_capacity(scene.tris.len()) };
    let mut color = Vec::with_capacity(scene.tris.len());
    let mut alpha = Vec::with_capacity(scene.tris.len());
    let mut set = Vec::with_capacity(scene.tris.len());
    for t in &scene.tris {
        let base = mesh.vertices.len();
        mesh.vertices.push(t.a);
        mesh.vertices.push(t.b);
        mesh.vertices.push(t.c);
        mesh.faces.push(Triangle { a: base, b: base + 1, c: base + 2 });
        color.push(t.color);
        alpha.push(t.alpha.clamp(0.0, 1.0));
        set.push(t.set);
    }
    let shape = to_parry_trimesh(&mesh)?;
    Some(SceneShape { shape, color, alpha, set })
}

// AI-FUNC-SUMMARY: Lambert headlight intensity for a hit normal (two-sided via abs); returns value in [ambient, 1]; side effects: none.
fn shade(normal: Vec3, ray_dir: Vec3, ambient: f64) -> f64 {
    let n_len = normal.dot(normal).sqrt();
    if n_len <= 1e-12 || !n_len.is_finite() {
        return ambient;
    }
    let ndl = (normal.dot(ray_dir).abs() / n_len).clamp(0.0, 1.0);
    ambient + (1.0 - ambient) * ndl
}

struct HitRec {
    t: f64,
    tri: u32,
    normal: Vec3,
}

fn collect_hits(shape: &SceneShape, origin: Vec3, dir: Vec3, max_t: f64) -> Vec<HitRec> {
    let ray = Ray::new(Point::new(origin.x, origin.y, origin.z), Vector::new(dir.x, dir.y, dir.z));
    let mut hits: Vec<HitRec> = Vec::new();
    let mut cb = |idx: &u32| -> bool {
        let tri = shape.shape.triangle(*idx);
        if let Some(hit) = tri.cast_local_ray_and_get_normal(&ray, max_t, false) {
            hits.push(HitRec {
                t: hit.time_of_impact,
                tri: *idx,
                normal: Vec3::new(hit.normal.x, hit.normal.y, hit.normal.z),
            });
        }
        true
    };
    let mut visitor = RayIntersectionsVisitor::new(&ray, max_t, &mut cb);
    shape.shape.qbvh().traverse_depth_first(&mut visitor);
    hits.sort_by(|a, b| a.t.total_cmp(&b.t).then(a.tri.cmp(&b.tri)));
    hits
}

// AI-FUNC-SUMMARY:
// Purpose: Render an extracted RenderScene to RGBA8 on the CPU with exact front-to-back transparency compositing (PLAN §9.5), plus curve/wireframe overlays and point markers.
// Inputs: scene (triangles with per-face color/alpha/set, segments, markers), validated camera, resolution, settings (RGBA background incl. alpha, ambient).
// Returns: RenderedImage; pixel alpha reflects accumulated coverage over the (possibly transparent) background.
// Side effects: Parallelizes over image rows with rayon.
// Notes: All hits along each ray are enumerated via a QBVH RayIntersectionsVisitor and per-triangle casts, sorted by distance; coincident hits within a relative tolerance are deduplicated preferring Face-set triangles over Volume-set boundary faces (welded interfaces would otherwise composite twice). Lines occlusion-test against the depth at which the surface pass became effectively opaque, with a small camera-ward bias so curves lying on surfaces stay visible.
pub fn render_scene_cpu(
    scene: &RenderScene,
    camera: &RenderCamera,
    width: usize,
    height: usize,
    settings: &SceneRenderSettings,
) -> RenderedImage {
    let bg = settings.background;
    let mut image = RenderedImage::filled(width, height, bg);
    let mut depth = vec![f64::INFINITY; width * height];

    let scene_shape = build_scene_shape(scene);
    let dedup_tol = (camera.far - camera.near).abs().max(1e-9) * 1e-9;

    if let Some(shape) = &scene_shape {
        let row_len = width * 4;
        image
            .rgba
            .par_chunks_mut(row_len)
            .zip(depth.par_chunks_mut(width))
            .enumerate()
            .for_each(|(py, (row, depth_row))| {
                for px in 0..width {
                    let (origin, dir) = camera.ray_for_pixel(px, py, width, height);
                    let mut hits = collect_hits(shape, origin, dir, f64::MAX);
                    dedup_coincident(&mut hits, shape, dedup_tol);

                    let mut acc = [0.0f64; 3];
                    let mut transmit = 1.0f64;
                    let mut opaque_depth = f64::INFINITY;
                    for h in &hits {
                        let a = shape.alpha[h.tri as usize];
                        if a <= 0.0 {
                            continue;
                        }
                        let intensity = shade(h.normal, dir, settings.ambient);
                        let c = shape.color[h.tri as usize];
                        for k in 0..3 {
                            acc[k] += transmit * a * c[k] as f64 * intensity;
                        }
                        transmit *= 1.0 - a;
                        if transmit < 1.0 / 512.0 {
                            opaque_depth = h.t;
                            break;
                        }
                    }
                    let bg_a = bg[3] as f64 / 255.0;
                    for k in 0..3 {
                        acc[k] += transmit * bg[k] as f64 * bg_a;
                    }
                    let out_a = 1.0 - transmit * (1.0 - bg_a);
                    row[px * 4] = acc[0].round().clamp(0.0, 255.0) as u8;
                    row[px * 4 + 1] = acc[1].round().clamp(0.0, 255.0) as u8;
                    row[px * 4 + 2] = acc[2].round().clamp(0.0, 255.0) as u8;
                    row[px * 4 + 3] = (out_a * 255.0).round().clamp(0.0, 255.0) as u8;
                    depth_row[px] = opaque_depth;
                }
            });
    }

    let depth_bias = (camera.far - camera.near).abs().max(1e-9) * 1e-4;
    for seg in &scene.segments {
        draw_segment(&mut image, &depth, camera, width, height, seg.a, seg.b, seg.color, depth_bias);
    }
    for m in &scene.markers {
        draw_marker(&mut image, camera, width, height, m.p, m.color);
    }

    image
}

fn dedup_coincident(hits: &mut Vec<HitRec>, shape: &SceneShape, tol: f64) {
    if hits.len() < 2 {
        return;
    }
    let mut out: Vec<HitRec> = Vec::with_capacity(hits.len());
    for h in hits.drain(..) {
        if let Some(last) = out.last_mut() {
            if (h.t - last.t).abs() <= tol {
                let keep_new = shape.set[h.tri as usize] > shape.set[last.tri as usize];
                if keep_new {
                    *last = h;
                }
                continue;
            }
        }
        out.push(h);
    }
    *hits = out;
}

// AI-FUNC-SUMMARY: Project a world point to (pixel x, pixel y, camera depth along forward); returns None behind the camera in perspective mode; side effects: none.
fn project_point(
    camera: &RenderCamera,
    p: Vec3,
    width: usize,
    height: usize,
) -> Option<(f64, f64, f64)> {
    let v = p.sub(camera.eye);
    let z = v.dot(camera.forward);
    let (ndc_x, ndc_y) = match camera.projection {
        RenderProjection::Orthographic => (
            v.dot(camera.right) / camera.ortho_half_width,
            v.dot(camera.up) / camera.ortho_half_height,
        ),
        RenderProjection::Perspective => {
            if z <= camera.near * 0.25 {
                return None;
            }
            (
                v.dot(camera.right) / z / (camera.persp_tan_half_fov * camera.aspect),
                v.dot(camera.up) / z / camera.persp_tan_half_fov,
            )
        }
    };
    let fx = (ndc_x + 1.0) * 0.5 * width as f64 - 0.5;
    let fy = (1.0 - ndc_y) * 0.5 * height as f64 - 0.5;
    Some((fx, fy, z))
}

#[allow(clippy::too_many_arguments)]
fn draw_segment(
    image: &mut RenderedImage,
    depth: &[f64],
    camera: &RenderCamera,
    width: usize,
    height: usize,
    a: Vec3,
    b: Vec3,
    color: [u8; 3],
    depth_bias: f64,
) {
    let (Some(pa), Some(pb)) = (
        project_point(camera, a, width, height),
        project_point(camera, b, width, height),
    ) else {
        return;
    };
    let steps = pa.0.abs().max(pb.0.abs()).max(1.0);
    let n = ((pb.0 - pa.0).abs().max((pb.1 - pa.1).abs()).ceil() as usize)
        .clamp(1, (steps as usize + width + height).max(2) * 2);
    for s in 0..=n {
        let f = s as f64 / n as f64;
        let x = pa.0 + (pb.0 - pa.0) * f;
        let y = pa.1 + (pb.1 - pa.1) * f;
        let z = pa.2 + (pb.2 - pa.2) * f;
        let (xi, yi) = (x.round() as i64, y.round() as i64);
        if xi < 0 || yi < 0 || xi >= width as i64 || yi >= height as i64 {
            continue;
        }
        let pix = yi as usize * width + xi as usize;
        if z - depth_bias <= depth[pix] {
            let o = pix * 4;
            image.rgba[o] = color[0];
            image.rgba[o + 1] = color[1];
            image.rgba[o + 2] = color[2];
            image.rgba[o + 3] = 255;
        }
    }
}

fn draw_marker(
    image: &mut RenderedImage,
    camera: &RenderCamera,
    width: usize,
    height: usize,
    p: Vec3,
    color: [u8; 3],
) {
    let Some((fx, fy, _)) = project_point(camera, p, width, height) else {
        return;
    };
    let (cx, cy) = (fx.round() as i64, fy.round() as i64);
    for d in -3i64..=3 {
        for (xi, yi) in [(cx + d, cy), (cx, cy + d)] {
            if xi >= 0 && yi >= 0 && xi < width as i64 && yi < height as i64 {
                let o = (yi as usize * width + xi as usize) * 4;
                image.rgba[o] = color[0];
                image.rgba[o + 1] = color[1];
                image.rgba[o + 2] = color[2];
                image.rgba[o + 3] = 255;
            }
        }
    }
}

// AI-FUNC-SUMMARY:
// Purpose: Resolve a named view preset to (view_direction, up_vector) for RenderCameraSpec.
// Inputs: name — front/back/left/right/top/bottom/iso_ne/iso_nw/iso_se/iso_sw (case-insensitive).
// Returns: Some((direction, up)) or None for unknown names.
// Side effects: None.
// Notes: Conventions — front looks along +Y (camera on the -Y side), right looks along -X, top looks along -Z with +Y up; iso presets look inward from the four upper compass corners with +Z up.
pub fn named_view(name: &str) -> Option<([f64; 3], [f64; 3])> {
    let z_up = [0.0, 0.0, 1.0];
    match name.trim().to_ascii_lowercase().as_str() {
        "front" => Some(([0.0, 1.0, 0.0], z_up)),
        "back" => Some(([0.0, -1.0, 0.0], z_up)),
        "left" => Some(([1.0, 0.0, 0.0], z_up)),
        "right" => Some(([-1.0, 0.0, 0.0], z_up)),
        "top" => Some(([0.0, 0.0, -1.0], [0.0, 1.0, 0.0])),
        "bottom" => Some(([0.0, 0.0, 1.0], [0.0, 1.0, 0.0])),
        "iso_ne" => Some(([-1.0, -1.0, -1.0], z_up)),
        "iso_nw" => Some(([1.0, -1.0, -1.0], z_up)),
        "iso_se" => Some(([-1.0, 1.0, -1.0], z_up)),
        "iso_sw" => Some(([1.0, 1.0, -1.0], z_up)),
        _ => None,
    }
}
