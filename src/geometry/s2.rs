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

pub(crate) const DEFAULT_CPU_EXACT_BUDGET_BYTES: u64 = 768 * 1024 * 1024;
pub(crate) const NS_PER_FFT_UNIT: f64 = 2.0;
pub(crate) const NS_PER_DIRECT_PAIR: f64 = 0.36;
pub(crate) const DIRECT_PARALLEL_EFFICIENCY: f64 = 1.0 / 3.0;

// AI-FUNC-SUMMARY: Return the smallest 2,3,5-smooth length >= min_len (1 for 0/1) by enumerating 2^a*3^b*5^c with checked arithmetic; None on overflow.
fn smooth_fft_length(min_len: usize) -> Option<usize> {
    let target = min_len.max(1);
    let mut best: Option<usize> = None;
    let mut p5 = 1usize;
    loop {
        let mut p35 = p5;
        loop {
            let mut v = p35;
            while v < target {
                match v.checked_mul(2) {
                    Some(next) => v = next,
                    None => break,
                }
            }
            if v >= target && best.is_none_or(|b| v < b) {
                best = Some(v);
            }
            if p35 >= target {
                break;
            }
            match p35.checked_mul(3) {
                Some(next) => p35 = next,
                None => break,
            }
        }
        if p5 >= target {
            break;
        }
        match p5.checked_mul(5) {
            Some(next) => p5 = next,
            None => break,
        }
    }
    best
}

// AI-FUNC-SUMMARY: Pad each axis to the smallest 2,3,5-smooth length >= 2N-1, which keeps linear autocorrelation free of circular wrap for every shift |d| <= N-1; None on overflow.
fn padded_fft_dims(dims: [usize; 3]) -> Option<[usize; 3]> {
    let mut out = [0usize; 3];
    for (slot, n) in out.iter_mut().zip(dims) {
        *slot = smooth_fft_length(n.checked_mul(2)?.saturating_sub(1).max(1))?;
    }
    Some(out)
}

/// CPU exact S2 kernel chosen by `plan_exact_cpu`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ExactCpuMethod {
    Fft,
    Direct,
}

/// Modeled cost and working set of both CPU exact kernels for one grid.
#[derive(Clone, Debug)]
pub(crate) struct ExactCpuPlan {
    pub method: ExactCpuMethod,
    pub padded: [usize; 3],
    pub offsets: u64,
    pub pair_work: u128,
    pub fft_bytes: Option<u64>,
    pub direct_bytes: Option<u64>,
    pub fft_seconds: f64,
    pub direct_seconds: f64,
    pub budget_bytes: u64,
    pub reason: String,
}

impl ExactCpuPlan {
    // AI-FUNC-SUMMARY: Return the peak working-set estimate of the selected kernel; None when it overflowed u64.
    pub fn selected_bytes(&self) -> Option<u64> {
        match self.method {
            ExactCpuMethod::Fft => self.fft_bytes,
            ExactCpuMethod::Direct => self.direct_bytes,
        }
    }

    // AI-FUNC-SUMMARY: Return true when the selected kernel's working set is known and fits the plan budget.
    pub fn fits_budget(&self) -> bool {
        self.selected_bytes().is_some_and(|b| b <= self.budget_bytes)
    }

    // AI-FUNC-SUMMARY: Format the selection, both modeled wall times, both working sets and the reason as one observable log line.
    pub fn describe(&self) -> String {
        let bytes = |b: Option<u64>| b.map_or("overflow".to_string(), |b| b.to_string());
        format!(
            "method={} padded={:?} offsets={} pair_work={} fft_est_s={:.4} direct_est_s={:.4} fft_bytes={} direct_bytes={} budget_bytes={} reason={}",
            match self.method {
                ExactCpuMethod::Fft => "fft",
                ExactCpuMethod::Direct => "direct",
            },
            self.padded,
            self.offsets,
            self.pair_work,
            self.fft_seconds,
            self.direct_seconds,
            bytes(self.fft_bytes),
            bytes(self.direct_bytes),
            self.budget_bytes,
            self.reason
        )
    }
}

// AI-FUNC-SUMMARY: Count in-domain shell offsets K, exact direct pair work W = sum of (nx-|dx|)(ny-|dy|)(nz-|dz|) and the largest per-radius offset count for radii 1..=r_max; enumerates one octant of the clamped offset ball once (sign copies weighted by multiplicity, loops cut at r_max) with the shell iterator's half-open float bounds; integer sums make the parallel merge order-free.
fn exact_shell_work(dims: [usize; 3], r_max: usize, pitch: f64) -> (u64, u128, u64) {
    if r_max == 0 {
        return (0, 0, 0);
    }
    let [nx, ny, nz] = dims;
    let cells = (nx as u128) * (ny as u128) * (nz as u128);
    let half_width = 0.5 / pitch;
    let bounds = |r: usize| {
        let r_vox = r as f64 / pitch;
        (r_vox, (r_vox - half_width).max(0.0).powi(2), (r_vox + half_width).powi(2))
    };
    let mut per_radius = vec![(0u64, 0u128); r_max + 1];
    for (r, slot) in per_radius.iter_mut().enumerate().skip(1) {
        if bounds(r).0 <= 1e-12 {
            *slot = (1, cells);
        }
    }
    let outer = r_max as f64 / pitch + half_width + 1.0;
    let lim = if outer.is_finite() && outer < isize::MAX as f64 { outer.ceil() as isize } else { isize::MAX };
    let span = |n: usize| (n as isize - 1).min(lim);
    let (sx, sy, sz) = (span(nx), span(ny), span(nz));
    let accumulate = |mut acc: Vec<(u64, u128)>, dx: isize| {
        let ax = dx as u128;
        for ay in 0..=sy as u128 {
            let row2 = ax * ax + ay * ay;
            if (row2 as f64).sqrt() * pitch + 0.5 > r_max as f64 + 2.0 {
                break;
            }
            for az in 0..=sz as u128 {
                if ax == 0 && ay == 0 && az == 0 {
                    continue;
                }
                let d2 = (row2 + az * az) as f64;
                let guess = (d2.sqrt() * pitch + 0.5).floor();
                if !guess.is_finite() || guess > r_max as f64 + 1.0 {
                    break;
                }
                let guess = guess as usize;
                let signs = [ax, ay, az].iter().filter(|&&v| v != 0).count() as u32;
                let copies = 1u64 << signs;
                let work = (nx as u128 - ax) * (ny as u128 - ay) * (nz as u128 - az) * copies as u128;
                let first = guess.saturating_sub(1).max(1);
                let last = guess.saturating_add(1).min(r_max);
                for (r, slot) in acc.iter_mut().enumerate().take(last + 1).skip(first) {
                    let (r_vox, low2, high2) = bounds(r);
                    if r_vox > 1e-12 && d2 >= low2 && d2 < high2 {
                        slot.0 += copies;
                        slot.1 += work;
                    }
                }
            }
        }
        acc
    };
    let merge = |mut a: Vec<(u64, u128)>, b: Vec<(u64, u128)>| {
        for (x, y) in a.iter_mut().zip(b) {
            x.0 += y.0;
            x.1 += y.1;
        }
        a
    };
    let zero = || vec![(0u64, 0u128); r_max + 1];
    let found = (0..=sx)
        .into_par_iter()
        .fold(zero, accumulate)
        .reduce(zero, merge);
    merge(per_radius, found)
        .iter()
        .fold((0u64, 0u128, 0u64), |(k, w, m), &(rk, rw)| (k + rk, w + rw, m.max(rk)))
}

// AI-FUNC-SUMMARY: Estimate the FFT kernel's peak bytes with checked arithmetic: occupancy, complex grid, transpose when the transform uses more than one task, per-worker line/scratch buffers, axis plans and the output curve; None on overflow.
fn fft_working_set_bytes(dims: [usize; 3], padded: [usize; 3], r_max: usize, workers: usize) -> Option<u64> {
    let cells = dims.iter().try_fold(1u64, |a, &n| a.checked_mul(n as u64))?;
    let padded_cells = padded.iter().try_fold(1u64, |a, &n| a.checked_mul(n as u64))?;
    let tasks = (workers.max(1) as u64).min(padded_cells.div_ceil(65536)).max(1);
    let complex = std::mem::size_of::<Complex<f64>>() as u64;
    let arrays = padded_cells.checked_mul(complex)?.checked_mul(if tasks > 1 { 2 } else { 1 })?;
    let max_axis = *padded.iter().max()? as u64;
    let scratch = (workers.max(1) as u64).checked_mul(4)?.checked_mul(max_axis)?.checked_mul(complex)?;
    let plans = 12u64.checked_mul(max_axis)?.checked_mul(complex)?;
    let output = (r_max as u64).checked_add(1)?.checked_mul(32)?;
    cells.checked_add(arrays)?.checked_add(scratch)?.checked_add(plans)?.checked_add(output)
}

// AI-FUNC-SUMMARY: Estimate the direct kernel's peak bytes with checked arithmetic: occupancy, output curve and, per concurrently processed radius, the in-domain offset list plus its per-offset integer counts; None on overflow.
fn direct_working_set_bytes(dims: [usize; 3], r_max: usize, max_shell_offsets: u64, workers: usize) -> Option<u64> {
    let cells = dims.iter().try_fold(1u64, |a, &n| a.checked_mul(n as u64))?;
    let concurrent = (workers.max(1) as u64).min(r_max.max(1) as u64);
    let per_offset = (std::mem::size_of::<[isize; 3]>() + 2 * std::mem::size_of::<usize>()) as u64;
    let shells = max_shell_offsets.checked_mul(per_offset)?.checked_mul(concurrent)?;
    let output = (r_max as u64).checked_add(1)?.checked_mul(32)?;
    cells.checked_add(shells)?.checked_add(output)
}

// AI-FUNC-SUMMARY:
// Purpose: Choose the CPU exact S2 kernel (FFT autocorrelation or direct shell-pair enumeration) for one grid from a calibrated cost model under a peak-memory budget.
// Inputs: grid dims, r_max, positive pitch, worker count, working-set budget in bytes.
// Returns: ExactCpuPlan with both modeled wall times, both checked working sets, the selection and a reason string.
// Side effects: None (enumerates the clamped offset box once in the current Rayon pool).
// Notes: Constants are single-worker release fits (exact_cost_model_calibration). FFT time = NS_PER_FFT_UNIT*P*log2(P) with no parallel credit (the calibration measured none at 4 workers); direct time = NS_PER_DIRECT_PAIR*W / (1 + DIRECT_PARALLEL_EFFICIENCY*(min(workers, K)-1)). Only kernels whose working set fits are eligible; the cheaper eligible one wins (FFT on ties). When neither fits, direct is chosen because it needs the least memory and the reason says so. Both kernels return identical integer counts, so the choice never changes results.
pub(crate) fn plan_exact_cpu(dims: [usize; 3], r_max: usize, pitch: f64, workers: usize, budget_bytes: u64) -> ExactCpuPlan {
    let workers = workers.max(1);
    let padded_opt = padded_fft_dims(dims);
    let padded = padded_opt.unwrap_or([usize::MAX; 3]);
    let (offsets, pair_work, max_shell) = exact_shell_work(dims, r_max, pitch);
    let fft_bytes = padded_opt.and_then(|p| fft_working_set_bytes(dims, p, r_max, workers));
    let direct_bytes = direct_working_set_bytes(dims, r_max, max_shell, workers);
    let padded_cells = padded.iter().fold(1f64, |a, &n| a * n as f64);
    let fft_seconds = NS_PER_FFT_UNIT * padded_cells * padded_cells.max(2.0).log2() * 1e-9;
    let direct_tasks = (workers as f64).min(offsets.max(1) as f64);
    let direct_speedup = 1.0 + DIRECT_PARALLEL_EFFICIENCY * (direct_tasks - 1.0);
    let direct_seconds = NS_PER_DIRECT_PAIR * pair_work as f64 / direct_speedup * 1e-9;
    let fits = |b: Option<u64>| b.is_some_and(|b| b <= budget_bytes);
    let (method, reason) = match (fits(fft_bytes), fits(direct_bytes)) {
        (true, true) if fft_seconds <= direct_seconds => (ExactCpuMethod::Fft, "both fit budget; fft modeled cheaper"),
        (true, true) => (ExactCpuMethod::Direct, "both fit budget; direct modeled cheaper"),
        (true, false) => (ExactCpuMethod::Fft, "only fft fits budget"),
        (false, true) => (ExactCpuMethod::Direct, "fft working set exceeds budget"),
        (false, false) => (ExactCpuMethod::Direct, "no kernel fits budget; direct needs least memory"),
    };
    ExactCpuPlan {
        method,
        padded,
        offsets,
        pair_work,
        fft_bytes,
        direct_bytes,
        fft_seconds,
        direct_seconds,
        budget_bytes,
        reason: reason.to_string(),
    }
}

type ExactPlanKey = ([usize; 3], usize, u64, usize, u64);

static EXACT_PLAN_CACHE: std::sync::Mutex<Option<(ExactPlanKey, ExactCpuPlan)>> = std::sync::Mutex::new(None);

// AI-FUNC-SUMMARY: Return the plan for (dims, r_max, pitch, workers, budget), reusing the last computed plan for an identical key and printing one "[Info] CPU exact S2 plan" line only when the key changes, so repeated SA evaluations neither replan nor spam.
pub(crate) fn cached_exact_plan(dims: [usize; 3], r_max: usize, pitch: f64, workers: usize, budget_bytes: u64) -> ExactCpuPlan {
    let key = (dims, r_max, pitch.to_bits(), workers, budget_bytes);
    if let Ok(guard) = EXACT_PLAN_CACHE.lock() {
        if let Some((cached_key, plan)) = guard.as_ref() {
            if *cached_key == key {
                return plan.clone();
            }
        }
    }
    let plan = plan_exact_cpu(dims, r_max, pitch, workers, budget_bytes);
    println!("[Info] CPU exact S2 plan: grid={dims:?} r_max={r_max} workers={workers} {}", plan.describe());
    if let Ok(mut guard) = EXACT_PLAN_CACHE.lock() {
        *guard = Some((key, plan.clone()));
    }
    plan
}

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
    // AI-FUNC-SUMMARY: Allocate a dimension-specific FFT grid/transpose and six shared immutable axis plans for the caller's 2,3,5-smooth padded dimensions.
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
    let padded = padded_fft_dims(dims).expect("FFT dimensions overflow");
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

// AI-FUNC-SUMMARY: Return true when shift (dx,dy,dz) leaves at least one valid voxel pair inside an nx*ny*nz grid; side effects: None.
fn offset_in_domain(off: &[isize; 3], dims: [usize; 3]) -> bool {
    off.iter().zip(dims).all(|(d, n)| (d.unsigned_abs()) < n)
}

// AI-FUNC-SUMMARY: Count (hits, valid) voxel pairs for one in-domain shift by scanning contiguous z runs of both endpoints; returns integers identical to the per-voxel loop; side effects: None.
fn direct_pair_counts(occ: &[bool], dims: [usize; 3], off: [isize; 3]) -> (usize, usize) {
    let [nx, ny, nz] = dims;
    let [dx, dy, dz] = off;
    let range = |d: isize, n: usize| {
        if d < 0 {
            (d.unsigned_abs(), n)
        } else {
            (0, n - d as usize)
        }
    };
    let (x0, x1) = range(dx, nx);
    let (y0, y1) = range(dy, ny);
    let (z0, z1) = range(dz, nz);
    let run = z1 - z0;
    let mut hits = 0usize;
    for x in x0..x1 {
        let x2 = (x as isize + dx) as usize;
        for y in y0..y1 {
            let y2 = (y as isize + dy) as usize;
            let a = index_3d_to_flat(x, y, z0, ny, nz);
            let b = index_3d_to_flat(x2, y2, (z0 as isize + dz) as usize, ny, nz);
            hits += occ[a..a + run]
                .iter()
                .zip(&occ[b..b + run])
                .filter(|(p, q)| **p && **q)
                .count();
        }
    }
    (hits, (x1 - x0) * (y1 - y0) * run)
}

// AI-FUNC-SUMMARY: Assemble per-radius (value, supported) results into the output curve, interpolating unsupported radii and pinning S2(0)=vf; side effects: None.
fn finish_exact_curve(results: Vec<(f64, bool)>, vf: f64) -> Vec<f64> {
    let mut out = vec![0.0; results.len()];
    let mut has_support = vec![false; results.len()];
    for (r, (value, supported)) in results.into_iter().enumerate() {
        out[r] = value;
        has_support[r] = supported;
    }
    fill_missing_s2_with_smooth_interpolation(&mut out, &has_support, vf);
    out[0] = vf;
    out
}

// AI-FUNC-SUMMARY:
// Purpose: Compute exact S2 by direct pair enumeration per shell offset (no FFT).
// Inputs: occupancy grid, dimensions, max radius, voxel pitch, volume fraction at r=0.
// Returns: S2 values for r=0..r_max with smooth interpolation for unsupported radii.
// Side effects: None.
// Notes: Parallel over radii and, inside each radius, over its in-domain offsets (indexed collect, then an in-order sum), so results are bit-identical to calculate_s2_exact_fft for any worker count.
fn calculate_s2_exact_direct(
    occ: &[bool],
    nx: usize,
    ny: usize,
    nz: usize,
    r_max: usize,
    voxel_pitch: f64,
    vf: f64,
) -> Vec<f64> {
    let dims = [nx, ny, nz];
    let min_len = (65536 / (nx * ny * nz).max(1)).max(1);
    let results: Vec<(f64, bool)> = (0..=r_max)
        .into_par_iter()
        .map(|r| {
            if r == 0 {
                return (vf, true);
            }
            let offsets: Vec<[isize; 3]> = shell_offset_iter(r as f64 / voxel_pitch, 0.5 / voxel_pitch)
                .filter(|off| offset_in_domain(off, dims))
                .collect();
            if offsets.is_empty() {
                return (0.0, false);
            }
            let counts: Vec<(usize, usize)> = offsets
                .par_iter()
                .with_min_len(min_len)
                .map(|&off| direct_pair_counts(occ, dims, off))
                .collect();
            let shell_sum = counts
                .iter()
                .fold(0.0, |sum, &(hits, valid)| sum + hits as f64 / valid as f64);
            (shell_sum / counts.len() as f64, true)
        })
        .collect();
    finish_exact_curve(results, vf)
}

// AI-FUNC-SUMMARY:
// Purpose: Compute exact S2 using FFT-based autocorrelation.
// Inputs: occupancy grid, dimensions, max radius, voxel pitch, volume fraction at r=0.
// Returns: S2 values for r=0..r_max with smooth interpolation for unsupported radii.
// Side effects: None.
// Notes: Each correlation value is rounded to the nearest integer pair count before use, so every shell term equals the direct kernel's integer ratio and the result is independent of padding and FFT rounding; offsets come from the lazy shell iterator in the same order as the direct kernel.
fn calculate_s2_exact_fft(
    occ: &[bool],
    nx: usize,
    ny: usize,
    nz: usize,
    r_max: usize,
    voxel_pitch: f64,
    vf: f64,
) -> Vec<f64> {
    let dims = [nx, ny, nz];
    with_fft_correlation(occ, dims, |corr, [fx, fy, fz]| {
        let wrap = |d: isize, n: usize| if d >= 0 { d as usize } else { n - d.unsigned_abs() };
        let results: Vec<(f64, bool)> = (0..=r_max)
            .into_par_iter()
            .map(|r| {
                if r == 0 {
                    return (vf, true);
                }
                let mut shell_sum = 0.0;
                let mut used = 0usize;
                for off in shell_offset_iter(r as f64 / voxel_pitch, 0.5 / voxel_pitch) {
                    if !offset_in_domain(&off, dims) {
                        continue;
                    }
                    let [dx, dy, dz] = off;
                    let valid = (nx - dx.unsigned_abs()) * (ny - dy.unsigned_abs()) * (nz - dz.unsigned_abs());
                    let hits = corr[fft_index_3d(wrap(dx, fx), wrap(dy, fy), wrap(dz, fz), fy, fz)]
                        .re
                        .round()
                        .max(0.0);
                    shell_sum += hits / valid as f64;
                    used += 1;
                }
                if used == 0 {
                    (0.0, false)
                } else {
                    (shell_sum / used as f64, true)
                }
            })
            .collect();
        finish_exact_curve(results, vf)
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
// Side effects: Prints warning when exact requested with invalid voxel_pitch; exact prints one "[Info] CPU exact S2 plan" line per new plan key.
// Notes: For "exact" method, VoxelS2::calculate picks FFT or direct enumeration via the cached cost/memory plan (identical results either way). For other methods, uses voxelized MC sampling.
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

    // AI-FUNC-SUMMARY: Evaluate exact S2 with an explicitly chosen CPU kernel on the prepared grid; both kernels return identical values, so this exists for selection-independence tests and benchmarks.
    pub(crate) fn calculate_exact_with(&self, r_max: usize, method: ExactCpuMethod) -> Vec<f64> {
        let [nx, ny, nz] = self.dims;
        let occ = &self.occupancy;
        let occupied = occ.iter().filter(|&&v| v).count();
        if occupied == 0 {
            return vec![0.0; r_max + 1];
        }
        let vf = occupied as f64 / (nx * ny * nz).max(1) as f64;
        let pitch = self.pitch.max(1e-9);
        match method {
            ExactCpuMethod::Fft => calculate_s2_exact_fft(occ, nx, ny, nz, r_max, pitch, vf),
            ExactCpuMethod::Direct => calculate_s2_exact_direct(occ, nx, ny, nz, r_max, pitch, vf),
        }
    }

    // AI-FUNC-SUMMARY: Evaluate exact or voxel MC S2 on an immutable prepared grid without repeating mesh splitting or containment queries; exact picks FFT or direct through the cached cost/memory plan (logged once per plan key); returns the occupancy VF at radius zero.
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
            let pitch = effective_pitch.max(1e-9);
            let plan = cached_exact_plan(self.dims, r_max, pitch, rayon::current_num_threads(), DEFAULT_CPU_EXACT_BUDGET_BYTES);
            self.calculate_exact_with(r_max, plan.method)
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
    let mut shell_pipeline = crate::gpu::s2_shell::GpuShellS2Pipeline::with_device(vox_pipeline.shared_device())?;
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
                        assert_eq!(direct, fft, "{dims:?} shift={shift} workers={workers}");
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
            assert_eq!(fft, direct);
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
        for [nx, ny, nz] in [[1, 2, 9], [6, 4, 3], [8, 2, 5], [7, 11, 13], [1, 1, 40], [2, 37, 3], [4, 1, 1]] {
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
                        assert!((corr[i] - count as f64).abs() < 1e-6, "{nx},{ny},{nz}: {dx},{dy},{dz}");
                        assert_eq!(corr[i].round() as i64, count, "{nx},{ny},{nz}: {dx},{dy},{dz}");
                    }
                }
            }
        }
    }
}


#[cfg(test)]
mod exact_plan_tests {
    use super::*;

    // AI-FUNC-SUMMARY: Test helper building a prepared grid from a deterministic nonuniform occupancy pattern without a mesh.
    fn grid(dims: [usize; 3], pitch: f64, salt: usize) -> VoxelS2 {
        let n = dims.iter().product::<usize>();
        let occupancy = (0..n).map(|i| (i * 37 + i / 5 + salt) % 11 < 4).collect();
        VoxelS2 { occupancy, dims, pitch }
    }

    // AI-FUNC-SUMMARY: Check every padded length is the smallest 2,3,5-smooth value >= 2N-1 against brute force, including the degenerate N=0/1 axes.
    #[test]
    fn smooth_padding_is_minimal_and_linear_safe() {
        let smooth = |mut v: usize| {
            for p in [2, 3, 5] {
                while v.is_multiple_of(p) {
                    v /= p;
                }
            }
            v == 1
        };
        for n in 0..3000usize {
            let need = (2 * n).saturating_sub(1).max(1);
            let got = smooth_fft_length(need).unwrap();
            let brute = (need..).find(|&v| smooth(v)).unwrap();
            assert_eq!(got, brute, "n={n}");
        }
        assert_eq!(padded_fft_dims([7, 11, 13]), Some([15, 24, 25]));
        assert_eq!(padded_fft_dims([1, 1, 40]), Some([1, 1, 80]));
        assert!(padded_fft_dims([usize::MAX, 1, 1]).is_none());
    }

    // AI-FUNC-SUMMARY: Verify the planner's single-pass offset count and pair work equal a brute-force sum over the lazy shell iterator for integer and fractional pitches.
    #[test]
    fn planner_shell_work_matches_shell_enumeration() {
        for (dims, r_max, pitch) in [([6, 4, 3], 6, 1.0), ([9, 2, 7], 5, 0.5), ([5, 5, 5], 4, 2.0), ([1, 1, 30], 12, 0.7), ([3, 3, 3], 3, 1e12)] {
            let mut k = 0u64;
            let mut w = 0u128;
            let mut max_k = 0u64;
            for r in 1..=r_max {
                let mut shell = 0u64;
                for off in shell_offset_iter(r as f64 / pitch, 0.5 / pitch) {
                    if offset_in_domain(&off, dims) {
                        shell += 1;
                        w += off.iter().zip(dims).map(|(d, n)| (n - d.unsigned_abs()) as u128).product::<u128>();
                    }
                }
                k += shell;
                max_k = max_k.max(shell);
            }
            assert_eq!(exact_shell_work(dims, r_max, pitch), (k, w, max_k), "{dims:?} r_max={r_max} pitch={pitch}");
        }
    }

    // AI-FUNC-SUMMARY: Force FFT and direct kernels on the same grids (long/thin, FFT-unfriendly, fractional pitch) under 1/2/8 workers and require bit-identical curves, also matching the planner-selected calculate().
    #[test]
    fn forced_kernels_are_selection_independent() {
        for workers in [1, 2, 8] {
            rayon::ThreadPoolBuilder::new().num_threads(workers).build().unwrap().install(|| {
                for (dims, r_max, pitch) in [([7, 11, 13], 9, 1.0), ([1, 1, 64], 20, 1.0), ([2, 37, 3], 12, 0.5), ([16, 16, 16], 6, 1.5), ([13, 5, 9], 4, 1.0)] {
                    for salt in 0..2 {
                        let g = grid(dims, pitch, salt);
                        let fft = g.calculate_exact_with(r_max, ExactCpuMethod::Fft);
                        let direct = g.calculate_exact_with(r_max, ExactCpuMethod::Direct);
                        assert_eq!(fft, direct, "{dims:?} pitch={pitch} workers={workers}");
                        assert_eq!(g.calculate(r_max, "exact", 0), fft);
                    }
                }
            });
        }
    }

    // AI-FUNC-SUMMARY: Check budget gating: a budget below the FFT working set forces direct, a budget admitting only FFT forces FFT, and an unusable budget still reports direct with an explicit reason.
    #[test]
    fn planner_respects_memory_budget() {
        let dims = [40, 40, 40];
        let open = plan_exact_cpu(dims, 3, 1.0, 4, u64::MAX);
        let fft = open.fft_bytes.unwrap();
        let direct = open.direct_bytes.unwrap();
        assert!(fft > direct && fft >= 79u64.pow(3) * 16);
        let no_fft = plan_exact_cpu(dims, 3, 1.0, 4, fft - 1);
        assert_eq!(no_fft.method, ExactCpuMethod::Direct);
        assert!(no_fft.fits_budget() && no_fft.reason.contains("fft working set exceeds"));
        let none = plan_exact_cpu(dims, 3, 1.0, 4, 1);
        assert_eq!(none.method, ExactCpuMethod::Direct);
        assert!(!none.fits_budget() && none.reason.contains("no kernel fits"));
        let far = plan_exact_cpu([64, 64, 64], 60, 1.0, 1, u64::MAX);
        assert_eq!(far.method, ExactCpuMethod::Fft, "{}", far.describe());
        let near = plan_exact_cpu([200, 200, 200], 1, 1.0, 1, u64::MAX);
        assert_eq!(near.method, ExactCpuMethod::Direct, "{}", near.describe());
    }

    // AI-FUNC-SUMMARY: Release calibration benchmark: time FFT and direct kernels on grids spanning both regimes at 1 and 4 workers, printing raw seconds with modeled units (P*log2 P and pair work) so NS_PER_FFT_UNIT and NS_PER_DIRECT_PAIR can be fitted.
    #[test]
    #[ignore = "release cost-model calibration"]
    fn exact_cost_model_calibration() {
        let cases: [([usize; 3], usize); 6] = [([32, 32, 32], 2), ([32, 32, 32], 8), ([64, 64, 64], 2), ([64, 64, 64], 6), ([96, 96, 96], 3), ([128, 128, 32], 4)];
        for workers in [1, 4] {
            let pool = rayon::ThreadPoolBuilder::new().num_threads(workers).build().unwrap();
            pool.install(|| {
                for (dims, r_max) in cases {
                    let g = grid(dims, 1.0, 0);
                    let plan = plan_exact_cpu(dims, r_max, 1.0, workers, u64::MAX);
                    let p = plan.padded.iter().product::<usize>() as f64;
                    g.calculate_exact_with(r_max, ExactCpuMethod::Fft);
                    for sample in 0..5 {
                        let mut t = [0.0f64; 2];
                        let order = if sample % 2 == 0 { [0, 1] } else { [1, 0] };
                        for m in order {
                            let method = if m == 0 { ExactCpuMethod::Fft } else { ExactCpuMethod::Direct };
                            let start = std::time::Instant::now();
                            std::hint::black_box(g.calculate_exact_with(r_max, method));
                            t[m] = start.elapsed().as_secs_f64();
                        }
                        println!(
                            "EXACT_COST dims={dims:?} r_max={r_max} workers={workers} sample={sample} padded={:?} p_log2p={:.0} offsets={} pair_work={} fft_s={:.6} direct_s={:.6} ns_per_fft_unit={:.4} ns_per_pair={:.4} planned={:?}",
                            plan.padded, p * p.log2(), plan.offsets, plan.pair_work, t[0], t[1],
                            t[0] * 1e9 / (p * p.log2()),
                            t[1] * 1e9 / plan.pair_work as f64,
                            plan.method
                        );
                    }
                }
            });
        }
    }

    // AI-FUNC-SUMMARY: Release benchmark comparing exact 2N-1 padding with 2,3,5-smooth padding on FFT-unfriendly axis lengths, printing raw correlation times per sample.
    #[test]
    #[ignore = "release padding benchmark"]
    fn smooth_padding_benchmark() {
        for dims in [[50, 50, 50], [64, 64, 64], [71, 67, 53], [100, 100, 20]] {
            let n = dims.iter().product::<usize>();
            let occ: Vec<bool> = (0..n).map(|i| i % 7 < 3).collect();
            let naive = dims.map(|d| 2 * d - 1);
            let smooth = padded_fft_dims(dims).unwrap();
            for sample in 0..5 {
                let mut t = [0.0f64; 2];
                for (slot, padded) in [(0usize, naive), (1usize, smooth)] {
                    FFT_WORKSPACE.with(|s| *s.borrow_mut() = None);
                    let mut ws = FftWorkspace::new(padded);
                    for (i, v) in occ.iter().enumerate() {
                        let (x, y, z) = (i / (dims[1] * dims[2]), (i / dims[2]) % dims[1], i % dims[2]);
                        ws.grid[fft_index_3d(x, y, z, padded[1], padded[2])].re = if *v { 1.0 } else { 0.0 };
                    }
                    let start = std::time::Instant::now();
                    ws.transform(false);
                    for v in &mut ws.grid {
                        *v *= v.conj();
                    }
                    ws.transform(true);
                    t[slot] = start.elapsed().as_secs_f64();
                    std::hint::black_box(ws.grid[0]);
                }
                println!("FFT_PADDING dims={dims:?} sample={sample} naive={naive:?} smooth={smooth:?} naive_s={:.6} smooth_s={:.6} workers={}", t[0], t[1], rayon::current_num_threads());
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
