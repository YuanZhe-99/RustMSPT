use crate::config::placement::ResolvedPlacement;
use crate::error::Result;
use crate::geometry::spatial::{estimate_cell_size, SpatialGrid};
use crate::geometry::{MeshQueryScratch, PreparedMeshQuery, VoidIndex};
use crate::io::{save_tiff_or_folder, Volume3D, VolumeNumericType};
use crate::pipeline::placement_feasibility::PlacedParticle;
use crate::types::{BoundingBox, Vec3};
use rayon::prelude::*;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// Phase codes in the written label field.
pub const PHASE_MATRIX: u8 = 0;
pub const PHASE_PARTICLE: u8 = 1;
pub const PHASE_VOID: u8 = 2;

/// What the label stacks are, so a reader need not infer it from the arrays.
///
/// `Volume3D` carries neither spacing nor origin - it is pure index space - so
/// without this header the voxels could not be placed back in the run's frame.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VoxelLabelsHeader {
    pub schema_version: String,
    pub unit: String,
    pub voxel_size: f64,
    /// The world position of the centre of voxel (0, 0, 0).
    pub origin: [f64; 3],
    pub dims: [usize; 3],
    /// How the flat arrays are laid out. z-major is what `Volume3D` uses
    /// throughout this crate: index = z * width * height + y * width + x.
    pub axis_order: String,
    pub index_formula: String,
    pub phases: Vec<PhaseLabel>,
    pub particle_id: String,
    pub files: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PhaseLabel {
    pub id: u8,
    pub name: String,
}

// AI-FUNC-SUMMARY:
// Purpose: Write the three-phase label field and the per-voxel particle id field.
// Inputs: the resolved config, the voxel size, the placed particles, and the void index.
// Returns: the paths written, in the order they are listed in the header.
// Side effects: Creates directories and writes two TIFF stacks and a JSON header.
// Notes: A voxel is classified by its centre. The void wins wherever both claim it, because the
// void owns any overlap: it is frozen, so a particle crossing into it does not take that volume out
// of the void phase. That is the same rule the record's `overlap_owner` states, applied to voxels.
// Parallel over z-slabs writing disjoint memory, so the result is bit-identical whatever the thread
// count: each voxel is decided from the geometry alone, with no shared state.
// Particles are looked up through their bounding boxes rather than tested one by one - a domain of
// a few hundred voxels a side against a few hundred particles is otherwise a hundred million
// point-in-mesh tests.
pub fn write_voxel_labels(
    config: &ResolvedPlacement,
    voxel_size: f64,
    placed: &[PlacedParticle],
    void: Option<&VoidIndex>,
) -> Result<Vec<PathBuf>> {
    let domain = config.domain;
    let size = domain.size();
    let nx = ((size.x / voxel_size).round() as usize).max(1);
    let ny = ((size.y / voxel_size).round() as usize).max(1);
    let nz = ((size.z / voxel_size).round() as usize).max(1);

    let centre = |ix: usize, iy: usize, iz: usize| {
        Vec3::new(
            domain.min.x + (ix as f64 + 0.5) * voxel_size,
            domain.min.y + (iy as f64 + 0.5) * voxel_size,
            domain.min.z + (iz as f64 + 0.5) * voxel_size,
        )
    };

    let total = nx
        .checked_mul(ny)
        .and_then(|n| n.checked_mul(nz))
        .ok_or_else(|| {
            crate::error::RustMsptError::InvalidConfig("voxel label dimensions overflow".into())
        })?;
    let mut phase = vec![0i64; total];
    let mut ids = vec![0i64; total];
    let prepared: Vec<_> = placed
        .iter()
        .map(|p| PreparedMeshQuery::new(&p.mesh))
        .collect();
    let boxes: Vec<_> = placed.iter().map(|p| p.bbox).collect();
    let cell = estimate_cell_size(&boxes)
        .max(size.x.max(size.y).max(size.z) / (2.0 * (placed.len().max(1) as f64).cbrt()));
    let grid = if placed.is_empty() {
        None
    } else {
        Some(SpatialGrid::build(
            &boxes.iter().copied().enumerate().collect::<Vec<_>>(),
            domain,
            cell,
        ))
    };
    let slab = nx * ny;
    let checks: usize = phase
        .par_chunks_mut(1024)
        .zip(ids.par_chunks_mut(1024))
        .enumerate()
        .map_init(
            || {
                (
                    MeshQueryScratch::default(),
                    crate::geometry::spatial::SpatialQueryScratch::default(),
                )
            },
            |(scratch, spatial), (tile, (phases, indices))| {
                let base = tile * 1024;
                let end = base + phases.len() - 1;
                let first = [base % nx, (base / nx) % ny, base / slab];
                let last = [end % nx, (end / nx) % ny, end / slab];
                let lo = centre(
                    if base / nx == end / nx { first[0] } else { 0 },
                    if first[2] == last[2] { first[1] } else { 0 },
                    first[2],
                );
                let hi = centre(
                    if base / nx == end / nx {
                        last[0]
                    } else {
                        nx - 1
                    },
                    if first[2] == last[2] { last[1] } else { ny - 1 },
                    last[2],
                );
                if let Some(grid) = &grid {
                    grid.query_into(BoundingBox { min: lo, max: hi }, 0.0, usize::MAX, spatial);
                } else {
                    spatial.neighbors.clear();
                }
                let candidates = &mut spatial.neighbors;
                candidates.sort_unstable();
                let mut checks = 0usize;
                for (offset, (phase, id)) in phases.iter_mut().zip(indices.iter_mut()).enumerate() {
                    let flat = base + offset;
                    let p = centre(flat % nx, (flat / nx) % ny, flat / slab);
                    if void.is_some_and(|index| index.contains_point(p)) {
                        *phase = PHASE_VOID as i64;
                        continue;
                    }
                    let (hit, point_checks) =
                        particle_at_prepared(placed, &prepared, candidates, p, scratch);
                    checks += point_checks;
                    if let Some(hit) = hit {
                        *phase = PHASE_PARTICLE as i64;
                        *id = hit as i64 + 1;
                    } else {
                        *phase = PHASE_MATRIX as i64;
                    }
                }
                checks
            },
        )
        .sum();
    println!(
        "[Info] Voxel label queries: voxels={total}, particles={}, bbox_tests={checks}",
        placed.len()
    );

    let dir = config.outputs.dir.join("voxel_labels");
    std::fs::create_dir_all(&dir)?;

    let phase_volume = Volume3D {
        width: nx,
        height: ny,
        depth: nz,
        data: phase,
        numeric_type: VolumeNumericType::U8,
    };
    let id_volume = Volume3D {
        width: nx,
        height: ny,
        depth: nz,
        data: ids,
        numeric_type: VolumeNumericType::U32,
    };
    let phase_path = dir.join("phase.tiff");
    let id_path = dir.join("particle_id.tiff");
    save_tiff_or_folder(&phase_volume, &phase_path, None)?;
    save_tiff_or_folder(&id_volume, &id_path, None)?;

    let header = VoxelLabelsHeader {
        schema_version: "rustmspt.placement.voxel_labels/1".to_string(),
        unit: config.unit.clone(),
        voxel_size,
        origin: [
            domain.min.x + 0.5 * voxel_size,
            domain.min.y + 0.5 * voxel_size,
            domain.min.z + 0.5 * voxel_size,
        ],
        dims: [nx, ny, nz],
        axis_order: "zyx".to_string(),
        index_formula: "index = z * width * height + y * width + x".to_string(),
        phases: vec![
            PhaseLabel {
                id: PHASE_MATRIX,
                name: "matrix".to_string(),
            },
            PhaseLabel {
                id: PHASE_PARTICLE,
                name: "particle".to_string(),
            },
            PhaseLabel {
                id: PHASE_VOID,
                name: "void".to_string(),
            },
        ],
        particle_id: "0 means no particle; otherwise acceptance_index + 1".to_string(),
        files: vec!["phase.tiff".to_string(), "particle_id.tiff".to_string()],
    };
    let header_path = dir.join("voxel_labels.json");
    crate::pipeline::placement_outputs::write_json(&header_path, &header)?;

    Ok(vec![phase_path, id_path, header_path])
}

// AI-FUNC-SUMMARY:
// Purpose: Find which placed particle, if any, contains a point.
// Inputs: the placed particles and the point.
// Returns: the acceptance index of the containing particle, or None.
// Side effects: None.
// Notes: The bounding-box test comes first, so the ray cast runs only for the handful of particles
// whose box holds the point. Particles are checked in acceptance order and the first containing one
// wins; they never overlap, so at most one can, and the order only decides which is found first at
// a shared boundary.
#[cfg(test)]
fn particle_at(placed: &[PlacedParticle], p: Vec3) -> Option<usize> {
    placed
        .iter()
        .filter(|q| q.bbox.contains_point(p))
        .find(|q| point_in_particle(&q.mesh, q.bbox, p))
        .map(|q| q.acceptance_index)
}

// AI-FUNC-SUMMARY: Ray-parity containment for one particle mesh; returns bool; side effects: none.
// Notes: Shares the void index's ray direction and hit tolerance, so a point on a shared surface is
// not claimed by both phases through disagreeing conventions.
#[cfg(test)]
fn point_in_particle(mesh: &crate::types::Mesh, bbox: BoundingBox, p: Vec3) -> bool {
    if !bbox.expanded(1e-12).contains_point(p) {
        return false;
    }
    crate::geometry::point_inside_mesh(mesh, p)
}

// AI-FUNC-SUMMARY: Query a tile's sorted particle indices using cached mesh views; return the first acceptance ID and actual bbox checks while preserving original slice-order priority.
fn particle_at_prepared(
    placed: &[PlacedParticle],
    prepared: &[PreparedMeshQuery<'_>],
    candidates: &[usize],
    p: Vec3,
    scratch: &mut MeshQueryScratch,
) -> (Option<usize>, usize) {
    let mut checks = 0;
    let hit = candidates.iter().find_map(|&i| {
        checks += 1;
        (placed[i].bbox.contains_point(p) && prepared[i].contains_point(p, scratch))
            .then_some(placed[i].acceptance_index)
    });
    (hit, checks)
}

#[cfg(test)]
mod tests {
    use super::*;
    // AI-FUNC-SUMMARY: Verify indexed prepared label queries against the retained full-scan oracle, including a shared boundary and original slice-order ownership.
    #[test]
    fn prepared_particle_labels_match_original() {
        let placed: Vec<_> = [0.0, 1.0, 4.0]
            .into_iter()
            .enumerate()
            .map(|(i, x)| {
                let bbox = BoundingBox {
                    min: Vec3::new(x, 0.0, 0.0),
                    max: Vec3::new(x + 1.0, 1.0, 1.0),
                };
                PlacedParticle {
                    acceptance_index: 10 - i,
                    source_index: 0,
                    shell_index: 0,
                    scale: 1.0,
                    rotation: crate::geometry::UnitQuat::identity(),
                    translation: Vec3::new(0.0, 0.0, 0.0),
                    equivalent_diameter: 1.0,
                    reach: 1.0,
                    size_class: 0,
                    volume_full: 1.0,
                    volume_in_domain: 1.0,
                    void_overlap_volume: 0.0,
                    clipped_faces: Vec::new(),
                    bbox,
                    mesh: crate::geometry::box_mesh(bbox),
                    shape: None,
                    triangle_range: (0, 12),
                }
            })
            .collect();
        let prepared: Vec<_> = placed
            .iter()
            .map(|p| PreparedMeshQuery::new(&p.mesh))
            .collect();
        let domain = BoundingBox::from_size(Vec3::new(6.0, 2.0, 2.0));
        let boxes: Vec<_> = placed
            .iter()
            .enumerate()
            .map(|(i, p)| (i, p.bbox))
            .collect();
        let grid = SpatialGrid::build(&boxes, domain, 1.0);
        let mut scratch = MeshQueryScratch::default();
        for ix in 0..=120 {
            let p = Vec3::new(ix as f64 / 20.0, 0.5, 0.5);
            let mut ids = grid.query_neighbors(BoundingBox { min: p, max: p }, usize::MAX);
            ids.sort_unstable();
            assert_eq!(
                particle_at_prepared(&placed, &prepared, &ids, p, &mut scratch).0,
                particle_at(&placed, p)
            );
        }
    }
}
