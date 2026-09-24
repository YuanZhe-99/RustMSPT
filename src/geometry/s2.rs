use super::mesh_query::{PreparedMeshQuery, MeshQueryScratch};
use crate::types::{BoundingBox, Mesh, Vec3};
use super::bbox::mesh_bbox;
use super::mesh_ops::split_mesh_into_granules;
use super::volume::volume_fraction_in_bbox;
use rand::Rng;
use rayon::prelude::*;
use rustfft::num_complex::Complex;
use rustfft::{Fft, FftPlanner};
use std::sync::Arc;
use std::cell::RefCell;

pub const RAY_DIR_GPU: (f64, f64, f64) = (0.9428090415820634, 0.2705980500730985, 0.19611613513818402);

// AI-FUNC-SUMMARY: Convert 3D voxel index to flat array index using y/z strides; returns usize; side effects: None.
fn index_3d_to_flat(x: usize, y: usize, z: usize, ny: usize, nz: usize) -> usize {
    x * ny * nz + y * nz + z
}

// AI-FUNC-SUMMARY:
// Purpose: Moller-Trumbore ray-triangle intersection test.
// Inputs: ray origin/direction and triangle vertices a/b/c.
// Returns: Some(t) distance on hit, None on miss or near-parallel ray.
// Side effects: None.
// Notes: Returns only positive t (forward intersections).
pub(super) fn ray_intersects_triangle(origin: Vec3, dir: Vec3, a: Vec3, b: Vec3, c: Vec3) -> Option<f64> {
    let eps = 1e-10;
    let edge1 = b.sub(a);
    let edge2 = c.sub(a);
    let h = dir.cross(edge2);
    let det = edge1.dot(h);
    if det.abs() <= eps {
        return None;
    }

    let inv_det = 1.0 / det;
    let s = origin.sub(a);
    let u = inv_det * s.dot(h);
    if !(0.0 - eps..=1.0 + eps).contains(&u) {
        return None;
    }

    let q = s.cross(edge1);
    let v = inv_det * dir.dot(q);
    if v < -eps || (u + v) > 1.0 + eps {
        return None;
    }

    let t = inv_det * edge2.dot(q);
    if t > eps {
        Some(t)
    } else {
        None
    }
}

// AI-FUNC-SUMMARY:
// Purpose: Determine whether a point is inside a closed mesh using ray-casting (odd-hit rule).
// Inputs: mesh reference and query point.
// Returns: true if point is inside the mesh volume.
// Side effects: None.
// Notes: Uses a fixed non-axis-aligned ray direction to reduce edge-case misses. Deduplicates near-equal hit distances.
pub fn point_inside_mesh(mesh: &Mesh, point: Vec3) -> bool {
    let Some(bb) = mesh_bbox(mesh) else {
        return false;
    };
    let eps = 1e-9;
    if point.x < bb.min.x - eps
        || point.y < bb.min.y - eps
        || point.z < bb.min.z - eps
        || point.x > bb.max.x + eps
        || point.y > bb.max.y + eps
        || point.z > bb.max.z + eps
    {
        return false;
    }

    let dir = Vec3::new(0.9428090415820634, 0.2705980500730985, 0.19611613513818402);
    let mut ts: Vec<f64> = Vec::new();
    for f in &mesh.faces {
        let a = mesh.vertices[f.a];
        let b = mesh.vertices[f.b];
        let c = mesh.vertices[f.c];
        if let Some(t) = ray_intersects_triangle(point, dir, a, b, c) {
            ts.push(t);
        }
    }

    if ts.is_empty() {
        return false;
    }

    ts.sort_by(|x, y| x.partial_cmp(y).unwrap_or(std::cmp::Ordering::Equal));
    let mut unique_hits = 0usize;
    let mut last_t = f64::NEG_INFINITY;
    for t in ts {
        if (t - last_t).abs() > 1e-8 {
            unique_hits += 1;
            last_t = t;
        }
    }

    unique_hits % 2 == 1
}

// AI-FUNC-SUMMARY:
// Purpose: Voxelize a mesh inside a bounding box by ray-casting point containment tests.
// Inputs: mesh, bounding box, voxel pitch.
// Returns: Tuple of (occupancy boolean grid, [nx, ny, nz] grid dimensions).
// Side effects: None.
// Notes: Parallel x-slabs reuse scratch and prepared per-granule BVHs; voxel ranges are clamped to the domain.
fn build_bbox_occupancy(mesh: &Mesh, bbox: BoundingBox, voxel_pitch: f64) -> (Vec<bool>, [usize; 3]) {
    let size = bbox.size();
    let nx = ((size.x / voxel_pitch).ceil() as usize).max(1);
    let ny = ((size.y / voxel_pitch).ceil() as usize).max(1);
    let nz = ((size.z / voxel_pitch).ceil() as usize).max(1);
    let mut occ = vec![false; nx * ny * nz];

    let parts = split_mesh_into_granules(mesh);
    let source_parts: Vec<&Mesh> = if parts.is_empty() {
        vec![mesh]
    } else {
        parts.iter().collect()
    };
    let mut part_ranges = Vec::new();
    for p in source_parts {
        let Some(pb) = mesh_bbox(p) else {
            continue;
        };

        let x0 = (((pb.min.x - bbox.min.x) / voxel_pitch).floor() as isize).max(0) as usize;
        let y0 = (((pb.min.y - bbox.min.y) / voxel_pitch).floor() as isize).max(0) as usize;
        let z0 = (((pb.min.z - bbox.min.z) / voxel_pitch).floor() as isize).max(0) as usize;

        let x1 = (((pb.max.x - bbox.min.x) / voxel_pitch).ceil() as isize).clamp(0, nx as isize) as usize;
        let y1 = (((pb.max.y - bbox.min.y) / voxel_pitch).ceil() as isize).clamp(0, ny as isize) as usize;
        let z1 = (((pb.max.z - bbox.min.z) / voxel_pitch).ceil() as isize).clamp(0, nz as isize) as usize;

        if x0 < x1 && y0 < y1 && z0 < z1 {
            part_ranges.push((PreparedMeshQuery::new(p), x0, x1, y0, y1, z0, z1));
        }
    }

    occ.par_chunks_mut(ny * nz)
        .enumerate()
        .for_each_init(MeshQueryScratch::default, |scratch, (x, slab)| {
            let cx = bbox.min.x + (x as f64 + 0.5) * voxel_pitch;

            for (p, x0, x1, y0, y1, z0, z1) in &part_ranges {
                if x < *x0 || x >= *x1 {
                    continue;
                }

                for y in *y0..*y1 {
                    let cy = bbox.min.y + (y as f64 + 0.5) * voxel_pitch;
                    for z in *z0..*z1 {
                        let idx = y * nz + z;
                        if slab[idx] {
                            continue;
                        }

                        let center = Vec3::new(
                            cx,
                            cy,
                            bbox.min.z + (z as f64 + 0.5) * voxel_pitch,
                        );
                        if p.contains_point(center, scratch) {
                            slab[idx] = true;
                        }
                    }
                }
            }
        });

    (occ, [nx, ny, nz])
}

// AI-FUNC-SUMMARY:
// Purpose: Enumerate integer voxel offsets near a spherical shell distance.
// Inputs: shell center distance and half-width in voxel units.
// Returns: Vec of [dx, dy, dz] offset vectors within the shell annulus.
// Side effects: None.
// Notes: Returns [[0,0,0]] for near-zero distance. Used by both exact and MC S2 methods.
pub fn shell_offsets_for_distance(distance_vox: f64, half_width_vox: f64) -> Vec<[isize; 3]> {
    if distance_vox <= 1e-12 {
        return vec![[0, 0, 0]];
    }

    let low2 = (distance_vox - half_width_vox).max(0.0).powi(2);
    let high2 = (distance_vox + half_width_vox).powi(2);
    let lim = (distance_vox + half_width_vox + 1.0).ceil() as isize;
    let mut offsets = Vec::new();

    for dx in -lim..=lim {
        for dy in -lim..=lim {
            for dz in -lim..=lim {
                if dx == 0 && dy == 0 && dz == 0 {
                    continue;
                }
                let d2 = (dx * dx + dy * dy + dz * dz) as f64;
                if d2 >= low2 && d2 < high2 {
                    offsets.push([dx, dy, dz]);
                }
            }
        }
    }

    offsets
}

// AI-FUNC-SUMMARY: Lazily enumerate a shell in the same x/y/z order and half-open bounds as the Vec API, retaining only range cursors; large norms use overflow-safe u128 arithmetic.
#[cfg(any(feature = "gpu", test))]
fn shell_offset_iter(distance_vox: f64, half_width_vox: f64) -> impl Iterator<Item = [isize; 3]> {
    let origin_only = distance_vox <= 1e-12;
    let low2 = (distance_vox - half_width_vox).max(0.0).powi(2);
    let high2 = (distance_vox + half_width_vox).powi(2);
    let lim = if origin_only { 0 } else { (distance_vox + half_width_vox + 1.0).ceil() as isize };
    let start = lim.checked_neg().unwrap_or(isize::MAX);
    let safe_integer = lim >= 0 && (lim as u128) * (lim as u128) <= isize::MAX as u128 / 3;
    std::iter::once([0,0,0]).filter(move |_| origin_only).chain(
        (start..=lim).flat_map(move |dx| {
            (start..=lim).flat_map(move |dy| {
                (start..=lim).filter_map(move |dz| {
                    if dx == 0 && dy == 0 && dz == 0 { return None; }
                    let d2 = if safe_integer { (dx*dx + dy*dy + dz*dz) as f64 } else {
                        let [x,y,z] = [dx,dy,dz].map(|v| v.unsigned_abs() as u128);
                        (x*x + y*y + z*z) as f64
                    };
                    (d2 >= low2 && d2 < high2).then_some([dx,dy,dz])
                })
            })
        })
    )
}

// AI-FUNC-SUMMARY:
// Purpose: Fill unsupported S2 radii using smooth interpolation (linear for 2 points, cubic spline for 3+).
// Inputs: mutable S2 values array, support flags per radius, and S2(0)=VF.
// Returns: None (modifies values in place).
// Side effects: Mutates the values slice.
// Notes: Clamps output to [0,1]. Always sets values[0] = vf.
fn fill_missing_s2_with_smooth_interpolation(values: &mut [f64], has_support: &[bool], vf: f64) {
    if values.is_empty() || values.len() != has_support.len() {
        return;
    }

    values[0] = vf;

    let mut x_known: Vec<usize> = Vec::new();
    let mut y_known: Vec<f64> = Vec::new();
    for i in 0..values.len() {
        if i == 0 || has_support[i] {
            x_known.push(i);
            y_known.push(values[i]);
        }
    }

    if x_known.len() < 2 {
        return;
    }

    if x_known.len() == 2 {
        let xl = x_known[0];
        let xr = x_known[1];
        if xr <= xl + 1 {
            return;
        }
        let yl = y_known[0];
        let yr = y_known[1];
        let width = (xr - xl) as f64;
        for i in (xl + 1)..xr {
            if has_support[i] {
                continue;
            }
            let t = (i - xl) as f64 / width;
            let s = t * t * (3.0 - 2.0 * t);
            values[i] = (yl + (yr - yl) * s).clamp(0.0, 1.0);
        }
        return;
    }

    let m = x_known.len();
    let xs: Vec<f64> = x_known.iter().map(|&x| x as f64).collect();
    let ys = y_known;

    let mut h = vec![0.0f64; m - 1];
    for i in 0..(m - 1) {
        h[i] = (xs[i + 1] - xs[i]).max(1e-12);
    }

    let mut a = vec![0.0f64; m];
    let mut b = vec![0.0f64; m];
    let mut c = vec![0.0f64; m];
    let mut d = vec![0.0f64; m];

    b[0] = 1.0;
    b[m - 1] = 1.0;
    for i in 1..(m - 1) {
        a[i] = h[i - 1];
        b[i] = 2.0 * (h[i - 1] + h[i]);
        c[i] = h[i];
        d[i] = 6.0 * ((ys[i + 1] - ys[i]) / h[i] - (ys[i] - ys[i - 1]) / h[i - 1]);
    }

    for i in 1..m {
        let denom = if b[i - 1].abs() < 1e-12 { 1e-12 } else { b[i - 1] };
        let w = a[i] / denom;
        b[i] -= w * c[i - 1];
        d[i] -= w * d[i - 1];
    }

    let mut m2 = vec![0.0f64; m];
    let last_denom = if b[m - 1].abs() < 1e-12 { 1e-12 } else { b[m - 1] };
    m2[m - 1] = d[m - 1] / last_denom;
    for i in (0..(m - 1)).rev() {
        let denom = if b[i].abs() < 1e-12 { 1e-12 } else { b[i] };
        m2[i] = (d[i] - c[i] * m2[i + 1]) / denom;
    }

    for seg in 0..(m - 1) {
        let xl = x_known[seg];
        let xr = x_known[seg + 1];
        if xr <= xl + 1 {
            continue;
        }

        let x0 = xs[seg];
        let x1 = xs[seg + 1];
        let y0 = ys[seg];
        let y1 = ys[seg + 1];
        let hseg = (x1 - x0).max(1e-12);
        let m20 = m2[seg];
        let m21 = m2[seg + 1];

        for i in (xl + 1)..xr {
            if has_support[i] {
                continue;
            }
            let x = i as f64;
            let acoef = (x1 - x) / hseg;
            let bcoef = (x - x0) / hseg;
            let y = acoef * y0
                + bcoef * y1
                + ((acoef * acoef * acoef - acoef) * m20
                    + (bcoef * bcoef * bcoef - bcoef) * m21)
                    * (hseg * hseg / 6.0);
            values[i] = y.clamp(0.0, 1.0);
        }
    }
}

// AI-FUNC-SUMMARY: Convert 3D FFT-grid index to flat index using y/z strides; returns usize; side effects: None.
fn fft_index_3d(x: usize, y: usize, z: usize, ny: usize, nz: usize) -> usize {
    x * ny * nz + y * nz + z
}

const FFT_RETAIN_BYTES: usize = 16 * 1024 * 1024;

struct FftWorkspace {
    dims: [usize; 3],
    forward: [Arc<dyn Fft<f64>>; 3],
    inverse: [Arc<dyn Fft<f64>>; 3],
    grid: Vec<Complex<f64>>,
    transpose: Vec<Complex<f64>>,
}

thread_local! {
    static FFT_WORKSPACE: RefCell<Option<FftWorkspace>> = const { RefCell::new(None) };
}

impl FftWorkspace {
    // AI-FUNC-SUMMARY: Allocate a dimension-specific FFT grid/transpose and six shared immutable axis plans; preserves exact 2N-1 padding chosen by the caller.
    fn new(dims: [usize; 3]) -> Self {
        let mut planner = FftPlanner::<f64>::new();
        let forward = dims.map(|n| planner.plan_fft_forward(n));
        let inverse = dims.map(|n| planner.plan_fft_inverse(n));
        let cells = dims.iter().product();
        Self {
            dims,
            forward,
            inverse,
            grid: vec![Complex::default(); cells],
            transpose: Vec::new(),
        }
    }

    // AI-FUNC-SUMMARY: Return retained array bytes for the cache admission limit; FFT plans also retain dimension-dependent implementation data.
    fn array_bytes(&self) -> usize {
        (self.grid.capacity() + self.transpose.capacity())
            .saturating_mul(std::mem::size_of::<Complex<f64>>())
    }

    // AI-FUNC-SUMMARY: Run an in-place separable transform using cached axis plans/transpose and task-local scratch; normalize inverse values in the installed Rayon pool.
    fn transform(&mut self, inverse: bool) {
        let [nx, ny, nz] = self.dims;
        let plans = if inverse {
            &self.inverse
        } else {
            &self.forward
        };
        let [fft_x, fft_y, fft_z] = plans;
        let data = &mut self.grid;
        let buf = &mut self.transpose;
        let tasks = rayon::current_num_threads().min(data.len().div_ceil(65536)).max(1);
        if tasks == 1 {
            let scratch_len = plans
                .iter()
                .map(|p| p.get_inplace_scratch_len())
                .max()
                .unwrap_or(0);
            let mut scratch = vec![Complex::default(); scratch_len];
            for line in data.chunks_mut(nz) {
                fft_z.process_with_scratch(line, &mut scratch);
            }
            let mut line = vec![Complex::default(); ny.max(nx)];
            for x in 0..nx {
                for z in 0..nz {
                    for y in 0..ny {
                        line[y] = data[fft_index_3d(x, y, z, ny, nz)];
                    }
                    fft_y.process_with_scratch(&mut line[..ny], &mut scratch);
                    for y in 0..ny {
                        data[fft_index_3d(x, y, z, ny, nz)] = line[y];
                    }
                }
            }
            for y in 0..ny {
                for z in 0..nz {
                    for x in 0..nx {
                        line[x] = data[fft_index_3d(x, y, z, ny, nz)];
                    }
                    fft_x.process_with_scratch(&mut line[..nx], &mut scratch);
                    for x in 0..nx {
                        data[fft_index_3d(x, y, z, ny, nz)] = line[x];
                    }
                }
            }
            if inverse {
                let norm = data.len() as f64;
                for v in data {
                    *v /= norm;
                }
            }
            return;
        }

        buf.resize(data.len(), Complex::default());
        data.par_chunks_mut(nz).with_min_len((nx * ny / tasks).max(1)).for_each_init(
            || vec![Complex::default(); fft_z.get_inplace_scratch_len()],
            |scratch, line| fft_z.process_with_scratch(line, scratch),
        );

        data.par_chunks_mut(ny * nz).with_min_len((nx / tasks).max(1)).for_each_init(
            || {
                (
                    vec![Complex::default(); ny],
                    vec![Complex::default(); fft_y.get_inplace_scratch_len()],
                )
            },
            |(tmp_y, scratch), x_slab| {
                for z in 0..nz {
                    for y in 0..ny {
                        tmp_y[y] = x_slab[y * nz + z];
                    }
                    fft_y.process_with_scratch(tmp_y, scratch);
                    for y in 0..ny {
                        x_slab[y * nz + z] = tmp_y[y];
                    }
                }
            },
        );

        {
            let data_ref: &[Complex<f64>] = data;
            buf.par_chunks_mut(nx * nz)
                .with_min_len((ny / tasks).max(1))
                .enumerate()
                .for_each(|(y, y_slab)| {
                    for z in 0..nz {
                        for x in 0..nx {
                            y_slab[z * nx + x] = data_ref[fft_index_3d(x, y, z, ny, nz)];
                        }
                    }
                });
        }
        buf.par_chunks_mut(nx).with_min_len((ny * nz / tasks).max(1)).for_each_init(
            || vec![Complex::default(); fft_x.get_inplace_scratch_len()],
            |scratch, line| fft_x.process_with_scratch(line, scratch),
        );
        {
            let buf_ref: &[Complex<f64>] = buf;
            data.par_chunks_mut(ny * nz)
                .with_min_len((nx / tasks).max(1))
                .enumerate()
                .for_each(|(x, x_slab)| {
                    for y in 0..ny {
                        for z in 0..nz {
                            x_slab[y * nz + z] = buf_ref[(y * nz + z) * nx + x];
                        }
                    }
                });
        }

        if inverse {
            let norm = (nx * ny * nz) as f64;
            data.par_iter_mut()
                .with_min_len((nx * ny * nz / tasks).max(1))
                .for_each(|v| *v /= norm);
        }
    }
}

// AI-FUNC-SUMMARY: Compute occupancy autocorrelation in an exclusively owned reusable workspace, expose correlation storage to a consumer, then retain at most 16 MiB of arrays per calling thread; nested Rayon calls never hold a TLS borrow.
fn with_fft_correlation<R>(
    occ: &[bool],
    dims: [usize; 3],
    consume: impl FnOnce(&[Complex<f64>], [usize; 3]) -> R,
) -> R {
    let padded = dims.map(|n| {
        n.checked_mul(2)
            .and_then(|n| n.checked_sub(1))
            .expect("FFT dimensions overflow")
    });
    let cached = FFT_WORKSPACE.with(|slot| slot.borrow_mut().take());
    let mut workspace = cached
        .filter(|workspace| workspace.dims == padded)
        .unwrap_or_else(|| FftWorkspace::new(padded));
    let [nx, ny, nz] = dims;
    let [fx, fy, fz] = padded;
    let cells = workspace.grid.len();
    let tasks = rayon::current_num_threads().min(cells.div_ceil(65536)).max(1);
    let fill = |(x, slab): (usize, &mut [Complex<f64>])| {
        slab.fill(Complex::default());
        if x < nx {
            for y in 0..ny {
                for z in 0..nz {
                    slab[y * fz + z].re = if occ[index_3d_to_flat(x,y,z,ny,nz)] { 1.0 } else { 0.0 };
                }
            }
        }
    };
    if tasks == 1 {
        workspace.grid.chunks_mut(fy * fz).enumerate().for_each(fill);
    } else {
        workspace.grid.par_chunks_mut(fy * fz).with_min_len((fx / tasks).max(1)).enumerate().for_each(fill);
    }
    workspace.transform(false);
    if tasks == 1 {
        for v in &mut workspace.grid {
            *v *= v.conj();
        }
    } else {
        workspace
            .grid
            .par_iter_mut()
            .with_min_len((cells / tasks).max(1))
            .for_each(|v| *v *= v.conj());
    }
    workspace.transform(true);
    let result = consume(&workspace.grid, padded);
    if workspace.array_bytes() <= FFT_RETAIN_BYTES && padded.iter().all(|&n| n <= 4096) {
        FFT_WORKSPACE.with(|slot| *slot.borrow_mut() = Some(workspace));
    }
    result
}

// AI-FUNC-SUMMARY:
// Purpose: Compute occupancy autocorrelation counts via FFT convolution (FFT -> power spectrum -> IFFT).
// Inputs: occupancy grid and nx/ny/nz dimensions.
// Returns: Correlation count grid and padded FFT dimensions [fx, fy, fz].
// Side effects: None.
// Notes: Pads to 2N-1 per axis to avoid circular convolution artifacts.
#[cfg(test)]
fn autocorrelation_counts_fft(occ: &[bool], nx: usize, ny: usize, nz: usize) -> (Vec<f64>, [usize; 3]) {
    with_fft_correlation(occ, [nx, ny, nz], |grid, dims| {
        (grid.iter().map(|v| v.re.max(0.0)).collect(), dims)
    })
}

// AI-FUNC-SUMMARY:
// Purpose: Compute exact S2 by direct pair enumeration per shell offset (no FFT).
// Inputs: occupancy grid, dimensions, max radius, voxel pitch, volume fraction at r=0.
// Returns: S2 values for r=0..r_max with smooth interpolation for unsupported radii.
// Side effects: None.
// Notes: Parallelizes over radii via rayon. Used as fallback when FFT grid would be too large.
fn calculate_s2_exact_direct(
    occ: &[bool],
    nx: usize,
    ny: usize,
    nz: usize,
    r_max: usize,
    voxel_pitch: f64,
    vf: f64,
) -> Vec<f64> {
    let results: Vec<(f64, bool)> = (0..=r_max)
        .into_par_iter()
        .map(|r| {
            if r == 0 {
                return (vf, true);
            }

            let r_vox = r as f64 / voxel_pitch;
            let half_width_vox = 0.5 / voxel_pitch;
            let offsets = shell_offsets_for_distance(r_vox, half_width_vox);
            let mut shell_sum = 0.0;
            let mut used_offsets = 0usize;

            for off in &offsets {
                let dx = off[0];
                let dy = off[1];
                let dz = off[2];

                let x_start = if dx < 0 { (-dx) as usize } else { 0 };
                let y_start = if dy < 0 { (-dy) as usize } else { 0 };
                let z_start = if dz < 0 { (-dz) as usize } else { 0 };

                let x_end = if dx > 0 { nx.saturating_sub(dx as usize) } else { nx };
                let y_end = if dy > 0 { ny.saturating_sub(dy as usize) } else { ny };
                let z_end = if dz > 0 { nz.saturating_sub(dz as usize) } else { nz };

                if x_start >= x_end || y_start >= y_end || z_start >= z_end {
                    continue;
                }

                let mut valid_pairs = 0usize;
                let mut hit_pairs = 0usize;

                for x in x_start..x_end {
                    for y in y_start..y_end {
                        for z in z_start..z_end {
                            let x2 = (x as isize + dx) as usize;
                            let y2 = (y as isize + dy) as usize;
                            let z2 = (z as isize + dz) as usize;

                            let i1 = index_3d_to_flat(x, y, z, ny, nz);
                            let i2 = index_3d_to_flat(x2, y2, z2, ny, nz);
                            valid_pairs += 1;
                            if occ[i1] && occ[i2] {
                                hit_pairs += 1;
                            }
                        }
                    }
                }

                if valid_pairs > 0 {
                    shell_sum += hit_pairs as f64 / valid_pairs as f64;
                    used_offsets += 1;
                }
            }

            if used_offsets == 0 {
                (0.0, false)
            } else {
                (shell_sum / used_offsets as f64, true)
            }
        })
        .collect();

    let mut out = vec![0.0; r_max + 1];
    let mut has_support = vec![false; r_max + 1];
    for (r, (value, supported)) in results.into_iter().enumerate() {
        out[r] = value;
        has_support[r] = supported;
    }
    fill_missing_s2_with_smooth_interpolation(&mut out, &has_support, vf);
    out[0] = vf;
    out
}

// AI-FUNC-SUMMARY:
// Purpose: Compute exact S2 using FFT-based autocorrelation.
// Inputs: occupancy grid, dimensions, max radius, voxel pitch, volume fraction at r=0.
// Returns: S2 values for r=0..r_max with smooth interpolation for unsupported radii.
// Side effects: None.
// Notes: Parallelizes over radii. Falls back gracefully when offsets exceed grid dimensions.
fn calculate_s2_exact_fft(
    occ: &[bool],
    nx: usize,
    ny: usize,
    nz: usize,
    r_max: usize,
    voxel_pitch: f64,
    vf: f64,
) -> Vec<f64> {
    with_fft_correlation(occ, [nx, ny, nz], |corr, [fx, fy, fz]| {

    let get_corr = |dx: isize, dy: isize, dz: isize| {
        let ix = if dx >= 0 { dx as usize } else { (fx as isize + dx) as usize };
        let iy = if dy >= 0 { dy as usize } else { (fy as isize + dy) as usize };
        let iz = if dz >= 0 { dz as usize } else { (fz as isize + dz) as usize };
        corr[fft_index_3d(ix, iy, iz, fy, fz)].re.max(0.0)
    };

    let results: Vec<(f64, bool)> = (0..=r_max)
        .into_par_iter()
        .map(|r| {
            if r == 0 {
                return (vf, true);
            }

            let r_vox = r as f64 / voxel_pitch;
            let half_width_vox = 0.5 / voxel_pitch;
            let offsets = shell_offsets_for_distance(r_vox, half_width_vox);
            let mut shell_sum = 0.0;
            let mut used = 0usize;

            for off in &offsets {
                let dx = off[0];
                let dy = off[1];
                let dz = off[2];

                let adx = dx.unsigned_abs();
                let ady = dy.unsigned_abs();
                let adz = dz.unsigned_abs();
                if adx >= nx || ady >= ny || adz >= nz {
                    continue;
                }

                let valid_pairs = (nx - adx) * (ny - ady) * (nz - adz);
                if valid_pairs == 0 {
                    continue;
                }

                let hits = get_corr(dx, dy, dz);
                shell_sum += hits / valid_pairs as f64;
                used += 1;
            }

            if used == 0 {
                (0.0, false)
            } else {
                (shell_sum / used as f64, true)
            }
        })
        .collect();

    let mut out = vec![0.0; r_max + 1];
    let mut has_support = vec![false; r_max + 1];
    for (r, (value, supported)) in results.into_iter().enumerate() {
        out[r] = value;
        has_support[r] = supported;
    }
    fill_missing_s2_with_smooth_interpolation(&mut out, &has_support, vf);
    out[0] = vf;
    out
    })
}

// AI-FUNC-SUMMARY:
// Purpose: Compute S2 via Monte Carlo sampling directly on the mesh (no voxelization).
// Inputs: mesh, bounding box, max radius, sample count.
// Returns: S2 values for r=0..r_max.
// Side effects: None.
// Notes: Draws one seed and uses fixed radius/sample-block streams under the installed pool; retains the continuous point-pair definition and out-of-bounds rejection.
fn calculate_s2_monte_carlo_mesh(mesh: &Mesh, bbox: BoundingBox, r_max: usize, samples: usize) -> Vec<f64> {
    calculate_s2_mesh_mc_seeded(mesh, bbox, r_max, samples, rand::random(), true)
}

// AI-FUNC-SUMMARY: Evaluate continuous mesh MC with recorded seed and fixed radius/sample-block streams; parallel integer counts are deterministic across workers, and the full-scan option is retained for equivalence/performance benchmarks.
pub fn calculate_s2_mesh_mc_seeded(mesh: &Mesh, bbox: BoundingBox, r_max: usize, samples: usize, seed: u64, prepared: bool) -> Vec<f64> {
    calculate_s2_mesh_mc_seeded_with_vf(mesh, bbox, r_max, samples, seed, prepared, volume_fraction_in_bbox(mesh, bbox))
}

// AI-FUNC-SUMMARY: Evaluate the unchanged fixed-seed continuous MC samples using a caller-validated geometric VF; used by island-local contribution caching.
pub(crate) fn calculate_s2_mesh_mc_seeded_with_vf(mesh: &Mesh, bbox: BoundingBox, r_max: usize, samples: usize, seed: u64, prepared: bool, vf: f64) -> Vec<f64> {
    use rand::SeedableRng;
    const BLOCK: usize = 2048;
    if r_max == 0 { return vec![vf]; }
    let mc_samples = samples.max(200);
    let blocks = mc_samples.div_ceil(BLOCK);
    let query = prepared.then(|| PreparedMeshQuery::new(mesh));
    let min = bbox.min;
    let max = bbox.max;
    let counts: Vec<(u64,u64)> = (0..r_max*blocks).into_par_iter()
        .map_init(MeshQueryScratch::default, |scratch, task| {
            let r = task/blocks+1;
            let block = task%blocks;
            let mut rng = rand_chacha::ChaCha12Rng::seed_from_u64(seed);
            rng.set_stream((r as u64).wrapping_mul(0x9e3779b97f4a7c15) ^ block as u64);
            let (mut hits,mut valid) = (0u64,0u64);
            for _ in block*BLOCK..mc_samples.min((block*BLOCK).saturating_add(BLOCK)) {
                let p = Vec3::new(rng.gen_range(min.x..max.x),rng.gen_range(min.y..max.y),rng.gen_range(min.z..max.z));
                let dir = loop {
                    let x = rng.gen_range(-1.0f64..1.0f64);
                    let y = rng.gen_range(-1.0f64..1.0f64);
                    let z = rng.gen_range(-1.0f64..1.0f64);
                    let n2 = x*x+y*y+z*z;
                    if n2 > 1e-12 && n2 <= 1.0 { let inv=1.0/n2.sqrt(); break Vec3::new(x*inv,y*inv,z*inv); }
                };
                let q = p.add(dir.scale(r as f64));
                if q.x<min.x || q.x>max.x || q.y<min.y || q.y>max.y || q.z<min.z || q.z>max.z { continue; }
                valid += 1;
                let contains = |point, scratch: &mut MeshQueryScratch| {
                    if let Some(query) = &query { query.contains_point(point,scratch) } else { point_inside_mesh(mesh,point) }
                };
                if contains(p,scratch) && contains(q,scratch) { hits += 1; }
            }
            (hits,valid)
        }).collect();
    let mut out = vec![vf];
    for radius in counts.chunks(blocks) {
        let (hits,valid) = radius.iter().fold((0u64,0u64),|(h,v),&(a,b)|(h+a,v+b));
        out.push(if valid==0 {0.0} else {hits as f64/valid as f64});
    }
    out
}

// AI-FUNC-SUMMARY:
// Purpose: Compute the S2 two-point correlation function by the configured method.
// Inputs: mesh, bounding box, r_max, voxel_pitch, method name ("exact" or "monte_carlo"), sample count for MC.
// Returns: Vec<f64> of S2 values indexed by radius (r=0..r_max).
// Side effects: Prints warning when exact requested with invalid voxel_pitch.
// Notes: For "exact" method, chooses FFT or direct enumeration based on grid size threshold. For other methods, uses voxelized MC sampling.
pub fn calculate_s2(
    mesh: &Mesh,
    bbox: BoundingBox,
    r_max: usize,
    voxel_pitch: f64,
    method: &str,
    samples: usize,
) -> Vec<f64> {
    if method != "exact" && voxel_pitch <= 0.0 {
        return calculate_s2_monte_carlo_mesh(mesh, bbox, r_max, samples);
    }

    let effective_pitch = if voxel_pitch <= 0.0 {
        println!(
            "[Warning] exact S2 requested with voxel_pitch <= 0; falling back to voxel_pitch=1.0"
        );
        1.0
    } else {
        voxel_pitch
    };

    VoxelS2::new(mesh, bbox, effective_pitch).calculate(r_max, method, samples)
}

/// Immutable voxelization shared by exact and voxel Monte Carlo evaluations.
pub struct VoxelS2 {
    occupancy: Vec<bool>,
    dims: [usize; 3],
    pitch: f64,
}

impl VoxelS2 {
    // AI-FUNC-SUMMARY: Prepare one occupancy grid for a fixed mesh, domain and positive pitch; owns grid data and performs parallel voxelization.
    pub fn new(mesh: &Mesh, bbox: BoundingBox, pitch: f64) -> Self {
        let pitch = pitch.max(1e-9);
        let (occupancy, dims) = build_bbox_occupancy(mesh, bbox, pitch);
        Self { occupancy, dims, pitch }
    }

    // AI-FUNC-SUMMARY: Evaluate exact or voxel MC S2 on an immutable prepared grid without repeating mesh splitting or containment queries; returns the occupancy VF at radius zero.
    pub fn calculate(&self, r_max: usize, method: &str, samples: usize) -> Vec<f64> {
    let [nx, ny, nz] = self.dims;
    let occ = &self.occupancy;
    let effective_pitch = self.pitch;
    let total_vox = (nx * ny * nz).max(1);
    let occupied_count = occ.iter().filter(|&&v| v).count();
    let vf = occupied_count as f64 / total_vox as f64;

    if occupied_count == 0 {
        return vec![0.0; r_max + 1];
    }

    match method {
        "exact" => {
            let fx = (2 * nx).saturating_sub(1).max(1);
            let fy = (2 * ny).saturating_sub(1).max(1);
            let fz = (2 * nz).saturating_sub(1).max(1);
            let fft_cells = fx.saturating_mul(fy).saturating_mul(fz);
            let max_fft_cells = 24_000_000usize;

            if fft_cells > max_fft_cells {
                calculate_s2_exact_direct(occ, nx, ny, nz, r_max, effective_pitch.max(1e-9), vf)
            } else {
                calculate_s2_exact_fft(occ, nx, ny, nz, r_max, effective_pitch.max(1e-9), vf)
            }
        }
        _ => {
            let mc_samples = samples.max(200);
            let pitch = effective_pitch.max(1e-9);
            let half_width_vox = 0.5 / pitch;
            let shells: Vec<Vec<[isize; 3]>> = (1..=r_max)
                .map(|r| shell_offsets_for_distance(r as f64 / pitch, half_width_vox))
                .collect();
            let results: Vec<(f64, bool)> = (0..=r_max)
                .into_par_iter()
                .map(|r| {
                    if r == 0 {
                        return (vf, true);
                    }

                    let offsets = &shells[r - 1];
                    if offsets.is_empty() {
                        return (0.0, false);
                    }
                    let mut valid = 0usize;
                    let mut hits = 0usize;
                    let mut rng = rand::thread_rng();

                    for _ in 0..mc_samples {
                        let x = rng.gen_range(0..nx);
                        let y = rng.gen_range(0..ny);
                        let z = rng.gen_range(0..nz);
                        let off = &offsets[rng.gen_range(0..offsets.len())];

                        let x2 = x as isize + off[0];
                        let y2 = y as isize + off[1];
                        let z2 = z as isize + off[2];
                        if x2 < 0 || y2 < 0 || z2 < 0 {
                            continue;
                        }
                        let x2u = x2 as usize;
                        let y2u = y2 as usize;
                        let z2u = z2 as usize;
                        if x2u >= nx || y2u >= ny || z2u >= nz {
                            continue;
                        }

                        valid += 1;
                        let i1 = index_3d_to_flat(x, y, z, ny, nz);
                        let i2 = index_3d_to_flat(x2u, y2u, z2u, ny, nz);
                        if occ[i1] && occ[i2] {
                            hits += 1;
                        }
                    }

                    if valid == 0 {
                        (0.0, false)
                    } else {
                        (hits as f64 / valid as f64, true)
                    }
                })
                .collect();

            let mut out = vec![0.0; r_max + 1];
            let mut has_support = vec![false; r_max + 1];
            for (r, (value, supported)) in results.into_iter().enumerate() {
                out[r] = value;
                has_support[r] = supported;
            }
            fill_missing_s2_with_smooth_interpolation(&mut out, &has_support, vf);
            out[0] = vf;
            out
        }
    }
}

}

// AI-FUNC-SUMMARY: Convenience wrapper for Monte Carlo S2 estimation with default pitch; returns S2 values; side effects: None.
pub fn approximate_s2(mesh: &Mesh, bbox: BoundingBox, r_max: usize, samples: usize) -> Vec<f64> {
    calculate_s2(mesh, bbox, r_max, 1.0, "monte_carlo", samples)
}

// AI-FUNC-SUMMARY: Compute L2 norm (Euclidean distance) between two vectors over their common length prefix; returns f64 (0.0 for empty); side effects: None.
pub fn l2_norm(a: &[f64], b: &[f64]) -> f64 {
    let n = a.len().min(b.len());
    if n == 0 {
        return 0.0;
    }
    let sum = (0..n).map(|i| {
        let d = a[i] - b[i];
        d * d
    });
    sum.sum::<f64>().sqrt()
}

// AI-FUNC-SUMMARY:
// Purpose: Compute S2 with optional GPU acceleration for Monte Carlo method.
// Inputs: mesh, bbox, r_max, voxel_pitch, method, samples, optional mutable GPU pipeline.
// Returns: Vec<f64> of S2 values indexed by radius (r=0..r_max).
// Side effects: Dispatches GPU compute if gpu_pipeline is Some and method is monte_carlo/both.
// Notes: GPU execution errors fall back to continuous CPU mesh MC; absent GPU or exact method uses the requested CPU method. Sets r=0 to volume fraction. Only available with feature "gpu".
#[cfg(feature = "gpu")]
pub fn calculate_s2_with_gpu(
    mesh: &Mesh,
    bbox: BoundingBox,
    r_max: usize,
    voxel_pitch: f64,
    method: &str,
    samples: usize,
    gpu_pipeline: Option<&mut crate::gpu::s2::GpuS2Pipeline>,
) -> Vec<f64> {
    if let Some(gpu) = gpu_pipeline {
        if method == "monte_carlo" || method == "both" {
            let t0 = std::time::Instant::now();
            let mut result = match gpu.calculate_s2_gpu(bbox, r_max, samples) {
                Ok(result) => result,
                Err(error) => {
                    eprintln!("[Warning] GPU S2 MC failed: {error}; falling back to CPU mesh MC");
                    return calculate_s2(mesh, bbox, r_max, 0.0, "monte_carlo", samples);
                }
            };
            let elapsed = t0.elapsed().as_secs_f64();
            let vf = volume_fraction_in_bbox(mesh, bbox);
            result[0] = vf;
            println!("[Info] GPU S2 Monte Carlo: {:.3}s, {} radii, {} samples/radius", elapsed, r_max + 1, samples);
            return result;
        }
    }
    calculate_s2(mesh, bbox, r_max, voxel_pitch, method, samples)
}

// AI-FUNC-SUMMARY:
// Purpose: Compute exact S2 using GPU voxelization + GPU shell pair counting.
// Inputs: mesh, bbox, r_max, voxel_pitch, volume fraction.
// Returns: Vec<f64> of S2 values for r=0..r_max.
// Side effects: Initializes two GPU pipelines (voxel + shell), dispatches compute.
// Notes: Uses f32 ray-casting for voxelization, then u32 shell pair counting. Only available with feature "gpu".
#[cfg(feature = "gpu")]
pub fn calculate_s2_gpu_exact(mesh: &Mesh, bbox: BoundingBox, r_max: usize, voxel_pitch: f64) -> Vec<f64> {
    match try_calculate_s2_gpu_exact(mesh, bbox, r_max, voxel_pitch) {
        Ok(result) => result,
        Err(error) => {
            eprintln!("[Warning] GPU exact failed: {error}; falling back to CPU exact");
            calculate_s2(mesh, bbox, r_max, voxel_pitch, "exact", 10000)
        }
    }
}

// AI-FUNC-SUMMARY: Compute GPU voxel-exact S2 with checked normalized grid dimensions and explicit errors; leaves fallback policy to the caller.
#[cfg(feature = "gpu")]
pub fn try_calculate_s2_gpu_exact(
    mesh: &Mesh,
    bbox: BoundingBox,
    r_max: usize,
    voxel_pitch: f64,
) -> std::result::Result<Vec<f64>, String> {
    try_calculate_s2_gpu_exact_limited(mesh, bbox, r_max, voxel_pitch, None)
}

// AI-FUNC-SUMMARY: Plan the resident exact working set and budget-dependent shell batch before GPU initialization; preserve exact semantics and explicit errors.
#[cfg(feature = "gpu")]
pub(crate) fn try_calculate_s2_gpu_exact_limited(mesh: &Mesh, bbox: BoundingBox, r_max: usize, voxel_pitch: f64, limit_mb: Option<u64>) -> std::result::Result<Vec<f64>, String> {
    if r_max.checked_add(1).is_none() || r_max > u32::MAX as usize { return Err("GPU exact radius count exceeds supported range".into()); }
    if !voxel_pitch.is_finite() { return Err("S2 voxel pitch must be finite".into()); }
    let voxel_pitch = if voxel_pitch <= 0.0 { 1.0 } else { voxel_pitch };
    let size = bbox.size();
    let mut dims = [0u32;3];
    for (i,size) in [size.x,size.y,size.z].into_iter().enumerate() {
        let n = (size / voxel_pitch).ceil().max(1.0);
        if !size.is_finite() || size <= 0.0 || !n.is_finite() || n > u32::MAX as f64 { return Err("GPU exact grid dimensions exceed supported range".into()); }
        dims[i] = n as u32;
    }
    let [nx,ny,nz] = dims;
    let total_vox = nx.checked_mul(ny).and_then(|n| n.checked_mul(nz)).ok_or("GPU exact grid cell count overflow")? as usize;
    let memory = crate::compute::exact_memory::ExactMemoryPlan::new(mesh.faces.len(), total_vox, limit_mb)?;
    memory.check_budget(limit_mb)?;
    let pitch = voxel_pitch as f32;

    let t0 = std::time::Instant::now();

    let mut vox_pipeline = crate::gpu::voxel::GpuVoxelPipeline::new(mesh, bbox)?;
    let occupied = vox_pipeline.voxelize_count(nx, ny, nz, pitch)?;
    let vf = occupied as f64 / total_vox as f64;

    if occupied == 0 {
        return Ok(vec![0.0; r_max + 1]);
    }

    let pitch_f64 = voxel_pitch.max(1e-9);
    let half_width = 0.5 / pitch_f64;
    let mut has_support = vec![false; r_max + 1];
    has_support[0] = true;
    let mut offset_count = 0u128;
    let offsets = (1..=r_max).flat_map(|r| {
        let mut shell = shell_offset_iter(r as f64 / pitch_f64, half_width).peekable();
        has_support[r] = shell.peek().is_some();
        shell.map(move |off| (r as u32, off))
    }).inspect(|_| offset_count += 1);
    let (device, queue) = vox_pipeline.device_queue();
    let mut shell_pipeline = crate::gpu::s2_shell::GpuShellS2Pipeline::with_device(device, queue)?;
    shell_pipeline.set_batch_partial_limit(memory.batch_partials)?;
    let mut result = shell_pipeline.compute_s2_shell_resident_stream(
        &vox_pipeline.occupancy_buffer(), dims, offsets, r_max, pitch_f64, vf,
    )?;

    fill_missing_s2_with_smooth_interpolation(&mut result, &has_support, vf);
    result[0] = vf;

    let elapsed = t0.elapsed().as_secs_f64();
    println!("[Info] GPU exact S2: {:.3}s, {}x{}x{} grid, {} offsets, {} radii, batch_partials={}, estimated_peak_bytes={}",
        elapsed, nx, ny, nz, offset_count, r_max + 1, memory.batch_partials, memory.peak_bytes);
    Ok(result)
}

#[cfg(test)]
mod fft_workspace_tests {
    use super::*;

    // AI-FUNC-SUMMARY: Verify cached storage reuse never retains stale occupancy, supports nested consumers and drops array worksets above its retention cap.
    #[test]
    fn cache_reuse_reset_nested_and_release() {
        FFT_WORKSPACE.with(|slot| *slot.borrow_mut() = None);
        let dims = [4, 3, 2];
        let solid = vec![true; 24];
        let empty = vec![false; 24];
        let first = with_fft_correlation(&solid, dims, |grid, _| {
            assert!((grid[0].re - 24.0).abs() < 1e-9);
            grid.as_ptr() as usize
        });
        let second = with_fft_correlation(&empty, dims, |grid, _| {
            assert!(grid.iter().all(|v| v.norm() < 1e-9));
            with_fft_correlation(&solid, dims, |nested, _| assert!((nested[0].re - 24.0).abs() < 1e-9));
            assert!(grid.iter().all(|v| v.norm() < 1e-9));
            grid.as_ptr() as usize
        });
        assert_eq!(first, second);
        with_fft_correlation(&vec![true; 129 * 129 * 9], [129, 129, 9], |grid, _| assert!((grid[0].re - (129 * 129 * 9) as f64).abs() < 1e-7));
        FFT_WORKSPACE.with(|slot| assert!(slot.borrow().is_none()));
    }

    // AI-FUNC-SUMMARY: Compare cached exact FFT shells with exhaustive direct pair enumeration for repeated changing occupancy under several worker budgets.
    #[test]
    fn cached_fft_matches_direct_shells_across_workers() {
        for workers in [1, 2, 8] {
            rayon::ThreadPoolBuilder::new().num_threads(workers).build().unwrap().install(|| {
                for dims in [[1, 2, 9], [6, 4, 3], [8, 2, 5]] {
                    let [nx, ny, nz] = dims;
                    for shift in 0..3 {
                        let occ: Vec<_> = (0..nx*ny*nz).map(|i| (i + shift) % 7 < 3).collect();
                        let vf = occ.iter().filter(|&&v| v).count() as f64 / occ.len() as f64;
                        let direct = calculate_s2_exact_direct(&occ,nx,ny,nz,6,1.0,vf);
                        let fft = calculate_s2_exact_fft(&occ,nx,ny,nz,6,1.0,vf);
                        assert!(direct.iter().zip(fft).all(|(a,b)| (a-b).abs() < 1e-10));
                    }
                }
            });
        }
    }

    // AI-FUNC-SUMMARY: Exercise the parallel FFT branch on nonuniform occupancy and compare supported/unsupported shell values with integer direct enumeration.
    #[test]
    fn parallel_fft_grain_matches_direct() {
        rayon::ThreadPoolBuilder::new().num_threads(8).build().unwrap().install(|| {
            let n = 24;
            let occ: Vec<_> = (0..n*n*n).map(|i| (i * 37 + i / n) % 11 < 4).collect();
            let vf = occ.iter().filter(|&&v| v).count() as f64 / occ.len() as f64;
            let fft = calculate_s2_exact_fft(&occ,n,n,n,2,1.0,vf);
            let direct = calculate_s2_exact_direct(&occ,n,n,n,2,1.0,vf);
            assert!(fft.iter().zip(direct).all(|(a,b)| (a-b).abs() < 1e-10));
        });
    }

    // AI-FUNC-SUMMARY: Benchmark fresh versus retained FFT plans/grid/transpose on identical occupancy, alternating mode order and printing raw release timings.
    #[test]
    #[ignore = "release performance measurement"]
    fn fft_cache_benchmark() {
        for dims in [[8,8,8], [32,24,16], [64,16,16]] {
            let occ: Vec<_> = (0..dims.iter().product()).map(|i| i % 7 < 3).collect();
            for workers in [1,2,8] {
                let pool = rayon::ThreadPoolBuilder::new().num_threads(workers).build().unwrap();
                pool.install(|| {
                    for trial in 0..6 {
                        let mut times = [0.0;2];
                        for mode in if trial % 2 == 0 { [0,1] } else { [1,0] } {
                            with_fft_correlation(&occ, dims, |_,_| ());
                            let start = std::time::Instant::now();
                            for _ in 0..10 {
                                if mode == 0 { FFT_WORKSPACE.with(|s| *s.borrow_mut() = None); }
                                std::hint::black_box(with_fft_correlation(&occ,dims,|v,_| v[0].re));
                            }
                            times[mode] = start.elapsed().as_secs_f64();
                        }
                        println!("fft_cache dims={dims:?} workers={workers} trial={trial} repeats=10 fresh_s={} cached_s={}",times[0],times[1]);
                    }
                });
            }
        }
    }

    // AI-FUNC-SUMMARY: Compare scratch-backed FFT correlations with integer pair counts on thin and unfriendly padded dimensions, including negative offsets.
    #[test]
    fn fft_scratch_matches_integer_pairs() {
        for [nx, ny, nz] in [[1, 2, 9], [6, 4, 3], [8, 2, 5]] {
            let occ: Vec<bool> = (0..nx * ny * nz).map(|i| i % 7 < 3).collect();
            let (corr, [fx, fy, fz]) = autocorrelation_counts_fft(&occ, nx, ny, nz);
            for dx in -(nx as isize - 1)..nx as isize {
                for dy in -(ny as isize - 1)..ny as isize {
                    for dz in -(nz as isize - 1)..nz as isize {
                        let mut count = 0;
                        for x in 0..nx {
                            for y in 0..ny {
                                for z in 0..nz {
                                    let (a, b, c) = (x as isize + dx, y as isize + dy, z as isize + dz);
                                    if a >= 0 && b >= 0 && c >= 0 && a < nx as isize && b < ny as isize && c < nz as isize
                                        && occ[index_3d_to_flat(x, y, z, ny, nz)]
                                        && occ[index_3d_to_flat(a as usize, b as usize, c as usize, ny, nz)] { count += 1; }
                                }
                            }
                        }
                        let i = fft_index_3d(dx.rem_euclid(fx as isize) as usize, dy.rem_euclid(fy as isize) as usize, dz.rem_euclid(fz as isize) as usize, fy, fz);
                        assert!((corr[i] - count as f64).abs() < 1e-9, "{nx},{ny},{nz}: {dx},{dy},{dz}");
                    }
                }
            }
        }
    }
}


#[cfg(test)]
mod shell_iterator_tests {
    use super::*;
    // AI-FUNC-SUMMARY: Compare exact ordered lazy output with the independent original cube collector across shell boundaries, zero radii and empty shells.
    #[test]
    fn lazy_shell_matches_original_order_and_bounds() {
        for radius in [-1.0,0.0,1e-12,1e-11,0.1,0.5,1.0,2.0,3.5,8.0,17.0] {
            for width in [0.0,0.01,0.5,1.0,2.5] {
                assert_eq!(shell_offset_iter(radius,width).collect::<Vec<_>>(),shell_offsets_for_distance(radius,width), "r={radius} width={width}");
            }
        }
        for edge in [1.0f64,2.0,3.0,5.0,9.0] {
            for radius in [f64::from_bits(edge.to_bits()-1),edge,f64::from_bits(edge.to_bits()+1)] {
                assert_eq!(shell_offset_iter(radius,0.5).collect::<Vec<_>>(),shell_offsets_for_distance(radius,0.5));
            }
        }
        let mut partial = shell_offset_iter(17.0,0.5);
        let reference = shell_offsets_for_distance(17.0,0.5);
        assert_eq!(partial.by_ref().take(7).collect::<Vec<_>>(),reference[..7]);
        assert_eq!(partial.collect::<Vec<_>>(),reference[7..]);
    }
    // AI-FUNC-SUMMARY: Measure original shell materialization versus lazy enumeration with identical order-sensitive checksums and report Vec payload capacity separately from timing.
    #[test]
    #[ignore = "release lazy shell enumeration benchmark"]
    fn lazy_shell_benchmark() {
        use std::time::Instant;
        let digest = |iter: &mut dyn Iterator<Item=[isize;3]>| {
            iter.fold((0usize,0u64), |(count,hash),[x,y,z]| (count+1, hash.rotate_left(7) ^ (x as u64).wrapping_mul(31) ^ (y as u64).wrapping_mul(17) ^ z as u64))
        };
        for radius in [16.0,64.0,128.0] {
            let warm = shell_offsets_for_distance(radius,0.5);
            let expected=digest(&mut warm.iter().copied());
            assert_eq!(digest(&mut shell_offset_iter(radius,0.5)),expected);
            for sample in 0..5 {
                let original=|| {
                    let start=Instant::now();
                    let vec=shell_offsets_for_distance(radius,0.5);
                    let bytes=vec.capacity()*std::mem::size_of::<[isize;3]>();
                    let actual=digest(&mut vec.iter().copied());
                    let elapsed=start.elapsed().as_secs_f64();
                    assert_eq!(actual,expected);
                    (elapsed,bytes)
                };
                let lazy=|| {
                    let start=Instant::now();
                    let actual=digest(&mut shell_offset_iter(radius,0.5));
                    let elapsed=start.elapsed().as_secs_f64();
                    assert_eq!(actual,expected);
                    elapsed
                };
                let (old,new)=if sample%2==0 {(original(),lazy())} else {let new=lazy();(original(),new)};
                eprintln!("SHELL_LAZY_BENCH radius={radius} sample={sample} offsets={} old_seconds={:.9} new_seconds={new:.9} old_capacity_bytes={}",expected.0,old.0,old.1);
            }
        }
    }

}
