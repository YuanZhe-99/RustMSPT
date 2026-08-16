//! S4 - Sizing field and the S3<->S4 coupling loop
//! (PLAN §10.6, SPEC_meshgen_geometry §11 - the frozen update rule, monotonicity
//! argument, and termination guards).
//!
//! G3-2 owns the fixed-point **driver**: the update rule, the hysteresis band,
//! per-region locking, oscillation detection, the iteration cap, and the
//! post-loop G-8 ordering assertion. The sizing **constraint** `C(R)` - curvature,
//! feature proximity, local feature size, and the gap-regime terms of §10.6 - is
//! G4-1's and lives in the second half of this file; it enters the driver as a
//! caller-supplied closure so the frozen loop stays independent of what limits it.
//!
//! G4-1 (the sizing field) has three parts:
//!
//! 1. **Sources** (`collect_geometry_sources`, `gap_sources`) - a point plus the
//!    largest element allowed there, one per criterion of §10.6: discrete surface
//!    curvature under the chord-error tolerance, feature-curve curvature and
//!    corners, and local feature size from the S3 separation field.
//! 2. **The graded field** (`SizingLookup`) - `h(x) = min_s (h_s + beta*|x - s|)`
//!    clamped to `[h_min, h_max]`, with `beta = grading - 1`. Written this way the
//!    field is `beta`-Lipschitz **by construction**, which is the 2:1 gradation of
//!    §10.6: over a distance of one element the size may at most double. No
//!    smoothing pass and no relaxation sweep is needed, so grading cannot depend on
//!    iteration order or thread count.
//! 3. **The octree** (`build_sizing_field`) - the background subdivision the field
//!    lives on, refined level by level until every leaf is no larger than the field
//!    inside it. G4-2 balances and tetrahedralizes it; here it carries `sizing_h`
//!    and is what `s04_sizing` previews.
//!
//! **The scalar `h` in the coupling driver is the governing thin-feature size**, not
//! a global mesh size: `C(R)` is a minimum over the *thin regions* only. The plan
//! writes the frozen rule with a scalar `h` but its config sketch calls the regime
//! thresholds "x local h(x)"; taking the global minimum of the field would let one
//! sharp feature anywhere shrink the thresholds everywhere and decline every sheet
//! conversion in the model. The field itself stays spatial - it is the thresholds,
//! and only the thresholds, that read a single number off the thin regions.
//!
//! The rule the driver implements (SPEC_meshgen_geometry §11.1):
//!
//! ```text
//! h^(n+1) = max(h_min, min(h^(n), C(R^(n))))
//! R^(n+1) = regimes from t_sheet = tau_s * h^(n+1), t_layer = tau_l * h^(n+1)
//! ```
//!
//! `min(h^(n), .)` is normative: `h` is a running minimum and may never rise, and
//! Rule S3-M keeps every region's `t_r` fixed while it falls. Both together give
//! the one-way chain `Sheet -> Band -> Normal`, so a region changes regime at most
//! twice and the assignment stabilises.

use crate::error::{Result, RustMsptError};
use crate::io::vtu::{ArrayData, DataArray, VtuDoc, VTK_VOXEL};
use crate::meshgen::arrange::{ArrangeComponent, ArrangedSurface};
use crate::meshgen::features::FeatureSet;
use crate::meshgen::gapfield::{GapField, Regime, SkipReason};
use crate::meshgen::surface::ConditionedSurface;
use crate::types::Vec3;
use rayon::prelude::*;
use std::collections::BTreeMap;

// AI-FUNC-SUMMARY:
// Purpose: Inputs to the S3<->S4 coupling loop (thresholds, size bounds, guards).
// Notes: `tau_sheet`/`tau_layer` are the §6.3 gap factors, `hysteresis_enter`/`hysteresis_leave`
//   the 0.9/1.1 dead band, `max_iterations` the frozen cap of 5, and `h_tolerance` the 5% relative
//   change below which the loop is converged.
#[derive(Debug, Clone)]
pub struct CouplingOptions {
    pub tau_sheet: f64,
    pub tau_layer: f64,
    pub h_max: f64,
    pub h_min: f64,
    pub eps: f64,
    pub hysteresis_enter: f64,
    pub hysteresis_leave: f64,
    pub max_iterations: usize,
    pub h_tolerance: f64,
}

impl Default for CouplingOptions {
    // AI-FUNC-SUMMARY: PLAN §6.3 / SPEC §11.4 defaults for the coupling loop; returns CouplingOptions; side effects: none.
    fn default() -> Self {
        CouplingOptions {
            tau_sheet: 0.2,
            tau_layer: 1.0,
            h_max: 0.05,
            h_min: 0.002,
            eps: 1.0e-4,
            hysteresis_enter: 0.9,
            hysteresis_leave: 1.1,
            max_iterations: 5,
            h_tolerance: 0.05,
        }
    }
}

// AI-FUNC-SUMMARY: Why a region stopped participating in the loop; side effects: none.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum LockReason {
    /// The region tightened its regime (moved back toward `Sheet`); frozen there.
    Tightened,
    /// The region's regime differs from its value two iterations earlier.
    Oscillated,
    /// The loop hit the iteration cap with this region still unstable.
    IterationCap,
}

// AI-FUNC-SUMMARY:
// Purpose: The result of one coupling run: the converged size, the regimes, and every guard that fired.
// Notes: A locked region keeps the regime recorded here; oscillation and cap locks are always to
//   `Normal` (volumetric), never to `Sheet` - the frozen direction of the conservative fallback.
#[derive(Debug, Clone, Default)]
pub struct CouplingReport {
    pub h: f64,
    pub t_sheet: f64,
    pub t_layer: f64,
    pub iterations: usize,
    pub converged: bool,
    pub hit_cap: bool,
    pub regimes: Vec<Regime>,
    pub locks: Vec<Option<LockReason>>,
    pub changes: Vec<usize>,
}

impl CouplingReport {
    // AI-FUNC-SUMMARY: Region indices locked for the given reason; returns Vec<usize>; side effects: none.
    pub fn locked_for(&self, reason: LockReason) -> Vec<usize> {
        self.locks
            .iter()
            .enumerate()
            .filter(|(_, lock)| **lock == Some(reason))
            .map(|(index, _)| index)
            .collect()
    }
}

// AI-FUNC-SUMMARY:
// Purpose: Classify one region against the current thresholds, with the hysteresis dead band.
// Inputs: the region's frozen `t_r`, its previous regime, thresholds, and the enter/leave factors.
// Returns: the regime for this iteration.
// Side effects: None.
// Notes: A region enters a tighter regime only below `enter * threshold` and leaves it only above
//   `leave * threshold`, so a value sitting exactly on a threshold cannot flip every iteration.
//   Because transitions are one-way under a falling `h`, the dead band can delay a transition by at
//   most one iteration and can never create a cycle (SPEC_meshgen_geometry §11.3).
pub fn regime_for(
    t_r: f64,
    previous: Regime,
    t_sheet: f64,
    t_layer: f64,
    enter: f64,
    leave: f64,
) -> Regime {
    if !t_r.is_finite() {
        return Regime::Normal;
    }
    let sheet_bound = if previous == Regime::Sheet {
        leave * t_sheet
    } else {
        enter * t_sheet
    };
    if t_r <= sheet_bound {
        return Regime::Sheet;
    }
    let layer_bound = if previous == Regime::Normal {
        enter * t_layer
    } else {
        leave * t_layer
    };
    if t_r <= layer_bound {
        Regime::Band
    } else {
        Regime::Normal
    }
}

// AI-FUNC-SUMMARY: Whether `next` is tighter than `current` (moves back toward Sheet); returns bool; side effects: none.
fn tightens(current: Regime, next: Regime) -> bool {
    next > current
}

// AI-FUNC-SUMMARY:
// Purpose: Run the S3<->S4 fixed-point loop to a regime assignment and a converged sizing value.
// Inputs: each region's frozen `t_r` (Rule S3-M), options, and the sizing constraint `C(R)`.
// Returns: CouplingReport, or InvalidConfig when the post-loop G-8 ordering assertion fails.
// Side effects: Calls `constraint` once per iteration.
// Notes: `constraint` receives the current regimes and returns the sizing value they imply; the
//   driver applies the running minimum and the `h_min` floor itself, so a constraint that tries to
//   raise `h` cannot break monotonicity. Guards: a region that tightens locks immediately; a region
//   whose regime differs from its value two iterations earlier is locked to `Normal` with a WARN
//   caller-side; hitting the cap locks every still-changing region to `Normal`.
pub fn couple_gap_and_sizing<F>(
    t_r: &[f64],
    options: &CouplingOptions,
    mut constraint: F,
) -> Result<CouplingReport>
where
    F: FnMut(&[Regime], f64) -> f64,
{
    let mut h = options.h_max;
    let mut regimes: Vec<Regime> = vec![Regime::Normal; t_r.len()];
    let mut locks: Vec<Option<LockReason>> = vec![None; t_r.len()];
    let mut changes: Vec<usize> = vec![0; t_r.len()];
    let mut history: Vec<Vec<Regime>> = vec![Vec::new(); t_r.len()];

    // Iteration 0 assigns regimes from the bootstrap size before any constraint runs.
    for (index, value) in t_r.iter().enumerate() {
        regimes[index] = regime_for(
            *value,
            Regime::Normal,
            options.tau_sheet * h,
            options.tau_layer * h,
            options.hysteresis_enter,
            options.hysteresis_leave,
        );
        history[index].push(regimes[index]);
    }

    let mut iterations = 0usize;
    let mut converged = false;
    let mut hit_cap = false;
    while iterations < options.max_iterations {
        iterations += 1;
        let proposed = constraint(&regimes, h);
        let next_h = options.h_min.max(h.min(proposed));
        let relative_change = if h > 0.0 {
            (h - next_h).abs() / h
        } else {
            0.0
        };
        h = next_h;
        let t_sheet = options.tau_sheet * h;
        let t_layer = options.tau_layer * h;

        let mut any_change = false;
        for (index, value) in t_r.iter().enumerate() {
            if locks[index].is_some() {
                continue;
            }
            let previous = regimes[index];
            let next = regime_for(
                *value,
                previous,
                t_sheet,
                t_layer,
                options.hysteresis_enter,
                options.hysteresis_leave,
            );
            if next == previous {
                history[index].push(next);
                continue;
            }
            any_change = true;
            changes[index] += 1;
            // Oscillation: back to where this region stood two iterations ago.
            let two_back = history[index]
                .len()
                .checked_sub(2)
                .and_then(|position| history[index].get(position).copied());
            if two_back == Some(next) {
                regimes[index] = Regime::Normal;
                locks[index] = Some(LockReason::Oscillated);
                history[index].push(Regime::Normal);
                continue;
            }
            regimes[index] = next;
            history[index].push(next);
            if tightens(previous, next) {
                locks[index] = Some(LockReason::Tightened);
            }
        }

        if !any_change && relative_change < options.h_tolerance {
            converged = true;
            break;
        }
    }

    if !converged {
        hit_cap = true;
        for (index, lock) in locks.iter_mut().enumerate() {
            if lock.is_none() && changes[index] > 0 {
                *lock = Some(LockReason::IterationCap);
                regimes[index] = Regime::Normal;
            }
        }
    }

    let t_sheet = options.tau_sheet * h;
    let t_layer = options.tau_layer * h;
    // Post-loop assertion G-8, on the realised field rather than the configured one.
    if options.eps >= 0.5 * t_sheet || t_sheet >= t_layer || t_layer > h {
        return Err(RustMsptError::InvalidConfig(format!(
            "G-8 ordering assertion failed on the realised field: eps={} t_sheet={} t_layer={} h={}; \
             the loop must keep eps << t_sheet < t_layer <= h",
            options.eps, t_sheet, t_layer, h
        )));
    }

    Ok(CouplingReport {
        h,
        t_sheet,
        t_layer,
        iterations,
        converged,
        hit_cap,
        regimes,
        locks,
        changes,
    })
}

// ---------------------------------------------------------------------------
// G4-1: the sizing field
// ---------------------------------------------------------------------------

/// Hard ceiling on octree depth, independent of `h_min`. Level 12 is a 4096^3
/// virtual lattice - far past anything `h_min_frac` can reasonably ask for, and
/// the point where 32-bit cell coordinates stop being comfortable.
pub const SIZING_MAX_LEVEL: u32 = 12;
/// Leaf budget for one field. Refinement stops at the last level that fits, so
/// the cap is a *level* decision and never depends on visit order.
pub const SIZING_MAX_LEAVES: usize = 1_000_000;
/// How far above the requested size the realized field may sit between two LFS
/// cover points. A `beta`-Lipschitz field cannot be exactly constant over a region
/// covered by discrete sources; spacing the cover at `LFS_COVER_TOLERANCE * h / beta`
/// bounds the overshoot at `(1 + LFS_COVER_TOLERANCE) * h`. Halving it buys a factor
/// of two in the bound for eight times the sources (four in-plane, two across), which
/// is the wrong trade for a criterion whose input - a measured separation - is itself
/// only good to the sampling.
pub const LFS_COVER_TOLERANCE: f64 = 0.5;

/// Grid buckets per axis for the source lookup.
const SOURCE_GRID_MAX_DIM: i64 = 96;

// AI-FUNC-SUMMARY: Which §10.6 criterion produced a sizing source; side effects: none.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum SizingCriterion {
    /// Discrete surface curvature under the chord-error tolerance.
    Curvature,
    /// Feature-curve curvature (the same chord-error rule applied to a polyline).
    Feature,
    /// A corner or junction node: resolve its shortest incident feature segment.
    Corner,
    /// Proximity to a locked curve - a sharp edge, an S2 intersection curve, a rim or
    /// a non-manifold edge. Unlike `Feature`, which measures how much a curve *bends*,
    /// this asks only that the lattice be fine enough near the curve to represent it.
    Curve,
    /// Local feature size from the S3 separation field.
    Gap,
}

// AI-FUNC-SUMMARY:
// Purpose: One sizing constraint: at `point` no element may be larger than `h`.
// Notes: `h` is already clamped into `[h_min, h_max]` by the collector, so the field's
//   clamp is idempotent and a source can never ask for something the config forbids.
#[derive(Debug, Clone, Copy)]
pub struct SizingSource {
    pub point: Vec3,
    pub h: f64,
    pub criterion: SizingCriterion,
}

// AI-FUNC-SUMMARY:
// Purpose: Inputs to the sizing field: the domain, the size bounds, the four §10.6 criteria's
//   tolerances, the gradation, and the octree budget.
// Notes: Every length is in the normalized frame the pipeline works in (coordinates divided by the
//   domain diagonal), which is why `h_max`/`h_min` are the config's `*_frac` values unscaled.
#[derive(Debug, Clone)]
pub struct SizingOptions {
    pub domain_min: Vec3,
    pub domain_max: Vec3,
    pub h_max: f64,
    pub h_min: f64,
    pub chord_error_frac: f64,
    pub feature_angle_deg: f64,
    pub grading: f64,
    pub gap_cells: f64,
    /// Elements the lattice must fit across a locked curve's neighbourhood, so a cell
    /// straddles at most one curve. The curve analogue of `gap_cells`.
    pub curve_cells: f64,
    /// The envelope tolerance; one half of the local-feature-size floor below which
    /// a separation is not a gap the sizing field may chase - see `lfs_floor`.
    pub eps: f64,
    /// Cap on the barycentric grid one wall face may contribute to the LFS
    /// criterion, and the budget over all of them. A wall covered at spacing `h`
    /// contributes about as many sources as the octree will grow leaves there, so
    /// the budget is a guard against a pathological input, not a routine limit -
    /// when it binds, every face's spacing is scaled by one common factor, so the
    /// cover degrades uniformly rather than in whichever order faces were visited.
    pub lfs_points_per_face: usize,
    pub lfs_max_sources: usize,
    pub max_level: u32,
    pub max_leaves: usize,
}

impl Default for SizingOptions {
    // AI-FUNC-SUMMARY: PLAN §6.3 defaults over the unit domain; returns SizingOptions; side effects: none.
    fn default() -> Self {
        SizingOptions {
            domain_min: Vec3::new(0.0, 0.0, 0.0),
            domain_max: Vec3::new(1.0, 1.0, 1.0),
            h_max: 0.05,
            h_min: 0.002,
            chord_error_frac: 0.2,
            feature_angle_deg: 45.0,
            grading: 2.0,
            gap_cells: 4.0,
            curve_cells: 2.0,
            eps: 1.0e-4,
            lfs_points_per_face: 4096,
            lfs_max_sources: 2_000_000,
            max_level: SIZING_MAX_LEVEL,
            max_leaves: SIZING_MAX_LEAVES,
        }
    }
}

impl SizingOptions {
    // AI-FUNC-SUMMARY: The field's Lipschitz constant `grading - 1`; returns f64; side effects: none.
    pub fn beta(&self) -> f64 {
        (self.grading - 1.0).max(0.0)
    }

    // AI-FUNC-SUMMARY:
    // Purpose: The separation below which the local-feature-size criterion does not apply.
    // Returns: `max(eps, gap_cells * h_min)`.
    // Side effects: None.
    // Notes: Two floors, for two different reasons, and the criterion needs both.
    //   *Below `eps`* the surfaces are a contact: S2's coincidence policy already calls anything
    //   that close the same surface, and on real input (three intersecting reference bodies) the samples
    //   along an intersection curve measure `t = 0` exactly.
    //   *Below `gap_cells * h_min`* the gap is unresolvable: no permitted element size fits
    //   `gap_cells` elements across it, so the constraint would saturate at `h_min` and stay there.
    //   Both are the same statement - such a gap belongs to the band/sheet templates (or, if S3
    //   declined it, to G7's ladder), not to the sizing field. Leaving them in does real damage
    //   rather than merely wasting elements: because `C(R)` is a single scalar, one speck region
    //   would pin the coupling's thresholds to `h_min` and decline **every** band conversion in the
    //   model - which is exactly what the first run of this stage on `TestCaseIntersect1` did.
    pub fn lfs_floor(&self) -> f64 {
        self.eps.max(self.gap_cells * self.h_min)
    }

    // AI-FUNC-SUMMARY: Clamp one proposed size into `[h_min, h_max]`; returns f64; side effects: none.
    pub fn clamp_h(&self, h: f64) -> f64 {
        if !h.is_finite() {
            return self.h_max;
        }
        h.clamp(self.h_min, self.h_max)
    }

    // AI-FUNC-SUMMARY:
    // Purpose: The element size a chord-error tolerance permits on a feature of radius `radius`.
    // Returns: `2*R*sqrt(f*(2-f))` - the chord of a circle of radius `R` whose sagitta is `f*R`.
    // Side effects: None.
    // Notes: `f` is `chord_error_frac`, a *relative* sag; clamped below 1 so the square root stays
    //   real and a nonsensical tolerance degrades to "no constraint" rather than to a panic.
    fn h_for_radius(&self, radius: f64) -> f64 {
        if !radius.is_finite() || radius <= 0.0 {
            return self.h_max;
        }
        let f = self.chord_error_frac.clamp(1.0e-6, 0.999_999);
        2.0 * radius * (f * (2.0 - f)).sqrt()
    }
}

// ---------------------------------------------------------------------------
// Sources
// ---------------------------------------------------------------------------

// AI-FUNC-SUMMARY: Unnormalized triangle normal (twice the area vector); returns Vec3; side effects: none.
fn face_normal(a: Vec3, b: Vec3, c: Vec3) -> Vec3 {
    b.sub(a).cross(c.sub(a))
}

// AI-FUNC-SUMMARY: Euclidean length; returns f64; side effects: none.
fn length(v: Vec3) -> f64 {
    v.dot(v).sqrt()
}

// AI-FUNC-SUMMARY:
// Purpose: Radius of the circle through two segments meeting with turn angle `theta` at spacing `chord`.
// Returns: `chord / (2 sin(theta/2))`, or None when the turn is too small to imply any curvature.
// Side effects: None.
// Notes: `cos_theta` arrives as a normalized dot product, so `sin(theta/2) = sqrt((1-cos)/2)` is
//   computed algebraically - `acos` is never called (SPEC_meshgen_numerics §8.1).
fn radius_from_turn(cos_theta: f64, chord: f64) -> Option<f64> {
    let cos_theta = cos_theta.clamp(-1.0, 1.0);
    let half_sin_sq = (1.0 - cos_theta) * 0.5;
    // NaN-safe on both: a degenerate input must return None, not a NaN radius.
    if half_sin_sq.is_nan() || half_sin_sq <= 1.0e-12 || chord.is_nan() || chord <= 0.0 {
        return None;
    }
    Some(chord / (2.0 * half_sin_sq.sqrt()))
}

// AI-FUNC-SUMMARY:
// Purpose: Curvature sources - one per smooth interior edge of the **conditioned input** surface (§10.6).
// Inputs: the S0 conditioned surface and the sizing options.
// Returns: sources at the midpoints of edges whose discrete curvature asks for less than `h_max`.
// Side effects: None.
// Notes: A *sharp* edge (dihedral deviation beyond `feature_angle_deg`) is skipped: it is a feature
//   the mesh conforms to, not curvature to resolve, and treating its 90-degree turn as curvature
//   would drive every box corner to `h_min`. Edges asking for `h_max` or more emit nothing - the
//   field's ceiling already says that - which keeps the source set proportional to the *curved*
//   part of the input rather than to its triangle count.
//
//   **The length in the radius is the width *across* the edge, not the edge's own length.** The
//   dihedral across an edge measures how far the normal turns as you cross it, so the matching
//   distance is the two incident triangles' mean height over that edge, `(A1 + A2) / L` - never `L`
//   itself. The two agree only on an isotropic tessellation. Where they disagree the edge-length
//   form is simply wrong: on a UV sphere its short polar latitude edges carry a full-size dihedral,
//   so it reports a radius tending to zero at the poles and drives a perfectly uniform sphere to
//   `h_min` there. With the width it returns `1/R` in both directions everywhere on that sphere,
//   which is the right answer. (This is also why a *geodesic-neighbourhood* estimator, sketched as
//   G4-5 when the artifact was first seen, is not needed: the defect was a mismatched length, not a
//   too-local stencil.)
//
//   **It must be the conditioned surface, not the arranged one.** Both lengths are read from the
//   *sampling*, and S2 corefinement splits edges without changing the dihedral they carry: the same
//   physical curvature measured on a thrice-split edge returns a fraction of the radius, and a
//   fraction of the radius asks for a fraction of the element. Measured on the arranged surface of
//   `TestCaseIntersect1` this artifact alone drove the field to `h_min` around every intersection
//   curve. Curvature is a property of the input surface, and the input tessellation is where it is
//   read.
pub fn curvature_sources(
    surface: &ConditionedSurface,
    options: &SizingOptions,
) -> Vec<SizingSource> {
    let cos_feature = (options.feature_angle_deg.to_radians()).cos();
    let normals: Vec<Vec3> = surface
        .faces
        .par_iter()
        .map(|face| {
            face_normal(
                surface.vertices[face[0]],
                surface.vertices[face[1]],
                surface.vertices[face[2]],
            )
        })
        .collect();

    // Per-face edge lists collected into an indexed buffer, concatenated in index
    // order, then sorted - the permitted parallel shape (PLAN §12.5).
    let per_face: Vec<[(usize, usize, u32); 3]> = surface
        .faces
        .par_iter()
        .enumerate()
        .map(|(index, face)| {
            let n = *face;
            [
                (n[0].min(n[1]), n[0].max(n[1]), index as u32),
                (n[1].min(n[2]), n[1].max(n[2]), index as u32),
                (n[2].min(n[0]), n[2].max(n[0]), index as u32),
            ]
        })
        .collect();
    let mut edges: Vec<(usize, usize, u32)> = Vec::with_capacity(per_face.len() * 3);
    for triple in per_face {
        edges.extend_from_slice(&triple);
    }
    edges.par_sort_unstable();

    let mut groups: Vec<(usize, usize)> = Vec::new();
    let mut start = 0usize;
    for index in 1..=edges.len() {
        if index == edges.len() || (edges[index].0, edges[index].1) != (edges[start].0, edges[start].1)
        {
            groups.push((start, index));
            start = index;
        }
    }

    let per_group: Vec<Option<SizingSource>> = groups
        .par_iter()
        .map(|(lo, hi)| {
            // Only a manifold interior edge carries a dihedral angle; a rim edge has
            // no second face and a non-manifold edge is an S1 feature, not curvature.
            if hi - lo != 2 {
                return None;
            }
            let (va, vb, left) = edges[*lo];
            let right = edges[*lo + 1].2;
            let na = normals[left as usize];
            let nb = normals[right as usize];
            let norm_a = length(na);
            let norm_b = length(nb);
            if !(norm_a > 0.0 && norm_b > 0.0) {
                return None;
            }
            let cos_theta = na.dot(nb) / (norm_a * norm_b);
            if cos_theta < cos_feature {
                return None;
            }
            let pa = surface.vertices[va];
            let pb = surface.vertices[vb];
            let edge_length = length(pb.sub(pa));
            if edge_length.is_nan() || edge_length <= 0.0 {
                return None;
            }
            // The distance travelled *across* the edge, not along it: the mean of the
            // two incident triangles' heights over it, `(A1 + A2) / L` (each height is
            // `2A/L`). The dihedral measures how far the normal turns over exactly
            // that distance.
            let width = (norm_a + norm_b) * 0.5 / edge_length;
            let radius = radius_from_turn(cos_theta, width)?;
            let h = options.h_for_radius(radius);
            if h >= options.h_max {
                return None;
            }
            Some(SizingSource {
                point: pa.add(pb).scale(0.5),
                h: options.clamp_h(h),
                criterion: SizingCriterion::Curvature,
            })
        })
        .collect();

    per_group.into_iter().flatten().collect()
}

// AI-FUNC-SUMMARY:
// Purpose: Feature-proximity sources - feature-curve curvature plus corner/junction nodes (§10.6).
// Inputs: the S0 conditioned surface, the S1 feature set, and the options.
// Returns: sources along curves that bend, and one per corner node.
// Side effects: None.
// Notes: Read on the **input** tessellation, for the same reason as `curvature_sources`: both the
//   turn rule and the corner rule are chord-length rules, and a chord shortened by corefinement is
//   not a finer feature. That is also why S2's intersection curves contribute nothing here - they
//   have no input tessellation to be read on, their segment lengths are whatever the arrangement
//   produced, and the mesh conforms to them through S7/S8 rather than by refining near them.
//   A *straight* sharp edge likewise emits nothing: refining along every sharp edge would refine
//   every cube edge in the model for no accuracy gain. A corner asks for its shortest incident
//   segment, which is what makes a corner neighbourhood resolvable.
pub fn feature_sources(
    surface: &ConditionedSurface,
    features: &FeatureSet,
    options: &SizingOptions,
) -> Vec<SizingSource> {
    let mut sources: Vec<SizingSource> = Vec::new();
    // Shortest incident curve segment per node, for the corner rule.
    let mut shortest: BTreeMap<usize, f64> = BTreeMap::new();
    for curve in &features.curves {
        if curve.vertices.len() < 2 {
            continue;
        }
        let points: Vec<Vec3> = curve
            .vertices
            .iter()
            .map(|node| surface.vertices[*node])
            .collect();
        for window in 0..points.len() - 1 {
            let seg = length(points[window + 1].sub(points[window]));
            if seg.is_nan() || seg <= 0.0 {
                continue;
            }
            for node in [curve.vertices[window], curve.vertices[window + 1]] {
                let entry = shortest.entry(node).or_insert(f64::INFINITY);
                if seg < *entry {
                    *entry = seg;
                }
            }
        }
        for index in 1..points.len().saturating_sub(1) {
            let back = points[index].sub(points[index - 1]);
            let forward = points[index + 1].sub(points[index]);
            let (nb, nf) = (length(back), length(forward));
            if nb.is_nan() || nb <= 0.0 || nf.is_nan() || nf <= 0.0 {
                continue;
            }
            let cos_turn = back.dot(forward) / (nb * nf);
            let Some(radius) = radius_from_turn(cos_turn, nb.min(nf)) else {
                continue;
            };
            let h = options.h_for_radius(radius);
            if h >= options.h_max {
                continue;
            }
            sources.push(SizingSource {
                point: points[index],
                h: options.clamp_h(h),
                criterion: SizingCriterion::Feature,
            });
        }
    }
    for node in features.corners.iter().chain(features.junctions.iter()) {
        let Some(segment) = shortest.get(node) else {
            continue;
        };
        if !segment.is_finite() || *segment >= options.h_max {
            continue;
        }
        sources.push(SizingSource {
            point: surface.vertices[*node],
            h: options.clamp_h(*segment),
            criterion: SizingCriterion::Corner,
        });
    }
    sources
}

// AI-FUNC-SUMMARY:
// Purpose: Refine the lattice along every **locked curve** - sharp edges, S2 intersection curves,
//   rims and non-manifold edges - so a cell straddles at most one of them.
// Inputs: the clipped arranged surface and the sizing options.
// Returns: sources sampled along each curve at the spacing they ask for.
// Side effects: None.
// Notes: This is a *proximity* rule, and it is the third criterion because the other two are
//   **chord** rules and a chord rule cannot see these features at all. `curvature_sources` and
//   `feature_sources` both measure how much geometry *bends*; a cube's sharp edge is perfectly
//   straight and two surfaces crossing transversally bend not at all, so both rules emit nothing
//   and the field never refines toward the very curves the mesh has to reproduce. Measured before
//   this existed: A-1, A-2, A-3, A-4 and A-7 all reported `geometry sources: 0 total` while S1 was
//   reporting 12 to 24 curves, and A-8 reported 0 feature-curve sources against 180 sharp curves.
//
//   `feature_sources` records the previous reasoning - that intersection curves "have no input
//   tessellation to be read on ... and the mesh conforms to them through S7/S8 rather than by
//   refining near them". The first half is right and is why this reads segment *positions* and not
//   segment lengths. The second half was not true of the implementation: S7 only snaps nodes that
//   already happen to lie within `0.30 * l_min` of a curve, and S8 refuses to cut a cell two patches
//   cross at all. Neither can conform to a curve the lattice is too coarse to see.
//
//   The rule is a fixed factor rather than an adaptive one on purpose: `h_max / curve_cells` is
//   defensible without a second geometric query, and grading spreads it into a band. Making it
//   adapt to curve-to-curve separation is the natural extension and is deliberately not taken here,
//   so the effect of the criterion can be measured on its own.
pub fn curve_sources(arranged: &ArrangedSurface, options: &SizingOptions) -> Vec<SizingSource> {
    use crate::meshgen::arrange::ArrangedCurveKind;
    let mut sources: Vec<SizingSource> = Vec::new();
    if !options.curve_cells.is_finite() || options.curve_cells <= 0.0 {
        return sources;
    }
    let target = options.clamp_h(options.h_max / options.curve_cells);
    if target >= options.h_max {
        // Nothing to ask for: the ambient size already satisfies the rule.
        return sources;
    }
    // The step at which a segment is sampled. Sampling only at the endpoints leaves
    // the middle of a long segment coarse, and an arranged segment can be many times
    // `h_max` long - a cube edge is often one segment end to end.
    let step = target.max(f64::MIN_POSITIVE);
    for curve in &arranged.curves {
        // The domain box is not a feature the mesh conforms *to* - the lattice already
        // lies on it exactly - and refining along it would refine all twelve edges of
        // the domain for nothing.
        if curve.kind == ArrangedCurveKind::Box {
            continue;
        }
        for window in curve.nodes.windows(2) {
            let a = arranged.vertices[window[0]];
            let b = arranged.vertices[window[1]];
            let span = b.sub(a);
            let length = span.dot(span).sqrt();
            if !length.is_finite() || length <= 0.0 {
                continue;
            }
            let steps = (length / step).ceil().max(1.0);
            if !steps.is_finite() || steps > options.lfs_max_sources as f64 {
                continue;
            }
            let steps = steps as usize;
            for k in 0..=steps {
                let t = k as f64 / steps as f64;
                sources.push(SizingSource {
                    point: a.add(span.scale(t)),
                    h: target,
                    criterion: SizingCriterion::Curve,
                });
            }
        }
    }
    sources
}

// AI-FUNC-SUMMARY:
// Purpose: The two regime-independent criteria of §10.6, in one canonical list.
// Inputs: the S0 conditioned surface, the S1 feature set, and the sizing options.
// Returns: curvature + feature sources, sorted into the canonical source order.
// Side effects: None.
pub fn collect_geometry_sources(
    surface: &ConditionedSurface,
    features: &FeatureSet,
    options: &SizingOptions,
) -> Vec<SizingSource> {
    let mut sources = curvature_sources(surface, options);
    sources.extend(feature_sources(surface, features, options));
    sort_sources(&mut sources);
    sources
}

// AI-FUNC-SUMMARY:
// Purpose: Local-feature-size sources from the S3 separation field (§10.6), regime-aware.
// Inputs: the gap field, the *effective* regime per region, and the options.
// Returns: one source per measured sample that stays volumetric, asking for `t / gap_cells`.
// Side effects: None.
// Notes: This is the regime-dependent half of `C(R)`, and the reason the S3<->S4 loop exists at
//   all. A sample inside a region that was converted to a band or a sheet emits **nothing**: that
//   gap is meshed by a template, and asking to put two elements across it would refine exactly the
//   feature the conversion removes. A sample in a region that stayed volumetric because the thin
//   path *failed* (`LowConfidence`, `MidSurfaceInvalid`) must still be resolved, so it emits
//   `t / gap_cells`. A region declined as `Speck` or `IntersectionWedge` emits nothing either: its
//   small `t` reports a degenerate region, not a gap needing elements across, and letting it through
//   drives the field to the floor around a feature that carries no requirement.
//
//   **A separation at or below `lfs_floor` emits nothing** - it is a contact, or a gap no permitted
//   element size can span. See `SizingOptions::lfs_floor` for why both halves of that floor matter.
//
//   **The constraint covers each wall triangle, not the sample point alone.** A point source only
//   binds the field at that point; between two of them the `beta`-Lipschitz field rises by
//   `beta * d / 2`, so isolated samples on a coarsely tessellated wall leave the *middle* of the
//   gap unresolved. Rendering `s04` for G4-4 showed exactly that - a 0.02 gap whose field ran from
//   0.010 up to 0.031, coarser than the gap is wide. Each face therefore takes the tightest request
//   among its samples and is covered by a barycentric grid at half that size, **extruded across the
//   gap the face measured**. Both halves are needed and for different reasons: without the in-plane
//   grid the field rises between samples along the wall, and without the extrusion it rises between
//   the two walls - at the mid-plane of a gap of width `t` a wall-only cover gives
//   `h + beta*t/2`, which for the natural `h = t/gap_cells` is wider than the gap itself.
pub fn gap_sources(
    surface: &ArrangedSurface,
    field: &GapField,
    effective_regimes: &[Regime],
    options: &SizingOptions,
) -> Vec<SizingSource> {
    let mut converted = vec![false; field.samples.len()];
    for region in &field.regions {
        let regime = effective_regimes
            .get(region.id)
            .copied()
            .unwrap_or(region.regime);
        // A converted region is meshed by the thin path, so its gap needs no elements
        // across. A region DECLINED as `Speck` or `IntersectionWedge` is suppressed for
        // the opposite reason: its `t` is small because the region is degenerate, not
        // because a real gap needs resolving there. A wedge closes to zero thickness at
        // the intersection curve and a speck is smaller than one bootstrap element, so
        // `t / gap_cells` drives the field to the floor around a feature that carries no
        // requirement - A-7b built 361,163 sources, 360,835 of them from declined
        // regions, taking `h` from 0.069 to 0.010 and the mesh to 1.2M tets for two
        // boxes. Resolution at an intersection curve is `curve_sources`' job (§10.6).
        // `LowConfidence` and `MidSurfaceInvalid` are NOT suppressed: those are real gaps
        // the thin path failed to take, and they still have to be meshed volumetrically.
        // `Undersampled` is, and the plate fixtures are why: a one- or two-sample fragment
        // left over beside a converted band region is a piece of *that same gap*, and
        // letting it ask for `t / gap_cells` puts two elements across a gap the band
        // conversion has already undertaken to mesh with one - 340,360 LFS sources and a
        // ten-fold mesh on a7b, from seventeen fragments of a gap that was measured
        // correctly. A measurement too sparse for S3 to act on is too sparse to bind the
        // field to its floor.
        let declined = matches!(
            region.skip,
            Some(SkipReason::Speck) | Some(SkipReason::Undersampled)
        );
        if regime == Regime::Normal && !declined {
            continue;
        }
        for sample in &region.samples {
            if let Some(slot) = converted.get_mut(*sample) {
                *slot = true;
            }
        }
    }
    let floor = options.lfs_floor();

    // What each sample asks for, which arranged face (if any) it sits on, and the
    // vector across the gap it measured - the slab the constraint has to cover.
    let per_sample: Vec<Option<(f64, Vec3, i64, Vec3)>> = field
        .samples
        .par_iter()
        .enumerate()
        .map(|(index, sample)| {
            if converted[index] {
                return None;
            }
            let t = if sample.t_exact.is_finite() {
                sample.t_exact
            } else {
                sample.t
            };
            if !t.is_finite() || t <= floor {
                return None;
            }
            let h = t / options.gap_cells;
            if h >= options.h_max {
                return None;
            }
            let face = field
                .face_of_tri
                .get(sample.tri)
                .copied()
                .unwrap_or(-1);
            // Prefer the measured correspondence over the ray direction: an oblique
            // gap is crossed by the segment to the paired point, not by the normal.
            let across = match &sample.pairing {
                Some(pairing) => pairing.point.sub(sample.point),
                None => sample.direction.scale(t),
            };
            Some((options.clamp_h(h), sample.point, face, across))
        })
        .collect();

    // The tightest request per face, folded serially in sample order - an exact
    // `min`, so the value cannot depend on how the map was scheduled.
    let mut face_h: Vec<f64> = vec![f64::INFINITY; surface.faces.len()];
    let mut face_across: Vec<Vec3> = vec![Vec3::new(0.0, 0.0, 0.0); surface.faces.len()];
    let mut loose: Vec<SizingSource> = Vec::new();
    for entry in per_sample.into_iter().flatten() {
        let (h, point, face, across) = entry;
        if face >= 0 && (face as usize) < face_h.len() {
            let slot = &mut face_h[face as usize];
            if h < *slot {
                *slot = h;
                face_across[face as usize] = across;
            }
        } else {
            // A sample on a virtual domain wall has no arranged face to cover.
            loose.push(SizingSource {
                point,
                h,
                criterion: SizingCriterion::Gap,
            });
        }
    }

    let beta = options.beta().max(1.0e-12);
    let triangle_of = |index: usize| -> [Vec3; 3] {
        let face = &surface.faces[index];
        [
            surface.vertices[face.nodes[0]],
            surface.vertices[face.nodes[1]],
            surface.vertices[face.nodes[2]],
        ]
    };
    // Under `beta`-Lipschitz growth a point `d` from the nearest cover point sits
    // at `h + beta*d`, so this spacing bounds the realized field over the gap at
    // `(1 + LFS_COVER_TOLERANCE) * h`.
    let base_spacing = |h: f64| -> f64 { LFS_COVER_TOLERANCE * h / beta };
    let counts: Vec<usize> = face_h
        .par_iter()
        .enumerate()
        .map(|(index, h)| {
            if !h.is_finite() {
                return 0;
            }
            let spacing = base_spacing(*h);
            let grid = grid_points(triangle_of(index), spacing, options.lfs_points_per_face);
            let span = length(face_across[index]);
            let layers = if span > spacing {
                (span / spacing).ceil() as usize
            } else {
                0
            };
            grid * (layers + 1)
        })
        .collect();
    let total: usize = counts.iter().sum();
    // One common relaxation factor when the budget binds - never a per-face
    // decision, which would depend on the order faces were considered in.
    let relax = if total > options.lfs_max_sources && options.lfs_max_sources > 0 {
        (total as f64 / options.lfs_max_sources as f64).sqrt()
    } else {
        1.0
    };
    let per_face: Vec<Vec<SizingSource>> = face_h
        .par_iter()
        .enumerate()
        .map(|(index, h)| {
            if !h.is_finite() {
                return Vec::new();
            }
            let spacing = base_spacing(*h) * relax;
            let cover = triangle_grid(
                triangle_of(index),
                spacing,
                options.lfs_points_per_face,
            );
            // Extrude the wall cover across the gap it measured. Without this the
            // constraint holds on the walls and fails in between: the field at the
            // mid-plane of a gap of width `t` is `h + beta*t/2`, which for the
            // natural `h = t/gap_cells` is *wider than the gap itself*. Sources
            // have to fill the slab, not bound it.
            let across = face_across[index];
            let span = length(across);
            let layers = if span > spacing {
                (span / spacing).ceil() as usize
            } else {
                0
            };
            let mut out = Vec::with_capacity(cover.len() * (layers + 1));
            for point in cover {
                out.push(SizingSource {
                    point,
                    h: *h,
                    criterion: SizingCriterion::Gap,
                });
                for layer in 1..=layers {
                    out.push(SizingSource {
                        point: point.add(across.scale(layer as f64 / layers as f64)),
                        h: *h,
                        criterion: SizingCriterion::Gap,
                    });
                }
            }
            out
        })
        .collect();

    let mut sources: Vec<SizingSource> = loose;
    for face_sources in per_face {
        sources.extend(face_sources);
    }
    sort_sources(&mut sources);
    sources
}

// AI-FUNC-SUMMARY: Barycentric divisions a triangle needs at the given spacing, capped; returns None for a degenerate triangle; side effects: none.
fn grid_divisions(triangle: [Vec3; 3], spacing: f64, max_points: usize) -> Option<usize> {
    let edge = |a: Vec3, b: Vec3| length(b.sub(a));
    let longest = edge(triangle[0], triangle[1])
        .max(edge(triangle[1], triangle[2]))
        .max(edge(triangle[2], triangle[0]));
    if longest.is_nan() || longest <= 0.0 || spacing.is_nan() || spacing <= 0.0 {
        return None;
    }
    let mut n = ((longest / spacing).ceil() as usize).max(1);
    // (n+1)(n+2)/2 points; solve the cap backwards rather than generating and truncating.
    while n > 1 && (n + 1) * (n + 2) / 2 > max_points.max(3) {
        n -= 1;
    }
    Some(n)
}

// AI-FUNC-SUMMARY: How many points `triangle_grid` would emit for this triangle; returns the count; side effects: none.
fn grid_points(triangle: [Vec3; 3], spacing: f64, max_points: usize) -> usize {
    grid_divisions(triangle, spacing, max_points)
        .map(|n| (n + 1) * (n + 2) / 2)
        .unwrap_or(3)
}

// AI-FUNC-SUMMARY:
// Purpose: Cover a triangle with a barycentric point grid no coarser than `spacing`.
// Inputs: the triangle, the target spacing, and a cap on the number of points.
// Returns: the grid, always including the three vertices.
// Side effects: None.
// Notes: `n` is chosen from the longest edge and then clamped so the point count stays under the
//   cap, so a large triangle asking for a tiny size degrades to a coarser cover rather than to an
//   unbounded one. The grid is generated in a fixed index order and carries no floating-point
//   accumulation - `(i*a + j*b + k*c)/n` is evaluated from the barycentric integers each time.
fn triangle_grid(triangle: [Vec3; 3], spacing: f64, max_points: usize) -> Vec<Vec3> {
    let Some(n) = grid_divisions(triangle, spacing, max_points) else {
        return vec![triangle[0], triangle[1], triangle[2]];
    };
    let mut out = Vec::with_capacity((n + 1) * (n + 2) / 2);
    let scale = 1.0 / n as f64;
    for i in 0..=n {
        for j in 0..=(n - i) {
            let k = n - i - j;
            out.push(
                triangle[0]
                    .scale(i as f64 * scale)
                    .add(triangle[1].scale(j as f64 * scale))
                    .add(triangle[2].scale(k as f64 * scale)),
            );
        }
    }
    out
}

// AI-FUNC-SUMMARY:
// Purpose: Put a source list into the canonical order (position, then size, then criterion).
// Side effects: Sorts in place.
// Notes: The field's value is a `min` and so is order-independent by construction; the order exists
//   so that two runs print, hash and snapshot the same list.
fn sort_sources(sources: &mut [SizingSource]) {
    sources.par_sort_unstable_by(|left, right| {
        left.point
            .x
            .total_cmp(&right.point.x)
            .then(left.point.y.total_cmp(&right.point.y))
            .then(left.point.z.total_cmp(&right.point.z))
            .then(left.h.total_cmp(&right.h))
            .then(left.criterion.cmp(&right.criterion))
    });
}

// ---------------------------------------------------------------------------
// The graded field
// ---------------------------------------------------------------------------

// AI-FUNC-SUMMARY:
// Purpose: `h(x) = clamp(min_s (h_s + beta*|x - s|))` - the sizing field of §10.6 with its 2:1
//   gradation built in rather than relaxed in.
// Notes: Being a minimum of `beta`-Lipschitz functions the result is `beta`-Lipschitz, so
//   `|h(x) - h(y)| <= beta*|x - y|` holds everywhere with no smoothing pass, no neighbour sweep and
//   no iteration order to get wrong. Sources are bucketed in a uniform grid and searched in
//   expanding Chebyshev rings; a ring is skipped once even the smallest source in it could not beat
//   the running best.
#[derive(Debug, Clone)]
pub struct SizingLookup {
    sources: Vec<SizingSource>,
    origin: Vec3,
    cell: f64,
    dims: [i64; 3],
    buckets: Vec<Vec<u32>>,
    smallest: f64,
    beta: f64,
    h_min: f64,
    h_max: f64,
}

impl SizingLookup {
    // AI-FUNC-SUMMARY:
    // Purpose: Bucket a source list into the lookup grid.
    // Inputs: canonical source list, sizing options.
    // Returns: SizingLookup; an empty list yields the constant field `h_max`.
    // Side effects: None.
    pub fn build(sources: Vec<SizingSource>, options: &SizingOptions) -> SizingLookup {
        let mut min = Vec3::new(f64::INFINITY, f64::INFINITY, f64::INFINITY);
        let mut max = Vec3::new(f64::NEG_INFINITY, f64::NEG_INFINITY, f64::NEG_INFINITY);
        let mut smallest = f64::INFINITY;
        for source in &sources {
            min = Vec3::new(
                min.x.min(source.point.x),
                min.y.min(source.point.y),
                min.z.min(source.point.z),
            );
            max = Vec3::new(
                max.x.max(source.point.x),
                max.y.max(source.point.y),
                max.z.max(source.point.z),
            );
            smallest = smallest.min(source.h);
        }
        if sources.is_empty() {
            min = Vec3::new(0.0, 0.0, 0.0);
            max = Vec3::new(1.0, 1.0, 1.0);
            smallest = options.h_max;
        }
        let span = max.sub(min);
        let longest = span.x.max(span.y).max(span.z).max(f64::MIN_POSITIVE);
        let per_axis = (sources.len() as f64).cbrt().ceil().max(1.0);
        let side = (per_axis as i64).clamp(1, SOURCE_GRID_MAX_DIM);
        let cell = (longest / side as f64).max(f64::MIN_POSITIVE);
        let dim = |extent: f64| -> i64 { ((extent / cell).floor() as i64 + 1).clamp(1, side) };
        let dims = [dim(span.x), dim(span.y), dim(span.z)];
        let mut buckets = vec![Vec::new(); (dims[0] * dims[1] * dims[2]) as usize];
        for (index, source) in sources.iter().enumerate() {
            let key = Self::bucket_of(min, cell, dims, source.point);
            buckets[Self::flat(dims, key)].push(index as u32);
        }
        SizingLookup {
            sources,
            origin: min,
            cell,
            dims,
            buckets,
            smallest,
            beta: options.beta(),
            h_min: options.h_min,
            h_max: options.h_max,
        }
    }

    // AI-FUNC-SUMMARY: Grid coordinates of one point, clamped into the grid; returns [i64; 3]; side effects: none.
    fn bucket_of(origin: Vec3, cell: f64, dims: [i64; 3], point: Vec3) -> [i64; 3] {
        let axis = |value: f64, base: f64, dim: i64| -> i64 {
            (((value - base) / cell).floor() as i64).clamp(0, dim - 1)
        };
        [
            axis(point.x, origin.x, dims[0]),
            axis(point.y, origin.y, dims[1]),
            axis(point.z, origin.z, dims[2]),
        ]
    }

    // AI-FUNC-SUMMARY: Flatten grid coordinates into a bucket index; returns usize; side effects: none.
    fn flat(dims: [i64; 3], key: [i64; 3]) -> usize {
        (key[2] * dims[1] * dims[0] + key[1] * dims[0] + key[0]) as usize
    }

    // AI-FUNC-SUMMARY: Number of sources behind this field; returns usize; side effects: none.
    pub fn len(&self) -> usize {
        self.sources.len()
    }

    // AI-FUNC-SUMMARY: Whether the field is the constant `h_max`; returns bool; side effects: none.
    pub fn is_empty(&self) -> bool {
        self.sources.is_empty()
    }

    // AI-FUNC-SUMMARY: The field's sources in canonical order; returns a slice; side effects: none.
    pub fn sources(&self) -> &[SizingSource] {
        &self.sources
    }

    // AI-FUNC-SUMMARY: Evaluate the graded sizing field at one point; returns f64; side effects: none.
    pub fn eval(&self, point: Vec3) -> f64 {
        self.eval_box(point, point)
    }

    // AI-FUNC-SUMMARY:
    // Purpose: The **exact** minimum of the graded field over an axis-aligned box.
    // Returns: `clamp(min_s (h_s + beta*dist(box, s)), h_min, h_max)`.
    // Side effects: None.
    // Notes: This is what the octree refines against, and it is exact rather than a bound because
    //   the source-to-box distance is closed-form: no cell is ever split because a *bound* was
    //   pessimistic, which is what a "centre value minus beta times the half diagonal" estimate does
    //   to a field that is locally flat (it splits a uniform field one level too far, everywhere).
    //   Rings expand from the box's own bucket range; a source in ring `r >= 1` is at least
    //   `(r-1)*cell` away, so once `smallest + beta*(r-1)*cell` reaches the running best no later
    //   ring can improve it and the search stops. The running best is a serial `min` fold, which is
    //   exact and order-independent - no float reduction crosses a thread boundary.
    pub fn eval_box(&self, lo: Vec3, hi: Vec3) -> f64 {
        if self.sources.is_empty() {
            return self.h_max;
        }
        let key_lo = Self::bucket_of(self.origin, self.cell, self.dims, lo);
        let key_hi = Self::bucket_of(self.origin, self.cell, self.dims, hi);
        let mut best = self.h_max;
        let reach = self.dims[0].max(self.dims[1]).max(self.dims[2]);
        for ring in 0..=reach {
            if ring >= 1 {
                let bound = self.smallest + self.beta * (ring - 1) as f64 * self.cell;
                if bound >= best {
                    break;
                }
            }
            for z in (key_lo[2] - ring)..=(key_hi[2] + ring) {
                if z < 0 || z >= self.dims[2] {
                    continue;
                }
                for y in (key_lo[1] - ring)..=(key_hi[1] + ring) {
                    if y < 0 || y >= self.dims[1] {
                        continue;
                    }
                    for x in (key_lo[0] - ring)..=(key_hi[0] + ring) {
                        if x < 0 || x >= self.dims[0] {
                            continue;
                        }
                        // Only the shell around the previous ring is new here.
                        let shell = |value: i64, lo: i64, hi: i64| -> bool {
                            value == lo - ring || value == hi + ring
                        };
                        if ring > 0
                            && !shell(x, key_lo[0], key_hi[0])
                            && !shell(y, key_lo[1], key_hi[1])
                            && !shell(z, key_lo[2], key_hi[2])
                        {
                            continue;
                        }
                        for index in &self.buckets[Self::flat(self.dims, [x, y, z])] {
                            let source = &self.sources[*index as usize];
                            let distance = box_distance(lo, hi, source.point);
                            let value = source.h + self.beta * distance;
                            if value < best {
                                best = value;
                            }
                        }
                    }
                }
            }
        }
        best.clamp(self.h_min, self.h_max)
    }
}

// AI-FUNC-SUMMARY: Distance from a point to an axis-aligned box (zero inside); returns f64; side effects: none.
fn box_distance(lo: Vec3, hi: Vec3, point: Vec3) -> f64 {
    let axis = |value: f64, lo: f64, hi: f64| -> f64 { (lo - value).max(value - hi).max(0.0) };
    let dx = axis(point.x, lo.x, hi.x);
    let dy = axis(point.y, lo.y, hi.y);
    let dz = axis(point.z, lo.z, hi.z);
    (dx * dx + dy * dy + dz * dz).sqrt()
}

// ---------------------------------------------------------------------------
// The background octree
// ---------------------------------------------------------------------------

// AI-FUNC-SUMMARY:
// Purpose: One octree leaf: its integer position at its own level, and the size the field allows in it.
// Notes: The leaf's side is `root_size / 2^level` and its minimum corner is
//   `origin + coord * side`, so leaf geometry is exact in the integer lattice and two leaves at
//   different levels still share exact corner coordinates.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SizingLeaf {
    pub coord: [u32; 3],
    pub level: u32,
    pub h: f64,
}

// AI-FUNC-SUMMARY: What the octree build produced and which budgets it hit; side effects: none.
#[derive(Debug, Clone, Default)]
pub struct SizingStats {
    pub n_sources: usize,
    pub n_leaves: usize,
    pub h_smallest: f64,
    pub h_largest: f64,
    pub max_level_used: u32,
    /// Leaves that are still larger than the field asks for, because refining them
    /// would have crossed `max_level` or the leaf budget.
    pub n_unresolved: usize,
    pub level_capped: bool,
    pub leaf_capped: bool,
}

// AI-FUNC-SUMMARY:
// Purpose: The background octree the sizing field lives on (PLAN §10.6/§10.7; G4-2 balances it).
// Notes: `leaves` is in canonical `(level, coord)` order, which is the order the VTU is written in
//   and the order any downstream stage may rely on.
#[derive(Debug, Clone)]
pub struct SizingField {
    pub origin: Vec3,
    pub root_size: f64,
    pub max_level: u32,
    pub leaves: Vec<SizingLeaf>,
    pub stats: SizingStats,
}

impl SizingField {
    // AI-FUNC-SUMMARY: Side length of a cell at the given level; returns f64; side effects: none.
    pub fn level_size(&self, level: u32) -> f64 {
        self.root_size / (1u64 << level) as f64
    }

    // AI-FUNC-SUMMARY: Minimum corner of one leaf in world coordinates; returns Vec3; side effects: none.
    pub fn leaf_min(&self, leaf: &SizingLeaf) -> Vec3 {
        let size = self.level_size(leaf.level);
        Vec3::new(
            self.origin.x + leaf.coord[0] as f64 * size,
            self.origin.y + leaf.coord[1] as f64 * size,
            self.origin.z + leaf.coord[2] as f64 * size,
        )
    }

    // AI-FUNC-SUMMARY: Centre of one leaf in world coordinates; returns Vec3; side effects: none.
    pub fn leaf_center(&self, leaf: &SizingLeaf) -> Vec3 {
        let half = self.level_size(leaf.level) * 0.5;
        self.leaf_min(leaf).add(Vec3::new(half, half, half))
    }

    // AI-FUNC-SUMMARY:
    // Purpose: The leaf containing a point, if the point lies in a populated part of the root cube.
    // Returns: index into `leaves`, or None outside the tree.
    // Side effects: None.
    // Notes: Walks from the deepest level up, so the first hit is the leaf that actually owns the
    //   point. `leaves` is sorted by `(level, coord)`, so each level is one binary search and the
    //   whole query is `O(max_level * log n)` with no allocation. Cells outside the domain box were
    //   dropped during refinement, hence the `Option`.
    pub fn locate(&self, point: Vec3) -> Option<usize> {
        let finest = self.level_size(self.max_level);
        let axis = |value: f64, base: f64| -> Option<u32> {
            let index = ((value - base) / finest).floor();
            if index < 0.0 || index >= (1u64 << self.max_level) as f64 {
                return None;
            }
            Some(index as u32)
        };
        let deep = [
            axis(point.x, self.origin.x)?,
            axis(point.y, self.origin.y)?,
            axis(point.z, self.origin.z)?,
        ];
        for level in (0..=self.max_level).rev() {
            let shift = self.max_level - level;
            let coord = [deep[0] >> shift, deep[1] >> shift, deep[2] >> shift];
            if let Ok(slot) = self
                .leaves
                .binary_search_by_key(&(level, coord), |leaf| (leaf.level, leaf.coord))
            {
                return Some(slot);
            }
        }
        None
    }

    // AI-FUNC-SUMMARY: The sizing value stored in the leaf containing `point`; returns Option<f64>; side effects: none.
    pub fn sample(&self, point: Vec3) -> Option<f64> {
        self.locate(point).map(|slot| self.leaves[slot].h)
    }
}

// AI-FUNC-SUMMARY:
// Purpose: Refine the background octree until every leaf is no larger than the field inside it.
// Inputs: the graded field, and the options carrying the domain, `h_min`, and the budgets.
// Returns: SizingField with leaves in canonical `(level, coord)` order.
// Side effects: None.
// Notes: One parallel pass per level (per-cell decisions into an indexed buffer, consumed in index
//   order - PLAN §12.5), so no decision can see another cell's result and thread count cannot reach
//   the output. The split test compares the cell's side against `eval_box` - the field's exact
//   minimum over that cell - so a leaf is never larger than the smallest element any point inside it
//   asks for, and a cell is never split because an estimate was pessimistic. Both budgets are
//   enforced level by level - the whole next level is refused or accepted together - so hitting one
//   cannot make the result depend on traversal order.
//
//   **Cells that miss the domain box are dropped, but only down to `forest_level`** - the coarsest
//   level whose cells already satisfy `h_max`. Below it every child is kept even where it sticks
//   out. The restriction is not an optimization, it is what keeps the lattice conforming: the drop
//   set has to be a subtree cut, or S5's `f(F)` breaks. A face whose centre is a lattice corner
//   takes case Q, which emits the face's four edge midpoints - and those are lattice corners only
//   because the four finer neighbours across the face exist (SPEC §3.1 L1). Drop one of them and the
//   coarse cell emits a node the neighbouring cell never sees, which is a hanging node. Cutting whole
//   subtrees instead means a hull face has *no* finer neighbours at all, so its centre is not a
//   corner, so case Q never arises there. Measured: on a 1 x 1 x 0.35 domain the unrestricted drop
//   gave 216 hanging nodes and 8 non-manifold edges; this gives zero.
pub fn build_sizing_field(lookup: &SizingLookup, options: &SizingOptions) -> SizingField {
    let extent = options.domain_max.sub(options.domain_min);
    let root_size = extent.x.max(extent.y).max(extent.z).max(f64::MIN_POSITIVE);
    let mut max_level = 0u32;
    while max_level < options.max_level
        && root_size / (1u64 << max_level) as f64 > options.h_min
    {
        max_level += 1;
    }
    // The coarsest level whose cells already satisfy `h_max`; below it the octree
    // is never trimmed against the domain.
    let mut forest_level = 0u32;
    while forest_level < max_level && root_size / (1u64 << forest_level) as f64 > options.h_max {
        forest_level += 1;
    }
    let mut leaves: Vec<SizingLeaf> = Vec::new();
    let mut current: Vec<[u32; 3]> = vec![[0, 0, 0]];
    let mut level = 0u32;
    let mut unresolved = 0usize;
    let mut level_capped = false;
    let mut leaf_capped = false;

    loop {
        let size = root_size / (1u64 << level) as f64;
        let decisions: Vec<Option<(bool, f64)>> = current
            .par_iter()
            .map(|coord| {
                let min = Vec3::new(
                    options.domain_min.x + coord[0] as f64 * size,
                    options.domain_min.y + coord[1] as f64 * size,
                    options.domain_min.z + coord[2] as f64 * size,
                );
                let max = min.add(Vec3::new(size, size, size));
                // A cell entirely outside the domain box carries no mesh - but only
                // whole subtrees may be cut (see the note above).
                if level <= forest_level
                    && (max.x <= options.domain_min.x
                    || max.y <= options.domain_min.y
                    || max.z <= options.domain_min.z
                    || min.x >= options.domain_max.x
                    || min.y >= options.domain_max.y
                    || min.z >= options.domain_max.z)
                {
                    return None;
                }
                let smallest = lookup.eval_box(min, max);
                Some((size > smallest, smallest))
            })
            .collect();

        let splits = decisions
            .iter()
            .filter(|decision| matches!(decision, Some((true, _))))
            .count();
        let can_descend = level < max_level;
        let fits = leaves.len() + (current.len() - splits) + splits * 8 <= options.max_leaves;
        if !can_descend {
            level_capped |= splits > 0;
        }
        if can_descend && !fits {
            leaf_capped = true;
        }
        let descend = can_descend && fits && splits > 0;

        let mut next: Vec<[u32; 3]> = Vec::with_capacity(splits * 8);
        for (coord, decision) in current.iter().zip(decisions.iter()) {
            let Some((split, h)) = decision else {
                continue;
            };
            if *split && descend {
                for dz in 0..2u32 {
                    for dy in 0..2u32 {
                        for dx in 0..2u32 {
                            next.push([
                                coord[0] * 2 + dx,
                                coord[1] * 2 + dy,
                                coord[2] * 2 + dz,
                            ]);
                        }
                    }
                }
            } else {
                if *split {
                    unresolved += 1;
                }
                leaves.push(SizingLeaf {
                    coord: *coord,
                    level,
                    h: h.clamp(options.h_min, options.h_max),
                });
            }
        }
        if next.is_empty() {
            break;
        }
        current = next;
        level += 1;
    }

    leaves.sort_unstable_by_key(|leaf| (leaf.level, leaf.coord));
    let mut stats = SizingStats {
        n_sources: lookup.len(),
        n_leaves: leaves.len(),
        h_smallest: f64::INFINITY,
        h_largest: 0.0,
        max_level_used: 0,
        n_unresolved: unresolved,
        level_capped,
        leaf_capped,
    };
    for leaf in &leaves {
        stats.h_smallest = stats.h_smallest.min(leaf.h);
        stats.h_largest = stats.h_largest.max(leaf.h);
        stats.max_level_used = stats.max_level_used.max(leaf.level);
    }
    if leaves.is_empty() {
        stats.h_smallest = 0.0;
    }
    SizingField {
        origin: options.domain_min,
        root_size,
        max_level,
        leaves,
        stats,
    }
}

// ---------------------------------------------------------------------------
// The constraint C(R) the coupling driver calls
// ---------------------------------------------------------------------------

// AI-FUNC-SUMMARY:
// Purpose: The sizing constraint of the frozen update rule, assembled from the geometry field and
//   the thin regions.
// Notes: `geometry` is the minimum of the *regime-independent* field over each region's samples;
//   `separation` is the region's frozen `t_r` (Rule S3-M). `evaluate` is what the driver's closure
//   calls: a region that stays volumetric asks for `t_r / gap_cells`, a region that converts asks
//   for nothing, and either way the geometry term applies. With no thin regions the constraint is
//   `h_max` - there is nothing for the coupling to decide, and the spatial field still carries the
//   geometry limits.
//
//   **A region S3 declined carries `separation = INFINITY` here.** `C(R)` is a single scalar for
//   the whole model, so whatever sets it sets the regime thresholds everywhere; letting a region
//   that S3 judged unreliable - a six-sample speck, a low-confidence group, a failed mid-surface -
//   set that scalar means one bad measurement declines every legitimate conversion in the model,
//   which is what a 0.75-unit speck did to all eight band regions of `TestCaseIntersect1`. Nothing
//   is under-resolved by the exclusion: those samples still emit LFS sources through `gap_sources`,
//   so the *field* refines around them exactly as measured. Only the global thresholds stop being
//   hostage to a measurement S3 itself declined to act on.
#[derive(Debug, Clone)]
pub struct SizingConstraint {
    pub geometry: Vec<f64>,
    pub separation: Vec<f64>,
    pub h_max: f64,
    pub gap_cells: f64,
    pub lfs_floor: f64,
}

impl SizingConstraint {
    // AI-FUNC-SUMMARY:
    // Purpose: Build the per-region constraint terms from the geometry field and the gap field.
    // Inputs: the geometry-only lookup, the gap field, the options.
    // Returns: SizingConstraint indexed by region id.
    // Side effects: None.
    pub fn new(
        geometry_field: &SizingLookup,
        field: &GapField,
        options: &SizingOptions,
    ) -> SizingConstraint {
        let geometry: Vec<f64> = field
            .regions
            .par_iter()
            .map(|region| {
                let mut best = options.h_max;
                for sample in &region.samples {
                    if let Some(sample) = field.samples.get(*sample) {
                        best = best.min(geometry_field.eval(sample.point));
                    }
                }
                best
            })
            .collect();
        SizingConstraint {
            geometry,
            // A region S3 declined is not a thin region as far as the coupling is
            // concerned - see the struct note.
            separation: field
                .regions
                .iter()
                .map(|region| {
                    if region.skip.is_some() {
                        f64::INFINITY
                    } else {
                        region.t_r
                    }
                })
                .collect(),
            h_max: options.h_max,
            gap_cells: options.gap_cells,
            lfs_floor: options.lfs_floor(),
        }
    }

    // AI-FUNC-SUMMARY:
    // Purpose: `C(R)` - the largest size every thin region can live with under the given regimes.
    // Inputs: the current regime per region.
    // Returns: the constraint value; `h_max` when there are no regions.
    // Side effects: None.
    // Notes: A serial fold in region-id order over exact `min`s: the value cannot depend on how the
    //   work was scheduled. The driver applies `min(h, .)` and the `h_min` floor itself, so a
    //   constraint that rose could still never break monotonicity.
    pub fn evaluate(&self, regimes: &[Regime]) -> f64 {
        let mut value = self.h_max;
        for (index, regime) in regimes.iter().enumerate() {
            if let Some(geometry) = self.geometry.get(index) {
                value = value.min(*geometry);
            }
            if *regime != Regime::Normal {
                continue;
            }
            let Some(separation) = self.separation.get(index) else {
                continue;
            };
            // Same floor as `gap_sources`: a contact, or a gap below `gap_cells * h_min`, is
            // the thin machinery's problem and must not reach the thresholds.
            if separation.is_finite() && *separation > self.lfs_floor {
                value = value.min(separation / self.gap_cells);
            }
        }
        value
    }

    // AI-FUNC-SUMMARY:
    // Purpose: Which region and which term the converged constraint came from.
    // Inputs: the converged regimes.
    // Returns: `(region, value, from_gap)` for the binding term, or None when nothing binds below
    //   `h_max`; ties go to the lowest region id.
    // Side effects: None.
    // Notes: `C(R)` is one scalar over the whole model, so knowing *which* region set it is the
    //   difference between "the mesh is fine because of that 0.7-wide gap" and an unexplained
    //   global refinement. The pipeline prints it.
    pub fn binding_region(&self, regimes: &[Regime]) -> Option<(usize, f64, bool)> {
        let mut best: Option<(usize, f64, bool)> = None;
        let mut consider = |index: usize, value: f64, from_gap: bool| {
            if value >= self.h_max {
                return;
            }
            match best {
                Some((_, current, _)) if current <= value => {}
                _ => best = Some((index, value, from_gap)),
            }
        };
        for (index, regime) in regimes.iter().enumerate() {
            if let Some(geometry) = self.geometry.get(index) {
                consider(index, *geometry, false);
            }
            if *regime != Regime::Normal {
                continue;
            }
            if let Some(separation) = self.separation.get(index) {
                if separation.is_finite() && *separation > self.lfs_floor {
                    consider(index, separation / self.gap_cells, true);
                }
            }
        }
        best
    }
}

// ---------------------------------------------------------------------------
// s04_sizing
// ---------------------------------------------------------------------------

// AI-FUNC-SUMMARY:
// Purpose: Encode a sizing field as the `s04_sizing` preview VTU (SPEC_meshgen_contracts §3).
// Inputs: the built field and the run's components (for the component tables).
// Returns: VtuDoc of `cell_kind = 3` voxel cells carrying the `sizing_h` point array.
// Side effects: None.
// Notes: Corners are deduplicated on the octree's own integer lattice at `max_level`, so no two
//   emitted points can be coincident ([V2]) and a level jump shares exact coordinates rather than
//   nearly-equal floats. A point's `sizing_h` is the smallest value among the leaves touching it,
//   which is what makes the level jumps legible in a heatmap. There are no tets, so [V1]/[V3]/[V4]
//   have nothing to check and the always-present tet arrays carry their sentinels.
pub fn sizing_to_doc(field: &SizingField, components: &[ArrangeComponent]) -> VtuDoc {
    let finest = field.level_size(field.max_level);
    let mut point_ids: BTreeMap<[u32; 3], usize> = BTreeMap::new();
    let mut corners: Vec<[[u32; 3]; 8]> = Vec::with_capacity(field.leaves.len());
    for leaf in &field.leaves {
        let shift = field.max_level - leaf.level;
        let base = [
            leaf.coord[0] << shift,
            leaf.coord[1] << shift,
            leaf.coord[2] << shift,
        ];
        let step = 1u32 << shift;
        let mut cell = [[0u32; 3]; 8];
        // VTK_VOXEL corner order: x fastest, then y, then z.
        for (slot, corner) in cell.iter_mut().enumerate() {
            let dx = (slot & 1) as u32;
            let dy = ((slot >> 1) & 1) as u32;
            let dz = ((slot >> 2) & 1) as u32;
            *corner = [
                base[0] + dx * step,
                base[1] + dy * step,
                base[2] + dz * step,
            ];
            point_ids.insert(*corner, 0);
        }
        corners.push(cell);
    }
    for (slot, (_, id)) in point_ids.iter_mut().enumerate() {
        *id = slot;
    }

    let mut doc = VtuDoc {
        points: Vec::with_capacity(point_ids.len()),
        ..Default::default()
    };
    for key in point_ids.keys() {
        doc.points.push(Vec3::new(
            field.origin.x + key[0] as f64 * finest,
            field.origin.y + key[1] as f64 * finest,
            field.origin.z + key[2] as f64 * finest,
        ));
    }
    let mut sizing_h = vec![f64::INFINITY; doc.points.len()];
    for (leaf, cell) in field.leaves.iter().zip(corners.iter()) {
        for corner in cell {
            let id = point_ids[corner];
            doc.connectivity.push(id as i64);
            if leaf.h < sizing_h[id] {
                sizing_h[id] = leaf.h;
            }
        }
        doc.offsets.push(doc.connectivity.len() as i64);
        doc.types.push(VTK_VOXEL);
    }

    let cells = field.leaves.len();
    doc.cell_data
        .push(DataArray::scalar("cell_kind", ArrayData::U8(vec![3; cells])));
    doc.cell_data.push(DataArray::scalar(
        "region_key",
        ArrayData::I32(vec![-1; cells]),
    ));
    doc.cell_data.push(DataArray::scalar(
        "partition_id",
        ArrayData::I32(vec![-1; cells]),
    ));
    doc.cell_data
        .push(DataArray::scalar("regime", ArrayData::U8(vec![255; cells])));
    doc.cell_data.push(DataArray::scalar(
        "face_tag_key",
        ArrayData::I32(vec![-1; cells]),
    ));
    doc.cell_data.push(DataArray::scalar(
        "curve_id",
        ArrayData::I32(vec![-1; cells]),
    ));

    let points = doc.points.len();
    doc.point_data.push(DataArray::scalar(
        "n_id_key",
        ArrayData::I32(vec![0; points]),
    ));
    doc.point_data.push(DataArray::scalar(
        "constraint_kind",
        ArrayData::U8(vec![0; points]),
    ));
    doc.point_data.push(DataArray::scalar(
        "constraint_ref",
        ArrayData::I32(vec![-1; points]),
    ));
    doc.point_data.push(DataArray::scalar(
        "sizing_h",
        ArrayData::F32(
            sizing_h
                .iter()
                .map(|value| if value.is_finite() { *value as f32 } else { -1.0 })
                .collect(),
        ),
    ));

    push_field(&mut doc, "RegionSetOffsets", 1, ArrayData::I64(vec![1]));
    push_field(&mut doc, "RegionSetComponents", 1, ArrayData::I32(vec![0]));
    push_field(
        &mut doc,
        "RegionSetPriority",
        1,
        ArrayData::U32(vec![u32::MAX]),
    );
    push_field(&mut doc, "NIdSetOffsets", 1, ArrayData::I64(vec![1]));
    push_field(&mut doc, "NIdSetComponents", 1, ArrayData::I32(vec![0]));
    push_field(&mut doc, "FaceTagOffsets", 1, ArrayData::I64(Vec::new()));
    push_field(&mut doc, "FaceTagComponents", 1, ArrayData::I32(Vec::new()));
    push_field(&mut doc, "FaceTagOrientation", 1, ArrayData::I32(Vec::new()));
    push_field(&mut doc, "FaceTagKind", 1, ArrayData::U8(Vec::new()));
    push_field(&mut doc, "FaceTagSideElems", 2, ArrayData::I32(Vec::new()));
    push_field(
        &mut doc,
        "ComponentX",
        1,
        ArrayData::I32(components.iter().map(|c| c.x).collect()),
    );
    push_field(
        &mut doc,
        "ComponentY",
        1,
        ArrayData::U32(components.iter().map(|c| c.priority).collect()),
    );
    push_field(
        &mut doc,
        "ComponentKind",
        1,
        ArrayData::U8(components.iter().map(|c| c.kind).collect()),
    );
    push_field(
        &mut doc,
        "ComponentClosed",
        1,
        ArrayData::U8(components.iter().map(|c| u8::from(c.closed)).collect()),
    );
    push_field(&mut doc, "CurveKind", 1, ArrayData::U8(Vec::new()));
    push_field(&mut doc, "CurveCompOffsets", 1, ArrayData::I64(Vec::new()));
    push_field(
        &mut doc,
        "CurveCompComponents",
        1,
        ArrayData::I32(Vec::new()),
    );
    doc
}

// AI-FUNC-SUMMARY: Append one field-data array with an explicit component count; side effects: mutates VtuDoc field_data.
fn push_field(doc: &mut VtuDoc, name: &str, components: usize, data: ArrayData) {
    doc.field_data.push(DataArray {
        name: name.to_string(),
        components,
        data,
    });
}
