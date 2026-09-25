use crate::config::placement::ResolvedPlacement;
use crate::error::Result;
use crate::geometry::spatial::{estimate_cell_size, SpatialGrid};
use crate::geometry::{MeshQueryScratch, PreparedMeshQuery, VoidIndex};
use crate::io::{TiffPageEncoder, VolumeNumericType};
use crate::pipeline::placement_feasibility::PlacedParticle;
use crate::types::{BoundingBox, Vec3};
use rayon::prelude::*;
use serde::{Deserialize, Serialize};
use std::fs::File;
use std::io::{BufWriter, Write};
use std::path::{Path, PathBuf};

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

const LABEL_SLAB_VOXELS: usize = 1 << 22;

// AI-FUNC-SUMMARY:
// Purpose: Write the three-phase label field and the per-voxel particle id field.
// Inputs: the resolved config, the voxel size, the placed particles, and the void index.
// Returns: the paths written, in the order they are listed in the header.
// Side effects: Creates directories and writes two TIFF stacks and a JSON header.
// Notes: A voxel is classified by its centre. The void wins wherever both claim it, because the
// void owns any overlap: it is frozen, so a particle crossing into it does not take that volume out
// of the void phase. That is the same rule the record's `overlap_owner` states, applied to voxels.
// Both stacks are computed and encoded in bounded z-slabs of about LABEL_SLAB_VOXELS voxels, so the
// two whole i64 volumes are never resident at once; each voxel is decided from the geometry alone,
// so the files are byte-identical whatever the slab size or thread count. Particles are looked up
// through their bounding boxes rather than tested one by one.
pub fn write_voxel_labels(
    config: &ResolvedPlacement,
    voxel_size: f64,
    placed: &[PlacedParticle],
    void: Option<&VoidIndex>,
) -> Result<Vec<PathBuf>> {
    let domain = config.domain;
    let dims = label_dims(domain, voxel_size)?;
    let [nx, ny, nz] = dims;
    let dir = config.outputs.dir.join("voxel_labels");
    std::fs::create_dir_all(&dir)?;
    let slab_slices = (LABEL_SLAB_VOXELS / (nx * ny).max(1)).max(1);
    let (phase_path, id_path) =
        write_label_stacks(domain, voxel_size, placed, void, &dir, slab_slices)?;

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

// AI-FUNC-SUMMARY: Compute label grid dimensions (round(size/voxel), at least 1 per axis) and reject a voxel count that overflows usize; returns [nx, ny, nz]; side effects: None.
fn label_dims(domain: BoundingBox, voxel_size: f64) -> Result<[usize; 3]> {
    let size = domain.size();
    let nx = ((size.x / voxel_size).round() as usize).max(1);
    let ny = ((size.y / voxel_size).round() as usize).max(1);
    let nz = ((size.z / voxel_size).round() as usize).max(1);
    nx.checked_mul(ny)
        .and_then(|n| n.checked_mul(nz))
        .ok_or_else(|| {
            crate::error::RustMsptError::InvalidConfig("voxel label dimensions overflow".into())
        })?;
    Ok([nx, ny, nz])
}

struct LabelQuery<'a> {
    domain: BoundingBox,
    voxel_size: f64,
    dims: [usize; 3],
    placed: &'a [PlacedParticle],
    prepared: Vec<PreparedMeshQuery<'a>>,
    grid: Option<SpatialGrid>,
    void: Option<&'a VoidIndex>,
}

impl<'a> LabelQuery<'a> {
    // AI-FUNC-SUMMARY: Prepare per-particle mesh queries and the particle bbox grid once for all slabs; returns the query context; side effects: None.
    fn new(
        domain: BoundingBox,
        voxel_size: f64,
        placed: &'a [PlacedParticle],
        void: Option<&'a VoidIndex>,
    ) -> Result<Self> {
        let dims = label_dims(domain, voxel_size)?;
        let size = domain.size();
        let prepared = placed
            .iter()
            .map(|p| PreparedMeshQuery::new(&p.mesh))
            .collect();
        let boxes: Vec<_> = placed.iter().map(|p| p.bbox).collect();
        let cell = estimate_cell_size(&boxes)
            .max(size.x.max(size.y).max(size.z) / (2.0 * (placed.len().max(1) as f64).cbrt()));
        let grid = (!placed.is_empty()).then(|| {
            SpatialGrid::build(
                &boxes.iter().copied().enumerate().collect::<Vec<_>>(),
                domain,
                cell,
            )
        });
        Ok(Self {
            domain,
            voxel_size,
            dims,
            placed,
            prepared,
            grid,
            void,
        })
    }

    // AI-FUNC-SUMMARY: World position of a voxel centre; returns Vec3; side effects: None.
    fn centre(&self, ix: usize, iy: usize, iz: usize) -> Vec3 {
        Vec3::new(
            self.domain.min.x + (ix as f64 + 0.5) * self.voxel_size,
            self.domain.min.y + (iy as f64 + 0.5) * self.voxel_size,
            self.domain.min.z + (iz as f64 + 0.5) * self.voxel_size,
        )
    }

    // AI-FUNC-SUMMARY:
    // Purpose: Classify every voxel of the z-slab starting at z0 into phase and particle-id buffers.
    // Inputs: first slice index and two equal-length buffers holding whole slices.
    // Returns: the number of particle bbox tests performed.
    // Side effects: Overwrites every element of both buffers.
    // Notes: Parallel over 1024-voxel tiles writing disjoint memory; void wins, then the lowest particle index whose solid contains the centre, exactly as a whole-volume pass would decide.
    fn fill_slab(&self, z0: usize, phase: &mut [i64], ids: &mut [i64]) -> usize {
        let [nx, ny, _] = self.dims;
        let slab = nx * ny;
        let offset = z0 * slab;
        phase
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
                    let base = offset + tile * 1024;
                    let end = base + phases.len() - 1;
                    let first = [base % nx, (base / nx) % ny, base / slab];
                    let last = [end % nx, (end / nx) % ny, end / slab];
                    let lo = self.centre(
                        if base / nx == end / nx { first[0] } else { 0 },
                        if first[2] == last[2] { first[1] } else { 0 },
                        first[2],
                    );
                    let hi = self.centre(
                        if base / nx == end / nx {
                            last[0]
                        } else {
                            nx - 1
                        },
                        if first[2] == last[2] { last[1] } else { ny - 1 },
                        last[2],
                    );
                    if let Some(grid) = &self.grid {
                        grid.query_into(BoundingBox { min: lo, max: hi }, 0.0, usize::MAX, spatial);
                    } else {
                        spatial.neighbors.clear();
                    }
                    let candidates = &mut spatial.neighbors;
                    candidates.sort_unstable();
                    let mut checks = 0usize;
                    for (local, (phase, id)) in phases.iter_mut().zip(indices.iter_mut()).enumerate() {
                        let flat = base + local;
                        let p = self.centre(flat % nx, (flat / nx) % ny, flat / slab);
                        *id = 0;
                        if self.void.is_some_and(|index| index.contains_point(p)) {
                            *phase = PHASE_VOID as i64;
                            continue;
                        }
                        let (hit, point_checks) =
                            particle_at_prepared(self.placed, &self.prepared, candidates, p, scratch);
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
            .sum()
    }
}

// AI-FUNC-SUMMARY:
// Purpose: Stream the phase (u8) and particle-id (u32) label stacks to dir/phase.tiff and dir/particle_id.tiff slab by slab.
// Inputs: domain, voxel size, placed particles, optional void, output directory, and slices per slab (>= 1; need not divide the depth).
// Returns: the phase and particle-id paths.
// Side effects: Creates/overwrites both TIFF files, flushing each explicitly; prints the bbox-test count.
// Notes: Peak label memory is two slab buffers (2 * slab_slices * nx * ny i64) instead of two whole volumes. Pages are appended in z order through the same page encoder as save_tiff_or_folder, so the bytes match the whole-volume path. On error both files may be left partially written.
pub(crate) fn write_label_stacks(
    domain: BoundingBox,
    voxel_size: f64,
    placed: &[PlacedParticle],
    void: Option<&VoidIndex>,
    dir: &Path,
    slab_slices: usize,
) -> Result<(PathBuf, PathBuf)> {
    let query = LabelQuery::new(domain, voxel_size, placed, void)?;
    let [nx, ny, nz] = query.dims;
    let slab_slices = slab_slices.clamp(1, nz);
    let phase_path = dir.join("phase.tiff");
    let id_path = dir.join("particle_id.tiff");
    let mut phase_file = BufWriter::new(File::create(&phase_path)?);
    let mut id_file = BufWriter::new(File::create(&id_path)?);
    let mut checks = 0usize;
    {
        let mut phase_pages =
            TiffPageEncoder::new(&mut phase_file, nx, ny, VolumeNumericType::U8)?;
        let mut id_pages = TiffPageEncoder::new(&mut id_file, nx, ny, VolumeNumericType::U32)?;
        let mut phase = vec![0i64; slab_slices * nx * ny];
        let mut ids = vec![0i64; slab_slices * nx * ny];
        for z0 in (0..nz).step_by(slab_slices) {
            let len = slab_slices.min(nz - z0) * nx * ny;
            checks += query.fill_slab(z0, &mut phase[..len], &mut ids[..len]);
            phase_pages.write_slices(&phase[..len])?;
            id_pages.write_slices(&ids[..len])?;
        }
    }
    phase_file.flush()?;
    id_file.flush()?;
    println!(
        "[Info] Voxel label queries: voxels={}, particles={}, bbox_tests={checks}, slab_slices={slab_slices}",
        nx * ny * nz,
        placed.len()
    );
    Ok((phase_path, id_path))
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
    // AI-FUNC-SUMMARY: Build a box particle for label tests with the given acceptance index and bounds.
    fn box_particle(acceptance_index: usize, bbox: BoundingBox) -> PlacedParticle {
        PlacedParticle {
            acceptance_index,
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
    }

    // AI-FUNC-SUMMARY: Prove the slab writer's phase/id TIFF bytes equal a whole-volume Volume3D + save_tiff_or_folder reference for slab sizes 1, non-dividing, dividing, whole and oversized, with void-wins, touching particles and odd dimensions.
    #[test]
    fn slab_label_stacks_are_byte_identical_to_whole_volume_output() {
        use crate::io::{save_tiff_or_folder, Volume3D};
        let domain = BoundingBox {
            min: Vec3::new(-1.0, 0.5, 2.0),
            max: Vec3::new(6.0, 5.5, 9.5),
        };
        let voxel = 0.25;
        let placed: Vec<_> = [
            (3, (-0.5, 1.0, 2.5), (1.5, 3.0, 5.0)),
            (1, (1.5, 1.0, 2.5), (3.0, 3.0, 5.0)),
            (7, (3.2, 2.0, 4.0), (5.5, 5.2, 9.0)),
            (0, (0.0, 3.5, 7.0), (2.0, 5.0, 9.4)),
        ]
        .into_iter()
        .map(|(id, a, b)| {
            box_particle(
                id,
                BoundingBox {
                    min: Vec3::new(a.0, a.1, a.2),
                    max: Vec3::new(b.0, b.1, b.2),
                },
            )
        })
        .collect();
        let void = VoidIndex::build(&crate::geometry::icosphere_mesh(Vec3::new(4.0, 3.5, 6.5), 1.3, 2))
            .unwrap();
        let temp = tempfile::tempdir().unwrap();
        let [nx, ny, nz] = label_dims(domain, voxel).unwrap();
        assert_eq!([nx, ny, nz], [28, 20, 30]);
        let query = LabelQuery::new(domain, voxel, &placed, Some(&void)).unwrap();
        let mut phase = vec![0i64; nx * ny * nz];
        let mut ids = vec![0i64; nx * ny * nz];
        query.fill_slab(0, &mut phase, &mut ids);
        for (code, want) in [(PHASE_MATRIX, true), (PHASE_PARTICLE, true), (PHASE_VOID, true)] {
            assert_eq!(phase.contains(&(code as i64)), want);
        }
        assert!(phase.iter().zip(&ids).all(|(&p, &i)| (p == PHASE_PARTICLE as i64) == (i != 0)));
        let reference = temp.path().join("reference");
        for (name, data, ty) in [
            ("phase.tiff", phase, VolumeNumericType::U8),
            ("particle_id.tiff", ids, VolumeNumericType::U32),
        ] {
            let volume = Volume3D {
                width: nx,
                height: ny,
                depth: nz,
                data,
                numeric_type: ty,
            };
            save_tiff_or_folder(&volume, &reference.join(name), None).unwrap();
        }
        for slab in [1, 2, 3, 4, 7, 29, 30, 31, 1000] {
            let dir = temp.path().join(format!("slab{slab}"));
            std::fs::create_dir_all(&dir).unwrap();
            write_label_stacks(domain, voxel, &placed, Some(&void), &dir, slab).unwrap();
            for name in ["phase.tiff", "particle_id.tiff"] {
                assert_eq!(
                    std::fs::read(dir.join(name)).unwrap(),
                    std::fs::read(reference.join(name)).unwrap(),
                    "slab={slab} file={name}"
                );
            }
        }
    }

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
