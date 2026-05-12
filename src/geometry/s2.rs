use crate::types::{BoundingBox, Mesh, Vec3};
use super::bbox::mesh_bbox;
use super::mesh_ops::split_mesh_into_granules;
use super::volume::volume_fraction_in_bbox;
use rand::Rng;
use rayon::prelude::*;
use rustfft::num_complex::Complex;
use rustfft::FftPlanner;

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
fn ray_intersects_triangle(origin: Vec3, dir: Vec3, a: Vec3, b: Vec3, c: Vec3) -> Option<f64> {
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
// Notes: Parallelizes over x-slabs via rayon. Splits mesh into granules for per-granule bbox culling.
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

        let x1 = (((pb.max.x - bbox.min.x) / voxel_pitch).ceil() as isize).min(nx as isize) as usize;
        let y1 = (((pb.max.y - bbox.min.y) / voxel_pitch).ceil() as isize).min(ny as isize) as usize;
        let z1 = (((pb.max.z - bbox.min.z) / voxel_pitch).ceil() as isize).min(nz as isize) as usize;

        if x0 < x1 && y0 < y1 && z0 < z1 {
            part_ranges.push((p, x0, x1, y0, y1, z0, z1));
        }
    }

    occ.par_chunks_mut(ny * nz)
        .enumerate()
        .for_each(|(x, slab)| {
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
                        if point_inside_mesh(p, center) {
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

// AI-FUNC-SUMMARY:
// Purpose: Perform a separable 3D FFT (or inverse IFFT) in-place on a complex data buffer.
// Inputs: complex data buffer, nx/ny/nz dimensions, inverse flag.
// Returns: None (modifies data in place).
// Side effects: Mutates the data buffer.
// Notes: Each thread creates its own FftPlanner (Fft is not Clone). Normalizes by N on inverse.
fn fft_3d_in_place(data: &mut [Complex<f64>], nx: usize, ny: usize, nz: usize, inverse: bool) {
    let mut planner = FftPlanner::<f64>::new();
    let fft_z = if inverse {
        planner.plan_fft_inverse(nz)
    } else {
        planner.plan_fft_forward(nz)
    };
    data.par_chunks_mut(nz).for_each(|line| {
        fft_z.process(line);
    });

    let fft_y = if inverse {
        planner.plan_fft_inverse(ny)
    } else {
        planner.plan_fft_forward(ny)
    };
    data.par_chunks_mut(ny * nz).for_each(|x_slab| {
        let mut tmp_y = vec![Complex::<f64>::new(0.0, 0.0); ny];
        for z in 0..nz {
            for y in 0..ny {
                tmp_y[y] = x_slab[y * nz + z];
            }
            fft_y.process(&mut tmp_y);
            for y in 0..ny {
                x_slab[y * nz + z] = tmp_y[y];
            }
        }
    });

    let fft_x = if inverse {
        planner.plan_fft_inverse(nx)
    } else {
        planner.plan_fft_forward(nx)
    };
    let total_lines = ny * nz;
    let mut buf = vec![Complex::<f64>::new(0.0, 0.0); nx * total_lines];
    {
        let data_ref: &[Complex<f64>] = data;
        buf.par_chunks_mut(nx * nz).enumerate().for_each(|(y, y_slab)| {
            for z in 0..nz {
                for x in 0..nx {
                    y_slab[z * nx + x] = data_ref[fft_index_3d(x, y, z, ny, nz)];
                }
            }
        });
    }
    buf.par_chunks_mut(nx).for_each(|line| {
        fft_x.process(line);
    });
    {
        let buf_ref: &[Complex<f64>] = &buf;
        data.par_chunks_mut(ny * nz).enumerate().for_each(|(x, x_slab)| {
            for y in 0..ny {
                for z in 0..nz {
                    x_slab[y * nz + z] = buf_ref[(y * nz + z) * nx + x];
                }
            }
        });
    }

    if inverse {
        let norm = (nx * ny * nz) as f64;
        for v in data.iter_mut() {
            *v /= norm;
        }
    }
}

// AI-FUNC-SUMMARY:
// Purpose: Compute occupancy autocorrelation counts via FFT convolution (FFT -> power spectrum -> IFFT).
// Inputs: occupancy grid and nx/ny/nz dimensions.
// Returns: Correlation count grid and padded FFT dimensions [fx, fy, fz].
// Side effects: None.
// Notes: Pads to 2N-1 per axis to avoid circular convolution artifacts.
fn autocorrelation_counts_fft(occ: &[bool], nx: usize, ny: usize, nz: usize) -> (Vec<f64>, [usize; 3]) {
    let fx = (2 * nx).saturating_sub(1).max(1);
    let fy = (2 * ny).saturating_sub(1).max(1);
    let fz = (2 * nz).saturating_sub(1).max(1);
    let mut grid = vec![Complex::<f64>::new(0.0, 0.0); fx * fy * fz];

    for x in 0..nx {
        for y in 0..ny {
            for z in 0..nz {
                if occ[index_3d_to_flat(x, y, z, ny, nz)] {
                    grid[fft_index_3d(x, y, z, fy, fz)] = Complex::new(1.0, 0.0);
                }
            }
        }
    }

    fft_3d_in_place(&mut grid, fx, fy, fz, false);
    for v in grid.iter_mut() {
        *v *= v.conj();
    }
    fft_3d_in_place(&mut grid, fx, fy, fz, true);

    let corr = grid.iter().map(|v| v.re.max(0.0)).collect::<Vec<_>>();
    (corr, [fx, fy, fz])
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
    let (corr, fdims) = autocorrelation_counts_fft(occ, nx, ny, nz);
    let [fx, fy, fz] = fdims;

    let get_corr = |dx: isize, dy: isize, dz: isize| {
        let ix = if dx >= 0 { dx as usize } else { (fx as isize + dx) as usize };
        let iy = if dy >= 0 { dy as usize } else { (fy as isize + dy) as usize };
        let iz = if dz >= 0 { dz as usize } else { (fz as isize + dz) as usize };
        corr[fft_index_3d(ix, iy, iz, fy, fz)]
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
}

// AI-FUNC-SUMMARY:
// Purpose: Compute S2 via Monte Carlo sampling directly on the mesh (no voxelization).
// Inputs: mesh, bounding box, max radius, sample count.
// Returns: S2 values for r=0..r_max.
// Side effects: None.
// Notes: Parallelizes over radii. Uses random point pairs with random directions; discards out-of-bounds pairs.
fn calculate_s2_monte_carlo_mesh(
    mesh: &Mesh,
    bbox: BoundingBox,
    r_max: usize,
    samples: usize,
) -> Vec<f64> {
    let vf = volume_fraction_in_bbox(mesh, bbox);
    let mc_samples = samples.max(200);
    let min = bbox.min;
    let max = bbox.max;

    let mut out: Vec<f64> = (0..=r_max)
        .into_par_iter()
        .map(|r| {
            if r == 0 {
                return vf;
            }

            let rr = r as f64;
            let mut valid = 0usize;
            let mut hits = 0usize;
            let mut rng = rand::thread_rng();

            for _ in 0..mc_samples {
                let p = Vec3::new(
                    rng.gen_range(min.x..max.x),
                    rng.gen_range(min.y..max.y),
                    rng.gen_range(min.z..max.z),
                );

                let dir = loop {
                    let x = rng.gen_range(-1.0f64..1.0f64);
                    let y = rng.gen_range(-1.0f64..1.0f64);
                    let z = rng.gen_range(-1.0f64..1.0f64);
                    let n2: f64 = x * x + y * y + z * z;
                    if n2 > 1e-12 && n2 <= 1.0 {
                        let inv = 1.0 / n2.sqrt();
                        break Vec3::new(x * inv, y * inv, z * inv);
                    }
                };

                let q = p.add(dir.scale(rr));
                if q.x < min.x
                    || q.x > max.x
                    || q.y < min.y
                    || q.y > max.y
                    || q.z < min.z
                    || q.z > max.z
                {
                    continue;
                }

                valid += 1;
                if point_inside_mesh(mesh, p) && point_inside_mesh(mesh, q) {
                    hits += 1;
                }
            }

            if valid == 0 {
                0.0
            } else {
                hits as f64 / valid as f64
            }
        })
        .collect();
    out[0] = vf;
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

    let (occ, dims) = build_bbox_occupancy(mesh, bbox, effective_pitch.max(1e-9));
    let [nx, ny, nz] = dims;
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
                calculate_s2_exact_direct(&occ, nx, ny, nz, r_max, effective_pitch.max(1e-9), vf)
            } else {
                calculate_s2_exact_fft(&occ, nx, ny, nz, r_max, effective_pitch.max(1e-9), vf)
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
// Notes: Falls back to CPU calculate_s2 when gpu_pipeline is None or method is exact. Sets r=0 to volume fraction. Only available with feature "gpu".
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
            let mut result = gpu.calculate_s2_gpu(bbox, r_max, samples);
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
pub fn calculate_s2_gpu_exact(
    mesh: &Mesh,
    bbox: BoundingBox,
    r_max: usize,
    voxel_pitch: f64,
) -> Vec<f64> {
    let size = bbox.size();
    let pitch = voxel_pitch.max(1e-9) as f32;
    let nx = ((size.x / voxel_pitch).ceil() as u32).max(1);
    let ny = ((size.y / voxel_pitch).ceil() as u32).max(1);
    let nz = ((size.z / voxel_pitch).ceil() as u32).max(1);
    let total_vox = (nx * ny * nz) as usize;

    let t0 = std::time::Instant::now();

    let mut vox_pipeline = match crate::gpu::voxel::GpuVoxelPipeline::new(mesh, bbox) {
        Ok(p) => p,
        Err(e) => {
            println!("[Warning] GPU voxel pipeline init failed: {e}, falling back to CPU");
            return calculate_s2(mesh, bbox, r_max, voxel_pitch, "exact", 10000);
        }
    };
    let occ = vox_pipeline.voxelize(nx, ny, nz, pitch);
    let occupied: u32 = occ.iter().sum();
    let vf = occupied as f64 / total_vox as f64;

    if occupied == 0 {
        return vec![0.0; r_max + 1];
    }

    let pitch_f64 = voxel_pitch.max(1e-9);
    let half_width = 0.5 / pitch_f64;
    let mut all_offsets: Vec<(u32, [isize; 3])> = Vec::new();
    for r in 1..=r_max {
        let r_vox = r as f64 / pitch_f64;
        let offsets = shell_offsets_for_distance(r_vox, half_width);
        for off in offsets {
            all_offsets.push((r as u32, off));
        }
    }

    let mut shell_pipeline = match crate::gpu::s2_shell::GpuShellS2Pipeline::new() {
        Ok(p) => p,
        Err(e) => {
            println!("[Warning] GPU shell pipeline init failed: {e}, falling back to CPU");
            return calculate_s2(mesh, bbox, r_max, voxel_pitch, "exact", 10000);
        }
    };

    let mut result = shell_pipeline.compute_s2_shell(&occ, nx, ny, nz, &all_offsets, r_max, pitch_f64, vf);

    let mut has_support = vec![false; r_max + 1];
    has_support[0] = true;
    for r in 1..=r_max {
        let r_vox = r as f64 / pitch_f64;
        let offsets = shell_offsets_for_distance(r_vox, 0.5 / pitch_f64);
        if !offsets.is_empty() {
            has_support[r] = true;
        }
    }
    fill_missing_s2_with_smooth_interpolation(&mut result, &has_support, vf);
    result[0] = vf;

    let elapsed = t0.elapsed().as_secs_f64();
    println!("[Info] GPU exact S2: {:.3}s, {}x{}x{} grid, {} offsets, {} radii",
        elapsed, nx, ny, nz, all_offsets.len(), r_max + 1);
    result
}
