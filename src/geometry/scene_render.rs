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
    opaque: bool,
}

// AI-FUNC-SUMMARY: Prepare immutable triangle geometry/material arrays and select strictly opaque scenes with at least 192 triangles for nearest-group rendering.
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
    // The measured 24-triangle scene loses to the extra traversal; the first
    // profitable layered case has 192 triangles. Retain all-hits below that.
    let opaque = alpha.len() >= 192 && alpha.iter().all(|&a| a == 1.0);
    Some(SceneShape { shape, color, alpha, set, opaque })
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

#[derive(Clone, Copy)]
struct HitRec {
    t: f64,
    tri: u32,
    normal: Vec3,
}

// AI-FUNC-SUMMARY: Clear and refill reusable all-hit scratch, preserving distance/triangle sorting.
fn collect_hits(shape: &SceneShape, origin: Vec3, dir: Vec3, max_t: f64, hits: &mut Vec<HitRec>) {
    let ray = Ray::new(Point::new(origin.x, origin.y, origin.z), Vector::new(dir.x, dir.y, dir.z));
    hits.clear();
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
}

// AI-FUNC-SUMMARY: Find the first opaque coincidence group with a nearest QBVH query followed by a bounded all-hit query; preserve distance/triangle sorting, Face priority and the selected triangle normal/depth. Transparency and zero-alpha materials use the full reference path.
fn collect_opaque_hits(shape: &SceneShape, origin: Vec3, dir: Vec3, tol: f64, hits: &mut Vec<HitRec>) {
    let ray = Ray::new(Point::new(origin.x, origin.y, origin.z), Vector::new(dir.x, dir.y, dir.z));
    hits.clear();
    if let Some(nearest) = shape.shape.cast_local_ray_and_get_normal(&ray, f64::MAX, false) {
        // Face can replace Volume once, moving the dedup anchor by at most tol.
        // Round outward so boundary coincidences survive the finite query bound.
        let max_t = (nearest.time_of_impact + 2.0 * tol).next_up();
        collect_hits(shape, origin, dir, max_t, hits);
        dedup_coincident(hits, shape, tol);
        hits.truncate(1);
    }
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
    PreparedScene::new(scene).render(camera, width, height, settings)
}

// AI-FUNC-SUMMARY: Borrow a scene and retain its immutable QBVH/materials across camera renders.
pub struct PreparedScene<'a> {
    scene: &'a RenderScene,
    shape: Option<SceneShape>,
}

impl<'a> PreparedScene<'a> {
    // AI-FUNC-SUMMARY: Build one scene accelerator; the borrowed scene cannot change during reuse.
    pub fn new(scene: &'a RenderScene) -> Self {
        Self { scene, shape: build_scene_shape(scene) }
    }

    // AI-FUNC-SUMMARY: Render a camera using the retained accelerator and task-local hit scratch.
    pub fn render(&self, camera: &RenderCamera, width: usize, height: usize, settings: &SceneRenderSettings) -> RenderedImage {
        render_prepared_scene(self.scene, self.shape.as_ref(), camera, width, height, settings, super::render::cpu_render_tile_pixels(width, height))
    }
}

// AI-FUNC-SUMMARY: Composite reference all-hits or the opaque nearest coincidence group, preserving face priority and overlay depth.
fn render_prepared_scene(scene: &RenderScene, scene_shape: Option<&SceneShape>, camera: &RenderCamera, width: usize, height: usize, settings: &SceneRenderSettings, tile_pixels: usize) -> RenderedImage {
    let bg = settings.background;
    let mut image = RenderedImage::filled(width, height, bg);
    let mut depth = vec![f64::INFINITY; width * height];

    let dedup_tol = (camera.far - camera.near).abs().max(1e-9) * 1e-9;

    if let Some(shape) = scene_shape {
        image
            .rgba
            .par_chunks_mut(tile_pixels * 4)
            .zip(depth.par_chunks_mut(tile_pixels))
            .enumerate()
            .for_each_init(Vec::new, |hits, (tile, (row, depth_row))| {
                let start = tile * tile_pixels;
                let mut py = start / width;
                let mut px = start % width;
                for local in 0..depth_row.len() {
                    let (origin, dir) = camera.ray_for_pixel(px, py, width, height);
                    if shape.opaque {
                        collect_opaque_hits(shape, origin, dir, dedup_tol, hits);
                    } else {
                        collect_hits(shape, origin, dir, f64::MAX, hits);
                        dedup_coincident(hits, shape, dedup_tol);
                    }

                    let mut acc = [0.0f64; 3];
                    let mut transmit = 1.0f64;
                    let mut opaque_depth = f64::INFINITY;
                    for h in hits.iter() {
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
                    row[local * 4] = acc[0].round().clamp(0.0, 255.0) as u8;
                    row[local * 4 + 1] = acc[1].round().clamp(0.0, 255.0) as u8;
                    row[local * 4 + 2] = acc[2].round().clamp(0.0, 255.0) as u8;
                    row[local * 4 + 3] = (out_a * 255.0).round().clamp(0.0, 255.0) as u8;
                    depth_row[local] = opaque_depth;
                    px += 1;
                    if px == width { px = 0; py += 1; }
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

// AI-FUNC-SUMMARY: Compact sorted coincident hits in place, retaining the existing Face-over-Volume priority and moving anchor.
fn dedup_coincident(hits: &mut Vec<HitRec>, shape: &SceneShape, tol: f64) {
    let mut kept = 0;
    for read in 0..hits.len() {
        let h = hits[read];
        if kept > 0 && (h.t - hits[kept - 1].t).abs() <= tol {
            if shape.set[h.tri as usize] > shape.set[hits[kept - 1].tri as usize] {
                hits[kept - 1] = h;
            }
        } else {
            hits[kept] = h;
            kept += 1;
        }
    }
    hits.truncate(kept);
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

#[cfg(test)]
mod tile_tests {
    use super::*;
    use crate::geometry::render::tile_tests::fixture_camera;
    use crate::meshgen::render_scene::{SceneTri,SceneSegment};

    // AI-FUNC-SUMMARY: Compare nearest-group and full-hit selection at coincidence boundaries, triangle-order ties, misses, and a moved Face anchor.
    #[test]
    fn opaque_nearest_preserves_coincidence_selection() {
        for delta in [0.0, 1e-6_f64.next_down(), 1e-6, 1e-6_f64.next_up(), 1.5e-6, 3e-6] {
            for reverse in [false, true] {
                let mut scene = RenderScene::default();
                for (z, set) in [(1.0, SetKind::Volume), (1.0+delta, SetKind::Face), (1.0+delta+0.5e-6, SetKind::Face), (3.0, SetKind::Volume)] {
                    scene.tris.push(SceneTri { a:Vec3::new(-2.0,-2.0,z), b:Vec3::new(2.0,-2.0,z), c:Vec3::new(0.0,2.0,z), color:[80,160,40], alpha:1.0, set });
                }
                if reverse { scene.tris.reverse(); }
                let shape = build_scene_shape(&scene).unwrap();
                for x in [0.0, 8.0] {
                    let origin=Vec3::new(x,0.0,0.0); let dir=Vec3::new(0.0,0.0,1.0);
                    let mut reference=Vec::new(); let mut nearest=Vec::new();
                    collect_hits(&shape,origin,dir,f64::MAX,&mut reference);
                    dedup_coincident(&mut reference,&shape,1e-6);
                    collect_opaque_hits(&shape,origin,dir,1e-6,&mut nearest);
                    assert_eq!(nearest.len(),reference.len().min(1));
                    if let Some(expected)=reference.first() {
                        assert_eq!(nearest[0].t,expected.t);
                        assert_eq!(nearest[0].tri,expected.tri);
                        assert_eq!(nearest[0].normal,expected.normal);
                    }
                }
            }
        }
    }

    // AI-FUNC-SUMMARY: Build layered opaque cubes with coincident Face colors and an overlay for output and performance comparisons.
    fn opaque_layers(layers: usize) -> RenderScene {
        let mesh=crate::geometry::box_mesh(crate::types::BoundingBox::from_size(Vec3::new(1.0,1.0,1.0)));
        let mut scene=RenderScene::default();
        for layer in 0..layers {
            let shift=Vec3::new(0.0,0.0,layer as f64 * 0.01);
            for f in &mesh.faces {
                let tri=SceneTri { a:mesh.vertices[f.a].add(shift),b:mesh.vertices[f.b].add(shift),c:mesh.vertices[f.c].add(shift),color:[80,160,40],alpha:1.0,set:SetKind::Volume };
                scene.tris.push(tri.clone());
                scene.tris.push(SceneTri { color:[160,40,80],set:SetKind::Face,..tri });
            }
        }
        scene.segments.push(SceneSegment { a:Vec3::new(0.0,0.5,0.0), b:Vec3::new(1.0,0.5,0.0), color:[0,255,0] });
        scene
    }

    // AI-FUNC-SUMMARY: Compare opaque complete RGBA/depth-tested overlays to the all-hit reference under both projections and worker budgets; reject fast-path classification for partial/zero/NaN opacity.
    #[test]
    fn opaque_nearest_images_match_all_hits() {
        let mut scene=opaque_layers(8);
        let settings=SceneRenderSettings { background:[40,10,20,64],ambient:0.3 };
        let mut reference=PreparedScene::new(&scene);
        reference.shape.as_mut().unwrap().opaque=false;
        let fast=PreparedScene::new(&scene);
        assert!(fast.shape.as_ref().unwrap().opaque);
        for projection in [RenderProjection::Orthographic,RenderProjection::Perspective] {
            let camera=fixture_camera(projection);
            let expected=reference.render(&camera,37,29,&settings);
            for workers in [1,2,8] {
                let pool=rayon::ThreadPoolBuilder::new().num_threads(workers).build().unwrap();
                assert_eq!(pool.install(||fast.render(&camera,37,29,&settings)).rgba,expected.rgba);
            }
        }
        for alpha in [0.0,0.5,f64::NAN] {
            scene.tris[0].alpha=alpha;
            assert!(!build_scene_shape(&scene).unwrap().opaque);
        }
    }

    // AI-FUNC-SUMMARY: Measure prepared opaque nearest-group versus all-hit rendering using identical images, warmed alternating samples, excluding scene construction/PNG.
    #[test]
    #[ignore = "release opaque nearest benchmark"]
    fn opaque_nearest_benchmark() {
        let camera=fixture_camera(RenderProjection::Orthographic);
        let settings=SceneRenderSettings::default();
        for layers in [1,8,64] {
            let scene=opaque_layers(layers);
            let mut reference=PreparedScene::new(&scene);
            reference.shape.as_mut().unwrap().opaque=false;
            let mut fast=PreparedScene::new(&scene);
            fast.shape.as_mut().unwrap().opaque=true;
            for workers in [1,8] {
                let pool=rayon::ThreadPoolBuilder::new().num_threads(workers).build().unwrap();
                pool.install(||assert_eq!(reference.render(&camera,256,256,&settings).rgba,fast.render(&camera,256,256,&settings).rgba));
                let run=|prepared:&PreparedScene|pool.install(|| {
                    let now=std::time::Instant::now();
                    std::hint::black_box(prepared.render(&camera,256,256,&settings));
                    now.elapsed().as_secs_f64()
                });
                run(&reference); run(&fast);
                for sample in 0..5 {
                    let (old,new)=if sample%2==0 {(run(&reference),run(&fast))}else {let new=run(&fast);(run(&reference),new)};
                    eprintln!("OPAQUE_BENCH layers={layers} workers={workers} sample={sample} all={old:.9} nearest={new:.9}");
                }
            }
        }
    }

    // AI-FUNC-SUMMARY: Verify tiled transparent/coincident face compositing and depth-tested overlays against forced rows for ragged, wide, tall and tiny images at 1/2/8 workers.
    #[test]
    fn scene_pixel_tiles_match_rows() {
        let mesh = crate::geometry::box_mesh(crate::types::BoundingBox::from_size(Vec3::new(1.0,1.0,1.0)));
        let mut scene = RenderScene::default();
        for face in &mesh.faces {
            scene.tris.push(SceneTri { a:mesh.vertices[face.a], b:mesh.vertices[face.b], c:mesh.vertices[face.c], color:[180,70,20], alpha:0.5, set:SetKind::Volume });
            scene.tris.push(SceneTri { a:mesh.vertices[face.a], b:mesh.vertices[face.b], c:mesh.vertices[face.c], color:[20,70,180], alpha:0.5, set:SetKind::Face });
        }
        scene.segments.push(SceneSegment { a:Vec3::new(0.0,0.5,0.5), b:Vec3::new(1.0,0.5,0.5), color:[0,255,0] });
        let prepared = PreparedScene::new(&scene);
        let settings = SceneRenderSettings { background:[40,10,20,64], ambient:0.3 };
        for projection in [RenderProjection::Orthographic,RenderProjection::Perspective] {
            let camera = fixture_camera(projection);
            for (w,h) in [(1,1),(37,29),(65537,1),(1,65537),(8193,3)] {
                let expected = render_prepared_scene(&scene,prepared.shape.as_ref(),&camera,w,h,&settings,w);
                for workers in [1,2,8] {
                    let pool = rayon::ThreadPoolBuilder::new().num_threads(workers).build().unwrap();
                    let actual = pool.install(|| prepared.render(&camera,w,h,&settings));
                    assert_eq!(actual.rgba,expected.rgba,"{projection:?} {w}x{h} workers={workers}");
                }
            }
        }
    }
    // AI-FUNC-SUMMARY: Measure prepared opaque/transparent scene task grains independently of accelerator construction and PNG output, with alternating warm samples and row-output equality checks.
    #[test]
    #[ignore = "release scene scheduling benchmark"]
    fn scene_pixel_tile_benchmark() {
        let mesh = crate::geometry::box_mesh(crate::types::BoundingBox::from_size(Vec3::new(1.0,1.0,1.0)));
        let camera = fixture_camera(RenderProjection::Orthographic);
        for alpha in [1.0,0.5] {
            let mut scene = RenderScene::default();
            for f in &mesh.faces { scene.tris.push(SceneTri { a:mesh.vertices[f.a], b:mesh.vertices[f.b], c:mesh.vertices[f.c], color:[80,160,40], alpha, set:SetKind::Volume }); }
            let prepared = PreparedScene::new(&scene);
            let settings = SceneRenderSettings::default();
            for workers in [1,2,8] {
                let pool = rayon::ThreadPoolBuilder::new().num_threads(workers).build().unwrap();
                for (w,h) in [(65536,1),(256,256),(32,32),(37,29),(64,64),(65,65)] {
                    pool.install(|| assert_eq!(prepared.render(&camera,w,h,&settings).rgba,render_prepared_scene(&scene,prepared.shape.as_ref(),&camera,w,h,&settings,w).rgba));
                    let run = |legacy: bool| pool.install(|| {
                        let now = std::time::Instant::now();
                        for _ in 0..3 {
                            let grain = if legacy { w } else { super::super::render::cpu_render_tile_pixels(w,h) };
                            std::hint::black_box(render_prepared_scene(&scene,prepared.shape.as_ref(),&camera,w,h,&settings,grain));
                        }
                        now.elapsed().as_secs_f64()
                    });
                    for sample in 0..6 {
                        let (old,new) = if sample%2==0 { (run(true),run(false)) } else { let new=run(false); (run(true),new) };
                        eprintln!("SCENE_TILE_BENCH alpha={alpha} workers={workers} width={w} height={h} sample={sample} repeats=3 rows={old:.9} tiles={new:.9}");
                    }
                }
            }
        }
    }

}
