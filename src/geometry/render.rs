use super::bbox::mesh_bbox;
use super::collision::to_parry_trimesh;
use super::mesh_ops::vec_norm;
use crate::error::{Result, RustMsptError};
use crate::types::{Mesh, RenderedImage, Vec3};
use parry3d_f64::math::{Point, Vector};
use parry3d_f64::query::{Ray, RayCast};
use rayon::prelude::*;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RenderProjection {
    Orthographic,
    Perspective,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RenderCameraSpec {
    pub focus_point: [f64; 3],
    pub view_direction: [f64; 3],
    pub up_vector: Option<[f64; 3]>,
    pub projection: RenderProjection,
    pub perspective_fov_degrees: f64,
    pub camera_distance: Option<f64>,
    pub fit_padding: f64,
    pub width: usize,
    pub height: usize,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RenderCamera {
    pub eye: Vec3,
    pub forward: Vec3,
    pub right: Vec3,
    pub up: Vec3,
    pub projection: RenderProjection,
    pub ortho_half_width: f64,
    pub ortho_half_height: f64,
    pub persp_tan_half_fov: f64,
    pub aspect: f64,
    pub near: f64,
    pub far: f64,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RenderSettings {
    pub background: [u8; 3],
    pub base_color: [u8; 3],
    pub ambient: f64,
}

impl Default for RenderSettings {
    // AI-FUNC-SUMMARY: Default render appearance (white background, steel-blue surface, 0.25 ambient); returns RenderSettings; side effects: None.
    fn default() -> Self {
        Self {
            background: [255, 255, 255],
            base_color: [140, 160, 190],
            ambient: 0.25,
        }
    }
}

// AI-FUNC-SUMMARY:
// Purpose: Validate a 3-element vector and return it as a fixed-size array.
// Inputs: config field name for error messages, raw values.
// Returns: [f64; 3].
// Side effects: None.
// Notes: Returns InvalidConfig unless the slice has exactly 3 finite elements.
pub fn parse_render_vec3(name: &str, values: &[f64]) -> Result<[f64; 3]> {
    if values.len() != 3 {
        return Err(RustMsptError::InvalidConfig(format!(
            "render.{name} must have exactly 3 elements, got {}",
            values.len()
        )));
    }
    if values.iter().any(|v| !v.is_finite()) {
        return Err(RustMsptError::InvalidConfig(format!(
            "render.{name} must contain only finite values"
        )));
    }
    Ok([values[0], values[1], values[2]])
}

// AI-FUNC-SUMMARY: Parse projection config string ("orthographic"/"ortho" or "perspective"/"persp", case-insensitive); returns RenderProjection; side effects: None.
// Notes: Returns InvalidConfig for any other value.
pub fn parse_render_projection(raw: &str) -> Result<RenderProjection> {
    let normalized = raw.trim().to_ascii_lowercase();
    match normalized.as_str() {
        "orthographic" | "ortho" => Ok(RenderProjection::Orthographic),
        "perspective" | "persp" => Ok(RenderProjection::Perspective),
        _ => Err(RustMsptError::InvalidConfig(format!(
            "render.projection must be 'orthographic' or 'perspective', got '{raw}'"
        ))),
    }
}

// AI-FUNC-SUMMARY: Normalize a vector or report the named config field as invalid; returns unit Vec3; side effects: None.
fn normalize_or_err(name: &str, v: Vec3) -> Result<Vec3> {
    let n = vec_norm(v);
    if n <= 1e-12 {
        return Err(RustMsptError::InvalidConfig(format!(
            "render.{name} must be a non-zero vector"
        )));
    }
    Ok(v.scale(1.0 / n))
}

// AI-FUNC-SUMMARY: Build an orthonormal camera basis from a view direction and an up hint; returns (forward, right, up); side effects: None.
// Notes: Falls back to (0,1,0) then (1,0,0) when the up hint is nearly parallel to the view direction.
fn build_camera_basis(forward: Vec3, up_hint: Vec3) -> (Vec3, Vec3, Vec3) {
    let mut up = up_hint;
    if forward.cross(up).dot(forward.cross(up)).sqrt() <= 1e-9 {
        up = Vec3::new(0.0, 1.0, 0.0);
        if forward.cross(up).dot(forward.cross(up)).sqrt() <= 1e-9 {
            up = Vec3::new(1.0, 0.0, 0.0);
        }
    }
    let right = forward.cross(up);
    let right = right.scale(1.0 / vec_norm(right));
    let up = right.cross(forward);
    (forward, right, up)
}

// AI-FUNC-SUMMARY:
// Purpose: Build and validate a render camera from a camera specification.
// Inputs: mesh (for auto-framing from its bbox), spec (focus point, view direction, optional up
//         vector, projection, vertical FOV in degrees, optional explicit camera distance,
//         fit padding fraction, and output resolution in pixels).
// Returns: RenderCamera with orthonormal basis, auto-fitted extents, and near/far clip distances.
// Side effects: None.
// Notes: Orthographic auto-fits half extents to the mesh bbox projected onto the camera plane and
// adjusts for the image aspect ratio. Perspective auto distance fits the bbox bounding sphere
// within the FOV. All distances are derived from the bbox bounding-sphere radius around the focus.
pub fn build_render_camera(mesh: &Mesh, spec: &RenderCameraSpec) -> Result<RenderCamera> {
    let (width, height) = (spec.width, spec.height);
    let fit_padding = spec.fit_padding;
    if width == 0 || height == 0 {
        return Err(RustMsptError::InvalidConfig(
            "render.width and render.height must be positive".to_string(),
        ));
    }
    if !fit_padding.is_finite() || fit_padding < 0.0 {
        return Err(RustMsptError::InvalidConfig(
            "render.fit_padding must be a non-negative finite value".to_string(),
        ));
    }
    let forward = normalize_or_err(
        "view_direction",
        Vec3::new(
            spec.view_direction[0],
            spec.view_direction[1],
            spec.view_direction[2],
        ),
    )?;
    let focus = Vec3::new(
        spec.focus_point[0],
        spec.focus_point[1],
        spec.focus_point[2],
    );
    let up_hint = match spec.up_vector {
        Some(v) => normalize_or_err("up_vector", Vec3::new(v[0], v[1], v[2]))?,
        None => Vec3::new(0.0, 0.0, 1.0),
    };
    let (forward, right, up) = build_camera_basis(forward, up_hint);

    let bbox = mesh_bbox(mesh)
        .ok_or_else(|| RustMsptError::InvalidMesh("cannot render an empty mesh".to_string()))?;
    let corners = bbox_corners(bbox);
    let radius = corners
        .iter()
        .map(|c| vec_norm(c.sub(focus)))
        .fold(0.0_f64, f64::max)
        .max(1e-6);

    if let Some(d) = spec.camera_distance {
        if !d.is_finite() || d <= 0.0 {
            return Err(RustMsptError::InvalidConfig(
                "render.camera_distance must be a positive finite value".to_string(),
            ));
        }
    }

    let aspect = width as f64 / height as f64;
    let mut camera = RenderCamera {
        eye: focus,
        forward,
        right,
        up,
        projection: spec.projection,
        ortho_half_width: 0.0,
        ortho_half_height: 0.0,
        persp_tan_half_fov: 0.0,
        aspect,
        near: 0.0,
        far: 0.0,
    };

    match spec.projection {
        RenderProjection::Orthographic => {
            let distance = spec.camera_distance.unwrap_or(2.0 * radius);
            let mut extent_right = corners
                .iter()
                .map(|c| c.sub(focus).dot(right).abs())
                .fold(0.0_f64, f64::max);
            let mut extent_up = corners
                .iter()
                .map(|c| c.sub(focus).dot(up).abs())
                .fold(0.0_f64, f64::max);
            if extent_right <= 1e-12 && extent_up <= 1e-12 {
                extent_right = radius;
                extent_up = radius;
            } else if extent_right <= 1e-12 {
                extent_right = extent_up * aspect;
            } else if extent_up <= 1e-12 {
                extent_up = extent_right / aspect;
            }
            if extent_right / extent_up > aspect {
                extent_up = extent_right / aspect;
            } else {
                extent_right = extent_up * aspect;
            }
            camera.eye = focus.sub(forward.scale(distance));
            camera.ortho_half_width = extent_right * (1.0 + fit_padding);
            camera.ortho_half_height = extent_up * (1.0 + fit_padding);
            camera.near = (distance - 2.0 * radius).max(radius * 1e-3);
            camera.far = distance + 2.0 * radius;
        }
        RenderProjection::Perspective => {
            if !spec.perspective_fov_degrees.is_finite()
                || spec.perspective_fov_degrees <= 0.0
                || spec.perspective_fov_degrees >= 179.0
            {
                return Err(RustMsptError::InvalidConfig(
                    "render.perspective_fov_degrees must be in (0, 179)".to_string(),
                ));
            }
            let half_fov = spec.perspective_fov_degrees.to_radians() * 0.5;
            let tan_half = half_fov.tan();
            let tan_half_horizontal = tan_half * aspect;
            let fit_distance = radius
                / half_fov.sin().min(
                    tan_half_horizontal / (1.0 + tan_half_horizontal * tan_half_horizontal).sqrt(),
                );
            let distance = spec
                .camera_distance
                .unwrap_or(fit_distance * (1.0 + fit_padding));
            camera.eye = focus.sub(forward.scale(distance));
            camera.persp_tan_half_fov = tan_half;
            camera.near = (distance - 2.0 * radius).max(distance * 1e-4);
            camera.far = distance + 2.0 * radius;
        }
    }

    Ok(camera)
}

// AI-FUNC-SUMMARY: Return the 8 corner points of an axis-aligned bounding box; returns [Vec3; 8]; side effects: None.
fn bbox_corners(bbox: crate::types::BoundingBox) -> [Vec3; 8] {
    let (mn, mx) = (bbox.min, bbox.max);
    [
        Vec3::new(mn.x, mn.y, mn.z),
        Vec3::new(mx.x, mn.y, mn.z),
        Vec3::new(mn.x, mx.y, mn.z),
        Vec3::new(mx.x, mx.y, mn.z),
        Vec3::new(mn.x, mn.y, mx.z),
        Vec3::new(mx.x, mn.y, mx.z),
        Vec3::new(mn.x, mx.y, mx.z),
        Vec3::new(mx.x, mx.y, mx.z),
    ]
}

impl RenderCamera {
    // AI-FUNC-SUMMARY: Compute the world-space ray for a pixel (top-left origin, pixel centers sampled); returns (origin, unit direction); side effects: None.
    pub fn ray_for_pixel(&self, px: usize, py: usize, width: usize, height: usize) -> (Vec3, Vec3) {
        let ndc_x = ((px as f64 + 0.5) / width as f64) * 2.0 - 1.0;
        let ndc_y = 1.0 - ((py as f64 + 0.5) / height as f64) * 2.0;
        match self.projection {
            RenderProjection::Orthographic => {
                let origin = self
                    .eye
                    .add(self.right.scale(ndc_x * self.ortho_half_width))
                    .add(self.up.scale(ndc_y * self.ortho_half_height));
                (origin, self.forward)
            }
            RenderProjection::Perspective => {
                let dir = self
                    .forward
                    .add(
                        self.right
                            .scale(ndc_x * self.persp_tan_half_fov * self.aspect),
                    )
                    .add(self.up.scale(ndc_y * self.persp_tan_half_fov));
                (self.eye, dir.scale(1.0 / vec_norm(dir)))
            }
        }
    }

    // AI-FUNC-SUMMARY: Build the row-major view-projection matrix (wgpu NDC conventions: x/y in [-1,1], z in [0,1]); returns 4x4 matrix; side effects: None.
    // Notes: Uses the camera's stored near/far distances; view space looks down -Z (right-handed).
    pub fn view_proj_matrix(&self) -> [[f64; 4]; 4] {
        let (r, u, f, e) = (self.right, self.up, self.forward, self.eye);
        let view = [
            [r.x, r.y, r.z, -r.dot(e)],
            [u.x, u.y, u.z, -u.dot(e)],
            [-f.x, -f.y, -f.z, f.dot(e)],
            [0.0, 0.0, 0.0, 1.0],
        ];
        let range = (self.far - self.near).max(1e-12);
        let proj = match self.projection {
            RenderProjection::Orthographic => [
                [1.0 / self.ortho_half_width, 0.0, 0.0, 0.0],
                [0.0, 1.0 / self.ortho_half_height, 0.0, 0.0],
                [0.0, 0.0, -1.0 / range, -self.near / range],
                [0.0, 0.0, 0.0, 1.0],
            ],
            RenderProjection::Perspective => {
                let f1 = 1.0 / (self.persp_tan_half_fov * self.aspect);
                let f2 = 1.0 / self.persp_tan_half_fov;
                let a = self.far / (self.near - self.far);
                let b = self.far * self.near / (self.near - self.far);
                [
                    [f1, 0.0, 0.0, 0.0],
                    [0.0, f2, 0.0, 0.0],
                    [0.0, 0.0, a, b],
                    [0.0, 0.0, -1.0, 0.0],
                ]
            }
        };
        mat4_mul(&proj, &view)
    }
}

// AI-FUNC-SUMMARY: Multiply two row-major 4x4 matrices (a * b); returns 4x4 matrix; side effects: None.
fn mat4_mul(a: &[[f64; 4]; 4], b: &[[f64; 4]; 4]) -> [[f64; 4]; 4] {
    let mut out = [[0.0; 4]; 4];
    for (i, row) in out.iter_mut().enumerate() {
        for (j, cell) in row.iter_mut().enumerate() {
            *cell = (0..4).map(|k| a[i][k] * b[k][j]).sum();
        }
    }
    out
}

// AI-FUNC-SUMMARY: Lambert-style headlight intensity for a hit normal; returns value in [ambient, 1]; side effects: None.
// Notes: The ray direction always points away from the camera, so -dir is the direction to the camera;
// parry3d orients hit normals against the ray, making the dot product non-negative for frontmost hits.
fn shade_intensity(normal: Vec3, ray_dir: Vec3, ambient: f64) -> f64 {
    let n_len = vec_norm(normal);
    if n_len <= 1e-12 || !n_len.is_finite() {
        return ambient;
    }
    let n = normal.scale(1.0 / n_len);
    let to_camera = ray_dir.scale(-1.0);
    let ndl = n.dot(to_camera).clamp(0.0, 1.0);
    ambient + (1.0 - ambient) * ndl
}

// AI-FUNC-SUMMARY: Apply intensity to a base channel and round to u8; returns shaded channel; side effects: None.
fn shade_channel(base: u8, intensity: f64) -> u8 {
    (base as f64 * intensity).round().clamp(0.0, 255.0) as u8
}

// AI-FUNC-SUMMARY:
// Purpose: Render a mesh into an RGBA8 image on the CPU via BVH-accelerated ray casting.
// Inputs: mesh, validated camera, output resolution, shading settings.
// Returns: RenderedImage (background-filled where no triangle is hit).
// Side effects: Parallelizes over disjoint pixel tasks in the current Rayon pool.
// Notes: Uses parry3d's QBVH-backed cast_local_ray_and_get_normal with the nearest hit; degenerate
// triangles never intersect a ray (zero cross product), matching GPU rasterization behavior.
// Rows are written top-to-bottom, matching image conventions.
pub fn render_mesh_cpu(
    mesh: &Mesh,
    camera: &RenderCamera,
    width: usize,
    height: usize,
    settings: &RenderSettings,
) -> RenderedImage {
    render_mesh_cpu_with_tiles(mesh, camera, width, height, settings, cpu_render_tile_pixels(width, height))
}

// AI-FUNC-SUMMARY: Choose bounded contiguous pixel tasks, grouping short rows and splitting wide rows; use one serial task with a single worker, without creating another pool.
pub(super) fn cpu_render_tile_pixels(width: usize, height: usize) -> usize {
    let total = width.saturating_mul(height).max(1);
    let workers = rayon::current_num_threads();
    // Keep existing row parallelism when the minimum tile grain would leave too few tasks.
    if height >= workers && total <= workers.saturating_mul(1024) { return width.max(1); }
    if workers == 1 { return total; }
    let target = (total / workers.saturating_mul(4)).clamp(256, 4096);
    if width > 0 && width <= target { (target / width).max(1) * width } else { target }
}

// AI-FUNC-SUMMARY: Render nearest-hit pixels in disjoint contiguous tasks using exact original pixel coordinates; an explicit grain supports row-reference correctness and performance comparisons.
fn render_mesh_cpu_with_tiles(mesh: &Mesh, camera: &RenderCamera, width: usize, height: usize, settings: &RenderSettings, tile_pixels: usize) -> RenderedImage {
    let background = [
        settings.background[0],
        settings.background[1],
        settings.background[2],
        255,
    ];
    let mut image = RenderedImage::filled(width, height, background);
    let Some(shape) = to_parry_trimesh(mesh) else {
        return image;
    };

    image
        .rgba
        .par_chunks_mut(tile_pixels * 4)
        .enumerate()
        .for_each(|(tile, row)| {
            let start = tile * tile_pixels;
            let mut py = start / width;
            let mut px = start % width;
            for local in 0..row.len() / 4 {
                let (origin, dir) = camera.ray_for_pixel(px, py, width, height);
                let ray = Ray::new(
                    Point::new(origin.x, origin.y, origin.z),
                    Vector::new(dir.x, dir.y, dir.z),
                );
                if let Some(hit) = shape.cast_local_ray_and_get_normal(&ray, f64::MAX, false) {
                    let normal = Vec3::new(hit.normal.x, hit.normal.y, hit.normal.z);
                    let intensity = shade_intensity(normal, dir, settings.ambient);
                    row[local * 4] = shade_channel(settings.base_color[0], intensity);
                    row[local * 4 + 1] = shade_channel(settings.base_color[1], intensity);
                    row[local * 4 + 2] = shade_channel(settings.base_color[2], intensity);
                }
                px += 1;
                if px == width { px = 0; py += 1; }
            }
        });

    image
}

#[cfg(test)]
pub(crate) mod tile_tests {
    use super::*;

    // AI-FUNC-SUMMARY: Build a fixed camera whose footprint stays constant across aspect ratios, exposing real ray work in strip-image scheduling tests.
    pub(crate) fn fixture_camera(projection: RenderProjection) -> RenderCamera {
        RenderCamera { eye: Vec3::new(0.5,0.5,-3.0), forward: Vec3::new(0.0,0.0,1.0), right: Vec3::new(1.0,0.0,0.0), up: Vec3::new(0.0,1.0,0.0), projection,
            ortho_half_width: 0.6, ortho_half_height: 0.6, persp_tan_half_fov: 0.2, aspect: 1.0, near: 0.1, far: 10.0 }
    }

    // AI-FUNC-SUMMARY: Compare tiled nearest-hit output byte-for-byte against forced row scheduling for both projections, ragged tasks, tiny images and extreme aspect ratios at 1/2/8 workers.
    #[test]
    fn mesh_pixel_tiles_match_rows() {
        let mesh = crate::geometry::box_mesh(crate::types::BoundingBox::from_size(Vec3::new(1.0,1.0,1.0)));
        for projection in [RenderProjection::Orthographic, RenderProjection::Perspective] {
            let camera = fixture_camera(projection);
            for (w,h) in [(1,1),(37,29),(65537,1),(1,65537),(8193,3)] {
                let expected = render_mesh_cpu_with_tiles(&mesh,&camera,w,h,&RenderSettings::default(),w);
                for workers in [1,2,8] {
                    let pool = rayon::ThreadPoolBuilder::new().num_threads(workers).build().unwrap();
                    let actual = pool.install(|| render_mesh_cpu(&mesh,&camera,w,h,&RenderSettings::default()));
                    assert_eq!(actual.rgba,expected.rgba,"{projection:?} {w}x{h} workers={workers}");
                }
            }
        }
    }

    // AI-FUNC-SUMMARY: Verify small-image comparisons really retain the reference row grain while a single long row supplies work to every configured worker.
    #[test]
    fn pixel_grain_preserves_small_row_parallelism() {
        let pool = rayon::ThreadPoolBuilder::new().num_threads(8).build().unwrap();
        pool.install(|| {
            for (w,h) in [(32,32),(37,29),(64,64),(65,65)] {
                assert_eq!(cpu_render_tile_pixels(w,h),w);
            }
            let grain = cpu_render_tile_pixels(65536,1);
            assert!(65536usize.div_ceil(grain) >= rayon::current_num_threads());
        });
    }

    // AI-FUNC-SUMMARY: Benchmark row versus pixel scheduling with identical ray work and QBVH preparation, reporting warm alternating raw samples under 1/2/8 worker pools.
    #[test]
    #[ignore = "release render scheduling benchmark"]
    fn render_pixel_tile_benchmark() {
        let mesh = crate::geometry::icosphere_mesh(Vec3::new(0.5,0.5,0.5),0.5,3);
        let camera = fixture_camera(RenderProjection::Orthographic);
        for workers in [1,2,8] {
            let pool = rayon::ThreadPoolBuilder::new().num_threads(workers).build().unwrap();
            for (w,h) in [(65536,1),(1,65536),(256,256),(32,32)] {
                let run = |legacy: bool| pool.install(|| {
                    let now = std::time::Instant::now();
                    for _ in 0..3 {
                        let grain = if legacy { w } else { cpu_render_tile_pixels(w,h) };
                        std::hint::black_box(render_mesh_cpu_with_tiles(&mesh,&camera,w,h,&RenderSettings::default(),grain));
                    }
                    now.elapsed().as_secs_f64()
                });
                for sample in 0..6 {
                    let (old,new) = if sample%2==0 { (run(true),run(false)) } else { let new=run(false); (run(true),new) };
                    eprintln!("RENDER_TILE_BENCH workers={workers} width={w} height={h} sample={sample} repeats=3 rows={old:.9} tiles={new:.9}");
                }
            }
        }
    }
}
