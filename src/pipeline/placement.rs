use crate::config::placement::{
    BoundaryMode, OnUnattainable, OrientationMode, PlacementOrder, PositionMode,
    ResolvedDistribution, ResolvedPlacement, TargetBasis, VoidCrossing,
};
use crate::error::{Result, RustMsptError};
use crate::geometry::{
    mesh_bbox, sample_uniform_quaternion, transform_shell, UnitQuat,
};
use crate::geometry::spatial::SpatialGrid;
use crate::geometry::{VoidIndex, VoidVolumeMethod};
use crate::io::{load_stl, save_stl, sha256_file};
use crate::pipeline::placement_feasibility::{
    check_placement, Candidate, FeasibilityContext, PlacedParticle, RejectReason,
};
use crate::pipeline::placement_library::{load_shape_library, ShapeLibrary};
use crate::pipeline::placement_outputs::*;
use crate::pipeline::placement_sizes::{
    build_classes, order_for_placement, plan_size_multiset, SizeClass, SizeDraw, SizeSource,
};
use crate::pipeline::rng::{seeded_rng, u01, uniform_index, uniform_range};
use crate::pipeline::Pipeline;
use crate::types::{Mesh, Triangle, Vec3};
use crate::version::build_identity;
use rand_chacha::ChaCha12Rng;
use std::collections::BTreeMap;
use std::path::Path;
use std::time::Instant;

/// A hard ceiling on how many particles one plan may contain, so a target that is
/// unreachable at the configured size distribution is refused rather than looping.
const MAX_PLANNED_PARTICLES: usize = 5_000_000;

/// Runs the seeded, recorded, void-aware placement engine.
///
/// Holds an already-validated config: every cross-field rule has been applied and
/// every path resolved by `PlacementParams::validate`, so nothing here decides
/// what a field means.
pub struct PlacementPipeline {
    pub config: ResolvedPlacement,
}

impl Pipeline for PlacementPipeline {
    // AI-FUNC-SUMMARY:
    // Purpose: Place particles into the domain under the resolved placement config and write every output.
    // Inputs: self.config, already validated.
    // Returns: Ok(()) once the outputs are written, including when the run stopped short of its target.
    // Side effects: Reads the shape files; writes the geometry, record, report and size CSV.
    // Notes: Stopping short is a result, not a failure: it exits zero and says so in the report.
    // Only an unusable config or an output that cannot be written is an error.
    fn run(&self) -> Result<()> {
        let outcome = run_placement(&self.config)?;
        for line in &outcome.summary_lines {
            println!("{line}");
        }
        Ok(())
    }
}

/// What a completed run produced, for callers that want it in-process.
#[derive(Debug)]
pub struct PlacementOutcome {
    pub placed: usize,
    pub stop_reason: StopReason,
    pub volume_fraction_solid: f64,
    pub summary_lines: Vec<String>,
}

// AI-FUNC-SUMMARY:
// Purpose: Run the placement engine end to end and write every output file.
// Inputs: the resolved config.
// Returns: a summary of what was placed and why the run stopped.
// Side effects: Reads inputs, creates the output directory, writes the STL, record, report and CSV.
// Notes: The testable core: the pipeline's run() is a thin wrapper so tests need not go through
// stdout. The report is written twice - once as `running` before placement starts, once as
// `finished` at the end - so a run that is killed still leaves evidence of what it was.
pub fn run_placement(config: &ResolvedPlacement) -> Result<PlacementOutcome> {
    let started = Instant::now();
    let identity = build_identity();
    let tool = ToolRecord::from(&identity);

    std::fs::create_dir_all(&config.outputs.dir)?;

    let library = load_shape_library(
        &config.shape_files,
        &config.shape_paths_as_written,
        config.filters.as_ref(),
    )?;
    let source = SizeSource::prepare(&config.distribution)?;
    let classes = build_classes(&config.classes, &source);

    // The void is loaded and validated before anything is planned: a run whose
    // void is unusable should say so before it spends any budget.
    let void = match &config.void {
        None => None,
        Some(v) => {
            let mesh = load_stl(&v.file)?;
            let index = VoidIndex::build(&mesh)?;
            if !index
                .bbox()
                .expanded(0.0)
                .intersects_domain(config.domain)
            {
                return Err(RustMsptError::InvalidConfig(format!(
                    "the void in {} does not meet the domain at all, which is a frame or unit \
                     error rather than an empty pore network",
                    v.file.display()
                )));
            }
            Some(index)
        }
    };
    let (void_volume_in_domain, void_volume_method) = match &void {
        None => (0.0, None),
        Some(index) => {
            let (v, method) = index.volume_in_domain(config.domain);
            (v, Some(method))
        }
    };

    let basis_volume = match config.target_basis {
        TargetBasis::Domain => config.domain.volume(),
        // "Solid" means the domain minus the void, which is what a solid-phase
        // fraction is normally quoted against.
        TargetBasis::Solid => {
            let solid = config.domain.volume() - void_volume_in_domain;
            if solid <= 0.0 {
                return Err(RustMsptError::InvalidConfig(format!(
                    "the void fills the whole domain ({void_volume_in_domain} of \
                     {}), so there is no solid region to place into",
                    config.domain.volume()
                )));
            }
            solid
        }
    };

    let target_volume = basis_volume * config.target_volume_fraction;
    let mut rng = seeded_rng(config.seed);
    let mut plan = plan_size_multiset(
        &mut rng,
        &source,
        &classes,
        target_volume,
        MAX_PLANNED_PARTICLES,
    )?;
    order_for_placement(
        &mut plan.draws,
        config.placement_order == PlacementOrder::Descending,
    );

    let threads = resolve_threads(config.threads);
    let frame = FrameRecord {
        unit: config.unit.clone(),
        origin: [config.domain.min.x, config.domain.min.y, config.domain.min.z],
        axis_order: "xyz".to_string(),
        handedness: "right".to_string(),
        domain: DomainRecord {
            min: [config.domain.min.x, config.domain.min.y, config.domain.min.z],
            max: [config.domain.max.x, config.domain.max.y, config.domain.max.z],
        },
    };

    // Write the report once before placing anything. A six-hour run that is killed
    // then leaves a file saying what it was, rather than nothing at all.
    let void_report = build_void_report(
        config,
        void.as_ref(),
        void_volume_in_domain,
        void_volume_method,
    )?;
    let mut report = blank_report(
        config,
        &tool,
        &frame,
        &library,
        &plan,
        basis_volume,
        threads,
        void_report,
    );
    write_json(&config.outputs.report, &report)?;

    let mut state = EngineState::new(config, &library, &classes, &plan.draws);
    place_all(
        config,
        &library,
        &classes,
        void.as_ref(),
        &mut rng,
        &mut plan.draws,
        &mut state,
    );

    // A top-up is allowed only when every planned size was placed and volume was
    // lost to the boundary. Drawing one after a failure would re-draw from the
    // same distribution, mostly produce small particles, and quietly make up the
    // shortfall - which is the one thing the plan-first design exists to prevent.
    let mut top_up = TopUpReport {
        batches: 0,
        drawn: 0,
        placed: 0,
    };
    if state.shortfall_total == 0 && !state.budget_spent(config) {
        run_top_up(
            config,
            &library,
            &classes,
            void.as_ref(),
            &source,
            &mut rng,
            &mut state,
            target_volume,
            &mut top_up,
        )?;
    }

    let elapsed = started.elapsed().as_secs_f64();
    let stop = decide_stop(config, &state, &plan, target_volume, elapsed);

    let outputs = write_outputs(
        config,
        &library,
        &state,
        &tool,
        &frame,
        &classes,
        void.as_ref(),
    )?;
    finish_report(
        &mut report,
        config,
        &state,
        &classes,
        &top_up,
        &stop,
        elapsed,
        threads,
        outputs,
        target_volume,
    );
    write_json(&config.outputs.report, &report)?;

    let summary_lines = summary(config, &state, &stop, target_volume, basis_volume);
    Ok(PlacementOutcome {
        placed: state.placed.len(),
        stop_reason: stop.reason,
        volume_fraction_solid: state.volume_solid / basis_volume,
        summary_lines,
    })
}

// AI-FUNC-SUMMARY: Resolve a thread setting into a worker count; returns at least 1; side effects: reads available_parallelism.
fn resolve_threads(threads: i32) -> usize {
    let available = std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(1);
    if threads <= 0 {
        available
    } else {
        (threads as usize).min(available.max(threads as usize))
    }
}

/// Everything the placement loop accumulates.
struct EngineState {
    placed: Vec<PlacedParticle>,
    /// Rebuilt as particles are accepted. Cell size comes from the largest planned
    /// particle plus the gap rather than from `estimate_cell_size`, whose 1.0 floor
    /// makes it unit-dependent.
    grid: SpatialGrid,
    merged: Mesh,
    /// Running total, accumulated sequentially in acceptance order. A parallel sum
    /// over f64 depends on the reduction tree and therefore on the thread count,
    /// so the number that gates the stop would not be reproducible.
    volume_in_domain: f64,
    volume_solid: f64,
    attempts: usize,
    rejections: BTreeMap<RejectReason, usize>,
    drawn_per_class: Vec<usize>,
    placed_per_class: Vec<usize>,
    top_up_drawn_per_class: Vec<usize>,
    top_up_placed_per_class: Vec<usize>,
    shortfall_total: usize,
    first_failed_diameter: Option<f64>,
    stopped_early: bool,
}

impl EngineState {
    fn new(
        config: &ResolvedPlacement,
        library: &ShapeLibrary,
        classes: &[SizeClass],
        draws: &[SizeDraw],
    ) -> EngineState {
        let largest = draws
            .iter()
            .map(|d| d.diameter)
            .fold(0.0f64, f64::max)
            .max(1e-9);
        let extent_ratio = library.max_extent_ratio.max(1.0);
        let cell_size = largest * extent_ratio + config.gap_particle_particle;
        let mut drawn_per_class = vec![0usize; classes.len()];
        for d in draws {
            if let Some(slot) = drawn_per_class.get_mut(d.class) {
                *slot += 1;
            }
        }
        EngineState {
            grid: SpatialGrid::new(config.domain, cell_size.max(1e-9)),
            placed: Vec::new(),
            merged: Mesh::empty(),
            volume_in_domain: 0.0,
            volume_solid: 0.0,
            attempts: 0,
            rejections: BTreeMap::new(),
            drawn_per_class,
            placed_per_class: vec![0usize; classes.len()],
            top_up_drawn_per_class: vec![0usize; classes.len()],
            top_up_placed_per_class: vec![0usize; classes.len()],
            shortfall_total: 0,
            first_failed_diameter: None,
            stopped_early: false,
        }
    }

    // AI-FUNC-SUMMARY: True when the whole-run attempt budget is used up; returns bool; side effects: none.
    fn budget_spent(&self, config: &ResolvedPlacement) -> bool {
        self.attempts >= config.budget.total_attempts
    }

    // AI-FUNC-SUMMARY: Record one rejection under its reason; returns nothing; side effects: mutates the tally.
    fn reject(&mut self, reason: RejectReason) {
        *self.rejections.entry(reason).or_insert(0) += 1;
    }
}

// AI-FUNC-SUMMARY:
// Purpose: Attempt every planned size in order, accepting what fits.
// Inputs: the config, library, classes, generator, the ordered draws, and the mutable state.
// Returns: None.
// Side effects: Mutates the state: accepted particles, the merged mesh, the grid, tallies.
// Notes: The RNG consumption schedule is fixed and lives here: every attempt draws all seven of its
// variates up front - one for the shell, three for the orientation, three for the position - before
// any check runs. Drawing them lazily would make the stream depend on which check short-circuited,
// so reordering the checks later would silently change every placement.
#[allow(clippy::too_many_arguments)]
fn place_all(
    config: &ResolvedPlacement,
    library: &ShapeLibrary,
    classes: &[SizeClass],
    void: Option<&VoidIndex>,
    rng: &mut ChaCha12Rng,
    draws: &mut [SizeDraw],
    state: &mut EngineState,
) {
    for draw in draws.iter() {
        if state.budget_spent(config) {
            state.stopped_early = true;
            break;
        }
        let placed = try_place_one(config, library, void, rng, draw, state);
        if placed {
            if let Some(slot) = state.placed_per_class.get_mut(draw.class) {
                *slot += 1;
            }
        } else {
            state.shortfall_total += 1;
            if state.first_failed_diameter.is_none() {
                state.first_failed_diameter = Some(draw.diameter);
            }
            if config.on_unattainable == OnUnattainable::Stop {
                state.stopped_early = true;
                break;
            }
        }
    }
    let _ = classes;
}

// AI-FUNC-SUMMARY:
// Purpose: Try to place one particle of a given size within its per-particle attempt budget.
// Inputs: the config, library, generator, the size draw, and the mutable state.
// Returns: true when a placement was accepted.
// Side effects: Mutates the state.
// Notes: Every attempt draws its seven variates in the same fixed order regardless of outcome.
fn try_place_one(
    config: &ResolvedPlacement,
    library: &ShapeLibrary,
    void: Option<&VoidIndex>,
    rng: &mut ChaCha12Rng,
    draw: &SizeDraw,
    state: &mut EngineState,
) -> bool {
    for _ in 0..config.budget.attempts_per_particle {
        if state.budget_spent(config) {
            return false;
        }
        state.attempts += 1;

        // 1 draw: which shell.
        let shell_index = uniform_index(rng, library.shells.len());
        let shell = &library.shells[shell_index];
        let scale = draw.diameter / shell.equivalent_diameter;

        // 3 draws: the orientation.
        let rotation = match config.orientation {
            OrientationMode::UniformSo3 => sample_uniform_quaternion(rng),
            OrientationMode::Fixed => {
                let _ = (u01(rng), u01(rng), u01(rng));
                UnitQuat::identity()
            }
        };

        // The particle's reach from its own centre. Rotation cannot change it, so
        // it bounds the particle whatever orientation came up.
        let reach = shell.bounding_radius * scale;
        let box_for_centre = match config.boundary.mode {
            BoundaryMode::Strict => config
                .domain
                .expanded(-(reach + config.boundary.min_boundary_dist)),
            BoundaryMode::Clip | BoundaryMode::Periodic => config.domain,
        };

        // 3 or 4 draws: the centroid.
        //
        // feasible_uniform: uniform in the box above. In strict mode that box is
        // the domain eroded by the particle's circumscribed-sphere radius, which
        // does NOT depend on the orientation. Eroding by the *rotated* bounding
        // box instead would make the proposal box a function of the orientation,
        // under-weighting orientations with a smaller footprint and correlating
        // orientation with position near the walls - exactly the kind of artefact
        // a consumer would notice in the pair statistics.
        //
        // void_neighbourhood: an area-weighted point on the void surface, offset
        // outward by a distance in the declared band. This is a deliberate
        // construction, not a random one, and the report names it as such.
        let centre = match config.position.mode {
            PositionMode::FeasibleUniform => Vec3::new(
                uniform_range(rng, box_for_centre.min.x, box_for_centre.max.x),
                uniform_range(rng, box_for_centre.min.y, box_for_centre.max.y),
                uniform_range(rng, box_for_centre.min.z, box_for_centre.max.z),
            ),
            PositionMode::VoidNeighbourhood => {
                let (lo, hi) = config.position.band.unwrap_or((0.0, 0.0));
                match void {
                    Some(index) => {
                        let (surface, normal) = index.sample_surface_point(rng);
                        let offset = uniform_range(rng, lo, hi);
                        surface.add(normal.scale(offset))
                    }
                    None => {
                        // validate() refuses this combination, so it cannot be
                        // reached; the draws are still consumed so the stream does
                        // not depend on the branch taken.
                        let _ = (u01(rng), u01(rng), u01(rng), u01(rng));
                        Vec3::new(0.0, 0.0, 0.0)
                    }
                }
            }
        };
        if box_for_centre.volume() <= 0.0 {
            // The particle cannot fit the domain at all under this boundary rule.
            state.reject(RejectReason::OutsideDomain);
            continue;
        }

        let candidate = transform_shell(&shell.canonical, scale, rotation, centre);
        let Some(cand_bbox) = mesh_bbox(&candidate) else {
            state.reject(RejectReason::ZeroInDomainVolume);
            continue;
        };
        let neighbours = state.grid.query_neighbors_with_margin(
            cand_bbox,
            config.gap_particle_particle,
            usize::MAX,
        );

        let ctx = FeasibilityContext {
            domain: config.domain,
            boundary: &config.boundary,
            gap_particle_particle: config.gap_particle_particle,
            placed: &state.placed,
            neighbours: &neighbours,
            void,
            void_crossing: config
                .void
                .as_ref()
                .map(|v| v.crossing)
                .unwrap_or(VoidCrossing::Forbidden),
            void_gap: config.void.as_ref().map(|v| v.gap).unwrap_or(0.0),
            neighbourhood_band: (config.position.mode == PositionMode::VoidNeighbourhood)
                .then_some(config.position.band)
                .flatten(),
        };
        // The full volume is exact from the source shell: scaling by s multiplies
        // volume by s^3. Summing the transformed mesh's tetrahedra on every
        // attempt would give the same number more slowly and less exactly.
        let proposal = Candidate {
            mesh: &candidate,
            bbox: cand_bbox,
            centre,
            reach,
            volume_full: shell.volume * scale * scale * scale,
        };
        match check_placement(&ctx, &proposal) {
            Err(reason) => {
                state.reject(reason);
            }
            Ok(accepted) => {
                let volume_full = proposal.volume_full;
                // Only measured when crossing is allowed. With crossing forbidden
                // the particle keeps a gap from the void, so the overlap is zero
                // by construction and paying for a voxel sweep would be waste.
                let overlap = match (void, config.void.as_ref()) {
                    (Some(index), Some(v)) if v.crossing == VoidCrossing::Allowed => index
                        .overlap_volume(
                            &candidate,
                            cand_bbox,
                            config.domain,
                            v.overlap_voxel_size.unwrap_or(0.0),
                        ),
                    _ => 0.0,
                };
                accept(
                    state, shell_index, library, draw, scale, rotation, centre, reach, volume_full,
                    cand_bbox, overlap, candidate, accepted,
                );
                return true;
            }
        }
    }
    false
}

// AI-FUNC-SUMMARY:
// Purpose: Commit an accepted candidate: append its geometry, record its transform, update the index.
// Inputs: the state, config, the shell it came from, the size draw, its transform, mesh and check result.
// Returns: None.
// Side effects: Mutates the state.
// Notes: The triangle range is the merged mesh's face count before and after appending, so a reader
// can pull one particle's triangles out of the merged STL without reconstructing it. The volume
// accumulators are sequential, in acceptance order, for the reason given on EngineState.
#[allow(clippy::too_many_arguments)]
fn accept(
    state: &mut EngineState,
    shell_index: usize,
    library: &ShapeLibrary,
    draw: &SizeDraw,
    scale: f64,
    rotation: UnitQuat,
    translation: Vec3,
    reach: f64,
    volume_full: f64,
    bbox: crate::types::BoundingBox,
    void_overlap_volume: f64,
    mesh: Mesh,
    accepted: crate::pipeline::placement_feasibility::Accepted,
) {
    let shell = &library.shells[shell_index];
    let start = state.merged.faces.len();
    let base = state.merged.vertices.len();
    state.merged.vertices.extend(mesh.vertices.iter().copied());
    state.merged.faces.extend(mesh.faces.iter().map(|f| Triangle {
        a: f.a + base,
        b: f.b + base,
        c: f.c + base,
    }));
    let end = state.merged.faces.len();

    let index = state.placed.len();
    let particle = PlacedParticle {
        acceptance_index: index,
        source_index: shell.source_index,
        shell_index: shell.shell_index,
        scale,
        rotation,
        translation,
        equivalent_diameter: draw.diameter,
        reach,
        size_class: draw.class,
        volume_full,
        volume_in_domain: accepted.volume_in_domain,
        void_overlap_volume,
        clipped_faces: accepted.clipped_faces,
        bbox,
        mesh,
        shape: accepted.shape,
        triangle_range: (start, end),
    };
    state.volume_in_domain += particle.volume_in_domain;
    state.volume_solid += particle.volume_in_domain_solid();
    state.grid.insert(index, particle.bbox);
    state.placed.push(particle);
}

// AI-FUNC-SUMMARY:
// Purpose: Draw and place further batches when clipping alone left the target short.
// Inputs: the config, library, classes, size source, generator, state, target volume, and the top-up tally.
// Returns: Ok(()).
// Side effects: Mutates the state and the tally.
// Notes: Only reached when every planned size was placed. Each batch is a fresh closest-sum multiset
// for the remaining deficit, and its counts are tallied separately from the target ones so the CSV
// cannot hide a shortfall behind a top-up.
#[allow(clippy::too_many_arguments)]
fn run_top_up(
    config: &ResolvedPlacement,
    library: &ShapeLibrary,
    classes: &[SizeClass],
    void: Option<&VoidIndex>,
    source: &SizeSource,
    rng: &mut ChaCha12Rng,
    state: &mut EngineState,
    target_volume: f64,
    top_up: &mut TopUpReport,
) -> Result<()> {
    while top_up.batches < config.budget.max_top_up_batches {
        let deficit = target_volume * (1.0 - config.target_tolerance) - state.volume_solid;
        if deficit <= 0.0 || state.budget_spent(config) {
            break;
        }
        let mut batch = plan_size_multiset(rng, source, classes, deficit, MAX_PLANNED_PARTICLES)?;
        order_for_placement(
            &mut batch.draws,
            config.placement_order == PlacementOrder::Descending,
        );
        top_up.batches += 1;
        top_up.drawn += batch.draws.len();
        let before = state.placed.len();
        for draw in &batch.draws {
            if state.budget_spent(config) {
                state.stopped_early = true;
                break;
            }
            if let Some(slot) = state.top_up_drawn_per_class.get_mut(draw.class) {
                *slot += 1;
            }
            if try_place_one(config, library, void, rng, draw, state) {
                if let Some(slot) = state.top_up_placed_per_class.get_mut(draw.class) {
                    *slot += 1;
                }
            }
        }
        let gained = state.placed.len() - before;
        top_up.placed += gained;
        if gained == 0 {
            // Another batch would draw from the same distribution and fail the
            // same way; stopping here is what keeps the budget meaningful.
            break;
        }
    }
    Ok(())
}

/// Why the run ended, and the detail the report carries with it.
struct StopDecision {
    reason: StopReason,
    detail: BTreeMap<String, String>,
}

// AI-FUNC-SUMMARY:
// Purpose: Decide which of the four stop reasons a finished run ended with.
// Inputs: the config, engine state, plan, target volume and elapsed time.
// Returns: the reason and its detail.
// Side effects: None.
// Notes: The precedence is the point, and two rules genuinely co-occur.
// Placed-nothing outranks everything: a run with no particles has no distribution to describe.
// Unattainable outranks budget-exhausted, because exhausting the per-particle budget is *how* an
// unattainable size is detected - without this order every distribution failure would report as a
// budget failure, and the requirement that the run say why it stopped would never be met.
// Nothing here can emit a fifth word: a run that placed everything it planned but lost volume to
// the boundary is target_reached with the deficit named in the detail.
fn decide_stop(
    config: &ResolvedPlacement,
    state: &EngineState,
    plan: &crate::pipeline::placement_sizes::SizePlan,
    target_volume: f64,
    elapsed: f64,
) -> StopDecision {
    let mut detail = BTreeMap::new();
    detail.insert("planned".to_string(), plan.draws.len().to_string());
    detail.insert("placed".to_string(), state.placed.len().to_string());
    detail.insert("attempts".to_string(), state.attempts.to_string());

    if state.placed.is_empty() {
        let (top, count) = state
            .rejections
            .iter()
            .max_by_key(|(_, n)| **n)
            .map(|(r, n)| (r.as_str(), *n))
            .unwrap_or(("none", 0));
        detail.insert("dominant_rejection".to_string(), top.to_string());
        detail.insert("dominant_rejection_count".to_string(), count.to_string());
        detail.insert(
            "message".to_string(),
            format!(
                "no particle could be placed in {} attempts; most were rejected as {top}",
                state.attempts
            ),
        );
        return StopDecision {
            reason: StopReason::NoFeasiblePlacement,
            detail,
        };
    }

    if state.shortfall_total > 0 {
        detail.insert("shortfall".to_string(), state.shortfall_total.to_string());
        if let Some(d) = state.first_failed_diameter {
            detail.insert("first_failed_diameter".to_string(), format!("{d}"));
        }
        detail.insert(
            "on_unattainable".to_string(),
            match config.on_unattainable {
                OnUnattainable::SkipReported => "skip_reported".to_string(),
                OnUnattainable::Stop => "stop".to_string(),
            },
        );
        detail.insert(
            "message".to_string(),
            format!(
                "{} of {} planned sizes could not be placed; no replacement was drawn, so the \
                 shortfall stands in its own size classes",
                state.shortfall_total,
                plan.draws.len()
            ),
        );
        return StopDecision {
            reason: StopReason::DistributionUnattainable,
            detail,
        };
    }

    if state.budget_spent(config) {
        detail.insert("limit".to_string(), "total_attempts".to_string());
        detail.insert(
            "value".to_string(),
            config.budget.total_attempts.to_string(),
        );
        detail.insert(
            "message".to_string(),
            format!(
                "the whole-run attempt budget of {} was used up",
                config.budget.total_attempts
            ),
        );
        return StopDecision {
            reason: StopReason::BudgetExhausted,
            detail,
        };
    }

    let reached = state.volume_solid >= target_volume * (1.0 - config.target_tolerance);
    detail.insert(
        "volume_solid".to_string(),
        format!("{}", state.volume_solid),
    );
    detail.insert("target_volume".to_string(), format!("{target_volume}"));
    if reached {
        detail.insert(
            "message".to_string(),
            "every planned size was placed and the target volume fraction was reached".to_string(),
        );
    } else {
        detail.insert("deficit_cause".to_string(), "clipping".to_string());
        detail.insert(
            "message".to_string(),
            "every planned size was placed, but volume lost at the domain boundary left the \
             fraction short of its target"
                .to_string(),
        );
    }
    let _ = elapsed;
    StopDecision {
        reason: StopReason::TargetReached,
        detail,
    }
}

// AI-FUNC-SUMMARY:
// Purpose: Write the geometry, the per-particle record and the size CSV.
// Inputs: the config, library, state, tool identity, frame and classes.
// Returns: the manifest of files written.
// Side effects: Writes files under the output directory.
// Notes: A run that placed nothing writes no STL: save_stl refuses an empty mesh, and calling it
// would turn "stopped short" into a non-zero exit, contradicting the contract that stopping short
// is a result. The record and the report are written either way.
#[allow(clippy::too_many_arguments)]
fn write_outputs(
    config: &ResolvedPlacement,
    library: &ShapeLibrary,
    state: &EngineState,
    tool: &ToolRecord,
    frame: &FrameRecord,
    classes: &[SizeClass],
    void: Option<&VoidIndex>,
) -> Result<Vec<FileEntry>> {
    let mut outputs = Vec::new();

    let wrote_stl = if state.placed.is_empty() {
        false
    } else {
        save_stl(&config.outputs.particles_stl, &state.merged, "particles")?;
        outputs.push(describe_output("particles", &config.outputs.particles_stl));
        true
    };

    let size = config.domain.size();
    let max_extent = size.x.max(size.y).max(size.z);
    let record = RecordFile {
        schema_version: RECORD_SCHEMA.to_string(),
        tool: tool.clone(),
        seed: config.seed,
        rng: "chacha12".to_string(),
        frame: frame.clone(),
        conventions: conventions(&config.unit, max_extent),
        phases: vec![
            PhaseRecord {
                id: 0,
                name: "matrix".to_string(),
                geometry: None,
                overlap_owner: None,
            },
            PhaseRecord {
                id: 1,
                name: "particle".to_string(),
                geometry: wrote_stl.then(|| {
                    config
                        .outputs
                        .particles_stl
                        .file_name()
                        .map(|n| n.to_string_lossy().to_string())
                        .unwrap_or_default()
                }),
                overlap_owner: None,
            },
        ]
        .into_iter()
        .chain(config.void.as_ref().map(|v| PhaseRecord {
            id: 2,
            name: "void".to_string(),
            geometry: Some(
                Path::new(&v.path_as_written)
                    .file_name()
                    .map(|n| n.to_string_lossy().to_string())
                    .unwrap_or_else(|| v.path_as_written.clone()),
            ),
            // The void owns any overlap: it is frozen, so a particle that crosses
            // into it does not take that volume out of the void phase.
            overlap_owner: Some("void".to_string()),
        }))
        .collect(),
        sources: library
            .sources
            .iter()
            .map(|s| SourceRecord {
                index: s.index,
                path: s.path_as_written.clone(),
                resolved_path: s.path.to_string_lossy().to_string(),
                sha256: s.sha256.clone(),
                bytes: s.bytes,
                shells_found: s.shells_found,
                shells_kept: s.shells_kept,
            })
            .collect(),
        particles: state
            .placed
            .iter()
            .map(|p| particle_record(p, library))
            .collect(),
    };
    write_json(&config.outputs.record, &record)?;
    outputs.push(describe_output("record", &config.outputs.record));

    let rows = size_class_rows(state, classes);
    write_size_distribution_csv(&config.outputs.size_csv, &rows)?;
    outputs.push(describe_output("size_distribution", &config.outputs.size_csv));

    if config.outputs.per_particle_stl && !state.placed.is_empty() {
        let dir = config.outputs.dir.join("particles");
        std::fs::create_dir_all(&dir)?;
        for p in &state.placed {
            let path = dir.join(format!("{}.stl", entity_id(p.acceptance_index)));
            save_stl(&path, &p.mesh, "particle")?;
        }
        outputs.push(FileEntry {
            role: "per_particle_stl_dir".to_string(),
            path: dir.to_string_lossy().to_string(),
            sha256: None,
            bytes: None,
        });
    }

    if let Some(voxel_size) = config.outputs.voxel_labels {
        let written = crate::pipeline::placement_labels::write_voxel_labels(
            config, voxel_size, &state.placed, void,
        )?;
        for path in written {
            let role = path
                .file_stem()
                .map(|s| format!("voxel_{}", s.to_string_lossy()))
                .unwrap_or_else(|| "voxel_labels".to_string());
            outputs.push(describe_output(&role, &path));
        }
    }

    if let Some(v) = &config.void {
        if config.outputs.copy_void {
            let name = v
                .file
                .file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_else(|| "void.stl".to_string());
            let dest = config.outputs.dir.join(&name);
            // Copied byte for byte, never re-written through the STL writer: the
            // void is frozen, and the report records that its input and output
            // digests match so a reader can check that claim rather than trust it.
            std::fs::copy(&v.file, &dest)?;
            outputs.push(describe_output("void", &dest));
        }
    }

    // The report lists itself, but cannot contain its own digest.
    outputs.push(FileEntry {
        role: "report".to_string(),
        path: config.outputs.report.to_string_lossy().to_string(),
        sha256: None,
        bytes: None,
    });
    Ok(outputs)
}

// AI-FUNC-SUMMARY: The stable id of a placed particle; returns "p" plus its zero-padded acceptance index; side effects: none.
pub fn entity_id(acceptance_index: usize) -> String {
    format!("p{acceptance_index:06}")
}

// AI-FUNC-SUMMARY: Turn one placed particle into its record entry; returns ParticleRecord; side effects: none.
fn particle_record(p: &PlacedParticle, library: &ShapeLibrary) -> ParticleRecord {
    let shell = library
        .shells
        .iter()
        .find(|s| s.source_index == p.source_index && s.shell_index == p.shell_index);
    let source = library.sources.get(p.source_index);
    ParticleRecord {
        entity_id: entity_id(p.acceptance_index),
        acceptance_index: p.acceptance_index,
        source_shape: SourceShapeRecord {
            source_index: p.source_index,
            path: source.map(|s| s.path_as_written.clone()).unwrap_or_default(),
            sha256: source.map(|s| s.sha256.clone()).unwrap_or_default(),
            shell_index: p.shell_index,
            shell_sha256: shell.map(|s| s.shell_sha256.clone()).unwrap_or_default(),
            shell_centroid: shell
                .map(|s| [s.centroid.x, s.centroid.y, s.centroid.z])
                .unwrap_or([0.0; 3]),
            shell_volume: shell.map(|s| s.volume).unwrap_or(0.0),
            shell_equivalent_diameter: shell.map(|s| s.equivalent_diameter).unwrap_or(0.0),
        },
        scale: p.scale,
        rotation: RotationRecord {
            quaternion: p.rotation.to_wxyz(),
            matrix: p.rotation.to_matrix(),
        },
        translation: [p.translation.x, p.translation.y, p.translation.z],
        equivalent_diameter: p.equivalent_diameter,
        volume: VolumeRecord {
            full: p.volume_full,
            in_domain: p.volume_in_domain,
            in_domain_solid: p.volume_in_domain_solid(),
        },
        clipped: ClippedRecord {
            any: !p.clipped_faces.is_empty(),
            faces: p.clipped_faces.iter().map(|f| f.to_string()).collect(),
        },
        void_overlap_volume: p.void_overlap_volume,
        size_class: p.size_class,
        triangle_range: [p.triangle_range.0, p.triangle_range.1],
        bbox: DomainRecord {
            min: [p.bbox.min.x, p.bbox.min.y, p.bbox.min.z],
            max: [p.bbox.max.x, p.bbox.max.y, p.bbox.max.z],
        },
    }
}

// AI-FUNC-SUMMARY: Build the per-class target-against-actual rows; returns the rows in class order; side effects: none.
fn size_class_rows(state: &EngineState, classes: &[SizeClass]) -> Vec<SizeClassRow> {
    let planned: usize = state.drawn_per_class.iter().sum();
    classes
        .iter()
        .enumerate()
        .map(|(i, c)| {
            let drawn = state.drawn_per_class.get(i).copied().unwrap_or(0);
            let placed = state.placed_per_class.get(i).copied().unwrap_or(0);
            SizeClassRow {
                class: i,
                lo: c.lo,
                hi: c.hi,
                target_frequency: c.target_frequency,
                target_count: (c.target_frequency * planned as f64).round() as usize,
                drawn,
                placed,
                shortfall: drawn.saturating_sub(placed),
                top_up_drawn: state.top_up_drawn_per_class.get(i).copied().unwrap_or(0),
                top_up_placed: state.top_up_placed_per_class.get(i).copied().unwrap_or(0),
            }
        })
        .collect()
}

// AI-FUNC-SUMMARY: Build the report as it stands before placement starts; returns ReportFile with status "running"; side effects: none.
#[allow(clippy::too_many_arguments)]
fn blank_report(
    config: &ResolvedPlacement,
    tool: &ToolRecord,
    frame: &FrameRecord,
    library: &ShapeLibrary,
    plan: &crate::pipeline::placement_sizes::SizePlan,
    basis_volume: f64,
    threads: usize,
    void_report: Option<VoidReport>,
) -> ReportFile {
    let mut inputs: Vec<FileEntry> = library
        .sources
        .iter()
        .map(|s| FileEntry {
            role: "shape".to_string(),
            path: s.path.to_string_lossy().to_string(),
            sha256: Some(s.sha256.clone()),
            bytes: Some(s.bytes),
        })
        .collect();
    if let ResolvedDistribution::Histogram { csv, .. } = &config.distribution {
        inputs.push(describe_input("size_histogram", csv));
    }
    let mut samplers = BTreeMap::new();
    samplers.insert(
        "orientation".to_string(),
        match config.orientation {
            OrientationMode::UniformSo3 => "shoemake_uniform_quaternion".to_string(),
            OrientationMode::Fixed => "fixed".to_string(),
        },
    );
    samplers.insert(
        "position".to_string(),
        match config.position.mode {
            // Uniform on the set of placements feasible given the particles
            // already accepted. The assembly is a random sequential adsorption
            // configuration, not an equilibrium hard-core one, and the name says
            // so rather than claiming more than the method gives.
            PositionMode::FeasibleUniform => "rejection_uniform_rsa".to_string(),
            // A deliberate construction. Never reported as random.
            PositionMode::VoidNeighbourhood => "void_neighbourhood_band".to_string(),
        },
    );

    ReportFile {
        schema_version: REPORT_SCHEMA.to_string(),
        status: "running".to_string(),
        tool: tool.clone(),
        seed: config.seed,
        rng: "chacha12".to_string(),
        runtime: RuntimeRecord {
            threads,
            elapsed_s: 0.0,
        },
        config: describe_input("config", &config.config_path),
        inputs,
        frame: frame.clone(),
        void: void_report,
        shapes: ShapesReport {
            files: library.sources.len(),
            shells_total: library.shells.len(),
            shells_rejected: library
                .rejected
                .iter()
                .map(|r| RejectedShellRow {
                    source_index: r.source_index,
                    shell_index: r.shell_index,
                    reason: r.reason.clone(),
                })
                .collect(),
            max_extent_ratio: library.max_extent_ratio,
        },
        target: TargetReport {
            volume_fraction: config.target_volume_fraction,
            basis: match config.target_basis {
                TargetBasis::Domain => "domain".to_string(),
                TargetBasis::Solid => "solid".to_string(),
            },
            basis_volume,
            tolerance: config.target_tolerance,
            distribution: match &config.distribution {
                ResolvedDistribution::Lognormal {
                    median,
                    sigma_log,
                    min,
                    max,
                } => format!(
                    "lognormal(median={median}, sigma_log={sigma_log}, min={min}, max={max})"
                ),
                ResolvedDistribution::Histogram {
                    path_as_written, ..
                } => format!("histogram({path_as_written})"),
            },
        },
        plan: PlanReport {
            planned_particles: plan.draws.len(),
            planned_volume: plan.planned_volume,
            planned_volume_error: plan.planned_volume_error,
        },
        actual: ActualReport {
            particles: 0,
            volume_in_domain: 0.0,
            volume_in_domain_solid: 0.0,
            volume_fraction_domain: 0.0,
            volume_fraction_solid: 0.0,
            median_diameter: None,
            mean_diameter: None,
            sigma_log_diameter: None,
        },
        size_classes: Vec::new(),
        top_up: TopUpReport {
            batches: 0,
            drawn: 0,
            placed: 0,
        },
        attempts: AttemptsReport {
            total: 0,
            per_particle_budget: config.budget.attempts_per_particle,
        },
        rejections: RejectReason::ALL
            .iter()
            .map(|r| (r.as_str().to_string(), 0usize))
            .collect(),
        budget: BudgetReport {
            total_attempts: config.budget.total_attempts,
            wall_time_s: config.budget.wall_time_s,
            consumed_attempts: 0,
            elapsed_s: 0.0,
            max_top_up_batches: config.budget.max_top_up_batches,
        },
        samplers,
        stop_reason: None,
        stop_detail: BTreeMap::new(),
        outputs: Vec::new(),
    }
}

// AI-FUNC-SUMMARY:
// Purpose: Describe the frozen void for the report, including how its volume was measured.
// Inputs: the config, the built index, the in-domain volume and the method that produced it.
// Returns: Some(report) when the run has a void.
// Side effects: Reads the void file to hash it.
// Notes: `sha256_out` is filled in later, once the copy exists. Recording both digests is what lets
// a reader check the "never filled, moved or cleaned" promise instead of trusting it.
fn build_void_report(
    config: &ResolvedPlacement,
    void: Option<&VoidIndex>,
    volume_in_domain: f64,
    method: Option<VoidVolumeMethod>,
) -> Result<Option<VoidReport>> {
    let (Some(v), Some(index)) = (config.void.as_ref(), void) else {
        return Ok(None);
    };
    let (sha256_in, _) = sha256_file(&v.file)?;
    Ok(Some(VoidReport {
        path: v.path_as_written.clone(),
        sha256_in,
        sha256_out: None,
        shells: index.shells(),
        orientation: if index.is_outward() { "outward" } else { "inward" }.to_string(),
        crossing: match v.crossing {
            VoidCrossing::Forbidden => "forbidden".to_string(),
            VoidCrossing::Allowed => "allowed".to_string(),
        },
        gap: v.gap,
        volume_total: index.total_volume(),
        volume_in_domain,
        volume_method: match method {
            Some(VoidVolumeMethod::ExactShellSum) => "exact_shell_sum".to_string(),
            Some(VoidVolumeMethod::ExactClip) => "exact_clip".to_string(),
            None => "none".to_string(),
        },
        inside_domain: method == Some(VoidVolumeMethod::ExactShellSum),
        overlap_voxel_size: v.overlap_voxel_size,
        overlap_owner: "void".to_string(),
    }))
}

// AI-FUNC-SUMMARY: Describe an input file with its digest for the report; returns FileEntry; side effects: reads the file.
fn describe_input(role: &str, path: &Path) -> FileEntry {
    match sha256_file(path) {
        Ok((sha256, bytes)) => FileEntry {
            role: role.to_string(),
            path: path.to_string_lossy().to_string(),
            sha256: Some(sha256),
            bytes: Some(bytes),
        },
        Err(_) => FileEntry {
            role: role.to_string(),
            path: path.to_string_lossy().to_string(),
            sha256: None,
            bytes: None,
        },
    }
}

// AI-FUNC-SUMMARY: Fill in everything the finished run knows; returns nothing; side effects: mutates the report.
#[allow(clippy::too_many_arguments)]
fn finish_report(
    report: &mut ReportFile,
    config: &ResolvedPlacement,
    state: &EngineState,
    classes: &[SizeClass],
    top_up: &TopUpReport,
    stop: &StopDecision,
    elapsed: f64,
    threads: usize,
    outputs: Vec<FileEntry>,
    target_volume: f64,
) {
    let mut diameters: Vec<f64> = state.placed.iter().map(|p| p.equivalent_diameter).collect();
    diameters.sort_by(|a, b| a.total_cmp(b));
    let n = diameters.len();
    let median = (n > 0).then(|| diameters[n / 2]);
    let mean = (n > 0).then(|| diameters.iter().sum::<f64>() / n as f64);
    let sigma_log = (n > 1).then(|| {
        let logs: Vec<f64> = diameters.iter().map(|d| d.ln()).collect();
        let m = logs.iter().sum::<f64>() / n as f64;
        (logs.iter().map(|l| (l - m) * (l - m)).sum::<f64>() / (n as f64 - 1.0)).sqrt()
    });

    report.status = "finished".to_string();
    report.runtime = RuntimeRecord { threads, elapsed_s: elapsed };
    report.actual = ActualReport {
        particles: n,
        volume_in_domain: state.volume_in_domain,
        volume_in_domain_solid: state.volume_solid,
        volume_fraction_domain: state.volume_in_domain / config.domain.volume(),
        volume_fraction_solid: state.volume_solid / (target_volume / config.target_volume_fraction),
        median_diameter: median,
        mean_diameter: mean,
        sigma_log_diameter: sigma_log,
    };
    report.size_classes = size_class_rows(state, classes);
    report.top_up = top_up.clone();
    report.attempts = AttemptsReport {
        total: state.attempts,
        per_particle_budget: config.budget.attempts_per_particle,
    };
    report.rejections = RejectReason::ALL
        .iter()
        .map(|r| {
            (
                r.as_str().to_string(),
                state.rejections.get(r).copied().unwrap_or(0),
            )
        })
        .collect();
    report.budget.consumed_attempts = state.attempts;
    report.budget.elapsed_s = elapsed;
    report.stop_reason = Some(stop.reason);
    report.stop_detail = stop.detail.clone();
    if let Some(v) = report.void.as_mut() {
        v.sha256_out = outputs
            .iter()
            .find(|o| o.role == "void")
            .and_then(|o| o.sha256.clone());
    }
    report.outputs = outputs;
}

// AI-FUNC-SUMMARY: Build the human-readable stdout summary; returns the lines; side effects: none.
fn summary(
    config: &ResolvedPlacement,
    state: &EngineState,
    stop: &StopDecision,
    target_volume: f64,
    basis_volume: f64,
) -> Vec<String> {
    let mut lines = vec![
        format!(
            "[Info] Placement seed {} ({}), {} thread setting",
            config.seed, "chacha12", config.threads
        ),
        format!(
            "[Info] Placed {} particle(s); volume fraction {:.6} of the {} basis (target {:.6})",
            state.placed.len(),
            state.volume_solid / basis_volume,
            match config.target_basis {
                TargetBasis::Domain => "domain",
                TargetBasis::Solid => "solid",
            },
            config.target_volume_fraction
        ),
        format!(
            "[Info] Attempts {} of a {} budget",
            state.attempts, config.budget.total_attempts
        ),
        format!(
            "[Info] Stop reason: {}",
            serde_json::to_string(&stop.reason)
                .unwrap_or_default()
                .trim_matches('"')
        ),
    ];
    if let Some(message) = stop.detail.get("message") {
        lines.push(format!("[Info] {message}"));
    }
    if state.shortfall_total > 0 {
        lines.push(format!(
            "[Warning] {} planned size(s) could not be placed; no replacement was drawn, so the \
             shortfall stands in its own size classes",
            state.shortfall_total
        ));
    }
    if state.volume_solid < target_volume * (1.0 - config.target_tolerance) {
        lines.push(format!(
            "[Warning] Target volume fraction not reached: {:.6} against {:.6}",
            state.volume_solid / basis_volume,
            config.target_volume_fraction
        ));
    }
    lines.push(format!(
        "[Info] Wrote {}",
        config.outputs.dir.display()
    ));
    lines
}

// Re-exported so tests can drive a run without going through stdout.
pub use crate::pipeline::placement_outputs::{ParticleRecord, RecordFile, ReportFile, StopReason};

// AI-FUNC-SUMMARY: Read a written record back, for tests and consumers; returns the parsed record; side effects: reads the file.
pub fn read_record(path: &Path) -> Result<RecordFile> {
    let text = std::fs::read_to_string(path)?;
    serde_json::from_str(&text).map_err(|e| {
        RustMsptError::InvalidConfig(format!("cannot parse {}: {e}", path.display()))
    })
}

// AI-FUNC-SUMMARY: Read a written report back, for tests and consumers; returns the parsed report; side effects: reads the file.
pub fn read_report(path: &Path) -> Result<ReportFile> {
    let text = std::fs::read_to_string(path)?;
    serde_json::from_str(&text).map_err(|e| {
        RustMsptError::InvalidConfig(format!("cannot parse {}: {e}", path.display()))
    })
}

