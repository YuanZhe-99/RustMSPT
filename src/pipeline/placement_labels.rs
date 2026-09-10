use crate::config::placement::ResolvedPlacement;
use crate::error::Result;
use crate::geometry::VoidIndex;
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

    let mut phase = vec![0i64; nx * ny * nz];
    let mut ids = vec![0i64; nx * ny * nz];

    let slab = nx * ny;
    phase
        .par_chunks_mut(slab)
        .zip(ids.par_chunks_mut(slab))
        .enumerate()
        .for_each(|(iz, (phase_slab, id_slab))| {
            for iy in 0..ny {
                for ix in 0..nx {
                    let p = centre(ix, iy, iz);
                    let flat = iy * nx + ix;
                    if let Some(index) = void {
                        if index.contains_point(p) {
                            phase_slab[flat] = PHASE_VOID as i64;
                            // A void voxel never carries a particle id: the void
                            // owns it, so naming a particle there would contradict
                            // the phase beside it.
                            id_slab[flat] = 0;
                            continue;
                        }
                    }
                    if let Some(hit) = particle_at(placed, p) {
                        phase_slab[flat] = PHASE_PARTICLE as i64;
                        // Ids start at 1 so that 0 means "no particle" rather
                        // than "the first one".
                        id_slab[flat] = hit as i64 + 1;
                    } else {
                        phase_slab[flat] = PHASE_MATRIX as i64;
                        id_slab[flat] = 0;
                    }
                }
            }
        });

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
fn point_in_particle(mesh: &crate::types::Mesh, bbox: BoundingBox, p: Vec3) -> bool {
    if !bbox.expanded(1e-12).contains_point(p) {
        return false;
    }
    crate::geometry::point_inside_mesh(mesh, p)
}
