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

// AI-FUNC-SUMMARY:
// Purpose: Turn a covariance matrix into a right-handed crop frame whose columns are principal axes in descending eigenvalue order.
// Inputs: symmetric covariance matrix.
// Returns: rotation matrix (columns = frame axes).
// Side effects: None.
// Notes: Adjacent sorted eigenvalues within PCA_DEGENERATE_REL_TOL of the largest magnitude form one eigenspace. A full 3-D eigenspace yields exactly the scan axes. A 2-D eigenspace's columns are the scan axes x, y, z (in that order) projected into it and Gram-Schmidt orthonormalized, accepting an axis only when its residual norm is >= 0.5 (enough axes always exist), so each column has a positive component along its source axis. A non-degenerate column is sign-fixed so its largest-magnitude component (lowest axis on exact ties) is positive, because the eigen-solver's sign flips under last-bit covariance changes. A negative determinant then flips column 2.
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
    let mut start = 0;
    while start < 3 {
        let mut end = start + 1;
        while end < 3 && (values[end - 1] - values[end]).abs() <= PCA_DEGENERATE_REL_TOL * scale {
            end += 1;
        }
        if end - start == 1 {
            let c = &mut columns[start];
            let lead = (0..3).fold(0, |best, i| if c[i].abs() > c[best].abs() { i } else { best });
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
        let c2 = -rot.column(2).into_owned();
        rot.set_column(2, &c2);
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

// AI-FUNC-SUMMARY:
// Purpose: GPU-accelerated rotate-and-crop: upload volume to GPU, dispatch compute, download result.
// Inputs: same as rotate_and_crop.
// Returns: Ok(Volume3D) or GPU error string.
// Side effects: Initializes GPU pipeline on first call, dispatches GPU compute.
// Notes: Converts i64 volume data to i32 for GPU, converts back on download. Only available with feature "gpu".
#[cfg(feature = "gpu")]
fn rotate_and_crop_gpu(
    volume: &Volume3D,
    background: i64,
    rot: &Matrix3<f64>,
    centroid: &Vector3<f64>,
    min_v: &Vector3<f64>,
    max_v: &Vector3<f64>,
    interpolation_mode: InterpolationMode,
) -> std::result::Result<Volume3D, String> {
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
    let out_w = dimension(x0, x1)?;
    let out_h = dimension(y0, y1)?;
    let out_d = dimension(z0, z1)?;
    let source_dims = [volume.width, volume.height, volume.depth].map(u32::try_from);
    let [src_w, src_h, src_d] = source_dims;
    let (src_w, src_h, src_d) = (
        src_w.map_err(|_| "source width exceeds u32")?,
        src_h.map_err(|_| "source height exceeds u32")?,
        src_d.map_err(|_| "source depth exceeds u32")?,
    );

    if !gpu_crop_values_supported(volume, background, interpolation_mode) {
        return Err(
            "crop GPU cannot preserve input integers for the selected interpolation".into(),
        );
    }
    let t0 = std::time::Instant::now();

    let mut pipeline = crate::gpu::volume_transform::GpuVolumeTransformPipeline::new()?;

    let src_i32: Vec<i32> = volume
        .data
        .iter()
        .map(|&v| i32::try_from(v).map_err(|_| "crop GPU input exceeds i32".to_string()))
        .collect::<std::result::Result<_, _>>()?;
    let bg_i32 = i32::try_from(background).map_err(|_| "crop GPU background exceeds i32")?;
    let interp = match interpolation_mode {
        InterpolationMode::Nearest => 0u32,
        InterpolationMode::Trilinear => 1u32,
    };
    let origin = Vector3::new(x0 as f64, y0 as f64, z0 as f64);

    let out_i32 = pipeline.rotate_and_crop(
        &src_i32, src_w, src_h, src_d, bg_i32, rot, centroid, &origin, out_w, out_h, out_d, interp,
    )?;

    let data: Vec<i64> = out_i32.iter().map(|&v| v as i64).collect();
    let elapsed = t0.elapsed().as_secs_f64();
    println!(
        "[Info] GPU volume transform: {:.3}s, {}x{}x{} -> {}x{}x{}",
        elapsed, volume.width, volume.height, volume.depth, out_w, out_h, out_d
    );

    Ok(Volume3D {
        width: out_w as usize,
        height: out_h as usize,
        depth: out_d as usize,
        data,
        numeric_type: volume.numeric_type,
    })
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
    // AI-FUNC-SUMMARY: Execute all crop stages under one configured pool, preserving interpolation/type/backend/fallback policy; report completed-stage wall times, with backend selection and GPU initialization included in transform_and_backend.
    fn run_in_pool(&self) -> Result<()> {
        let requested = crate::compute::policy::configured_mode(&self.config.acceleration)?;
        let total_started = std::time::Instant::now();
        let stage_started = std::time::Instant::now();
        let input_volume = load_input_volume(&self.config)?;
        println!("[Timing] crop stage=load seconds={:.9}", stage_started.elapsed().as_secs_f64());
        println!(
            "[Info] Crop input loaded: shape=({},{},{})",
            input_volume.width, input_volume.height, input_volume.depth
        );

        let stage_started = std::time::Instant::now();
        let background = detect_background_mode(&input_volume);
        println!("[Timing] crop stage=background seconds={:.9}", stage_started.elapsed().as_secs_f64());
        println!("[Info] Background value detected: {}", background);

        let interpolation_mode = parse_interpolation_mode(self.config.interpolation.as_deref())?;
        println!("[Info] Interpolation mode: {:?}", interpolation_mode);

        let stage_started = std::time::Instant::now();
        let (rot, centroid, min_v, max_v, fg_count) = estimate_pca_bbox(&input_volume, background)?;
        println!("[Timing] crop stage=pca seconds={:.9}", stage_started.elapsed().as_secs_f64());
        let stage_started = std::time::Instant::now();
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
        let estimated = (input_volume.data.len() as u64)
            .checked_mul(4)
            .and_then(|n| {
                (out_total as u64)
                    .checked_mul(8)
                    .and_then(|out| n.checked_add(out))
            })
            .and_then(|n| n.checked_add(128));
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
            ))
        } else {
            None
        };
        #[cfg(not(feature = "gpu"))]
        let attempted: Option<std::result::Result<Volume3D, String>> = None;
        let (cropped, backend) = match attempted {
            Some(Ok(volume)) => (volume, "gpu"),
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

        println!("[Timing] crop stage=transform_and_backend seconds={:.9}", stage_started.elapsed().as_secs_f64());
        let stage_started = std::time::Instant::now();
        let trim_pixels = resolve_trim_pixels(self.config.edge_trim, &cropped, background)?;
        println!("[Info] Edge trim pixels (xy): {}", trim_pixels);
        let cropped = trim_volume_border(cropped, trim_pixels)?;
        println!("[Timing] crop stage=trim seconds={:.9}", stage_started.elapsed().as_secs_f64());

        println!(
            "[Info] Cropped output shape=({},{},{})",
            cropped.width, cropped.height, cropped.depth
        );

        let stage_started = std::time::Instant::now();
        let output = Path::new(&self.config.output.path);
        let prefix = self.config.output.folder_prefix.as_deref();
        let ext = self.config.output.folder_extension.as_deref();
        save_tiff_or_folder_with_ext(&cropped, output, prefix, ext)?;
        println!("[Timing] crop stage=encode_write seconds={:.9}", stage_started.elapsed().as_secs_f64());
        println!(
            "[Info] Crop pipeline completed. Output written: {}",
            output.display()
        );
        println!("[Timing] crop stage=total_in_pool seconds={:.9}", total_started.elapsed().as_secs_f64());
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
