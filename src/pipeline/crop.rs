use crate::config::CropConfig;
use crate::error::{Result, RustMsptError};
use crate::io::{
    load_raw_folder, load_tiff_or_folder_with_range, save_tiff_or_folder_with_ext, ByteOrder,
    RawFolderSpec, Volume3D,
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

/// Parse byte order text into enum.
/// Inputs: optional config string.
/// Outputs: parsed byte order.
fn parse_byte_order(value: Option<&str>) -> Result<ByteOrder> {
    match value.unwrap_or("little").trim().to_ascii_lowercase().as_str() {
        "little" | "le" => Ok(ByteOrder::LittleEndian),
        "big" | "be" => Ok(ByteOrder::BigEndian),
        other => Err(RustMsptError::InvalidConfig(format!(
            "Invalid byte_order: {other}. Expected little or big"
        ))),
    }
}

/// Parse interpolation mode text.
/// Inputs: optional config string.
/// Outputs: interpolation mode, defaulting to trilinear.
fn parse_interpolation_mode(value: Option<&str>) -> Result<InterpolationMode> {
    match value.unwrap_or("trilinear").trim().to_ascii_lowercase().as_str() {
        "nearest" => Ok(InterpolationMode::Nearest),
        "trilinear" => Ok(InterpolationMode::Trilinear),
        other => Err(RustMsptError::InvalidConfig(format!(
            "Invalid interpolation: {other}. Expected nearest or trilinear"
        ))),
    }
}

/// Read input volume according to crop input type.
/// Inputs: crop config.
/// Outputs: loaded 3D volume.
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

/// Compute linear index for voxel coordinates.
/// Inputs: volume shape and voxel indices.
/// Outputs: linear data index.
fn voxel_index(width: usize, height: usize, x: usize, y: usize, z: usize) -> usize {
    z * width * height + y * width + x
}

/// Sample one voxel value by integer index with background fallback.
/// Inputs: volume, background value, and integer coordinates.
/// Outputs: voxel value or background when out of bounds.
fn sample_voxel_or_background(volume: &Volume3D, background: i64, x: isize, y: isize, z: isize) -> i64 {
    if x < 0
        || y < 0
        || z < 0
        || x as usize >= volume.width
        || y as usize >= volume.height
        || z as usize >= volume.depth
    {
        return background;
    }
    let idx = voxel_index(volume.width, volume.height, x as usize, y as usize, z as usize);
    volume.data[idx]
}

/// Sample source volume with nearest-neighbor interpolation.
/// Inputs: source coordinates and volume context.
/// Outputs: sampled scalar value.
fn sample_nearest(volume: &Volume3D, background: i64, src_x: f64, src_y: f64, src_z: f64) -> i64 {
    let x = src_x.round() as isize;
    let y = src_y.round() as isize;
    let z = src_z.round() as isize;
    sample_voxel_or_background(volume, background, x, y, z)
}

/// Sample source volume with trilinear interpolation.
/// Inputs: source coordinates and volume context.
/// Outputs: interpolated scalar value rounded to i64.
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

/// Snap near-integer floating bound to exact integer to avoid epsilon expansion.
/// Inputs: floating value and tolerance.
/// Outputs: stabilized bound value.
fn stabilize_bound(value: f64, eps: f64) -> f64 {
    let rounded = value.round();
    if (value - rounded).abs() <= eps {
        rounded
    } else {
        value
    }
}

/// Convert floating min/max bound into inclusive integer range.
/// Inputs: floating min/max and tolerance for near-integer stabilization.
/// Outputs: inclusive integer [start, end].
fn float_bounds_to_inclusive_i64(min_v: f64, max_v: f64, eps: f64) -> (isize, isize) {
    let min_s = stabilize_bound(min_v, eps);
    let max_s = stabilize_bound(max_v, eps);
    let start = min_s.floor() as isize;
    let end = max_s.ceil() as isize;
    (start, end)
}

/// Compute non-background ratio on boundary shell with a given thickness.
/// Inputs: volume, background value, and shell thickness.
/// Outputs: ratio in [0,1] where higher means stronger edge artifacts.
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

/// Infer trim pixels from boundary artifact intensity.
/// Inputs: rotated-cropped volume and background value.
/// Outputs: suggested trim pixels in [0,2].
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

/// Parse trim setting from config.
/// Inputs: optional configured trim value and current volume for auto mode.
/// Outputs: effective trim pixels in [0,2].
fn resolve_trim_pixels(config_value: Option<i32>, volume: &Volume3D, background: i64) -> Result<usize> {
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

/// Trim border voxels on XY faces only.
/// Inputs: source volume and trim pixels.
/// Outputs: trimmed volume or error when resulting shape is invalid.
fn trim_volume_border(volume: &Volume3D, trim: usize) -> Result<Volume3D> {
    if trim == 0 {
        return Ok(volume.clone());
    }

    if volume.width <= 2 * trim || volume.height <= 2 * trim {
        return Err(RustMsptError::InvalidConfig(format!(
            "edge_trim={} is too large for output shape ({},{},{})",
            trim, volume.width, volume.height, volume.depth
        )));
    }

    let out_w = volume.width - 2 * trim;
    let out_h = volume.height - 2 * trim;
    let out_d = volume.depth;
    let mut out = vec![0i64; out_w * out_h * out_d];

    for z in 0..out_d {
        for y in 0..out_h {
            for x in 0..out_w {
                let src_x = x + trim;
                let src_y = y + trim;
                let src_z = z;
                let src_idx = voxel_index(volume.width, volume.height, src_x, src_y, src_z);
                let dst_idx = voxel_index(out_w, out_h, x, y, z);
                out[dst_idx] = volume.data[src_idx];
            }
        }
    }

    Ok(Volume3D {
        width: out_w,
        height: out_h,
        depth: out_d,
        data: out,
        numeric_type: volume.numeric_type,
    })
}

/// Detect background value from boundary voxels using mode.
/// Inputs: volume.
/// Outputs: detected background scalar value.
fn detect_background_mode(volume: &Volume3D) -> i64 {
    let mut counts: HashMap<i64, usize> = HashMap::new();

    for z in 0..volume.depth {
        for y in 0..volume.height {
            for x in 0..volume.width {
                let on_boundary = x == 0
                    || y == 0
                    || z == 0
                    || x + 1 == volume.width
                    || y + 1 == volume.height
                    || z + 1 == volume.depth;
                if !on_boundary {
                    continue;
                }
                let idx = voxel_index(volume.width, volume.height, x, y, z);
                *counts.entry(volume.data[idx]).or_insert(0usize) += 1;
            }
        }
    }

    counts
        .into_iter()
        .max_by_key(|(_, c)| *c)
        .map(|(v, _)| v)
        .unwrap_or(0)
}

/// Compute PCA rotation and foreground bounds in rotated space.
/// Inputs: volume and detected background value.
/// Outputs: rotation, centroid, rotated min/max, and foreground count.
fn estimate_pca_bbox(
    volume: &Volume3D,
    background: i64,
) -> Result<(Matrix3<f64>, Vector3<f64>, Vector3<f64>, Vector3<f64>, usize)> {
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

/// Rotate full volume and crop to rotated foreground bounding box.
/// Inputs: source volume, background value, and PCA geometry outputs.
/// Outputs: cropped, axis-aligned volume.
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
    let slice_len = out_w * out_h;

    data.par_chunks_mut(slice_len).enumerate().for_each(|(z, slab)| {
        let z_coord = z0 as f64 + z as f64;
        for y in 0..out_h {
            let y_coord = y0 as f64 + y as f64;
            for x in 0..out_w {
                let local = Vector3::new(
                    x0 as f64 + x as f64,
                    y_coord,
                    z_coord,
                );
                let src = rot * local + centroid;
                let idx = y * out_w + x;
                slab[idx] = match interpolation_mode {
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

    Volume3D {
        width: out_w,
        height: out_h,
        depth: out_d,
        data,
        numeric_type: volume.numeric_type,
    }
}

impl Pipeline for CropPipeline {
    fn run(&self) -> Result<()> {
        // Purpose: Load CT volume, detect foreground, PCA-align, crop bounding box, and save TIFF output.
        // Inputs: crop pipeline config with source and output settings.
        // Outputs: cropped axis-aligned TIFF volume.
        let input_volume = load_input_volume(&self.config)?;
        println!(
            "[Info] Crop input loaded: shape=({},{},{})",
            input_volume.width, input_volume.height, input_volume.depth
        );

        let background = detect_background_mode(&input_volume);
        println!("[Info] Background value detected: {}", background);

        let interpolation_mode = parse_interpolation_mode(self.config.interpolation.as_deref())?;
        println!("[Info] Interpolation mode: {:?}", interpolation_mode);

        let (rot, centroid, min_v, max_v, fg_count) = estimate_pca_bbox(&input_volume, background)?;
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

        let cropped = rotate_and_crop(
            &input_volume,
            background,
            &rot,
            &centroid,
            &min_v,
            &max_v,
            interpolation_mode,
        );

        let trim_pixels = resolve_trim_pixels(self.config.edge_trim, &cropped, background)?;
        println!("[Info] Edge trim pixels (xy): {}", trim_pixels);
        let cropped = trim_volume_border(&cropped, trim_pixels)?;

        println!(
            "[Info] Cropped output shape=({},{},{})",
            cropped.width, cropped.height, cropped.depth
        );

        let output = Path::new(&self.config.output.path);
        let prefix = self.config.output.folder_prefix.as_deref();
        let ext = self.config.output.folder_extension.as_deref();
        save_tiff_or_folder_with_ext(&cropped, output, prefix, ext)?;
        println!("[Info] Crop pipeline completed. Output written: {}", output.display());
        Ok(())
    }
}
