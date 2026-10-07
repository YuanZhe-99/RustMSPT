//! Durable state snapshots, separate from visualization exports and progress heartbeats.
use super::*;
use crate::pipeline::placement_sizes::SizePlan;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::io::Write;
use std::path::PathBuf;

const SCHEMA: &str = "rustmspt.placement.checkpoint/2";
#[derive(Default, Clone, Serialize, Deserialize)]
pub(super) struct IndividualCursor {
    pub plan: Option<SizePlan>,
    pub draws: Vec<SizeDraw>,
    pub next: usize,
    pub attempts_in_draw: usize,
    pub phase: String,
    pub top_up_batches: usize,
    pub top_up_drawn: usize,
    pub top_up_placed: usize,
    pub batch_before: usize,
    pub counted_current: bool,
    pub extensions: usize,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub failed_draws: Vec<SizeDraw>,
}
#[derive(Serialize, Deserialize)]
pub(super) struct SavedState {
    particles: Vec<ParticleRecord>,
    reaches: Vec<f64>,
    volume_in_domain: f64,
    volume_solid: f64,
    attempts: usize,
    rejections: BTreeMap<RejectReason, usize>,
    drawn: Vec<usize>,
    placed: Vec<usize>,
    top_drawn: Vec<usize>,
    top_placed: Vec<usize>,
    shortfall: usize,
    first_failed: Option<f64>,
    stopped_early: bool,
}
#[derive(Serialize, Deserialize)]
pub(super) struct Snapshot {
    schema: String,
    fingerprint: String,
    pub target_fraction: f64,
    pub total_budget: usize,
    pub mode: String,
    pub word_pos: String,
    pub state: SavedState,
    pub individual: IndividualCursor,
    pub aggregate: Value,
    pub complete: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub free_space: Option<free_space::Index>,
}
#[derive(Serialize, Deserialize)]
struct Envelope {
    checksum: String,
    snapshot: Snapshot,
}
pub(super) struct Store {
    fingerprint: String,
    path: PathBuf,
    last_saved: Instant,
    last_particles: usize,
}
// AI-FUNC-SUMMARY: Produce a descriptive configuration/compatibility error; no side effects.
fn invalid(message: impl Into<String>) -> RustMsptError {
    RustMsptError::InvalidConfig(message.into())
}
impl Store {
    // AI-FUNC-SUMMARY: Hash physical config, source bytes, histogram, void and exact executable; excludes output locations, worker/cache/checkpoint knobs and explicitly checked target/total budget.
    pub fn new(config: &ResolvedPlacement, library: &ShapeLibrary) -> Result<Option<Self>> {
        if !config.checkpoint.enabled {
            return Ok(None);
        }
        for output in [
            &config.outputs.particles_stl,
            &config.outputs.record,
            &config.outputs.report,
            &config.outputs.size_csv,
        ] {
            for reserved in [
                "checkpoint.json",
                "checkpoint.previous.json",
                "checkpoint.tmp.json",
                "checkpoint.previous.tmp.json",
            ] {
                if output == &config.outputs.dir.join(reserved) {
                    return Err(invalid(format!(
                        "placement output conflicts with reserved checkpoint file {reserved}"
                    )));
                }
            }
        }
        // Fingerprint actual resolved values, including in-process caller changes.
        // Debug formatting is stable within the exact executable checked below.
        let mut physical = config.clone();
        physical.target_volume_fraction = 0.0;
        physical.budget.total_attempts = 0;
        physical.threads = 0;
        physical.memory = Default::default();
        physical.checkpoint = Default::default();
        physical.config_path = PathBuf::new();
        physical.outputs.dir = PathBuf::new();
        physical.outputs.particles_stl = PathBuf::new();
        physical.outputs.record = PathBuf::new();
        physical.outputs.report = PathBuf::new();
        physical.outputs.size_csv = PathBuf::new();
        physical.outputs.per_particle_stl = false;
        physical.outputs.voxel_labels = None;
        physical.outputs.copy_void = false;
        let mut identity = format!("{physical:?}").into_bytes();
        for s in &library.sources {
            identity.extend_from_slice(s.sha256.as_bytes());
        }
        if let Some(initial) = &config.initial_particles {
            identity.extend_from_slice(sha256_file(&initial.record)?.0.as_bytes());
            identity.extend_from_slice(sha256_file(&initial.report)?.0.as_bytes());
            if let Some(path) = &initial.pending_checkpoint {
                identity.extend_from_slice(sha256_file(path)?.0.as_bytes());
            }
        }
        if let Some(v) = &config.void {
            identity.extend_from_slice(sha256_file(&v.file)?.0.as_bytes());
        }
        if let ResolvedDistribution::Histogram { csv, .. } = &config.distribution {
            identity.extend_from_slice(sha256_file(csv)?.0.as_bytes());
        }
        static EXECUTABLE_HASH: std::sync::OnceLock<String> = std::sync::OnceLock::new();
        let executable = match EXECUTABLE_HASH.get() {
            Some(s) => s.clone(),
            None => {
                let hash = sha256_file(&std::env::current_exe()?)?.0;
                let _ = EXECUTABLE_HASH.set(hash.clone());
                hash
            }
        };
        identity.extend_from_slice(executable.as_bytes());
        Ok(Some(Self {
            fingerprint: crate::io::sha256_bytes(&identity),
            path: config.outputs.dir.join("checkpoint.json"),
            last_saved: Instant::now(),
            last_particles: 0,
        }))
    }
    // AI-FUNC-SUMMARY: Decide periodic safe-point saving by elapsed time or newly accepted real particle count.
    pub fn due(&self, config: &ResolvedPlacement, count: usize) -> bool {
        (config.checkpoint.interval_seconds > 0
            && self.last_saved.elapsed().as_secs() >= config.checkpoint.interval_seconds)
            || (config.checkpoint.every_particles > 0
                && count.saturating_sub(self.last_particles) >= config.checkpoint.every_particles)
    }
    // AI-FUNC-SUMMARY: Read a checksum-verified generation, falling back only on missing/corrupt current file; refuse incompatible inputs, target changes without extend and reduced attempt budgets before writing outputs.
    pub fn load(&self, config: &ResolvedPlacement, mode: &str) -> Result<Option<Snapshot>> {
        let Some(path) = &config.checkpoint.resume_from else {
            return Ok(None);
        };
        let path = PathBuf::from(path);
        let decode = |p: &Path| -> Result<Snapshot> {
            let e: Envelope = serde_json::from_slice(&std::fs::read(p)?)
                .map_err(|e| invalid(format!("invalid checkpoint: {e}")))?;
            let bytes = serde_json::to_vec(&e.snapshot).map_err(|e| invalid(e.to_string()))?;
            if crate::io::sha256_bytes(&bytes) != e.checksum {
                return Err(invalid("checkpoint checksum mismatch"));
            }
            Ok(e.snapshot)
        };
        let snap = match decode(&path) {
            Ok(s) => s,
            Err(current_error) => {
                let previous = path.with_extension("previous.json");
                eprintln!(
                    "[Checkpoint] current generation unavailable ({current_error}); trying {}",
                    previous.display()
                );
                decode(&previous)?
            }
        };
        if snap.schema != SCHEMA || snap.fingerprint != self.fingerprint || snap.mode != mode {
            return Err(invalid("checkpoint is incompatible with this executable, input geometry or physical configuration"));
        }
        if config.target_volume_fraction != snap.target_fraction
            && (!config.checkpoint.extend || config.target_volume_fraction < snap.target_fraction)
        {
            return Err(invalid("a higher target requires checkpoint.extend=true; a lower target is not a continuation"));
        }
        if config.budget.total_attempts < snap.total_budget {
            return Err(invalid("resume cannot reduce the total attempt budget"));
        }
        if config.checkpoint.extend && !snap.complete {
            return Err(invalid(
                "extend requires a completed checkpoint; first resume the unfinished task",
            ));
        }
        Ok(Some(snap))
    }
    // AI-FUNC-SUMMARY: Atomically persist a checksummed snapshot and retain the last valid generation; fsync both files and directory on Unix before reporting success.
    pub fn save(&mut self, snap: Snapshot, count: usize) -> Result<()> {
        let bytes = serde_json::to_vec(&snap).map_err(|e| invalid(e.to_string()))?;
        let e = Envelope {
            checksum: crate::io::sha256_bytes(&bytes),
            snapshot: snap,
        };
        let tmp = self.path.with_extension("tmp.json");
        let mut f = std::fs::File::create(&tmp)?;
        serde_json::to_writer(&mut f, &e).map_err(|e| invalid(e.to_string()))?;
        f.flush()?;
        f.sync_all()?;
        if self.path.exists() {
            // Never replace a good previous generation with a corrupted current one.
            let valid = std::fs::read(&self.path)
                .ok()
                .and_then(|b| serde_json::from_slice::<Envelope>(&b).ok())
                .is_some_and(|e| {
                    serde_json::to_vec(&e.snapshot)
                        .is_ok_and(|b| crate::io::sha256_bytes(&b) == e.checksum)
                });
            if valid {
                let prev_tmp = self.path.with_extension("previous.tmp.json");
                std::fs::copy(&self.path, &prev_tmp)?;
                std::fs::File::open(&prev_tmp)?.sync_all()?;
                std::fs::rename(&prev_tmp, self.path.with_extension("previous.json"))?;
            }
        }
        std::fs::rename(&tmp, &self.path)?;
        #[cfg(unix)]
        std::fs::File::open(self.path.parent().unwrap())?.sync_all()?;
        self.last_saved = Instant::now();
        self.last_particles = count;
        Ok(())
    }
}
// AI-FUNC-SUMMARY: Capture exact accumulators, accepted transforms and cursor without any world triangle storage; no geometry queries.
pub(super) fn snapshot(
    config: &ResolvedPlacement,
    library: &ShapeLibrary,
    state: &EngineState,
    rng: &ChaCha12Rng,
    mode: &str,
    aggregate: Value,
    complete: bool,
) -> Snapshot {
    Snapshot {
        schema: SCHEMA.into(),
        fingerprint: state.checkpoint.as_ref().unwrap().fingerprint.clone(),
        target_fraction: config.target_volume_fraction,
        total_budget: config.budget.total_attempts,
        mode: mode.into(),
        word_pos: rng.get_word_pos().to_string(),
        state: SavedState {
            particles: state
                .placed
                .iter()
                .map(|p| particle_record(p, library))
                .collect(),
            reaches: state.placed.iter().map(|p| p.reach).collect(),
            volume_in_domain: state.volume_in_domain,
            volume_solid: state.volume_solid,
            attempts: state.attempts,
            rejections: state.rejections.clone(),
            drawn: state.drawn_per_class.clone(),
            placed: state.placed_per_class.clone(),
            top_drawn: state.top_up_drawn_per_class.clone(),
            top_placed: state.top_up_placed_per_class.clone(),
            shortfall: state.shortfall_total,
            first_failed: state.first_failed_diameter,
            stopped_early: state.stopped_early,
        },
        individual: state.cursor.clone(),
        aggregate,
        complete,
        free_space: state.free_space.clone(),
    }
}
// AI-FUNC-SUMMARY: Save at a committed safe point when due or forced; propagates I/O errors so no successful checkpoint is falsely reported.
#[allow(clippy::too_many_arguments)]
pub(super) fn save(
    config: &ResolvedPlacement,
    library: &ShapeLibrary,
    state: &mut EngineState,
    rng: &ChaCha12Rng,
    mode: &str,
    aggregate: Value,
    complete: bool,
    force: bool,
) -> Result<()> {
    if state
        .checkpoint
        .as_ref()
        .is_some_and(|s| force || s.due(config, state.placed.len()))
    {
        let snap = snapshot(config, library, state, rng, mode, aggregate, complete);
        state
            .checkpoint
            .as_mut()
            .unwrap()
            .save(snap, state.placed.len())?;
    }
    Ok(())
}
// AI-FUNC-SUMMARY: Restore records, ordered spatial index and RNG without restoring cached world triangles; validate source identities, triangle ranges and record lengths.
pub(super) fn restore(
    snap: &Snapshot,
    library: &ShapeLibrary,
    state: &mut EngineState,
    rng: &mut ChaCha12Rng,
) -> Result<()> {
    let saved = &snap.state;
    if saved.particles.len() != saved.reaches.len()
        || saved.drawn.len() != state.drawn_per_class.len()
        || saved.placed.len() != saved.drawn.len()
        || saved.top_drawn.len() != saved.drawn.len()
        || saved.top_placed.len() != saved.drawn.len()
    {
        return Err(invalid("checkpoint state lengths are inconsistent"));
    }
    for (i, p) in saved.particles.iter().enumerate() {
        let shell = library
            .shells
            .iter()
            .find(|s| {
                s.source_index == p.source_shape.source_index
                    && s.shell_index == p.source_shape.shell_index
                    && s.shell_sha256 == p.source_shape.shell_sha256
            })
            .ok_or_else(|| invalid("checkpoint source shell not found"))?;
        if p.acceptance_index != i
            || p.triangle_range[0] != state.triangle_count
            || p.triangle_range[1].checked_sub(p.triangle_range[0])
                != Some(shell.canonical.faces.len())
        {
            return Err(invalid(
                "checkpoint particle ordering/ranges are inconsistent",
            ));
        }
        let rotation = UnitQuat {
            w: p.rotation.quaternion[0],
            x: p.rotation.quaternion[1],
            y: p.rotation.quaternion[2],
            z: p.rotation.quaternion[3],
        };
        let translation = Vec3::new(p.translation[0], p.translation[1], p.translation[2]);
        let bbox = crate::types::BoundingBox {
            min: Vec3::new(p.bbox.min[0], p.bbox.min[1], p.bbox.min[2]),
            max: Vec3::new(p.bbox.max[0], p.bbox.max[1], p.bbox.max[2]),
        };
        let faces = p
            .clipped
            .faces
            .iter()
            .map(|s| match s.as_str() {
                "xmin" => Ok("xmin"),
                "xmax" => Ok("xmax"),
                "ymin" => Ok("ymin"),
                "ymax" => Ok("ymax"),
                "zmin" => Ok("zmin"),
                "zmax" => Ok("zmax"),
                _ => Err(invalid("invalid clipped face in checkpoint")),
            })
            .collect::<Result<Vec<_>>>()?;
        state.placed.push(PlacedParticle {
            acceptance_index: i,
            source_index: shell.source_index,
            shell_index: shell.shell_index,
            scale: p.scale,
            rotation,
            translation,
            equivalent_diameter: p.equivalent_diameter,
            reach: saved.reaches[i],
            size_class: p.size_class,
            volume_full: p.volume.full,
            volume_in_domain: p.volume.in_domain,
            void_overlap_volume: p.void_overlap_volume,
            clipped_faces: faces,
            bbox,
            mesh: Mesh::empty(),
            shape: None,
            geometry: Some(crate::pipeline::placement_geometry::GeometryHandle::new(
                i,
                shell.canonical.clone(),
                p.scale,
                rotation,
                translation,
                state.geometry_cache.clone(),
                state.simplified_collision,
            )),
            triangle_range: (p.triangle_range[0], p.triangle_range[1]),
        });
        state.grid.insert(i, bbox);
        state.triangle_count = p.triangle_range[1];
    }
    state.volume_in_domain = saved.volume_in_domain;
    state.volume_solid = saved.volume_solid;
    state.attempts = saved.attempts;
    state.rejections = saved.rejections.clone();
    state.drawn_per_class = saved.drawn.clone();
    state.placed_per_class = saved.placed.clone();
    state.top_up_drawn_per_class = saved.top_drawn.clone();
    state.top_up_placed_per_class = saved.top_placed.clone();
    state.shortfall_total = saved.shortfall;
    state.first_failed_diameter = saved.first_failed;
    state.stopped_early = saved.stopped_early;
    state.cursor = snap.individual.clone();
    state.free_space = snap.free_space.clone();
    if let Some(index) = &mut state.free_space {
        index.rebuild();
    }
    rng.set_word_pos(
        snap.word_pos
            .parse()
            .map_err(|_| invalid("invalid checkpoint RNG position"))?,
    );
    Ok(())
}

// AI-FUNC-SUMMARY: Import a verified remaining size multiset into a new algorithm task, retaining all accepted particles and optionally retrying original failed sizes; never bypass normal executable-bound resume.
pub(super) fn import_remaining_plan(
    config: &ResolvedPlacement,
    library: &ShapeLibrary,
    classes: &[SizeClass],
) -> Result<SizePlan> {
    let initial = config.initial_particles.as_ref().unwrap();
    let path = initial.pending_checkpoint.as_ref().unwrap();
    let envelope: Envelope = serde_json::from_slice(&std::fs::read(path)?)
        .map_err(|e| invalid(format!("invalid imported checkpoint: {e}")))?;
    let encoded = serde_json::to_vec(&envelope.snapshot).map_err(|e| invalid(e.to_string()))?;
    if crate::io::sha256_bytes(&encoded) != envelope.checksum {
        return Err(invalid("imported checkpoint checksum mismatch"));
    }
    let snap = envelope.snapshot;
    if !["rustmspt.placement.checkpoint/1", SCHEMA].contains(&snap.schema.as_str())
        || snap.mode != "individual"
        || snap.individual.phase != "primary"
        || snap.individual.extensions != 0
        || snap.individual.top_up_batches != 0
        || snap.target_fraction != config.target_volume_fraction
        || snap.individual.next > snap.individual.draws.len()
    {
        return Err(invalid("remaining-plan import supports an unextended individual primary plan with the same target"));
    }
    let record = read_record(&initial.record)?;
    let report = read_report(&initial.report)?;
    let digest = sha256_file(path)?.0;
    if !report
        .outputs
        .iter()
        .any(|e| e.role == "checkpoint" && e.sha256.as_ref() == Some(&digest))
        || serde_json::to_value(&snap.state.particles).map_err(|e| invalid(e.to_string()))?
            != serde_json::to_value(&record.particles).map_err(|e| invalid(e.to_string()))?
    {
        return Err(invalid(
            "imported plan does not match its frozen report/particle record",
        ));
    }
    let plan = snap
        .individual
        .plan
        .as_ref()
        .ok_or_else(|| invalid("imported plan missing"))?;
    let inherited = plan
        .draws
        .len()
        .checked_sub(snap.individual.draws.len())
        .ok_or_else(|| invalid("imported plan lengths inconsistent"))?;
    if inherited > record.particles.len() {
        return Err(invalid(
            "imported inherited count exceeds accepted population",
        ));
    }
    let mut accepted = BTreeMap::<(u64, usize), usize>::new();
    for p in &record.particles[inherited..] {
        *accepted
            .entry((p.equivalent_diameter.to_bits(), p.size_class))
            .or_default() += 1;
    }
    let mut failed = Vec::new();
    for draw in &snap.individual.draws[..snap.individual.next] {
        let key = (draw.diameter.to_bits(), draw.class);
        if let Some(n) = accepted.get_mut(&key).filter(|n| **n > 0) {
            *n -= 1;
        } else {
            failed.push(draw.clone());
        }
    }
    if accepted.values().any(|n| *n != 0) || failed.len() != snap.state.shortfall {
        return Err(invalid(
            "cannot reconcile accepted/failed original sizes; refuses inferred replacements",
        ));
    }
    let mut draws = snap.individual.draws[snap.individual.next..].to_vec();
    let pending = draws.len();
    if initial.retry_failed {
        draws.extend(failed.iter().cloned());
    }
    if record.particles.len().saturating_add(draws.len()) > MAX_PLANNED_PARTICLES {
        return Err(invalid("imported size plan exceeds member cap"));
    }
    for draw in &draws {
        if !draw.diameter.is_finite()
            || draw.diameter <= 0.0
            || draw.class
                != crate::pipeline::placement_sizes::class_for_diameter(classes, draw.diameter)
        {
            return Err(invalid("imported planned size/class invalid"));
        }
    }
    for p in &record.particles {
        if !library.shells.iter().any(|s| {
            s.shell_sha256 == p.source_shape.shell_sha256
                && s.source_index == p.source_shape.source_index
                && s.shell_index == p.source_shape.shell_index
        }) {
            return Err(invalid("imported plan source identity differs"));
        }
    }
    let planned_volume = draws
        .iter()
        .map(|d| std::f64::consts::PI * d.diameter * d.diameter * d.diameter / 6.0)
        .sum();
    eprintln!(
        "[InitialPlan] pending={} failed_original={} retry_failed={} new_draws={}",
        pending,
        failed.len(),
        initial.retry_failed,
        draws.len()
    );
    Ok(SizePlan {
        draws,
        planned_volume,
        target_volume: planned_volume,
        planned_volume_error: 0.0,
    })
}
