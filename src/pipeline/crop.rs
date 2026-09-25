use crate::config::CropConfig;
use crate::error::{Result, RustMsptError};
use crate::io::{
    load_raw_folder, load_tiff_or_folder_with_range, save_tiff_or_folder_with_ext, ByteOrder,
    RawFolderSpec, Volume3D, VolumeNumericType,
};
use crate::pipeline::Pipeline;
use nalgebra::{Matrix3, SymmetricEigen, Vector3};
use rayon::prelude::*;
use std::cmp::Ordering;
use std::collections::HashMap;
use std::path::Path;

pub struct CropPipeline {
    pub config: CropConfig,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum InterpolationMode {
    Nearest,
    Trilinear,
}

// AI-FUNC-SUMMARY: Check lossless i64-to-i32 upload and, for trilinear interpolation, exact f32 input representation; nearest retains all i32 label bits.
fn gpu_crop_values_supported(volume: &Volume3D, background: i64, mode: InterpolationMode) -> bool {
    let supported = |value: i64| {
        i32::try_from(value).is_ok()
            && (matches!(mode, InterpolationMode::Nearest) || (value as f32) as i64 == value)
    };
    supported(background) && volume.data.iter().copied().all(supported)
}

// AI-FUNC-SUMMARY: Parse byte order config string into ByteOrder enum; returns ByteOrder; side effects: None.
fn parse_byte_order(value: Option<&str>) -> Result<ByteOrder> {
    match value
        .unwrap_or("little")
        .trim()
        .to_ascii_lowercase()
        .as_str()
    {
        "little" | "le" => Ok(ByteOrder::LittleEndian),
        "big" | "be" => Ok(ByteOrder::BigEndian),
        other => Err(RustMsptError::InvalidConfig(format!(
            "Invalid byte_order: {other}. Expected little or big"
        ))),
    }
}

// AI-FUNC-SUMMARY: Parse interpolation mode config string (nearest/trilinear); returns InterpolationMode, defaulting to trilinear; side effects: None.
fn parse_interpolation_mode(value: Option<&str>) -> Result<InterpolationMode> {
    match value
        .unwrap_or("trilinear")
        .trim()
        .to_ascii_lowercase()
        .as_str()
    {
        "nearest" => Ok(InterpolationMode::Nearest),
        "trilinear" => Ok(InterpolationMode::Trilinear),
        other => Err(RustMsptError::InvalidConfig(format!(
            "Invalid interpolation: {other}. Expected nearest or trilinear"
        ))),
    }
}

// AI-FUNC-SUMMARY:
// Purpose: Read input volume according to crop config type (raw or tiff).
// Inputs: CropConfig with input type, path, slice range, and optional raw spec.
// Returns: Loaded Volume3D.
// Side effects: Reads files from disk.
// Notes: Returns InvalidConfig for unsupported input types.
fn load_input_volume(config: &CropConfig) -> Result<Volume3D> {
    let input = &config.input;
    let path = Path::new(&input.path);
    let slice_start = input.slice_start.unwrap_or(-1) as isize;
    let slice_end = input.slice_end.unwrap_or(-1) as isize;

    match input.r#type.trim().to_ascii_lowercase().as_str() {
        "raw" => {
            let raw = input.raw.as_ref().ok_or_else(|| {
                RustMsptError::InvalidConfig(
                    "input.raw is required when input.type=raw (width/height/bits/signed/byte_order)"
                        .to_string(),
                )
            })?;
            let spec = RawFolderSpec {
                folder: path.to_path_buf(),
                width: raw.width,
                height: raw.height,
                bits: raw.bits,
                signed: raw.signed,
                byte_order: parse_byte_order(raw.byte_order.as_deref())?,
                slice_start,
                slice_end,
            };
            load_raw_folder(&spec)
        }
        "tiff" | "tif" => load_tiff_or_folder_with_range(path, slice_start, slice_end),
        other => Err(RustMsptError::InvalidConfig(format!(
            "Unsupported crop input.type: {other}. Expected raw or tiff"
        ))),
    }
}

// AI-FUNC-SUMMARY: Compute linear data index for voxel coordinates (x, y, z) in a volume; returns usize; side effects: None.
fn voxel_index(width: usize, height: usize, x: usize, y: usize, z: usize) -> usize {
    z * width * height + y * width + x
}

// AI-FUNC-SUMMARY: Sample a voxel value at integer coordinates, returning background value when out of bounds; returns i64; side effects: None.
fn sample_voxel_or_background(
    volume: &Volume3D,
    background: i64,
    x: isize,
    y: isize,
    z: isize,
) -> i64 {
    if x < 0
        || y < 0
        || z < 0
        || x as usize >= volume.width
        || y as usize >= volume.height
        || z as usize >= volume.depth
    {
        return background;
    }
    let idx = voxel_index(
        volume.width,
        volume.height,
        x as usize,
        y as usize,
        z as usize,
    );
    volume.data[idx]
}

// AI-FUNC-SUMMARY: Sample source volume with nearest-neighbor interpolation at floating-point coordinates; returns i64; side effects: None.
fn sample_nearest(volume: &Volume3D, background: i64, src_x: f64, src_y: f64, src_z: f64) -> i64 {
    let x = src_x.round() as isize;
    let y = src_y.round() as isize;
    let z = src_z.round() as isize;
    sample_voxel_or_background(volume, background, x, y, z)
}

// AI-FUNC-SUMMARY: Sample source volume with trilinear interpolation at floating-point coordinates; returns i64 (rounded); side effects: None.
fn sample_trilinear(volume: &Volume3D, background: i64, src_x: f64, src_y: f64, src_z: f64) -> i64 {
    let x0 = src_x.floor() as isize;
    let y0 = src_y.floor() as isize;
    let z0 = src_z.floor() as isize;
    let x1 = x0 + 1;
    let y1 = y0 + 1;
    let z1 = z0 + 1;

    let tx = src_x - x0 as f64;
    let ty = src_y - y0 as f64;
    let tz = src_z - z0 as f64;

    let c000 = sample_voxel_or_background(volume, background, x0, y0, z0) as f64;
    let c100 = sample_voxel_or_background(volume, background, x1, y0, z0) as f64;
    let c010 = sample_voxel_or_background(volume, background, x0, y1, z0) as f64;
    let c110 = sample_voxel_or_background(volume, background, x1, y1, z0) as f64;
    let c001 = sample_voxel_or_background(volume, background, x0, y0, z1) as f64;
    let c101 = sample_voxel_or_background(volume, background, x1, y0, z1) as f64;
    let c011 = sample_voxel_or_background(volume, background, x0, y1, z1) as f64;
    let c111 = sample_voxel_or_background(volume, background, x1, y1, z1) as f64;

    let c00 = c000 * (1.0 - tx) + c100 * tx;
    let c10 = c010 * (1.0 - tx) + c110 * tx;
    let c01 = c001 * (1.0 - tx) + c101 * tx;
    let c11 = c011 * (1.0 - tx) + c111 * tx;
    let c0 = c00 * (1.0 - ty) + c10 * ty;
    let c1 = c01 * (1.0 - ty) + c11 * ty;
    let value = c0 * (1.0 - tz) + c1 * tz;

    value.round() as i64
}

// AI-FUNC-SUMMARY: Snap a near-integer floating point value to its exact integer to avoid epsilon expansion; returns stabilized f64; side effects: None.
fn stabilize_bound(value: f64, eps: f64) -> f64 {
    let rounded = value.round();
    if (value - rounded).abs() <= eps {
        rounded
    } else {
        value
    }
}

// AI-FUNC-SUMMARY: Convert floating-point min/max bounds to inclusive integer [start, end] range with near-integer stabilization; returns (isize, isize); side effects: None.
fn float_bounds_to_inclusive_i64(min_v: f64, max_v: f64, eps: f64) -> (isize, isize) {
    let min_s = stabilize_bound(min_v, eps);
    let max_s = stabilize_bound(max_v, eps);
    let start = min_s.floor() as isize;
    let end = max_s.ceil() as isize;
    (start, end)
}

// AI-FUNC-SUMMARY:
// Purpose: Compute the ratio of non-background voxels on the boundary shell of a volume.
// Inputs: volume, background value, and shell thickness in voxels.
// Returns: Ratio in [0,1] where higher means stronger edge artifacts.
// Side effects: None.
fn boundary_non_bg_ratio(volume: &Volume3D, background: i64, thickness: usize) -> f64 {
    if thickness == 0 {
        return 0.0;
    }

    let mut total = 0usize;
    let mut non_bg = 0usize;

    for z in 0..volume.depth {
        for y in 0..volume.height {
            for x in 0..volume.width {
                let on_shell = x < thickness
                    || y < thickness
                    || z < thickness
                    || x + thickness >= volume.width
                    || y + thickness >= volume.height
                    || z + thickness >= volume.depth;
                if !on_shell {
                    continue;
                }
                total += 1;
                let idx = voxel_index(volume.width, volume.height, x, y, z);
                if volume.data[idx] != background {
                    non_bg += 1;
                }
            }
        }
    }

    if total == 0 {
        0.0
    } else {
        non_bg as f64 / total as f64
    }
}

// AI-FUNC-SUMMARY:
// Purpose: Infer how many border pixels to trim based on boundary artifact intensity.
// Inputs: rotated-cropped volume and background value.
// Returns: Suggested trim pixels in [0,2].
// Side effects: None.
fn infer_trim_pixels(volume: &Volume3D, background: i64) -> usize {
    let r1 = boundary_non_bg_ratio(volume, background, 1);
    let r2 = boundary_non_bg_ratio(volume, background, 2);

    if r1 > 0.08 && r2 > 0.04 {
        2
    } else if r1 > 0.03 {
        1
    } else {
        0
    }
}

// AI-FUNC-SUMMARY:
// Purpose: Resolve the effective edge trim pixel count from config (supports -1 for auto-detection).
// Inputs: config value (-1=auto, 0/1/2=explicit), current volume, and background value.
// Returns: Trim pixel count clamped to [0, min(2, volume_half_size)].
// Side effects: None.
fn resolve_trim_pixels(
    config_value: Option<i32>,
    volume: &Volume3D,
    background: i64,
) -> Result<usize> {
    let requested = match config_value.unwrap_or(0) {
        -1 => Ok(infer_trim_pixels(volume, background)),
        0 => Ok(0),
        1 => Ok(1),
        2 => Ok(2),
        other => Err(RustMsptError::InvalidConfig(format!(
            "Invalid edge_trim: {other}. Expected -1, 0, 1, or 2"
        ))),
    }?;

    let max_trim_w = volume.width.saturating_sub(1) / 2;
    let max_trim_h = volume.height.saturating_sub(1) / 2;
    let max_trim = max_trim_w.min(max_trim_h).min(2);

    Ok(requested.min(max_trim))
}

// AI-FUNC-SUMMARY:
// Purpose: Consume a volume and compact retained XY rows in its existing allocation.
// Inputs: owned source volume and trim pixels.
// Returns: Trimmed volume with unchanged depth and numeric type, or an invalid-shape error.
// Side effects: Moves retained rows toward the start and truncates the data without reallocating.
fn trim_volume_border(mut volume: Volume3D, trim: usize) -> Result<Volume3D> {
    if trim == 0 {
        return Ok(volume);
    }
    if trim > volume.width.saturating_sub(1) / 2 || trim > volume.height.saturating_sub(1) / 2 {
        return Err(RustMsptError::InvalidConfig(format!(
            "edge_trim={} is too large for output shape ({},{},{})",
            trim, volume.width, volume.height, volume.depth
        )));
    }
    let out_w = volume.width - 2 * trim;
    let out_h = volume.height - 2 * trim;
    for z in 0..volume.depth {
        for y in 0..out_h {
            let src = voxel_index(volume.width, volume.height, trim, y + trim, z);
            let dst = voxel_index(out_w, out_h, 0, y, z);
            volume.data.copy_within(src..src + out_w, dst);
        }
    }
    volume.data.truncate(out_w * out_h * volume.depth);
    volume.width = out_w;
    volume.height = out_h;
    Ok(volume)
}

// AI-FUNC-SUMMARY: Visit each boundary voxel once in z-major order without scanning interior voxels; the caller supplies a specialized counter, and collapsed dimensions do not duplicate edges/corners.
fn for_each_boundary_value(volume: &Volume3D, mut record: impl FnMut(i64)) {
    for z in 0..volume.depth {
        let slab = z * volume.width * volume.height;
        if z == 0 || z + 1 == volume.depth {
            for &value in &volume.data[slab..slab + volume.width * volume.height] { record(value); }
        } else {
            for y in 0..volume.height {
                let row = slab + y * volume.width;
                if y == 0 || y + 1 == volume.height {
                    for &value in &volume.data[row..row + volume.width] { record(value); }
                } else {
                    record(volume.data[row]);
                    if volume.width > 1 { record(volume.data[row + volume.width - 1]); }
                }
            }
        }
    }
}

// AI-FUNC-SUMMARY:
// Purpose: Detect the background value of a volume by finding the most frequent value on boundary voxels.
// Inputs: volume reference.
// Returns: The modal boundary voxel value as i64, choosing the smallest tied value.
// Notes: Bounded dense counters cover common 8/16-bit types; sparse spill preserves arbitrary i64 values and large types.
// Side effects: None.
fn detect_background_mode(volume: &Volume3D) -> i64 {
    let mut counts: HashMap<i64, usize> = HashMap::new();
    if volume.width == 0 || volume.height == 0 || volume.depth == 0 { return 0; }
    // Dense counters avoid per-voxel hashing for common integer image types.
    // Unexpected out-of-range values remain valid keys in the sparse spill map.
    let (base, bins) = match volume.numeric_type {
        VolumeNumericType::U8 => (0i64, 256),
        VolumeNumericType::I8 => (-128, 256),
        VolumeNumericType::U16 if volume.data.len() >= 65_536 => (0, 65_536),
        VolumeNumericType::I16 if volume.data.len() >= 65_536 => (-32_768, 65_536),
        _ => (0, 0),
    };
    if bins == 0 {
        for_each_boundary_value(volume, |value| { *counts.entry(value).or_insert(0usize) += 1; });
        return counts.into_iter().max_by(|a, b| a.1.cmp(&b.1).then_with(|| b.0.cmp(&a.0)))
            .map(|(value, _)| value).unwrap_or(0);
    }
    let mut dense = vec![0usize; bins];
    let (mut best_value, mut best_count) = (0i64, 0usize);
    let mut record = |value: i64| {
        let index = value.checked_sub(base).and_then(|n| usize::try_from(n).ok());
        let count = if let Some(index) = index.filter(|&n| n < dense.len()) {
            &mut dense[index]
        } else {
            counts.entry(value).or_insert(0usize)
        };
        *count += 1;
        if *count > best_count || (*count == best_count && value < best_value) {
            best_count = *count;
            best_value = value;
        }
    };
    for_each_boundary_value(volume, &mut record);
    // Equal boundary counts choose the smallest value, independent of hash iteration order.
    best_value

}

/// PCA result: (rotation, centroid, rotated min, rotated max, foreground voxel count).
type PcaEstimate = (Matrix3<f64>, Vector3<f64>, Vector3<f64>, Vector3<f64>, usize);

// AI-FUNC-SUMMARY:
// Purpose: Compute PCA rotation and foreground bounding box in the rotated coordinate frame.
// Inputs: volume and detected background value.
// Returns: Tuple of (rotation matrix, centroid, rotated min, rotated max, foreground voxel count).
// Side effects: None.
// Notes: One statistics pass (exact per-row integer moments merged with Chan's formula inside each fixed 65536-voxel block, blocks merged in ascending order) replaces the centroid and covariance passes; a second pass projects bounds. Worker count never changes the result. The frame comes from pca_frame (canonical basis for near-degenerate eigenspaces, right-handed). Returns error if no foreground voxels found.
fn estimate_pca_bbox(
    volume: &Volume3D,
    background: i64,
) -> Result<PcaEstimate> {
    let partials = foreground_row_blocks(volume, MomentState::default, |state, x0, y, z, row| {
        let mut count = 0u64;
        let mut sum = 0u128;
        let mut sum_sq = 0u128;
        for (x, &value) in row.iter().enumerate() {
            if value != background {
                let x = (x0 + x) as u128;
                count += 1;
                sum += x;
                sum_sq += x * x;
            }
        }
        if count > 0 {
            state.merge(&MomentState::from_row(count, sum, sum_sq, y, z));
        }
    });
    let stats = partials.into_iter().fold(MomentState::default(), |mut total, block| {
        total.merge(&block);
        total
    });
    if stats.count == 0 {
        return Err(RustMsptError::InvalidConfig(
            "No foreground voxels found after background detection".to_string(),
        ));
    }
    let count = stats.count as usize;
    let centroid = stats.mean;
    let rot = pca_frame(stats.m2 / stats.count as f64);
    let (min_v, max_v) = projected_bounds(volume, background, &rot, &centroid);
    Ok((rot, centroid, min_v, max_v, count))
}

/// Running count, mean and centered second-moment matrix of foreground coordinates.
#[derive(Clone, Copy, Debug, PartialEq)]
struct MomentState {
    count: u64,
    mean: Vector3<f64>,
    m2: Matrix3<f64>,
}

impl Default for MomentState {
    // AI-FUNC-SUMMARY: Return the empty moment state (count 0, zero mean and M2); side effects: None.
    fn default() -> Self {
        Self { count: 0, mean: Vector3::zeros(), m2: Matrix3::zeros() }
    }
}

impl MomentState {
    // AI-FUNC-SUMMARY: Build the exact moments of one foreground row segment from integer count, sum x and sum x^2 at fixed (y, z); only the xx entry of M2 is nonzero; side effects: None.
    fn from_row(count: u64, sum: u128, sum_sq: u128, y: usize, z: usize) -> Self {
        let n = count as f64;
        let mean_x = sum as f64 / n;
        let centered = sum_sq as f64 - (sum as f64) * mean_x;
        let mut m2 = Matrix3::zeros();
        m2[(0, 0)] = centered.max(0.0);
        Self { count, mean: Vector3::new(mean_x, y as f64, z as f64), m2 }
    }

    // AI-FUNC-SUMMARY: Merge another moment state into this one with Chan's parallel formula (mean shift and delta outer product weighted by na*nb/n); empty operands are no-ops; side effects: mutates self.
    fn merge(&mut self, other: &MomentState) {
        if other.count == 0 {
            return;
        }
        if self.count == 0 {
            *self = *other;
            return;
        }
        let na = self.count as f64;
        let nb = other.count as f64;
        let n = na + nb;
        let delta = other.mean - self.mean;
        self.mean += delta * (nb / n);
        self.m2 += other.m2 + delta * delta.transpose() * (na * nb / n);
        self.count += other.count;
    }
}

const PCA_DEGENERATE_REL_TOL: f64 = 1e-3;
/// Below this magnitude a principal axis is treated as perpendicular to its own scan axis, so the
/// diagonal cannot decide its sign and the largest-component rule does instead.
const PCA_SIGN_DIAGONAL_TOL: f64 = 1e-6;

// AI-FUNC-SUMMARY:
// Purpose: Turn a covariance matrix into a right-handed crop frame whose columns are principal axes in descending eigenvalue order.
// Inputs: symmetric covariance matrix.
// Returns: rotation matrix (columns = frame axes).
// Side effects: None.
// Notes: Adjacent sorted eigenvalues within PCA_DEGENERATE_REL_TOL of the largest magnitude form one eigenspace. A full 3-D eigenspace yields exactly the scan axes. A 2-D eigenspace's columns are the scan axes x, y, z (in that order) projected into it and Gram-Schmidt orthonormalized, accepting an axis only when its residual norm is >= 0.5 (enough axes always exist), so each column has a positive component along its source axis. A non-degenerate column k is sign-fixed so its component along scan axis k is positive - the frame closest to the scan axes, so an upright sample stays upright - because the eigen-solver's sign flips under last-bit covariance changes; only when that component is below PCA_SIGN_DIAGONAL_TOL does the largest-magnitude component (lowest axis on exact ties) decide. A negative determinant then flips the non-degenerate column with the smallest diagonal magnitude (highest index on ties), which gives up the least rotation; column 2 only if every column is degenerate. The earlier largest-component-only rule turned the repository CT 180 degrees and reversed its slice axis (PLAN.Performance.md §68).
fn pca_frame(cov: Matrix3<f64>) -> Matrix3<f64> {
    let eig = SymmetricEigen::new(cov);
    let mut order = [0usize, 1usize, 2usize];
    order.sort_by(|a, b| {
        eig.eigenvalues[*b]
            .partial_cmp(&eig.eigenvalues[*a])
            .unwrap_or(Ordering::Equal)
    });
    let values = order.map(|i| eig.eigenvalues[i]);
    let mut columns = order.map(|i| eig.eigenvectors.column(i).into_owned());
    let scale = values.iter().fold(0.0f64, |m, v| m.max(v.abs()));
    let mut free = [false; 3];
    let mut start = 0;
    while start < 3 {
        let mut end = start + 1;
        while end < 3 && (values[end - 1] - values[end]).abs() <= PCA_DEGENERATE_REL_TOL * scale {
            end += 1;
        }
        if end - start == 1 {
            free[start] = true;
            let c = &mut columns[start];
            let lead = if c[start].abs() >= PCA_SIGN_DIAGONAL_TOL {
                start
            } else {
                (0..3).fold(0, |best, i| if c[i].abs() > c[best].abs() { i } else { best })
            };
            if c[lead] < 0.0 {
                *c = -*c;
            }
        } else if end - start == 3 {
            columns = [Vector3::x(), Vector3::y(), Vector3::z()];
        } else {
            let span = &columns[start..end];
            let project = |v: &Vector3<f64>| span.iter().fold(Vector3::zeros(), |acc, u| acc + u * u.dot(v));
            let mut chosen: Vec<Vector3<f64>> = Vec::with_capacity(end - start);
            for axis in 0..3 {
                if chosen.len() == end - start {
                    break;
                }
                let mut residual = project(&Vector3::ith(axis, 1.0));
                for c in &chosen {
                    residual -= c * c.dot(&residual);
                }
                let norm = residual.norm();
                if norm >= 0.5 {
                    chosen.push(residual / norm);
                }
            }
            if chosen.len() == end - start {
                for (slot, c) in columns[start..end].iter_mut().zip(chosen) {
                    *slot = c;
                }
            }
        }
        start = end;
    }
    let mut rot = Matrix3::from_columns(&columns);
    if rot.determinant() < 0.0 {
        let flip = (0..3)
            .filter(|&k| free[k])
            .fold(None, |best: Option<usize>, k| match best {
                Some(b) if rot[(b, b)].abs() < rot[(k, k)].abs() => Some(b),
                _ => Some(k),
            })
            .unwrap_or(2);
        let c = -rot.column(flip).into_owned();
        rot.set_column(flip, &c);
    }
    rot
}

// AI-FUNC-SUMMARY: Project every foreground voxel into the rotated frame about the centroid over fixed 65536-voxel blocks and return the (min, max) corners; min/max merges are order-independent; side effects: None.
fn projected_bounds(volume: &Volume3D, background: i64, rot: &Matrix3<f64>, centroid: &Vector3<f64>) -> (Vector3<f64>, Vector3<f64>) {
    let inv = rot.transpose();
    let empty_bounds = || (Vector3::repeat(f64::INFINITY), Vector3::repeat(f64::NEG_INFINITY));
    let partials = foreground_blocks(volume, background, empty_bounds, |(min, max), p| {
        let q = inv * (p - centroid);
        for axis in 0..3 { min[axis] = min[axis].min(q[axis]); max[axis] = max[axis].max(q[axis]); }
    });
    partials.into_iter().fold(empty_bounds(), |(mut min, mut max), (lo, hi)| {
        for axis in 0..3 { min[axis] = min[axis].min(lo[axis]); max[axis] = max[axis].max(hi[axis]); }
        (min, max)
    })
}

#[cfg(test)]
// AI-FUNC-SUMMARY: Previous fixed-block three-pass PCA (centroid pass, centered covariance pass, bounds pass) retained as the oracle for the one-pass moment merge; uses the same pca_frame.
fn estimate_pca_bbox_three_pass(
    volume: &Volume3D,
    background: i64,
) -> Result<PcaEstimate> {
    let partials = foreground_blocks(volume, background, || (0usize, Vector3::zeros()), |state, p| {
        state.0 += 1;
        state.1 += p;
    });
    let (count, sum) = partials.into_iter().fold((0usize, Vector3::zeros()), |(count, sum), (n, value)| (count + n, sum + value));
    if count == 0 {
        return Err(RustMsptError::InvalidConfig(
            "No foreground voxels found after background detection".to_string(),
        ));
    }
    let centroid = sum / count as f64;
    let partials = foreground_blocks(volume, background, Matrix3::zeros, |cov, p| {
        let d = p - centroid;
        *cov += d * d.transpose();
    });
    let cov = partials.into_iter().fold(Matrix3::zeros(), |sum, value| sum + value) / count as f64;
    let rot = pca_frame(cov);
    let (min_v, max_v) = projected_bounds(volume, background, &rot, &centroid);
    Ok((rot, centroid, min_v, max_v, count))
}

// AI-FUNC-SUMMARY: Scan fixed 65536-voxel blocks in parallel and hand each contiguous row segment (start x, y, z, values) to the accumulator in source order, collecting partials by block index regardless of worker count.
fn foreground_row_blocks<T: Send>(
    volume: &Volume3D,
    initial: impl Fn() -> T + Sync + Send,
    accumulate: impl Fn(&mut T, usize, usize, usize, &[i64]) + Sync + Send,
) -> Vec<T> {
    const BLOCK: usize = 65536;
    let scan = |(block, values): (usize, &[i64])| {
        let mut result = initial();
        let mut offset = 0;
        while offset < values.len() {
            let index = block * BLOCK + offset;
            let x0 = index % volume.width;
            let y = (index / volume.width) % volume.height;
            let z = index / (volume.width * volume.height);
            let count = (volume.width - x0).min(values.len() - offset);
            accumulate(&mut result, x0, y, z, &values[offset..offset + count]);
            offset += count;
        }
        result
    };
    if volume.data.len() < 1_048_576 || rayon::current_num_threads() == 1 {
        volume.data.chunks(BLOCK).enumerate().map(scan).collect()
    } else {
        let tasks = volume.data.len().div_ceil(1_048_576).min(rayon::current_num_threads());
        let blocks_per_task = volume.data.len().div_ceil(BLOCK).div_ceil(tasks);
        volume.data.par_chunks(BLOCK).enumerate().with_min_len(blocks_per_task).map(scan).collect()
    }
}

// AI-FUNC-SUMMARY: Scan fixed 65536-voxel blocks in parallel, visiting foreground positions in source order and collecting partials by block index regardless of worker count.
fn foreground_blocks<T: Send>(
    volume: &Volume3D,
    background: i64,
    initial: impl Fn() -> T + Sync + Send,
    accumulate: impl Fn(&mut T, Vector3<f64>) + Sync + Send,
) -> Vec<T> {
    foreground_row_blocks(volume, initial, |result, x0, y, z, row| {
        for (x, &value) in row.iter().enumerate() {
            if value != background { accumulate(result, Vector3::new((x0 + x) as f64, y as f64, z as f64)); }
        }
    })
}

#[cfg(test)]
// AI-FUNC-SUMMARY: Original serial three-pass PCA retained as a numerical oracle for fixed-block reductions.
fn estimate_pca_bbox_serial(
    volume: &Volume3D,
    background: i64,
) -> Result<PcaEstimate> {
    let mut count: usize = 0;
    let mut sum = Vector3::new(0.0, 0.0, 0.0);

    for z in 0..volume.depth {
        for y in 0..volume.height {
            for x in 0..volume.width {
                let idx = voxel_index(volume.width, volume.height, x, y, z);
                if volume.data[idx] == background {
                    continue;
                }
                count += 1;
                sum += Vector3::new(x as f64, y as f64, z as f64);
            }
        }
    }

    if count == 0 {
        return Err(RustMsptError::InvalidConfig(
            "No foreground voxels found after background detection".to_string(),
        ));
    }

    let centroid = sum / count as f64;
    let mut cov = Matrix3::zeros();

    for z in 0..volume.depth {
        for y in 0..volume.height {
            for x in 0..volume.width {
                let idx = voxel_index(volume.width, volume.height, x, y, z);
                if volume.data[idx] == background {
                    continue;
                }
                let d = Vector3::new(x as f64, y as f64, z as f64) - centroid;
                cov[(0, 0)] += d.x * d.x;
                cov[(0, 1)] += d.x * d.y;
                cov[(0, 2)] += d.x * d.z;
                cov[(1, 0)] += d.y * d.x;
                cov[(1, 1)] += d.y * d.y;
                cov[(1, 2)] += d.y * d.z;
                cov[(2, 0)] += d.z * d.x;
                cov[(2, 1)] += d.z * d.y;
                cov[(2, 2)] += d.z * d.z;
            }
        }
    }
    cov /= count as f64;

    let eig = SymmetricEigen::new(cov);
    let mut order = [0usize, 1usize, 2usize];
    order.sort_by(|a, b| {
        eig.eigenvalues[*b]
            .partial_cmp(&eig.eigenvalues[*a])
            .unwrap_or(Ordering::Equal)
    });

    let mut rot = Matrix3::from_columns(&[
        eig.eigenvectors.column(order[0]).into_owned(),
        eig.eigenvectors.column(order[1]).into_owned(),
        eig.eigenvectors.column(order[2]).into_owned(),
    ]);

    if rot.determinant() < 0.0 {
        let c2 = -rot.column(2).into_owned();
        rot.set_column(2, &c2);
    }

    let inv = rot.transpose();
    let mut min_v = Vector3::new(f64::INFINITY, f64::INFINITY, f64::INFINITY);
    let mut max_v = Vector3::new(f64::NEG_INFINITY, f64::NEG_INFINITY, f64::NEG_INFINITY);

    for z in 0..volume.depth {
        for y in 0..volume.height {
            for x in 0..volume.width {
                let idx = voxel_index(volume.width, volume.height, x, y, z);
                if volume.data[idx] == background {
                    continue;
                }
                let p = Vector3::new(x as f64, y as f64, z as f64);
                let q = inv * (p - centroid);
                min_v.x = min_v.x.min(q.x);
                min_v.y = min_v.y.min(q.y);
                min_v.z = min_v.z.min(q.z);
                max_v.x = max_v.x.max(q.x);
                max_v.y = max_v.y.max(q.y);
                max_v.z = max_v.z.max(q.z);
            }
        }
    }

    Ok((rot, centroid, min_v, max_v, count))
}

// AI-FUNC-SUMMARY:
// Purpose: Rotate the full volume and crop to the rotated foreground bounding box, producing a new axis-aligned volume.
// Inputs: source volume, background value, rotation matrix, centroid, rotated min/max bounds, and interpolation mode.
// Returns: Cropped, axis-aligned Volume3D.
// Side effects: None.
// Notes: Parallelizes over z-slices via rayon. Maps each output voxel back to source coordinates using the inverse rotation.
fn rotate_and_crop(
    volume: &Volume3D,
    background: i64,
    rot: &Matrix3<f64>,
    centroid: &Vector3<f64>,
    min_v: &Vector3<f64>,
    max_v: &Vector3<f64>,
    interpolation_mode: InterpolationMode,
) -> Volume3D {
    let eps = 1e-3;
    let (x0, x1) = float_bounds_to_inclusive_i64(min_v.x, max_v.x, eps);
    let (y0, y1) = float_bounds_to_inclusive_i64(min_v.y, max_v.y, eps);
    let (z0, z1) = float_bounds_to_inclusive_i64(min_v.z, max_v.z, eps);

    let out_w = (x1 - x0 + 1).max(1) as usize;
    let out_h = (y1 - y0 + 1).max(1) as usize;
    let out_d = (z1 - z0 + 1).max(1) as usize;

    let mut data = vec![background; out_w * out_h * out_d];
    // Preserve cheap slice loops when depth already exposes enough parallelism.
    // Otherwise use row-aligned tiles to make shallow volumes share the pool.
    if out_d >= rayon::current_num_threads() {
        data.par_chunks_mut(out_w * out_h)
            .enumerate()
            .for_each(|(z, slab)| {
                let z_coord = z0 as f64 + z as f64;
                for y in 0..out_h {
                    let y_coord = y0 as f64 + y as f64;
                    for x in 0..out_w {
                        let src =
                            rot * Vector3::new(x0 as f64 + x as f64, y_coord, z_coord) + centroid;
                        slab[y * out_w + x] = match interpolation_mode {
                            InterpolationMode::Nearest => {
                                sample_nearest(volume, background, src.x, src.y, src.z)
                            }
                            InterpolationMode::Trilinear => {
                                sample_trilinear(volume, background, src.x, src.y, src.z)
                            }
                        };
                    }
                }
            });
    } else {
        let rows_per_task = (4096 / out_w).max(1).min(out_h);
        data.par_chunks_mut(out_w * rows_per_task)
            .enumerate()
            .for_each(|(tile, values)| {
                for (row, values) in values.chunks_mut(out_w).enumerate() {
                    let row_index = tile * rows_per_task + row;
                    let y_coord = y0 as f64 + (row_index % out_h) as f64;
                    let z_coord = z0 as f64 + (row_index / out_h) as f64;
                    for (x, value) in values.iter_mut().enumerate() {
                        let local = Vector3::new(x0 as f64 + x as f64, y_coord, z_coord);
                        let src = rot * local + centroid;
                        *value = match interpolation_mode {
                            InterpolationMode::Nearest => {
                                sample_nearest(volume, background, src.x, src.y, src.z)
                            }
                            InterpolationMode::Trilinear => {
                                sample_trilinear(volume, background, src.x, src.y, src.z)
                            }
                        };
                    }
                }
            });
    }

    Volume3D {
        width: out_w,
        height: out_h,
        depth: out_d,
        data,
        numeric_type: volume.numeric_type,
    }
}

const CROP_GPU_PARAMS_BYTES: u64 = 160;
const CROP_GPU_FIXED_BYTES: u64 = CROP_GPU_PARAMS_BYTES + 4 + 4;
type CropTileShape = fn(usize, [usize; 3]) -> [usize; 3];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct CropSourceBlock {
    origin: [usize; 3],
    dims: [usize; 3],
}

impl CropSourceBlock {
    // AI-FUNC-SUMMARY: The block grown by `pad` voxels on every side and clamped to the source; returns the new block; side effects: none.
    #[cfg(feature = "gpu")]
    fn padded(&self, pad: usize, src_dims: [usize; 3]) -> CropSourceBlock {
        let mut origin = [0usize; 3];
        let mut dims = [0usize; 3];
        for axis in 0..3 {
            let lo = self.origin[axis].saturating_sub(pad);
            let hi = (self.origin[axis] + self.dims[axis]).saturating_add(pad).min(src_dims[axis]);
            origin[axis] = lo;
            dims[axis] = hi.saturating_sub(lo);
        }
        CropSourceBlock { origin, dims }
    }

    // AI-FUNC-SUMMARY: Count block voxels with checked multiplication; returns None on overflow; side effects: none.
    fn voxels(&self) -> Option<u64> {
        self.dims
            .iter()
            .try_fold(1u64, |acc, &d| acc.checked_mul(d as u64))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct CropTilePlan {
    out_dims: [usize; 3],
    tile_dims: [usize; 3],
    tiles: usize,
    max_block_voxels: u64,
    max_tile_voxels: u64,
    peak_bytes: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct CropTilePlanError {
    needed: Option<u64>,
    reason: String,
}

// AI-FUNC-SUMMARY:
// Purpose: Find the source sub-block a GPU output tile can read, including interpolation halo and an f32 error margin.
// Inputs: full source dims, rotation/centroid (src = rot * local + centroid), integer output origin, inclusive whole-output tile bounds.
// Returns: Clamped block origin/dims (a zero dim means no in-volume sample is reachable) or an error for nonfinite coordinates.
// Side effects: None.
// Notes: The affine image of the tile box has its AABB at the 8 corners. The halo spans floor(min)..floor(max)+1, which covers both
//        trilinear neighbours and nearest rounding; the margin bounds the shader's f32 rounding of rot/centroid/origin and products.
fn crop_tile_source_block(
    src_dims: [usize; 3],
    rot: &Matrix3<f64>,
    centroid: &Vector3<f64>,
    origin: [isize; 3],
    lo: [usize; 3],
    hi: [usize; 3],
) -> std::result::Result<CropSourceBlock, String> {
    let bounds = |axis: usize| {
        let a = origin[axis] as f64 + lo[axis] as f64;
        let b = origin[axis] as f64 + hi[axis] as f64;
        (a, b)
    };
    let local = [bounds(0), bounds(1), bounds(2)];
    let mut block = CropSourceBlock {
        origin: [0; 3],
        dims: [0; 3],
    };
    for (i, &src_dim) in src_dims.iter().enumerate() {
        let mut min = centroid[i];
        let mut max = centroid[i];
        let mut scale = centroid[i].abs();
        for (j, &(a, b)) in local.iter().enumerate() {
            let r = rot[(i, j)];
            min += (r * a).min(r * b);
            max += (r * a).max(r * b);
            scale += r.abs() * (a.abs().max(b.abs()) + 1.0);
        }
        let margin = 16.0 * f64::from(f32::EPSILON) * (scale + 1.0) + 1e-6;
        let (low, high) = ((min - margin).floor(), (max + margin).floor() + 1.0);
        if !low.is_finite() || !high.is_finite() {
            return Err("crop GPU tile source bounds are nonfinite".into());
        }
        let last = src_dim as f64 - 1.0;
        if high < 0.0 || low > last {
            return Ok(CropSourceBlock {
                origin: [0; 3],
                dims: [0; 3],
            });
        }
        let start = low.max(0.0) as usize;
        let end = high.min(last) as usize;
        block.origin[i] = start;
        block.dims[i] = end - start + 1;
    }
    Ok(block)
}

// AI-FUNC-SUMMARY: Peak logical GPU bytes for retained max source block (buffer plus queued upload copy) and max output tile (output plus staging), plus params and guard words; returns None on overflow.
fn crop_gpu_peak_bytes(max_block_voxels: u64, max_tile_voxels: u64) -> Option<u64> {
    let block = max_block_voxels.max(1).checked_mul(4)?.checked_mul(2)?;
    let tile = max_tile_voxels.max(1).checked_mul(4)?;
    block
        .checked_add(tile)?
        .checked_add(tile.checked_add(4)?)?
        .checked_add(CROP_GPU_FIXED_BYTES)
}

// AI-FUNC-SUMMARY: Visit whole-output tiles in z, y, x order as inclusive (lo, hi) bounds; stops and returns the first visitor error; side effects: runs the visitor.
fn for_each_crop_tile<E>(
    out_dims: [usize; 3],
    tile_dims: [usize; 3],
    mut visit: impl FnMut([usize; 3], [usize; 3]) -> std::result::Result<(), E>,
) -> std::result::Result<(), E> {
    for z in (0..out_dims[2]).step_by(tile_dims[2]) {
        for y in (0..out_dims[1]).step_by(tile_dims[1]) {
            for x in (0..out_dims[0]).step_by(tile_dims[0]) {
                let lo = [x, y, z];
                let hi = [
                    (x + tile_dims[0]).min(out_dims[0]) - 1,
                    (y + tile_dims[1]).min(out_dims[1]) - 1,
                    (z + tile_dims[2]).min(out_dims[2]) - 1,
                ];
                visit(lo, hi)?;
            }
        }
    }
    Ok(())
}

// AI-FUNC-SUMMARY:
// Purpose: Evaluate one tile shape against the logical budget and per-buffer device limit.
// Inputs: source/output dims, transform, tile dims, optional byte budget and optional single-buffer limit.
// Returns: Ok(plan) when every tile's retained maxima fit, or Err with the smallest known requirement at the first failing tile.
// Side effects: None.
#[allow(clippy::too_many_arguments)]
fn evaluate_crop_tiling(
    src_dims: [usize; 3],
    rot: &Matrix3<f64>,
    centroid: &Vector3<f64>,
    origin: [isize; 3],
    out_dims: [usize; 3],
    tile_dims: [usize; 3],
    budget: Option<u64>,
    buffer_limit: Option<u64>,
) -> std::result::Result<CropTilePlan, CropTilePlanError> {
    let refuse = |needed: Option<u64>, reason: &str| CropTilePlanError {
        needed,
        reason: reason.to_string(),
    };
    let mut plan = CropTilePlan {
        out_dims,
        tile_dims,
        tiles: 0,
        max_block_voxels: 0,
        max_tile_voxels: 0,
        peak_bytes: 0,
    };
    for_each_crop_tile(out_dims, tile_dims, |lo, hi| {
        let block = crop_tile_source_block(src_dims, rot, centroid, origin, lo, hi)
            .map_err(|reason| refuse(None, &reason))?;
        let block_voxels = block
            .voxels()
            .ok_or_else(|| refuse(None, "crop GPU source block size overflows"))?;
        let tile_voxels = (0..3)
            .try_fold(1u64, |acc, axis| acc.checked_mul((hi[axis] - lo[axis] + 1) as u64))
            .ok_or_else(|| refuse(None, "crop GPU tile size overflows"))?;
        let max_block = plan.max_block_voxels.max(block_voxels);
        let max_tile = plan.max_tile_voxels.max(tile_voxels);
        let peak = crop_gpu_peak_bytes(max_block, max_tile);
        let largest_buffer = max_block.max(max_tile).checked_mul(4);
        if max_block > u64::from(u32::MAX) || max_tile > u64::from(u32::MAX) {
            return Err(refuse(peak, "crop GPU tile exceeds u32 voxel indexing"));
        }
        if let (Some(limit), Some(bytes)) = (buffer_limit, largest_buffer) {
            if bytes > limit {
                return Err(refuse(peak, "crop GPU tile exceeds device buffer limits"));
            }
        }
        let peak = peak.ok_or_else(|| refuse(None, "crop GPU tile bytes overflow"))?;
        if budget.is_some_and(|b| peak > b) {
            return Err(refuse(Some(peak), "crop GPU tile exceeds the logical GPU budget"));
        }
        plan.max_block_voxels = max_block;
        plan.max_tile_voxels = max_tile;
        plan.peak_bytes = peak;
        plan.tiles += 1;
        Ok(())
    })?;
    Ok(plan)
}

// AI-FUNC-SUMMARY:
// Purpose: Choose the largest GPU output tiling that fits the logical budget and device buffer limit.
// Inputs: source dims, transform, integer output origin, output dims, optional byte budget, optional single-buffer limit.
// Returns: A plan (one tile when the whole output fits) or an error carrying a lower bound on the minimal tile requirement.
// Side effects: None.
// Notes: Prefers z slabs of full xy extent, then y row groups of one slice, then x runs of one row; within a level it binary-searches
//        the extent, and every returned plan was evaluated over all of its tiles, so it fits even if cost is not monotone.
fn plan_crop_gpu_tiles(
    src_dims: [usize; 3],
    rot: &Matrix3<f64>,
    centroid: &Vector3<f64>,
    origin: [isize; 3],
    out_dims: [usize; 3],
    budget: Option<u64>,
    buffer_limit: Option<u64>,
) -> std::result::Result<CropTilePlan, CropTilePlanError> {
    if out_dims.contains(&0) || src_dims.contains(&0) {
        return Err(CropTilePlanError {
            needed: None,
            reason: "crop GPU dimensions must be positive".into(),
        });
    }
    let evaluate = |tile: [usize; 3]| {
        evaluate_crop_tiling(src_dims, rot, centroid, origin, out_dims, tile, budget, buffer_limit)
    };
    let [w, h, d] = out_dims;
    let levels: [(usize, CropTileShape); 3] = [
        (2, |t, [w, h, _]| [w, h, t]),
        (1, |t, [w, _, _]| [w, t, 1]),
        (0, |t, _| [t, 1, 1]),
    ];
    let mut last_error = None;
    for (axis, shape) in levels {
        let extent = [w, h, d][axis];
        if let Ok(plan) = evaluate(shape(extent, out_dims)) {
            return Ok(plan);
        }
        let mut best = match evaluate(shape(1, out_dims)) {
            Ok(plan) => plan,
            Err(error) => {
                last_error = Some(error);
                continue;
            }
        };
        let (mut lo, mut hi) = (1usize, extent - 1);
        while lo < hi {
            let mid = lo + (hi - lo).div_ceil(2);
            match evaluate(shape(mid, out_dims)) {
                Ok(plan) => {
                    best = plan;
                    lo = mid;
                }
                Err(_) => hi = mid - 1,
            }
        }
        return Ok(best);
    }
    Err(last_error.unwrap_or(CropTilePlanError {
        needed: None,
        reason: "crop GPU has no feasible tile".into(),
    }))
}

// AI-FUNC-SUMMARY:
// Purpose: GPU rotate-and-crop executed as budget-planned output tiles, each uploading only its halo-expanded source block.
// Inputs: same as rotate_and_crop plus an optional logical GPU byte budget.
// Returns: Ok((Volume3D, tile count)) or a GPU/planning error string (including "no single minimal tile fits").
// Side effects: Initializes the GPU pipeline, pre-sizes buffers to the plan maxima, dispatches one transform per tile, prints timing.
// Notes: Tiles reproduce the single-dispatch arithmetic exactly (absolute output coordinates, full-source bounds tests); a runtime halo
//        guard catches any out-of-block access; the tile is then rerun with its block grown (1, 2, 4... voxels per side) while
//        the grown block fits the budget, and only then is it an error. Only available with feature "gpu".
#[cfg(feature = "gpu")]
#[allow(clippy::too_many_arguments)]
fn rotate_and_crop_gpu(
    volume: &Volume3D,
    background: i64,
    rot: &Matrix3<f64>,
    centroid: &Vector3<f64>,
    min_v: &Vector3<f64>,
    max_v: &Vector3<f64>,
    interpolation_mode: InterpolationMode,
    budget: Option<u64>,
) -> std::result::Result<(Volume3D, usize), String> {
    rotate_and_crop_gpu_with(volume, background, rot, centroid, min_v, max_v, interpolation_mode, budget, 0)
        .map(|(volume, tiles, _)| (volume, tiles))
}

// AI-FUNC-SUMMARY: rotate_and_crop_gpu with `first_block_shrink` voxels removed from the high x side of every tile's first source block; returns the volume, tile count and halo-retry count; side effects: as rotate_and_crop_gpu.
// Notes: Production passes 0. A positive value makes the planned blocks deliberately too small so tests can drive
// the halo-guard retry path, which the planner's conservative margin otherwise never reaches.
#[cfg(feature = "gpu")]
#[allow(clippy::too_many_arguments)]
fn rotate_and_crop_gpu_with(
    volume: &Volume3D,
    background: i64,
    rot: &Matrix3<f64>,
    centroid: &Vector3<f64>,
    min_v: &Vector3<f64>,
    max_v: &Vector3<f64>,
    interpolation_mode: InterpolationMode,
    budget: Option<u64>,
    first_block_shrink: usize,
) -> std::result::Result<(Volume3D, usize, usize), String> {
    let eps = 1e-3;
    let (x0, x1) = float_bounds_to_inclusive_i64(min_v.x, max_v.x, eps);
    let (y0, y1) = float_bounds_to_inclusive_i64(min_v.y, max_v.y, eps);
    let (z0, z1) = float_bounds_to_inclusive_i64(min_v.z, max_v.z, eps);

    let dimension = |lo: isize, hi: isize| {
        hi.checked_sub(lo)
            .and_then(|n| n.checked_add(1))
            .and_then(|n| u32::try_from(n.max(1)).ok())
            .ok_or("crop GPU dimension exceeds u32")
    };
    let out_dims = [dimension(x0, x1)?, dimension(y0, y1)?, dimension(z0, z1)?];
    let src_dims = [volume.width, volume.height, volume.depth];
    let src_dims_u32 = src_dims.map(u32::try_from);
    let [src_w, src_h, src_d] = src_dims_u32;
    let source_dims = [
        src_w.map_err(|_| "source width exceeds u32")?,
        src_h.map_err(|_| "source height exceeds u32")?,
        src_d.map_err(|_| "source depth exceeds u32")?,
    ];

    if !gpu_crop_values_supported(volume, background, interpolation_mode) {
        return Err(
            "crop GPU cannot preserve input integers for the selected interpolation".into(),
        );
    }
    let t0 = std::time::Instant::now();

    let mut pipeline = crate::gpu::volume_transform::GpuVolumeTransformPipeline::new()?;
    let limits = pipeline.device_limits();
    let buffer_limit = limits
        .max_buffer_size
        .min(u64::from(limits.max_storage_buffer_binding_size));
    let origin_i = [x0, y0, z0];
    let out_usize = out_dims.map(|d| d as usize);
    let plan = plan_crop_gpu_tiles(
        src_dims,
        rot,
        centroid,
        origin_i,
        out_usize,
        budget,
        Some(buffer_limit),
    )
    .map_err(|error| match error.needed {
        Some(needed) => format!(
            "{}; no single minimal tile fits (needs at least {needed} bytes)",
            error.reason
        ),
        None => error.reason,
    })?;
    let block_bytes = plan.max_block_voxels.checked_mul(4).ok_or("crop GPU block bytes overflow")?;
    let tile_bytes = plan.max_tile_voxels.checked_mul(4).ok_or("crop GPU tile bytes overflow")?;
    pipeline.reserve_capacity(block_bytes, tile_bytes)?;

    let bg_i32 = i32::try_from(background).map_err(|_| "crop GPU background exceeds i32")?;
    let interp = match interpolation_mode {
        InterpolationMode::Nearest => 0u32,
        InterpolationMode::Trilinear => 1u32,
    };
    let origin = Vector3::new(x0 as f64, y0 as f64, z0 as f64);
    let out_total = out_usize
        .iter()
        .try_fold(1usize, |acc, &d| acc.checked_mul(d))
        .ok_or("crop GPU output size overflows")?;
    let mut data = vec![background; out_total];
    let (out_w, out_h) = (out_usize[0], out_usize[1]);
    let mut block_values: Vec<i32> = Vec::new();
    let mut halo_retries = 0usize;
    for_each_crop_tile(out_usize, plan.tile_dims, |lo, hi| {
        let planned = crop_tile_source_block(src_dims, rot, centroid, origin_i, lo, hi)?;
        let as_u32 = |v: usize| u32::try_from(v).map_err(|_| "crop GPU tile index exceeds u32");
        let tile_dims = [
            as_u32(hi[0] - lo[0] + 1)?,
            as_u32(hi[1] - lo[1] + 1)?,
            as_u32(hi[2] - lo[2] + 1)?,
        ];
        // A tripped halo guard means the shader wanted a source voxel the planned block left out. That
        // is a margin that proved too small, not a reason to give up the GPU: grow the block (1, 2, 4...
        // voxels per side) and run the tile again, as long as the grown block still fits the budget the
        // plan was made for. The retry recomputes the same tile, so its values are what they would have been.
        let mut pad = 0usize;
        let values = loop {
            let mut block = planned.padded(pad, src_dims);
            if pad == 0 && first_block_shrink > 0 {
                block.dims[0] = block.dims[0].saturating_sub(first_block_shrink).max(1);
            }
            block_values.clear();
            if !block.dims.contains(&0) {
                let [bx, by, bz] = block.origin;
                let [bw, bh, bd] = block.dims;
                for z in bz..bz + bd {
                    for y in by..by + bh {
                        let row = voxel_index(volume.width, volume.height, bx, y, z);
                        for &value in &volume.data[row..row + bw] {
                            block_values.push(
                                i32::try_from(value).map_err(|_| "crop GPU input exceeds i32")?,
                            );
                        }
                    }
                }
            }
            let tile = crate::gpu::volume_transform::TransformTile {
                block: &block_values,
                block_origin: [as_u32(block.origin[0])?, as_u32(block.origin[1])?, as_u32(block.origin[2])?],
                block_dims: [as_u32(block.dims[0])?, as_u32(block.dims[1])?, as_u32(block.dims[2])?],
                source_dims,
                tile_offset: [as_u32(lo[0])?, as_u32(lo[1])?, as_u32(lo[2])?],
                tile_dims,
            };
            match pipeline.transform_tile(&tile, bg_i32, rot, centroid, &origin, interp) {
                Err(error) if error.contains(crate::gpu::volume_transform::HALO_GUARD_ERROR) => {
                    let next = (pad * 2).max(1);
                    let grown = planned.padded(next, src_dims);
                    let whole = block.dims == src_dims;
                    let fits = grown.voxels().is_some_and(|voxels| {
                        crop_gpu_peak_bytes(voxels, plan.max_tile_voxels)
                            .is_some_and(|peak| budget.is_none_or(|limit| peak <= limit))
                            && voxels.saturating_mul(4) <= buffer_limit
                    });
                    if whole || !fits {
                        return Err(error);
                    }
                    let grown_bytes = grown.voxels().unwrap_or(u64::MAX).saturating_mul(4);
                    if grown_bytes > block_bytes {
                        pipeline.reserve_capacity(grown_bytes, tile_bytes)?;
                    }
                    halo_retries += 1;
                    pad = next;
                }
                other => break other?,
            }
        };
        let row_len = tile_dims[0] as usize;
        for (row_index, row) in values.chunks_exact(row_len).enumerate() {
            let y = lo[1] + row_index % tile_dims[1] as usize;
            let z = lo[2] + row_index / tile_dims[1] as usize;
            let start = voxel_index(out_w, out_h, lo[0], y, z);
            for (slot, &value) in data[start..start + row_len].iter_mut().zip(row) {
                *slot = i64::from(value);
            }
        }
        Ok::<(), String>(())
    })?;

    let elapsed = t0.elapsed().as_secs_f64();
    println!(
        "[Info] GPU volume transform: {:.3}s, {}x{}x{} -> {}x{}x{}, tiles={} tile={}x{}x{} peak_bytes={} halo_retries={}",
        elapsed,
        volume.width,
        volume.height,
        volume.depth,
        out_dims[0],
        out_dims[1],
        out_dims[2],
        plan.tiles,
        plan.tile_dims[0],
        plan.tile_dims[1],
        plan.tile_dims[2],
        plan.peak_bytes,
        halo_retries
    );

    Ok((
        Volume3D {
            width: out_usize[0],
            height: out_usize[1],
            depth: out_usize[2],
            data,
            numeric_type: volume.numeric_type,
        },
        plan.tiles,
        halo_retries,
    ))
}

impl Pipeline for CropPipeline {
    // AI-FUNC-SUMMARY:
    // Purpose: Execute the crop pipeline: load CT volume, detect background, PCA-align, rotate and crop, optionally trim border artifacts, and save TIFF output.
    // Inputs: CropConfig with input type/path, output path, interpolation mode, and edge trim setting.
    // Returns: Ok(()) or error.
    // Side effects: Reads volume from disk; writes cropped TIFF output to disk; prints diagnostics to stdout.
    fn run(&self) -> Result<()> {
        let available = std::thread::available_parallelism()
            .map(|n| n.get())
            .unwrap_or(1);
        let workers = match self.config.cpu_max.unwrap_or(-1) {
            -1 => available,
            n => (n.max(1) as usize).min(available),
        };
        let pool = rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .map_err(|e| {
                RustMsptError::InvalidConfig(format!("Failed to build crop thread pool: {e}"))
            })?;
        pool.install(|| self.run_in_pool())
    }
}

impl CropPipeline {
    // AI-FUNC-SUMMARY: Execute all crop stages under one configured pool, preserving interpolation/type/backend/fallback policy; report completed-stage wall times through StageTimer, with backend selection and GPU initialization included in transform_and_backend, then workers and peak RSS.
    fn run_in_pool(&self) -> Result<()> {
        let requested = crate::compute::policy::configured_mode(&self.config.acceleration)?;
        let mut timer = crate::pipeline::timing::StageTimer::start("crop");
        let input_volume = load_input_volume(&self.config)?;
        timer.stage("load");
        println!(
            "[Info] Crop input loaded: shape=({},{},{})",
            input_volume.width, input_volume.height, input_volume.depth
        );

        timer.restart();
        let background = detect_background_mode(&input_volume);
        timer.stage("background");
        println!("[Info] Background value detected: {}", background);

        let interpolation_mode = parse_interpolation_mode(self.config.interpolation.as_deref())?;
        println!("[Info] Interpolation mode: {:?}", interpolation_mode);

        timer.restart();
        let (rot, centroid, min_v, max_v, fg_count) = estimate_pca_bbox(&input_volume, background)?;
        timer.stage("pca");
        println!("[Info] Foreground voxels: {}", fg_count);
        println!(
            "[Info] Rotated bbox: min=({:.3},{:.3},{:.3}) max=({:.3},{:.3},{:.3})",
            min_v.x, min_v.y, min_v.z, max_v.x, max_v.y, max_v.z
        );
        let (x0, x1) = float_bounds_to_inclusive_i64(min_v.x, max_v.x, 1e-3);
        let (y0, y1) = float_bounds_to_inclusive_i64(min_v.y, max_v.y, 1e-3);
        let (z0, z1) = float_bounds_to_inclusive_i64(min_v.z, max_v.z, 1e-3);
        println!(
            "[Info] Rotated integer bbox: x=[{},{}] y=[{},{}] z=[{},{}]",
            x0, x1, y0, y1, z0, z1
        );

        let mut out_total = 1usize;
        for (lo, hi) in [(x0, x1), (y0, y1), (z0, z1)] {
            let count = hi
                .checked_sub(lo)
                .and_then(|n| n.checked_add(1))
                .and_then(|n| usize::try_from(n.max(1)).ok())
                .ok_or_else(|| {
                    RustMsptError::InvalidConfig("crop output dimension overflow".into())
                })?;
            out_total = out_total
                .checked_mul(count)
                .ok_or_else(|| RustMsptError::InvalidConfig("crop output size overflow".into()))?;
        }
        let consider_gpu = requested == crate::compute::backend::AccelerationMode::Gpu
            || (requested == crate::compute::backend::AccelerationMode::Auto
                && out_total >= self.config.acceleration.gpu_min_voxels);
        let integer_safe = !consider_gpu
            || gpu_crop_values_supported(&input_volume, background, interpolation_mode);
        if consider_gpu && !integer_safe {
            let reason = "crop GPU cannot preserve input integers for the selected interpolation";
            if !self.config.acceleration.cpu_fallback {
                return Err(RustMsptError::Gpu(reason.into()));
            }
            eprintln!("[Info] {reason}; using CPU");
        }
        let budget = self
            .config
            .acceleration
            .gpu_memory_limit_mb
            .and_then(|mb| mb.checked_mul(1024 * 1024));
        let estimated = if consider_gpu && integer_safe {
            let out_dims = [x0, y0, z0]
                .into_iter()
                .zip([x1, y1, z1])
                .map(|(lo, hi)| (hi - lo + 1).max(1) as usize)
                .collect::<Vec<_>>();
            match plan_crop_gpu_tiles(
                [input_volume.width, input_volume.height, input_volume.depth],
                &rot,
                &centroid,
                [x0, y0, z0],
                [out_dims[0], out_dims[1], out_dims[2]],
                budget,
                None,
            ) {
                Ok(plan) => {
                    println!(
                        "[Info] crop GPU plan: tiles={} tile={}x{}x{} peak_bytes={}",
                        plan.tiles,
                        plan.tile_dims[0],
                        plan.tile_dims[1],
                        plan.tile_dims[2],
                        plan.peak_bytes
                    );
                    Some(plan.peak_bytes)
                }
                Err(error) => {
                    eprintln!("[Info] crop GPU plan: {}", error.reason);
                    error.needed
                }
            }
        } else {
            None
        };
        let selection = crate::compute::policy::resolve_execution(
            &self.config.acceleration,
            requested,
            out_total,
            self.config.acceleration.gpu_min_voxels,
            integer_safe,
            estimated,
        )?;
        if let Some(reason) = &selection.fallback {
            eprintln!("[Info] crop: {}", reason.reason);
        }
        #[cfg(feature = "gpu")]
        let attempted = if selection.backend.is_gpu() {
            Some(rotate_and_crop_gpu(
                &input_volume,
                background,
                &rot,
                &centroid,
                &min_v,
                &max_v,
                interpolation_mode,
                budget,
            ))
        } else {
            None
        };
        #[cfg(not(feature = "gpu"))]
        let attempted: Option<std::result::Result<(Volume3D, usize), String>> = None;
        let (cropped, backend) = match attempted {
            Some(Ok((volume, _tiles))) => (volume, "gpu"),
            Some(Err(error)) if !self.config.acceleration.cpu_fallback => {
                return Err(RustMsptError::Gpu(format!("crop transform: {error}")))
            }
            other => {
                if let Some(Err(error)) = other {
                    eprintln!("[Warning] crop transform: {error}; falling back to CPU");
                }
                (
                    rotate_and_crop(
                        &input_volume,
                        background,
                        &rot,
                        &centroid,
                        &min_v,
                        &max_v,
                        interpolation_mode,
                    ),
                    "cpu",
                )
            }
        };
        println!(
            "[Info] Crop execution: backend={backend}, workers={}, worker_index={:?}",
            rayon::current_num_threads(),
            rayon::current_thread_index()
        );

        timer.stage("transform_and_backend");
        let trim_pixels = resolve_trim_pixels(self.config.edge_trim, &cropped, background)?;
        println!("[Info] Edge trim pixels (xy): {}", trim_pixels);
        let cropped = trim_volume_border(cropped, trim_pixels)?;
        timer.stage("trim");

        println!(
            "[Info] Cropped output shape=({},{},{})",
            cropped.width, cropped.height, cropped.depth
        );

        timer.restart();
        let output = Path::new(&self.config.output.path);
        let prefix = self.config.output.folder_prefix.as_deref();
        let ext = self.config.output.folder_extension.as_deref();
        save_tiff_or_folder_with_ext(&cropped, output, prefix, ext)?;
        timer.stage("encode_write");
        println!(
            "[Info] Crop pipeline completed. Output written: {}",
            output.display()
        );
        timer.total("total_in_pool");
        timer.report_resources();
        Ok(())
    }
}

#[cfg(test)]
mod performance_tests {
    use super::*;

    // AI-FUNC-SUMMARY: Compare owned in-place trimming with indexed source rows and verify allocation reuse.
    #[test]
    fn owned_trim_preserves_rows_and_allocation() {
        for (w, h, d) in [(1, 1, 1), (9, 7, 3), (8, 6, 2)] {
            for trim in 0..=2 {
                let mut source = Volume3D {
                    width: w,
                    height: h,
                    depth: d,
                    data: vec![0; w * h * d],
                    numeric_type: crate::io::volume::VolumeNumericType::U32,
                };
                for (i, value) in source.data.iter_mut().enumerate() {
                    *value = i as i64;
                }
                let ptr = source.data.as_ptr();
                let expected: Vec<_> = (0..d)
                    .flat_map(|z| {
                        (trim..h.saturating_sub(trim)).flat_map(move |y| {
                            (trim..w.saturating_sub(trim))
                                .map(move |x| voxel_index(w, h, x, y, z) as i64)
                        })
                    })
                    .collect();
                let result = trim_volume_border(source, trim);
                if w <= 2 * trim || h <= 2 * trim {
                    assert!(result.is_err());
                    continue;
                }
                let result = result.unwrap();
                assert_eq!(result.data, expected);
                assert_eq!(result.data.as_ptr(), ptr);
                assert_eq!(
                    (result.width, result.height, result.depth),
                    (w - 2 * trim, h - 2 * trim, d)
                );
            }
        }
    }
    // AI-FUNC-SUMMARY: Compare tiled CPU sampling with a serial voxel oracle across workers and tile tails.
    #[test]
    fn tiled_resampling_matches_serial_coordinates() {
        let volume = Volume3D {
            width: 90,
            height: 80,
            depth: 3,
            data: (0..21600).map(|i| (i % 251) as i64 - 100).collect(),
            numeric_type: crate::io::volume::VolumeNumericType::I16,
        };
        let rot = Matrix3::new(0.98, -0.1, 0.0, 0.1, 0.98, 0.0, 0.0, 0.0, 1.0);
        let center = Vector3::new(1.5, -0.5, 0.0);
        for depth in [1, 3] {
            let min = Vector3::new(-1.0, -2.0, 0.0);
            let max = Vector3::new(83.0, 68.0, (depth - 1) as f64);
            for mode in [InterpolationMode::Nearest, InterpolationMode::Trilinear] {
                let mut expected = Vec::new();
                for z in 0..depth {
                    for y in 0..71 {
                        for x in 0..85 {
                            let src = rot * Vector3::new(x as f64 - 1.0, y as f64 - 2.0, z as f64)
                                + center;
                            expected.push(match mode {
                                InterpolationMode::Nearest => {
                                    sample_nearest(&volume, -999, src.x, src.y, src.z)
                                }
                                InterpolationMode::Trilinear => {
                                    sample_trilinear(&volume, -999, src.x, src.y, src.z)
                                }
                            });
                        }
                    }
                }
                for workers in [1, 2, 8] {
                    let pool = rayon::ThreadPoolBuilder::new()
                        .num_threads(workers)
                        .build()
                        .unwrap();
                    let result = pool.install(|| {
                        rotate_and_crop(&volume, -999, &rot, &center, &min, &max, mode)
                    });
                    assert_eq!((result.width, result.height, result.depth), (85, 71, depth));
                    assert_eq!(result.data, expected);
                }
            }
        }
    }
    // AI-FUNC-SUMMARY: Measure original slice scheduling versus bounded tiles with identical trilinear outputs.
    #[test]
    #[ignore = "release performance measurement"]
    fn crop_tile_benchmark() {
        use std::time::Instant;
        for (width, height, depth) in [(1024, 512, 1), (256, 256, 32)] {
            let volume = Volume3D {
                width,
                height,
                depth,
                data: (0..width * height * depth)
                    .map(|i| (i % 251) as i64)
                    .collect(),
                numeric_type: crate::io::volume::VolumeNumericType::U16,
            };
            let rot = Matrix3::identity();
            let center = Vector3::new(0.25, 0.25, 0.25);
            let min = Vector3::zeros();
            let max = Vector3::new((width - 1) as f64, (height - 1) as f64, (depth - 1) as f64);
            for workers in [1, 2, 8] {
                let pool = rayon::ThreadPoolBuilder::new()
                    .num_threads(workers)
                    .build()
                    .unwrap();
                for repeat in 0..6 {
                    pool.install(|| {
                        let start = Instant::now();
                        let mut old = vec![0; volume.data.len()];
                        old.par_chunks_mut(width*height).enumerate().for_each(|(z, slab)| {
                            for y in 0..height { for x in 0..width {
                                let src = rot * Vector3::new(x as f64, y as f64, z as f64) + center;
                                slab[y*width+x] = sample_trilinear(&volume, 0, src.x, src.y, src.z);
                            } }
                        });
                        let old_secs = start.elapsed().as_secs_f64();
                        let start = Instant::now();
                        let new = rotate_and_crop(&volume, 0, &rot, &center, &min, &max, InterpolationMode::Trilinear);
                        let new_secs = start.elapsed().as_secs_f64();
                        assert_eq!(new.data, old);
                        println!("crop_tiles shape={width}x{height}x{depth} workers={workers} repeat={repeat} slice_seconds={old_secs:.9} tile_seconds={new_secs:.9}");
                    });
                }
            }
        }
    }
    // AI-FUNC-SUMMARY: Build asymmetric and rank-deficient foreground volumes spanning multiple fixed PCA blocks.
    fn pca_fixture(kind: usize) -> Volume3D {
        let (width, height, depth) = (97usize, 73usize, 29usize);
        let mut data = vec![0; width * height * depth];
        for z in 0..depth { for y in 0..height { for x in 0..width {
            let dx = x as f64 - 48.0;
            let dy = y as f64 - 36.0;
            let dz = z as f64 - 14.0;
            let solid = match kind {
                0 => ((dx + 0.37*dy)/34.0).powi(2) + ((dy - 0.12*dz)/21.0).powi(2) + (dz/10.0).powi(2) < 1.0,
                1 => dx.abs() <= 10.0 && dy.abs() <= 10.0 && dz.abs() <= 10.0,
                2 => x == 48 && y == 36,
                3 => z == 14 && dx.abs() <= 10.0 && dy.abs() <= 10.0,
                _ => x == 48 && y == 36 && z == 14,
            };
            if solid { data[voxel_index(width, height, x, y, z)] = 1; }
        } } }
        Volume3D { width, height, depth, data, numeric_type: crate::io::volume::VolumeNumericType::U8 }
    }

    // AI-FUNC-SUMMARY: Verify fixed-block PCA is worker-independent and agrees with the serial oracle's axes up to the canonical column signs, including degenerate foregrounds.
    #[test]
    fn fixed_block_pca_matches_serial_and_workers() {
        for kind in 0..5 {
            let mut volume = pca_fixture(kind);
            if kind == 0 {
                volume.depth *= 8;
                volume.data.resize(volume.width * volume.height * volume.depth, 0);
            }
            let serial = estimate_pca_bbox_serial(&volume, 0).unwrap();
            let mut reference = None;
            for workers in [1, 2, 8] {
                let pool = rayon::ThreadPoolBuilder::new().num_threads(workers).build().unwrap();
                let actual = pool.install(|| estimate_pca_bbox(&volume, 0)).unwrap();
                if let Some(previous) = &reference { assert_eq!(&actual, previous); }
                assert_eq!(actual.4, serial.4);
                assert!((actual.1 - serial.1).norm() < 1e-12);
                assert!((actual.0.transpose()*actual.0 - Matrix3::identity()).norm() < 1e-12);
                assert!(actual.0.determinant() > 0.999999999);
                if kind == 0 {
                    for c in 0..3 {
                        let (a, b) = (actual.0.column(c), serial.0.column(c));
                        assert!((a - b).norm().min((a + b).norm()) < 1e-10, "{} {}", actual.0, serial.0);
                    }
                }
                reference = Some(actual);
            }
        }
        let mut empty = pca_fixture(4); empty.data.fill(0);
        assert!(estimate_pca_bbox(&empty, 0).is_err());
    }

    // AI-FUNC-SUMMARY: Test helper voxelizing a shape predicate into a U8 volume of the given size.
    fn shape_volume(dims: (usize, usize, usize), solid: impl Fn(f64, f64, f64) -> bool) -> Volume3D {
        let (width, height, depth) = dims;
        let mut data = vec![0; width * height * depth];
        for z in 0..depth { for y in 0..height { for x in 0..width {
            if solid(x as f64, y as f64, z as f64) { data[voxel_index(width, height, x, y, z)] = 1; }
        } } }
        Volume3D { width, height, depth, data, numeric_type: crate::io::volume::VolumeNumericType::U8 }
    }

    // AI-FUNC-SUMMARY: Verify the one-pass moment merge matches the fixed-block three-pass oracle within tight tolerances (frame, centroid, bounds, nearest crop output) on oblique non-degenerate samples at serial and parallel sizes, bit-identical across 1/2/8 workers.
    #[test]
    fn online_moments_match_three_pass_and_workers() {
        let mut large = pca_fixture(0);
        large.depth *= 8;
        large.data.resize(large.width * large.height * large.depth, 0);
        let oblique = shape_volume((120, 90, 110), |x, y, z| {
            let (a, b, c) = (x - 61.3, y - 44.7, z - 52.1);
            let u = 0.8 * a + 0.36 * b - 0.48 * c;
            let v = -0.6 * a + 0.48 * b - 0.64 * c;
            let w = 0.0 * a + 0.8 * b + 0.6 * c;
            (u / 50.0).powi(2) + (v / 30.0).powi(2) + (w / 17.0).powi(2) < 1.0
        });
        for volume in [pca_fixture(0), large, oblique] {
            let oracle = estimate_pca_bbox_three_pass(&volume, 0).unwrap();
            let mut reference = None;
            for workers in [1, 2, 8] {
                let pool = rayon::ThreadPoolBuilder::new().num_threads(workers).build().unwrap();
                let actual = pool.install(|| estimate_pca_bbox(&volume, 0)).unwrap();
                if let Some(previous) = &reference { assert_eq!(&actual, previous); }
                assert_eq!(actual.4, oracle.4);
                assert!((actual.1 - oracle.1).norm() < 1e-11, "{:?} {:?}", actual.1, oracle.1);
                assert!((actual.0 - oracle.0).norm() < 1e-10, "{} {}", actual.0, oracle.0);
                assert!((actual.2 - oracle.2).norm() < 1e-8);
                assert!((actual.3 - oracle.3).norm() < 1e-8);
                let new = rotate_and_crop(&volume, 0, &actual.0, &actual.1, &actual.2, &actual.3, InterpolationMode::Nearest);
                let old = rotate_and_crop(&volume, 0, &oracle.0, &oracle.1, &oracle.2, &oracle.3, InterpolationMode::Nearest);
                assert_eq!(new.data, old.data);
                reference = Some(actual);
            }
        }
    }

    // AI-FUNC-SUMMARY: A sample rotated about the scan z axis must come back upright: for covariances with distinct eigenvalues rotated 50 degrees about z (the repository CT's shape) and 140 degrees, with tiny noise, pca_frame returns the pure z rotation - non-negative diagonal, z column +z, right-handed - never the 180-degree turn the largest-component rule produced.
    #[test]
    fn frame_keeps_the_scan_orientation_of_a_sample_rotated_about_z() {
        for (degrees, noise) in [(50.0f64, 0.0), (50.0, 1e-9), (140.0, 0.0), (-35.0, 1e-9)] {
            let (s, c) = degrees.to_radians().sin_cos();
            let r = Matrix3::new(c, -s, 0.0, s, c, 0.0, 0.0, 0.0, 1.0);
            let d = Matrix3::from_diagonal(&Vector3::new(9.0, 4.0, 1.0));
            let mut cov = r * d * r.transpose();
            cov[(0, 1)] += noise;
            cov[(1, 0)] += noise;
            let frame = pca_frame(cov);
            assert!(frame.determinant() > 0.999_999, "{degrees}: {frame}");
            assert!(frame[(2, 2)] > 0.999_999, "{degrees}: slice axis must stay +z: {frame}");
            let expected = if c >= 0.0 { r } else { Matrix3::new(-c, s, 0.0, -s, -c, 0.0, 0.0, 0.0, 1.0) };
            assert!((frame - expected).norm() < 1e-6, "{degrees}: {frame} vs {expected}");
            for k in 0..3 {
                assert!(frame[(k, k)] >= 0.0, "{degrees}: diagonal {k} negative: {frame}");
            }
        }
    }

    // AI-FUNC-SUMMARY: Check the canonical frame for triple-degenerate (cube, sphere) and double-degenerate (cylinders along z and x) foregrounds, and that slightly perturbed versions of each give the identical frame and a right-handed rotation.
    #[test]
    fn near_degenerate_eigenspaces_use_canonical_frame() {
        let cube = |extra: bool| shape_volume((40, 40, 40), move |x, y, z| {
            (x - 20.0).abs() <= 10.0 && (y - 20.0).abs() <= 10.0 && (z - 20.0).abs() <= 10.0 || (extra && x == 21.0 && y == 17.0 && z == 31.0)
        });
        let sphere = |extra: bool| shape_volume((48, 48, 48), move |x, y, z| {
            (x - 23.5).powi(2) + (y - 23.5).powi(2) + (z - 23.5).powi(2) < 15.0f64.powi(2) || (extra && x == 24.0 && y == 23.0 && z == 39.0)
        });
        let cylinder_z = |extra: bool| shape_volume((40, 40, 80), move |x, y, z| {
            (x - 19.5).powi(2) + (y - 19.5).powi(2) < 100.0 && (5.0..75.0).contains(&z) || (extra && x == 25.0 && y == 12.0 && z == 40.0)
        });
        let cylinder_x = |extra: bool| shape_volume((80, 40, 40), move |x, y, z| {
            (y - 19.5).powi(2) + (z - 19.5).powi(2) < 100.0 && (5.0..75.0).contains(&x) || (extra && x == 40.0 && y == 27.0 && z == 14.0)
        });
        let frame = |v: &Volume3D| estimate_pca_bbox(v, 0).unwrap().0;
        let identity = Matrix3::identity();
        for rot in [frame(&cube(false)), frame(&cube(true)), frame(&sphere(false)), frame(&sphere(true))] {
            assert_eq!(rot, identity, "{rot}");
        }
        let z_frame = Matrix3::from_columns(&[Vector3::z(), Vector3::x(), Vector3::y()]);
        for rot in [frame(&cylinder_z(false)), frame(&cylinder_z(true))] {
            assert!((rot - z_frame).norm() < 1e-12, "{rot}");
        }
        let x_long = frame(&cylinder_x(false));
        let x_perturbed = frame(&cylinder_x(true));
        assert!((x_long - identity).norm() < 1e-12, "{x_long}");
        assert!((x_long - x_perturbed).norm() < 1e-12, "{x_long} {x_perturbed}");
        assert!(x_long.determinant() > 0.999999999);
    }

    // AI-FUNC-SUMMARY: Feed pca_frame covariances whose degenerate eigenspace is tilted away from the scan axes, with and without relative noise far below the tolerance, and require identical right-handed orthonormal frames whose degenerate columns span the eigenspace.
    #[test]
    fn tilted_degenerate_eigenspace_is_stable_under_noise() {
        let axis = Vector3::new(1.0, 2.0, 2.0) / 3.0;
        let base = Matrix3::identity() * 4.0 + axis * axis.transpose() * 5.0;
        let noise = Matrix3::new(1.0, 0.3, -0.2, 0.3, -0.7, 0.5, -0.2, 0.5, 0.4) * 1e-9;
        let a = pca_frame(base);
        let b = pca_frame(base + noise);
        assert!((a.column(0).dot(&axis).abs() - 1.0).abs() < 1e-9);
        assert!((a - b).norm() < 1e-6, "{a} {b}");
        assert!((a.transpose() * a - Matrix3::identity()).norm() < 1e-12);
        assert!(a.determinant() > 0.999999999);
        for c in 1..3 {
            assert!(a.column(c).dot(&axis).abs() < 1e-9);
        }
        let triple = pca_frame(Matrix3::identity() * 7.0 + noise);
        assert_eq!(triple, Matrix3::identity());
    }

    // AI-FUNC-SUMMARY: Release benchmark of the three-pass fixed-block PCA versus the one-pass moment merge on small, large and extra-large oblique volumes, one warmup plus five alternating samples per worker count.
    #[test]
    #[ignore = "release performance measurement"]
    fn pca_online_benchmark() {
        let upscale = |source: &Volume3D, f: usize| {
            let mut v = Volume3D { width: source.width*f, height: source.height*f, depth: source.depth*f, data: vec![0; source.data.len()*f*f*f], numeric_type: source.numeric_type };
            for z in 0..v.depth { for y in 0..v.height { for x in 0..v.width {
                v.data[voxel_index(v.width, v.height, x, y, z)] = source.data[voxel_index(source.width, source.height, x/f, y/f, z/f)];
            } } }
            v
        };
        let small = pca_fixture(0);
        let large = upscale(&small, 2);
        let xlarge = upscale(&small, 4);
        for (case, volume) in [("small", &small), ("large", &large), ("xlarge", &xlarge)] {
            for workers in [1, 2, 4] {
                let pool = rayon::ThreadPoolBuilder::new().num_threads(workers).build().unwrap();
                pool.install(|| {
                    estimate_pca_bbox(volume, 0).unwrap();
                    estimate_pca_bbox_three_pass(volume, 0).unwrap();
                    for sample in 0..5 {
                        let mut times = [0.0; 2];
                        for which in if sample % 2 == 0 { [0, 1] } else { [1, 0] } {
                            let start = std::time::Instant::now();
                            for _ in 0..10 {
                                std::hint::black_box(if which == 0 { estimate_pca_bbox_three_pass(volume, 0) } else { estimate_pca_bbox(volume, 0) }).unwrap();
                            }
                            times[which] = start.elapsed().as_secs_f64();
                        }
                        println!("PCA_ONLINE case={case} voxels={} workers={workers} sample={sample} repeats=10 three_pass_s={:.6} online_s={:.6}", volume.data.len(), times[0], times[1]);
                    }
                });
            }
        }
    }

    // AI-FUNC-SUMMARY: Report, for the repository's real CT RAW stack, the covariance eigenvalues and whether the canonical frame differs from the original serial PCA frame (column signs or degenerate basis); prints only, never asserts on the data.
    #[test]
    #[ignore = "requires data/input/ct_stack"]
    fn real_ct_frame_report() {
        let spec = RawFolderSpec {
            folder: std::path::PathBuf::from("data/input/ct_stack"),
            width: 744,
            height: 789,
            bits: 16,
            signed: false,
            byte_order: ByteOrder::LittleEndian,
            slice_start: -1,
            slice_end: -1,
        };
        let volume = load_raw_folder(&spec).unwrap();
        let background = detect_background_mode(&volume);
        let serial = estimate_pca_bbox_serial(&volume, background).unwrap();
        let online = estimate_pca_bbox(&volume, background).unwrap();
        let three = estimate_pca_bbox_three_pass(&volume, background).unwrap();
        let signs: Vec<f64> = (0..3).map(|c| online.0.column(c).dot(&serial.0.column(c)).signum()).collect();
        let (_, _, _, _, count) = online;
        println!("REAL_CT background={background} count={count} column_dot_signs={signs:?}");
        println!("REAL_CT serial={} online={} three_pass_diff={:.3e}", serial.0, online.0, (online.0 - three.0).norm());
        println!("REAL_CT serial_bounds={:?}..{:?} online_bounds={:?}..{:?}", serial.2, serial.3, online.2, online.3);
    }

    // AI-FUNC-SUMMARY: Measure serial and fixed-block three-pass PCA with one warmup and five alternating samples per worker count.
    #[test]
    #[ignore = "release performance measurement"]
    fn pca_block_benchmark() {
        let small = pca_fixture(0);
        let mut large = Volume3D { width: small.width*2, height: small.height*2, depth: small.depth*2, data: vec![0; small.data.len()*8], numeric_type: small.numeric_type };
        for z in 0..large.depth { for y in 0..large.height { for x in 0..large.width {
            large.data[voxel_index(large.width, large.height, x, y, z)] = small.data[voxel_index(small.width, small.height, x/2, y/2, z/2)];
        } } }
        for (case, volume) in [("small", small), ("large", large)] {
        for workers in [1, 2, 8] {
            let pool = rayon::ThreadPoolBuilder::new().num_threads(workers).build().unwrap();
            for sample in 0..6 {
                pool.install(|| {
                    let mut times = [0.0; 2];
                    for which in if sample % 2 == 0 { [0, 1] } else { [1, 0] } {
                        let start = std::time::Instant::now();
                        for _ in 0..10 {
                            std::hint::black_box(if which == 0 { estimate_pca_bbox_serial(&volume, 0) } else { estimate_pca_bbox(&volume, 0) }).unwrap();
                        }
                        times[which] = start.elapsed().as_secs_f64();
                    }
                    println!("pca_blocks case={case} workers={workers} sample={sample} repeats=10 serial_seconds={:.9} parallel_seconds={:.9}", times[0], times[1]);
                });
            }
        }
    }
    }

    // AI-FUNC-SUMMARY: Compare dense/sparse background modes to an independent full-grid ordered-map oracle across integer metadata, collapsed dimensions, ties and out-of-range i64 extrema.
    #[test]
    fn background_dense_and_spill_match_reference() {
        for (width,height,depth) in [(1,7,3),(7,1,3),(7,3,1),(256,256,1),(64,64,16)] {
            for values in [vec![0,1,128,255],vec![-32768,-1,0,32767],vec![0,1,65535,65536],vec![i64::MIN,i64::MAX,-129,256]] {
                let mut volume=Volume3D {width,height,depth,data:(0..width*height*depth).map(|i|values[(i*17+3)%values.len()]).collect(),numeric_type:VolumeNumericType::U8};
                let mut counts=std::collections::BTreeMap::new();
                for z in 0..depth {for y in 0..height {for x in 0..width {
                    if x==0 || x+1==width || y==0 || y+1==height || z==0 || z+1==depth {
                        *counts.entry(volume.data[(z*height+y)*width+x]).or_insert(0usize)+=1;
                    }
                }}}
                let max_count=*counts.values().max().unwrap();
                let expected=counts.into_iter().find(|(_,n)|*n==max_count).unwrap().0;
                for ty in [VolumeNumericType::U8,VolumeNumericType::I8,VolumeNumericType::U16,VolumeNumericType::I16,VolumeNumericType::U32,VolumeNumericType::I32] {
                    volume.numeric_type=ty;
                    assert_eq!(detect_background_mode(&volume),expected,"{width}x{height}x{depth} {ty:?}");
                }
            }
        }
    }

    // AI-FUNC-SUMMARY: Compare face-only boundary counts with full-scan counting, including collapsed dimensions and deterministic ties.
    #[test]
    fn background_faces_match_shell_without_duplicate_edges() {
        for width in [1, 2, 9] { for height in [1, 2, 7] { for depth in [1, 2, 5] {
            let volume = Volume3D { width, height, depth, data: (0..width*height*depth).map(|i| ((i*17+3)%11) as i64 - 5).collect(), numeric_type: crate::io::volume::VolumeNumericType::I16 };
            let mut expected = std::collections::BTreeMap::new();
            for z in 0..depth { for y in 0..height { for x in 0..width {
                if x == 0 || x + 1 == width || y == 0 || y + 1 == height || z == 0 || z + 1 == depth {
                    *expected.entry(volume.data[voxel_index(width,height,x,y,z)]).or_insert(0usize) += 1;
                }
            } } }
            let count = expected.values().copied().max().unwrap();
            let value = expected.into_iter().find(|(_, n)| *n == count).unwrap().0;
            assert_eq!(detect_background_mode(&volume), value);
        } } }
    }

}

#[cfg(test)]
mod gpu_tile_plan_tests {
    use super::*;

    // AI-FUNC-SUMMARY: Build a unit-quaternion rotation about a normalized axis for oblique tile fixtures; returns Matrix3; side effects: none.
    pub(super) fn axis_rotation(axis: [f64; 3], angle: f64) -> Matrix3<f64> {
        let axis = nalgebra::Unit::new_normalize(Vector3::new(axis[0], axis[1], axis[2]));
        *nalgebra::Rotation3::from_axis_angle(&axis, angle).matrix()
    }

    // AI-FUNC-SUMMARY: Compute rotated-frame min/max covering the whole source box plus a background margin; returns (min, max); side effects: none.
    pub(super) fn covering_bounds(
        dims: [usize; 3],
        rot: &Matrix3<f64>,
        centroid: &Vector3<f64>,
        margin: f64,
    ) -> (Vector3<f64>, Vector3<f64>) {
        let mut min = Vector3::repeat(f64::INFINITY);
        let mut max = Vector3::repeat(f64::NEG_INFINITY);
        for corner in 0..8 {
            let p = Vector3::new(
                if corner & 1 == 0 { 0.0 } else { dims[0] as f64 - 1.0 },
                if corner & 2 == 0 { 0.0 } else { dims[1] as f64 - 1.0 },
                if corner & 4 == 0 { 0.0 } else { dims[2] as f64 - 1.0 },
            );
            let local = rot.transpose() * (p - centroid);
            min = min.inf(&local);
            max = max.sup(&local);
        }
        (min.add_scalar(-margin), max.add_scalar(margin))
    }

    // AI-FUNC-SUMMARY: Verify tiles partition the output and every f64 sample index a tile reaches (nearest and trilinear neighbours) lies in its planned block.
    #[test]
    fn tile_blocks_cover_every_sample_and_partition_output() {
        let src = [13usize, 11, 7];
        for rot in [
            Matrix3::identity(),
            Matrix3::new(0.0, -1.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0),
            axis_rotation([1.0, 2.0, 3.0], 0.7),
        ] {
            let centroid = Vector3::new(6.0, 5.0, 3.0);
            let (min, max) = covering_bounds(src, &rot, &centroid, 2.0);
            let origin = [min.x.floor() as isize, min.y.floor() as isize, min.z.floor() as isize];
            let out = [
                (max.x.ceil() - min.x.floor()) as usize + 1,
                (max.y.ceil() - min.y.floor()) as usize + 1,
                (max.z.ceil() - min.z.floor()) as usize + 1,
            ];
            for tile in [out, [out[0], out[1], 3], [out[0], 4, 1], [5, 1, 1], [1, 1, 1]] {
                let mut seen = vec![0u8; out.iter().product()];
                for_each_crop_tile(out, tile, |lo, hi| {
                    let block = crop_tile_source_block(src, &rot, &centroid, origin, lo, hi).unwrap();
                    for z in lo[2]..=hi[2] {
                        for y in lo[1]..=hi[1] {
                            for x in lo[0]..=hi[0] {
                                seen[(z * out[1] + y) * out[0] + x] += 1;
                                let local = Vector3::new(
                                    (origin[0] + x as isize) as f64,
                                    (origin[1] + y as isize) as f64,
                                    (origin[2] + z as isize) as f64,
                                );
                                let s = rot * local + centroid;
                                let mut reads = vec![[s.x.round(), s.y.round(), s.z.round()]];
                                for corner in 0..8 {
                                    reads.push([0, 1, 2].map(|axis| {
                                        s[axis].floor() + ((corner >> axis) & 1) as f64
                                    }));
                                }
                                for read in reads {
                                    let inside = (0..3)
                                        .all(|axis| read[axis] >= 0.0 && read[axis] < src[axis] as f64);
                                    if inside {
                                        for (axis, &coordinate) in read.iter().enumerate() {
                                            let i = coordinate as usize;
                                            assert!(
                                                i >= block.origin[axis]
                                                    && i < block.origin[axis] + block.dims[axis],
                                                "axis {axis} index {i} outside {block:?}"
                                            );
                                        }
                                    }
                                }
                            }
                        }
                    }
                    Ok::<(), ()>(())
                })
                .unwrap();
                assert!(seen.iter().all(|&n| n == 1));
            }
        }
    }

    // AI-FUNC-SUMMARY: Verify planner levels (single tile, z slabs, rows, x runs), budget adherence, device buffer limits and minimal-tile refusal.
    #[test]
    fn planner_shrinks_levels_and_refuses_minimal_overflow() {
        let src = [40usize, 30, 20];
        let rot = axis_rotation([0.2, -0.4, 1.0], 0.5);
        let centroid = Vector3::new(19.5, 14.5, 9.5);
        let (min, max) = covering_bounds(src, &rot, &centroid, 1.0);
        let origin = [min.x.floor() as isize, min.y.floor() as isize, min.z.floor() as isize];
        let out = [
            (max.x.ceil() - min.x.floor()) as usize + 1,
            (max.y.ceil() - min.y.floor()) as usize + 1,
            (max.z.ceil() - min.z.floor()) as usize + 1,
        ];
        let plan = |budget, limit| plan_crop_gpu_tiles(src, &rot, &centroid, origin, out, budget, limit);
        let full = plan(None, None).unwrap();
        assert_eq!((full.tiles, full.tile_dims), (1, out));
        let mut levels = [false; 3];
        let mut budget = full.peak_bytes;
        loop {
            match plan(Some(budget), None) {
                Ok(p) => {
                    assert!(p.peak_bytes <= budget);
                    let covered: usize = p.tile_dims.iter().product();
                    assert!(p.tiles >= out.iter().product::<usize>().div_ceil(covered));
                    if p.tile_dims[0] == out[0] && p.tile_dims[1] == out[1] && p.tiles > 1 {
                        levels[0] = true;
                    } else if p.tile_dims[0] == out[0] && p.tile_dims[1] < out[1] {
                        levels[1] = true;
                    } else if p.tile_dims[0] < out[0] {
                        levels[2] = true;
                    }
                    budget = budget * 7 / 10;
                }
                Err(error) => {
                    assert!(error.needed.unwrap() > budget, "{error:?}");
                    break;
                }
            }
        }
        assert_eq!(levels, [true; 3]);
        let limited = plan(None, Some(4 * 4000)).unwrap();
        assert!(limited.tiles > 1);
        assert!(limited.max_block_voxels * 4 <= 16_000 && limited.max_tile_voxels * 4 <= 16_000);
        assert!(plan(Some(0), None).is_err());
        assert!(plan(None, Some(4)).is_err());
    }
}

#[cfg(all(test, feature = "gpu"))]
mod gpu_tile_tests {
    use super::gpu_tile_plan_tests::{axis_rotation, covering_bounds};
    use super::*;
    use crate::gpu::volume_transform::GpuVolumeTransformPipeline;

    // AI-FUNC-SUMMARY: Build a typed fixture: U8/U16 wrapping ramps, I32 full-range labels for nearest or f32-exact signed values for trilinear.
    fn fixture(dims: [usize; 3], kind: VolumeNumericType, trilinear: bool) -> Volume3D {
        let n = dims.iter().product::<usize>();
        let data = (0..n as i64)
            .map(|i| match kind {
                VolumeNumericType::U8 => (i * 37 + 11) % 256,
                VolumeNumericType::U16 => (i * 7919 + 3) % 65_536,
                _ if trilinear => (i * 9_973) % 16_000_001 - 8_000_000,
                _ => ((i * 2_654_435_761) % 4_294_967_296) - 2_147_483_648,
            })
            .collect();
        Volume3D {
            width: dims[0],
            height: dims[1],
            depth: dims[2],
            data,
            numeric_type: kind,
        }
    }

    // AI-FUNC-SUMMARY: Run the original whole-source single-dispatch GPU transform for the same bounds; returns i64 output voxels.
    #[allow(clippy::too_many_arguments)]
    fn untiled(
        gpu: &mut GpuVolumeTransformPipeline,
        volume: &Volume3D,
        background: i64,
        rot: &Matrix3<f64>,
        centroid: &Vector3<f64>,
        min: &Vector3<f64>,
        max: &Vector3<f64>,
        mode: InterpolationMode,
    ) -> Vec<i64> {
        let (x0, x1) = float_bounds_to_inclusive_i64(min.x, max.x, 1e-3);
        let (y0, y1) = float_bounds_to_inclusive_i64(min.y, max.y, 1e-3);
        let (z0, z1) = float_bounds_to_inclusive_i64(min.z, max.z, 1e-3);
        let src: Vec<i32> = volume.data.iter().map(|&v| v as i32).collect();
        let origin = Vector3::new(x0 as f64, y0 as f64, z0 as f64);
        gpu.rotate_and_crop(
            &src,
            volume.width as u32,
            volume.height as u32,
            volume.depth as u32,
            background as i32,
            rot,
            centroid,
            &origin,
            (x1 - x0 + 1) as u32,
            (y1 - y0 + 1) as u32,
            (z1 - z0 + 1) as u32,
            matches!(mode, InterpolationMode::Trilinear) as u32,
        )
        .unwrap()
        .into_iter()
        .map(i64::from)
        .collect()
    }

    // AI-FUNC-SUMMARY: Distance of the nearest f64 source coordinate component to a rounding tie, used to justify f32/f64 nearest mismatches.
    fn tie_distance(rot: &Matrix3<f64>, centroid: &Vector3<f64>, min: &Vector3<f64>, out: [usize; 2], index: usize) -> f64 {
        let (x0, y0, z0) = (min.x.floor(), min.y.floor(), min.z.floor());
        let x = (index % out[0]) as f64;
        let y = ((index / out[0]) % out[1]) as f64;
        let z = (index / (out[0] * out[1])) as f64;
        let s = rot * Vector3::new(x0 + x, y0 + y, z0 + z) + centroid;
        s.iter().map(|v| ((v - v.floor()) - 0.5).abs()).fold(f64::INFINITY, f64::min)
    }

    // AI-FUNC-SUMMARY: Compare budget-forced GPU tilings (single slices, non-dividing slabs, rows, x runs) with the untiled GPU and CPU paths for identity, 90-degree and oblique rotations, nearest/trilinear and U8/U16/I32, then refuse a budget no tile fits.
    #[test]
    fn tiled_gpu_matches_untiled_and_cpu() {
        let mut gpu = match GpuVolumeTransformPipeline::new() {
            Ok(gpu) => gpu,
            Err(error) => {
                eprintln!("SKIP no GPU: {error}");
                return;
            }
        };
        let src = [13usize, 11, 7];
        let rotations = [
            ("identity", Matrix3::identity(), Vector3::new(6.25, 5.0, 3.0)),
            (
                "rot90",
                Matrix3::new(0.0, -1.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0),
                Vector3::new(6.0, 5.0, 3.0),
            ),
            ("oblique", axis_rotation([1.0, 2.0, 3.0], 0.7), Vector3::new(6.1, 4.9, 3.05)),
        ];
        let mut shapes = std::collections::BTreeSet::new();
        for (name, rot, centroid) in rotations {
            let (min, max) = covering_bounds(src, &rot, &centroid, 2.5);
            for kind in [VolumeNumericType::U8, VolumeNumericType::U16, VolumeNumericType::I32] {
                for mode in [InterpolationMode::Nearest, InterpolationMode::Trilinear] {
                    let trilinear = mode == InterpolationMode::Trilinear;
                    let volume = fixture(src, kind, trilinear);
                    let background = if kind == VolumeNumericType::I32 { -77 } else { 3 };
                    let reference = untiled(&mut gpu, &volume, background, &rot, &centroid, &min, &max, mode);
                    let cpu = rotate_and_crop(&volume, background, &rot, &centroid, &min, &max, mode);
                    assert!(reference.contains(&background), "{name}: fixture must sample background");
                    let (single, tiles) =
                        rotate_and_crop_gpu(&volume, background, &rot, &centroid, &min, &max, mode, None).unwrap();
                    assert_eq!(tiles, 1);
                    assert_eq!(single.data, reference, "{name} {kind:?} {mode:?} single");
                    let out = [cpu.width, cpu.height, cpu.depth];
                    let origin = [min.x.floor() as isize, min.y.floor() as isize, min.z.floor() as isize];
                    let full = plan_crop_gpu_tiles(src, &rot, &centroid, origin, out, None, None).unwrap();
                    let mut budget = full.peak_bytes;
                    let ratio = if name == "oblique" { 55 } else { 35 };
                    while let Ok(plan) =
                        plan_crop_gpu_tiles(src, &rot, &centroid, origin, out, Some(budget), None)
                    {
                        let (tiled, tiles) = rotate_and_crop_gpu(
                            &volume, background, &rot, &centroid, &min, &max, mode, Some(budget),
                        )
                        .unwrap();
                        assert_eq!(tiles, plan.tiles);
                        assert_eq!(tiled.data, reference, "{name} {kind:?} {mode:?} tile {:?}", plan.tile_dims);
                        let t = plan.tile_dims;
                        shapes.insert((
                            t[2] == 1,
                            t[0] == out[0] && t[1] < out[1],
                            t[0] < out[0],
                            (0..3).any(|axis| !out[axis].is_multiple_of(t[axis])),
                        ));
                        budget = budget * ratio / 100;
                    }
                    let refused = rotate_and_crop_gpu(
                        &volume, background, &rot, &centroid, &min, &max, mode, Some(budget.min(200)),
                    )
                    .unwrap_err();
                    assert!(refused.contains("no single minimal tile fits"), "{refused}");
                    match mode {
                        InterpolationMode::Nearest if name != "oblique" => {
                            assert_eq!(cpu.data, reference, "{name} {kind:?} nearest cpu");
                        }
                        InterpolationMode::Nearest => {
                            let mut mismatches = 0;
                            for (index, (a, b)) in cpu.data.iter().zip(&reference).enumerate() {
                                if a != b {
                                    mismatches += 1;
                                    let d = tie_distance(&rot, &centroid, &min, [out[0], out[1]], index);
                                    assert!(d < 1e-4, "{name} {kind:?} nearest mismatch {index} tie distance {d}");
                                }
                            }
                            assert!(mismatches * 100 <= cpu.data.len());
                        }
                        InterpolationMode::Trilinear => {
                            let bound = if kind == VolumeNumericType::I32 { 8 } else { 1 };
                            for (a, b) in cpu.data.iter().zip(&reference) {
                                assert!((a - b).abs() <= bound, "{name} {kind:?} trilinear {a} vs {b}");
                            }
                        }
                    }
                }
            }
        }
        assert!(shapes.iter().any(|s| s.0), "no single-slice tiles: {shapes:?}");
        assert!(shapes.iter().any(|s| s.1), "no row tiles: {shapes:?}");
        assert!(shapes.iter().any(|s| s.2), "no x-run tiles: {shapes:?}");
        assert!(shapes.iter().any(|s| s.3), "no non-dividing tiles: {shapes:?}");
    }


    // AI-FUNC-SUMMARY: Force every tile's first source block to miss voxels its samples need; the halo guard trips, the tile reruns with a grown block, and the result still equals the untiled dispatch for nearest and trilinear, whole-volume and budget-tiled plans; no file output.
    #[test]
    fn halo_guard_retries_with_a_grown_block_and_keeps_the_result() {
        let mut gpu = match GpuVolumeTransformPipeline::new() {
            Ok(gpu) => gpu,
            Err(error) => {
                eprintln!("SKIP no GPU: {error}");
                return;
            }
        };
        let src = [13usize, 11, 7];
        let rot = axis_rotation([1.0, 2.0, 3.0], 0.7);
        let centroid = Vector3::new(6.1, 4.9, 3.05);
        let (min, max) = covering_bounds(src, &rot, &centroid, 2.5);
        for mode in [InterpolationMode::Nearest, InterpolationMode::Trilinear] {
            let volume = fixture(src, VolumeNumericType::U16, mode == InterpolationMode::Trilinear);
            let reference = untiled(&mut gpu, &volume, 3, &rot, &centroid, &min, &max, mode);
            let out = {
                let cpu = rotate_and_crop(&volume, 3, &rot, &centroid, &min, &max, mode);
                [cpu.width, cpu.height, cpu.depth]
            };
            let origin = [min.x.floor() as isize, min.y.floor() as isize, min.z.floor() as isize];
            let full = plan_crop_gpu_tiles(src, &rot, &centroid, origin, out, None, None).unwrap();
            for budget in [None, Some(full.peak_bytes / 2)] {
                let (plain, tiles, retries) =
                    rotate_and_crop_gpu_with(&volume, 3, &rot, &centroid, &min, &max, mode, budget, 0).unwrap();
                assert_eq!(plain.data, reference);
                assert_eq!(retries, 0, "the planner's margin never trips the guard");
                let (shrunk, shrunk_tiles, retries) =
                    rotate_and_crop_gpu_with(&volume, 3, &rot, &centroid, &min, &max, mode, budget.map(|b| b * 2), 3).unwrap();
                if budget.is_none() {
                    assert_eq!(shrunk_tiles, tiles, "no budget: same single-tile plan");
                }
                assert!(retries > 0, "{mode:?} {budget:?}: shrinking the first blocks must trip the guard");
                assert_eq!(shrunk.data, reference, "{mode:?} {budget:?}: retried tiles keep the result");
            }
        }
    }
    // AI-FUNC-SUMMARY: Release benchmark of untiled versus budget-forced ~4 and ~16 tile GPU transforms (1 warmup + 5 samples, identical outputs), printing raw seconds and medians.
    #[test]
    #[ignore = "release performance measurement"]
    fn crop_gpu_tile_benchmark() {
        let src = [192usize, 160, 96];
        let rot = axis_rotation([0.3, -0.5, 1.0], 0.4);
        let centroid = Vector3::new(95.5, 79.5, 47.5);
        let (min, max) = covering_bounds(src, &rot, &centroid, 1.0);
        let volume = fixture(src, VolumeNumericType::U16, true);
        let (x0, x1) = float_bounds_to_inclusive_i64(min.x, max.x, 1e-3);
        let (y0, y1) = float_bounds_to_inclusive_i64(min.y, max.y, 1e-3);
        let (z0, z1) = float_bounds_to_inclusive_i64(min.z, max.z, 1e-3);
        let out = [(x1 - x0 + 1) as usize, (y1 - y0 + 1) as usize, (z1 - z0 + 1) as usize];
        let origin = [x0, y0, z0];
        let full = plan_crop_gpu_tiles(src, &rot, &centroid, origin, out, None, None).unwrap();
        let budget_for = |target: usize| {
            let mut budget = full.peak_bytes;
            loop {
                let plan = plan_crop_gpu_tiles(src, &rot, &centroid, origin, out, Some(budget), None).unwrap();
                if plan.tiles >= target {
                    return (budget, plan.tiles);
                }
                budget = budget * 95 / 100;
            }
        };
        let cases = [("untiled", None), ("tiles4", Some(budget_for(4))), ("tiles16", Some(budget_for(16)))];
        for mode in [InterpolationMode::Nearest, InterpolationMode::Trilinear] {
            let mut reference: Option<Vec<i64>> = None;
            for (name, budget) in cases {
                let mut samples = Vec::new();
                let mut tiles = 0;
                for repeat in 0..6 {
                    let start = std::time::Instant::now();
                    let (result, n) = rotate_and_crop_gpu(
                        &volume, 0, &rot, &centroid, &min, &max, mode, budget.map(|b| b.0),
                    )
                    .unwrap();
                    let seconds = start.elapsed().as_secs_f64();
                    tiles = n;
                    match &reference {
                        Some(r) => assert_eq!(&result.data, r),
                        None => reference = Some(result.data),
                    }
                    if repeat > 0 {
                        samples.push(seconds);
                    }
                }
                let mut sorted = samples.clone();
                sorted.sort_by(|a, b| a.partial_cmp(b).unwrap());
                println!(
                    "crop_gpu_tiles mode={mode:?} case={name} budget={:?} tiles={tiles} out={}x{}x{} samples={samples:?} median={:.6}",
                    budget.map(|b| b.0),
                    out[0],
                    out[1],
                    out[2],
                    sorted[2]
                );
            }
        }
    }
}
