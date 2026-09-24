use crate::config::placement::ShapeFilters;
use crate::error::{Result, RustMsptError};
use crate::geometry::{
    mesh_bbox, mesh_closedness, mesh_metrics, mesh_signed_volume, mesh_volume_centroid,
    split_mesh_into_granules, translate_mesh,
};
use crate::io::{load_stl_hashed, sha256_bytes};
use crate::types::{BoundingBox, Mesh, Vec3};
use std::path::PathBuf;

/// One candidate shape: a closed shell from a source file, measured and centred.
#[derive(Debug, Clone)]
pub struct ShapeShell {
    /// Which entry of `shapes.files` this came from. List order, not directory order.
    pub source_index: usize,
    /// Which shell within that file, in first-face order.
    pub shell_index: usize,
    /// sha256 over the shell's own geometry, so a file re-exported with its faces
    /// in another order is a detectably different shape rather than a silently
    /// different one at the same ordinal.
    pub shell_sha256: String,
    /// The shell translated so its volume centroid sits at the origin. Scaling and
    /// rotating this is the placement transform, with no pivot bookkeeping left.
    pub canonical: Mesh,
    /// The volume centroid in the source file's own coordinates. Recorded, so a
    /// reader reconstructs from the number we used rather than recomputing one.
    pub centroid: Vec3,
    pub volume: f64,
    pub surface_area: f64,
    pub equivalent_diameter: f64,
    pub sphericity: f64,
    /// Distance from the centroid to the furthest vertex. Orientation-independent,
    /// which is what lets the domain be eroded by one number regardless of how the
    /// particle ends up turned.
    pub bounding_radius: f64,
    pub bbox: BoundingBox,
}

/// A source file the library was built from.
#[derive(Debug, Clone)]
pub struct ShapeSource {
    pub index: usize,
    pub path: PathBuf,
    pub path_as_written: String,
    pub sha256: String,
    pub bytes: u64,
    pub shells_found: usize,
    pub shells_kept: usize,
}

/// A shell that was read but not kept, and why.
#[derive(Debug, Clone)]
pub struct RejectedShell {
    pub source_index: usize,
    pub shell_index: usize,
    pub reason: String,
}

/// Every shape a run may draw from, plus what was read and not kept.
#[derive(Debug, Clone)]
pub struct ShapeLibrary {
    pub sources: Vec<ShapeSource>,
    pub shells: Vec<ShapeShell>,
    pub rejected: Vec<RejectedShell>,
    /// The largest ratio of bounding radius to half the equivalent diameter over
    /// the library. Scaling a shell to a target diameter preserves its shape, so a
    /// value well above 1 means a "20 micron particle" reaches much further than
    /// 10 microns from its centre - which is what decides whether it fits a channel.
    pub max_extent_ratio: f64,
}

// AI-FUNC-SUMMARY:
// Purpose: Load every listed STL, split it into closed shells, measure them, and apply the library filters.
// Inputs: the resolved shape file paths, the paths as written, and the optional filters.
// Returns: the library, or an error when a file cannot be read or a shell is not a closed solid.
// Side effects: Reads and hashes each STL in the same forward pass; binary input uses bounded record buffering.
// Notes: An unclosed shell is a refusal, not a skip. Volume, centroid and equivalent diameter are
// meaningless for an open surface, so quietly dropping one would change the size distribution a run
// actually drew from without saying so. The filters are a different matter: aspect and sharpness
// ratios are scale-invariant, so a shell that fails them fails at every size and is reported as
// rejected rather than refused.
// Shell order is first-face order within a file and list order across files, which is what makes
// (source_index, shell_index) reproducible. Directory listing order is never used: it is
// OS-dependent, and the config lists files explicitly for exactly this reason.
pub fn load_shape_library(
    files: &[PathBuf],
    paths_as_written: &[String],
    filters: Option<&ShapeFilters>,
) -> Result<ShapeLibrary> {
    let mut sources = Vec::with_capacity(files.len());
    let mut shells = Vec::new();
    let mut rejected = Vec::new();

    for (source_index, path) in files.iter().enumerate() {
        let (mesh, sha256, bytes) = load_stl_hashed(path).map_err(|e| {
            if matches!(e, RustMsptError::Io(_)) {
                RustMsptError::InvalidConfig(format!("placement.shapes.files[{source_index}]: cannot read {}: {e}", path.display()))
            } else { e }
        })?;
        let granules = split_mesh_into_granules(&mesh);
        let shells_found = granules.len();
        if shells_found == 0 {
            return Err(RustMsptError::InvalidMesh(format!(
                "{} contains no closed shell to place",
                path.display()
            )));
        }

        let mut kept = 0usize;
        for (shell_index, shell) in granules.into_iter().enumerate() {
            if let Err(reason) = mesh_closedness(&shell) {
                return Err(RustMsptError::InvalidMesh(format!(
                    "{}, shell {shell_index}: {reason}. Every shape must be a closed solid: its \
                     volume, centroid and equivalent diameter are what the size distribution and \
                     the placement record are built from, so a shell that has none of them cannot \
                     be quietly skipped without changing what the run drew from.",
                    path.display()
                )));
            }
            let signed = mesh_signed_volume(&shell);
            if signed <= 0.0 {
                return Err(RustMsptError::InvalidMesh(format!(
                    "{}, shell {shell_index}: signed volume is {signed}, so its faces wind inward. \
                     Placement needs outward-facing shells; re-export the file with consistent \
                     outward orientation.",
                    path.display()
                )));
            }
            let Some(metrics) = mesh_metrics(&shell) else {
                return Err(RustMsptError::InvalidMesh(format!(
                    "{}, shell {shell_index}: closed, but its volume or surface area is not a \
                     usable positive number",
                    path.display()
                )));
            };
            let Some(centroid) = mesh_volume_centroid(&shell) else {
                return Err(RustMsptError::InvalidMesh(format!(
                    "{}, shell {shell_index}: has no volume centroid",
                    path.display()
                )));
            };
            let Some(bbox) = mesh_bbox(&shell) else {
                return Err(RustMsptError::InvalidMesh(format!(
                    "{}, shell {shell_index}: has no bounding box",
                    path.display()
                )));
            };

            if let Some(reason) = filter_reason(filters, bbox, &metrics) {
                rejected.push(RejectedShell {
                    source_index,
                    shell_index,
                    reason,
                });
                continue;
            }

            let mut canonical = shell.clone();
            translate_mesh(&mut canonical, centroid.scale(-1.0));
            let bounding_radius = canonical
                .vertices
                .iter()
                .map(|v| v.dot(*v).sqrt())
                .fold(0.0f64, f64::max);

            shells.push(ShapeShell {
                source_index,
                shell_index,
                shell_sha256: shell_geometry_sha256(&shell),
                centroid,
                volume: metrics.volume,
                surface_area: metrics.surface_area,
                equivalent_diameter: metrics.equivalent_diameter,
                sphericity: metrics.sphericity,
                bounding_radius,
                bbox,
                canonical,
            });
            kept += 1;
        }

        sources.push(ShapeSource {
            index: source_index,
            path: path.clone(),
            path_as_written: paths_as_written
                .get(source_index)
                .cloned()
                .unwrap_or_else(|| path.to_string_lossy().to_string()),
            sha256,
            bytes,
            shells_found,
            shells_kept: kept,
        });
    }

    if shells.is_empty() {
        let listed = rejected
            .iter()
            .map(|r| format!("shell {} of file {}: {}", r.shell_index, r.source_index, r.reason))
            .collect::<Vec<_>>()
            .join("; ");
        return Err(RustMsptError::InvalidMesh(format!(
            "the shape library is empty after filtering. Rejections: {}",
            if listed.is_empty() {
                "none recorded".to_string()
            } else {
                listed
            }
        )));
    }

    let max_extent_ratio = shells
        .iter()
        .map(|s| s.bounding_radius / (s.equivalent_diameter * 0.5))
        .fold(0.0f64, f64::max);

    Ok(ShapeLibrary {
        sources,
        shells,
        rejected,
        max_extent_ratio,
    })
}

// AI-FUNC-SUMMARY:
// Purpose: Decide whether a shell fails a library filter, and say which.
// Inputs: the optional filters, the shell's bounding box, and its metrics.
// Returns: Some(reason) when it fails, None when it passes or no filter applies.
// Side effects: None.
// Notes: Both ratios are scale-invariant, which is why they are applied once to the library rather
// than to every rescaled candidate: a shell that fails at its natural size fails at every size.
// The definitions match check_geometry_filters in the original engine, so the same config field
// means the same thing in both.
fn filter_reason(
    filters: Option<&ShapeFilters>,
    bbox: BoundingBox,
    metrics: &crate::geometry::MeshMetrics,
) -> Option<String> {
    let filters = filters?;
    let size = bbox.size();
    let extents = [size.x, size.y, size.z];
    let max_extent = extents.iter().cloned().fold(f64::MIN, f64::max);
    let min_extent = extents.iter().cloned().fold(f64::MAX, f64::min).max(1e-12);

    if let Some(limit) = filters.max_aspect_ratio {
        let ratio = max_extent / min_extent;
        if ratio > limit {
            return Some(format!(
                "bounding-box aspect ratio {ratio:.4} exceeds max_aspect_ratio {limit}"
            ));
        }
    }
    if let Some(limit) = filters.max_sharpness_ratio {
        let ratio =
            metrics.surface_area.powi(3) / (36.0 * std::f64::consts::PI * metrics.volume.powi(2));
        if ratio > limit {
            return Some(format!(
                "sharpness ratio {ratio:.4} exceeds max_sharpness_ratio {limit}"
            ));
        }
    }
    None
}

// AI-FUNC-SUMMARY:
// Purpose: Hash a shell's geometry so the same solid gets the same id however its file was written.
// Inputs: the shell.
// Returns: a lowercase hex sha256.
// Side effects: None.
// Notes: Hashes the vertex coordinates in first-face traversal order, as raw f64 bits, so the
// digest is exact rather than dependent on a text format. A shell ordinal says how a shape was
// found, not which shape it is; re-exporting a file with its faces permuted moves every ordinal
// silently. This digest turns that into something a consumer can check.
fn shell_geometry_sha256(shell: &Mesh) -> String {
    let mut bytes = Vec::with_capacity(shell.faces.len() * 9 * 8);
    for face in &shell.faces {
        for index in [face.a, face.b, face.c] {
            let v = shell.vertices[index];
            bytes.extend_from_slice(&v.x.to_le_bytes());
            bytes.extend_from_slice(&v.y.to_le_bytes());
            bytes.extend_from_slice(&v.z.to_le_bytes());
        }
    }
    sha256_bytes(&bytes)
}



