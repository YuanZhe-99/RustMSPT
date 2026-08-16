use super::Pipeline;
use crate::config::meshgen::{FemProfile as ConfigFemProfile, InputKind, MeshGenConfig};
use crate::error::{Result, RustMsptError};
use crate::io::stl::load_stl;
use crate::io::vtu::VtuEncoding;
use smallvec::SmallVec;
use crate::meshgen::features::{detect_features, features_to_doc_with_components};
use crate::meshgen::gapfield::{
    compute_gap_field, gapfield_to_doc, thin_context, GapFieldOptions, Regime, SkipReason,
};
use crate::meshgen::sizing::{
    build_sizing_field, collect_geometry_sources, couple_gap_and_sizing, curve_sources,
    gap_sources, sizing_to_doc, CouplingOptions, LockReason, SizingConstraint, SizingCriterion,
    SizingLookup, SizingOptions, SizingSource,
};
use crate::meshgen::classify::{Side, 
    classified_to_doc, classify_lattice_with, ClassifyOptions, PointClassifier,
};
use crate::meshgen::snap::{snap_lattice, snapped_to_doc, SnapOptions};
use crate::meshgen::cut::{cut_lattice, cut_to_doc, CutOptions};
use crate::meshgen::thin::{band_ladder, FemProfile, LadderOutcome, ThinOptions};
use crate::meshgen::lattice::{
    balance_octree, build_lattice_with_splits, lattice_to_doc, LatticeOptions,
};
use crate::meshgen::snapshot::{emit_snapshot, should_emit, warn_if_large, SnapshotMeta, Stage};
use crate::meshgen::surface::{condition_surface, condition_surface_to_doc_with_components};
use crate::meshgen::arrange::ArrangedCurveKind;
use crate::meshgen::{
    arrange_surface, arranged_surface_to_doc, clip_arranged_to_box, rebuild_topology,
    source_component_is_closed, ArrangeComponent, ArrangeOptions,
};
use crate::types::{Mesh, Vec3};
use std::path::Path;
use std::time::Instant;

/// How many times invariant K1's remedy may refine and rerun S4-S7 before the remaining
/// escalations are handed to S8 (`SPEC_meshgen_geometry.md` §5.1). Each pass halves the
/// element size where capture failed, so three passes cover an 8x range; past that the
/// cost is not justified by what is recovered, and the spec's second clause applies.
const K1_MAX_PASSES: usize = 3;

// AI-FUNC-SUMMARY: Report one pipeline stage's wall time when `RUSTMSPT_TIME_STAGES` is set; returns a fresh Instant for the next stage; side effects: writes to stderr.
fn stage_time(name: &str, started: Instant) -> Instant {
    if std::env::var_os("RUSTMSPT_TIME_STAGES").is_some() {
        eprintln!("[STAGE-TIME] {name} {:?}", started.elapsed());
    }
    Instant::now()
}

// AI-FUNC-SUMMARY:
// Purpose: Pipeline wrapper for the `mesh` subcommand; holds the parsed MeshGenConfig.
// Side effects: None until run().
// Notes: Runs S0, S1, the G2-1..G2-5 arrangement/clip/topology core, G3 (the
//   separation field, thin-region segmentation, and the S3<->S4 coupling driver),
//   S4 (G4-1: the sizing constraint, the graded field, and its octree), S5 (G4-2: the balanced
//   lattice), S6 (G5-1: parity classification) and S7 (G6-1: snap); S8..S11 remain pending.
pub struct MeshGenPipeline {
    pub config: MeshGenConfig,
}

impl MeshGenPipeline {
    // AI-FUNC-SUMMARY:
    // Purpose: Print the resolved generation plan to stdout so the user sees what
    //   the (pending) stages would consume.
    // Side effects: Writes to stdout.
    fn print_plan(&self) {
        let p = &self.config.meshgen;
        println!("[mesh] config validated; {} input(s):", p.inputs.len());
        for input in &p.inputs {
            println!(
                "  - {} (priority {}, kind {})",
                input.stl,
                input.resolved_priority(),
                match input.kind {
                    crate::config::meshgen::InputKind::Auto => "auto",
                    crate::config::meshgen::InputKind::Solid => "solid",
                    crate::config::meshgen::InputKind::Sheet => "sheet",
                }
            );
        }
        println!(
            "  domain [{},{},{}] - [{},{},{}]",
            p.domain.min[0],
            p.domain.min[1],
            p.domain.min[2],
            p.domain.max[0],
            p.domain.max[1],
            p.domain.max[2]
        );
        println!(
            "  sizing h_max_frac={} h_min_frac={} chord_error_frac={} feature_angle_deg={}",
            p.sizing.h_max_frac,
            p.sizing.h_min_frac,
            p.sizing.chord_error_frac,
            p.sizing.feature_angle_deg
        );
        println!(
            "  gaps t_layer_factor={} t_sheet_factor={} confidence_min={}",
            p.gaps.t_layer_factor, p.gaps.t_sheet_factor, p.gaps.confidence_min
        );
        println!(
            "  envelope eps_frac={} repair={:?} coincidence={:?} fem_profile={:?} determinism={:?} snapshots={:?}",
            p.envelope.eps_frac,
            p.repair.level,
            p.coincidence,
            p.fem_profile,
            p.determinism,
            p.snapshots
        );
        println!(
            "  output vtu={} abaqus={} report={}",
            p.output.vtu,
            p.output.abaqus.as_deref().unwrap_or("-"),
            p.output.report.as_deref().unwrap_or("-"),
        );
    }
}

// AI-FUNC-SUMMARY: Compute a deterministic hash over every effective mesh-generation config field for [V12] provenance; returns u64; side effects: none.
fn compute_config_hash(p: &crate::config::meshgen::MeshGenParams) -> u64 {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};
    let mut hasher = DefaultHasher::new();
    for input in &p.inputs {
        input.stl.hash(&mut hasher);
        input.resolved_priority().hash(&mut hasher);
        input.kind.hash(&mut hasher);
    }
    for v in &p.domain.min {
        v.to_bits().hash(&mut hasher);
    }
    for v in &p.domain.max {
        v.to_bits().hash(&mut hasher);
    }
    p.sizing.h_max_frac.to_bits().hash(&mut hasher);
    p.sizing.h_min_frac.to_bits().hash(&mut hasher);
    p.sizing.chord_error_frac.to_bits().hash(&mut hasher);
    p.sizing.feature_angle_deg.to_bits().hash(&mut hasher);
    p.sizing.grading.to_bits().hash(&mut hasher);
    p.sizing.gap_cells.to_bits().hash(&mut hasher);
    p.sizing.curve_cells.to_bits().hash(&mut hasher);
    p.gaps.t_layer_factor.to_bits().hash(&mut hasher);
    p.gaps.t_sheet_factor.to_bits().hash(&mut hasher);
    p.gaps.confidence_min.to_bits().hash(&mut hasher);
    p.envelope.eps_frac.to_bits().hash(&mut hasher);
    p.repair.level.hash(&mut hasher);
    p.coincidence.hash(&mut hasher);
    p.fem_profile.hash(&mut hasher);
    p.determinism.hash(&mut hasher);
    for (component, material) in &p.materials.by_component {
        component.hash(&mut hasher);
        material.hash(&mut hasher);
    }
    let mut region_materials: Vec<_> = p.materials.by_region_key.iter().collect();
    region_materials.sort_by(|left, right| left.0.cmp(right.0));
    for (region, material) in region_materials {
        region.hash(&mut hasher);
        material.hash(&mut hasher);
    }
    format!("{:?}", p.materials.unmapped).hash(&mut hasher);
    format!("{:?}", p.acceleration.mode).hash(&mut hasher);
    p.acceleration.backend.hash(&mut hasher);
    p.acceleration.cpu_fallback.hash(&mut hasher);
    p.acceleration.gpu_min_voxels.hash(&mut hasher);
    p.acceleration.gpu_min_pixels.hash(&mut hasher);
    p.acceleration.gpu_memory_limit_mb.hash(&mut hasher);
    p.acceleration.gpu_prefer_power.hash(&mut hasher);
    p.acceleration.gpu_precision.hash(&mut hasher);
    format!("{:?}", p.snapshots).hash(&mut hasher);
    p.output.vtu.hash(&mut hasher);
    p.output.abaqus.hash(&mut hasher);
    p.output.report.hash(&mut hasher);
    for value in [
        p.verify.max_ar_warn,
        p.verify.min_dihedral_deg,
        p.verify.low_dihedral_deg,
        p.verify.low_dihedral_share,
        p.verify.duplicate_node_tol_frac,
        p.verify.plane_tol_frac,
        p.verify.hanging_tol_frac,
    ] {
        value.map(f64::to_bits).hash(&mut hasher);
    }
    p.verify.max_items_per_section.hash(&mut hasher);
    p.verify.expected_partitions.hash(&mut hasher);
    p.verify.warn_is_fatal.hash(&mut hasher);
    hasher.finish()
}

impl Pipeline for MeshGenPipeline {
    // AI-FUNC-SUMMARY:
    // Purpose: Validate config, run S0/S1, G2-1..G2-5 arrangement+clip+topology, G3-1 gap field, G3-2 regimes + coupling loop against G4-1's constraint, G4-1's sizing field, G4-2's lattice, G5-1's classification, G6-1's snap, G6-2..G6-5's cut and G7-1's band slabs, emit s02..s08, then stop before S9.
    // Returns: Err(NotAvailable) naming S9 through S11, or Err(InvalidMesh) when the FEM-aware ladder rejects a thin region under `forbid_volumetric_fallback`.
    // Side effects: Reads input STLs, prints stage diagnostics, and emits contract-complete snapshots selected by the configured mode.
    // Notes: s02_arranged is emitted after box clipping and topology rebuild are complete; s03_gapfield
    //   carries the S3 `separation_t` point field and s04_sizing the S4 `sizing_h` field, both converted
    //   to output coordinates, and both emit under `snapshots: all`.
    fn run(&self) -> Result<()> {
        let p = &self.config.meshgen;
        p.validate()?;

        let mut meshes: Vec<Mesh> = Vec::with_capacity(p.inputs.len());
        for input in &p.inputs {
            let mesh = load_stl(Path::new(&input.stl)).map_err(|e| {
                RustMsptError::InvalidConfig(format!(
                    "meshgen.inputs: failed to read STL '{}': {e}",
                    input.stl
                ))
            })?;
            meshes.push(mesh);
        }

        self.print_plan();

        let snapshot_mode = p.snapshots;
        let output_path = Path::new(&p.output.vtu);
        let output_domain_min = Vec3::new(p.domain.min[0], p.domain.min[1], p.domain.min[2]);
        let output_domain_max = Vec3::new(p.domain.max[0], p.domain.max[1], p.domain.max[2]);
        let domain_size = output_domain_max.sub(output_domain_min);
        let domain_diag = domain_size.dot(domain_size).sqrt();
        for mesh in &mut meshes {
            for point in &mut mesh.vertices {
                *point = point.sub(output_domain_min).scale(1.0 / domain_diag);
            }
        }
        let domain_min = Vec3::new(0.0, 0.0, 0.0);
        let domain_max = domain_size.scale(1.0 / domain_diag);
        let eps = p.envelope.eps_frac;
        let config_hash = compute_config_hash(p);
        let encoding = VtuEncoding::Ascii;

        // --- S0: Conditioning and repair ---
        let mut clock = Instant::now();
        let cs = condition_surface(&meshes, eps, p.repair.level)?;
        clock = stage_time("S0", clock);
        let mut components = Vec::with_capacity(p.inputs.len());
        for (index, input) in p.inputs.iter().enumerate() {
            let x = index as i32 + 1;
            let closed = source_component_is_closed(&cs, x);
            let priority = u32::try_from(input.resolved_priority()).map_err(|_| {
                RustMsptError::InvalidConfig(format!(
                    "meshgen.inputs[{index}].priority must fit UInt32 for the contract VTU"
                ))
            })?;
            let kind = match input.kind {
                InputKind::Solid => 0,
                InputKind::Sheet => 1,
                InputKind::Auto => u8::from(!closed),
            };
            components.push(ArrangeComponent {
                x,
                priority,
                kind,
                closed,
            });
        }
        if cs.stats.n_degenerate > 0 || cs.stats.n_duplicate > 0 || cs.stats.n_pinhole > 0 {
            println!(
                "[S0] conditioned: {} -> {} verts ({} welded), {} -> {} faces ({} degen, {} dup, {} pinhole, {} orient fixed), {} components",
                cs.stats.n_input_vertices,
                cs.vertices.len(),
                cs.stats.n_welded,
                cs.stats.n_input_faces,
                cs.faces.len(),
                cs.stats.n_degenerate,
                cs.stats.n_duplicate,
                cs.stats.n_pinhole,
                cs.stats.n_orientation_fixed,
                cs.stats.n_components,
            );
        } else {
            println!(
                "[S0] conditioned: {} -> {} verts ({} welded), {} faces, {} components",
                cs.stats.n_input_vertices,
                cs.vertices.len(),
                cs.stats.n_welded,
                cs.faces.len(),
                cs.stats.n_components,
            );
        }
        if !cs.repair_log.actions.is_empty() {
            print!("{}", cs.repair_log.echo());
        }

        if should_emit(snapshot_mode, Stage::Conditioned) {
            let mut doc = condition_surface_to_doc_with_components(&cs, &components);
            for point in &mut doc.points {
                *point = output_domain_min.add(point.scale(domain_diag));
            }
            let meta = SnapshotMeta::new(
                Stage::Conditioned,
                config_hash,
                (output_domain_min, output_domain_max),
            )
            .with_determinism(p.determinism);
            let path = emit_snapshot(&mut doc, output_path, &meta, encoding)?;
            println!("[mesh] snapshot s00: {}", path.display());
        }

        // --- S1: Feature detection ---
        let fs = detect_features(&cs, p.sizing.feature_angle_deg);
        clock = stage_time("S1", clock);
        println!(
            "[S1] features: {} curves, {} junctions, {} corners",
            fs.curves.len(),
            fs.junctions.len(),
            fs.corners.len(),
        );

        if should_emit(snapshot_mode, Stage::Features) {
            let mut doc = features_to_doc_with_components(&cs, &fs, &components);
            for point in &mut doc.points {
                *point = output_domain_min.add(point.scale(domain_diag));
            }
            let meta = SnapshotMeta::new(
                Stage::Features,
                config_hash,
                (output_domain_min, output_domain_max),
            )
            .with_determinism(p.determinism);
            let path = emit_snapshot(&mut doc, output_path, &meta, encoding)?;
            println!("[mesh] snapshot s01: {}", path.display());
        }

        // --- S2/G2-1..G2-3: Arrangement, overlay, and calibrated degeneracy ---
        let arranged = arrange_surface(
            &cs,
            &fs,
            &ArrangeOptions {
                domain_min,
                domain_max,
                eps,
                coincidence: p.coincidence,
                components,
            },
        )?;
        clock = stage_time("S2-arrange", clock);
        println!(
            "[S2/G2-1..G2-3] arrangement: {} candidates, {} proper intersections, {} coplanar overlays, {} registry vertices, {} segments, {} faces, {} point features, {} coincidence events, {} degraded neighborhoods",
            arranged.stats.candidate_pairs,
            arranged.stats.proper_intersections,
            arranged.stats.coplanar_overlays,
            arranged.registry.vertices.len(),
            arranged.registry.segments.len(),
            arranged.faces.len(),
            arranged.point_features.len(),
            arranged.coincidence_events.len(),
            arranged.degraded.len(),
        );
        let mut warnings_by_case = std::collections::BTreeMap::new();
        for warning in &arranged.warnings {
            warnings_by_case
                .entry(warning.case)
                .or_insert_with(Vec::new)
                .push((warning.entities, warning.components.clone()));
        }
        for (case, occurrences) in warnings_by_case {
            println!(
                "[ARR-COINC] WARN {:?} occurrences={} entities/components={:?}",
                case,
                occurrences.len(),
                occurrences,
            );
        }
        for neighborhood in &arranged.degraded {
            println!(
                "[{}] {:?} triangles={:?} rho={:?} provenances={:?}",
                match neighborhood.reason {
                    crate::meshgen::DegradedReason::ResidualCrossing
                    | crate::meshgen::DegradedReason::RadiallyCoplanar
                    | crate::meshgen::DegradedReason::CollapsedContactSegment => "ARR-RESID",
                    _ => "ARR-PREC",
                },
                neighborhood.reason,
                neighborhood.triangles,
                neighborhood.rho,
                neighborhood.provenances,
            );
        }
        // --- G2-5b: Box clip ---
        let mut clipped = clip_arranged_to_box(&arranged, domain_min, domain_max, eps)?;
        clock = stage_time("S2-clip", clock);
        println!(
            "[G2-5b] box clip: {} -> {} faces, {} -> {} curves",
            arranged.faces.len(),
            clipped.faces.len(),
            arranged.curves.len(),
            clipped.curves.len(),
        );

        // --- G2-4: Post-clip topology rebuild + GWN ---
        let topo = rebuild_topology(&clipped);
        clock = stage_time("S2b-topology", clock);
        clipped.components = topo.components.clone();
        for defect in &topo.closure_defects {
            println!(
                "[G2-4/GWN] WARN: component {} classified as solid with closure defect: gwn={:.4}, certainty={:.4}, boundary_edges={}, defect_area={:.6}",
                defect.component,
                defect.gwn,
                defect.certainty,
                defect.boundary_edge_count,
                defect.defect_area,
            );
        }
        let n_solids = topo
            .classifications
            .values()
            .filter(|c| {
                matches!(
                    c,
                    crate::meshgen::ComponentClassification::SolidClosed
                        | crate::meshgen::ComponentClassification::SolidDefective
                )
            })
            .count();
        let n_sheets = topo
            .classifications
            .values()
            .filter(|c| matches!(c, crate::meshgen::ComponentClassification::Sheet))
            .count();
        println!(
            "[G2-4] topology rebuild: {} solid ({} closed, {} defective), {} sheet, {} closure defects",
            n_solids,
            topo.classifications
                .values()
                .filter(|c| matches!(c, crate::meshgen::ComponentClassification::SolidClosed))
                .count(),
            topo.closure_defects.len(),
            n_sheets,
            topo.closure_defects.len(),
        );

        // --- Emit s02_arranged ---
        if should_emit(snapshot_mode, Stage::Arranged) {
            let mut doc = arranged_surface_to_doc(&clipped);
            for point in &mut doc.points {
                *point = output_domain_min.add(point.scale(domain_diag));
            }
            let meta = SnapshotMeta::new(
                Stage::Arranged,
                config_hash,
                (output_domain_min, output_domain_max),
            )
            .with_determinism(p.determinism);
            let path = emit_snapshot(&mut doc, output_path, &meta, encoding)?;
            clock = stage_time("snapshot-s02", clock);
            println!("[mesh] snapshot s02: {}", path.display());
        }

        // --- S3/G3-1: Separation field (rays + closest-pair sweep + battery) ---
        let gap_options = GapFieldOptions {
            domain_min,
            domain_max,
            eps,
            h_bootstrap: p.sizing.h_max_frac,
            t_layer_factor: p.gaps.t_layer_factor,
            t_sheet_factor: p.gaps.t_sheet_factor,
            confidence_min: p.gaps.confidence_min,
            feature_angle_deg: p.sizing.feature_angle_deg,
            ..Default::default()
        };
        let gap_field = compute_gap_field(&clipped, &topo, &gap_options);
        clock = stage_time("S3", clock);
        // Every S3 length is measured in the normalized frame; report in model units.
        println!(
            "[S3/G3-1] gap field: {} samples ({} ray-paired, {} closest pairs, {} densified), {} groups, t_sheet={:.6} t_layer={:.6}",
            gap_field.stats.n_samples,
            gap_field.stats.n_ray_paired,
            gap_field.stats.n_closest_pairs,
            gap_field.stats.n_densified,
            gap_field.stats.n_groups,
            gap_field.t_sheet * domain_diag,
            gap_field.t_layer * domain_diag,
        );
        // --- S3/G3-2: Segmentation, regimes, rims, mid-surfaces, and the S3<->S4 loop ---
        println!(
            "[S3/G3-2] regions: {} total ({} sheet, {} band, {} skipped)",
            gap_field.stats.n_regions,
            gap_field.stats.n_sheet_regions,
            gap_field.stats.n_band_regions,
            gap_field.stats.n_skipped_regions,
        );
        // Skips are aggregated by reason: a coarse input can produce hundreds of
        // one-sample specks, and a wall of identical WARNs hides the real ones.
        let mut skips_by_reason: std::collections::BTreeMap<String, Vec<usize>> =
            std::collections::BTreeMap::new();
        for region in &gap_field.regions {
            if let Some(reason) = region.skip {
                skips_by_reason
                    .entry(format!("{reason:?}"))
                    .or_default()
                    .push(region.id);
            }
        }
        // The five-per-reason cap is right for a run log and wrong for diagnosing why a
        // regime was never reached, which needs every region's own numbers.
        let thin_diag = std::env::var("RUSTMSPT_THIN_DIAG").is_ok_and(|value| value != "0");
        let listed = if thin_diag { usize::MAX } else { 5 };
        for (reason, ids) in &skips_by_reason {
            println!(
                "[THIN-SKIP] WARN: {} region(s) declared thin but kept volumetric ({reason})",
                ids.len(),
            );
            for id in ids.iter().take(listed) {
                let region = &gap_field.regions[*id];
                println!(
                    "  region {} ({:?}, component {} side {} vs patch {}) declared {:?}: t_r={:.6}, t_max={:.6}, area={:.6}, confidence {:.3}, {} samples, failed checks 1-5 {:?}{}",
                    region.id,
                    region.pair_class,
                    region.component,
                    region.side,
                    region.opposite_patch,
                    region.declared_regime,
                    region.t_r * domain_diag,
                    region.t_max * domain_diag,
                    region.area * domain_diag * domain_diag,
                    region.confidence,
                    region.samples.len(),
                    region.failed_checks,
                    region
                        .mid_surface
                        .as_ref()
                        .map(|mid| format!(", mid-surface defects {:?}", mid.defects))
                        .unwrap_or_default(),
                );
            }
            if ids.len() > 5 {
                println!("  ... and {} more", ids.len() - 5);
            }
        }
        for region in &gap_field.regions {
            if region.skip.is_none() && region.regime != Regime::Normal {
                println!(
                    "[THIN] region {} ({:?}, component {} side {} vs patch {}) -> {:?}: t_r={:.6}, confidence {:.3}, {} samples, {} rim loop(s){}",
                    region.id,
                    region.pair_class,
                    region.component,
                    region.side,
                    region.opposite_patch,
                    region.regime,
                    region.t_r * domain_diag,
                    region.confidence,
                    region.samples.len(),
                    region.rims.len(),
                    region
                        .mid_surface
                        .as_ref()
                        .map(|mid| format!(", mid-surface {} triangles", mid.triangles.len()))
                        .unwrap_or_default(),
                );
            }
        }

        // --- S4/G4-1: sizing sources and the graded field ---
        // The geometry-only half of the field (curvature + features) is regime
        // independent, so it is built once and reused by every coupling iteration;
        // only the gap/LFS half depends on what the loop decides.
        let sizing_options = SizingOptions {
            domain_min,
            domain_max,
            h_max: p.sizing.h_max_frac,
            h_min: p.sizing.h_min_frac,
            chord_error_frac: p.sizing.chord_error_frac,
            feature_angle_deg: p.sizing.feature_angle_deg,
            grading: p.sizing.grading,
            gap_cells: p.sizing.gap_cells,
            curve_cells: p.sizing.curve_cells,
            eps,
            ..Default::default()
        };
        // The chord rules read the *input* tessellation (`cs`/`fs`); the curve rule reads
        // S2's arranged curves, because that is where an intersection curve first exists.
        let mut geometry_sources = collect_geometry_sources(&cs, &fs, &sizing_options);
        geometry_sources.extend(curve_sources(&clipped, &sizing_options));
        let geometry_field = SizingLookup::build(geometry_sources.clone(), &sizing_options);
        clock = stage_time("S4-sources", clock);
        let count_of = |criterion: SizingCriterion| {
            geometry_sources
                .iter()
                .filter(|source| source.criterion == criterion)
                .count()
        };
        let n_curvature = count_of(SizingCriterion::Curvature);
        let n_corner = count_of(SizingCriterion::Corner);
        let n_curve = count_of(SizingCriterion::Curve);
        println!(
            "[S4/G4-1] geometry sources: {} total ({} curvature, {} feature curve, {} corner, {} locked curve)",
            geometry_sources.len(),
            n_curvature,
            geometry_sources.len() - n_curvature - n_corner - n_curve,
            n_corner,
            n_curve,
        );

        // The S3<->S4 fixed point, now against the real constraint C(R).
        let coupling_options = CouplingOptions {
            tau_sheet: p.gaps.t_sheet_factor,
            tau_layer: p.gaps.t_layer_factor,
            h_max: p.sizing.h_max_frac,
            h_min: p.sizing.h_min_frac,
            eps,
            ..Default::default()
        };
        // A region S3 declined is volumetric whatever its separation says, so it
        // enters the loop with an infinite `t_r` - `regime_for` reads that as Normal.
        // Its real `t_r` still drives the LFS term through `SizingConstraint`.
        let separations: Vec<f64> = gap_field
            .regions
            .iter()
            .map(|region| {
                if region.skip.is_some() {
                    f64::INFINITY
                } else {
                    region.t_r
                }
            })
            .collect();
        let constraint = SizingConstraint::new(&geometry_field, &gap_field, &sizing_options);
        let coupling =
            couple_gap_and_sizing(&separations, &coupling_options, |regimes, _| {
                constraint.evaluate(regimes)
            })?;
        println!(
            "[S3/S4] coupling: {} iteration(s), converged={}, h={:.6}, t_sheet={:.6}, t_layer={:.6}",
            coupling.iterations,
            coupling.converged,
            coupling.h * domain_diag,
            coupling.t_sheet * domain_diag,
            coupling.t_layer * domain_diag,
        );
        // `C(R)` is one scalar for the whole model, so naming the region that set it is
        // what makes an unexpectedly fine mesh explainable.
        if let Some((region, value, from_gap)) = constraint.binding_region(&coupling.regimes) {
            println!(
                "[S3/S4] constraint bound by region {} ({}): C={:.6}{}",
                region,
                if from_gap {
                    "local feature size"
                } else {
                    "curvature/features"
                },
                value * domain_diag,
                gap_field
                    .regions
                    .get(region)
                    .map(|r| format!(
                        ", t_r={:.6}, {} sample(s){}",
                        r.t_r * domain_diag,
                        r.samples.len(),
                        r.skip
                            .map(|reason| format!(", declined ({reason:?})"))
                            .unwrap_or_default(),
                    ))
                    .unwrap_or_default(),
            );
        }
        if coupling.hit_cap {
            println!(
                "[S3/S4] WARN: iteration cap reached; regions {:?} locked volumetric",
                coupling.locked_for(LockReason::IterationCap),
            );
        }
        for reason in [LockReason::Oscillated, LockReason::Tightened] {
            let locked = coupling.locked_for(reason);
            if !locked.is_empty() {
                println!("[S3/S4] WARN: regions {locked:?} locked ({reason:?})");
            }
        }

        if should_emit(snapshot_mode, Stage::Gapfield) {
            let mut doc = gapfield_to_doc(&clipped, &gap_field);
            for point in &mut doc.points {
                *point = output_domain_min.add(point.scale(domain_diag));
            }
            if let Some(array) = doc
                .point_data
                .iter_mut()
                .find(|array| array.name == "separation_t")
            {
                if let crate::io::vtu::ArrayData::F32(values) = &mut array.data {
                    for value in values.iter_mut() {
                        if *value >= 0.0 {
                            *value = (f64::from(*value) * domain_diag) as f32;
                        }
                    }
                }
            }
            let meta = SnapshotMeta::new(
                Stage::Gapfield,
                config_hash,
                (output_domain_min, output_domain_max),
            )
            .with_determinism(p.determinism);
            let path = emit_snapshot(&mut doc, output_path, &meta, encoding)?;
            clock = stage_time("snapshot-s03", clock);
            println!("[mesh] snapshot s03: {}", path.display());
        }

        // --- S4/G4-1: the graded field and the background octree ---
        // The converted regions are known now, so the LFS sources can skip exactly
        // the gaps a band or sheet template will mesh.
        let effective_regimes: Vec<Regime> = gap_field
            .regions
            .iter()
            .enumerate()
            .map(|(index, region)| {
                if region.skip.is_some() {
                    Regime::Normal
                } else {
                    coupling
                        .regimes
                        .get(index)
                        .copied()
                        .unwrap_or(region.regime)
                }
            })
            .collect();
        // S8b's FEM-aware ladder (PLAN §10.11). It runs here, where a region's measured
        // gap and the converged element size are both known, and it is what makes the
        // `meshgen.thin` gates mean something: each thin region is placed on one of the
        // five rungs and the outcome is reported with the metric that decided it.
        let thin_options = ThinOptions {
            min_dihedral_deg: p.thin.band_min_dihedral_deg,
            max_aspect_ratio: p.thin.band_max_ar,
            fem_profile: match p.fem_profile {
                ConfigFemProfile::Explicit => FemProfile::Explicit,
                _ => FemProfile::Implicit,
            },
            altitude_ratio: p.thin.min_altitude_frac,
            regional_failure_share: p.thin.regional_failure_share,
            forbid_volumetric: p.thin.forbid_volumetric_fallback,
            h_min: sizing_options.h_min,
        };
        // The ladder's verdict is not advice: it *is* the regime S8b works to. Rung 4
        // ("representable but poor") demoting a region to volumetric only in the log
        // while `effective_regimes` still said Band or Sheet would have S8b band or
        // collapse a gap the ladder had already judged unmeshable. Rung 2 is the one
        // outcome that cannot be honoured, because re-running S4 at a finer `h` is not
        // built; the region keeps its declared regime and the rung is reported.
        let mut effective_regimes = effective_regimes;
        if p.thin.enabled {
            let mut outcomes: std::collections::BTreeMap<String, usize> =
                std::collections::BTreeMap::new();
            let mut rejected: Vec<String> = Vec::new();
            for (index, region) in gap_field.regions.iter().enumerate() {
                if region.skip.is_some()
                    || effective_regimes.get(index).copied() == Some(Regime::Normal)
                {
                    continue;
                }
                let decision = band_ladder(
                    region.t_r,
                    coupling.h,
                    coupling.t_sheet,
                    &thin_options,
                );
                if let Some(slot) = effective_regimes.get_mut(index) {
                    match decision.outcome {
                        LadderOutcome::Band => *slot = Regime::Band,
                        LadderOutcome::Sheet => *slot = Regime::Sheet,
                        LadderOutcome::Volumetric => *slot = Regime::Normal,
                        LadderOutcome::RefineLocally | LadderOutcome::Reject => {}
                    }
                }
                *outcomes
                    .entry(format!("{:?}", decision.outcome))
                    .or_insert(0) += 1;
                if decision.outcome == LadderOutcome::Reject {
                    rejected.push(format!(
                        "region {index} (t={:.6}, h={:.6}): {}",
                        region.t_r * domain_diag,
                        coupling.h * domain_diag,
                        decision.reason
                    ));
                }
            }
            if !outcomes.is_empty() {
                println!("[S8b/G7-1] FEM-aware ladder over {} thin region(s): {outcomes:?}", outcomes.values().sum::<usize>());
            }
            for line in &rejected {
                println!("[S8b/G7-1] REJECT {line}");
            }
            if !rejected.is_empty() {
                return Err(RustMsptError::InvalidMesh(format!(
                    "meshgen.thin: {} thin region(s) were rejected by the FEM-aware ladder and \
                     `forbid_volumetric_fallback` leaves no fallback; see the [S8b/G7-1] REJECT \
                     lines above",
                    rejected.len()
                )));
            }
        }

        let mut sources = geometry_sources;
        let lfs_sources = gap_sources(&clipped, &gap_field, &effective_regimes, &sizing_options);
        let n_lfs = lfs_sources.len();
        // A `Speck`/`IntersectionWedge` region is declined AND suppressed as an LFS source
        // (see `gap_sources`); say how much of the field that removed, so a coarse result
        // near a declined region is attributable rather than mysterious.
        let n_lfs_suppressed: usize = gap_field
            .regions
            .iter()
            .filter(|region| {
                matches!(
                    region.skip,
                    Some(SkipReason::Speck) | Some(SkipReason::Undersampled)
                )
            })
            .map(|region| region.samples.len())
            .sum();
        if n_lfs_suppressed > 0 {
            println!(
                "[S4/G4-1] LFS suppressed: {n_lfs_suppressed} sample(s) in regions declined as speck or intersection wedge"
            );
        }
        sources.extend(lfs_sources);

        // --- S4 field -> S5 lattice -> S6 classify -> S7 snap, under invariant K1's
        // refine-and-retry (SPEC_meshgen_geometry.md §5.1).
        //
        // S7 reports two escalation sets it cannot fix in place: edges one component
        // crosses more than once, and locked curve segments no chain of mesh edges
        // covers. The frozen remedy for both is the same - "refine one level, and if the
        // sizing floor is reached, hand the cells to §7" - and it was skipped outright
        // until 2026-08-07, which is why curve coverage measured 0 of 122 on A-3 and a
        // plain cube reproduced none of its twelve sharp edges. Refining the snap radius
        // instead does not work and makes capture worse: the motion cap is
        // `SNAP_MOTION_CAP * l_min` and shrinks with `h`, so a finer mesh snaps *less*
        // unless the elements are actually placed on the curve.
        //
        // Each pass re-asks the sizing field for half the length scale that failed, at
        // the point that failed, and reruns S5-S7 on the result. A request already at
        // `clamp_h`'s floor cannot refine further, so a pass that adds no *binding*
        // source stops the loop and the escalation stands - exactly the spec's second
        // clause. The cap bounds cost, not correctness.
        let mut k1_extra: Vec<SizingSource> = Vec::new();
        let mut k1_passes = 0usize;
        let (sizing_lookup, sizing_field, balanced, lattice, point_classifier, classification, snapped) = loop {
            let mut pass_sources = sources.clone();
            pass_sources.extend(k1_extra.iter().cloned());
            let sizing_lookup = SizingLookup::build(pass_sources, &sizing_options);
            let sizing_field = build_sizing_field(&sizing_lookup, &sizing_options);
            clock = stage_time("S4-field", clock);

            let (balanced, balance_splits) = balance_octree(&sizing_field);
            clock = stage_time("S5-balance", clock);
            let lattice =
                build_lattice_with_splits(&balanced, &LatticeOptions::default(), balance_splits)?;
            clock = stage_time("S5-lattice", clock);

            let point_classifier = PointClassifier::build(
                &clipped,
                &topo,
                &ClassifyOptions {
                    domain_min,
                    domain_max,
                },
            );
            let classification = classify_lattice_with(
                &lattice,
                &point_classifier,
                &clipped,
                &ClassifyOptions {
                    domain_min,
                    domain_max,
                },
            );
            clock = stage_time("S6", clock);

            let snapped = snap_lattice(
                &lattice,
                &clipped,
                &classification,
                &SnapOptions {
                    domain_min,
                    domain_max,
                    eps,
                },
            );
            clock = stage_time("S7", clock);

            // **P-4.7 probe (`RUSTMSPT_SUBCELL_PROBE=1`), print-only.** A body can pass through a
            // cell's interior touching no edge (PLAN §6.14). Refinement and labelling are both
            // closed for it (§6.15, §6.16), leaving only a cut driven by the surface fragment —
            // and the SHAPE of that fragment decides what the cut has to be able to do. Because
            // the body touches no tet edge, its trace on a face cannot reach the face's boundary,
            // so it is a closed loop strictly inside the face. Counting how many of a cell's four
            // faces carry a trace separates the cases: 2 is a body passing through, 0 is a body
            // ending inside the cell (a tip or a blob), and anything else is a shape the simple
            // through-cut cannot assume.
            if std::env::var_os("RUSTMSPT_SUBCELL_PROBE").is_some() {
                let crossed: std::collections::HashSet<[u32; 2]> = snapped
                    .crossings
                    .iter()
                    .map(|c| {
                        let (a, b) = (c.edge[0], c.edge[1]);
                        if a <= b { [a, b] } else { [b, a] }
                    })
                    .collect();
                // Segment where surface triangle `s` crosses the plane of `f`, clipped to `f`.
                let trace = |f: [Vec3; 3], s: [Vec3; 3]| -> bool {
                    let n = f[1].sub(f[0]).cross(f[2].sub(f[0]));
                    let scale = n.dot(n).sqrt();
                    if scale <= 0.0 {
                        return false;
                    }
                    let d: [f64; 3] = [
                        n.dot(s[0].sub(f[0])) / scale,
                        n.dot(s[1].sub(f[0])) / scale,
                        n.dot(s[2].sub(f[0])) / scale,
                    ];
                    let tol = 1.0e-12;
                    if d.iter().all(|x| *x > tol) || d.iter().all(|x| *x < -tol) {
                        return false;
                    }
                    // The two points where `s`'s edges meet the plane.
                    let mut hits: SmallVec<[Vec3; 3]> = SmallVec::new();
                    for k in 0..3 {
                        let (a, b) = (k, (k + 1) % 3);
                        if (d[a] > tol && d[b] < -tol) || (d[a] < -tol && d[b] > tol) {
                            let t = d[a] / (d[a] - d[b]);
                            hits.push(s[a].add(s[b].sub(s[a]).scale(t)));
                        } else if d[a].abs() <= tol {
                            hits.push(s[a]);
                        }
                    }
                    if hits.len() < 2 {
                        return false;
                    }
                    // Does any part of that segment lie inside `f`? Sampled — this is a census of
                    // which faces are touched, not the cut itself.
                    let inside = |q: Vec3| -> bool {
                        let area = |u: Vec3, v: Vec3, w: Vec3| {
                            v.sub(u).cross(w.sub(u)).dot(n) / (scale * scale)
                        };
                        area(f[0], f[1], q) >= -1.0e-9
                            && area(f[1], f[2], q) >= -1.0e-9
                            && area(f[2], f[0], q) >= -1.0e-9
                    };
                    (0..=8).any(|i| {
                        let t = i as f64 / 8.0;
                        inside(hits[0].add(hits[1].sub(hits[0]).scale(t)))
                    })
                };
                let mut hist = [0usize; 5];
                let mut cells = 0usize;
                // The area these cells can possibly contribute to the material boundary: their own
                // outer faces. An upper bound, and the point of it is to compare against `[V13]`'s
                // measured off-surface area before any of this is built.
                let mut face_area = 0.0f64;
                let mut face_area_two = 0.0f64;
                for (index, tet) in lattice.tets.iter().enumerate() {
                    let ambiguous: Vec<i32> = classification.records[index]
                        .entries
                        .iter()
                        .filter(|(_, side)| *side == Side::Ambiguous)
                        .map(|(x, _)| *x)
                        .collect();
                    if ambiguous.is_empty() {
                        continue;
                    }
                    if (0..4).any(|a| {
                        ((a + 1)..4).any(|b| {
                            let (x, y) = (tet[a], tet[b]);
                            crossed.contains(&if x <= y { [x, y] } else { [y, x] })
                        })
                    }) {
                        continue;
                    }
                    let centroid = tet
                        .iter()
                        .fold(Vec3::new(0.0, 0.0, 0.0), |acc, node| {
                            acc.add(snapped.nodes[*node as usize])
                        })
                        .scale(0.25);
                    let mut uncertain = 0usize;
                    if !ambiguous.iter().any(|x| {
                        point_classifier
                            .slot_of(*x)
                            .is_some_and(|slot| point_classifier.inside(centroid, slot, &mut uncertain))
                    }) {
                        continue;
                    }
                    cells += 1;
                    let mut touched = 0usize;
                    for slots in [[0usize, 1, 2], [0, 1, 3], [0, 2, 3], [1, 2, 3]] {
                        let f = [
                            snapped.nodes[tet[slots[0]] as usize],
                            snapped.nodes[tet[slots[1]] as usize],
                            snapped.nodes[tet[slots[2]] as usize],
                        ];
                        let any = clipped.faces.iter().any(|face| {
                            ambiguous.contains(&face.component)
                                && trace(
                                    f,
                                    [
                                        clipped.vertices[face.nodes[0]],
                                        clipped.vertices[face.nodes[1]],
                                        clipped.vertices[face.nodes[2]],
                                    ],
                                )
                        });
                        touched += usize::from(any);
                    }
                    hist[touched] += 1;
                    let area: f64 = [[0usize, 1, 2], [0, 1, 3], [0, 2, 3], [1, 2, 3]]
                        .iter()
                        .map(|slots| {
                            let (a, b, c) = (
                                snapped.nodes[tet[slots[0]] as usize],
                                snapped.nodes[tet[slots[1]] as usize],
                                snapped.nodes[tet[slots[2]] as usize],
                            );
                            let n = b.sub(a).cross(c.sub(a));
                            0.5 * n.dot(n).sqrt()
                        })
                        .sum();
                    face_area += area;
                    if touched == 2 {
                        face_area_two += area;
                    }
                }
                println!(
                    "[SUBCELL-PROBE] {cells} cell(s) with a body through the interior and no edge crossing; \
                     faces carrying a trace: 0 -> {}, 1 -> {}, 2 -> {}, 3 -> {}, 4 -> {}",
                    hist[0], hist[1], hist[2], hist[3], hist[4]
                );
                println!(
                    "[SUBCELL-PROBE] their outer faces total {face_area:.5} of area ({face_area_two:.5} on the two-face cells) - \
                     compare against `[V13]`'s measured off-surface area for the case"
                );
            }

            // A request binds only if it asks for something strictly finer than the
            // field already gives there; otherwise the floor is reached and retrying
            // would spin without changing the mesh.
            let mut binding = 0usize;
            for (point, scale) in &snapped.refine_requests {
                let want = sizing_options.clamp_h(scale * 0.5);
                if want < sizing_lookup.eval(*point) * (1.0 - 1.0e-9) {
                    k1_extra.push(SizingSource {
                        point: *point,
                        h: want,
                        criterion: SizingCriterion::Curve,
                    });
                    binding += 1;
                }
            }
            if binding > 0 && k1_passes < K1_MAX_PASSES {
                k1_passes += 1;
                println!(
                    "[S7/K1] pass {k1_passes}: {} escalation(s) reported, {binding} refinable; rebuilding S4-S7 one level finer",
                    snapped.refine_requests.len()
                );
                continue;
            }
            if binding > 0 {
                println!(
                    "[S7/K1] {} escalation(s) still open after {K1_MAX_PASSES} pass(es); the cells go to S8 (SPEC §5.1 second clause)",
                    snapped.refine_requests.len()
                );
            } else if !snapped.refine_requests.is_empty() {
                println!(
                    "[S7/K1] {} escalation(s) are at the h_min floor and cannot refine further; the cells go to S8 (SPEC §5.1 second clause)",
                    snapped.refine_requests.len()
                );
            }
            break (
                sizing_lookup,
                sizing_field,
                balanced,
                lattice,
                point_classifier,
                classification,
                snapped,
            );
        };

        println!(
            "[S4/G4-1] sizing field: {} sources (+{} LFS), {} leaves, levels 0..{}, h in [{:.6}, {:.6}], grading {:.2}",
            sizing_lookup.len(),
            n_lfs,
            sizing_field.stats.n_leaves,
            sizing_field.stats.max_level_used,
            sizing_field.stats.h_smallest * domain_diag,
            sizing_field.stats.h_largest * domain_diag,
            p.sizing.grading,
        );
        if sizing_field.stats.n_unresolved > 0 {
            println!(
                "[S4/G4-1] WARN: {} leaf/leaves stayed coarser than the field asks for ({}{})",
                sizing_field.stats.n_unresolved,
                if sizing_field.stats.level_capped {
                    format!("depth cap {} reached", sizing_field.max_level)
                } else {
                    String::new()
                },
                if sizing_field.stats.leaf_capped {
                    format!(" leaf budget {} reached", sizing_options.max_leaves)
                } else {
                    String::new()
                },
            );
        }

        if should_emit(snapshot_mode, Stage::Sizing) {
            let mut doc = sizing_to_doc(&sizing_field, &clipped.components);
            for point in &mut doc.points {
                *point = output_domain_min.add(point.scale(domain_diag));
            }
            if let Some(array) = doc
                .point_data
                .iter_mut()
                .find(|array| array.name == "sizing_h")
            {
                if let crate::io::vtu::ArrayData::F32(values) = &mut array.data {
                    for value in values.iter_mut() {
                        if *value >= 0.0 {
                            *value = (f64::from(*value) * domain_diag) as f32;
                        }
                    }
                }
            }
            let meta =
                SnapshotMeta::new(Stage::Sizing, config_hash, (output_domain_min, output_domain_max))
                    .with_determinism(p.determinism);
            let path = emit_snapshot(&mut doc, output_path, &meta, encoding)?;
            clock = stage_time("snapshot-s04", clock);
            println!("[mesh] snapshot s04: {}", path.display());
        }

        // --- S5/G4-2: strong 2:1 balance + Freudenthal/fan tetrahedralization ---
        // (built inside the K1 loop above; reported here on the pass that survived)
        println!(
            "[S5/G4-2] lattice: {} leaves after {} balance split(s) ({} Freudenthal, {} fan), {} nodes, {} tets, V in [{:.3e}, {:.3e}]",
            lattice.stats.n_leaves,
            lattice.stats.n_balance_splits,
            lattice.stats.n_freudenthal,
            lattice.stats.n_fan,
            lattice.stats.n_nodes,
            lattice.stats.n_tets,
            lattice.stats.min_volume * domain_diag.powi(3),
            lattice.stats.max_volume * domain_diag.powi(3),
        );

        let _ = warn_if_large(snapshot_mode, lattice.stats.n_tets);
        if should_emit(snapshot_mode, Stage::Lattice) {
            let sizing_per_cell: Vec<f64> =
                balanced.leaves.iter().map(|leaf| leaf.h).collect();
            let mut doc = lattice_to_doc(&lattice, &sizing_per_cell, &clipped.components);
            for point in &mut doc.points {
                *point = output_domain_min.add(point.scale(domain_diag));
            }
            if let Some(array) = doc
                .point_data
                .iter_mut()
                .find(|array| array.name == "sizing_h")
            {
                if let crate::io::vtu::ArrayData::F32(values) = &mut array.data {
                    for value in values.iter_mut() {
                        if *value >= 0.0 {
                            *value = (f64::from(*value) * domain_diag) as f32;
                        }
                    }
                }
            }
            let meta = SnapshotMeta::new(
                Stage::Lattice,
                config_hash,
                (output_domain_min, output_domain_max),
            )
            .with_determinism(p.determinism);
            let path = emit_snapshot(&mut doc, output_path, &meta, encoding)?;
            clock = stage_time("snapshot-s05", clock);
            println!("[mesh] snapshot s05: {}", path.display());
        }

        // --- S6/G5-1: parity classification, records, active patches ---
        // Built inside the K1 loop and shared with S8, which needs the same inside/outside
        // decision at points the lattice has no vertex at (SPEC §7.5).
        println!(
            "[S6/G5-1] classification: {} vertices x {} solid component(s) ({} sheet, {} defective), {} inside pair(s); tets {} background / {} owned / {} straddling; {} region key(s); {} inactive face(s)",
            classification.stats.n_vertices,
            classification.stats.n_solid_components,
            classification.stats.n_sheet_components,
            classification.stats.n_defective_components,
            classification.stats.n_inside_pairs,
            classification.stats.n_tets_background,
            classification.stats.n_tets_owned,
            classification.stats.n_tets_ambiguous,
            classification.region_sets.len(),
            classification.stats.n_inactive_faces,
        );
        println!(
            "[S6/G5-1] rays: {} first-shot, {} re-shot, {} exhausted (winding-number fallback), {} by winding number on a defective component (by design), {} exact-predicate escalation(s)",
            classification.stats.n_first_ray,
            classification.stats.n_reshoots,
            classification.stats.n_gwn_fallback,
            classification.stats.n_defective_gwn,
            classification.stats.n_filter_uncertain,
        );
        for warning in &classification.warnings {
            println!("{warning}");
        }

        if should_emit(snapshot_mode, Stage::Classified) {
            let mut doc = classified_to_doc(&lattice, &classification, &clipped.components);
            for point in &mut doc.points {
                *point = output_domain_min.add(point.scale(domain_diag));
            }
            let meta = SnapshotMeta::new(
                Stage::Classified,
                config_hash,
                (output_domain_min, output_domain_max),
            )
            .with_determinism(p.determinism);
            let path = emit_snapshot(&mut doc, output_path, &meta, encoding)?;
            clock = stage_time("snapshot-s06", clock);
            println!("[mesh] snapshot s06: {}", path.display());
        }

        // --- S7/G6-1: snap --- (run inside the K1 loop above)
        println!(
            "[S7/G6-1] snap: {} crossing(s) on {} of {} edge(s); {} candidate(s) -> {} corner / {} curve / {} surface; {} on-cut node(s)",
            snapped.stats.n_crossings,
            snapped.stats.n_crossed_edges,
            snapped.stats.n_edges,
            snapped.stats.n_candidates,
            snapped.stats.n_snapped_corner,
            snapped.stats.n_snapped_curve,
            snapped.stats.n_snapped_surface,
            snapped.stats.n_on_cut,
        );
        println!(
            "[S7/G6-1] moves: {} capped, {} rejected, {} box-constrained, {} promoted by the re-check ({} residual); motion max {:.3e}, mean {:.3e}",
            snapped.stats.n_capped,
            snapped.stats.n_rejected,
            snapped.stats.n_box_constrained,
            snapped.stats.n_recheck_promoted,
            snapped.stats.n_recheck_residual,
            snapped.stats.max_motion * domain_diag,
            snapped.stats.mean_motion * domain_diag,
        );
        println!(
            "[S7/G6-1] curve coverage: {} of {} locked segment(s) carried by a chain of mesh edges",
            snapped.stats.n_curve_segments_covered, snapped.stats.n_curve_segments,
        );
        for warning in &snapped.warnings {
            println!("{warning}");
        }

        if should_emit(snapshot_mode, Stage::Snapped) {
            let mut doc =
                snapped_to_doc(&lattice, &snapped, &classification, &clipped.components);
            for point in &mut doc.points {
                *point = output_domain_min.add(point.scale(domain_diag));
            }
            if let Some(array) = doc
                .point_data
                .iter_mut()
                .find(|array| array.name == "snap_motion")
            {
                if let crate::io::vtu::ArrayData::F32(values) = &mut array.data {
                    for value in values.iter_mut() {
                        *value = (f64::from(*value) * domain_diag) as f32;
                    }
                }
            }
            let meta = SnapshotMeta::new(
                Stage::Snapped,
                config_hash,
                (output_domain_min, output_domain_max),
            )
            .with_determinism(p.determinism);
            let path = emit_snapshot(&mut doc, output_path, &meta, encoding)?;
            clock = stage_time("snapshot-s07", clock);
            println!("[mesh] snapshot s07: {}", path.display());
        }

        // **Do not re-classify on the snapped positions.** S6's labels are stale by
        // exactly the snap, and the correlation is exact - a sphere and a cube snap
        // nothing and disagree about nothing; a strut lattice moves 2,060 nodes and
        // disagrees on 30,904 cells, which is 2,060 times the ~15 tets a Freudenthal node
        // touches. Re-running `classify_lattice` on `snapped.nodes` was tried
        // (2026-08-06): it holds conformity but moves the disagreement only 30,904 ->
        // 30,892 and makes component volume error *worse*, 32.3% -> 44.3%. The reason is
        // that a node snapped exactly onto a surface has no well-defined parity - the ray
        // test answers arbitrarily for a point *on* the boundary - so re-testing there
        // trades stale labels for meaningless ones. The staleness is real; re-running S6
        // is not the remedy.

        // --- S8/G6-2, G6-3: the cut ---
        let cut = cut_lattice(
            &lattice,
            &snapped,
            &classification,
            &point_classifier,
            &clipped.components,
            &CutOptions {
                eps,
                bands: p.thin.enabled,
                // S2's rim curves, so S8 can declare an open sheet's boundary rather
                // than leave `[V8]` to call every edge of it a pinhole.
                rim_segments: rim_segments(&clipped),
                curve_segments: curve_segments(&clipped),
                // The same curves kept whole, for `SPEC_meshgen_contracts.md` §2.3's curve
                // table and for `[V9]`.
                locked_curves: locked_curves(&clipped),
                // S2's coincident patches, so a face two solids share in exact contact is
                // declared for both rather than for neither.
                contact_patches: contact_patches(&clipped),
                min_dihedral_deg: p.thin.band_min_dihedral_deg,
                // G7-2: S8b works in cut nodes and S3 works in arranged faces; this
                // is the table that joins them, so a band element can name the thin
                // region whose gap it spans.
                thin: p.thin.enabled.then(|| {
                    thin_context(&gap_field, &effective_regimes, p.thin.collapse_sheets)
                }),
                ..Default::default()
            },
        );
        clock = stage_time("S8", clock);
        println!(
            "[S8/G6-2] cut: {} of {} cell(s) cut ({} A / {} B / {} C / {} D; {} by a welded sheet), {} cut node(s), {} -> {} tets, {} interface face(s)",
            cut.stats.n_cut_cells,
            cut.stats.n_parents,
            cut.stats.n_case_a,
            cut.stats.n_case_b,
            cut.stats.n_case_c,
            cut.stats.n_case_d,
            cut.stats.n_welded_sheet_cells,
            cut.stats.n_cut_nodes,
            cut.stats.n_parents,
            cut.stats.n_tets,
            cut.stats.n_interface_faces,
        );
        println!(
            "[S8/G6-2] quality: min dihedral {:.3} deg, worst volume error {:.3e}; {} cell(s) escalated",
            cut.stats.min_dihedral_deg,
            cut.stats.worst_volume_error,
            cut.escalated.len(),
        );
        if cut.stats.n_crossed_faces > 0 {
            println!(
                "[JCT-FACE] {} face(s) two patches cross were triangulated with both chords as edges (SPEC §7.6's precondition)",
                cut.stats.n_crossed_faces,
            );
        }
        if cut.stats.n_junction_splits > 0 {
            println!(
                "[JCT-CUT] {} escalated cell(s) cut along their surfaces into {} material piece(s) (SPEC §7.6); their material boundary is a face of the mesh, not a chamfer",
                cut.stats.n_junction_splits, cut.stats.n_junction_split_pieces,
            );
        }
        if cut.stats.n_steiner_fans > 0 {
            println!(
                "[JCT-FALLBACK] {} escalated cell(s) re-meshed as a conforming centroid fan ({} face(s) off the frozen table); their material boundary is chamfered by at most one cell",
                cut.stats.n_steiner_fans, cut.stats.n_generic_faces,
            );
        }
        if cut.stats.n_seeded_pieces > 0 {
            println!(
                "[JCT-SEED] {} escalated piece(s) had their ownership settled by a SPEC §7.5 interior sample ({} exact-predicate escalation(s)); inheriting the parent's unresolved record instead sent that material to background",
                cut.stats.n_seeded_pieces, cut.stats.n_seed_uncertain,
            );
        }
        if cut.stats.n_degenerate_fan_pieces > 0 {
            println!(
                "[JCT-DEGENERATE] {} fan piece(s) came out exactly flat and were dropped - each one leaves a hole in the cell it came from",
                cut.stats.n_degenerate_fan_pieces,
            );
        }
        if !cut.stats.n_band_declined.is_empty() {
            println!(
                "[S8b/G7-1] band: declined {:?}",
                cut.stats.n_band_declined
            );
        }
        if cut.stats.n_band_cells > 0 {
            println!(
                "[S8b/G7-1] band: {} sandwiched cell(s) split into three slabs by the doubly-cut face rule, so their gap survives as elements rather than being chamfered",
                cut.stats.n_band_cells,
            );
            let rows: Vec<String> = cut
                .stats
                .n_band_templates
                .iter()
                .map(|(template, count)| format!("{} x{count}", template.name()))
                .collect();
            println!(
                "[S8b/G7-1] band: gap slabs by frozen §8.2 row: {}; worst band dihedral {:.3} deg",
                rows.join(", "),
                cut.stats.band_min_dihedral_deg,
            );
        }
        if cut.stats.n_collapsed_pairs > 0 {
            println!(
                "[S8b/G7-2] rim collapse: {} lattice edge(s) below t_sheet carry one shared rim node instead of two wall crossings, so the gap is a welded sheet (§8.2 k=3)",
                cut.stats.n_collapsed_pairs,
            );
        }
        for warning in &cut.warnings {
            println!("{warning}");
        }

        if should_emit(snapshot_mode, Stage::Cut) {
            let mut doc = cut_to_doc(&cut, &clipped.components);
            for point in &mut doc.points {
                *point = output_domain_min.add(point.scale(domain_diag));
            }
            let meta = SnapshotMeta::new(
                Stage::Cut,
                config_hash,
                (output_domain_min, output_domain_max),
            )
            .with_determinism(p.determinism);
            let path = emit_snapshot(&mut doc, output_path, &meta, encoding)?;
            clock = stage_time("snapshot-s08", clock);
            println!("[mesh] snapshot s08: {}", path.display());
        }

        let _ = clock;

        Err(RustMsptError::NotAvailable(format!(
            "mesh: stages S9..S11 are not implemented yet; \
             S0+S1, G2-1..G2-5 arrangement+clip+topology, G3 gap field + regimes, \
             G4-1 sizing field + grading, G4-2 balanced lattice, G5-1 classification, \
             G6-1 snap, and G6-2/G6-3 cut complete; \
             {} input(s) loaded, output '{}'",
            p.inputs.len(),
            p.output.vtu
        )))
    }
}

// AI-FUNC-SUMMARY:
// Purpose: Gate G6-0's input - every locked curve of the arrangement as polyline segments.
// Inputs: the clipped arranged surface.
// Returns: one entry per polyline edge of every curve, whatever its kind, in lattice coordinates.
// Side effects: None.
// Notes: Deliberately wider than `rim_segments`. A rim is a sheet's boundary and carries a
//   declaration duty, so declaring the wrong curve as one would let a pinhole past `[V8]`; this is
//   the whole set of geometry the mesh is supposed to *conform* to, and being wrong about it costs
//   an escalation rather than a false declaration.
fn curve_segments(surface: &crate::meshgen::ArrangedSurface) -> Vec<(Vec3, Vec3)> {
    let mut out = Vec::new();
    for curve in &surface.curves {
        for window in curve.nodes.windows(2) {
            let (a, b) = (surface.vertices[window[0]], surface.vertices[window[1]]);
            if a != b {
                out.push((a, b));
            }
        }
    }
    out
}

// AI-FUNC-SUMMARY:
// Purpose: S2's locked curves with everything the VTU's curve table and `[V9]` need.
// Inputs: the clipped arranged surface.
// Returns: one `LockedCurve` per arranged curve - kind, component set, radial patch count and
//   polyline - in lattice coordinates.
// Side effects: None.
// Notes: Every curve is carried, including one the mesh turns out to hold no edge for: `[V9]`
//   has to be able to say "declared and not carried", and a table row that is simply absent
//   cannot say it. The radial patch count is `radial_patch_order`'s own answer, so the check
//   compares the mesh against what the arrangement decided rather than against a rule of
//   thumb about how many sectors two crossing surfaces ought to make.
fn locked_curves(surface: &crate::meshgen::ArrangedSurface) -> Vec<crate::meshgen::cut::LockedCurve> {
    surface
        .curves
        .iter()
        .map(|curve| crate::meshgen::cut::LockedCurve {
            kind: curve.kind as u8,
            components: curve.components.clone(),
            radial_patches: curve.radial_patches.len() as u32,
            segments: curve
                .nodes
                .windows(2)
                .filter_map(|window| {
                    let (a, b) = (surface.vertices[window[0]], surface.vertices[window[1]]);
                    (a != b).then_some((a, b))
                })
                .collect(),
        })
        .collect()
}

// AI-FUNC-SUMMARY:
// Purpose: The arranged rim curves as polyline segments, for S8's sheet-boundary declaration (G7-2).
// Inputs: the clipped arranged surface.
// Returns: one entry per rim polyline edge, in lattice coordinates.
// Side effects: None.
// Notes: Rim curves only - a sharp or intersection curve is not a place a sheet is allowed to end,
//   and declaring one as though it were would let a real pinhole past `[V8]`.
fn rim_segments(surface: &crate::meshgen::ArrangedSurface) -> Vec<(Vec3, Vec3)> {
    let mut out = Vec::new();
    for curve in &surface.curves {
        if curve.kind != ArrangedCurveKind::Rim {
            continue;
        }
        for window in curve.nodes.windows(2) {
            let (a, b) = (surface.vertices[window[0]], surface.vertices[window[1]]);
            if a != b {
                out.push((a, b));
            }
        }
    }
    out
}

// AI-FUNC-SUMMARY:
// Purpose: S2's coincident arranged patches - the faces belonging to more than one component - as triangles paired with their component set.
// Inputs: the clipped arranged surface.
// Returns: one entry per multi-tagged, non-box arranged face, in lattice coordinates.
// Side effects: None.
// Notes: This is the only place the arrangement's multi-tag survives into S8. Everything between
//   reads the single `ArrangedFace::component`, so without it a face two solids share in exact
//   contact reaches the cut as one body's boundary and is declared for neither.
fn contact_patches(
    surface: &crate::meshgen::ArrangedSurface,
) -> Vec<([Vec3; 3], smallvec::SmallVec<[i32; 2]>)> {
    let mut out = Vec::new();
    for face in &surface.faces {
        if face.box_tagged || face.components.len() < 2 {
            continue;
        }
        let mut components = face.components.clone();
        components.sort_unstable();
        components.dedup();
        if components.len() < 2 {
            continue;
        }
        out.push((
            [
                surface.vertices[face.nodes[0]],
                surface.vertices[face.nodes[1]],
                surface.vertices[face.nodes[2]],
            ],
            components,
        ));
    }
    out
}
