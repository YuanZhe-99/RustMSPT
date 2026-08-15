//! S3 - Separation (gap) field: rays, closest-pair sweep, pairing battery, confidence
//! (PLAN §10.5, SPEC_meshgen_geometry §11, SPEC_meshgen_numerics §2 class A).
//!
//! Samples sit on the arranged surface (vertices + face centroids, both sides of
//! every face) and on the closest points of every triangle pair within `t_layer`.
//! Each sample shoots one ray along its side direction and records the first
//! opposing hit; the closest-pair sweep supplies the geometric minima the rays
//! provably miss (oblique and edge-edge approaches). The five-check pairing
//! battery validates the correspondence map before any region may be converted,
//! and per-group confidence is the fraction of samples passing every applicable
//! check.
//!
//! Rule S3-M (frozen): the quantity used for a later regime decision is the exact
//! closest-pair distance `t_exact`, measured once here and never re-measured
//! across the S3<->S4 coupling iterations. The ray field (`t`, smoothed) supplies
//! the pairing map, the segmentation input, and the snapshot visualisation only.

use crate::io::vtu::{ArrayData, DataArray, VtuDoc, VTK_TRIANGLE};
use crate::meshgen::arrange::{arranged_surface_to_doc, ArrangedCurveKind, ArrangedSurface};
use crate::meshgen::predicates::orient3d;
use crate::meshgen::thin::{ThinContext, ThinRegime};
use crate::meshgen::topo::{ComponentClassification, RebuiltTopology};
use crate::types::Vec3;
use rayon::prelude::*;
use std::cmp::Reverse;
use std::collections::{BTreeMap, BTreeSet, BinaryHeap};

/// Cluster id per `(component, node, triangle)` - which smooth patch a face joins at a vertex.
type VertexClusterMap = BTreeMap<(i32, usize, usize), usize>;
/// Area-weighted unit normal per `(component, node, cluster)`.
type VertexClusterNormals = BTreeMap<(i32, usize, usize), Vec3>;

/// Background/domain-wall pseudo component (`BG_ID`, PLAN §5.1 R-A1).
pub const BOX_COMPONENT: i32 = 0;

/// Battery check 1 - mutual-ray consistency.
pub const FLAG_MUTUAL: u8 = 1 << 0;
/// Battery check 2 - the pairing target belongs to a resolved opposite patch.
pub const FLAG_OPPOSITE_PATCH: u8 = 1 << 1;
/// Battery check 3 - pairing continuity across adjacent samples.
pub const FLAG_CONTINUITY: u8 = 1 << 2;
/// Battery check 4 - no local crossing of adjacent correspondence segments.
pub const FLAG_NO_CROSSING: u8 = 1 << 3;
/// Battery check 5 - the correspondence preserves orientation.
pub const FLAG_ORIENTATION: u8 = 1 << 4;
/// All five battery bits.
pub const FLAGS_ALL: u8 = 0b0001_1111;

// AI-FUNC-SUMMARY: Report one S3 stage's wall time when `RUSTMSPT_TIME_STAGES` is set; side effects: writes to stderr.
fn time_stage(name: &str, started: std::time::Instant) {
    if std::env::var_os("RUSTMSPT_TIME_STAGES").is_some() {
        eprintln!("[S3-TIME] {name} {:?}", started.elapsed());
    }
}

const COS_OPPOSING: f64 = 0.5;
/// Same-component pairs need a much tighter facing cone than the 60-degree ray
/// condition: two facets meeting at a convex edge sit right inside the 45-60 degree
/// band, while the two faces of a genuine thin wall are nearly antiparallel.
const COS_OPPOSING_INTRA: f64 = 0.9;
const MUTUAL_RATIO: f64 = 0.3;
const MUTUAL_RADIUS_FACTOR: f64 = 0.5;
const GRADIENT_LIMIT: f64 = 0.5;
const CROSSING_SHARE: f64 = 0.05;
/// Work cap for one bounded-geodesic search. A shortcut path is a handful of faces by
/// definition, so a search that has expanded this many faces without reaching its
/// target has already answered "far along the surface". Capping keeps the pass linear
/// on inputs where two large surfaces meet.
const GEODESIC_MAX_VISITS: usize = 512;

// AI-FUNC-SUMMARY: Where a sample came from; returns nothing; side effects: none.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum SampleKind {
    Vertex,
    Centroid,
    ClosestPair,
    Densified,
}

// AI-FUNC-SUMMARY: Pair class of a validated correspondence (PLAN §10.5); returns nothing; side effects: none.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum PairClass {
    Unpaired,
    Intra(i32),
    Inter(i32, i32),
    SolidSheet(i32, i32),
    SheetSheet(i32, i32),
    SurfaceBox(i32),
}

// AI-FUNC-SUMMARY: The opposite end of one sample's correspondence; returns nothing; side effects: none.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct GapPairing {
    pub point: Vec3,
    pub tri: usize,
    pub component: i32,
    pub patch: i32,
}

// AI-FUNC-SUMMARY:
// Purpose: One separation-field sample with its ray correspondence, battery flags, and pair class.
// Notes: `side` is +1/-1 relative to the owning triangle's emitted normal; `t_raw` is the ray
//   separation, `t` the smoothed field, and `t_exact` the closest-pair distance when this sample
//   was produced or refined by the sweep (Rule S3-M's quantity). Unpaired samples carry INFINITY.
//   `vertex_cluster` indexes the smooth patch a vertex sample belongs to (0 for every other kind).
#[derive(Debug, Clone)]
pub struct GapSample {
    pub point: Vec3,
    pub tri: usize,
    pub component: i32,
    pub kind: SampleKind,
    pub side: i8,
    pub direction: Vec3,
    pub t_raw: f64,
    pub t: f64,
    pub t_exact: f64,
    pub pairing: Option<GapPairing>,
    pub pair_class: PairClass,
    pub flags: u8,
    pub applicable: u8,
    pub vertex: Option<usize>,
    pub vertex_cluster: usize,
}

impl GapSample {
    // AI-FUNC-SUMMARY: Whether every applicable battery check passed; returns bool; side effects: none.
    pub fn passes_battery(&self) -> bool {
        self.pairing.is_some() && (self.flags & self.applicable) == self.applicable
    }
}

// AI-FUNC-SUMMARY:
// Purpose: A provisional pre-region group: one wall side facing one opposite patch.
// Notes: G3-2 owns regime segmentation; this grouping is what makes a branching throat two
//   candidate regions instead of one chimeric pairing (battery check 2).
#[derive(Debug, Clone)]
pub struct GapGroup {
    pub component: i32,
    pub side: i8,
    pub opposite_patch: i32,
    pub pair_class: PairClass,
    pub samples: Vec<usize>,
    pub confidence: f64,
    pub t_r: f64,
}

// AI-FUNC-SUMMARY: Counters reported by the S3 driver; returns nothing; side effects: none.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct GapFieldStats {
    pub n_samples: usize,
    pub n_ray_paired: usize,
    pub n_closest_pairs: usize,
    pub n_densified: usize,
    pub n_battery_failures: [usize; 5],
    pub n_groups: usize,
    pub n_low_confidence_groups: usize,
    pub n_geodesic_shortcuts: usize,
    pub n_regions: usize,
    pub n_sheet_regions: usize,
    pub n_band_regions: usize,
    pub n_skipped_regions: usize,
}

// AI-FUNC-SUMMARY:
// Purpose: The S3 separation field: samples, opposite patches, provisional groups, thresholds, stats.
// Notes: `t_layer`/`t_sheet` are the bootstrap thresholds from `h_bootstrap`; the S3<->S4 loop
//   (G3-2/G4-1) only ever lowers them, and Rule S3-M keeps `t_exact` fixed while it does.
#[derive(Debug, Clone, Default)]
pub struct GapField {
    pub samples: Vec<GapSample>,
    pub groups: Vec<GapGroup>,
    pub regions: Vec<ThinRegion>,
    pub patch_of_tri: Vec<i32>,
    pub face_of_tri: Vec<i64>,
    pub t_layer: f64,
    pub t_sheet: f64,
    pub stats: GapFieldStats,
}

// AI-FUNC-SUMMARY: Thin-feature regime of one segmented region (SPEC_meshgen_geometry §11.3); side effects: none.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Regime {
    /// Ordinary volumetric meshing; the gap is resolvable by normal elements.
    Normal,
    /// One layer of thin elements across the gap.
    Band,
    /// Collapse to a single embedded sheet.
    Sheet,
}

// AI-FUNC-SUMMARY: Why a region that qualified geometrically is not converted (`[THIN-SKIP]` taxonomy); side effects: none.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum SkipReason {
    /// Battery confidence below `confidence_min` (the reference thin-feature design §3.3).
    LowConfidence,
    /// Less area than the speck floor: the region is smaller than one bootstrap element
    /// and its `t` is small because it is degenerate, not because a real gap sits there.
    Speck,
    /// Real area, but fewer samples than the floor asks for: S3 will not convert on a
    /// measurement this sparse, and the region is still a gap the field has to resolve.
    Undersampled,
    /// The mid-surface candidate failed validation and local repair is deferred.
    MidSurfaceInvalid,
    /// Too few paired vertex samples to build a mid-surface at all.
    MidSurfaceUnbuildable,
    /// The region reaches an intersection curve between its own two walls: the gap
    /// closes onto the curve, so this is the curve's neighbourhood (S8's junction
    /// machinery owns it), not a thin feature.
    IntersectionWedge,
}

// AI-FUNC-SUMMARY: Why a mid-surface candidate was rejected (the reference thin-feature design §3.4 validation); side effects: none.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum MidSurfaceDefect {
    DegenerateTriangle,
    OrientationReversed,
    NormalDeviation,
    SelfIntersection,
    BoundaryMismatch,
    EulerMismatch,
}

// AI-FUNC-SUMMARY:
// Purpose: The candidate mid-surface of a Sheet region: midpoints of the validated correspondence.
// Notes: `source_nodes[i]` is the wall-A arranged node that produced `points[i]`; triangles are
//   wall A's own restricted triangulation re-indexed onto the midpoints (the reference thin-feature design §3.4).
#[derive(Debug, Clone, Default)]
pub struct MidSurface {
    pub points: Vec<Vec3>,
    pub source_nodes: Vec<usize>,
    pub triangles: Vec<[usize; 3]>,
    pub defects: Vec<MidSurfaceDefect>,
}

impl MidSurface {
    // AI-FUNC-SUMMARY: Whether the candidate passed every §3.4 check; returns bool; side effects: none.
    pub fn is_valid(&self) -> bool {
        self.defects.is_empty() && !self.triangles.is_empty()
    }
}

// AI-FUNC-SUMMARY:
// Purpose: One segmented thin region: a connected run of one wall side facing one opposite patch.
// Notes: `declared_regime` is what the thresholds say; `regime` is what survives the confidence gate
//   and mid-surface validation - a skipped region always falls back to `Normal` (volumetric), never
//   to a sheet. `t_r` is the frozen Rule S3-M measurement. A region owns **both** walls of its gap
//   (the reference thin-feature design §3.3 pairing closure): `samples` spans both, `faces` is wall A - the wall the
//   region grew from, and the one its rims and mid-surface are built on - and `opposite_faces` is
//   the wall that joined through the closure.
#[derive(Debug, Clone)]
pub struct ThinRegion {
    pub id: usize,
    pub component: i32,
    pub side: i8,
    pub opposite_patch: i32,
    pub pair_class: PairClass,
    pub declared_regime: Regime,
    pub regime: Regime,
    pub samples: Vec<usize>,
    pub faces: Vec<usize>,
    pub opposite_faces: Vec<usize>,
    pub area: f64,
    pub t_r: f64,
    /// The widest separation any member sample reports, against `t_r`'s narrowest.
    /// A thin feature holds these together; a wedge is exactly where they part.
    pub t_max: f64,
    pub confidence: f64,
    pub failed_checks: [usize; 5],
    pub skip: Option<SkipReason>,
    pub rims: Vec<Vec<usize>>,
    pub mid_surface: Option<MidSurface>,
}

// AI-FUNC-SUMMARY:
// Purpose: Inputs to `compute_gap_field` (domain, tolerance, bootstrap size, gap factors, sampling controls).
// Notes: `h_bootstrap` is h^(0) from SPEC_meshgen_geometry §11.1 - the coarsest sizing the loop will
//   ever use, so the search radius `2*t_layer` is the widest one and the measurement stays valid as
//   thresholds shrink.
#[derive(Debug, Clone)]
pub struct GapFieldOptions {
    pub domain_min: Vec3,
    pub domain_max: Vec3,
    pub eps: f64,
    pub h_bootstrap: f64,
    pub t_layer_factor: f64,
    pub t_sheet_factor: f64,
    pub confidence_min: f64,
    pub geodesic_min_ratio: f64,
    pub speck_min_samples: usize,
    pub speck_min_area_factor: f64,
    pub hysteresis_enter: f64,
    pub hysteresis_leave: f64,
    pub feature_angle_deg: f64,
    pub include_box_walls: bool,
    pub densify_rounds: usize,
    pub smoothing_sweeps: usize,
}

impl Default for GapFieldOptions {
    // AI-FUNC-SUMMARY: Plan §6.3 defaults for the gap field; returns GapFieldOptions; side effects: none.
    fn default() -> Self {
        GapFieldOptions {
            domain_min: Vec3::new(0.0, 0.0, 0.0),
            domain_max: Vec3::new(1.0, 1.0, 1.0),
            eps: 1.0e-4,
            h_bootstrap: 0.05,
            t_layer_factor: 1.0,
            t_sheet_factor: 0.2,
            confidence_min: 0.9,
            geodesic_min_ratio: 3.0,
            speck_min_samples: 12,
            speck_min_area_factor: 4.0,
            hysteresis_enter: 0.9,
            hysteresis_leave: 1.1,
            feature_angle_deg: 45.0,
            include_box_walls: true,
            densify_rounds: 3,
            smoothing_sweeps: 3,
        }
    }
}

// AI-FUNC-SUMMARY: One triangle of the S3 query set: an arranged face or a virtual domain wall; side effects: none.
#[derive(Debug, Clone)]
struct QueryTri {
    a: Vec3,
    b: Vec3,
    c: Vec3,
    normal: Vec3,
    outward: Option<Vec3>,
    nodes: [usize; 3],
    face: Option<usize>,
    component: i32,
}

impl QueryTri {
    // AI-FUNC-SUMMARY: Centroid of the triangle; returns Vec3; side effects: none.
    fn centroid(&self) -> Vec3 {
        self.a.add(self.b).add(self.c).scale(1.0 / 3.0)
    }

    // AI-FUNC-SUMMARY: Whether this triangle uses the given arranged-surface node; returns bool; side effects: none.
    fn uses_node(&self, node: usize) -> bool {
        self.face.is_some() && self.nodes.contains(&node)
    }
}

// AI-FUNC-SUMMARY: Uniform bucket grid over triangle AABBs for deterministic ray and proximity queries; side effects: none.
struct TriGrid {
    origin: Vec3,
    cell: f64,
    dims: [i64; 3],
    buckets: Vec<Vec<u32>>,
}

impl TriGrid {
    // AI-FUNC-SUMMARY:
    // Purpose: Build a uniform grid whose cell is at least the query radius, capped at 128^3 buckets.
    // Inputs: triangles, query radius (t_layer).
    // Returns: TriGrid with every triangle registered in each bucket its AABB touches.
    // Side effects: None.
    fn build(tris: &[QueryTri], radius: f64) -> TriGrid {
        let mut min = Vec3::new(f64::INFINITY, f64::INFINITY, f64::INFINITY);
        let mut max = Vec3::new(f64::NEG_INFINITY, f64::NEG_INFINITY, f64::NEG_INFINITY);
        let mut extent_sum = 0.0;
        for tri in tris {
            for p in [tri.a, tri.b, tri.c] {
                min = Vec3::new(min.x.min(p.x), min.y.min(p.y), min.z.min(p.z));
                max = Vec3::new(max.x.max(p.x), max.y.max(p.y), max.z.max(p.z));
            }
            let (lo, hi) = tri_aabb(tri);
            let d = hi.sub(lo);
            extent_sum += d.x.max(d.y).max(d.z);
        }
        if tris.is_empty() {
            min = Vec3::new(0.0, 0.0, 0.0);
            max = Vec3::new(1.0, 1.0, 1.0);
        }
        let span = max.sub(min);
        let longest = span.x.max(span.y).max(span.z).max(f64::MIN_POSITIVE);
        let mean_extent = if tris.is_empty() {
            longest
        } else {
            extent_sum / tris.len() as f64
        };
        let mut cell = mean_extent.max(radius).max(longest / 128.0);
        if cell.is_nan() || cell <= 0.0 {
            cell = longest;
        }
        let dim = |s: f64| -> i64 { ((s / cell).ceil() as i64 + 1).clamp(1, 128) };
        let dims = [dim(span.x), dim(span.y), dim(span.z)];
        let mut grid = TriGrid {
            origin: min,
            cell,
            dims,
            buckets: vec![Vec::new(); (dims[0] * dims[1] * dims[2]) as usize],
        };
        for (index, tri) in tris.iter().enumerate() {
            let (lo, hi) = tri_aabb(tri);
            for key in grid.cell_range(lo, hi) {
                grid.buckets[key].push(index as u32);
            }
        }
        grid
    }

    // AI-FUNC-SUMMARY: Bucket indices whose cells overlap the given AABB; returns Vec<usize>; side effects: none.
    fn cell_range(&self, lo: Vec3, hi: Vec3) -> Vec<usize> {
        let axis = |value: f64, index: usize| -> i64 {
            (((value - component(self.origin, index)) / self.cell).floor() as i64)
                .clamp(0, self.dims[index] - 1)
        };
        let lo_i = [axis(lo.x, 0), axis(lo.y, 1), axis(lo.z, 2)];
        let hi_i = [axis(hi.x, 0), axis(hi.y, 1), axis(hi.z, 2)];
        let mut out = Vec::new();
        for z in lo_i[2]..=hi_i[2] {
            for y in lo_i[1]..=hi_i[1] {
                for x in lo_i[0]..=hi_i[0] {
                    out.push((z * self.dims[1] * self.dims[0] + y * self.dims[0] + x) as usize);
                }
            }
        }
        out
    }

    // AI-FUNC-SUMMARY: Sorted, deduplicated triangle indices whose bucket overlaps the AABB; returns Vec<u32>; side effects: none.
    fn query(&self, lo: Vec3, hi: Vec3) -> Vec<u32> {
        let mut out = Vec::new();
        self.query_into(lo, hi, &mut out);
        out
    }

    // AI-FUNC-SUMMARY:
    // Purpose: `query` into a caller-owned buffer, so a hot parallel loop allocates once per thread.
    // Side effects: Clears and fills `out`.
    // Notes: The sweep runs this per triangle; allocating a fresh Vec each time turns the global
    //   allocator into the bottleneck and the parallel loop gains nothing.
    fn query_into(&self, lo: Vec3, hi: Vec3, out: &mut Vec<u32>) {
        out.clear();
        for key in self.cell_range(lo, hi) {
            out.extend_from_slice(&self.buckets[key]);
        }
        out.sort_unstable();
        out.dedup();
    }
}

// AI-FUNC-SUMMARY: Read one component of a Vec3 by axis index; returns f64; side effects: none.
fn component(v: Vec3, index: usize) -> f64 {
    match index {
        0 => v.x,
        1 => v.y,
        _ => v.z,
    }
}

// AI-FUNC-SUMMARY: Axis-aligned bounds of one triangle; returns (min, max); side effects: none.
fn tri_aabb(tri: &QueryTri) -> (Vec3, Vec3) {
    let lo = Vec3::new(
        tri.a.x.min(tri.b.x).min(tri.c.x),
        tri.a.y.min(tri.b.y).min(tri.c.y),
        tri.a.z.min(tri.b.z).min(tri.c.z),
    );
    let hi = Vec3::new(
        tri.a.x.max(tri.b.x).max(tri.c.x),
        tri.a.y.max(tri.b.y).max(tri.c.y),
        tri.a.z.max(tri.b.z).max(tri.c.z),
    );
    (lo, hi)
}

// AI-FUNC-SUMMARY: Euclidean length of a vector; returns f64; side effects: none.
fn norm(v: Vec3) -> f64 {
    v.dot(v).sqrt()
}

// AI-FUNC-SUMMARY: Unit vector, or None when the input is degenerate; returns Option<Vec3>; side effects: none.
fn unit(v: Vec3) -> Option<Vec3> {
    let n = norm(v);
    if n > 0.0 && n.is_finite() {
        Some(v.scale(1.0 / n))
    } else {
        None
    }
}

// AI-FUNC-SUMMARY:
// Purpose: Compute the S3 separation field over a clipped, topology-rebuilt arranged surface.
// Inputs: arranged surface, rebuilt topology (component classification), gap-field options.
// Returns: GapField with samples, opposite patches, provisional groups, thresholds, and stats.
// Side effects: None (pure computation).
// Notes: Deterministic end to end - triangles follow arranged-face order, every candidate list is
//   sorted before use, and no float reduction depends on iteration order. Rays supply the pairing
//   map; the closest-pair sweep supplies the frozen `t_exact` (Rule S3-M).
pub fn compute_gap_field(
    surface: &ArrangedSurface,
    topo: &RebuiltTopology,
    options: &GapFieldOptions,
) -> GapField {
    let t_layer = options.t_layer_factor * options.h_bootstrap;
    let t_sheet = options.t_sheet_factor * options.h_bootstrap;
    let tris = build_query_triangles(surface, topo, options);
    if tris.is_empty() {
        return GapField {
            t_layer,
            t_sheet,
            ..Default::default()
        };
    }
    let grid = TriGrid::build(&tris, t_layer);
    let classification = topo.classifications.clone();

    let cos2_feature = {
        let cos = options.feature_angle_deg.to_radians().cos();
        cos * cos
    };
    let (cluster_of, cluster_normals) = build_vertex_clusters(&tris, cos2_feature);
    let mut samples = seed_surface_samples(surface, &tris, &cluster_of, &cluster_normals);
    let mut stats = GapFieldStats::default();

    let t0 = std::time::Instant::now();
    // R-P1: each sample is independent; `par_iter_mut` writes disjoint slots, so the result
    // is identical to the serial pass regardless of how the work is split.
    samples
        .par_iter_mut()
        .for_each(|sample| cast_sample_ray(sample, &tris, &grid, t_layer, options));
    time_stage("rays", t0);

    let t1 = std::time::Instant::now();
    let closest_pairs = closest_pair_sweep(&tris, &grid, t_layer);
    time_stage("sweep", t1);
    stats.n_closest_pairs = closest_pairs.len();
    // One seed per (triangle, opposite component): the closest approach of that
    // triangle towards that component. Without this a corner triangle seeds one
    // duplicate sample per opposite triangle it can see.
    let mut seeds: BTreeMap<(usize, i32), (f64, Vec3, usize, Vec3)> = BTreeMap::new();
    for pair in &closest_pairs {
        for (tri, point, other_tri, other_point) in [
            (pair.tri_a, pair.point_a, pair.tri_b, pair.point_b),
            (pair.tri_b, pair.point_b, pair.tri_a, pair.point_a),
        ] {
            let key = (tri, tris[other_tri].component);
            let candidate = (pair.distance, point, other_tri, other_point);
            match seeds.get(&key) {
                Some(existing) if existing.0 <= candidate.0 => {}
                _ => {
                    seeds.insert(key, candidate);
                }
            }
        }
    }
    for ((tri, _), (distance, point, other_tri, other_point)) in seeds {
        // A virtual domain wall is never a wall we mesh or convert, so it owns no
        // samples of its own; the pairing is recorded from the real surface's side.
        if tris[tri].face.is_some() {
            let direction = match unit(other_point.sub(point)) {
                Some(d) => d,
                None => continue,
            };
            let side = if direction.dot(tris[tri].normal) >= 0.0 {
                1
            } else {
                -1
            };
            samples.push(GapSample {
                point,
                tri,
                component: tris[tri].component,
                kind: SampleKind::ClosestPair,
                side,
                direction,
                t_raw: distance,
                t: distance,
                t_exact: distance,
                pairing: Some(GapPairing {
                    point: other_point,
                    tri: other_tri,
                    component: tris[other_tri].component,
                    patch: -1,
                }),
                pair_class: PairClass::Unpaired,
                flags: 0,
                applicable: FLAGS_ALL,
                vertex: None,
                vertex_cluster: 0,
            });
        }
    }

    let t2 = std::time::Instant::now();
    let (shortcut_count, shortcut_faces) =
        reject_geodesic_shortcuts(&mut samples, &tris, options.geodesic_min_ratio);
    time_stage("geodesic", t2);
    stats.n_geodesic_shortcuts = shortcut_count;

    let mut adjacency = build_adjacency(&samples, &tris, &cluster_of);
    let t3 = std::time::Instant::now();
    stats.n_densified = densify(&mut samples, &mut adjacency, &tris, &grid, t_layer, options);
    time_stage("densify", t3);

    smooth_field(&mut samples, &adjacency, options.smoothing_sweeps);

    let patch_of_tri = build_opposite_patches(&samples, &tris);
    for sample in samples.iter_mut() {
        if let Some(pairing) = sample.pairing.as_mut() {
            pairing.patch = patch_of_tri[pairing.tri];
        }
    }
    for sample in samples.iter_mut() {
        sample.pair_class = classify_pair(sample, &tris, &classification);
    }

    stats.n_ray_paired = samples
        .iter()
        .filter(|sample| sample.pairing.is_some() && sample.kind != SampleKind::ClosestPair)
        .count();

    let t4 = std::time::Instant::now();
    run_battery(&mut samples, &adjacency, &tris, &cluster_of, options, &mut stats);
    time_stage("battery", t4);

    let groups = build_groups(&samples, options.confidence_min, &mut stats);
    let mut contact_faces = shortcut_faces;
    for ((left, right), nodes) in intersection_curve_nodes(surface) {
        let entry = contact_faces.entry((left, right)).or_default();
        for (index, tri) in tris.iter().enumerate() {
            if tri.face.is_some() && tri.nodes.iter().any(|node| nodes.contains(node)) {
                entry.insert(index);
            }
        }
    }
    // Dilate by one face ring: the geodesic pass clears the pairings *at* a contact,
    // so a wedge region starts one ring out from it and would otherwise not overlap
    // the set at all. One ring is structural ("borders the contact"), not a tuned radius.
    if !contact_faces.is_empty() {
        let face_neighbours = face_adjacency(&tris);
        for faces in contact_faces.values_mut() {
            let mut dilated = faces.clone();
            for face in faces.iter() {
                dilated.extend(face_neighbours[*face].iter().map(|index| *index as usize));
            }
            *faces = dilated;
        }
    }
    let t5 = std::time::Instant::now();
    let regions = segment_regions(
        &samples,
        &groups,
        &tris,
        &adjacency,
        &contact_faces,
        t_sheet,
        t_layer,
        options,
        &mut stats,
    );
    time_stage("segment", t5);
    stats.n_samples = samples.len();
    let face_of_tri: Vec<i64> = tris
        .iter()
        .map(|tri| tri.face.map(|face| face as i64).unwrap_or(-1))
        .collect();

    GapField {
        samples,
        groups,
        regions,
        patch_of_tri,
        face_of_tri,
        t_layer,
        t_sheet,
        stats,
    }
}

// AI-FUNC-SUMMARY:
// Purpose: Build the S3 query triangle table: every arranged face plus (optionally) the 6 domain walls.
// Inputs: arranged surface, rebuilt topology, options.
// Returns: Vec<QueryTri> in arranged-face order, box walls appended last.
// Side effects: None.
// Notes: `outward` is set only for closed solid components, where the sign of the component's own
//   signed volume fixes the material side; S0 guarantees consistent winding but not global sign.
fn build_query_triangles(
    surface: &ArrangedSurface,
    topo: &RebuiltTopology,
    options: &GapFieldOptions,
) -> Vec<QueryTri> {
    let classification = &topo.classifications;
    let mut signed_volume: BTreeMap<i32, f64> = BTreeMap::new();
    for face in &surface.faces {
        let a = surface.vertices[face.nodes[0]];
        let b = surface.vertices[face.nodes[1]];
        let c = surface.vertices[face.nodes[2]];
        let contribution = a.dot(b.cross(c)) / 6.0;
        for (component, orientation) in face
            .components
            .iter()
            .copied()
            .zip(face.tag_orientations.iter().copied())
        {
            *signed_volume.entry(component).or_insert(0.0) += contribution * f64::from(orientation);
        }
    }

    let mut tris: Vec<QueryTri> = Vec::with_capacity(surface.faces.len() + 12);
    for (index, face) in surface.faces.iter().enumerate() {
        let a = surface.vertices[face.nodes[0]];
        let b = surface.vertices[face.nodes[1]];
        let c = surface.vertices[face.nodes[2]];
        // A degenerate face keeps its slot so triangle indices stay aligned with
        // arranged-face indices; the zero normal excludes it from every query.
        let normal = unit(b.sub(a).cross(c.sub(a))).unwrap_or(Vec3::new(0.0, 0.0, 0.0));
        let closed_solid = matches!(
            classification.get(&face.component),
            Some(ComponentClassification::SolidClosed)
        );
        let outward = if closed_solid {
            let volume = signed_volume.get(&face.component).copied().unwrap_or(0.0);
            if volume > 0.0 {
                Some(normal)
            } else if volume < 0.0 {
                Some(normal.scale(-1.0))
            } else {
                None
            }
        } else {
            None
        };
        tris.push(QueryTri {
            a,
            b,
            c,
            normal,
            outward,
            nodes: face.nodes,
            face: Some(index),
            component: face.component,
        });
    }

    if options.include_box_walls {
        let lo = options.domain_min;
        let hi = options.domain_max;
        let corner = |i: usize| -> Vec3 {
            Vec3::new(
                if i & 1 == 0 { lo.x } else { hi.x },
                if i & 2 == 0 { lo.y } else { hi.y },
                if i & 4 == 0 { lo.z } else { hi.z },
            )
        };
        // Each domain face as two triangles, wound so the normal points into the domain.
        const WALLS: [[usize; 4]; 6] = [
            [0, 2, 6, 4], // x = min
            [1, 5, 7, 3], // x = max
            [0, 4, 5, 1], // y = min
            [2, 3, 7, 6], // y = max
            [0, 1, 3, 2], // z = min
            [4, 6, 7, 5], // z = max
        ];
        for wall in WALLS {
            let quad = [
                corner(wall[0]),
                corner(wall[1]),
                corner(wall[2]),
                corner(wall[3]),
            ];
            for triangle in [[0usize, 1, 2], [0, 2, 3]] {
                let a = quad[triangle[0]];
                let b = quad[triangle[1]];
                let c = quad[triangle[2]];
                let Some(normal) = unit(b.sub(a).cross(c.sub(a))) else {
                    continue;
                };
                tris.push(QueryTri {
                    a,
                    b,
                    c,
                    normal,
                    outward: Some(normal.scale(-1.0)),
                    nodes: [usize::MAX; 3],
                    face: None,
                    component: BOX_COMPONENT,
                });
            }
        }
    }
    tris
}

// AI-FUNC-SUMMARY:
// Purpose: Group the faces incident to each (component, vertex) into smooth patches ("clusters").
// Inputs: query triangle table, cos^2 of the feature angle.
// Returns: cluster id per (component, node, triangle) and the area-weighted unit normal per cluster.
// Side effects: None.
// Notes: A vertex on a sharp edge or corner belongs to several walls, and their averaged normal
//   points into none of them - a box corner would shoot its ray diagonally out through a side face.
//   Faces join one cluster only when their normals agree within the feature angle, so every wall
//   meeting at the vertex gets its own sample along its own normal.
fn build_vertex_clusters(
    tris: &[QueryTri],
    cos2_feature: f64,
) -> (VertexClusterMap, VertexClusterNormals) {
    let mut incident: BTreeMap<(i32, usize), Vec<usize>> = BTreeMap::new();
    for (index, tri) in tris.iter().enumerate() {
        if tri.face.is_none() || tri.normal.dot(tri.normal) == 0.0 {
            continue;
        }
        for node in tri.nodes {
            incident.entry((tri.component, node)).or_default().push(index);
        }
    }

    let mut cluster_of: VertexClusterMap = BTreeMap::new();
    let mut normals: VertexClusterNormals = BTreeMap::new();
    for ((component, node), faces) in &incident {
        let mut parent: Vec<usize> = (0..faces.len()).collect();
        fn find(parent: &mut [usize], mut x: usize) -> usize {
            while parent[x] != x {
                parent[x] = parent[parent[x]];
                x = parent[x];
            }
            x
        }
        for i in 0..faces.len() {
            for j in (i + 1)..faces.len() {
                let ni = tris[faces[i]].normal;
                let nj = tris[faces[j]].normal;
                let dot = ni.dot(nj);
                if dot > 0.0 && dot * dot >= cos2_feature {
                    let (ri, rj) = (find(&mut parent, i), find(&mut parent, j));
                    if ri != rj {
                        let (lo, hi) = if ri < rj { (ri, rj) } else { (rj, ri) };
                        parent[hi] = lo;
                    }
                }
            }
        }
        // Cluster ids follow the smallest member index, so they are stable under any
        // face ordering that preserves the canonical arranged-face order.
        let mut roots: Vec<usize> = (0..faces.len()).map(|i| find(&mut parent, i)).collect();
        let mut unique: Vec<usize> = roots.clone();
        unique.sort_unstable();
        unique.dedup();
        let mut accumulated: Vec<Vec3> = vec![Vec3::new(0.0, 0.0, 0.0); unique.len()];
        for (i, root) in roots.iter_mut().enumerate() {
            let cluster = unique.iter().position(|r| r == root).expect("root is listed");
            let tri = &tris[faces[i]];
            let area = norm(tri.b.sub(tri.a).cross(tri.c.sub(tri.a))) * 0.5;
            accumulated[cluster] = accumulated[cluster].add(tri.normal.scale(area));
            cluster_of.insert((*component, *node, faces[i]), cluster);
        }
        for (cluster, sum) in accumulated.into_iter().enumerate() {
            if let Some(normal) = unit(sum) {
                normals.insert((*component, *node, cluster), normal);
            }
        }
    }
    (cluster_of, normals)
}

// AI-FUNC-SUMMARY:
// Purpose: Seed one centroid sample per face side and one vertex sample per (component, vertex, smooth cluster, side).
// Inputs: arranged surface, query triangle table, vertex-cluster tables.
// Returns: Vec<GapSample> in deterministic order (centroids by face, then vertex samples by key).
// Side effects: None.
// Notes: One sample per smooth patch at a vertex, along that patch's own area-weighted normal -
//   see `build_vertex_clusters` for why a single averaged normal is wrong at a sharp vertex.
fn seed_surface_samples(
    surface: &ArrangedSurface,
    tris: &[QueryTri],
    cluster_of: &VertexClusterMap,
    cluster_normals: &VertexClusterNormals,
) -> Vec<GapSample> {
    let mut samples = Vec::new();
    for (index, tri) in tris.iter().enumerate() {
        if tri.face.is_none() || tri.normal.dot(tri.normal) == 0.0 {
            continue;
        }
        for side in [1i8, -1] {
            samples.push(new_surface_sample(
                tri.centroid(),
                index,
                tri.component,
                SampleKind::Centroid,
                side,
                tri.normal.scale(f64::from(side)),
                None,
            ));
        }
    }

    let mut cluster_tri: BTreeMap<(i32, usize, usize), usize> = BTreeMap::new();
    for ((component, node, tri), cluster) in cluster_of {
        cluster_tri
            .entry((*component, *node, *cluster))
            .or_insert(*tri);
    }
    for ((component, node, cluster), normal) in cluster_normals {
        let Some(&tri) = cluster_tri.get(&(*component, *node, *cluster)) else {
            continue;
        };
        for side in [1i8, -1] {
            samples.push(new_surface_sample(
                surface.vertices[*node],
                tri,
                *component,
                SampleKind::Vertex,
                side,
                normal.scale(f64::from(side)),
                Some((*node, *cluster)),
            ));
        }
    }
    samples
}

// AI-FUNC-SUMMARY: Construct an unpaired surface sample; `vertex` is `(node, smooth cluster)` for a vertex sample; returns GapSample; side effects: none.
fn new_surface_sample(
    point: Vec3,
    tri: usize,
    component: i32,
    kind: SampleKind,
    side: i8,
    direction: Vec3,
    vertex: Option<(usize, usize)>,
) -> GapSample {
    GapSample {
        point,
        tri,
        component,
        kind,
        side,
        direction,
        t_raw: f64::INFINITY,
        t: f64::INFINITY,
        t_exact: f64::INFINITY,
        pairing: None,
        pair_class: PairClass::Unpaired,
        flags: 0,
        applicable: FLAGS_ALL,
        vertex: vertex.map(|(node, _)| node),
        vertex_cluster: vertex.map(|(_, cluster)| cluster).unwrap_or(0),
    }
}

// AI-FUNC-SUMMARY:
// Purpose: Shoot one sample's ray and record the first opposing hit within 2*t_layer.
// Inputs: sample (mutated), triangle table, grid, t_layer, options.
// Returns: None.
// Side effects: Sets `t_raw`, `t`, and `pairing` on the sample when a hit is accepted.
// Notes: Excludes every triangle incident to the sample's own point (its vertex, or any vertex of
//   its face), so a corefined contact curve can never pair a surface with itself. The opposing test
//   is |dir . n_hat| >= cos(60 deg); when both walls are closed solids the material-side signs must
//   also oppose, which is what separates a real gap from the far side of the same wall.
fn cast_sample_ray(
    sample: &mut GapSample,
    tris: &[QueryTri],
    grid: &TriGrid,
    t_layer: f64,
    options: &GapFieldOptions,
) {
    let origin_tri = &tris[sample.tri];
    let delta = (1.0e-3 * options.h_bootstrap).max(options.eps);
    let length = 2.0 * t_layer;
    let origin = sample.point.add(sample.direction.scale(delta));
    let far = sample.point.add(sample.direction.scale(length));
    let lo = Vec3::new(
        origin.x.min(far.x),
        origin.y.min(far.y),
        origin.z.min(far.z),
    );
    let hi = Vec3::new(
        origin.x.max(far.x),
        origin.y.max(far.y),
        origin.z.max(far.z),
    );

    let launch_sign = origin_tri
        .outward
        .map(|outward| sample.direction.dot(outward).signum());

    let mut best: Option<(f64, usize, Vec3)> = None;
    for candidate in grid.query(lo, hi) {
        let index = candidate as usize;
        if index == sample.tri {
            continue;
        }
        let tri = &tris[index];
        if let Some(node) = sample.vertex {
            if tri.uses_node(node) {
                continue;
            }
        } else if origin_tri.face.is_some()
            && tri.face.is_some()
            && origin_tri.nodes.iter().any(|node| tri.uses_node(*node))
        {
            continue;
        }
        if tri.normal.dot(tri.normal) == 0.0 {
            continue;
        }
        let Some((hit, point)) = ray_triangle(origin, sample.direction, tri, length) else {
            continue;
        };
        if sample.direction.dot(tri.normal).abs() < COS_OPPOSING {
            continue;
        }
        if let (Some(launch), Some(outward)) = (launch_sign, tri.outward) {
            let hit_sign = sample.direction.dot(outward).signum();
            if hit_sign != -launch {
                continue;
            }
        }
        let key = (hit, index);
        if best
            .as_ref()
            .map(|(best_hit, best_index, _)| key < (*best_hit, *best_index))
            .unwrap_or(true)
        {
            best = Some((hit, index, point));
        }
    }

    if let Some((_, index, point)) = best {
        let separation = norm(point.sub(sample.point));
        sample.t_raw = separation;
        sample.t = separation;
        sample.pairing = Some(GapPairing {
            point,
            tri: index,
            component: tris[index].component,
            patch: -1,
        });
    }
}

// AI-FUNC-SUMMARY:
// Purpose: Moller-Trumbore ray-triangle intersection restricted to [0, length].
// Returns: Some((parameter, point)) on a hit, None otherwise.
// Side effects: None.
fn ray_triangle(
    origin: Vec3,
    direction: Vec3,
    tri: &QueryTri,
    length: f64,
) -> Option<(f64, Vec3)> {
    let e1 = tri.b.sub(tri.a);
    let e2 = tri.c.sub(tri.a);
    let pvec = direction.cross(e2);
    let det = e1.dot(pvec);
    if det.abs() <= 0.0 {
        return None;
    }
    let inv = 1.0 / det;
    let tvec = origin.sub(tri.a);
    let u = tvec.dot(pvec) * inv;
    if !(0.0..=1.0).contains(&u) {
        return None;
    }
    let qvec = tvec.cross(e1);
    let v = direction.dot(qvec) * inv;
    if v < 0.0 || u + v > 1.0 {
        return None;
    }
    let t = e2.dot(qvec) * inv;
    if t < 0.0 || t > length {
        return None;
    }
    Some((t, origin.add(direction.scale(t))))
}

// AI-FUNC-SUMMARY: One triangle-pair closest approach kept by the sweep; side effects: none.
#[derive(Debug, Clone, Copy)]
struct ClosestPair {
    tri_a: usize,
    tri_b: usize,
    point_a: Vec3,
    point_b: Vec3,
    distance: f64,
}

// AI-FUNC-SUMMARY:
// Purpose: Whether two triangles face each other across the segment joining their closest points.
// Inputs: unit direction from A to B, the two triangles.
// Returns: true when both walls are within 60 degrees of facing the segment and, for closed solids,
//   the material sides oppose.
// Side effects: None.
// Notes: This is what separates a real gap from the tessellation spacing of one curved patch: on a
//   curved strip the segment joining two nearby facets is almost tangential, so |dir . n| is small.
fn opposing_pair(direction: Vec3, from: &QueryTri, to: &QueryTri) -> bool {
    if from.component == to.component {
        // Within one patch the only admissible approach is a genuine facing pair: a
        // near-tangential segment is the patch's own tessellation spacing, and a
        // segment at 45-60 degrees to both normals is the wedge of material inside a
        // convex edge - neither is a thin feature.
        if direction.dot(from.normal).abs() < COS_OPPOSING_INTRA
            || direction.dot(to.normal).abs() < COS_OPPOSING_INTRA
        {
            return false;
        }
    }
    if let (Some(from_outward), Some(to_outward)) = (from.outward, to.outward) {
        let launch = direction.dot(from_outward);
        let arrival = direction.dot(to_outward);
        // Only decide the material-side rule when the segment is not grazing both
        // walls; a grazing approach (box corner, T-junction) has no meaningful side.
        if launch.abs() >= COS_OPPOSING
            && arrival.abs() >= COS_OPPOSING
            && launch.signum() != -arrival.signum()
        {
            return false;
        }
    }
    true
}

// AI-FUNC-SUMMARY:
// Purpose: BVH-free closest-pair sweep over every triangle pair within t_layer (PLAN topic E).
// Inputs: triangle table, grid, t_layer.
// Returns: one ClosestPair per qualifying unordered pair, in ascending (tri_a, tri_b) order.
// Side effects: None.
// Notes: Pairs sharing an arranged node are skipped - after corefinement those touch exactly and
//   are contacts, not gaps - and so are pairs that do not face each other (`opposing_pair`).
//   Geodesic shortcuts are cleared later, over ray and sweep correspondences alike. This is the
//   step that catches the oblique and edge-edge approaches the normal rays provably miss.
fn closest_pair_sweep(tris: &[QueryTri], grid: &TriGrid, t_layer: f64) -> Vec<ClosestPair> {
    // R-P1/R-P2: one unit of work per triangle, collected into an indexed buffer and
    // concatenated in triangle order - never in completion order.
    let per_triangle: Vec<Vec<ClosestPair>> = tris
        .par_iter()
        .enumerate()
        .map_init(Vec::new, |candidates: &mut Vec<u32>, (index, tri)| {
            let mut pairs: Vec<ClosestPair> = Vec::new();
            let (lo, hi) = tri_aabb(tri);
            let expanded_lo = Vec3::new(lo.x - t_layer, lo.y - t_layer, lo.z - t_layer);
            let expanded_hi = Vec3::new(hi.x + t_layer, hi.y + t_layer, hi.z + t_layer);
            grid.query_into(expanded_lo, expanded_hi, candidates);
            for candidate in candidates.iter().copied() {
            let other = candidate as usize;
            if other <= index {
                continue;
            }
            let tri_b = &tris[other];
            if tri.face.is_some()
                && tri_b.face.is_some()
                && tri.nodes.iter().any(|node| tri_b.uses_node(*node))
            {
                continue;
            }
            if tri.face.is_none() && tri_b.face.is_none() {
                continue;
            }
            let (distance, point_a, point_b) = triangle_closest_pair(tri, tri_b);
            if distance > t_layer || distance <= 0.0 {
                continue;
            }
            let Some(direction) = unit(point_b.sub(point_a)) else {
                continue;
            };
            if !opposing_pair(direction, tri, tri_b) {
                continue;
            }
            pairs.push(ClosestPair {
                    tri_a: index,
                    tri_b: other,
                    point_a,
                    point_b,
                    distance,
                });
            }
            pairs
        })
        .collect();
    per_triangle.into_iter().flatten().collect()
}

// AI-FUNC-SUMMARY:
// Purpose: Clear every pairing whose two triangles are also close *along the surface*.
// Inputs: samples (mutated), triangle table, the minimum geodesic/straight-line ratio.
// Returns: (number of pairings cleared, the faces involved keyed by their component pair).
// Side effects: Resets `pairing`, `t_raw`, `t` and `t_exact` on rejected samples.
// Notes: A gap is a straight line through *free space*; a short surface path between its ends means
//   the segment is a shortcut through the geometry instead. Two families are caught by exactly this
//   rule: the wedge of material inside a convex edge of one body, and the wedge either side of an
//   intersection curve between two bodies - after corefinement the two surfaces share edges along
//   that curve, so a path exists and is short. A genuine thin feature is the opposite: geodesically
//   far (around the rim, or across disconnected bodies where no path exists at all) while spatially
//   close. Applied to ray and closest-pair correspondences alike, since both produce these families.
//   One bounded Dijkstra runs per distinct source triangle, capped at `ratio * separation`.
fn reject_geodesic_shortcuts(
    samples: &mut [GapSample],
    tris: &[QueryTri],
    ratio: f64,
) -> (usize, BTreeMap<(i32, i32), BTreeSet<usize>>) {
    let mut contact_faces: BTreeMap<(i32, i32), BTreeSet<usize>> = BTreeMap::new();
    if ratio <= 0.0 {
        return (0, contact_faces);
    }
    let mut sources: BTreeMap<usize, Vec<usize>> = BTreeMap::new();
    for (index, sample) in samples.iter().enumerate() {
        let Some(pairing) = sample.pairing else {
            continue;
        };
        if tris[sample.tri].face.is_none() || tris[pairing.tri].face.is_none() {
            continue;
        }
        sources.entry(sample.tri).or_default().push(index);
    }
    if sources.is_empty() {
        return (0, contact_faces);
    }
    let adjacency = face_adjacency(tris);
    // A pairing between two surfaces with no path between them at all - two separate
    // bodies - can never be a shortcut, and is the common case. Answer it with one
    // union-find lookup instead of a search.
    let patch_of_face = connected_patches(&adjacency, tris.len());
    let adjacency_ref: &[Vec<u32>] = &adjacency;
    // R-P1/R-P2: one bounded search per source triangle, each with its own reusable
    // scratch, collected in source order. `sources` is a BTreeMap, so that order is
    // canonical and the result cannot depend on how the work was split.
    let source_list: Vec<(&usize, &Vec<usize>)> = sources.iter().collect();
    let rejected: Vec<usize> = source_list
        .par_iter()
        .map_init(
            GeodesicScratch::default,
            |scratch, (source, sample_indices)| {
                let mut local: Vec<usize> = Vec::new();
                if sample_indices.iter().all(|index| {
                    samples[*index]
                        .pairing
                        .map(|pairing| patch_of_face[pairing.tri] != patch_of_face[**source])
                        .unwrap_or(true)
                }) {
                    return local;
                }
                let budget = sample_indices
                    .iter()
                    .map(|index| separation_of(&samples[*index]))
                    .fold(0.0f64, f64::max)
                    * ratio;
                let reachable = bounded_geodesic(**source, adjacency_ref, tris, budget, scratch);
                for index in sample_indices.iter() {
                    let Some(pairing) = samples[*index].pairing else {
                        continue;
                    };
                    if let Some(surface_distance) = reachable.get(&pairing.tri) {
                        if *surface_distance < ratio * separation_of(&samples[*index]) {
                            local.push(*index);
                        }
                    }
                }
                local
            },
        )
        .flatten()
        .collect();
    for index in &rejected {
        let sample = &mut samples[*index];
        if let Some(pairing) = sample.pairing {
            let (own, other) = (tris[sample.tri].component, tris[pairing.tri].component);
            let pair = if own <= other {
                (own, other)
            } else {
                (other, own)
            };
            let entry = contact_faces.entry(pair).or_default();
            entry.insert(sample.tri);
            entry.insert(pairing.tri);
        }
        sample.pairing = None;
        sample.pair_class = PairClass::Unpaired;
        sample.t_raw = f64::INFINITY;
        sample.t = f64::INFINITY;
        sample.t_exact = f64::INFINITY;
    }
    (rejected.len(), contact_faces)
}

// AI-FUNC-SUMMARY: The separation a sample reports (exact closest pair when measured, else the ray); returns f64; side effects: none.
fn separation_of(sample: &GapSample) -> f64 {
    if sample.t_exact.is_finite() {
        sample.t_exact
    } else {
        sample.t_raw
    }
}

// AI-FUNC-SUMMARY:
// Purpose: Label the connected patches of the face-adjacency graph.
// Inputs: adjacency lists, triangle count.
// Returns: patch id per triangle.
// Side effects: None.
// Notes: Two triangles in different patches have no surface path at all, which settles the shortcut
//   question without a search - the common case for two separate bodies.
fn connected_patches(adjacency: &[Vec<u32>], count: usize) -> Vec<usize> {
    let mut patch = vec![usize::MAX; count];
    let mut next = 0usize;
    for start in 0..count {
        if patch[start] != usize::MAX {
            continue;
        }
        let id = next;
        next += 1;
        patch[start] = id;
        let mut stack = vec![start];
        while let Some(current) = stack.pop() {
            for &neighbour in &adjacency[current] {
                let other = neighbour as usize;
                if patch[other] == usize::MAX {
                    patch[other] = id;
                    stack.push(other);
                }
            }
        }
    }
    patch
}

// AI-FUNC-SUMMARY:
// Purpose: Edge-adjacent face neighbours over the whole arranged complex.
// Returns: neighbour lists per triangle.
// Side effects: None.
// Notes: Adjacency deliberately crosses component boundaries. Corefinement makes two intersecting
//   surfaces share arranged edges along their intersection curve, so the surface path from one to
//   the other exists and is short near the curve - which is what lets `reject_geodesic_shortcuts`
//   tell that wedge apart from a real gap between two disconnected bodies.
fn face_adjacency(tris: &[QueryTri]) -> Vec<Vec<u32>> {
    let mut edge_faces: BTreeMap<(usize, usize), Vec<usize>> = BTreeMap::new();
    for (index, tri) in tris.iter().enumerate() {
        if tri.face.is_none() {
            continue;
        }
        for k in 0..3 {
            edge_faces
                .entry(edge_key(tri.nodes[k], tri.nodes[(k + 1) % 3]))
                .or_default()
                .push(index);
        }
    }
    let mut adjacency = vec![Vec::new(); tris.len()];
    for incident in edge_faces.values() {
        for (position, left) in incident.iter().enumerate() {
            for right in incident.iter().skip(position + 1) {
                adjacency[*left].push(*right as u32);
                adjacency[*right].push(*left as u32);
            }
        }
    }
    for list in adjacency.iter_mut() {
        list.sort_unstable();
        list.dedup();
    }
    adjacency
}

// AI-FUNC-SUMMARY:
// Purpose: Bounded Dijkstra over face adjacency, measuring centroid-to-centroid path length.
// Returns: reachable triangles mapped to their surface distance, within `budget`.
// Side effects: None.
// Notes: Distances are compared through sortable integer keys so the frontier order never depends
//   on float tie-breaking (SPEC_meshgen_numerics §8.1 Rule N7).
fn bounded_geodesic<'a>(
    source: usize,
    adjacency: &[Vec<u32>],
    tris: &[QueryTri],
    budget: f64,
    scratch: &'a mut GeodesicScratch,
) -> &'a BTreeMap<usize, f64> {
    scratch.reset();
    if budget <= 0.0 {
        return &scratch.best;
    }
    let mut visits = 0usize;
    scratch.best.insert(source, 0.0);
    scratch.frontier.push(Reverse((0u64, source)));
    while let Some(Reverse((key, current))) = scratch.frontier.pop() {
        visits += 1;
        if visits > GEODESIC_MAX_VISITS {
            break;
        }
        let distance = scratch.best.get(&current).copied().unwrap_or(f64::INFINITY);
        // Lazy deletion: a stale queue entry is one whose key no longer matches the
        // best distance recorded for that face.
        if key != distance.to_bits() || distance > budget {
            continue;
        }
        let from = tris[current].centroid();
        for &neighbour in &adjacency[current] {
            let other = neighbour as usize;
            let step = norm(tris[other].centroid().sub(from));
            let candidate = distance + step;
            if candidate > budget {
                continue;
            }
            if candidate < scratch.best.get(&other).copied().unwrap_or(f64::INFINITY) {
                scratch.best.insert(other, candidate);
                scratch.frontier.push(Reverse((candidate.to_bits(), other)));
            }
        }
    }
    &scratch.best
}

// AI-FUNC-SUMMARY:
// Purpose: Reusable working set for `bounded_geodesic`, owned by one thread.
// Notes: The pass runs one search per source triangle; allocating a fresh map and queue each time
//   makes the global allocator the bottleneck and the parallel loop stops scaling. The binary heap
//   is keyed by `(distance bits, face)`, a total order with a unique tiebreak, so pop order - and
//   therefore the result - does not depend on timing.
#[derive(Default)]
struct GeodesicScratch {
    best: BTreeMap<usize, f64>,
    frontier: BinaryHeap<Reverse<(u64, usize)>>,
}

impl GeodesicScratch {
    // AI-FUNC-SUMMARY: Clear both structures for the next source; side effects: mutates self.
    fn reset(&mut self) {
        self.best.clear();
        self.frontier.clear();
    }
}

// AI-FUNC-SUMMARY:
// Purpose: Exact closest pair of two non-intersecting triangles (9 edge-edge + 6 vertex-face candidates).
// Returns: (distance, point on A, point on B).
// Side effects: None.
// Notes: Valid because the arrangement leaves no properly intersecting face pair; penetrating
//   triangles would need an interior-crossing candidate this enumeration does not carry.
fn triangle_closest_pair(a: &QueryTri, b: &QueryTri) -> (f64, Vec3, Vec3) {
    let va = [a.a, a.b, a.c];
    let vb = [b.a, b.b, b.c];
    let mut best = (f64::INFINITY, va[0], vb[0]);
    let mut consider = |distance: f64, pa: Vec3, pb: Vec3| {
        if distance < best.0 {
            best = (distance, pa, pb);
        }
    };
    for i in 0..3 {
        for j in 0..3 {
            let (pa, pb) = segment_closest(va[i], va[(i + 1) % 3], vb[j], vb[(j + 1) % 3]);
            consider(norm(pb.sub(pa)), pa, pb);
        }
    }
    for i in 0..3 {
        let pb = point_triangle_closest(va[i], vb[0], vb[1], vb[2]);
        consider(norm(pb.sub(va[i])), va[i], pb);
        let pa = point_triangle_closest(vb[i], va[0], va[1], va[2]);
        consider(norm(vb[i].sub(pa)), pa, vb[i]);
    }
    best
}

// AI-FUNC-SUMMARY: Closest point pair between two segments (clamped, parallel-safe); returns (point on p, point on q); side effects: none.
fn segment_closest(p0: Vec3, p1: Vec3, q0: Vec3, q1: Vec3) -> (Vec3, Vec3) {
    let d1 = p1.sub(p0);
    let d2 = q1.sub(q0);
    let r = p0.sub(q0);
    let a = d1.dot(d1);
    let e = d2.dot(d2);
    let f = d2.dot(r);
    let (mut s, mut t);
    if a <= 0.0 && e <= 0.0 {
        return (p0, q0);
    }
    if a <= 0.0 {
        s = 0.0;
        t = (f / e).clamp(0.0, 1.0);
    } else {
        let c = d1.dot(r);
        if e <= 0.0 {
            t = 0.0;
            s = (-c / a).clamp(0.0, 1.0);
        } else {
            let b = d1.dot(d2);
            let denom = a * e - b * b;
            s = if denom > 0.0 {
                ((b * f - c * e) / denom).clamp(0.0, 1.0)
            } else {
                0.0
            };
            t = (b * s + f) / e;
            if t < 0.0 {
                t = 0.0;
                s = (-c / a).clamp(0.0, 1.0);
            } else if t > 1.0 {
                t = 1.0;
                s = ((b - c) / a).clamp(0.0, 1.0);
            }
        }
    }
    (p0.add(d1.scale(s)), q0.add(d2.scale(t)))
}

// AI-FUNC-SUMMARY: Closest point to `p` on triangle (a,b,c) by clamped barycentric regions; returns Vec3; side effects: none.
fn point_triangle_closest(p: Vec3, a: Vec3, b: Vec3, c: Vec3) -> Vec3 {
    let ab = b.sub(a);
    let ac = c.sub(a);
    let ap = p.sub(a);
    let d1 = ab.dot(ap);
    let d2 = ac.dot(ap);
    if d1 <= 0.0 && d2 <= 0.0 {
        return a;
    }
    let bp = p.sub(b);
    let d3 = ab.dot(bp);
    let d4 = ac.dot(bp);
    if d3 >= 0.0 && d4 <= d3 {
        return b;
    }
    let vc = d1 * d4 - d3 * d2;
    if vc <= 0.0 && d1 >= 0.0 && d3 <= 0.0 {
        let v = if d1 - d3 != 0.0 { d1 / (d1 - d3) } else { 0.0 };
        return a.add(ab.scale(v));
    }
    let cp = p.sub(c);
    let d5 = ab.dot(cp);
    let d6 = ac.dot(cp);
    if d6 >= 0.0 && d5 <= d6 {
        return c;
    }
    let vb = d5 * d2 - d1 * d6;
    if vb <= 0.0 && d2 >= 0.0 && d6 <= 0.0 {
        let w = if d2 - d6 != 0.0 { d2 / (d2 - d6) } else { 0.0 };
        return a.add(ac.scale(w));
    }
    let va = d3 * d6 - d5 * d4;
    if va <= 0.0 && (d4 - d3) >= 0.0 && (d5 - d6) >= 0.0 {
        let denom = (d4 - d3) + (d5 - d6);
        let w = if denom != 0.0 { (d4 - d3) / denom } else { 0.0 };
        return b.add(c.sub(b).scale(w));
    }
    let denom = va + vb + vc;
    if denom == 0.0 {
        return a;
    }
    let v = vb / denom;
    let w = vc / denom;
    a.add(ab.scale(v)).add(ac.scale(w))
}

// AI-FUNC-SUMMARY:
// Purpose: Build the sample adjacency graph used by smoothing, densification, and battery checks 3/4.
// Inputs: samples, triangle table, arranged surface.
// Returns: sorted neighbour lists per sample.
// Side effects: None.
// Notes: Centroid samples link to their face's vertex samples and to the centroid samples of
//   edge-adjacent faces of the same component and side; seeded samples link to their face centroid.
fn build_adjacency(
    samples: &[GapSample],
    tris: &[QueryTri],
    cluster_of: &VertexClusterMap,
) -> Vec<Vec<u32>> {
    let mut centroid_of: BTreeMap<(usize, i8), u32> = BTreeMap::new();
    let mut vertex_of: BTreeMap<(i32, usize, usize, i8), u32> = BTreeMap::new();
    for (index, sample) in samples.iter().enumerate() {
        match sample.kind {
            SampleKind::Centroid => {
                centroid_of.insert((sample.tri, sample.side), index as u32);
            }
            SampleKind::Vertex => {
                if let Some(node) = sample.vertex {
                    vertex_of.insert(
                        (sample.component, node, sample.vertex_cluster, sample.side),
                        index as u32,
                    );
                }
            }
            _ => {}
        }
    }

    let mut edge_faces: BTreeMap<(i32, usize, usize), Vec<usize>> = BTreeMap::new();
    for (index, tri) in tris.iter().enumerate() {
        if tri.face.is_none() {
            continue;
        }
        for k in 0..3 {
            let a = tri.nodes[k];
            let b = tri.nodes[(k + 1) % 3];
            let key = if a < b {
                (tri.component, a, b)
            } else {
                (tri.component, b, a)
            };
            edge_faces.entry(key).or_default().push(index);
        }
    }

    let mut adjacency = vec![Vec::new(); samples.len()];
    let link = |adjacency: &mut Vec<Vec<u32>>, a: u32, b: u32| {
        if a == b {
            return;
        }
        adjacency[a as usize].push(b);
        adjacency[b as usize].push(a);
    };

    for (&(tri_index, side), &centroid) in &centroid_of {
        let tri = &tris[tri_index];
        for node in tri.nodes {
            // A face links only to the vertex sample of its own smooth patch.
            let Some(&cluster) = cluster_of.get(&(tri.component, node, tri_index)) else {
                continue;
            };
            if let Some(&vertex) = vertex_of.get(&(tri.component, node, cluster, side)) {
                link(&mut adjacency, centroid, vertex);
            }
        }
        for k in 0..3 {
            let a = tri.nodes[k];
            let b = tri.nodes[(k + 1) % 3];
            let key = if a < b {
                (tri.component, a, b)
            } else {
                (tri.component, b, a)
            };
            let Some(incident) = edge_faces.get(&key) else {
                continue;
            };
            for &neighbour in incident {
                if neighbour <= tri_index {
                    continue;
                }
                if let Some(&other) = centroid_of.get(&(neighbour, side)) {
                    link(&mut adjacency, centroid, other);
                }
            }
        }
    }

    for (index, sample) in samples.iter().enumerate() {
        if matches!(sample.kind, SampleKind::Centroid | SampleKind::Vertex) {
            continue;
        }
        if let Some(&centroid) = centroid_of.get(&(sample.tri, sample.side)) {
            link(&mut adjacency, index as u32, centroid);
        }
    }

    for list in adjacency.iter_mut() {
        list.sort_unstable();
        list.dedup();
    }
    adjacency
}

// AI-FUNC-SUMMARY:
// Purpose: Adaptive densification where |grad t| between adjacent samples exceeds 0.5 (PLAN §10.5),
//   or where a thin gap is described more coarsely than the element size that will mesh it.
// Inputs: samples and adjacency (both mutated), triangle table, grid, t_layer, options.
// Returns: number of samples added.
// Side effects: Appends midpoint samples and their adjacency entries.
// Notes: Runs at most `densify_rounds` rounds and never subdivides below 2*eps.
//   The gradient criterion alone describes a *varying* gap well and a **uniform** one not at all,
//   which is backwards: a uniform gap is the canonical sheet or band. A plate modelled as a box
//   carries two samples per wall, the gradient between them is zero, no round fires, and the region
//   then dies on the speck floor - so the one shape the thin path exists for could never reach it.
//   The second criterion is spatial and applies only inside the thin band: a gap the sizing field
//   will resolve at `h_bootstrap` has to be *measured* at `h_bootstrap`, or its extent, its rims and
//   its mid-surface are all inferred from a handful of corners.
fn densify(
    samples: &mut Vec<GapSample>,
    adjacency: &mut Vec<Vec<u32>>,
    tris: &[QueryTri],
    grid: &TriGrid,
    t_layer: f64,
    options: &GapFieldOptions,
) -> usize {
    let mut added = 0usize;
    for _ in 0..options.densify_rounds {
        let mut candidates: Vec<(usize, usize)> = Vec::new();
        for (index, list) in adjacency.iter().enumerate() {
            for &neighbour in list {
                let other = neighbour as usize;
                if other <= index {
                    continue;
                }
                let a = &samples[index];
                let b = &samples[other];
                if !a.t_raw.is_finite() || !b.t_raw.is_finite() || a.side != b.side {
                    continue;
                }
                let distance = norm(b.point.sub(a.point));
                if distance < 2.0 * options.eps {
                    continue;
                }
                let undersampled = a.t_raw <= t_layer
                    && b.t_raw <= t_layer
                    && distance > options.h_bootstrap;
                if undersampled || (a.t_raw - b.t_raw).abs() / distance > GRADIENT_LIMIT {
                    candidates.push((index, other));
                }
            }
        }
        if candidates.is_empty() {
            break;
        }
        for (index, other) in candidates {
            let midpoint = samples[index].point.add(samples[other].point).scale(0.5);
            let Some(direction) = unit(samples[index].direction.add(samples[other].direction))
            else {
                continue;
            };
            let mut sample = new_surface_sample(
                midpoint,
                samples[index].tri,
                samples[index].component,
                SampleKind::Densified,
                samples[index].side,
                direction,
                None,
            );
            cast_sample_ray(&mut sample, tris, grid, t_layer, options);
            samples.push(sample);
            adjacency.push(vec![index as u32, other as u32]);
            let new_index = (samples.len() - 1) as u32;
            adjacency[index].push(new_index);
            adjacency[other].push(new_index);
            added += 1;
        }
        for list in adjacency.iter_mut() {
            list.sort_unstable();
            list.dedup();
        }
    }
    added
}

// AI-FUNC-SUMMARY:
// Purpose: Jacobi smoothing of the ray field over finite samples (the reference thin-feature design §3.2, 3 sweeps).
// Inputs: samples (mutated), adjacency, sweep count.
// Returns: None.
// Side effects: Updates `t` on finite samples; `t_raw` and `t_exact` are never touched.
// Notes: Jacobi (not Gauss-Seidel) so the result does not depend on traversal order.
fn smooth_field(samples: &mut [GapSample], adjacency: &[Vec<u32>], sweeps: usize) {
    for _ in 0..sweeps {
        let previous: Vec<f64> = samples.iter().map(|sample| sample.t).collect();
        for (index, sample) in samples.iter_mut().enumerate() {
            if !sample.t.is_finite() {
                continue;
            }
            let mut sum = 0.0;
            let mut count = 0usize;
            for &neighbour in &adjacency[index] {
                let value = previous[neighbour as usize];
                if value.is_finite() {
                    sum += value;
                    count += 1;
                }
            }
            if count > 0 {
                sample.t = 0.5 * previous[index] + 0.5 * (sum / count as f64);
            }
        }
    }
}

// AI-FUNC-SUMMARY:
// Purpose: Label connected patches of the triangles that are pairing targets (battery check 2).
// Inputs: samples, triangle table.
// Returns: patch id per triangle (-1 when the triangle is never a target).
// Side effects: None.
// Notes: Adjacency is shared-edge within one component, so a wall facing two different opposite
//   patches (branching throat) yields two groups instead of one chimeric pairing.
fn build_opposite_patches(samples: &[GapSample], tris: &[QueryTri]) -> Vec<i32> {
    let mut is_target = vec![false; tris.len()];
    for sample in samples {
        if let Some(pairing) = &sample.pairing {
            is_target[pairing.tri] = true;
        }
    }
    let mut edge_faces: BTreeMap<(i32, usize, usize), Vec<usize>> = BTreeMap::new();
    for (index, tri) in tris.iter().enumerate() {
        if !is_target[index] || tri.face.is_none() {
            continue;
        }
        for k in 0..3 {
            let a = tri.nodes[k];
            let b = tri.nodes[(k + 1) % 3];
            let key = if a < b {
                (tri.component, a, b)
            } else {
                (tri.component, b, a)
            };
            edge_faces.entry(key).or_default().push(index);
        }
    }
    let mut patch = vec![-1i32; tris.len()];
    let mut next = 0i32;
    for start in 0..tris.len() {
        if !is_target[start] || patch[start] >= 0 {
            continue;
        }
        let id = next;
        next += 1;
        let mut stack = vec![start];
        patch[start] = id;
        while let Some(current) = stack.pop() {
            let tri = &tris[current];
            if tri.face.is_none() {
                continue;
            }
            for k in 0..3 {
                let a = tri.nodes[k];
                let b = tri.nodes[(k + 1) % 3];
                let key = if a < b {
                    (tri.component, a, b)
                } else {
                    (tri.component, b, a)
                };
                let Some(incident) = edge_faces.get(&key) else {
                    continue;
                };
                for &neighbour in incident {
                    if patch[neighbour] < 0 {
                        patch[neighbour] = id;
                        stack.push(neighbour);
                    }
                }
            }
        }
    }
    patch
}

// AI-FUNC-SUMMARY:
// Purpose: Classify a sample's correspondence into the PLAN §10.5 pair classes.
// Returns: PairClass (Unpaired when the sample has no pairing).
// Side effects: None.
fn classify_pair(
    sample: &GapSample,
    tris: &[QueryTri],
    classification: &BTreeMap<i32, ComponentClassification>,
) -> PairClass {
    let Some(pairing) = &sample.pairing else {
        return PairClass::Unpaired;
    };
    let own = sample.component;
    let other = tris[pairing.tri].component;
    if tris[pairing.tri].face.is_none() {
        return PairClass::SurfaceBox(own);
    }
    if tris[sample.tri].face.is_none() {
        return PairClass::SurfaceBox(other);
    }
    if own == other {
        return PairClass::Intra(own);
    }
    let (lo, hi) = if own <= other {
        (own, other)
    } else {
        (other, own)
    };
    let is_sheet = |x: i32| matches!(classification.get(&x), Some(ComponentClassification::Sheet));
    match (is_sheet(lo), is_sheet(hi)) {
        (true, true) => PairClass::SheetSheet(lo, hi),
        (true, false) => PairClass::SolidSheet(hi, lo),
        (false, true) => PairClass::SolidSheet(lo, hi),
        (false, false) => PairClass::Inter(lo, hi),
    }
}

// AI-FUNC-SUMMARY:
// Purpose: Run PLAN 2's five-check pairing battery over every paired sample.
// Inputs: samples (mutated), adjacency, triangle table, arranged surface, options, stats sink.
// Returns: None.
// Side effects: Sets `flags`/`applicable` per sample and increments per-check failure counters.
// Notes: Checks are evaluated in the frozen order 1..5; a check that cannot apply to a sample (for
//   example continuity against a virtual domain wall) is removed from `applicable` rather than
//   silently passed.
fn run_battery(
    samples: &mut [GapSample],
    adjacency: &[Vec<u32>],
    tris: &[QueryTri],
    cluster_of: &VertexClusterMap,
    options: &GapFieldOptions,
    stats: &mut GapFieldStats,
) {
    let radius = MUTUAL_RADIUS_FACTOR * options.h_bootstrap;
    // Spatial bucket index over sample points (the reference mesher's kdtree role): check 1 asks
    // which samples of the opposite wall sit near q, not which triangle hosts them.
    let cell = radius.max(f64::MIN_POSITIVE);
    let bucket_key = |point: Vec3| -> [i64; 3] {
        [
            (point.x / cell).floor() as i64,
            (point.y / cell).floor() as i64,
            (point.z / cell).floor() as i64,
        ]
    };
    let mut samples_by_cell: BTreeMap<[i64; 3], Vec<usize>> = BTreeMap::new();
    for (index, sample) in samples.iter().enumerate() {
        samples_by_cell
            .entry(bucket_key(sample.point))
            .or_default()
            .push(index);
    }
    let neighbourhood = |point: Vec3| -> Vec<usize> {
        let base = bucket_key(point);
        let mut out = Vec::new();
        for dz in -1..=1 {
            for dy in -1..=1 {
                for dx in -1..=1 {
                    if let Some(list) =
                        samples_by_cell.get(&[base[0] + dx, base[1] + dy, base[2] + dz])
                    {
                        out.extend_from_slice(list);
                    }
                }
            }
        }
        out.sort_unstable();
        out.dedup();
        out
    };

    // Check 1 - mutual-ray consistency.
    let snapshot: Vec<(Vec3, f64, Option<GapPairing>, usize)> = samples
        .iter()
        .map(|sample| (sample.point, sample.t_raw, sample.pairing, sample.tri))
        .collect();
    // R-P1/R-P2: check 1 reads only the immutable snapshot, so each sample decides
    // independently; the verdicts are collected into an indexed buffer and applied in
    // index order.
    let mutual: Vec<Option<bool>> = (0..samples.len())
        .into_par_iter()
        .map(|index| {
            let pairing = snapshot[index].2?;
            if tris[pairing.tri].face.is_none() {
                return Some(false);
            }
            let point = snapshot[index].0;
            let t = if snapshot[index].1.is_finite() {
                snapshot[index].1
            } else {
                samples[index].t_exact
            };
            for other in neighbourhood(pairing.point) {
                if other == index || norm(snapshot[other].0.sub(pairing.point)) > radius {
                    continue;
                }
                let other_t = if snapshot[other].1.is_finite() {
                    snapshot[other].1
                } else {
                    samples[other].t_exact
                };
                let back = snapshot[other].2.map(|back| norm(back.point.sub(point)));
                if other_t.is_finite()
                    && t.is_finite()
                    && (other_t - t).abs() <= MUTUAL_RATIO * t.max(other_t)
                    && back.map(|distance| distance <= radius).unwrap_or(false)
                {
                    return Some(true);
                }
            }
            Some(false)
        })
        .collect();
    for (index, verdict) in mutual.into_iter().enumerate() {
        match verdict {
            None => {}
            Some(true) => samples[index].flags |= FLAG_MUTUAL,
            Some(false) => {
                // A virtual domain wall carries no samples of its own, so nothing there
                // can pair back; the check does not apply rather than failing.
                if samples[index]
                    .pairing
                    .map(|pairing| tris[pairing.tri].face.is_none())
                    .unwrap_or(false)
                {
                    samples[index].applicable &= !FLAG_MUTUAL;
                } else {
                    stats.n_battery_failures[0] += 1;
                }
            }
        }
    }

    // Check 2 - the pairing target resolves to an opposite patch.
    for sample in samples.iter_mut() {
        match sample.pairing {
            Some(pairing) if pairing.patch >= 0 => sample.flags |= FLAG_OPPOSITE_PATCH,
            Some(_) => stats.n_battery_failures[1] += 1,
            None => {}
        }
    }

    // Check 3 - pairing continuity within the 2-ring of the opposite wall.
    //
    // A sample is judged against its *own* neighbourhood, not vetoed by any one member of
    // it. Marking both ends of a discontinuous pair failed reads as symmetric fairness and
    // is not: the discontinuity has one author. At a plate's rim a sample's ray leaves the
    // gap and lands on a far body, and under the veto that single outlier condemned all
    // eight of its interior neighbours - measured on the sheet fixture, 80 vetoes from a
    // handful of rim samples, taking the region's confidence to 0.82 against a 0.90 gate
    // and putting the one shape the thin path exists for out of reach. A majority test
    // makes the outlier fail alone, which is what it is.
    let ring2 = build_face_two_ring(tris);
    let mut continuity_agree = vec![0usize; samples.len()];
    let mut continuity_disagree = vec![0usize; samples.len()];
    for index in 0..samples.len() {
        let Some(pairing) = samples[index].pairing else {
            continue;
        };
        if tris[pairing.tri].face.is_none() {
            samples[index].applicable &= !FLAG_CONTINUITY;
            continue;
        }
        for &neighbour in &adjacency[index] {
            let other = neighbour as usize;
            let Some(other_pairing) = samples[other].pairing else {
                continue;
            };
            if samples[other].side != samples[index].side {
                continue;
            }
            if tris[other_pairing.tri].face.is_none() {
                continue;
            }
            let near = pairing.tri == other_pairing.tri
                || ring2
                    .get(&pairing.tri)
                    .map(|set| set.contains(&other_pairing.tri))
                    .unwrap_or(false);
            if near {
                continuity_agree[index] += 1;
            } else {
                continuity_disagree[index] += 1;
                if std::env::var_os("RUSTMSPT_THIN_DIAG").is_some() {
                    eprintln!(
                        "[S3-DIAG] continuity: sample {index} at ({:.4},{:.4},{:.4}) tri {} -> tri {} vs neighbour {other} at ({:.4},{:.4},{:.4}) tri {} -> tri {}, |sep| {:.6} vs {:.6}",
                        samples[index].point.x, samples[index].point.y, samples[index].point.z,
                        samples[index].tri, pairing.tri,
                        samples[other].point.x, samples[other].point.y, samples[other].point.z,
                        samples[other].tri, other_pairing.tri,
                        separation_of(&samples[index]), separation_of(&samples[other]),
                    );
                }
            }
        }
    }
    for (index, sample) in samples.iter_mut().enumerate() {
        if sample.pairing.is_none() || sample.applicable & FLAG_CONTINUITY == 0 {
            continue;
        }
        if continuity_disagree[index] > continuity_agree[index] {
            stats.n_battery_failures[2] += 1;
        } else {
            sample.flags |= FLAG_CONTINUITY;
        }
    }

    // Check 4 - adjacent correspondence segments must not cross (fold detection).
    let mut crossing_ok = vec![true; samples.len()];
    for index in 0..samples.len() {
        let Some(pairing) = samples[index].pairing else {
            continue;
        };
        for &neighbour in &adjacency[index] {
            let other = neighbour as usize;
            if other <= index {
                continue;
            }
            let Some(other_pairing) = samples[other].pairing else {
                continue;
            };
            if samples[other].side != samples[index].side {
                continue;
            }
            let (ca, cb) = segment_closest(
                samples[index].point,
                pairing.point,
                samples[other].point,
                other_pairing.point,
            );
            let separation = norm(cb.sub(ca));
            let scale = norm(pairing.point.sub(samples[index].point))
                .min(norm(other_pairing.point.sub(samples[other].point)));
            let interior = norm(ca.sub(samples[index].point)) > CROSSING_SHARE * scale
                && norm(ca.sub(pairing.point)) > CROSSING_SHARE * scale
                && norm(cb.sub(samples[other].point)) > CROSSING_SHARE * scale
                && norm(cb.sub(other_pairing.point)) > CROSSING_SHARE * scale;
            if interior && separation < CROSSING_SHARE * scale {
                crossing_ok[index] = false;
                crossing_ok[other] = false;
            }
        }
    }
    for (index, sample) in samples.iter_mut().enumerate() {
        if sample.pairing.is_none() {
            continue;
        }
        if crossing_ok[index] {
            sample.flags |= FLAG_NO_CROSSING;
        } else {
            stats.n_battery_failures[3] += 1;
        }
    }

    // Check 5 - the correspondence must not reverse orientation on a face.
    let mut orientation_ok = vec![None; samples.len()];
    let mut vertex_samples: BTreeMap<(i32, usize, usize, i8), usize> = BTreeMap::new();
    for (index, sample) in samples.iter().enumerate() {
        if sample.kind == SampleKind::Vertex {
            if let Some(node) = sample.vertex {
                vertex_samples.insert(
                    (sample.component, node, sample.vertex_cluster, sample.side),
                    index,
                );
            }
        }
    }
    for (tri_index, tri) in tris.iter().enumerate() {
        if tri.face.is_none() {
            continue;
        }
        for side in [1i8, -1] {
            let mut members = Vec::with_capacity(3);
            for node in tri.nodes {
                // The triple must come from this face's own smooth patch at each vertex.
                let Some(&cluster) = cluster_of.get(&(tri.component, node, tri_index)) else {
                    continue;
                };
                if let Some(&index) = vertex_samples.get(&(tri.component, node, cluster, side)) {
                    members.push(index);
                }
            }
            if members.len() != 3 {
                continue;
            }
            let mapped: Vec<Vec3> = members
                .iter()
                .filter_map(|index| samples[*index].pairing.map(|pairing| pairing.point))
                .collect();
            if mapped.len() != 3 {
                continue;
            }
            let source = tri.b.sub(tri.a).cross(tri.c.sub(tri.a));
            let image = mapped[1].sub(mapped[0]).cross(mapped[2].sub(mapped[0]));
            let preserved = source.dot(image) > 0.0;
            for index in &members {
                orientation_ok[*index] = Some(preserved);
            }
        }
    }
    for (index, sample) in samples.iter_mut().enumerate() {
        if sample.pairing.is_none() {
            continue;
        }
        match orientation_ok[index] {
            Some(true) => sample.flags |= FLAG_ORIENTATION,
            Some(false) => stats.n_battery_failures[4] += 1,
            None => sample.applicable &= !FLAG_ORIENTATION,
        }
    }
}

// AI-FUNC-SUMMARY: Two-ring neighbourhood of every arranged face (shared vertices, same component); returns map; side effects: none.
fn build_face_two_ring(tris: &[QueryTri]) -> BTreeMap<usize, BTreeSet<usize>> {
    let mut node_faces: BTreeMap<(i32, usize), Vec<usize>> = BTreeMap::new();
    for (index, tri) in tris.iter().enumerate() {
        if tri.face.is_none() {
            continue;
        }
        for node in tri.nodes {
            node_faces.entry((tri.component, node)).or_default().push(index);
        }
    }
    let mut ring1: BTreeMap<usize, BTreeSet<usize>> = BTreeMap::new();
    for (index, tri) in tris.iter().enumerate() {
        if tri.face.is_none() {
            continue;
        }
        let entry = ring1.entry(index).or_default();
        for node in tri.nodes {
            if let Some(faces) = node_faces.get(&(tri.component, node)) {
                entry.extend(faces.iter().copied());
            }
        }
    }
    let mut ring2: BTreeMap<usize, BTreeSet<usize>> = BTreeMap::new();
    for (index, neighbours) in &ring1 {
        let entry = ring2.entry(*index).or_default();
        for neighbour in neighbours {
            if let Some(second) = ring1.get(neighbour) {
                entry.extend(second.iter().copied());
            }
        }
    }
    ring2
}

// AI-FUNC-SUMMARY:
// Purpose: Group paired samples by (component, side, opposite patch) and score per-group confidence.
// Inputs: samples, confidence gate, stats sink.
// Returns: groups sorted by their key.
// Side effects: Updates group counters on stats.
// Notes: `t_r` is the group's exact closest-pair distance when the sweep measured one, else the
//   smallest ray separation - the Rule S3-M quantity a later regime decision compares against.
fn build_groups(
    samples: &[GapSample],
    confidence_min: f64,
    stats: &mut GapFieldStats,
) -> Vec<GapGroup> {
    let mut buckets: BTreeMap<(i32, i8, i32), Vec<usize>> = BTreeMap::new();
    for (index, sample) in samples.iter().enumerate() {
        let Some(pairing) = &sample.pairing else {
            continue;
        };
        buckets
            .entry((sample.component, sample.side, pairing.patch))
            .or_default()
            .push(index);
    }
    let mut groups = Vec::with_capacity(buckets.len());
    for ((component, side, opposite_patch), members) in buckets {
        let passing = members
            .iter()
            .filter(|index| samples[**index].passes_battery())
            .count();
        let confidence = passing as f64 / members.len() as f64;
        let mut t_r = f64::INFINITY;
        for index in &members {
            let sample = &samples[*index];
            let value = if sample.t_exact.is_finite() {
                sample.t_exact
            } else {
                sample.t_raw
            };
            if value < t_r {
                t_r = value;
            }
        }
        let pair_class = members
            .iter()
            .map(|index| samples[*index].pair_class)
            .min()
            .unwrap_or(PairClass::Unpaired);
        if confidence < confidence_min {
            stats.n_low_confidence_groups += 1;
        }
        groups.push(GapGroup {
            component,
            side,
            opposite_patch,
            pair_class,
            samples: members,
            confidence,
            t_r,
        });
    }
    stats.n_groups = groups.len();
    groups
}

// AI-FUNC-SUMMARY:
// Purpose: Collect, per component pair, the arranged nodes lying on their intersection curves.
// Inputs: the arranged surface.
// Returns: map from the sorted component pair to that pair's intersection-curve nodes.
// Side effects: None.
// Notes: S2 already computed these curves exactly; a thin region that reaches one is the
//   neighbourhood of the curve rather than a gap, because the separation closes to zero there.
fn intersection_curve_nodes(surface: &ArrangedSurface) -> BTreeMap<(i32, i32), BTreeSet<usize>> {
    let mut nodes: BTreeMap<(i32, i32), BTreeSet<usize>> = BTreeMap::new();
    for curve in &surface.curves {
        if curve.kind != ArrangedCurveKind::Intersection {
            continue;
        }
        let mut components: Vec<i32> = curve.components.iter().copied().collect();
        components.sort_unstable();
        components.dedup();
        for (position, left) in components.iter().enumerate() {
            for right in components.iter().skip(position) {
                nodes
                    .entry((*left, *right))
                    .or_default()
                    .extend(curve.nodes.iter().copied());
            }
        }
    }
    nodes
}

// AI-FUNC-SUMMARY:
// Purpose: Segment paired samples into thin regions, gate them, and build rims + mid-surfaces (G3-2).
// Inputs: samples, groups, triangle table, adjacency, thresholds and options, stats sink.
// Returns: regions ordered by their group key, each with a regime and any `[THIN-SKIP]` reason.
// Side effects: Updates region counters on stats.
// Notes: Growth is the reference thin-feature design §3.3's hysteresis walk (seed below `enter * threshold`, grow below
//   `leave * threshold`) restricted to one group, so a branching throat can never grow one chimeric
//   region across two opposite patches. A region that fails any gate falls back to `Normal`
//   (volumetric) - never to `Sheet` - which is the frozen direction of the fallback.
#[allow(clippy::too_many_arguments)]
fn segment_regions(
    samples: &[GapSample],
    groups: &[GapGroup],
    tris: &[QueryTri],
    adjacency: &[Vec<u32>],
    contact_faces: &BTreeMap<(i32, i32), BTreeSet<usize>>,
    t_sheet: f64,
    t_layer: f64,
    options: &GapFieldOptions,
    stats: &mut GapFieldStats,
) -> Vec<ThinRegion> {
    let mut regions: Vec<ThinRegion> = Vec::new();
    let value_of = |index: usize| -> f64 {
        let sample = &samples[index];
        if sample.t_exact.is_finite() {
            sample.t_exact
        } else {
            sample.t
        }
    };

    // One sample belongs to at most one region, across every group: the pairing
    // closure below pulls the opposite wall into the region that grew first, so a
    // gap yields one region owning both of its walls rather than two mirror regions.
    let mut assigned: BTreeSet<usize> = BTreeSet::new();
    let mut samples_by_tri: BTreeMap<usize, Vec<usize>> = BTreeMap::new();
    for (index, sample) in samples.iter().enumerate() {
        samples_by_tri.entry(sample.tri).or_default().push(index);
    }

    for group in groups {
        let member_set: BTreeSet<usize> = group.samples.iter().copied().collect();
        for (regime, threshold) in [(Regime::Sheet, t_sheet), (Regime::Band, t_layer)] {
            let enter = options.hysteresis_enter * threshold;
            let leave = options.hysteresis_leave * threshold;
            // the reference thin-feature design §3.3's "seeds ... (not in SHEET)" is a test on the value,
            // not only on what an earlier pass happened to claim: a band region must
            // lie inside its own regime interval, or one region spans a separation
            // range no single band template could ever mesh. The floor is applied to
            // the *frozen* measurement (`separation_of`), because that is what becomes
            // the region's `t_r`; growth connectivity still follows the smoothed field.
            let floor = if regime == Regime::Band {
                options.hysteresis_leave * t_sheet
            } else {
                0.0
            };
            for &seed in &group.samples {
                if assigned.contains(&seed)
                    || value_of(seed) >= enter
                    || separation_of(&samples[seed]) <= floor
                {
                    continue;
                }
                // Deterministic ordered BFS: the frontier is a sorted set, so the
                // grown region does not depend on adjacency insertion order.
                let mut members: BTreeSet<usize> = BTreeSet::new();
                let mut frontier: BTreeSet<usize> = BTreeSet::new();
                frontier.insert(seed);
                while let Some(&current) = frontier.iter().next() {
                    frontier.remove(&current);
                    if !members.insert(current) {
                        continue;
                    }
                    assigned.insert(current);
                    for &neighbour in &adjacency[current] {
                        let other = neighbour as usize;
                        if members.contains(&other)
                            || assigned.contains(&other)
                            || !member_set.contains(&other)
                            || value_of(other) >= leave
                            || separation_of(&samples[other]) <= floor
                        {
                            continue;
                        }
                        frontier.insert(other);
                    }
                }
                // Pairing closure: the sample each member points at joins the same
                // region, so both walls of one gap live in one region.
                let wall: Vec<usize> = members.iter().copied().collect();
                let mut closure: Vec<usize> = Vec::new();
                for &index in &wall {
                    let Some(pairing) = samples[index].pairing else {
                        continue;
                    };
                    let Some(candidates) = samples_by_tri.get(&pairing.tri) else {
                        continue;
                    };
                    let mut best: Option<(u64, usize)> = None;
                    for &other in candidates {
                        if assigned.contains(&other) || samples[other].pairing.is_none() {
                            continue;
                        }
                        // The closure may not drag a member below the regime interval
                        // either - it would become the region's `t_r` and describe a
                        // separation the region's own template cannot mesh.
                        if separation_of(&samples[other]) <= floor {
                            continue;
                        }
                        let distance = norm(samples[other].point.sub(pairing.point));
                        let key = (distance.to_bits(), other);
                        if best.map(|current| key < current).unwrap_or(true) {
                            best = Some(key);
                        }
                    }
                    if let Some((_, other)) = best {
                        assigned.insert(other);
                        closure.push(other);
                    }
                }
                closure.sort_unstable();
                closure.dedup();
                regions.push(new_region(
                    regions.len(),
                    group,
                    regime,
                    wall,
                    closure,
                    samples,
                    tris,
                ));
            }
        }
    }

    // One gap must be one region. The hysteresis walk cannot always deliver that on its
    // own: the pairing closure claims the opposite wall's samples as it goes, and a later
    // seed on the same wall then finds those claimed samples in its way, stops there and
    // becomes an island. On the band plate fixture that left the 0.020 gap as one region
    // of 212 samples plus **21 fragments of one or two**, each of which was too sparse to
    // convert and so asked the sizing field for two elements across the very gap the band
    // conversion exists to avoid - 340,360 LFS sources and a ten-fold mesh. Two fragments
    // of one gap are adjacent through exactly those claimed samples, so joining regions of
    // the same group and regime that touch is enough, and it is order-independent: the
    // union is over an undirected relation and the result is re-indexed by lowest member.
    let mut region_of_sample: BTreeMap<usize, usize> = BTreeMap::new();
    for (index, region) in regions.iter().enumerate() {
        for sample in &region.samples {
            region_of_sample.entry(*sample).or_insert(index);
        }
    }
    let mut owner: Vec<usize> = (0..regions.len()).collect();
    fn find(owner: &mut [usize], mut index: usize) -> usize {
        while owner[index] != index {
            owner[index] = owner[owner[index]];
            index = owner[index];
        }
        index
    }
    let same_group = |left: &ThinRegion, right: &ThinRegion| -> bool {
        left.component == right.component
            && left.side == right.side
            && left.opposite_patch == right.opposite_patch
            && left.pair_class == right.pair_class
            && left.declared_regime == right.declared_regime
    };
    for (index, region) in regions.iter().enumerate() {
        for sample in &region.samples {
            for &neighbour in &adjacency[*sample] {
                let Some(other) = region_of_sample.get(&(neighbour as usize)).copied() else {
                    continue;
                };
                if other == index || !same_group(region, &regions[other]) {
                    continue;
                }
                let (a, b) = (find(&mut owner, index), find(&mut owner, other));
                if a != b {
                    owner[a.max(b)] = a.min(b);
                }
            }
        }
    }
    if (0..regions.len()).any(|index| find(&mut owner, index) != index) {
        let mut merged: BTreeMap<usize, Vec<usize>> = BTreeMap::new();
        for index in 0..regions.len() {
            let root = find(&mut owner, index);
            merged.entry(root).or_default().push(index);
        }
        let mut rebuilt: Vec<ThinRegion> = Vec::new();
        for parts in merged.values() {
            let head = &regions[parts[0]];
            let mut wall: Vec<usize> = Vec::new();
            let mut closure: Vec<usize> = Vec::new();
            for part in parts {
                let region = &regions[*part];
                let opposite: BTreeSet<usize> = region.opposite_faces.iter().copied().collect();
                for sample in &region.samples {
                    if opposite.contains(&samples[*sample].tri) {
                        closure.push(*sample);
                    } else {
                        wall.push(*sample);
                    }
                }
            }
            wall.sort_unstable();
            wall.dedup();
            closure.sort_unstable();
            closure.dedup();
            rebuilt.push(new_region_from(
                rebuilt.len(),
                head.component,
                head.side,
                head.opposite_patch,
                head.pair_class,
                head.declared_regime,
                wall,
                closure,
                samples,
                tris,
            ));
        }
        regions = rebuilt;
    }

    let speck_area = options.speck_min_area_factor * options.h_bootstrap * options.h_bootstrap;
    for region in regions.iter_mut() {
        if region_reaches_contact(region, contact_faces, t_sheet) {
            region.skip = Some(SkipReason::IntersectionWedge);
        } else if region.area < speck_area {
            region.skip = Some(SkipReason::Speck);
        } else if region.samples.len() < options.speck_min_samples {
            region.skip = Some(SkipReason::Undersampled);
        } else if region.confidence + f64::EPSILON < options.confidence_min {
            region.skip = Some(SkipReason::LowConfidence);
        }
        if region.skip.is_some() {
            region.regime = Regime::Normal;
            continue;
        }
        region.rims = region_rims(&region.faces, tris);
        if region.declared_regime == Regime::Sheet {
            match build_mid_surface(region, samples, tris) {
                Some(mid) if mid.is_valid() => {
                    region.regime = Regime::Sheet;
                    region.mid_surface = Some(mid);
                }
                Some(mid) => {
                    region.skip = Some(SkipReason::MidSurfaceInvalid);
                    region.regime = Regime::Normal;
                    region.mid_surface = Some(mid);
                }
                None => {
                    region.skip = Some(SkipReason::MidSurfaceUnbuildable);
                    region.regime = Regime::Normal;
                }
            }
        } else {
            region.regime = region.declared_regime;
        }
    }

    stats.n_regions = regions.len();
    stats.n_sheet_regions = regions
        .iter()
        .filter(|region| region.regime == Regime::Sheet)
        .count();
    stats.n_band_regions = regions
        .iter()
        .filter(|region| region.regime == Regime::Band)
        .count();
    stats.n_skipped_regions = regions.iter().filter(|region| region.skip.is_some()).count();
    regions
}

// AI-FUNC-SUMMARY:
// Purpose: Whether a region is an intersection wedge - it reaches a contact between its own two
//   walls *and* its gap closes onto it.
// Inputs: the region, the per-component-pair contact faces, and the sheet threshold.
// Returns: true when the region touches the contact set and its separation crosses `t_sheet`.
// Side effects: None.
// Notes: Contact faces are the union of two exact sets, both keyed by component pair so one pair's
//   contact can never disqualify another's gap: the faces incident to that pair's S2 intersection
//   curve, and the faces whose own pairing was cleared as a geodesic shortcut. Touching that set is
//   necessary but nowhere near sufficient, and taking it alone was a defect: on a limb attached to a
//   block, every triangle of the limb is incident to the root curve, so the limb's own uniform gap
//   was declared the curve's neighbourhood and the whole sheet regime was skipped. What makes a
//   wedge a wedge is that the gap *closes*: `t_r` is the narrowest separation any member reports and
//   `t_max` the widest, so a wedge straddles `t_sheet` - one end must be collapsed and the other
//   meshed, and no single template does both. A thin feature holds `t_r` and `t_max` on one side of
//   it and is meshable by exactly one row.
fn region_reaches_contact(
    region: &ThinRegion,
    contact_faces: &BTreeMap<(i32, i32), BTreeSet<usize>>,
    t_sheet: f64,
) -> bool {
    if !(region.t_r < t_sheet && region.t_max > t_sheet) {
        return false;
    }
    let pair = match region.pair_class {
        PairClass::Intra(x) => (x, x),
        PairClass::Inter(a, b) | PairClass::SolidSheet(a, b) | PairClass::SheetSheet(a, b) => {
            if a <= b {
                (a, b)
            } else {
                (b, a)
            }
        }
        PairClass::SurfaceBox(_) | PairClass::Unpaired => return false,
    };
    let Some(faces) = contact_faces.get(&pair) else {
        return false;
    };
    region
        .faces
        .iter()
        .chain(region.opposite_faces.iter())
        .any(|face| faces.contains(face))
}

// AI-FUNC-SUMMARY:
// Purpose: Assemble one region record from its member samples (faces, area, `t_r`, confidence, checks).
// Returns: ThinRegion in the declared regime, not yet gated.
// Side effects: None.
#[allow(clippy::too_many_arguments)]
fn new_region(
    id: usize,
    group: &GapGroup,
    declared_regime: Regime,
    wall: Vec<usize>,
    closure: Vec<usize>,
    samples: &[GapSample],
    tris: &[QueryTri],
) -> ThinRegion {
    new_region_from(
        id,
        group.component,
        group.side,
        group.opposite_patch,
        group.pair_class,
        declared_regime,
        wall,
        closure,
        samples,
        tris,
    )
}

// AI-FUNC-SUMMARY:
// Purpose: Assemble one region record from its descriptor and member samples.
// Inputs: the id, the group's four identifying fields, the declared regime, the two walls' samples,
//   and the sample/triangle tables.
// Returns: ThinRegion in the declared regime, not yet gated.
// Side effects: None.
// Notes: Split out from `new_region` so a region merged from fragments can be rebuilt without a
//   `GapGroup` to hand - after segmentation the groups are gone and only the regions remain.
#[allow(clippy::too_many_arguments)]
fn new_region_from(
    id: usize,
    component: i32,
    side: i8,
    opposite_patch: i32,
    pair_class: PairClass,
    declared_regime: Regime,
    wall: Vec<usize>,
    closure: Vec<usize>,
    samples: &[GapSample],
    tris: &[QueryTri],
) -> ThinRegion {
    let mut faces: Vec<usize> = wall.iter().map(|index| samples[*index].tri).collect();
    faces.sort_unstable();
    faces.dedup();
    let mut opposite_faces: Vec<usize> = closure.iter().map(|index| samples[*index].tri).collect();
    opposite_faces.sort_unstable();
    opposite_faces.dedup();
    let mut members = wall;
    members.extend(closure);
    members.sort_unstable();
    members.dedup();
    let area: f64 = faces
        .iter()
        .map(|tri| {
            let t = &tris[*tri];
            norm(t.b.sub(t.a).cross(t.c.sub(t.a))) * 0.5
        })
        .sum();
    let mut t_r = f64::INFINITY;
    let mut t_max = 0.0f64;
    let mut passing = 0usize;
    let mut failed = [0usize; 5];
    for index in &members {
        let sample = &samples[*index];
        let value = if sample.t_exact.is_finite() {
            sample.t_exact
        } else {
            sample.t_raw
        };
        if value < t_r {
            t_r = value;
        }
        if value.is_finite() && value > t_max {
            t_max = value;
        }
        if sample.passes_battery() {
            passing += 1;
        }
        for (bit, count) in failed.iter_mut().enumerate() {
            let mask = 1u8 << bit;
            if sample.applicable & mask != 0 && sample.flags & mask == 0 {
                *count += 1;
            }
        }
    }
    let confidence = if members.is_empty() {
        0.0
    } else {
        passing as f64 / members.len() as f64
    };
    ThinRegion {
        id,
        component,
        side,
        opposite_patch,
        pair_class,
        declared_regime,
        regime: declared_regime,
        samples: members,
        faces,
        opposite_faces,
        area,
        t_r,
        t_max,
        confidence,
        failed_checks: failed,
        skip: None,
        rims: Vec::new(),
        mid_surface: None,
    }
}

// AI-FUNC-SUMMARY:
// Purpose: Chain the boundary edges of a region's face set into rim loops (the reference thin-feature design §3.3).
// Inputs: the region's triangle indices, triangle table.
// Returns: node loops, each starting at its smallest node key, sorted deterministically.
// Side effects: None.
// Notes: A boundary edge is one incident to exactly one face of the region; open chains are
//   returned as walks, closed loops repeat their first node at the end.
fn region_rims(faces: &[usize], tris: &[QueryTri]) -> Vec<Vec<usize>> {
    let mut counts: BTreeMap<(usize, usize), usize> = BTreeMap::new();
    for &face in faces {
        let tri = &tris[face];
        if tri.face.is_none() {
            continue;
        }
        for k in 0..3 {
            let a = tri.nodes[k];
            let b = tri.nodes[(k + 1) % 3];
            let key = if a < b { (a, b) } else { (b, a) };
            *counts.entry(key).or_default() += 1;
        }
    }
    let mut incident: BTreeMap<usize, BTreeSet<usize>> = BTreeMap::new();
    for (&(a, b), &count) in &counts {
        if count != 1 {
            continue;
        }
        incident.entry(a).or_default().insert(b);
        incident.entry(b).or_default().insert(a);
    }
    let mut used: BTreeSet<(usize, usize)> = BTreeSet::new();
    let mut loops: Vec<Vec<usize>> = Vec::new();
    let starts: Vec<usize> = incident.keys().copied().collect();
    for start in starts {
        while let Some(next) = incident.get(&start).and_then(|neighbours| {
            neighbours
                .iter()
                .copied()
                .find(|node| !used.contains(&edge_key(start, *node)))
        }) {
            let mut chain = vec![start];
            let mut previous = start;
            let mut current = next;
            used.insert(edge_key(previous, current));
            chain.push(current);
            while current != start {
                let Some(step) = incident.get(&current).and_then(|neighbours| {
                    neighbours
                        .iter()
                        .copied()
                        .find(|node| *node != previous && !used.contains(&edge_key(current, *node)))
                }) else {
                    break;
                };
                used.insert(edge_key(current, step));
                chain.push(step);
                previous = current;
                current = step;
            }
            loops.push(chain);
        }
    }
    loops
}

// AI-FUNC-SUMMARY: Canonical undirected edge key; returns (min, max); side effects: none.
fn edge_key(a: usize, b: usize) -> (usize, usize) {
    if a < b {
        (a, b)
    } else {
        (b, a)
    }
}

// AI-FUNC-SUMMARY:
// Purpose: Build and validate a Sheet region's mid-surface (the reference thin-feature design §3.4).
// Inputs: the gated region, samples, triangle table.
// Returns: Some(MidSurface) - valid or carrying its defects - or None when too little of the
//   region's correspondence is anchored at vertices to build one at all.
// Side effects: None.
// Notes: Wall A is the region's own wall; every midpoint is `(p + phi(p)) / 2` at a paired vertex
//   sample, and the triangulation is wall A's own, re-indexed. Inherited connectivity is not
//   trusted: positive area, orientation, adjacent-normal deviation, self-intersection, rim
//   agreement, and Euler characteristic are all re-checked.
fn build_mid_surface(
    region: &ThinRegion,
    samples: &[GapSample],
    tris: &[QueryTri],
) -> Option<MidSurface> {
    // Wall A only: the region owns both walls after the pairing closure, and the
    // mid-surface is wall A's own triangulation re-indexed onto the midpoints.
    let wall_faces: BTreeSet<usize> = region.faces.iter().copied().collect();
    let mut midpoint_of_node: BTreeMap<usize, Vec3> = BTreeMap::new();
    // Every paired sample on wall A, in region order, as (position, half-offset to the
    // opposite wall). These carry the measured gap wherever it was actually measured.
    let mut offsets: Vec<(Vec3, Vec3)> = Vec::new();
    for index in &region.samples {
        let sample = &samples[*index];
        if !wall_faces.contains(&sample.tri) {
            continue;
        }
        let Some(pairing) = sample.pairing else {
            continue;
        };
        let half = pairing.point.sub(sample.point).scale(0.5);
        offsets.push((sample.point, half));
        if let Some(node) = sample.vertex {
            midpoint_of_node.entry(node).or_insert(sample.point.add(half));
        }
    }
    // A node with no vertex sample of its own is placed by the nearest measurement on the
    // same wall. Restricting the mid-surface to vertex samples alone made it unbuildable
    // for the very shape it is meant to describe: at a box plate's corner the vertex normal
    // is the diagonal of three faces, so the ray leaves the gap instead of crossing it and
    // every corner goes unpaired - four nodes, none of them usable, and the sheet regime
    // unreachable. The mid-surface of a sheet is wall A carried half the local gap inward,
    // and that is exactly what a neighbouring sample measures.
    if !offsets.is_empty() {
        for &face in &region.faces {
            let tri = &tris[face];
            if tri.face.is_none() {
                continue;
            }
            for (corner, &node) in tri.nodes.iter().enumerate() {
                if midpoint_of_node.contains_key(&node) {
                    continue;
                }
                let position = match corner {
                    0 => tri.a,
                    1 => tri.b,
                    _ => tri.c,
                };
                // Nearest measurement, ties broken by the earlier sample: R-P2 needs the
                // choice to be a function of the data, not of the traversal.
                let mut best: Option<(u64, Vec3)> = None;
                for (point, half) in &offsets {
                    let key = norm(point.sub(position)).to_bits();
                    if best.map(|(current, _)| key < current).unwrap_or(true) {
                        best = Some((key, *half));
                    }
                }
                if let Some((_, half)) = best {
                    midpoint_of_node.insert(node, position.add(half));
                }
            }
        }
    }
    if midpoint_of_node.len() < 3 {
        return None;
    }
    let index_of_node: BTreeMap<usize, usize> = midpoint_of_node
        .keys()
        .enumerate()
        .map(|(index, node)| (*node, index))
        .collect();
    let points: Vec<Vec3> = midpoint_of_node.values().copied().collect();
    let source_nodes: Vec<usize> = midpoint_of_node.keys().copied().collect();

    let mut triangles: Vec<[usize; 3]> = Vec::new();
    let mut source_faces: Vec<usize> = Vec::new();
    for &face in &region.faces {
        let tri = &tris[face];
        if tri.face.is_none() {
            continue;
        }
        let mapped: Vec<usize> = tri
            .nodes
            .iter()
            .filter_map(|node| index_of_node.get(node).copied())
            .collect();
        if mapped.len() != 3 {
            continue;
        }
        triangles.push([mapped[0], mapped[1], mapped[2]]);
        source_faces.push(face);
    }
    if triangles.is_empty() {
        return None;
    }

    let source_normals: Vec<Vec3> = source_faces
        .iter()
        .map(|face| {
            let tri = &tris[*face];
            tri.b.sub(tri.a).cross(tri.c.sub(tri.a))
        })
        .collect();
    let rim_edge_count: usize = region
        .rims
        .iter()
        .map(|rim| rim.len().saturating_sub(1))
        .sum();
    let mut wall_edges: BTreeSet<(usize, usize)> = BTreeSet::new();
    let mut wall_nodes: BTreeSet<usize> = BTreeSet::new();
    for &face in &source_faces {
        let tri = &tris[face];
        for k in 0..3 {
            wall_nodes.insert(tri.nodes[k]);
            wall_edges.insert(edge_key(tri.nodes[k], tri.nodes[(k + 1) % 3]));
        }
    }
    let wall_euler = wall_nodes.len() as i64 - wall_edges.len() as i64 + source_faces.len() as i64;

    let mut mid = MidSurface {
        points,
        source_nodes,
        triangles,
        defects: Vec::new(),
    };
    validate_mid_surface(&mut mid, &source_normals, rim_edge_count, wall_euler);
    Some(mid)
}

// AI-FUNC-SUMMARY:
// Purpose: Run the reference thin-feature design §3.4's checks over a mid-surface candidate and record every defect found.
// Inputs: the candidate (mutated), the source wall normal per triangle, the region's rim edge count,
//   and the source wall patch's Euler characteristic.
// Returns: None.
// Side effects: Sets `mid.defects` (sorted, deduplicated).
// Notes: Inherited connectivity is not trusted - positive area, orientation against the source wall,
//   adjacent-normal deviation, self-intersection, rim agreement, and Euler characteristic are all
//   re-checked. Angle gates use `cos^2` on squared dot products, never `acos`
//   (SPEC_meshgen_numerics §8.1).
pub fn validate_mid_surface(
    mid: &mut MidSurface,
    source_normals: &[Vec3],
    rim_edge_count: usize,
    wall_euler: i64,
) {
    let mut defects: BTreeSet<MidSurfaceDefect> = BTreeSet::new();
    let normals: Vec<Vec3> = mid
        .triangles
        .iter()
        .map(|triangle| {
            let a = mid.points[triangle[0]];
            let b = mid.points[triangle[1]];
            let c = mid.points[triangle[2]];
            b.sub(a).cross(c.sub(a))
        })
        .collect();

    for (index, normal) in normals.iter().enumerate() {
        if norm(*normal) <= 0.0 {
            defects.insert(MidSurfaceDefect::DegenerateTriangle);
            continue;
        }
        if let Some(source_normal) = source_normals.get(index) {
            if source_normal.dot(*normal) <= 0.0 {
                defects.insert(MidSurfaceDefect::OrientationReversed);
            }
        }
    }

    // Adjacent-triangle normal deviation < 60 degrees, by cos^2 comparison.
    let cos2_limit = 0.25;
    let mut edge_faces: BTreeMap<(usize, usize), Vec<usize>> = BTreeMap::new();
    for (index, triangle) in mid.triangles.iter().enumerate() {
        for k in 0..3 {
            edge_faces
                .entry(edge_key(triangle[k], triangle[(k + 1) % 3]))
                .or_default()
                .push(index);
        }
    }
    for incident in edge_faces.values() {
        if incident.len() != 2 {
            continue;
        }
        let (left, right) = (normals[incident[0]], normals[incident[1]]);
        let dot = left.dot(right);
        let denominator = left.dot(left) * right.dot(right);
        if denominator <= 0.0 {
            continue;
        }
        if dot <= 0.0 || (dot * dot) / denominator < cos2_limit {
            defects.insert(MidSurfaceDefect::NormalDeviation);
        }
    }

    if mid_surface_self_intersects(mid) {
        defects.insert(MidSurfaceDefect::SelfIntersection);
    }

    // Boundary edges must reproduce the region's rim loops, and the patch must keep
    // wall A's Euler characteristic - inherited connectivity is not trusted.
    let boundary_edges = edge_faces.values().filter(|faces| faces.len() == 1).count();
    if boundary_edges != rim_edge_count {
        defects.insert(MidSurfaceDefect::BoundaryMismatch);
    }
    let mid_euler = mid.points.len() as i64 - edge_faces.len() as i64 + mid.triangles.len() as i64;
    if mid_euler != wall_euler {
        defects.insert(MidSurfaceDefect::EulerMismatch);
    }

    mid.defects = defects.into_iter().collect();
}

// AI-FUNC-SUMMARY:
// Purpose: Detect any proper crossing among a mid-surface's own triangles.
// Returns: true when an edge of one triangle passes through the interior of another.
// Side effects: None.
// Notes: Pairs sharing a point index are skipped - they meet by construction. The test is the
//   sign pattern of `orient3d`, so a coplanar touch is not reported as a crossing.
fn mid_surface_self_intersects(mid: &MidSurface) -> bool {
    for (i, left) in mid.triangles.iter().enumerate() {
        for right in mid.triangles.iter().skip(i + 1) {
            if left.iter().any(|node| right.contains(node)) {
                continue;
            }
            for (source, target) in [(left, right), (right, left)] {
                let t = [
                    mid.points[target[0]],
                    mid.points[target[1]],
                    mid.points[target[2]],
                ];
                for k in 0..3 {
                    let p = mid.points[source[k]];
                    let q = mid.points[source[(k + 1) % 3]];
                    if segment_crosses_triangle(p, q, t) {
                        return true;
                    }
                }
            }
        }
    }
    false
}

// AI-FUNC-SUMMARY: Exact proper crossing of a segment through a triangle's interior; returns bool; side effects: none.
fn segment_crosses_triangle(p: Vec3, q: Vec3, tri: [Vec3; 3]) -> bool {
    let sign = |value: f64| -> i8 {
        if value > 0.0 {
            1
        } else if value < 0.0 {
            -1
        } else {
            0
        }
    };
    let side_p = sign(orient3d(tri[0], tri[1], tri[2], p));
    let side_q = sign(orient3d(tri[0], tri[1], tri[2], q));
    if side_p == 0 || side_q == 0 || side_p == side_q {
        return false;
    }
    let a = sign(orient3d(p, q, tri[0], tri[1]));
    let b = sign(orient3d(p, q, tri[1], tri[2]));
    let c = sign(orient3d(p, q, tri[2], tri[0]));
    a != 0 && a == b && b == c
}

// AI-FUNC-SUMMARY:
// Purpose: Build the `s03_gapfield` snapshot document: the arranged surface plus the `separation_t` point field.
// Inputs: arranged surface, computed gap field.
// Returns: VtuDoc ready for `emit_snapshot` (metadata is stamped there).
// Side effects: None.
// Notes: `separation_t` carries, per arranged vertex, the smallest finite separation of any sample
//   anchored at that vertex or on an incident face; `-1` marks "no pairing here" (the schema's
//   not-applicable convention for a quantity that is otherwise non-negative).
pub fn gapfield_to_doc(surface: &ArrangedSurface, field: &GapField) -> VtuDoc {
    let mut doc = arranged_surface_to_doc(surface);
    let mut separation = vec![f64::INFINITY; surface.vertices.len()];
    for sample in &field.samples {
        let value = if sample.t_exact.is_finite() {
            sample.t_exact
        } else {
            sample.t
        };
        if !value.is_finite() {
            continue;
        }
        if let Some(node) = sample.vertex {
            if value < separation[node] {
                separation[node] = value;
            }
            continue;
        }
        if let Some(face) = surface.faces.get(sample.tri) {
            for node in face.nodes {
                if value < separation[node] {
                    separation[node] = value;
                }
            }
        }
    }
    let data: Vec<f32> = separation
        .iter()
        .map(|value| if value.is_finite() { *value as f32 } else { -1.0 })
        .collect();
    doc.point_data
        .push(DataArray::scalar("separation_t", ArrayData::F32(data)));

    append_thin_regions(&mut doc, surface, field);
    doc
}

// AI-FUNC-SUMMARY:
// Purpose: Reduce S3's thin regions to the per-arranged-face lookups S8b needs (G7-2).
// Inputs: the gap field, the *effective* regimes - what survives the S3/S4 coupling and §10.11's
//   ladder, which is not always `ThinRegion::regime` - and `meshgen.thin.collapse_sheets`.
// Returns: a `ThinContext` indexed by arranged face.
// Side effects: None.
// Notes: **Both** walls are mapped. `gapfield_to_doc`'s own attribution walks `region.faces` only,
//   because on s03 a face is a wall of the region it grew from; S8b needs the opposite wall too,
//   since a band cell resolves its region from whichever of its two lids it reaches first and the
//   two must agree. Ties are broken exactly as s03 breaks them - a converted region wins over an
//   unconverted one, then the lowest id - so a face never reports two different regions in one run.
pub fn thin_context(
    field: &GapField,
    effective_regimes: &[Regime],
    collapse_sheets: bool,
) -> ThinContext {
    let face_count = field.face_of_tri.len();
    let mut region_of_face: Vec<i32> = vec![-1; face_count];
    let mut converted_face: Vec<bool> = vec![false; face_count];
    for region in &field.regions {
        let regime = effective_regimes
            .get(region.id)
            .copied()
            .unwrap_or(region.regime);
        let converted = regime != Regime::Normal;
        for &tri in region.faces.iter().chain(region.opposite_faces.iter()) {
            let Some(&face) = field.face_of_tri.get(tri) else {
                continue;
            };
            if face < 0 {
                continue;
            }
            let face = face as usize;
            if face >= face_count {
                continue;
            }
            if region_of_face[face] < 0 || (converted && !converted_face[face]) {
                region_of_face[face] = region.id as i32;
                converted_face[face] = converted;
            }
        }
    }
    let regime = field
        .regions
        .iter()
        .map(|region| {
            match effective_regimes
                .get(region.id)
                .copied()
                .unwrap_or(region.regime)
            {
                Regime::Normal => ThinRegime::Normal,
                Regime::Band => ThinRegime::Band,
                Regime::Sheet => ThinRegime::Sheet,
            }
        })
        .collect();
    let pair_class = field
        .regions
        .iter()
        .map(|region| pair_class_code(region.pair_class))
        .collect();
    ThinContext {
        region_of_face,
        regime,
        pair_class,
        t_sheet: field.t_sheet,
        collapse_sheets,
    }
}

// AI-FUNC-SUMMARY: The SPEC_meshgen_contracts §2.3 `ThinRegionPairClass` code of a pair class; returns u8; side effects: none.
pub fn pair_class_code(class: PairClass) -> u8 {
    match class {
        PairClass::Unpaired => 0,
        PairClass::Intra(_) => 1,
        PairClass::Inter(_, _) => 2,
        PairClass::SolidSheet(_, _) => 3,
        PairClass::SheetSheet(_, _) => 4,
        PairClass::SurfaceBox(_) => 5,
    }
}

/// `thin_role` values: an ordinary wall face, a wall face inside a converted region,
/// and a mid-surface face. `255` is the not-applicable sentinel (curve cells).
const THIN_ROLE_WALL: u8 = 0;
const THIN_ROLE_REGION_WALL: u8 = 1;
const THIN_ROLE_MID_SURFACE: u8 = 2;
const THIN_ROLE_NA: u8 = 255;

// AI-FUNC-SUMMARY:
// Purpose: Add the G3-2 region attribution, the thin-region tables, and the mid-surface faces to an s03 document.
// Inputs: the document built so far, arranged surface, gap field.
// Returns: None.
// Side effects: Appends cell arrays (`band_region`, `thin_role`), field tables (`ThinRegion*`),
//   mid-surface points/cells, and extends every always-present per-point and per-cell array.
// Notes: Mid-surface triangles are emitted as ordinary tagged face cells carrying their region's
//   component with `FaceTagKind = 1` (sheet) - a mid-surface is exactly the sheet that region will
//   collapse to - and are told apart from wall faces by `thin_role = 2`. `band_region` is the
//   schema's Dbg thin-region id; `thin_role` and the `ThinRegion*` tables are additive schema-v1
//   diagnostics recorded in SPEC_meshgen_contracts §2.1/§2.3.
fn append_thin_regions(doc: &mut VtuDoc, surface: &ArrangedSurface, field: &GapField) {
    let face_count = surface.faces.len();
    let curve_count = surface.curves.len();
    let original_cells = face_count + curve_count;

    // A face may belong to several regions; the converted one wins, then the lowest id.
    let mut region_of_face: Vec<i32> = vec![-1; face_count];
    let mut converted_face: Vec<bool> = vec![false; face_count];
    for region in &field.regions {
        let converted = region.regime != Regime::Normal;
        for &tri in &region.faces {
            let Some(&face) = field.face_of_tri.get(tri) else {
                continue;
            };
            if face < 0 {
                continue;
            }
            let face = face as usize;
            if region_of_face[face] < 0 || (converted && !converted_face[face]) {
                region_of_face[face] = region.id as i32;
                converted_face[face] = converted;
            }
        }
    }

    let mut band_region: Vec<i32> = Vec::with_capacity(original_cells);
    let mut thin_role: Vec<u8> = Vec::with_capacity(original_cells);
    for face in 0..face_count {
        band_region.push(region_of_face[face]);
        thin_role.push(if converted_face[face] {
            THIN_ROLE_REGION_WALL
        } else {
            THIN_ROLE_WALL
        });
    }
    for _ in 0..curve_count {
        band_region.push(-1);
        thin_role.push(THIN_ROLE_NA);
    }

    // Mid-surface geometry, appended after the existing cells.
    let mut n_id_key: Vec<i32> = read_i32_point_array(doc, "n_id_key");
    let mut constraint_kind: Vec<u8> = read_u8_point_array(doc, "constraint_kind");
    let mut constraint_ref: Vec<i32> = read_i32_point_array(doc, "constraint_ref");
    let mut separation_extra: Vec<f32> = Vec::new();
    let mut new_faces = 0usize;
    for region in &field.regions {
        let Some(mid) = region.mid_surface.as_ref().filter(|mid| mid.is_valid()) else {
            continue;
        };
        if region.regime != Regime::Sheet {
            continue;
        }
        let base = doc.points.len();
        for (index, point) in mid.points.iter().enumerate() {
            doc.points.push(*point);
            let source = mid.source_nodes[index];
            n_id_key.push(n_id_key.get(source).copied().unwrap_or(0));
            constraint_kind.push(constraint_kind.get(source).copied().unwrap_or(1));
            constraint_ref.push(constraint_ref.get(source).copied().unwrap_or(region.component));
            separation_extra.push(region.t_r as f32);
        }
        for triangle in &mid.triangles {
            doc.connectivity
                .extend(triangle.iter().map(|node| (base + node) as i64));
            doc.offsets.push(doc.connectivity.len() as i64);
            doc.types.push(VTK_TRIANGLE);
            band_region.push(region.id as i32);
            thin_role.push(THIN_ROLE_MID_SURFACE);
            new_faces += 1;
        }
    }

    if new_faces > 0 {
        set_i32_point_array(doc, "n_id_key", n_id_key);
        set_u8_point_array(doc, "constraint_kind", constraint_kind);
        set_i32_point_array(doc, "constraint_ref", constraint_ref);
        if let Some(array) = doc
            .point_data
            .iter_mut()
            .find(|array| array.name == "separation_t")
        {
            if let ArrayData::F32(values) = &mut array.data {
                values.extend(separation_extra);
            }
        }
        extend_cell_arrays(doc, new_faces, &field.regions, face_count);
    }

    doc.cell_data
        .push(DataArray::scalar("band_region", ArrayData::I32(band_region)));
    doc.cell_data
        .push(DataArray::scalar("thin_role", ArrayData::U8(thin_role)));

    let mut regimes: Vec<u8> = Vec::with_capacity(field.regions.len());
    let mut confidences: Vec<f32> = Vec::with_capacity(field.regions.len());
    let mut separations: Vec<f64> = Vec::with_capacity(field.regions.len());
    let mut skips: Vec<u8> = Vec::with_capacity(field.regions.len());
    for region in &field.regions {
        regimes.push(match region.regime {
            Regime::Normal => 0,
            Regime::Band => 1,
            Regime::Sheet => 2,
        });
        confidences.push(region.confidence as f32);
        separations.push(if region.t_r.is_finite() {
            region.t_r
        } else {
            -1.0
        });
        skips.push(match region.skip {
            None => 0,
            Some(SkipReason::LowConfidence) => 1,
            Some(SkipReason::Speck) => 2,
            Some(SkipReason::MidSurfaceInvalid) => 3,
            Some(SkipReason::MidSurfaceUnbuildable) => 4,
            Some(SkipReason::IntersectionWedge) => 5,
            Some(SkipReason::Undersampled) => 6,
        });
    }
    let pair_classes: Vec<u8> = field
        .regions
        .iter()
        .map(|region| pair_class_code(region.pair_class))
        .collect();
    push_gap_field(doc, "ThinRegionRegime", ArrayData::U8(regimes));
    push_gap_field(doc, "ThinRegionPairClass", ArrayData::U8(pair_classes));
    push_gap_field(doc, "ThinRegionConfidence", ArrayData::F32(confidences));
    push_gap_field(doc, "ThinRegionSeparation", ArrayData::F64(separations));
    push_gap_field(doc, "ThinRegionSkip", ArrayData::U8(skips));
}

// AI-FUNC-SUMMARY: Extend every always-present cell array for appended mid-surface faces; side effects: mutates doc.cell_data.
fn extend_cell_arrays(
    doc: &mut VtuDoc,
    new_faces: usize,
    regions: &[ThinRegion],
    original_face_count: usize,
) {
    // Mid-surface faces carry their region component's tag set, so the face-tag
    // tables gain a singleton entry per component that does not already have one.
    let mut sheet_components: Vec<i32> = regions
        .iter()
        .filter(|region| {
            region.regime == Regime::Sheet
                && region
                    .mid_surface
                    .as_ref()
                    .map(|mid| mid.is_valid())
                    .unwrap_or(false)
        })
        .map(|region| region.component)
        .collect();
    sheet_components.sort_unstable();
    sheet_components.dedup();

    let mut offsets = read_i64_field(doc, "FaceTagOffsets");
    let mut components = read_i32_field(doc, "FaceTagComponents");
    let mut orientations = read_i32_field(doc, "FaceTagOrientation");
    let mut kinds = read_u8_field(doc, "FaceTagKind");
    let mut key_of_component: BTreeMap<i32, i32> = BTreeMap::new();
    for component in &sheet_components {
        let mut found = None;
        let mut start = 0usize;
        for (index, end) in offsets.iter().enumerate() {
            let end = *end as usize;
            if end == start + 1 && components[start] == *component && kinds[index] == 1 {
                found = Some(index as i32);
                break;
            }
            start = end;
        }
        let key = match found {
            Some(key) => key,
            None => {
                components.push(*component);
                orientations.push(1);
                offsets.push(components.len() as i64);
                kinds.push(1);
                (offsets.len() - 1) as i32
            }
        };
        key_of_component.insert(*component, key);
    }
    set_i64_field(doc, "FaceTagOffsets", offsets);
    set_i32_field(doc, "FaceTagComponents", components);
    set_i32_field(doc, "FaceTagOrientation", orientations);
    set_u8_field(doc, "FaceTagKind", kinds);

    let mut mid_keys: Vec<i32> = Vec::with_capacity(new_faces);
    for region in regions {
        let Some(mid) = region.mid_surface.as_ref().filter(|mid| mid.is_valid()) else {
            continue;
        };
        if region.regime != Regime::Sheet {
            continue;
        }
        let key = key_of_component.get(&region.component).copied().unwrap_or(-1);
        for _ in &mid.triangles {
            mid_keys.push(key);
        }
    }

    for array in doc.cell_data.iter_mut() {
        match (array.name.as_str(), &mut array.data) {
            ("cell_kind", ArrayData::U8(values)) => values.extend(std::iter::repeat_n(1, new_faces)),
            ("regime", ArrayData::U8(values)) => {
                values.extend(std::iter::repeat_n(255, new_faces))
            }
            ("region_key", ArrayData::I32(values)) | ("partition_id", ArrayData::I32(values)) => {
                values.extend(std::iter::repeat_n(-1, new_faces))
            }
            ("curve_id", ArrayData::I32(values)) => {
                values.extend(std::iter::repeat_n(-1, new_faces))
            }
            ("face_tag_key", ArrayData::I32(values)) => values.extend(mid_keys.iter().copied()),
            _ => {}
        }
    }
    let _ = original_face_count;
    if let Some(array) = doc
        .field_data
        .iter_mut()
        .find(|array| array.name == "FaceTagSideElems")
    {
        if let ArrayData::I32(values) = &mut array.data {
            values.extend(std::iter::repeat_n(-1, new_faces * 2));
        }
    }
}

// AI-FUNC-SUMMARY: Append one field-data array to a gap-field document; side effects: mutates doc.field_data.
fn push_gap_field(doc: &mut VtuDoc, name: &str, data: ArrayData) {
    doc.field_data.push(DataArray {
        name: name.to_string(),
        components: 1,
        data,
    });
}

// AI-FUNC-SUMMARY: Read an Int32 point array into a Vec (empty when absent); returns Vec<i32>; side effects: none.
fn read_i32_point_array(doc: &VtuDoc, name: &str) -> Vec<i32> {
    match doc.point_array(name).map(|array| &array.data) {
        Some(ArrayData::I32(values)) => values.clone(),
        _ => Vec::new(),
    }
}

// AI-FUNC-SUMMARY: Read a UInt8 point array into a Vec (empty when absent); returns Vec<u8>; side effects: none.
fn read_u8_point_array(doc: &VtuDoc, name: &str) -> Vec<u8> {
    match doc.point_array(name).map(|array| &array.data) {
        Some(ArrayData::U8(values)) => values.clone(),
        _ => Vec::new(),
    }
}

// AI-FUNC-SUMMARY: Replace an Int32 point array's contents; side effects: mutates doc.point_data.
fn set_i32_point_array(doc: &mut VtuDoc, name: &str, values: Vec<i32>) {
    if let Some(array) = doc.point_data.iter_mut().find(|array| array.name == name) {
        array.data = ArrayData::I32(values);
    }
}

// AI-FUNC-SUMMARY: Replace a UInt8 point array's contents; side effects: mutates doc.point_data.
fn set_u8_point_array(doc: &mut VtuDoc, name: &str, values: Vec<u8>) {
    if let Some(array) = doc.point_data.iter_mut().find(|array| array.name == name) {
        array.data = ArrayData::U8(values);
    }
}

// AI-FUNC-SUMMARY: Read an Int64 field array into a Vec (empty when absent); returns Vec<i64>; side effects: none.
fn read_i64_field(doc: &VtuDoc, name: &str) -> Vec<i64> {
    match doc.field_array(name).map(|array| &array.data) {
        Some(ArrayData::I64(values)) => values.clone(),
        _ => Vec::new(),
    }
}

// AI-FUNC-SUMMARY: Read an Int32 field array into a Vec (empty when absent); returns Vec<i32>; side effects: none.
fn read_i32_field(doc: &VtuDoc, name: &str) -> Vec<i32> {
    match doc.field_array(name).map(|array| &array.data) {
        Some(ArrayData::I32(values)) => values.clone(),
        _ => Vec::new(),
    }
}

// AI-FUNC-SUMMARY: Read a UInt8 field array into a Vec (empty when absent); returns Vec<u8>; side effects: none.
fn read_u8_field(doc: &VtuDoc, name: &str) -> Vec<u8> {
    match doc.field_array(name).map(|array| &array.data) {
        Some(ArrayData::U8(values)) => values.clone(),
        _ => Vec::new(),
    }
}

// AI-FUNC-SUMMARY: Replace an Int64 field array's contents; side effects: mutates doc.field_data.
fn set_i64_field(doc: &mut VtuDoc, name: &str, values: Vec<i64>) {
    if let Some(array) = doc.field_data.iter_mut().find(|array| array.name == name) {
        array.data = ArrayData::I64(values);
    }
}

// AI-FUNC-SUMMARY: Replace an Int32 field array's contents; side effects: mutates doc.field_data.
fn set_i32_field(doc: &mut VtuDoc, name: &str, values: Vec<i32>) {
    if let Some(array) = doc.field_data.iter_mut().find(|array| array.name == name) {
        array.data = ArrayData::I32(values);
    }
}

// AI-FUNC-SUMMARY: Replace a UInt8 field array's contents; side effects: mutates doc.field_data.
fn set_u8_field(doc: &mut VtuDoc, name: &str, values: Vec<u8>) {
    if let Some(array) = doc.field_data.iter_mut().find(|array| array.name == name) {
        array.data = ArrayData::U8(values);
    }
}
