//! S2/G2-1 through G2-3 CPU corefinement reference implementation.
//!
//! This slice owns exact narrow phase, coplanar overlay and coincidence policy,
//! calibrated near-coincidence handling, canonical registry identity, constrained
//! triangulation, intersection features, provisional radial incidence, and typed
//! degraded neighborhoods. Topology rebuild/GWN, box clipping, and the production
//! broad phase remain assigned to G2-4 and G2-5.

use crate::config::meshgen::CoincidencePolicy;
use crate::error::{Result, RustMsptError};
use crate::io::vtu::{ArrayData, DataArray, VtuDoc, VTK_POLY_LINE, VTK_TRIANGLE};
use crate::meshgen::features::{FeatureEdgeKind, FeatureSet};
use crate::meshgen::predicates::{
    best_projection_axis, construct_coplanar_segment_intersection,
    construct_edge_triangle_intersection, construct_three_triangle_intersection, node_key,
    orient2d_axis, orient3d_filtered, project_to_2d, ConstructionOutcome, DeterminantRatio,
    PrecisionTier, ProjectionAxis,
};
use crate::meshgen::surface::{source_component_is_closed, ConditionedSurface, SurfaceComponent};
use crate::types::Vec3;
use rayon::prelude::*;
use smallvec::SmallVec;
use spade::{ConstrainedDelaunayTriangulation, HasPosition, Point2, Triangulation};
use std::collections::{BTreeMap, BTreeSet};

pub type TriId = u32;

// AI-FUNC-SUMMARY: Canonical post-weld source edge id using stable NodeKey-ordered vertex ids; side effects: none.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct EdgeId(pub u32, pub u32);

impl EdgeId {
    // AI-FUNC-SUMMARY: Build a sorted edge id; returns EdgeId; side effects: none.
    pub fn new(a: u32, b: u32) -> Self {
        if a <= b {
            Self(a, b)
        } else {
            Self(b, a)
        }
    }
}

// AI-FUNC-SUMMARY: Frozen symbolic provenance for one registry-owned intersection vertex; side effects: none.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum IsectProv {
    EdgeTri { edge: EdgeId, triangle: TriId },
    EdgeEdge { first: EdgeId, second: EdgeId },
    TriTriTri([TriId; 3]),
}

// AI-FUNC-SUMMARY: Frozen canonical key for one globally deduplicated intersection segment; side effects: none.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct SegKey {
    pub va: u32,
    pub vb: u32,
    pub ta: TriId,
    pub tb: TriId,
}

impl SegKey {
    // AI-FUNC-SUMMARY: Build a segment key with both endpoint and triangle pairs sorted; returns SegKey; side effects: none.
    pub fn new(va: u32, vb: u32, ta: TriId, tb: TriId) -> Self {
        let (va, vb) = if va <= vb { (va, vb) } else { (vb, va) };
        let (ta, tb) = if ta <= tb { (ta, tb) } else { (tb, ta) };
        Self { va, vb, ta, tb }
    }
}

// AI-FUNC-SUMMARY: Public arrangement name for the shared surface-stage component metadata row; side effects: none.
pub type ArrangeComponent = SurfaceComponent;

// AI-FUNC-SUMMARY: Frozen S2 coincidence cases C1 through C10; side effects: none.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum CoincidenceCase {
    C1,
    C2,
    C3,
    C4,
    C5,
    C6,
    C7,
    C8,
    C9,
    C10,
}

impl CoincidenceCase {
    // AI-FUNC-SUMMARY: Apply the frozen reject-column policy to one coincidence case; returns true only for C1/C2/C3/C7/C8/C9 in reject mode; side effects: none.
    pub fn rejected_by(self, policy: CoincidencePolicy) -> bool {
        policy == CoincidencePolicy::Reject
            && matches!(
                self,
                Self::C1 | Self::C2 | Self::C3 | Self::C7 | Self::C8 | Self::C9
            )
    }

    // AI-FUNC-SUMMARY: Determine whether one classified case produces a warning record; returns true in warn mode; side effects: none.
    pub fn warned_by(self, policy: CoincidencePolicy) -> bool {
        let _ = self;
        policy == CoincidencePolicy::Warn
    }
}

// AI-FUNC-SUMMARY: Canonically ordered source or virtual-domain entity participating in a coincidence event; side effects: none.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum CoincidenceEntity {
    Triangle(TriId),
    DomainFace(u8),
}

// AI-FUNC-SUMMARY: Deterministic typed record for one frozen coincidence-table classification; side effects: none.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct CoincidenceEvent {
    pub case: CoincidenceCase,
    pub entities: [CoincidenceEntity; 2],
    pub components: SmallVec<[i32; 2]>,
}

// AI-FUNC-SUMMARY: Typed reason that an arrangement neighborhood must retain the S7 alternating-projection fallback; side effects: none.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum DegradedReason {
    PrecisionFloor,
    QuantizedOrderAmbiguity,
    CollapsedContactSegment,
    ResidualCrossing,
    RadiallyCoplanar,
}

// AI-FUNC-SUMMARY: Public deterministic degraded-neighborhood record retaining source triangles, symbolic points, target geometry, and optional stability ratio for S7; side effects: none.
#[derive(Clone, Debug, PartialEq)]
pub struct DegradedNeighborhood {
    pub reason: DegradedReason,
    pub triangles: SmallVec<[TriId; 3]>,
    pub provenances: Vec<IsectProv>,
    pub points: Vec<Vec3>,
    pub rho: Option<f64>,
}

// AI-FUNC-SUMMARY: Typed C5/C6 point feature welded into the arranged node complex; side effects: none.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct ArrangedPointFeature {
    pub case: CoincidenceCase,
    pub node: usize,
    pub triangles: SmallVec<[TriId; 2]>,
    pub components: SmallVec<[i32; 2]>,
}

// AI-FUNC-SUMMARY: G2-1 through G2-3 options derived from the domain, envelope, coincidence policy, and input component table; side effects: none.
#[derive(Clone, Debug)]
pub struct ArrangeOptions {
    pub domain_min: Vec3,
    pub domain_max: Vec3,
    pub eps: f64,
    pub coincidence: CoincidencePolicy,
    pub components: Vec<ArrangeComponent>,
}

impl ArrangeOptions {
    // AI-FUNC-SUMMARY: Build arrangement options with the frozen default merge policy; returns ArrangeOptions; side effects: none.
    pub fn new(
        domain_min: Vec3,
        domain_max: Vec3,
        eps: f64,
        components: Vec<ArrangeComponent>,
    ) -> Self {
        Self {
            domain_min,
            domain_max,
            eps,
            coincidence: CoincidencePolicy::default(),
            components,
        }
    }

    // AI-FUNC-SUMMARY: Replace the coincidence policy on arrangement options; returns updated ArrangeOptions; side effects: none.
    pub fn with_coincidence(mut self, coincidence: CoincidencePolicy) -> Self {
        self.coincidence = coincidence;
        self
    }
}

// AI-FUNC-SUMMARY: One canonical registry vertex with committed output-node identity and every symbolically collected provenance alias; side effects: none.
#[derive(Clone, Debug, PartialEq)]
pub struct RegistryVertex {
    pub id: u32,
    pub provenance: IsectProv,
    pub aliases: Vec<IsectProv>,
    pub point: Vec3,
    pub node: usize,
    pub tier: PrecisionTier,
    pub rho: f64,
}

// AI-FUNC-SUMMARY: One canonical registry segment shared by both incident source triangles; side effects: none.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RegistrySegment {
    pub key: SegKey,
    pub nodes: [usize; 2],
    pub triangles: [TriId; 2],
    pub components: [i32; 2],
}

// AI-FUNC-SUMMARY: Deterministic global intersection registry emitted after collect-then-sort id assignment; side effects: none.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct IntersectionRegistry {
    pub vertices: Vec<RegistryVertex>,
    pub segments: Vec<RegistrySegment>,
}

// AI-FUNC-SUMMARY: Arranged curve kind matching contract CurveKind values; side effects: none.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum ArrangedCurveKind {
    Sharp = 0,
    Rim = 1,
    Intersection = 2,
    Box = 3,
}

// AI-FUNC-SUMMARY: One feature/intersection curve with component incidence and cyclic arranged-face order around the curve; side effects: none.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ArrangedCurve {
    pub kind: ArrangedCurveKind,
    pub components: SmallVec<[i32; 2]>,
    pub nodes: Vec<usize>,
    pub radial_patches: Vec<TriId>,
}

// AI-FUNC-SUMMARY: One canonical atomic child face retaining every source triangle, source orientation, component tag, per-tag orientation, and box-cap marker; side effects: none.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ArrangedFace {
    pub nodes: [usize; 3],
    pub source_triangle: TriId,
    pub component: i32,
    pub source_triangles: SmallVec<[TriId; 2]>,
    pub source_orientations: SmallVec<[i8; 2]>,
    pub components: SmallVec<[i32; 2]>,
    pub tag_orientations: SmallVec<[i8; 2]>,
    pub box_tagged: bool,
}

// AI-FUNC-SUMMARY: G2-1 through G2-3 diagnostic counters for exact intersections, overlays, contacts, coincidence events, and explicit degraded routes; side effects: none.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ArrangementStats {
    pub candidate_pairs: usize,
    pub proper_intersections: usize,
    pub contact_degraded: usize,
    pub precision_escalations: usize,
    pub precision_floor_routes: usize,
    pub triple_points: usize,
    pub split_faces: usize,
    pub coplanar_overlays: usize,
    pub point_features: usize,
    pub coincidence_events: usize,
    pub degraded_neighborhoods: usize,
}

// AI-FUNC-SUMMARY: G2-1 through G2-3 arranged surface with atomic multi-tag faces, curves, point features, sorted coincidence/warning/degraded records, registry, components, and diagnostics; side effects: none.
#[derive(Clone, Debug, PartialEq)]
pub struct ArrangedSurface {
    pub vertices: Vec<Vec3>,
    pub faces: Vec<ArrangedFace>,
    pub curves: Vec<ArrangedCurve>,
    pub corner_nodes: BTreeSet<usize>,
    pub point_features: Vec<ArrangedPointFeature>,
    pub registry: IntersectionRegistry,
    pub components: Vec<ArrangeComponent>,
    pub coincidence_events: Vec<CoincidenceEvent>,
    pub warnings: Vec<CoincidenceEvent>,
    pub degraded: Vec<DegradedNeighborhood>,
    pub stats: ArrangementStats,
}

#[derive(Clone, Copy, Debug)]
struct Normalization {
    origin: Vec3,
    scale: f64,
}

impl Normalization {
    // AI-FUNC-SUMMARY: Build the mandatory domain-diagonal normalization; returns Normalization or InvalidConfig; side effects: none.
    fn new(min: Vec3, max: Vec3) -> Result<Self> {
        let size = max.sub(min);
        let scale = size.dot(size).sqrt();
        if !scale.is_finite() || scale <= 0.0 {
            return Err(RustMsptError::InvalidConfig(
                "S2 normalization requires a finite positive domain diagonal".to_string(),
            ));
        }
        Ok(Self { origin: min, scale })
    }

    // AI-FUNC-SUMMARY: Map output coordinates into the normalized numerical frame; returns Vec3; side effects: none.
    fn normalize(self, p: Vec3) -> Vec3 {
        p.sub(self.origin).scale(1.0 / self.scale)
    }

    // AI-FUNC-SUMMARY: Map normalized coordinates back to output coordinates; returns Vec3; side effects: none.
    fn denormalize(self, p: Vec3) -> Vec3 {
        self.origin.add(p.scale(self.scale))
    }
}

#[derive(Clone, Debug)]
struct SourceTriangle {
    id: TriId,
    nodes: [usize; 3],
    stable_nodes: [u32; 3],
    component: i32,
}

#[derive(Clone, Copy, Debug)]
struct VertexProposal {
    point: Vec3,
    ratio: Option<DeterminantRatio>,
    second_ratio: Option<DeterminantRatio>,
    tier: PrecisionTier,
    rho: f64,
}

#[derive(Clone, Debug)]
struct PairSegment {
    endpoints: [IsectProv; 2],
    triangles: [TriId; 2],
}

#[derive(Clone, Debug)]
struct CandidatePoint {
    provenance: IsectProv,
    value: VertexProposal,
}

#[derive(Clone, Debug)]
struct DeferredConstruction {
    provenance: IsectProv,
    points: [Vec3; 2],
    rho: f64,
}

#[derive(Clone, Copy, Debug)]
enum C1Construction {
    Resolved(VertexProposal),
    Deferred { rho: f64 },
}

#[derive(Clone, Copy, Debug)]
enum C3Construction {
    Resolved(VertexProposal),
    Deferred { rho: f64 },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum TriangleContainment {
    Inside,
    Outside,
    BoundaryUncertain,
}

#[derive(Clone, Debug)]
enum PairIntersection {
    Disjoint,
    Proper(Box<[CandidatePoint; 2]>),
    Coplanar,
    Contact(Vec<CandidatePoint>),
    PrecisionDeferred(Vec<DeferredConstruction>),
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum SymbolicNode {
    Source(usize),
    Registry(IsectProv),
}

#[derive(Clone, Debug)]
struct CoplanarRelation {
    triangles: [TriId; 2],
    case: CoincidenceCase,
    area_overlap: bool,
    resolved: bool,
    contact_points: Vec<SymbolicNode>,
}

#[derive(Clone, Debug)]
struct NearCoincidence {
    triangles: [TriId; 2],
    matched_nodes: [(usize, usize); 3],
}

#[derive(Clone, Debug)]
struct PendingCurveSegment {
    nodes: [SymbolicNode; 2],
    triangles: SmallVec<[TriId; 2]>,
    components: SmallVec<[i32; 2]>,
}

#[derive(Clone, Debug)]
struct PendingPointFeature {
    case: CoincidenceCase,
    node: SymbolicNode,
    triangles: SmallVec<[TriId; 2]>,
    components: SmallVec<[i32; 2]>,
}

#[derive(Clone, Debug)]
struct CanonicalInput {
    triangles: Vec<SourceTriangle>,
    stable_to_surface: Vec<usize>,
}

// AI-FUNC-SUMMARY: Sort record used to assign source triangle ids independently of face emission order; side effects: none.
type CanonicalFaceRecord = (usize, i32, [(i64, i64, i64); 3]);

// AI-FUNC-SUMMARY: Canonical representative rank for one C7 source node; side effects: none.
type C7NodeRank = (TriId, (i64, i64, i64), usize);

// AI-FUNC-SUMMARY: Symbolic proposal-to-source-triangle incidence retained separately from normative intersection identity; side effects: none.
type ProposalOwners = BTreeMap<IsectProv, BTreeSet<TriId>>;

#[derive(Clone, Copy, Debug)]
struct CdtVertex {
    position: Point2<f64>,
    node: usize,
}

impl HasPosition for CdtVertex {
    type Scalar = f64;

    // AI-FUNC-SUMMARY: Return the immutable 2D position consumed by Spade; returns Point2<f64>; side effects: none.
    fn position(&self) -> Point2<Self::Scalar> {
        self.position
    }
}

// AI-FUNC-SUMMARY:
// Purpose: Execute the deterministic CPU G2-1 through G2-3 arrangement reference path.
// Inputs: S0 conditioned surface, S1 features, normalized-domain/envelope/component/policy options.
// Returns: ArrangedSurface with non-coplanar corefinement, common coplanar atomic subdivision, coincidence semantics, and typed degradation records.
// Side effects: None.
// Notes: Candidate generation is a deterministic CPU sweep-and-prune reference; G2-5 owns the production hybrid broad phase.
pub fn arrange_surface(
    surface: &ConditionedSurface,
    features: &FeatureSet,
    options: &ArrangeOptions,
) -> Result<ArrangedSurface> {
    if surface.faces.len() != surface.source_component.len() {
        return Err(RustMsptError::InvalidMesh(
            "S2 requires one persistent source_component per S0 face".to_string(),
        ));
    }
    let normalization = Normalization::new(options.domain_min, options.domain_max)?;
    let normalized: Vec<Vec3> = surface
        .vertices
        .iter()
        .map(|point| normalization.normalize(*point))
        .collect();
    let eps_normalized = options.eps / normalization.scale;
    let weld_step = 0.1 * eps_normalized;
    if !weld_step.is_finite() || weld_step <= 0.0 {
        return Err(RustMsptError::InvalidConfig(
            "S2 requires a finite positive weld step q = 0.1 * eps".to_string(),
        ));
    }
    let canonical = canonicalize_input(surface, &normalized, weld_step);
    let components = sorted_components(options, surface);
    let component_by_x: BTreeMap<i32, ArrangeComponent> = components
        .iter()
        .map(|component| (component.x, *component))
        .collect();
    let mut stats = ArrangementStats::default();
    let mut c1_cache: BTreeMap<IsectProv, C1Construction> = BTreeMap::new();
    let mut c3_cache: BTreeMap<IsectProv, C3Construction> = BTreeMap::new();
    let mut proposals: BTreeMap<IsectProv, VertexProposal> = BTreeMap::new();
    let mut proposal_owners = ProposalOwners::new();
    let mut pair_segments: BTreeMap<(TriId, TriId), PairSegment> = BTreeMap::new();
    let mut coplanar_relations = Vec::new();
    let mut near_coincidences = Vec::new();
    let mut pending_curves = Vec::new();
    let mut pending_points = Vec::new();
    let mut events = Vec::new();
    let mut degraded = Vec::new();

    let stage_clock = std::time::Instant::now();
    let candidate_pairs = triangle_candidate_pairs(&canonical.triangles, &normalized, eps_normalized)?;
    let stage_clock = time_arrange_stage("broad-phase", stage_clock);
    for (i, j) in candidate_pairs {
        let a = &canonical.triangles[i];
        let b = &canonical.triangles[j];
        if source_triangles_share_only_conforming_edge(a, b, &normalized) {
            continue;
        }
        stats.candidate_pairs += 1;
        let pair_key = (a.id.min(b.id), a.id.max(b.id));
        match intersect_source_triangles(a, b, &normalized, weld_step, &mut c1_cache)? {
            PairIntersection::Disjoint => {
                if let Some(near) = near_coincidence_match(
                    a,
                    b,
                    &normalized,
                    eps_normalized,
                    weld_step,
                    &mut degraded,
                ) {
                    push_pair_event(&mut events, CoincidenceCase::C7, a, b, &component_by_x);
                    push_full_semantic_events(&mut events, a, b, &component_by_x);
                    near_coincidences.push(near);
                }
            }
            PairIntersection::Coplanar => {
                if let Some((relation, case)) = classify_coplanar_pair(
                    a,
                    b,
                    &normalized,
                    weld_step,
                    &mut c3_cache,
                    &mut proposals,
                    &mut proposal_owners,
                    &mut degraded,
                    &mut stats,
                )? {
                    push_pair_event(&mut events, case, a, b, &component_by_x);
                    if matches!(
                        case,
                        CoincidenceCase::C1 | CoincidenceCase::C2 | CoincidenceCase::C3
                    ) {
                        push_full_semantic_events(&mut events, a, b, &component_by_x);
                    }
                    match case {
                        CoincidenceCase::C3 => stats.coplanar_overlays += 1,
                        CoincidenceCase::C4 => {
                            if let Some(segment) =
                                contact_curve_for_relation(&relation, a, b, &normalized, &proposals)
                            {
                                pending_curves.push(segment);
                            }
                        }
                        CoincidenceCase::C5 => {
                            if let Some(node) = relation.contact_points.first().cloned() {
                                pending_points.push(pending_point_feature(
                                    CoincidenceCase::C5,
                                    node,
                                    a,
                                    b,
                                ));
                            }
                        }
                        _ => {}
                    }
                    coplanar_relations.push(relation);
                }
            }
            PairIntersection::Contact(candidates) => {
                let mut symbolic: Vec<SymbolicNode> = Vec::new();
                for candidate in candidates {
                    let provenance = candidate.provenance.clone();
                    if insert_vertex_proposal(
                        &mut proposals,
                        &mut proposal_owners,
                        provenance.clone(),
                        candidate.value,
                        &[a.id, b.id],
                        weld_step,
                    )? && candidate.value.tier == PrecisionTier::DoubleDouble
                    {
                        stats.precision_escalations += 1;
                    }
                    symbolic.push(SymbolicNode::Registry(provenance));
                }
                symbolic.extend(
                    exact_contact_source_nodes(a, b, &normalized)
                        .into_iter()
                        .map(SymbolicNode::Source),
                );
                sort_symbolic_nodes(&mut symbolic, &normalized, &proposals, weld_step);
                let symbolic_before_weld = symbolic.clone();
                symbolic.dedup_by(|left, right| {
                    symbolic_node_key(left, &normalized, &proposals, weld_step)
                        == symbolic_node_key(right, &normalized, &proposals, weld_step)
                });
                let incompatible_weld = symbolic_before_weld.windows(2).any(|pair| {
                    symbolic_node_key(&pair[0], &normalized, &proposals, weld_step)
                        == symbolic_node_key(&pair[1], &normalized, &proposals, weld_step)
                        && symbolic_node_point(&pair[0], &normalized, &proposals)
                            != symbolic_node_point(&pair[1], &normalized, &proposals)
                });
                if incompatible_weld {
                    degraded.push(degraded_neighborhood(
                        DegradedReason::CollapsedContactSegment,
                        &[a.id, b.id],
                        symbolic_before_weld
                            .iter()
                            .filter_map(|node| match node {
                                SymbolicNode::Registry(provenance) => Some(provenance.clone()),
                                SymbolicNode::Source(_) => None,
                            })
                            .collect(),
                        symbolic_before_weld
                            .iter()
                            .map(|node| symbolic_node_point(node, &normalized, &proposals))
                            .collect(),
                        None,
                    ));
                }
                if symbolic.len() >= 2 {
                    push_pair_event(&mut events, CoincidenceCase::C4, a, b, &component_by_x);
                    pending_curves.push(PendingCurveSegment {
                        nodes: [symbolic[0].clone(), symbolic[symbolic.len() - 1].clone()],
                        triangles: SmallVec::from_slice(&[pair_key.0, pair_key.1]),
                        components: pair_components(a, b),
                    });
                } else if let Some(node) = symbolic.first().cloned() {
                    push_pair_event(&mut events, CoincidenceCase::C6, a, b, &component_by_x);
                    pending_points.push(pending_point_feature(CoincidenceCase::C6, node, a, b));
                }
            }
            PairIntersection::PrecisionDeferred(deferred) => {
                stats.precision_floor_routes += 1;
                let rho = deferred
                    .iter()
                    .map(|construction| construction.rho)
                    .min_by(f64::total_cmp)
                    .unwrap_or(0.0);
                degraded.push(degraded_neighborhood(
                    DegradedReason::PrecisionFloor,
                    &[a.id, b.id],
                    deferred
                        .iter()
                        .map(|construction| construction.provenance.clone())
                        .collect(),
                    deferred
                        .iter()
                        .flat_map(|construction| construction.points)
                        .collect(),
                    Some(rho),
                ));
            }
            PairIntersection::Proper(endpoints) => {
                stats.proper_intersections += 1;
                let mut keys = Vec::with_capacity(2);
                for endpoint in *endpoints {
                    let key = endpoint.provenance;
                    if insert_vertex_proposal(
                        &mut proposals,
                        &mut proposal_owners,
                        key.clone(),
                        endpoint.value,
                        &[a.id, b.id],
                        weld_step,
                    )? && endpoint.value.tier == PrecisionTier::DoubleDouble
                    {
                        stats.precision_escalations += 1;
                    }
                    keys.push(key);
                }
                pair_segments.insert(
                    pair_key,
                    PairSegment {
                        endpoints: [keys.remove(0), keys.remove(0)],
                        triangles: [pair_key.0, pair_key.1],
                    },
                );
            }
        }
    }

    classify_domain_coincidences(
        &canonical.triangles,
        &normalized,
        normalization.normalize(options.domain_min),
        normalization.normalize(options.domain_max),
        &component_by_x,
        &mut events,
    );

    collect_triple_points(
        &canonical.triangles,
        &normalized,
        weld_step,
        &pair_segments,
        &mut proposals,
        &mut proposal_owners,
        &mut stats,
        &mut degraded,
    )?;


    let mut stage_clock = time_arrange_stage("narrow-phase", stage_clock);
    let split_segments = split_pair_segments(&pair_segments, &proposals, weld_step, &mut degraded)?;
    let source_aliases = build_near_source_aliases(
        &near_coincidences,
        &normalized,
        eps_normalized,
        weld_step,
        &mut degraded,
    );
    let (vertices, node_normalized, source_node_map, mut registry) = build_registry_and_nodes(
        surface,
        &normalized,
        &canonical,
        normalization,
        weld_step,
        &proposals,
        &proposal_owners,
        &split_segments,
        &source_aliases,
        &mut degraded,
    )?;
    let (extra_constraints, extra_nodes, mut overlay_curves) = build_overlay_constraints(
        &canonical,
        &normalized,
        &node_normalized,
        &source_node_map,
        &registry,
        &proposals,
        &proposal_owners,
        &coplanar_relations,
        &pending_curves,
        &pending_points,
        weld_step,
        &mut degraded,
    )?;
    stage_clock = time_arrange_stage("registry+overlay", stage_clock);
    let source_faces = split_all_triangles(
        &canonical,
        &node_normalized,
        &source_node_map,
        &registry,
        &proposals,
        &extra_constraints,
        &extra_nodes,
        weld_step,
        &mut stats,
        &mut degraded,
    )?;
    stage_clock = time_arrange_stage("split-triangles", stage_clock);
    let faces = merge_atomic_faces(source_faces);
    promote_patch_events(&faces, &canonical, &mut events);
    sort_coincidence_events(&mut events);
    reject_for_policy(&events, options.coincidence)?;
    overlay_curves.extend(build_c3_boundary_curves(&faces, &canonical, &events));
    let warnings = if options.coincidence == CoincidencePolicy::Warn {
        events.clone()
    } else {
        Vec::new()
    };
    let (curves, mut corner_nodes) = build_arranged_curves(
        features,
        &canonical,
        &source_node_map,
        &registry,
        &faces,
        &node_normalized,
        weld_step,
        &overlay_curves,
        &mut degraded,
    )?;
    let mut point_features = resolve_point_features(
        &pending_points,
        &source_node_map,
        &registry,
        &proposals,
        &normalized,
        weld_step,
    )?;
    point_features.sort();
    point_features.dedup();
    corner_nodes.extend(point_features.iter().map(|feature| feature.node));
    stage_clock = time_arrange_stage("faces+curves", stage_clock);
    validate_arrangement(
        &faces,
        &node_normalized,
        &registry,
        &curves,
        &point_features,
        weld_step,
        eps_normalized,
        &mut degraded,
    )?;
    stage_clock = time_arrange_stage("validate", stage_clock);
    let _ = stage_clock;
    registry.segments.sort_by_key(|segment| segment.key);
    sort_degraded(&mut degraded);
    stats.point_features = point_features.len();
    stats.coincidence_events = events.len();
    stats.degraded_neighborhoods = degraded.len();
    Ok(ArrangedSurface {
        vertices,
        faces,
        curves,
        corner_nodes,
        point_features,
        registry,
        components,
        coincidence_events: events,
        warnings,
        degraded,
        stats,
    })
}

// AI-FUNC-SUMMARY:
// Purpose: Encode an arranged surface as a schema-v1 surface-stage VTU.
// Inputs: validated ArrangedSurface.
// Returns: VtuDoc with all always-present cell/point arrays and field tables; metadata is stamped by emit_snapshot.
// Side effects: None.
pub fn arranged_surface_to_doc(surface: &ArrangedSurface) -> VtuDoc {
    let face_count = surface.faces.len();
    let curve_count = surface.curves.len();
    let cell_count = face_count + curve_count;
    let mut doc = VtuDoc {
        points: surface.vertices.clone(),
        ..Default::default()
    };
    let mut cell_kind = Vec::with_capacity(cell_count);
    let region_key = vec![-1i32; cell_count];
    let partition_id = vec![-1i32; cell_count];
    let regime = vec![255u8; cell_count];
    let mut face_tag_key = Vec::with_capacity(cell_count);
    let mut curve_id = Vec::with_capacity(cell_count);
    let component_by_x: BTreeMap<i32, ArrangeComponent> = surface
        .components
        .iter()
        .map(|component| (component.x, *component))
        .collect();
    let face_tag_sets: BTreeSet<(Vec<(i32, i8)>, bool)> = surface
        .faces
        .iter()
        .map(|face| {
            (
                face.components
                    .iter()
                    .copied()
                    .zip(face.tag_orientations.iter().copied())
                    .collect(),
                face.box_tagged,
            )
        })
        .collect();
    let face_tag_by_set: BTreeMap<(Vec<(i32, i8)>, bool), i32> = face_tag_sets
        .iter()
        .enumerate()
        .map(|(index, set)| (set.clone(), index as i32))
        .collect();

    for face in &surface.faces {
        append_cell(&mut doc, &face.nodes, VTK_TRIANGLE);
        cell_kind.push(1);
        let tags: Vec<(i32, i8)> = face
            .components
            .iter()
            .copied()
            .zip(face.tag_orientations.iter().copied())
            .collect();
        face_tag_key.push(face_tag_by_set[&(tags, face.box_tagged)]);
        curve_id.push(-1);
    }
    for (index, curve) in surface.curves.iter().enumerate() {
        append_cell(&mut doc, &curve.nodes, VTK_POLY_LINE);
        cell_kind.push(2);
        face_tag_key.push(-1);
        curve_id.push(index as i32);
    }

    doc.cell_data
        .push(DataArray::scalar("cell_kind", ArrayData::U8(cell_kind)));
    doc.cell_data
        .push(DataArray::scalar("region_key", ArrayData::I32(region_key)));
    doc.cell_data.push(DataArray::scalar(
        "partition_id",
        ArrayData::I32(partition_id),
    ));
    doc.cell_data
        .push(DataArray::scalar("regime", ArrayData::U8(regime)));
    doc.cell_data.push(DataArray::scalar(
        "face_tag_key",
        ArrayData::I32(face_tag_key),
    ));
    doc.cell_data
        .push(DataArray::scalar("curve_id", ArrayData::I32(curve_id)));

    let mut incident_components: Vec<BTreeSet<i32>> = vec![BTreeSet::new(); surface.vertices.len()];
    for face in &surface.faces {
        for node in face.nodes {
            incident_components[node].extend(face.components.iter().copied());
        }
    }
    for curve in &surface.curves {
        for node in &curve.nodes {
            incident_components[*node].extend(curve.components.iter().copied());
        }
    }
    let sets: Vec<Vec<i32>> = incident_components
        .iter()
        .map(|set| set.iter().copied().collect())
        .collect();
    let unique_sets: BTreeSet<Vec<i32>> = sets.iter().cloned().collect();
    let set_keys: BTreeMap<Vec<i32>, i32> = unique_sets
        .iter()
        .enumerate()
        .map(|(index, set)| (set.clone(), index as i32))
        .collect();
    let n_id_key: Vec<i32> = sets.iter().map(|set| set_keys[set]).collect();
    let mut node_curve = vec![None; surface.vertices.len()];
    for (curve_index, curve) in surface.curves.iter().enumerate() {
        for node in &curve.nodes {
            node_curve[*node].get_or_insert(curve_index as i32);
        }
    }
    let mut constraint_kind = Vec::with_capacity(surface.vertices.len());
    let mut constraint_ref = Vec::with_capacity(surface.vertices.len());
    for node in 0..surface.vertices.len() {
        if surface.corner_nodes.contains(&node) {
            constraint_kind.push(3u8);
            constraint_ref.push(node_curve[node].unwrap_or(-1));
        } else if let Some(curve) = node_curve[node] {
            constraint_kind.push(2u8);
            constraint_ref.push(curve);
        } else {
            constraint_kind.push(1u8);
            constraint_ref.push(sets[node].first().copied().unwrap_or(-1));
        }
    }
    doc.point_data
        .push(DataArray::scalar("n_id_key", ArrayData::I32(n_id_key)));
    doc.point_data.push(DataArray::scalar(
        "constraint_kind",
        ArrayData::U8(constraint_kind),
    ));
    doc.point_data.push(DataArray::scalar(
        "constraint_ref",
        ArrayData::I32(constraint_ref),
    ));

    push_field(&mut doc, "RegionSetOffsets", 1, ArrayData::I64(vec![1]));
    push_field(&mut doc, "RegionSetComponents", 1, ArrayData::I32(vec![0]));
    push_field(
        &mut doc,
        "RegionSetPriority",
        1,
        ArrayData::U32(vec![u32::MAX]),
    );
    let mut n_id_offsets = Vec::with_capacity(unique_sets.len());
    let mut n_id_components = Vec::new();
    for set in &unique_sets {
        n_id_components.extend(set.iter().copied());
        n_id_offsets.push(n_id_components.len() as i64);
    }
    push_field(&mut doc, "NIdSetOffsets", 1, ArrayData::I64(n_id_offsets));
    push_field(
        &mut doc,
        "NIdSetComponents",
        1,
        ArrayData::I32(n_id_components),
    );

    let mut face_offsets = Vec::with_capacity(face_tag_sets.len());
    let mut face_components_flat = Vec::new();
    let mut face_orientations = Vec::new();
    let mut face_kinds = Vec::with_capacity(face_tag_sets.len());
    for (tags, box_tagged) in &face_tag_sets {
        face_components_flat.extend(tags.iter().map(|(component, _)| *component));
        face_orientations.extend(tags.iter().map(|(_, orientation)| *orientation));
        face_offsets.push(face_components_flat.len() as i64);
        // FaceTagKind: 0 interface, 1 sheet, 2 box cap (SPEC_meshgen_contracts §2.3).
        // A box-clip cap wins over the sheet kind; C10 keeps the sheet X in the same set.
        let sheet = tags.iter().any(|(component, _)| {
            component_by_x
                .get(component)
                .map(|meta| meta.kind == 1)
                .unwrap_or(false)
        });
        face_kinds.push(if *box_tagged {
            2
        } else {
            u8::from(sheet)
        });
    }
    push_field(&mut doc, "FaceTagOffsets", 1, ArrayData::I64(face_offsets));
    push_field(
        &mut doc,
        "FaceTagComponents",
        1,
        ArrayData::I32(face_components_flat),
    );
    push_field(
        &mut doc,
        "FaceTagOrientation",
        1,
        ArrayData::I32(face_orientations.into_iter().map(i32::from).collect()),
    );
    push_field(&mut doc, "FaceTagKind", 1, ArrayData::U8(face_kinds));
    push_field(
        &mut doc,
        "FaceTagSideElems",
        2,
        ArrayData::I32(vec![-1; face_count * 2]),
    );

    push_field(
        &mut doc,
        "ComponentX",
        1,
        ArrayData::I32(
            surface
                .components
                .iter()
                .map(|component| component.x)
                .collect(),
        ),
    );
    push_field(
        &mut doc,
        "ComponentY",
        1,
        ArrayData::U32(
            surface
                .components
                .iter()
                .map(|component| component.priority)
                .collect(),
        ),
    );
    push_field(
        &mut doc,
        "ComponentKind",
        1,
        ArrayData::U8(
            surface
                .components
                .iter()
                .map(|component| component.kind)
                .collect(),
        ),
    );
    push_field(
        &mut doc,
        "ComponentClosed",
        1,
        ArrayData::U8(
            surface
                .components
                .iter()
                .map(|component| u8::from(component.closed))
                .collect(),
        ),
    );

    let mut curve_offsets = Vec::with_capacity(curve_count);
    let mut curve_components = Vec::new();
    let mut curve_kinds = Vec::with_capacity(curve_count);
    for curve in &surface.curves {
        curve_components.extend(curve.components.iter().copied());
        curve_offsets.push(curve_components.len() as i64);
        curve_kinds.push(curve.kind as u8);
    }
    push_field(&mut doc, "CurveKind", 1, ArrayData::U8(curve_kinds));
    push_field(
        &mut doc,
        "CurveCompOffsets",
        1,
        ArrayData::I64(curve_offsets),
    );
    push_field(
        &mut doc,
        "CurveCompComponents",
        1,
        ArrayData::I32(curve_components),
    );
    doc
}

// AI-FUNC-SUMMARY: Return the sorted distinct component pair for two source triangles; returns SmallVec; side effects: none.
fn pair_components(a: &SourceTriangle, b: &SourceTriangle) -> SmallVec<[i32; 2]> {
    let mut components = SmallVec::from_slice(&[a.component, b.component]);
    components.sort_unstable();
    components.dedup();
    components
}

// AI-FUNC-SUMMARY: Append one canonical triangle-pair coincidence event; side effects: mutates event collection.
fn push_pair_event(
    events: &mut Vec<CoincidenceEvent>,
    case: CoincidenceCase,
    a: &SourceTriangle,
    b: &SourceTriangle,
    component_by_x: &BTreeMap<i32, ArrangeComponent>,
) {
    let _ = component_by_x;
    let (first, second) = if a.id <= b.id { (a, b) } else { (b, a) };
    events.push(CoincidenceEvent {
        case,
        entities: [
            CoincidenceEntity::Triangle(first.id),
            CoincidenceEntity::Triangle(second.id),
        ],
        components: pair_components(first, second),
    });
}

// AI-FUNC-SUMMARY: Add C8/C9 semantic classifications applicable to one full or epsilon coincidence without replacing its geometric case; side effects: mutates event collection.
fn push_full_semantic_events(
    events: &mut Vec<CoincidenceEvent>,
    a: &SourceTriangle,
    b: &SourceTriangle,
    component_by_x: &BTreeMap<i32, ArrangeComponent>,
) {
    let a_meta = component_by_x.get(&a.component);
    let b_meta = component_by_x.get(&b.component);
    if matches!((a_meta, b_meta), (Some(left), Some(right)) if left.priority != right.priority) {
        push_pair_event(events, CoincidenceCase::C8, a, b, component_by_x);
    }
    if matches!((a_meta, b_meta), (Some(left), Some(right)) if (left.kind == 0 && right.kind == 1) || (left.kind == 1 && right.kind == 0))
    {
        push_pair_event(events, CoincidenceCase::C9, a, b, component_by_x);
    }
}

// AI-FUNC-SUMMARY: Sort and deduplicate coincidence events by frozen case/entity/component order; side effects: mutates event collection.
fn sort_coincidence_events(events: &mut Vec<CoincidenceEvent>) {
    events.sort();
    events.dedup();
}

// AI-FUNC-SUMMARY: Enforce reject mode by aggregating every sorted C1/C2/C3/C7/C8/C9 offender into one ARR-COINC error; returns Ok or InvalidMesh; side effects: none.
fn reject_for_policy(events: &[CoincidenceEvent], policy: CoincidencePolicy) -> Result<()> {
    let offending: Vec<&CoincidenceEvent> = events
        .iter()
        .filter(|event| event.case.rejected_by(policy))
        .collect();
    if offending.is_empty() {
        return Ok(());
    }
    let entries = offending
        .iter()
        .map(|event| format!("{:?}:{:?}", event.case, event.entities))
        .collect::<Vec<_>>()
        .join(", ");
    Err(RustMsptError::InvalidMesh(format!(
        "[ARR-COINC] reject policy refused sorted offenders [{entries}]"
    )))
}

// AI-FUNC-SUMMARY: Project one point into the two in-face coordinates of a domain plane; returns [u,v]; side effects: none.
fn project_domain_face_point(point: Vec3, normal_axis: usize) -> [f64; 2] {
    match normal_axis {
        0 => [point.y, point.z],
        1 => [point.x, point.z],
        _ => [point.x, point.y],
    }
}

// AI-FUNC-SUMMARY: Test strict overlap of two counter-clockwise convex polygons using exact orientation signs on every separating axis; returns false for line/point contact; side effects: none.
fn convex_polygons_overlap_area(first: &[[f64; 2]], second: &[[f64; 2]]) -> bool {
    [first, second]
        .into_iter()
        .zip([second, first])
        .all(|(polygon, other)| {
            (0..polygon.len()).all(|index| {
                let start = polygon[index];
                let end = polygon[(index + 1) % polygon.len()];
                let start = Vec3::new(start[0], start[1], 0.0);
                let end = Vec3::new(end[0], end[1], 0.0);
                other.iter().any(|point| {
                    orient2d_axis(
                        start,
                        end,
                        Vec3::new(point[0], point[1], 0.0),
                        ProjectionAxis::Z,
                    ) > 0.0
                })
            })
        })
}

// AI-FUNC-SUMMARY: Test whether a coplanar triangle overlaps a bounded domain-face rectangle with strictly positive area using exact separating-axis signs; returns false for line/point contact; side effects: none.
fn triangle_overlaps_domain_face_area(
    triangle: [Vec3; 3],
    normal_axis: usize,
    domain_min: Vec3,
    domain_max: Vec3,
) -> bool {
    let min = project_domain_face_point(domain_min, normal_axis);
    let max = project_domain_face_point(domain_max, normal_axis);
    let mut triangle: Vec<[f64; 2]> = triangle
        .into_iter()
        .map(|point| project_domain_face_point(point, normal_axis))
        .collect();
    let as_point = |point: [f64; 2]| Vec3::new(point[0], point[1], 0.0);
    let orientation = orient2d_axis(
        as_point(triangle[0]),
        as_point(triangle[1]),
        as_point(triangle[2]),
        ProjectionAxis::Z,
    );
    if orientation == 0.0 {
        return false;
    }
    if orientation < 0.0 {
        triangle.swap(1, 2);
    }
    let rectangle = [
        [min[0], min[1]],
        [max[0], min[1]],
        [max[0], max[1]],
        [min[0], max[1]],
    ];
    convex_polygons_overlap_area(&triangle, &rectangle)
}

// AI-FUNC-SUMMARY: Classify sheet triangles with positive-area overlap on bounded normalized domain faces as C10 while leaving clipping/tags/curves to G2-5; side effects: appends events.
fn classify_domain_coincidences(
    triangles: &[SourceTriangle],
    vertices: &[Vec3],
    domain_min: Vec3,
    domain_max: Vec3,
    component_by_x: &BTreeMap<i32, ArrangeComponent>,
    events: &mut Vec<CoincidenceEvent>,
) {
    let planes = [
        (0u8, 0usize, domain_min.x),
        (1, 0, domain_max.x),
        (2, 1, domain_min.y),
        (3, 1, domain_max.y),
        (4, 2, domain_min.z),
        (5, 2, domain_max.z),
    ];
    for triangle in triangles {
        if component_by_x
            .get(&triangle.component)
            .is_none_or(|component| component.kind != 1)
        {
            continue;
        }
        for (face, axis, value) in planes {
            let on_plane = triangle.nodes.iter().all(|node| {
                let point = vertices[*node];
                match axis {
                    0 => point.x == value,
                    1 => point.y == value,
                    _ => point.z == value,
                }
            });
            if on_plane
                && triangle_overlaps_domain_face_area(
                    triangle.nodes.map(|node| vertices[node]),
                    axis,
                    domain_min,
                    domain_max,
                )
            {
                events.push(CoincidenceEvent {
                    case: CoincidenceCase::C10,
                    entities: [
                        CoincidenceEntity::Triangle(triangle.id),
                        CoincidenceEntity::DomainFace(face),
                    ],
                    components: SmallVec::from_slice(&[triangle.component]),
                });
            }
        }
    }
}

// AI-FUNC-SUMMARY: Build one canonical typed degraded-neighborhood record with sorted provenance and source identities; returns DegradedNeighborhood; side effects: none.
fn degraded_neighborhood(
    reason: DegradedReason,
    triangles: &[TriId],
    mut provenances: Vec<IsectProv>,
    mut points: Vec<Vec3>,
    rho: Option<f64>,
) -> DegradedNeighborhood {
    let mut triangles: SmallVec<[TriId; 3]> = triangles.iter().copied().collect();
    triangles.sort_unstable();
    triangles.dedup();
    provenances.sort();
    provenances.dedup();
    points.sort_by(|left, right| {
        left.x
            .total_cmp(&right.x)
            .then_with(|| left.y.total_cmp(&right.y))
            .then_with(|| left.z.total_cmp(&right.z))
    });
    points.dedup();
    DegradedNeighborhood {
        reason,
        triangles,
        provenances,
        points,
        rho,
    }
}

// AI-FUNC-SUMMARY: Sort and deduplicate degraded neighborhoods by reason/source/provenance/rho/target geometry; side effects: mutates records.
fn sort_degraded(degraded: &mut Vec<DegradedNeighborhood>) {
    degraded.sort_by(|left, right| {
        left.reason
            .cmp(&right.reason)
            .then_with(|| left.triangles.cmp(&right.triangles))
            .then_with(|| left.provenances.cmp(&right.provenances))
            .then_with(|| match (left.rho, right.rho) {
                (Some(a), Some(b)) => a.total_cmp(&b),
                (None, Some(_)) => std::cmp::Ordering::Less,
                (Some(_), None) => std::cmp::Ordering::Greater,
                (None, None) => std::cmp::Ordering::Equal,
            })
            .then_with(|| {
                left.points
                    .iter()
                    .flat_map(|point| [point.x.to_bits(), point.y.to_bits(), point.z.to_bits()])
                    .cmp(right.points.iter().flat_map(|point| {
                        [point.x.to_bits(), point.y.to_bits(), point.z.to_bits()]
                    }))
            })
    });
    degraded.dedup();
}

// AI-FUNC-SUMMARY: Derive every explicit unordered source-triangle pair covered by typed degradation records; returns BTreeSet; side effects: none.
fn degraded_triangle_pairs(degraded: &[DegradedNeighborhood]) -> BTreeSet<(TriId, TriId)> {
    let mut pairs = BTreeSet::new();
    for neighborhood in degraded {
        if !matches!(
            neighborhood.reason,
            DegradedReason::PrecisionFloor
                | DegradedReason::QuantizedOrderAmbiguity
                | DegradedReason::CollapsedContactSegment
                | DegradedReason::ResidualCrossing
        ) {
            continue;
        }
        for i in 0..neighborhood.triangles.len() {
            for j in i + 1..neighborhood.triangles.len() {
                pairs.insert((
                    neighborhood.triangles[i].min(neighborhood.triangles[j]),
                    neighborhood.triangles[i].max(neighborhood.triangles[j]),
                ));
            }
        }
    }
    pairs
}

// AI-FUNC-SUMMARY: Return the normalized coordinate represented by a source or registry symbolic node; returns Vec3; side effects: none.
fn symbolic_node_point(
    node: &SymbolicNode,
    vertices: &[Vec3],
    proposals: &BTreeMap<IsectProv, VertexProposal>,
) -> Vec3 {
    match node {
        SymbolicNode::Source(node) => vertices[*node],
        SymbolicNode::Registry(provenance) => proposals[provenance].point,
    }
}

// AI-FUNC-SUMMARY: Return the quantized weld key for one symbolic source/registry point; returns NodeKey tuple; side effects: none.
fn symbolic_node_key(
    node: &SymbolicNode,
    vertices: &[Vec3],
    proposals: &BTreeMap<IsectProv, VertexProposal>,
    weld_step: f64,
) -> (i64, i64, i64) {
    node_key(symbolic_node_point(node, vertices, proposals), weld_step)
}

// AI-FUNC-SUMMARY: Sort symbolic nodes by NodeKey then symbolic provenance so aliases occur only after all provenance was collected; side effects: mutates nodes.
fn sort_symbolic_nodes(
    nodes: &mut [SymbolicNode],
    vertices: &[Vec3],
    proposals: &BTreeMap<IsectProv, VertexProposal>,
    weld_step: f64,
) {
    nodes.sort_by(|left, right| {
        symbolic_node_key(left, vertices, proposals, weld_step)
            .cmp(&symbolic_node_key(right, vertices, proposals, weld_step))
            .then_with(|| left.cmp(right))
    });
}

// AI-FUNC-SUMMARY: Return exact projected point location relative to a nondegenerate triangle as inside=1, boundary=0, outside=-1; returns i8; side effects: none.
fn exact_point_location(point: Vec3, triangle: [Vec3; 3]) -> i8 {
    let axis = best_projection_axis(triangle[0], triangle[1], triangle[2]);
    let orientation = orient2d_axis(triangle[0], triangle[1], triangle[2], axis);
    if orientation == 0.0 {
        return -1;
    }
    let signs = [
        orient2d_axis(triangle[0], triangle[1], point, axis),
        orient2d_axis(triangle[1], triangle[2], point, axis),
        orient2d_axis(triangle[2], triangle[0], point, axis),
    ];
    let outside = if orientation > 0.0 {
        signs.iter().any(|sign| *sign < 0.0)
    } else {
        signs.iter().any(|sign| *sign > 0.0)
    };
    if outside {
        -1
    } else if signs.contains(&0.0) {
        0
    } else {
        1
    }
}

// AI-FUNC-SUMMARY: Collect source vertices lying exactly on and inside the opposite triangle for a noncoplanar contact; returns sorted source-node ids; side effects: none.
fn exact_contact_source_nodes(
    a: &SourceTriangle,
    b: &SourceTriangle,
    vertices: &[Vec3],
) -> Vec<usize> {
    let atri = a.nodes.map(|node| vertices[node]);
    let btri = b.nodes.map(|node| vertices[node]);
    let mut nodes = Vec::new();
    for node in a.nodes {
        let point = vertices[node];
        if orient3d_filtered(btri[0], btri[1], btri[2], point).0 == 0
            && exact_point_location(point, btri) >= 0
        {
            nodes.push(node);
        }
    }
    for node in b.nodes {
        let point = vertices[node];
        if orient3d_filtered(atri[0], atri[1], atri[2], point).0 == 0
            && exact_point_location(point, atri) >= 0
        {
            nodes.push(node);
        }
    }
    nodes.sort_unstable();
    nodes.dedup();
    nodes
}

// AI-FUNC-SUMMARY: Build one pending C5/C6 point feature from a canonical symbolic node and triangle pair; returns PendingPointFeature; side effects: none.
fn pending_point_feature(
    case: CoincidenceCase,
    node: SymbolicNode,
    a: &SourceTriangle,
    b: &SourceTriangle,
) -> PendingPointFeature {
    let mut triangles = SmallVec::from_slice(&[a.id, b.id]);
    triangles.sort_unstable();
    PendingPointFeature {
        case,
        node,
        triangles,
        components: pair_components(a, b),
    }
}

// AI-FUNC-SUMMARY: Detect calibrated C7 mutual one-to-one vertex coincidence by epsilon-squared inclusion for an otherwise disjoint triangle pair; returns canonical higher-to-lower node matches or None; side effects: appends q-ambiguity degradation.
fn near_coincidence_match(
    a: &SourceTriangle,
    b: &SourceTriangle,
    vertices: &[Vec3],
    eps: f64,
    weld_step: f64,
    degraded: &mut Vec<DegradedNeighborhood>,
) -> Option<NearCoincidence> {
    let atri = a.nodes.map(|node| vertices[node]);
    let btri = b.nodes.map(|node| vertices[node]);
    let eps2 = eps * eps;
    if !eps2.is_finite() || eps2 <= 0.0 {
        return None;
    }
    let mut mapping = [usize::MAX; 3];
    for (i, point) in atri.iter().enumerate() {
        let candidates: Vec<usize> = btri
            .iter()
            .enumerate()
            .filter_map(|(j, other)| {
                let delta = point.sub(*other);
                (delta.dot(delta) <= eps2).then_some(j)
            })
            .collect();
        if candidates.len() != 1 {
            if candidates.len() > 1 {
                degraded.push(degraded_neighborhood(
                    DegradedReason::QuantizedOrderAmbiguity,
                    &[a.id, b.id],
                    Vec::new(),
                    atri.into_iter().chain(btri).collect(),
                    None,
                ));
            }
            return None;
        }
        mapping[i] = candidates[0];
    }
    for (j, point) in btri.iter().enumerate() {
        let reverse: Vec<usize> = atri
            .iter()
            .enumerate()
            .filter_map(|(i, other)| {
                let delta = point.sub(*other);
                (delta.dot(delta) <= eps2).then_some(i)
            })
            .collect();
        if reverse.len() != 1 || mapping[reverse[0]] != j {
            if reverse.len() > 1 {
                degraded.push(degraded_neighborhood(
                    DegradedReason::QuantizedOrderAmbiguity,
                    &[a.id, b.id],
                    Vec::new(),
                    atri.into_iter().chain(btri).collect(),
                    None,
                ));
            }
            return None;
        }
    }
    let (lower, higher, lower_is_a) = if a.id <= b.id {
        (a, b, true)
    } else {
        (b, a, false)
    };
    let mut matched_nodes = [(0usize, 0usize); 3];
    if lower_is_a {
        for i in 0..3 {
            matched_nodes[i] = (higher.nodes[mapping[i]], lower.nodes[i]);
        }
    } else {
        for i in 0..3 {
            matched_nodes[mapping[i]] = (higher.nodes[i], lower.nodes[mapping[i]]);
        }
    }
    let q_ambiguous = matched_nodes.iter().any(|(higher, lower)| {
        let delta = vertices[*higher].sub(vertices[*lower]);
        delta.dot(delta) > eps2
            || (!delta.dot(delta).is_finite())
            || (node_key(vertices[*higher], weld_step) == node_key(vertices[*lower], weld_step)
                && vertices[*higher] != vertices[*lower])
    });
    if q_ambiguous {
        degraded.push(degraded_neighborhood(
            DegradedReason::QuantizedOrderAmbiguity,
            &[a.id, b.id],
            Vec::new(),
            matched_nodes
                .iter()
                .flat_map(|(higher, lower)| [vertices[*higher], vertices[*lower]])
                .collect(),
            None,
        ));
    }
    Some(NearCoincidence {
        triangles: [lower.id, higher.id],
        matched_nodes,
    })
}

// AI-FUNC-SUMMARY: Build deterministic C7 source-node clusters whose complete-link diameter never exceeds epsilon, selecting representatives by canonical triangle/NodeKey order; returns alias map; side effects: appends rejected transitive-merge degradation.
fn build_near_source_aliases(
    near: &[NearCoincidence],
    vertices: &[Vec3],
    eps: f64,
    weld_step: f64,
    degraded: &mut Vec<DegradedNeighborhood>,
) -> BTreeMap<usize, usize> {
    let mut sorted = near.to_vec();
    sorted.sort_by_key(|item| item.triangles);
    let mut clusters: Vec<BTreeSet<usize>> = Vec::new();
    let mut cluster_of: BTreeMap<usize, usize> = BTreeMap::new();
    let mut rank: BTreeMap<usize, C7NodeRank> = BTreeMap::new();
    let eps2 = eps * eps;
    for item in &sorted {
        for (higher, lower) in item.matched_nodes {
            rank.entry(lower).or_insert((
                item.triangles[0],
                node_key(vertices[lower], weld_step),
                lower,
            ));
            rank.entry(higher).or_insert((
                item.triangles[1],
                node_key(vertices[higher], weld_step),
                higher,
            ));
            for node in [lower, higher] {
                cluster_of.entry(node).or_insert_with(|| {
                    let index = clusters.len();
                    clusters.push(BTreeSet::from([node]));
                    index
                });
            }
        }
    }
    for item in sorted {
        let mut merge_pairs = Vec::new();
        let mut valid = true;
        for (higher, lower) in item.matched_nodes {
            let a = cluster_of[&higher];
            let b = cluster_of[&lower];
            if a == b {
                continue;
            }
            let within_diameter = clusters[a].iter().all(|left| {
                clusters[b].iter().all(|right| {
                    let delta = vertices[*left].sub(vertices[*right]);
                    let distance2 = delta.dot(delta);
                    distance2.is_finite() && distance2 <= eps2
                })
            });
            if !within_diameter {
                valid = false;
                break;
            }
            merge_pairs.push((higher, lower));
        }
        if !valid {
            degraded.push(degraded_neighborhood(
                DegradedReason::QuantizedOrderAmbiguity,
                &item.triangles,
                Vec::new(),
                item.matched_nodes
                    .iter()
                    .flat_map(|(higher, lower)| [vertices[*higher], vertices[*lower]])
                    .collect(),
                None,
            ));
            continue;
        }
        for (higher, lower) in merge_pairs {
            let a = cluster_of[&higher];
            let b = cluster_of[&lower];
            if a == b || clusters[a].is_empty() || clusters[b].is_empty() {
                continue;
            }
            let (keep, remove) = if a < b { (a, b) } else { (b, a) };
            let moved: Vec<usize> = clusters[remove].iter().copied().collect();
            for node in moved {
                clusters[keep].insert(node);
                cluster_of.insert(node, keep);
            }
            clusters[remove].clear();
        }
    }
    let mut aliases = BTreeMap::new();
    for cluster in clusters.into_iter().filter(|cluster| !cluster.is_empty()) {
        let representative = cluster
            .iter()
            .copied()
            .min_by_key(|node| rank[node])
            .unwrap_or(0);
        for node in cluster {
            if node != representative {
                aliases.insert(node, representative);
            }
        }
    }
    aliases
}

// AI-FUNC-SUMMARY: Follow deterministic C7 source-node aliases to their canonical representative; returns source node id; side effects: none.
fn resolve_source_alias(mut node: usize, aliases: &BTreeMap<usize, usize>) -> usize {
    let mut steps = 0usize;
    while let Some(next) = aliases.get(&node).copied() {
        if next == node || steps > aliases.len() {
            break;
        }
        node = next;
        steps += 1;
    }
    node
}

// AI-FUNC-SUMMARY: Return the three source edges with stable-id canonical endpoint order and normalized points; returns deterministic edge records; side effects: none.
fn canonical_source_edges(
    triangle: &SourceTriangle,
    vertices: &[Vec3],
) -> Vec<(EdgeId, [usize; 2], [Vec3; 2])> {
    let mut edges = Vec::with_capacity(3);
    for index in 0..3 {
        let next = (index + 1) % 3;
        let (first, second) = if triangle.stable_nodes[index] <= triangle.stable_nodes[next] {
            (index, next)
        } else {
            (next, index)
        };
        let nodes = [triangle.nodes[first], triangle.nodes[second]];
        edges.push((
            EdgeId::new(triangle.stable_nodes[first], triangle.stable_nodes[second]),
            nodes,
            nodes.map(|node| vertices[node]),
        ));
    }
    edges.sort_by_key(|edge| edge.0);
    edges
}

// AI-FUNC-SUMMARY: Test whether two exact orient2d signs strictly straddle zero without multiplying signs; returns bool; side effects: none.
fn strict_straddle(a: f64, b: f64) -> bool {
    (a < 0.0 && b > 0.0) || (a > 0.0 && b < 0.0)
}

// AI-FUNC-SUMMARY: Determine whether a symbolic coplanar intersection point set contains an exact non-collinear triple; returns bool; side effects: none.
fn symbolic_points_have_area(
    points: &[SymbolicNode],
    vertices: &[Vec3],
    proposals: &BTreeMap<IsectProv, VertexProposal>,
    axis: ProjectionAxis,
) -> bool {
    for i in 0..points.len() {
        for j in i + 1..points.len() {
            for k in j + 1..points.len() {
                if orient2d_axis(
                    symbolic_node_point(&points[i], vertices, proposals),
                    symbolic_node_point(&points[j], vertices, proposals),
                    symbolic_node_point(&points[k], vertices, proposals),
                    axis,
                ) != 0.0
                {
                    return true;
                }
            }
        }
    }
    false
}

// AI-FUNC-SUMMARY: Compare full coplanar triangle winding in one common exact projection; returns C1 for same and C2 for opposite orientation; side effects: none.
fn full_coincidence_case(
    a: &SourceTriangle,
    b: &SourceTriangle,
    vertices: &[Vec3],
) -> CoincidenceCase {
    let atri = a.nodes.map(|node| vertices[node]);
    let btri = b.nodes.map(|node| vertices[node]);
    let axis = best_projection_axis(atri[0], atri[1], atri[2]);
    let a_orientation = orient2d_axis(atri[0], atri[1], atri[2], axis);
    let b_orientation = orient2d_axis(btri[0], btri[1], btri[2], axis);
    if a_orientation.signum() == b_orientation.signum() {
        CoincidenceCase::C1
    } else {
        CoincidenceCase::C2
    }
}

// AI-FUNC-SUMMARY: Perform deterministic exact-topology coplanar triangle intersection, constructing every strict EdgeEdge point through frozen C3 before registry commit; returns typed relation/case or disjoint; side effects: mutates proposals, stats, and degraded records.
#[allow(clippy::too_many_arguments)]
fn classify_coplanar_pair(
    a: &SourceTriangle,
    b: &SourceTriangle,
    vertices: &[Vec3],
    weld_step: f64,
    c3_cache: &mut BTreeMap<IsectProv, C3Construction>,
    proposals: &mut BTreeMap<IsectProv, VertexProposal>,
    proposal_owners: &mut ProposalOwners,
    degraded: &mut Vec<DegradedNeighborhood>,
    stats: &mut ArrangementStats,
) -> Result<Option<(CoplanarRelation, CoincidenceCase)>> {
    let atri = a.nodes.map(|node| vertices[node]);
    let btri = b.nodes.map(|node| vertices[node]);
    let axis = best_projection_axis(atri[0], atri[1], atri[2]);
    let mut a_nodes = a.nodes;
    let mut b_nodes = b.nodes;
    a_nodes.sort_unstable();
    b_nodes.sort_unstable();
    if a_nodes == b_nodes {
        let case = full_coincidence_case(a, b, vertices);
        let mut contact_points: Vec<SymbolicNode> =
            a.nodes.into_iter().map(SymbolicNode::Source).collect();
        sort_symbolic_nodes(&mut contact_points, vertices, proposals, weld_step);
        return Ok(Some((
            CoplanarRelation {
                triangles: [a.id.min(b.id), a.id.max(b.id)],
                case,
                area_overlap: true,
                resolved: true,
                contact_points,
            },
            case,
        )));
    }

    let mut points = Vec::new();
    let mut strict_inside = false;
    for node in a.nodes {
        let location = exact_point_location(vertices[node], btri);
        if location >= 0 {
            strict_inside |= location > 0;
            points.push(SymbolicNode::Source(node));
        }
    }
    for node in b.nodes {
        let location = exact_point_location(vertices[node], atri);
        if location >= 0 {
            strict_inside |= location > 0;
            points.push(SymbolicNode::Source(node));
        }
    }

    let a_edges = canonical_source_edges(a, vertices);
    let b_edges = canonical_source_edges(b, vertices);
    let mut strict_crossings = 0usize;
    let mut required_deferred = false;
    let mut inserted_provenances = Vec::new();
    for a_edge in &a_edges {
        for b_edge in &b_edges {
            let a0 = orient2d_axis(b_edge.2[0], b_edge.2[1], a_edge.2[0], axis);
            let a1 = orient2d_axis(b_edge.2[0], b_edge.2[1], a_edge.2[1], axis);
            let b0 = orient2d_axis(a_edge.2[0], a_edge.2[1], b_edge.2[0], axis);
            let b1 = orient2d_axis(a_edge.2[0], a_edge.2[1], b_edge.2[1], axis);
            if !(strict_straddle(a0, a1) && strict_straddle(b0, b1)) {
                continue;
            }
            strict_crossings += 1;
            let (first, second) = if a_edge.0 <= b_edge.0 {
                (a_edge, b_edge)
            } else {
                (b_edge, a_edge)
            };
            let provenance = IsectProv::EdgeEdge {
                first: first.0,
                second: second.0,
            };
            let construction = if let Some(cached) = c3_cache.get(&provenance) {
                *cached
            } else {
                let constructed = match construct_coplanar_segment_intersection(
                    first.2, second.2, axis, weld_step,
                ) {
                    ConstructionOutcome::Resolved { value, tier, rho } => {
                        C3Construction::Resolved(VertexProposal {
                            point: value.point,
                            ratio: Some(value.first_ratio),
                            second_ratio: Some(value.second_ratio),
                            tier,
                            rho,
                        })
                    }
                    ConstructionOutcome::Deferred { rho } => C3Construction::Deferred { rho },
                };
                c3_cache.insert(provenance.clone(), constructed);
                constructed
            };
            match construction {
                C3Construction::Resolved(proposal) => {
                    if insert_vertex_proposal(
                        proposals,
                        proposal_owners,
                        provenance.clone(),
                        proposal,
                        &[a.id, b.id],
                        weld_step,
                    )? {
                        inserted_provenances.push(provenance.clone());
                        if proposal.tier == PrecisionTier::DoubleDouble {
                            stats.precision_escalations += 1;
                        }
                    }
                    points.push(SymbolicNode::Registry(provenance));
                }
                C3Construction::Deferred { rho } => {
                    required_deferred = true;
                    stats.precision_floor_routes += 1;
                    degraded.push(degraded_neighborhood(
                        DegradedReason::PrecisionFloor,
                        &[a.id, b.id],
                        vec![provenance],
                        first.2.into_iter().chain(second.2).collect(),
                        Some(rho),
                    ));
                }
            }
        }
    }
    if required_deferred {
        for provenance in inserted_provenances {
            proposals.remove(&provenance);
            proposal_owners.remove(&provenance);
        }
        points.retain(|point| matches!(point, SymbolicNode::Source(_)));
        sort_symbolic_nodes(&mut points, vertices, proposals, weld_step);
        points.dedup();
        return Ok(Some((
            CoplanarRelation {
                triangles: [a.id.min(b.id), a.id.max(b.id)],
                case: CoincidenceCase::C3,
                area_overlap: true,
                resolved: false,
                contact_points: points,
            },
            CoincidenceCase::C3,
        )));
    }
    sort_symbolic_nodes(&mut points, vertices, proposals, weld_step);
    points.dedup_by(|left, right| {
        symbolic_node_key(left, vertices, proposals, weld_step)
            == symbolic_node_key(right, vertices, proposals, weld_step)
    });
    if points.is_empty() {
        return Ok(None);
    }
    let has_area = symbolic_points_have_area(&points, vertices, proposals, axis);
    let topological_area = strict_inside || strict_crossings >= 2;
    if topological_area && !has_area {
        degraded.push(degraded_neighborhood(
            DegradedReason::QuantizedOrderAmbiguity,
            &[a.id, b.id],
            points
                .iter()
                .filter_map(|point| match point {
                    SymbolicNode::Registry(provenance) => Some(provenance.clone()),
                    SymbolicNode::Source(_) => None,
                })
                .collect(),
            points
                .iter()
                .map(|point| symbolic_node_point(point, vertices, proposals))
                .collect(),
            None,
        ));
    }
    let case = if has_area || topological_area {
        CoincidenceCase::C3
    } else if points.len() >= 2 {
        CoincidenceCase::C4
    } else {
        CoincidenceCase::C5
    };
    Ok(Some((
        CoplanarRelation {
            triangles: [a.id.min(b.id), a.id.max(b.id)],
            case,
            area_overlap: has_area || topological_area,
            resolved: true,
            contact_points: points,
        },
        case,
    )))
}

// AI-FUNC-SUMMARY: Select deterministic extreme endpoints for a C4 contact relation and build its pending intersection curve; returns segment or None; side effects: none.
fn contact_curve_for_relation(
    relation: &CoplanarRelation,
    a: &SourceTriangle,
    b: &SourceTriangle,
    vertices: &[Vec3],
    proposals: &BTreeMap<IsectProv, VertexProposal>,
) -> Option<PendingCurveSegment> {
    if relation.contact_points.len() < 2 {
        return None;
    }
    let mut points = relation.contact_points.clone();
    points.sort_by(|left, right| {
        let left = symbolic_node_point(left, vertices, proposals);
        let right = symbolic_node_point(right, vertices, proposals);
        left.x
            .total_cmp(&right.x)
            .then_with(|| left.y.total_cmp(&right.y))
            .then_with(|| left.z.total_cmp(&right.z))
    });
    Some(PendingCurveSegment {
        nodes: [points[0].clone(), points[points.len() - 1].clone()],
        triangles: SmallVec::from_slice(&relation.triangles),
        components: pair_components(a, b),
    })
}

// AI-FUNC-SUMMARY: Test whether symbolic C1/C3 provenance is incident to one defining source edge; returns bool; side effects: none.
fn provenance_uses_edge(provenance: &IsectProv, edge: EdgeId) -> bool {
    match provenance {
        IsectProv::EdgeTri { edge: defining, .. } => *defining == edge,
        IsectProv::EdgeEdge { first, second } => *first == edge || *second == edge,
        IsectProv::TriTriTri(_) => false,
    }
}

// AI-FUNC-SUMMARY: Report one arrangement stage's wall time when `RUSTMSPT_TIME_STAGES` is set; returns a fresh Instant; side effects: writes to stderr.
fn time_arrange_stage(name: &str, started: std::time::Instant) -> std::time::Instant {
    if std::env::var_os("RUSTMSPT_TIME_STAGES").is_some() {
        eprintln!("[S2-TIME] {name} {:?}", started.elapsed());
    }
    std::time::Instant::now()
}

// AI-FUNC-SUMMARY:
// Purpose: The defining edges a provenance can contribute an ordering ratio for.
// Returns: 0, 1 or 2 edges (`EdgeTri` has one, `EdgeEdge` two, `TriTriTri` none).
// Side effects: None.
// Notes: Mirrors `proposal_ratio_for_edge`'s match exactly; it exists so the edge-point index can
//   be built by walking the registry once instead of rescanning it per source triangle.
fn alias_edges(provenance: &IsectProv) -> SmallVec<[EdgeId; 2]> {
    match provenance {
        IsectProv::EdgeTri { edge, .. } => SmallVec::from_slice(&[*edge]),
        IsectProv::EdgeEdge { first, second, .. } => SmallVec::from_slice(&[*first, *second]),
        _ => SmallVec::new(),
    }
}

// AI-FUNC-SUMMARY: Return the retained determinant ratio of a C1/C3 proposal on one defining source edge; returns ratio or None; side effects: none.
fn proposal_ratio_for_edge(
    provenance: &IsectProv,
    proposal: &VertexProposal,
    edge: EdgeId,
) -> Option<DeterminantRatio> {
    match provenance {
        IsectProv::EdgeTri { edge: defining, .. } if *defining == edge => proposal.ratio,
        IsectProv::EdgeEdge { first, .. } if *first == edge => proposal.ratio,
        IsectProv::EdgeEdge { second, .. } if *second == edge => proposal.second_ratio,
        _ => None,
    }
}

// AI-FUNC-SUMMARY: Return a monotone dominant-coordinate key along a directed segment for deterministic fallback ordering; returns f64; side effects: none.
fn segment_parameter_key(point: Vec3, start: Vec3, end: Vec3) -> f64 {
    let delta = end.sub(start);
    if delta.x.abs() >= delta.y.abs() && delta.x.abs() >= delta.z.abs() {
        (point.x - start.x) * delta.x.signum()
    } else if delta.y.abs() >= delta.z.abs() {
        (point.y - start.y) * delta.y.signum()
    } else {
        (point.z - start.z) * delta.z.signum()
    }
}

// AI-FUNC-SUMMARY: Test exact represented-value collinearity and closed-range membership on a 3D segment; returns bool; side effects: none.
fn point_on_segment(point: Vec3, start: Vec3, end: Vec3) -> bool {
    let delta = end.sub(start);
    let offset = point.sub(start);
    let cross = delta.cross(offset);
    if cross.x != 0.0 || cross.y != 0.0 || cross.z != 0.0 {
        return false;
    }
    let projection = offset.dot(delta);
    projection >= 0.0 && projection <= delta.dot(delta)
}

// AI-FUNC-SUMMARY: Test whether a committed snap-rounded point lies within q of a closed segment; returns bool; side effects: none.
fn point_within_segment_weld_band(point: Vec3, start: Vec3, end: Vec3, weld_step: f64) -> bool {
    if point_on_segment(point, start, end) {
        return true;
    }
    let delta = end.sub(start);
    let length2 = delta.dot(delta);
    if !length2.is_finite() || length2 <= 0.0 {
        return point.sub(start).dot(point.sub(start)) <= weld_step * weld_step;
    }
    let parameter = point.sub(start).dot(delta) / length2;
    if !parameter.is_finite() || !(0.0..=1.0).contains(&parameter) {
        return false;
    }
    let closest = start.add(delta.scale(parameter));
    let residual = point.sub(closest);
    residual.dot(residual) <= weld_step * weld_step
}

// AI-FUNC-SUMMARY: Resolve a symbolic source/registry point to its committed output node after alias collection; returns node id or InvalidMesh; side effects: none.
fn resolve_symbolic_output_node(
    symbolic: &SymbolicNode,
    source_node_map: &[usize],
    registry_node_by_provenance: &BTreeMap<IsectProv, usize>,
) -> Result<usize> {
    match symbolic {
        SymbolicNode::Source(node) => source_node_map.get(*node).copied().ok_or_else(|| {
            RustMsptError::InvalidMesh(format!(
                "[ARR-RESID] symbolic source node {node} is outside the source map"
            ))
        }),
        SymbolicNode::Registry(provenance) => registry_node_by_provenance
            .get(provenance)
            .copied()
            .ok_or_else(|| {
                RustMsptError::InvalidMesh(format!(
                    "[ARR-RESID] symbolic registry node {provenance:?} was not committed"
                ))
            }),
    }
}

// AI-FUNC-SUMMARY: Return committed provenance-edge and relation-incident source nodes on one source edge in exact determinant-ratio order; returns deduplicated node sequence; side effects: none.
#[allow(clippy::too_many_arguments)]
fn atomic_edge_nodes(
    triangle: &SourceTriangle,
    edge: EdgeId,
    canonical: &CanonicalInput,
    source_node_map: &[usize],
    registry: &IntersectionRegistry,
    proposals: &BTreeMap<IsectProv, VertexProposal>,
    incident_nodes: &BTreeSet<usize>,
    node_points: &[Vec3],
) -> Vec<usize> {
    let mut source_endpoints = None;
    for index in 0..3 {
        let next = (index + 1) % 3;
        if EdgeId::new(triangle.stable_nodes[index], triangle.stable_nodes[next]) == edge {
            let stable_first = edge.0 as usize;
            let stable_second = edge.1 as usize;
            source_endpoints = Some([
                source_node_map[canonical.stable_to_surface[stable_first]],
                source_node_map[canonical.stable_to_surface[stable_second]],
            ]);
            break;
        }
    }
    let Some(endpoints) = source_endpoints else {
        return Vec::new();
    };
    let start_point = node_points[endpoints[0]];
    let end_point = node_points[endpoints[1]];
    let mut interior: Vec<(usize, Option<DeterminantRatio>, Option<IsectProv>)> = Vec::new();
    for vertex in &registry.vertices {
        for alias in &vertex.aliases {
            if let Some(ratio) = proposal_ratio_for_edge(alias, &proposals[alias], edge) {
                interior.push((vertex.node, Some(ratio), Some(alias.clone())));
            }
        }
    }
    for node in incident_nodes.iter().copied() {
        if node != endpoints[0]
            && node != endpoints[1]
            && point_on_segment(node_points[node], start_point, end_point)
        {
            interior.push((node, None, None));
        }
    }
    interior.sort_by(|left, right| {
        match (left.1, right.1) {
            (Some(a), Some(b)) => a.compare(b),
            _ => std::cmp::Ordering::Equal,
        }
        .then_with(|| {
            segment_parameter_key(node_points[left.0], start_point, end_point).total_cmp(
                &segment_parameter_key(node_points[right.0], start_point, end_point),
            )
        })
        .then_with(|| left.2.cmp(&right.2))
        .then_with(|| left.0.cmp(&right.0))
    });
    let mut nodes = vec![endpoints[0]];
    nodes.extend(interior.into_iter().map(|entry| entry.0));
    nodes.push(endpoints[1]);
    nodes.dedup();
    nodes
}

// AI-FUNC-SUMMARY: Build common source-edge/contact constraints and contact curves from only relation/provenance-incident nodes; returns per-triangle constraints/nodes and curves; side effects: appends collapsed-contact degradation.
#[allow(clippy::too_many_arguments, clippy::type_complexity)]
fn build_overlay_constraints(
    canonical: &CanonicalInput,
    source_vertices: &[Vec3],
    node_points: &[Vec3],
    source_node_map: &[usize],
    registry: &IntersectionRegistry,
    proposals: &BTreeMap<IsectProv, VertexProposal>,
    proposal_owners: &ProposalOwners,
    relations: &[CoplanarRelation],
    pending_curves: &[PendingCurveSegment],
    pending_points: &[PendingPointFeature],
    weld_step: f64,
    degraded: &mut Vec<DegradedNeighborhood>,
) -> Result<(
    BTreeMap<TriId, BTreeSet<(usize, usize)>>,
    BTreeMap<TriId, BTreeSet<usize>>,
    Vec<ArrangedCurve>,
)> {
    let triangle_by_id: BTreeMap<TriId, &SourceTriangle> = canonical
        .triangles
        .iter()
        .map(|triangle| (triangle.id, triangle))
        .collect();
    let registry_node_by_provenance: BTreeMap<IsectProv, usize> = registry
        .vertices
        .iter()
        .flat_map(|vertex| {
            vertex
                .aliases
                .iter()
                .cloned()
                .map(move |alias| (alias, vertex.node))
        })
        .collect();
    let mut constraints: BTreeMap<TriId, BTreeSet<(usize, usize)>> = BTreeMap::new();
    let mut extra_nodes: BTreeMap<TriId, BTreeSet<usize>> = BTreeMap::new();
    let mut curve_components: BTreeMap<(usize, usize), BTreeSet<i32>> = BTreeMap::new();

    for point in pending_points {
        let node = resolve_symbolic_output_node(
            &point.node,
            source_node_map,
            &registry_node_by_provenance,
        )?;
        for triangle in &point.triangles {
            extra_nodes.entry(*triangle).or_default().insert(node);
        }
    }
    let mut sorted_relations = relations.to_vec();
    sorted_relations.sort_by_key(|relation| (relation.triangles, relation.case));
    for relation in &sorted_relations {
        if !relation.resolved {
            continue;
        }
        for point in &relation.contact_points {
            let node =
                resolve_symbolic_output_node(point, source_node_map, &registry_node_by_provenance)?;
            for triangle in relation.triangles {
                extra_nodes.entry(triangle).or_default().insert(node);
            }
        }
    }
    for curve in pending_curves {
        let nodes = [
            resolve_symbolic_output_node(
                &curve.nodes[0],
                source_node_map,
                &registry_node_by_provenance,
            )?,
            resolve_symbolic_output_node(
                &curve.nodes[1],
                source_node_map,
                &registry_node_by_provenance,
            )?,
        ];
        if nodes[0] == nodes[1] {
            degraded.push(degraded_neighborhood(
                DegradedReason::CollapsedContactSegment,
                &curve.triangles,
                curve
                    .nodes
                    .iter()
                    .filter_map(|node| match node {
                        SymbolicNode::Registry(provenance) => Some(provenance.clone()),
                        SymbolicNode::Source(_) => None,
                    })
                    .collect(),
                vec![node_points[nodes[0]]],
                None,
            ));
            continue;
        }
        let start = node_points[nodes[0]];
        let end = node_points[nodes[1]];
        let mut atomic_nodes: BTreeSet<usize> = nodes.into_iter().collect();
        for triangle in &curve.triangles {
            for source in triangle_by_id[triangle].nodes {
                let node = source_node_map[source];
                if point_on_segment(node_points[node], start, end) {
                    atomic_nodes.insert(node);
                }
            }
            if let Some(incident) = extra_nodes.get(triangle) {
                atomic_nodes.extend(
                    incident
                        .iter()
                        .copied()
                        .filter(|node| point_on_segment(node_points[*node], start, end)),
                );
            }
        }
        for vertex in &registry.vertices {
            if vertex.aliases.iter().any(|alias| {
                proposal_owners.get(alias).is_some_and(|owners| {
                    curve
                        .triangles
                        .iter()
                        .all(|triangle| owners.contains(triangle))
                })
            }) && point_within_segment_weld_band(node_points[vertex.node], start, end, weld_step)
            {
                atomic_nodes.insert(vertex.node);
            }
        }
        let mut atomic_nodes: Vec<usize> = atomic_nodes.into_iter().collect();
        atomic_nodes.sort_by(|left, right| {
            segment_parameter_key(node_points[*left], start, end)
                .total_cmp(&segment_parameter_key(node_points[*right], start, end))
                .then_with(|| left.cmp(right))
        });
        for atomic in atomic_nodes.windows(2) {
            if atomic[0] == atomic[1] {
                continue;
            }
            let edge = sorted_pair(atomic[0], atomic[1]);
            curve_components
                .entry(edge)
                .or_default()
                .extend(curve.components.iter().copied());
            for triangle in &curve.triangles {
                constraints.entry(*triangle).or_default().insert(edge);
                extra_nodes
                    .entry(*triangle)
                    .or_default()
                    .extend(atomic.iter().copied());
            }
        }
    }

    for relation in sorted_relations {
        let a = triangle_by_id[&relation.triangles[0]];
        let b = triangle_by_id[&relation.triangles[1]];
        if !relation.resolved {
            continue;
        }
        if !relation.area_overlap {
            continue;
        }
        let target_points = |triangle: &SourceTriangle| {
            triangle
                .nodes
                .map(|node| node_points[source_node_map[node]])
        };
        for source in [a, b] {
            let incident_nodes = extra_nodes.get(&source.id).cloned().unwrap_or_default();
            for (edge, _, _) in canonical_source_edges(source, source_vertices) {
                let edge_nodes = atomic_edge_nodes(
                    source,
                    edge,
                    canonical,
                    source_node_map,
                    registry,
                    proposals,
                    &incident_nodes,
                    node_points,
                );
                for segment in edge_nodes.windows(2) {
                    if segment[0] == segment[1] {
                        continue;
                    }
                    let midpoint = node_points[segment[0]]
                        .add(node_points[segment[1]])
                        .scale(0.5);
                    for target in [a, b] {
                        if exact_point_location(midpoint, target_points(target)) >= 0 {
                            constraints
                                .entry(target.id)
                                .or_default()
                                .insert(sorted_pair(segment[0], segment[1]));
                            extra_nodes
                                .entry(target.id)
                                .or_default()
                                .extend(segment.iter().copied());
                        }
                    }
                }
            }
        }
    }

    let curves = curve_components
        .into_iter()
        .map(|((a, b), components)| ArrangedCurve {
            kind: ArrangedCurveKind::Intersection,
            components: components.into_iter().collect(),
            nodes: vec![a, b],
            radial_patches: Vec::new(),
        })
        .collect();
    Ok((constraints, extra_nodes, curves))
}

// AI-FUNC-SUMMARY: Build one single-source face with aligned source/tag orientation vectors; returns ArrangedFace; side effects: none.
fn single_source_face(nodes: [usize; 3], source_triangle: TriId, component: i32) -> ArrangedFace {
    ArrangedFace {
        nodes,
        source_triangle,
        component,
        source_triangles: SmallVec::from_slice(&[source_triangle]),
        source_orientations: SmallVec::from_slice(&[1]),
        components: SmallVec::from_slice(&[component]),
        tag_orientations: SmallVec::from_slice(&[1]),
        box_tagged: false,
    }
}

// AI-FUNC-SUMMARY: Return +1 for cyclic and -1 for reversed orientation between two permutations of the same triangle node set; returns i8; side effects: none.
fn face_orientation_against(reference: [usize; 3], candidate: [usize; 3]) -> i8 {
    for shift in 0..3 {
        if candidate[shift] == reference[0]
            && candidate[(shift + 1) % 3] == reference[1]
            && candidate[(shift + 2) % 3] == reference[2]
        {
            return 1;
        }
        if candidate[shift] == reference[0]
            && candidate[(shift + 2) % 3] == reference[1]
            && candidate[(shift + 1) % 3] == reference[2]
        {
            return -1;
        }
    }
    1
}

// AI-FUNC-SUMMARY: Merge geometrically identical atomic child faces into one canonical multi-source/multi-tag face while preserving per-source and per-tag orientation; returns sorted faces; side effects: none.
fn merge_atomic_faces(source_faces: Vec<ArrangedFace>) -> Vec<ArrangedFace> {
    let mut groups: BTreeMap<[usize; 3], Vec<ArrangedFace>> = BTreeMap::new();
    for face in source_faces {
        let mut key = face.nodes;
        key.sort_unstable();
        groups.entry(key).or_default().push(face);
    }
    let mut output = Vec::with_capacity(groups.len());
    for (_, mut group) in groups {
        group.sort_by_key(|face| (face.source_triangle, face.component, face.nodes));
        let reference = group[0].nodes;
        let mut sources: BTreeMap<TriId, i8> = BTreeMap::new();
        let mut tags: BTreeMap<i32, (TriId, i8)> = BTreeMap::new();
        for face in group {
            let orientation = face_orientation_against(reference, face.nodes);
            for source in face.source_triangles {
                sources.entry(source).or_insert(orientation);
            }
            for component in face.components {
                tags.entry(component)
                    .and_modify(|entry| {
                        if face.source_triangle < entry.0 {
                            *entry = (face.source_triangle, orientation);
                        }
                    })
                    .or_insert((face.source_triangle, orientation));
            }
        }
        let Some(source_triangle) = sources.keys().next().copied() else {
            continue;
        };
        let component = tags
            .iter()
            .min_by_key(|(_, (source, _))| *source)
            .map(|(component, _)| *component)
            .unwrap_or(0);
        output.push(ArrangedFace {
            nodes: reference,
            source_triangle,
            component,
            source_triangles: sources.keys().copied().collect(),
            source_orientations: sources.values().copied().collect(),
            components: tags.keys().copied().collect(),
            tag_orientations: tags.values().map(|(_, orientation)| *orientation).collect(),
            box_tagged: false,
        });
    }
    output.sort_by_key(|face| {
        let mut nodes = face.nodes;
        nodes.sort_unstable();
        (face.source_triangle, nodes, face.components.clone())
    });
    output
}

// AI-FUNC-SUMMARY: Decode the one- or two-component identity of a geometric coincidence event; returns a canonical pair, using (X,X) for self-overlap; side effects: none.
fn event_component_pair(event: &CoincidenceEvent) -> Option<(i32, i32)> {
    match event.components.as_slice() {
        [] => None,
        [component] => Some((*component, *component)),
        [first, second, ..] => Some(((*first).min(*second), (*first).max(*second))),
    }
}

// AI-FUNC-SUMMARY: Return source triangle ids on one atomic face that belong to a selected component; returns sorted ids; side effects: none.
fn face_sources_for_component(
    face: &ArrangedFace,
    component: i32,
    component_by_source: &BTreeMap<TriId, i32>,
) -> Vec<TriId> {
    face.source_triangles
        .iter()
        .copied()
        .filter(|source| component_by_source.get(source) == Some(&component))
        .collect()
}

// AI-FUNC-SUMMARY: Classify one atomic face as shared/exclusive for a component pair and return its local relative orientation when shared; returns (shared, exclusive, orientation); side effects: none.
fn atomic_face_pair_state(
    face: &ArrangedFace,
    pair: (i32, i32),
    component_by_source: &BTreeMap<TriId, i32>,
) -> (bool, bool, Option<i8>) {
    if pair.0 != pair.1 {
        let left = face.components.iter().position(|value| *value == pair.0);
        let right = face.components.iter().position(|value| *value == pair.1);
        return match (left, right) {
            (Some(left), Some(right)) => (
                true,
                false,
                Some(face.tag_orientations[left] * face.tag_orientations[right]),
            ),
            (Some(_), None) | (None, Some(_)) => (false, true, None),
            (None, None) => (false, false, None),
        };
    }

    let sources = face_sources_for_component(face, pair.0, component_by_source);
    if sources.len() >= 2 {
        let mut orientations = sources.iter().filter_map(|source| {
            face.source_triangles
                .iter()
                .position(|candidate| candidate == source)
                .map(|index| face.source_orientations[index])
        });
        let first = orientations.next().unwrap_or(1);
        let second = orientations.next().unwrap_or(first);
        (true, false, Some(first * second))
    } else if sources.len() == 1 {
        (false, true, None)
    } else {
        (false, false, None)
    }
}

// AI-FUNC-SUMMARY: Reclassify connected shared atomic patches independently as C1/C2 or C3 so disconnected/exclusive geometry cannot block promotion; side effects: replaces geometric events while retaining C7/C8/C9 semantics.
fn promote_patch_events(
    faces: &[ArrangedFace],
    canonical: &CanonicalInput,
    events: &mut Vec<CoincidenceEvent>,
) {
    let component_by_source: BTreeMap<TriId, i32> = canonical
        .triangles
        .iter()
        .map(|triangle| (triangle.id, triangle.component))
        .collect();
    let geometric_pairs: BTreeSet<(i32, i32)> = events
        .iter()
        .filter(|event| {
            matches!(
                event.case,
                CoincidenceCase::C1 | CoincidenceCase::C2 | CoincidenceCase::C3
            )
        })
        .filter_map(event_component_pair)
        .collect();
    let mut edge_faces: BTreeMap<(usize, usize), Vec<usize>> = BTreeMap::new();
    for (face_index, face) in faces.iter().enumerate() {
        for edge in triangle_edges(face.nodes) {
            edge_faces
                .entry(sorted_pair(edge[0], edge[1]))
                .or_default()
                .push(face_index);
        }
    }

    for pair in geometric_pairs {
        let shared: BTreeSet<usize> = faces
            .iter()
            .enumerate()
            .filter_map(|(index, face)| {
                atomic_face_pair_state(face, pair, &component_by_source)
                    .0
                    .then_some(index)
            })
            .collect();
        if shared.is_empty() {
            continue;
        }
        events.retain(|event| {
            !matches!(
                event.case,
                CoincidenceCase::C1 | CoincidenceCase::C2 | CoincidenceCase::C3
            ) || event_component_pair(event) != Some(pair)
        });

        let mut unvisited = shared.clone();
        while let Some(seed) = unvisited.iter().next().copied() {
            let mut stack = vec![seed];
            let mut patch = BTreeSet::new();
            unvisited.remove(&seed);
            while let Some(face_index) = stack.pop() {
                patch.insert(face_index);
                for edge in triangle_edges(faces[face_index].nodes) {
                    for adjacent in &edge_faces[&sorted_pair(edge[0], edge[1])] {
                        if shared.contains(adjacent) && unvisited.remove(adjacent) {
                            stack.push(*adjacent);
                        }
                    }
                }
            }

            let mut orientation = None;
            let mut orientation_consistent = true;
            let mut has_exclusive_boundary = false;
            let mut local_sources = [BTreeSet::new(), BTreeSet::new()];
            for face_index in &patch {
                let face = &faces[*face_index];
                let current = atomic_face_pair_state(face, pair, &component_by_source).2;
                if orientation.is_some_and(|value| Some(value) != current) {
                    orientation_consistent = false;
                } else if orientation.is_none() {
                    orientation = current;
                }
                for source in face_sources_for_component(face, pair.0, &component_by_source) {
                    local_sources[0].insert(source);
                }
                for source in face_sources_for_component(face, pair.1, &component_by_source) {
                    local_sources[1].insert(source);
                }
                for edge in triangle_edges(face.nodes) {
                    has_exclusive_boundary |= edge_faces[&sorted_pair(edge[0], edge[1])]
                        .iter()
                        .filter(|adjacent| !patch.contains(adjacent))
                        .any(|adjacent| {
                            atomic_face_pair_state(&faces[*adjacent], pair, &component_by_source).1
                        });
                }
            }
            let source_pair = if pair.0 == pair.1 {
                let mut sources = local_sources[0].iter().copied();
                match (sources.next(), sources.next()) {
                    (Some(first), Some(second)) => Some((first, second)),
                    _ => None,
                }
            } else {
                local_sources[0]
                    .iter()
                    .next()
                    .copied()
                    .zip(local_sources[1].iter().next().copied())
            };
            let Some((first, second)) = source_pair else {
                continue;
            };
            let case = if has_exclusive_boundary || !orientation_consistent {
                CoincidenceCase::C3
            } else if orientation == Some(-1) {
                CoincidenceCase::C2
            } else {
                CoincidenceCase::C1
            };
            let components = if pair.0 == pair.1 {
                SmallVec::from_slice(&[pair.0])
            } else {
                SmallVec::from_slice(&[pair.0, pair.1])
            };
            events.push(CoincidenceEvent {
                case,
                entities: [
                    CoincidenceEntity::Triangle(first.min(second)),
                    CoincidenceEntity::Triangle(first.max(second)),
                ],
                components,
            });
        }
    }
}

// AI-FUNC-SUMMARY: Emit C3 curves only on atomic edges separating shared support from exclusive support for an active patch-local C3 pair; returns sorted edge curves; side effects: none.
fn build_c3_boundary_curves(
    faces: &[ArrangedFace],
    canonical: &CanonicalInput,
    events: &[CoincidenceEvent],
) -> Vec<ArrangedCurve> {
    let pairs: BTreeSet<(i32, i32)> = events
        .iter()
        .filter(|event| event.case == CoincidenceCase::C3)
        .filter_map(event_component_pair)
        .collect();
    let component_by_source: BTreeMap<TriId, i32> = canonical
        .triangles
        .iter()
        .map(|triangle| (triangle.id, triangle.component))
        .collect();
    let mut edge_faces: BTreeMap<(usize, usize), Vec<&ArrangedFace>> = BTreeMap::new();
    for face in faces {
        for edge in triangle_edges(face.nodes) {
            edge_faces
                .entry(sorted_pair(edge[0], edge[1]))
                .or_default()
                .push(face);
        }
    }
    let mut curves: BTreeMap<(usize, usize), BTreeSet<i32>> = BTreeMap::new();
    for (edge, incident) in edge_faces {
        for pair in &pairs {
            let shared = incident
                .iter()
                .any(|face| atomic_face_pair_state(face, *pair, &component_by_source).0);
            let exclusive = incident
                .iter()
                .any(|face| atomic_face_pair_state(face, *pair, &component_by_source).1);
            if shared && exclusive {
                let components = curves.entry(edge).or_default();
                components.insert(pair.0);
                components.insert(pair.1);
            }
        }
    }
    curves
        .into_iter()
        .map(|((a, b), components)| ArrangedCurve {
            kind: ArrangedCurveKind::Intersection,
            components: components.into_iter().collect(),
            nodes: vec![a, b],
            radial_patches: Vec::new(),
        })
        .collect()
}

// AI-FUNC-SUMMARY: Resolve pending C5/C6 symbolic points to committed arranged nodes and canonical public records; returns point features; side effects: none.
fn resolve_point_features(
    pending: &[PendingPointFeature],
    source_node_map: &[usize],
    registry: &IntersectionRegistry,
    proposals: &BTreeMap<IsectProv, VertexProposal>,
    source_vertices: &[Vec3],
    weld_step: f64,
) -> Result<Vec<ArrangedPointFeature>> {
    let _ = (proposals, source_vertices, weld_step);
    let registry_nodes: BTreeMap<IsectProv, usize> = registry
        .vertices
        .iter()
        .flat_map(|vertex| {
            vertex
                .aliases
                .iter()
                .cloned()
                .map(move |alias| (alias, vertex.node))
        })
        .collect();
    pending
        .iter()
        .map(|feature| {
            Ok(ArrangedPointFeature {
                case: feature.case,
                node: resolve_symbolic_output_node(
                    &feature.node,
                    source_node_map,
                    &registry_nodes,
                )?,
                triangles: feature.triangles.clone(),
                components: feature.components.clone(),
            })
        })
        .collect()
}

// AI-FUNC-SUMMARY: Build stable source vertex and triangle ids by NodeKey/component order before pair processing; returns CanonicalInput; side effects: none.
fn canonicalize_input(
    surface: &ConditionedSurface,
    normalized: &[Vec3],
    weld_step: f64,
) -> CanonicalInput {
    let mut vertex_order: Vec<usize> = (0..normalized.len()).collect();
    vertex_order.sort_by_key(|index| (node_key(normalized[*index], weld_step), *index));
    let mut stable_vertex_ids = vec![0u32; normalized.len()];
    let mut stable_to_surface = Vec::with_capacity(normalized.len());
    for (stable, source) in vertex_order.into_iter().enumerate() {
        stable_vertex_ids[source] = stable as u32;
        stable_to_surface.push(source);
    }
    let mut records: Vec<CanonicalFaceRecord> = surface
        .faces
        .iter()
        .enumerate()
        .map(|(index, face)| {
            let mut keys = face.map(|node| node_key(normalized[node], weld_step));
            keys.sort_unstable();
            (index, surface.source_component[index], keys)
        })
        .collect();
    records.sort_by_key(|(index, component, keys)| (*component, *keys, *index));
    let triangles = records
        .into_iter()
        .enumerate()
        .map(|(id, (surface_index, component, _))| {
            let nodes = surface.faces[surface_index];
            SourceTriangle {
                id: id as TriId,
                nodes,
                stable_nodes: nodes.map(|node| stable_vertex_ids[node]),
                component,
            }
        })
        .collect();
    CanonicalInput {
        triangles,
        stable_to_surface,
    }
}

// AI-FUNC-SUMMARY: True only for same-component triangles whose sole intersection is one complete shared edge, retaining same-side coplanar overlaps for overlay; returns bool; side effects: none.
fn source_triangles_share_only_conforming_edge(
    a: &SourceTriangle,
    b: &SourceTriangle,
    vertices: &[Vec3],
) -> bool {
    if a.component != b.component {
        return false;
    }
    let shared: Vec<usize> = a
        .nodes
        .iter()
        .copied()
        .filter(|node| b.nodes.contains(node))
        .collect();
    if shared.len() != 2 {
        return false;
    }
    let Some(a_third) = a.nodes.iter().copied().find(|node| !shared.contains(node)) else {
        return false;
    };
    let Some(b_third) = b.nodes.iter().copied().find(|node| !shared.contains(node)) else {
        return false;
    };
    let atri = a.nodes.map(|node| vertices[node]);
    let btri = b.nodes.map(|node| vertices[node]);
    if orient3d_filtered(atri[0], atri[1], atri[2], vertices[b_third]).0 != 0
        || orient3d_filtered(btri[0], btri[1], btri[2], vertices[a_third]).0 != 0
    {
        return true;
    }
    let axis = best_projection_axis(atri[0], atri[1], atri[2]);
    let a_side = orient2d_axis(
        vertices[shared[0]],
        vertices[shared[1]],
        vertices[a_third],
        axis,
    );
    let b_side = orient2d_axis(
        vertices[shared[0]],
        vertices[shared[1]],
        vertices[b_third],
        axis,
    );
    strict_straddle(a_side, b_side)
}

// AI-FUNC-SUMMARY: Exact f64 AABB overlap reject for two source triangles; returns bool; side effects: none.
fn triangle_aabbs_overlap(a: &SourceTriangle, b: &SourceTriangle, vertices: &[Vec3]) -> bool {
    let (amin, amax) = triangle_bounds(a, vertices);
    let (bmin, bmax) = triangle_bounds(b, vertices);
    amin.x <= bmax.x
        && amax.x >= bmin.x
        && amin.y <= bmax.y
        && amax.y >= bmin.y
        && amin.z <= bmax.z
        && amax.z >= bmin.z
}

// AI-FUNC-SUMMARY: Compute one source triangle AABB; returns (min,max); side effects: none.
fn triangle_bounds(triangle: &SourceTriangle, vertices: &[Vec3]) -> (Vec3, Vec3) {
    let mut min = vertices[triangle.nodes[0]];
    let mut max = min;
    for node in &triangle.nodes[1..] {
        let point = vertices[*node];
        min = Vec3::new(min.x.min(point.x), min.y.min(point.y), min.z.min(point.z));
        max = Vec3::new(max.x.max(point.x), max.y.max(point.y), max.z.max(point.z));
    }
    (min, max)
}

// AI-FUNC-SUMMARY: Maximum dimension of an AABB; returns f64; side effects: none.
fn aabb_extent(bounds: &(Vec3, Vec3)) -> f64 {
    let dx = bounds.1.x - bounds.0.x;
    let dy = bounds.1.y - bounds.0.y;
    let dz = bounds.1.z - bounds.0.z;
    dx.max(dy).max(dz)
}

// AI-FUNC-SUMMARY: Exact f64 AABB overlap with inflation; returns bool; side effects: none.
fn aabbs_overlap(a: &(Vec3, Vec3), b: &(Vec3, Vec3), inflation: f64) -> bool {
    a.0.x - inflation <= b.1.x + inflation
        && a.1.x + inflation >= b.0.x - inflation
        && a.0.y - inflation <= b.1.y + inflation
        && a.1.y + inflation >= b.0.y - inflation
        && a.0.z - inflation <= b.1.z + inflation
        && a.1.z + inflation >= b.0.z - inflation
}

// AI-FUNC-SUMMARY: Map a world coordinate to a grid cell index; returns i64; side effects: none.
fn cell_index(coordinate: f64, origin: f64, cell_size: f64) -> i64 {
    if cell_size <= 0.0 {
        return 0;
    }
    ((coordinate - origin) / cell_size).floor() as i64
}

// AI-FUNC-SUMMARY: Deterministic hybrid broad phase: median-extent uniform grid with oversized side list and auto-switch to full pairwise; returns sorted index pairs; side effects: prints auto-switch log to stderr.
fn triangle_candidate_pairs(
    triangles: &[SourceTriangle],
    vertices: &[Vec3],
    inflation: f64,
) -> Result<Vec<(usize, usize)>> {
    let n = triangles.len();
    if n < 2 {
        return Ok(Vec::new());
    }

    let bounds: Vec<(Vec3, Vec3)> = triangles
        .iter()
        .map(|triangle| triangle_bounds(triangle, vertices))
        .collect();

    // Small input: full pairwise is cheaper than grid overhead.
    if n < 64 {
        return full_pairwise_aabb(&bounds, inflation);
    }

    // Compute extent statistics.
    let mut extents: Vec<f64> = bounds.iter().map(aabb_extent).collect();
    extents.sort_by(f64::total_cmp);
    let median_extent = extents[n / 2];
    if !median_extent.is_finite() || median_extent <= 0.0 {
        return full_pairwise_aabb(&bounds, inflation);
    }

    // Percentile and oversized fraction for auto-switch.
    let p95_index = (n as f64 * 0.95).ceil() as usize;
    let p95_extent = extents[p95_index.min(n - 1)];
    let oversized_count = extents
        .iter()
        .filter(|extent| **extent > 4.0 * median_extent)
        .count();
    let oversized_fraction = oversized_count as f64 / n as f64;
    let size_ratio = p95_extent / median_extent;

    let oversized_pct = oversized_fraction * 100.0;
    if size_ratio > 10.0 || oversized_fraction > 0.05 {
        eprintln!(
            "[ARR-BROAD] auto-switch to full pairwise: p95/p50={size_ratio:.1}, oversized={oversized_pct:.1}% ({oversized_count}/{n})"
        );
        return full_pairwise_aabb(&bounds, inflation);
    }

    // Uniform grid: cell size = median extent.
    let cell_size = median_extent;
    let origin = bounds
        .iter()
        .map(|bounds| bounds.0.x.min(bounds.0.y).min(bounds.0.z))
        .fold(f64::INFINITY, f64::min);

    let mut grid: BTreeMap<(i64, i64, i64), Vec<usize>> = BTreeMap::new();
    let mut oversized: Vec<usize> = Vec::new();

    for (i, bounds) in bounds.iter().enumerate() {
        let extent = aabb_extent(bounds);
        if extent > 4.0 * cell_size {
            oversized.push(i);
            continue;
        }
        let x0 = cell_index(bounds.0.x - inflation, origin, cell_size);
        let x1 = cell_index(bounds.1.x + inflation, origin, cell_size);
        let y0 = cell_index(bounds.0.y - inflation, origin, cell_size);
        let y1 = cell_index(bounds.1.y + inflation, origin, cell_size);
        let z0 = cell_index(bounds.0.z - inflation, origin, cell_size);
        let z1 = cell_index(bounds.1.z + inflation, origin, cell_size);
        for cx in x0..=x1 {
            for cy in y0..=y1 {
                for cz in z0..=z1 {
                    grid.entry((cx, cy, cz)).or_default().push(i);
                }
            }
        }
    }

    let mut pairs = BTreeSet::new();
    for (i, bounds_i) in bounds.iter().enumerate() {
        let extent = aabb_extent(bounds_i);
        if extent > 4.0 * cell_size {
            // Oversized triangles are checked against all others.
            for (j, bounds_j) in bounds.iter().enumerate() {
                if j < i && aabbs_overlap(bounds_j, bounds_i, inflation) {
                    pairs.insert((j, i));
                }
            }
            continue;
        }
        // Query grid cells overlapping this triangle's inflated AABB.
        let x0 = cell_index(bounds_i.0.x - inflation, origin, cell_size);
        let x1 = cell_index(bounds_i.1.x + inflation, origin, cell_size);
        let y0 = cell_index(bounds_i.0.y - inflation, origin, cell_size);
        let y1 = cell_index(bounds_i.1.y + inflation, origin, cell_size);
        let z0 = cell_index(bounds_i.0.z - inflation, origin, cell_size);
        let z1 = cell_index(bounds_i.1.z + inflation, origin, cell_size);
        for cx in x0..=x1 {
            for cy in y0..=y1 {
                for cz in z0..=z1 {
                    if let Some(cell_tris) = grid.get(&(cx, cy, cz)) {
                        for &j in cell_tris {
                            if j < i && aabbs_overlap(&bounds[j], bounds_i, inflation) {
                                pairs.insert((j, i));
                            }
                        }
                    }
                }
            }
        }
        // Also check against oversized triangles.
        for &j in &oversized {
            if j < i && aabbs_overlap(&bounds[j], bounds_i, inflation) {
                pairs.insert((j, i));
            }
        }
    }

    Ok(pairs.into_iter().collect())
}

// AI-FUNC-SUMMARY: Full O(n^2) pairwise AABB overlap check as deterministic fallback; returns sorted index pairs; side effects: none.
fn full_pairwise_aabb(bounds: &[(Vec3, Vec3)], inflation: f64) -> Result<Vec<(usize, usize)>> {
    let mut pairs = BTreeSet::new();
    for (i, bounds_i) in bounds.iter().enumerate() {
        for (j, bounds_j) in bounds.iter().enumerate().take(i) {
            if aabbs_overlap(bounds_j, bounds_i, inflation) {
                pairs.insert((j, i));
            }
        }
    }
    Ok(pairs.into_iter().collect())
}

// AI-FUNC-SUMMARY: Exact-sign non-coplanar narrow phase using canonical cached C1 constructions and q-conservative opposite-triangle acceptance; returns PairIntersection; side effects: populates the construction cache only.
fn intersect_source_triangles(
    a: &SourceTriangle,
    b: &SourceTriangle,
    vertices: &[Vec3],
    weld_step: f64,
    c1_cache: &mut BTreeMap<IsectProv, C1Construction>,
) -> Result<PairIntersection> {
    let atri = a.nodes.map(|node| vertices[node]);
    let btri = b.nodes.map(|node| vertices[node]);
    let signs_a = atri.map(|point| orient3d_filtered(btri[0], btri[1], btri[2], point).0);
    let signs_b = btri.map(|point| orient3d_filtered(atri[0], atri[1], atri[2], point).0);
    if all_one_side(signs_a) || all_one_side(signs_b) {
        return Ok(PairIntersection::Disjoint);
    }
    if signs_a.iter().all(|sign| *sign == 0) && signs_b.iter().all(|sign| *sign == 0) {
        return Ok(PairIntersection::Coplanar);
    }
    if signs_a.contains(&0) || signs_b.contains(&0) {
        let mut candidates = Vec::new();
        let mut deferred = collect_edge_candidates(
            a,
            b,
            atri,
            btri,
            signs_a,
            weld_step,
            c1_cache,
            &mut candidates,
        );
        deferred.extend(collect_edge_candidates(
            b,
            a,
            btri,
            atri,
            signs_b,
            weld_step,
            c1_cache,
            &mut candidates,
        ));
        if !deferred.is_empty() {
            deferred.sort_by(|left, right| {
                left.provenance
                    .cmp(&right.provenance)
                    .then_with(|| left.rho.total_cmp(&right.rho))
            });
            deferred.dedup_by(|left, right| left.provenance == right.provenance);
            return Ok(PairIntersection::PrecisionDeferred(deferred));
        }
        candidates.retain(|candidate| {
            let opposite = match &candidate.provenance {
                IsectProv::EdgeTri { triangle, .. } if *triangle == a.id => atri,
                IsectProv::EdgeTri { triangle, .. } if *triangle == b.id => btri,
                _ => return false,
            };
            point_inside_triangle(candidate.value.point, opposite)
        });
        return Ok(PairIntersection::Contact(candidates));
    }

    let mut candidates = Vec::new();
    let mut deferred = collect_edge_candidates(
        a,
        b,
        atri,
        btri,
        signs_a,
        weld_step,
        c1_cache,
        &mut candidates,
    );
    deferred.extend(collect_edge_candidates(
        b,
        a,
        btri,
        atri,
        signs_b,
        weld_step,
        c1_cache,
        &mut candidates,
    ));
    if !deferred.is_empty() {
        deferred.sort_by(|left, right| {
            left.provenance
                .cmp(&right.provenance)
                .then_with(|| left.rho.total_cmp(&right.rho))
        });
        deferred.dedup_by(|left, right| left.provenance == right.provenance);
        return Ok(PairIntersection::PrecisionDeferred(deferred));
    }

    let mut accepted: BTreeMap<IsectProv, CandidatePoint> = BTreeMap::new();
    let mut boundary_contact = false;
    for candidate in candidates {
        let opposite = match &candidate.provenance {
            IsectProv::EdgeTri { triangle, .. } if *triangle == a.id => atri,
            IsectProv::EdgeTri { triangle, .. } if *triangle == b.id => btri,
            _ => {
                return Err(RustMsptError::InvalidMesh(
                    "[ARR-RESID] C1 candidate references a triangle outside its pair".to_string(),
                ));
            }
        };
        match classify_point_in_triangle(candidate.value.point, opposite, weld_step) {
            TriangleContainment::Inside => {
                accepted
                    .entry(candidate.provenance.clone())
                    .or_insert(candidate);
            }
            TriangleContainment::Outside => {}
            TriangleContainment::BoundaryUncertain => {
                if point_inside_triangle(candidate.value.point, opposite) {
                    boundary_contact = true;
                    accepted
                        .entry(candidate.provenance.clone())
                        .or_insert(candidate);
                }
            }
        }
    }
    match accepted.len() {
        0 => return Ok(PairIntersection::Disjoint),
        2 if !boundary_contact => {}
        _ => return Ok(PairIntersection::Contact(accepted.into_values().collect())),
    }
    let mut endpoints = accepted.into_values();
    let (Some(first), Some(second), None) = (endpoints.next(), endpoints.next(), endpoints.next())
    else {
        return Ok(PairIntersection::Contact(Vec::new()));
    };
    if first.provenance == second.provenance
        || node_keys_same_or_adjacent(first.value.point, second.value.point, weld_step)
    {
        return Ok(PairIntersection::Contact(vec![first, second]));
    }
    Ok(PairIntersection::Proper(Box::new([first, second])))
}

// AI-FUNC-SUMMARY: Collect strict-straddle C1 candidates through a canonical EdgeTri cache, retaining provenance/edge targets for every deferred request; returns sorted deferred constructions; side effects: mutates cache/output.
#[allow(clippy::too_many_arguments)]
fn collect_edge_candidates(
    owner: &SourceTriangle,
    other: &SourceTriangle,
    owner_points: [Vec3; 3],
    other_points: [Vec3; 3],
    owner_signs: [i8; 3],
    weld_step: f64,
    c1_cache: &mut BTreeMap<IsectProv, C1Construction>,
    output: &mut Vec<CandidatePoint>,
) -> Vec<DeferredConstruction> {
    let mut deferred = Vec::new();
    for edge_index in 0..3 {
        let next = (edge_index + 1) % 3;
        if owner_signs[edge_index] == owner_signs[next] {
            continue;
        }
        let stable_a = owner.stable_nodes[edge_index];
        let stable_b = owner.stable_nodes[next];
        let (edge, p, q) = if stable_a < stable_b {
            (
                EdgeId(stable_a, stable_b),
                owner_points[edge_index],
                owner_points[next],
            )
        } else {
            (
                EdgeId(stable_b, stable_a),
                owner_points[next],
                owner_points[edge_index],
            )
        };
        let provenance = IsectProv::EdgeTri {
            edge,
            triangle: other.id,
        };
        let construction = if let Some(cached) = c1_cache.get(&provenance) {
            *cached
        } else {
            let constructed =
                match construct_edge_triangle_intersection(other_points, p, q, weld_step) {
                    ConstructionOutcome::Deferred { rho } => C1Construction::Deferred { rho },
                    ConstructionOutcome::Resolved { value, tier, rho } => {
                        C1Construction::Resolved(VertexProposal {
                            point: value.point,
                            ratio: Some(value.ratio),
                            second_ratio: None,
                            tier,
                            rho,
                        })
                    }
                };
            c1_cache.insert(provenance.clone(), constructed);
            constructed
        };
        match construction {
            C1Construction::Deferred { rho } => {
                deferred.push(DeferredConstruction {
                    provenance,
                    points: [p, q],
                    rho,
                });
            }
            C1Construction::Resolved(value) => {
                output.push(CandidatePoint { provenance, value });
            }
        }
    }
    deferred
}

// AI-FUNC-SUMMARY: True when three exact plane signs are strictly all positive or all negative; returns bool; side effects: none.
fn all_one_side(signs: [i8; 3]) -> bool {
    signs.iter().all(|sign| *sign > 0) || signs.iter().all(|sign| *sign < 0)
}

// AI-FUNC-SUMMARY: Exact projected point-in-triangle test including the boundary; returns bool; side effects: none.
fn point_inside_triangle(point: Vec3, triangle: [Vec3; 3]) -> bool {
    let axis = best_projection_axis(triangle[0], triangle[1], triangle[2]);
    let orientation = orient2d_axis(triangle[0], triangle[1], triangle[2], axis);
    if orientation == 0.0 {
        return false;
    }
    let signs = [
        orient2d_axis(triangle[0], triangle[1], point, axis),
        orient2d_axis(triangle[1], triangle[2], point, axis),
        orient2d_axis(triangle[2], triangle[0], point, axis),
    ];
    if orientation > 0.0 {
        signs.iter().all(|sign| *sign >= 0.0)
    } else {
        signs.iter().all(|sign| *sign <= 0.0)
    }
}

// AI-FUNC-SUMMARY: Detect positive-area overlap between two exactly coplanar child triangles using exact point locations and strict edge crossings; returns bool; side effects: none.
fn coplanar_triangles_overlap_area(
    a: &SourceTriangle,
    b: &SourceTriangle,
    vertices: &[Vec3],
) -> bool {
    let atri = a.nodes.map(|node| vertices[node]);
    let btri = b.nodes.map(|node| vertices[node]);
    if atri
        .iter()
        .any(|point| exact_point_location(*point, btri) > 0)
        || btri
            .iter()
            .any(|point| exact_point_location(*point, atri) > 0)
    {
        return true;
    }
    let axis = best_projection_axis(atri[0], atri[1], atri[2]);
    let mut crossings = 0usize;
    for a_edge in triangle_edges([0, 1, 2]) {
        for b_edge in triangle_edges([0, 1, 2]) {
            let a0 = orient2d_axis(btri[b_edge[0]], btri[b_edge[1]], atri[a_edge[0]], axis);
            let a1 = orient2d_axis(btri[b_edge[0]], btri[b_edge[1]], atri[a_edge[1]], axis);
            let b0 = orient2d_axis(atri[a_edge[0]], atri[a_edge[1]], btri[b_edge[0]], axis);
            let b1 = orient2d_axis(atri[a_edge[0]], atri[a_edge[1]], btri[b_edge[1]], axis);
            if strict_straddle(a0, a1) && strict_straddle(b0, b1) {
                crossings += 1;
            }
        }
    }
    crossings >= 2
}

// AI-FUNC-SUMMARY: Classify an approximate constructed point by exact projected signs, deferring every opposite-triangle boundary whose q-band could contain it; returns TriangleContainment; side effects: none.
fn classify_point_in_triangle(
    point: Vec3,
    triangle: [Vec3; 3],
    weld_step: f64,
) -> TriangleContainment {
    let axis = best_projection_axis(triangle[0], triangle[1], triangle[2]);
    let orientation = orient2d_axis(triangle[0], triangle[1], triangle[2], axis);
    if orientation == 0.0 {
        return TriangleContainment::BoundaryUncertain;
    }
    let mut outside = false;
    let mut uncertain = false;
    for edge in triangle_edges([0, 1, 2]) {
        let sign = orient2d_axis(triangle[edge[0]], triangle[edge[1]], point, axis);
        let a = project_to_2d(triangle[edge[0]], axis);
        let b = project_to_2d(triangle[edge[1]], axis);
        let edge_length = (b.x - a.x).hypot(b.y - a.y);
        let boundary_band = weld_step * edge_length;
        if !sign.is_finite() || !boundary_band.is_finite() || sign.abs() <= boundary_band {
            uncertain = true;
        } else if (orientation > 0.0 && sign < 0.0) || (orientation < 0.0 && sign > 0.0) {
            outside = true;
        }
    }
    if uncertain {
        TriangleContainment::BoundaryUncertain
    } else if outside {
        TriangleContainment::Outside
    } else {
        TriangleContainment::Inside
    }
}

// AI-FUNC-SUMMARY: Detect two constructed points whose equal or adjacent quantization keys make a proper segment endpoint pair ambiguous; returns bool; side effects: none.
fn node_keys_same_or_adjacent(a: Vec3, b: Vec3, weld_step: f64) -> bool {
    let a = node_key(a, weld_step);
    let b = node_key(b, weld_step);
    a.0.abs_diff(b.0) <= 1 && a.1.abs_diff(b.1) <= 1 && a.2.abs_diff(b.2) <= 1
}

// AI-FUNC-SUMMARY: Insert one proposal while enforcing canonical registry identity and accumulating non-normative owner-triangle incidence; returns whether a new key was inserted; side effects: mutates proposal and ownership maps.
fn insert_vertex_proposal(
    proposals: &mut BTreeMap<IsectProv, VertexProposal>,
    proposal_owners: &mut ProposalOwners,
    key: IsectProv,
    proposal: VertexProposal,
    owner_triangles: &[TriId],
    weld_step: f64,
) -> Result<bool> {
    proposal_owners
        .entry(key.clone())
        .or_default()
        .extend(owner_triangles.iter().copied());
    if let Some(existing) = proposals.get(&key) {
        if node_key(existing.point, weld_step) != node_key(proposal.point, weld_step) {
            return Err(RustMsptError::InvalidMesh(format!(
                "[ARR-RESID] provenance {key:?} produced inconsistent weld keys"
            )));
        }
        return Ok(false);
    }
    proposals.insert(key, proposal);
    Ok(true)
}

// AI-FUNC-SUMMARY: Enumerate sparse pair-segment triangle cliques, commit canonical C2 points, and retain DD-floor/contact ambiguity as typed degraded neighborhoods; side effects: mutates proposals/stats/degraded records.
fn collect_triple_points(
    triangles: &[SourceTriangle],
    vertices: &[Vec3],
    weld_step: f64,
    pair_segments: &BTreeMap<(TriId, TriId), PairSegment>,
    proposals: &mut BTreeMap<IsectProv, VertexProposal>,
    proposal_owners: &mut ProposalOwners,
    stats: &mut ArrangementStats,
    degraded: &mut Vec<DegradedNeighborhood>,
) -> Result<()> {
    let mut forward_neighbors: BTreeMap<TriId, BTreeSet<TriId>> = BTreeMap::new();
    for &(a, b) in pair_segments.keys() {
        forward_neighbors.entry(a).or_default().insert(b);
    }
    let triangles_by_id: BTreeMap<TriId, &SourceTriangle> = triangles
        .iter()
        .map(|triangle| (triangle.id, triangle))
        .collect();

    for (a, a_neighbors) in &forward_neighbors {
        for b in a_neighbors {
            let Some(b_neighbors) = forward_neighbors.get(b) else {
                continue;
            };
            for c in a_neighbors.intersection(b_neighbors) {
                let ids = [*a, *b, *c];
                let tris = ids.map(|id| triangles_by_id[&id].nodes.map(|node| vertices[node]));
                let (point, tier, rho) =
                    match construct_three_triangle_intersection(tris, weld_step) {
                        ConstructionOutcome::Deferred { rho } => {
                            stats.precision_floor_routes += 1;
                            degraded.push(degraded_neighborhood(
                                DegradedReason::PrecisionFloor,
                                &ids,
                                vec![IsectProv::TriTriTri(ids)],
                                Vec::new(),
                                Some(rho),
                            ));
                            continue;
                        }
                        ConstructionOutcome::Resolved { value, tier, rho } => (value, tier, rho),
                    };
                let mut all_inside = true;
                let mut boundary_uncertain = false;
                for triangle in tris {
                    match classify_point_in_triangle(point, triangle, weld_step) {
                        TriangleContainment::Inside => {}
                        TriangleContainment::Outside => all_inside = false,
                        TriangleContainment::BoundaryUncertain => boundary_uncertain = true,
                    }
                }
                if boundary_uncertain {
                    stats.contact_degraded += 1;
                    degraded.push(degraded_neighborhood(
                        DegradedReason::QuantizedOrderAmbiguity,
                        &ids,
                        vec![IsectProv::TriTriTri(ids)],
                        vec![point],
                        Some(rho),
                    ));
                    continue;
                }
                if !all_inside {
                    continue;
                }
                let inserted = insert_vertex_proposal(
                    proposals,
                    proposal_owners,
                    IsectProv::TriTriTri(ids),
                    VertexProposal {
                        point,
                        ratio: None,
                        second_ratio: None,
                        tier,
                        rho,
                    },
                    &ids,
                    weld_step,
                )?;
                if inserted {
                    if tier == PrecisionTier::DoubleDouble {
                        stats.precision_escalations += 1;
                    }
                    stats.triple_points += 1;
                }
            }
        }
    }
    Ok(())
}

// AI-FUNC-SUMMARY: Split pair segments at symbolically incident C2 points in deterministic dominant-coordinate order, collapsing q-ambiguous order into typed degraded neighborhoods; returns ordered provenance-key segments; side effects: appends degraded records.
fn split_pair_segments(
    pair_segments: &BTreeMap<(TriId, TriId), PairSegment>,
    proposals: &BTreeMap<IsectProv, VertexProposal>,
    weld_step: f64,
    degraded: &mut Vec<DegradedNeighborhood>,
) -> Result<Vec<(IsectProv, IsectProv, [TriId; 2])>> {
    let mut triples_by_pair: BTreeMap<(TriId, TriId), Vec<IsectProv>> = BTreeMap::new();
    for key in proposals.keys() {
        let IsectProv::TriTriTri(ids) = key else {
            continue;
        };
        for pair in [(ids[0], ids[1]), (ids[0], ids[2]), (ids[1], ids[2])] {
            if pair_segments.contains_key(&pair) {
                triples_by_pair.entry(pair).or_default().push(key.clone());
            }
        }
    }
    let mut output = Vec::new();
    for (pair_key, segment) in pair_segments {
        let start = proposals[&segment.endpoints[0]].point;
        let end = proposals[&segment.endpoints[1]].point;
        let delta = end.sub(start);
        let dominant = if delta.x.abs() >= delta.y.abs() && delta.x.abs() >= delta.z.abs() {
            0
        } else if delta.y.abs() >= delta.z.abs() {
            1
        } else {
            2
        };
        let coordinate = |point: Vec3| match dominant {
            0 => point.x,
            1 => point.y,
            _ => point.z,
        };
        let mut point_keys: BTreeSet<IsectProv> = segment.endpoints.iter().cloned().collect();
        if let Some(triples) = triples_by_pair.get(pair_key) {
            point_keys.extend(triples.iter().cloned());
        }
        let mut points: Vec<IsectProv> = point_keys.into_iter().collect();
        points.sort_by(|left, right| {
            coordinate(proposals[left].point)
                .total_cmp(&coordinate(proposals[right].point))
                .then_with(|| left.cmp(right))
        });
        let mut ordered = Vec::with_capacity(points.len());
        for point in points {
            if let Some(previous) = ordered.last() {
                let separation =
                    coordinate(proposals[&point].point) - coordinate(proposals[previous].point);
                if !separation.is_finite() || separation <= weld_step {
                    degraded.push(degraded_neighborhood(
                        DegradedReason::QuantizedOrderAmbiguity,
                        &segment.triangles,
                        vec![previous.clone(), point.clone()],
                        vec![proposals[previous].point, proposals[&point].point],
                        Some(proposals[previous].rho.min(proposals[&point].rho)),
                    ));
                    continue;
                }
            }
            ordered.push(point);
        }
        for successive in ordered.windows(2) {
            let separation = coordinate(proposals[&successive[1]].point)
                - coordinate(proposals[&successive[0]].point);
            debug_assert!(separation.is_finite() && separation > weld_step);
            output.push((
                successive[0].clone(),
                successive[1].clone(),
                segment.triangles,
            ));
        }
    }
    Ok(output)
}

// AI-FUNC-SUMMARY: Assign registry IDs by canonical provenance-group order before geometric output-node ordering, apply C7 aliases, and build non-collapsed segments; returns node/registry products; side effects: appends typed alias/contact degradation records.
#[allow(clippy::too_many_arguments, clippy::type_complexity)]
fn build_registry_and_nodes(
    surface: &ConditionedSurface,
    normalized: &[Vec3],
    canonical: &CanonicalInput,
    normalization: Normalization,
    weld_step: f64,
    proposals: &BTreeMap<IsectProv, VertexProposal>,
    proposal_owners: &ProposalOwners,
    split_segments: &[(IsectProv, IsectProv, [TriId; 2])],
    source_aliases: &BTreeMap<usize, usize>,
    degraded: &mut Vec<DegradedNeighborhood>,
) -> Result<(Vec<Vec3>, Vec<Vec3>, Vec<usize>, IntersectionRegistry)> {
    let mut groups_by_key: BTreeMap<(i64, i64, i64), Vec<IsectProv>> = BTreeMap::new();
    for (provenance, proposal) in proposals {
        groups_by_key
            .entry(node_key(proposal.point, weld_step))
            .or_default()
            .push(provenance.clone());
    }
    let mut proposal_groups: Vec<((i64, i64, i64), Vec<IsectProv>)> = groups_by_key
        .into_iter()
        .map(|(key, mut aliases)| {
            aliases.sort();
            aliases.dedup();
            (key, aliases)
        })
        .collect();
    proposal_groups.sort_by(|left, right| {
        left.1[0]
            .cmp(&right.1[0])
            .then_with(|| left.0.cmp(&right.0))
    });
    let mut registry_ids = BTreeMap::new();
    for (id, (_, aliases)) in proposal_groups.iter().enumerate() {
        for alias in aliases {
            registry_ids.insert(alias.clone(), id as u32);
        }
    }
    let mut node_points: BTreeMap<(i64, i64, i64), Vec3> = BTreeMap::new();
    for index in &canonical.stable_to_surface {
        let representative = resolve_source_alias(*index, source_aliases);
        let point = normalized[representative];
        node_points
            .entry(node_key(point, weld_step))
            .or_insert(point);
    }
    for proposal in proposals.values() {
        let key = node_key(proposal.point, weld_step);
        node_points.entry(key).or_insert_with(|| {
            Vec3::new(
                key.0 as f64 * weld_step,
                key.1 as f64 * weld_step,
                key.2 as f64 * weld_step,
            )
        });
    }
    let node_ids: BTreeMap<(i64, i64, i64), usize> = node_points
        .keys()
        .enumerate()
        .map(|(id, key)| (*key, id))
        .collect();
    let node_normalized: Vec<Vec3> = node_points.values().copied().collect();
    let vertices: Vec<Vec3> = node_normalized
        .iter()
        .map(|point| normalization.denormalize(*point))
        .collect();
    let source_node_map: Vec<usize> = normalized
        .iter()
        .enumerate()
        .map(|(index, _)| {
            let representative = resolve_source_alias(index, source_aliases);
            node_ids[&node_key(normalized[representative], weld_step)]
        })
        .collect();
    let mut registry_vertices = Vec::with_capacity(proposal_groups.len());
    for (key, aliases) in proposal_groups {
        let alias_points: Vec<Vec3> = aliases.iter().map(|alias| proposals[alias].point).collect();
        if alias_points.windows(2).any(|pair| pair[0] != pair[1]) {
            let triangles: Vec<TriId> = aliases
                .iter()
                .filter_map(|alias| proposal_owners.get(alias))
                .flat_map(|owners| owners.iter().copied())
                .collect();
            degraded.push(degraded_neighborhood(
                DegradedReason::QuantizedOrderAmbiguity,
                &triangles,
                aliases.clone(),
                alias_points,
                None,
            ));
        }
        let id = registry_ids[&aliases[0]];
        let node = node_ids[&key];
        let tier = if aliases
            .iter()
            .any(|alias| proposals[alias].tier == PrecisionTier::DoubleDouble)
        {
            PrecisionTier::DoubleDouble
        } else {
            PrecisionTier::F64
        };
        let rho = aliases
            .iter()
            .map(|alias| proposals[alias].rho)
            .min_by(f64::total_cmp)
            .unwrap_or(0.0);
        registry_vertices.push(RegistryVertex {
            id,
            provenance: aliases[0].clone(),
            aliases,
            point: vertices[node],
            node,
            tier,
            rho,
        });
    }
    registry_vertices.sort_by_key(|vertex| vertex.id);
    let mut segment_map: BTreeMap<SegKey, RegistrySegment> = BTreeMap::new();
    let component_by_triangle: BTreeMap<TriId, i32> = canonical
        .triangles
        .iter()
        .map(|triangle| (triangle.id, triangle.component))
        .collect();
    for (a, b, triangles) in split_segments {
        let va = registry_ids[a];
        let vb = registry_ids[b];
        let mut nodes = [
            registry_vertices[va as usize].node,
            registry_vertices[vb as usize].node,
        ];
        if va > vb {
            nodes.swap(0, 1);
        }
        if nodes[0] == nodes[1] {
            degraded.push(degraded_neighborhood(
                DegradedReason::CollapsedContactSegment,
                triangles,
                vec![a.clone(), b.clone()],
                vec![proposals[a].point, proposals[b].point],
                Some(proposals[a].rho.min(proposals[b].rho)),
            ));
            continue;
        }
        let key = SegKey::new(va, vb, triangles[0], triangles[1]);
        segment_map.entry(key).or_insert(RegistrySegment {
            key,
            nodes,
            triangles: *triangles,
            components: [
                component_by_triangle[&triangles[0]],
                component_by_triangle[&triangles[1]],
            ],
        });
    }
    let _ = surface;
    Ok((
        vertices,
        node_normalized,
        source_node_map,
        IntersectionRegistry {
            vertices: registry_vertices,
            segments: segment_map.into_values().collect(),
        },
    ))
}

// AI-FUNC-SUMMARY: Split every affected source triangle with restricted Spade CDT using exact C1/C3 edge order, pre-registered overlay/contact nodes, and common atomic constraints; returns single-source arranged faces; side effects: mutates stats and typed degradation records.
#[allow(clippy::too_many_arguments)]
fn split_all_triangles(
    canonical: &CanonicalInput,
    node_points: &[Vec3],
    source_node_map: &[usize],
    registry: &IntersectionRegistry,
    proposals: &BTreeMap<IsectProv, VertexProposal>,
    extra_constraints: &BTreeMap<TriId, BTreeSet<(usize, usize)>>,
    extra_nodes: &BTreeMap<TriId, BTreeSet<usize>>,
    weld_step: f64,
    stats: &mut ArrangementStats,
    degraded: &mut Vec<DegradedNeighborhood>,
) -> Result<Vec<ArrangedFace>> {
    // Two indexes replace a full rescan of the registry per source triangle. Both are
    // built by walking the registry in its canonical order, so each list holds exactly
    // the sequence the serial scan produced and every downstream comparison is
    // unchanged. Without them this loop is O(triangles x registry) and dominates the
    // whole pipeline: 534 s of a 587 s run on a 12k-face input.
    let mut segments_by_triangle: BTreeMap<TriId, Vec<&RegistrySegment>> = BTreeMap::new();
    for segment in &registry.segments {
        for triangle in segment.triangles {
            segments_by_triangle
                .entry(triangle)
                .or_default()
                .push(segment);
        }
    }
    type EdgePoint = (usize, Option<DeterminantRatio>, Option<IsectProv>);
    let mut edge_points_by_edge: BTreeMap<EdgeId, Vec<EdgePoint>> = BTreeMap::new();
    for vertex in &registry.vertices {
        for alias in &vertex.aliases {
            let proposal = &proposals[alias];
            for edge in alias_edges(alias) {
                if let Some(ratio) = proposal_ratio_for_edge(alias, proposal, edge) {
                    edge_points_by_edge
                        .entry(edge)
                        .or_default()
                        .push((vertex.node, Some(ratio), Some(alias.clone())));
                }
            }
        }
    }

    let no_segments: Vec<&RegistrySegment> = Vec::new();
    let no_points: Vec<EdgePoint> = Vec::new();
    // R-P1/R-P2: one independent unit of work per source triangle, collected into an
    // indexed buffer and concatenated in triangle order - never in completion order.
    let per_triangle: Vec<Result<(Vec<ArrangedFace>, Vec<DegradedNeighborhood>, bool)>> = canonical
        .triangles
        .par_iter()
        .map(|triangle| {
            let mut output: Vec<ArrangedFace> = Vec::new();
            let mut degraded: Vec<DegradedNeighborhood> = Vec::new();
            let relevant: &Vec<&RegistrySegment> = segments_by_triangle
                .get(&triangle.id)
                .unwrap_or(&no_segments);
            let triangle_extra_constraints = extra_constraints.get(&triangle.id);
            let triangle_extra_nodes = extra_nodes.get(&triangle.id);
            if relevant.is_empty()
                && triangle_extra_constraints.is_none_or(BTreeSet::is_empty)
                && triangle_extra_nodes.is_none_or(BTreeSet::is_empty)
            {
                output.push(single_source_face(
                    triangle.nodes.map(|node| source_node_map[node]),
                    triangle.id,
                    triangle.component,
                ));
                return Ok((output, degraded, false));
            }
        let mut constraints: BTreeSet<(usize, usize)> = BTreeSet::new();
        let mut forbidden_boundary_chords: BTreeSet<(usize, usize)> = BTreeSet::new();
        for edge_index in 0..3 {
            let next = (edge_index + 1) % 3;
            let edge = EdgeId::new(
                triangle.stable_nodes[edge_index],
                triangle.stable_nodes[next],
            );
            let stable_a = triangle.stable_nodes[edge_index] as usize;
            let stable_b = triangle.stable_nodes[next] as usize;
            let source_a = canonical.stable_to_surface[stable_a];
            let source_b = canonical.stable_to_surface[stable_b];
            let (start, end) = if triangle.stable_nodes[edge_index] <= triangle.stable_nodes[next] {
                (source_node_map[source_a], source_node_map[source_b])
            } else {
                (source_node_map[source_b], source_node_map[source_a])
            };
            let edge_start_point = node_points[start];
            let edge_end_point = node_points[end];
            let mut edge_points: Vec<EdgePoint> =
                edge_points_by_edge.get(&edge).unwrap_or(&no_points).clone();
            if let Some(nodes) = triangle_extra_nodes {
                for node in nodes {
                    if *node != start
                        && *node != end
                        && point_on_segment(node_points[*node], edge_start_point, edge_end_point)
                    {
                        edge_points.push((*node, None, None));
                    }
                }
            }
            edge_points.sort_by(|left, right| {
                let exact = match (left.1, right.1) {
                    (Some(a), Some(b)) => a.compare(b),
                    _ => std::cmp::Ordering::Equal,
                };
                exact
                    .then_with(|| {
                        segment_parameter_key(node_points[left.0], edge_start_point, edge_end_point)
                            .total_cmp(&segment_parameter_key(
                                node_points[right.0],
                                edge_start_point,
                                edge_end_point,
                            ))
                    })
                    .then_with(|| left.2.cmp(&right.2))
                    .then_with(|| left.0.cmp(&right.0))
            });
            for pair in edge_points.windows(2) {
                if pair[0].0 != pair[1].0
                    && matches!((pair[0].1, pair[1].1), (Some(a), Some(b)) if a.compare(b).is_eq())
                {
                    degraded.push(degraded_neighborhood(
                        DegradedReason::QuantizedOrderAmbiguity,
                        &[triangle.id],
                        pair.iter().filter_map(|point| point.2.clone()).collect(),
                        vec![node_points[pair[0].0], node_points[pair[1].0]],
                        None,
                    ));
                }
            }
            edge_points.dedup_by_key(|point| point.0);
            let mut sequence = vec![start];
            sequence.extend(edge_points.into_iter().map(|point| point.0));
            sequence.push(end);
            sequence.dedup();
            for i in 0..sequence.len() {
                for j in i + 2..sequence.len() {
                    forbidden_boundary_chords.insert(sorted_pair(sequence[i], sequence[j]));
                }
            }
            for pair in sequence.windows(2) {
                if pair[0] != pair[1] {
                    constraints.insert(sorted_pair(pair[0], pair[1]));
                }
            }
        }
            for segment in relevant {
                constraints.insert(sorted_pair(segment.nodes[0], segment.nodes[1]));
            }
            if let Some(extra) = triangle_extra_constraints {
                constraints.extend(extra.iter().copied());
            }
            let parent_nodes = triangle.nodes.map(|node| source_node_map[node]);
            let children = triangulate_parent_impl(
                parent_nodes,
                &constraints,
                triangle_extra_nodes.cloned().unwrap_or_default(),
                node_points,
                weld_step,
                &forbidden_boundary_chords,
            )?;
            for nodes in children {
                output.push(single_source_face(nodes, triangle.id, triangle.component));
            }
            Ok((output, degraded, true))
        })
        .collect();

    let mut output = Vec::new();
    for result in per_triangle {
        let (faces, mut faces_degraded, was_split) = result?;
        output.extend(faces);
        degraded.append(&mut faces_degraded);
        if was_split {
            stats.split_faces += 1;
        }
    }
    output.sort_by_key(|face| {
        let mut nodes = face.nodes;
        nodes.sort_unstable();
        (face.source_triangle, nodes)
    });
    Ok(output)
}

// AI-FUNC-SUMMARY: Run one projected constrained Delaunay triangulation using only pre-registered points and try_add_constraint, refusing crossings without panic and verifying coverage/area tiling; returns oriented child triangles or InvalidMesh; side effects: none.
pub fn triangulate_parent(
    parent_nodes: [usize; 3],
    constraints: &BTreeSet<(usize, usize)>,
    extra_nodes: BTreeSet<usize>,
    node_points: &[Vec3],
    weld_step: f64,
) -> Result<Vec<[usize; 3]>> {
    triangulate_parent_impl(
        parent_nodes,
        constraints,
        extra_nodes,
        node_points,
        weld_step,
        &BTreeSet::new(),
    )
}

// AI-FUNC-SUMMARY: Execute guarded local CDT while excluding chords that bypass intermediate registry nodes on a split parent edge; returns oriented child triangles or InvalidMesh; side effects: none.
fn triangulate_parent_impl(
    parent_nodes: [usize; 3],
    constraints: &BTreeSet<(usize, usize)>,
    extra_nodes: BTreeSet<usize>,
    node_points: &[Vec3],
    weld_step: f64,
    forbidden_boundary_chords: &BTreeSet<(usize, usize)>,
) -> Result<Vec<[usize; 3]>> {
    let parent = parent_nodes.map(|node| node_points[node]);
    let axis = best_projection_axis(parent[0], parent[1], parent[2]);
    let parent_orientation = orient2d_axis(parent[0], parent[1], parent[2], axis);
    if parent_orientation == 0.0 {
        return Err(RustMsptError::InvalidMesh(
            "[ARR-RESID] local CDT parent is degenerate".to_string(),
        ));
    }
    let mut nodes: BTreeSet<usize> = parent_nodes.into_iter().collect();
    for (a, b) in constraints {
        nodes.insert(*a);
        nodes.insert(*b);
    }
    nodes.extend(extra_nodes);
    let mut cdt = ConstrainedDelaunayTriangulation::<CdtVertex>::new();
    let mut handles = BTreeMap::new();
    for node in nodes {
        let projected = project_point(node_points[node], axis);
        let handle = cdt
            .insert(CdtVertex {
                position: projected,
                node,
            })
            .map_err(|error| {
                RustMsptError::InvalidMesh(format!(
                    "[ARR-RESID] Spade insertion failed for node {node}: {error:?}"
                ))
            })?;
        handles.insert(node, handle);
    }
    for (a, b) in constraints {
        let inserted = cdt.try_add_constraint(handles[a], handles[b]);
        if inserted.is_empty() {
            return Err(RustMsptError::InvalidMesh(format!(
                "[ARR-RESID] crossing or refused local CDT constraint ({a}, {b}); constraints={constraints:?}"
            )));
        }
    }
    let mut children = Vec::new();
    for face in cdt.inner_faces() {
        let mut nodes = face.vertices().map(|vertex| vertex.data().node);
        if triangle_edges(nodes)
            .into_iter()
            .map(|edge| sorted_pair(edge[0], edge[1]))
            .any(|edge| forbidden_boundary_chords.contains(&edge))
        {
            continue;
        }
        let points = nodes.map(|node| node_points[node]);
        let centroid = points[0].add(points[1]).add(points[2]).scale(1.0 / 3.0);
        if !point_inside_triangle(centroid, parent) {
            continue;
        }
        let orientation = orient2d_axis(points[0], points[1], points[2], axis);
        if orientation == 0.0 {
            return Err(RustMsptError::InvalidMesh(
                "[ARR-RESID] local CDT emitted a zero-area child".to_string(),
            ));
        }
        if orientation.signum() != parent_orientation.signum() {
            nodes.swap(1, 2);
        }
        children.push(nodes);
    }
    children.sort_by_key(|nodes| {
        let mut key = *nodes;
        key.sort_unstable();
        key
    });
    if children.is_empty() {
        return Err(RustMsptError::InvalidMesh(
            "[ARR-RESID] local CDT emitted no child triangles".to_string(),
        ));
    }
    let child_edges: BTreeSet<(usize, usize)> = children
        .iter()
        .flat_map(|nodes| triangle_edges(*nodes))
        .map(|edge| sorted_pair(edge[0], edge[1]))
        .collect();
    if let Some(missing) = constraints.iter().find(|edge| !child_edges.contains(edge)) {
        return Err(RustMsptError::InvalidMesh(format!(
            "[ARR-RESID] local CDT omitted constraint edge {missing:?}"
        )));
    }
    let parent_area2 = parent_orientation.abs();
    let child_area2: f64 = children
        .iter()
        .map(|nodes| {
            orient2d_axis(
                node_points[nodes[0]],
                node_points[nodes[1]],
                node_points[nodes[2]],
                axis,
            )
            .abs()
        })
        .sum();
    let projected = parent.map(|point| project_to_2d(point, axis));
    let perimeter = (projected[1].x - projected[0].x).hypot(projected[1].y - projected[0].y)
        + (projected[2].x - projected[1].x).hypot(projected[2].y - projected[1].y)
        + (projected[0].x - projected[2].x).hypot(projected[0].y - projected[2].y);
    let tolerance = 8.0 * weld_step * perimeter + 64.0 * f64::EPSILON * parent_area2;
    if (child_area2 - parent_area2).abs() > tolerance {
        return Err(RustMsptError::InvalidMesh(format!(
            "[ARR-RESID] local CDT child area does not tile its parent: child={child_area2:.17e}, parent={parent_area2:.17e}, tolerance={tolerance:.3e}"
        )));
    }
    Ok(children)
}

// AI-FUNC-SUMMARY: Convert a selected 3D projection into Spade Point2; returns Point2<f64>; side effects: none.
fn project_point(point: Vec3, axis: ProjectionAxis) -> Point2<f64> {
    let point = project_to_2d(point, axis);
    Point2::new(point.x, point.y)
}

// AI-FUNC-SUMMARY: Validate atomic-child/registry/curve incidence and retain residual crossings as typed degradation; returns Ok or invariant InvalidMesh; side effects: appends degraded records.
#[allow(clippy::too_many_arguments)]
fn validate_arrangement(
    faces: &[ArrangedFace],
    vertices: &[Vec3],
    registry: &IntersectionRegistry,
    curves: &[ArrangedCurve],
    point_features: &[ArrangedPointFeature],
    weld_step: f64,
    candidate_inflation: f64,
    degraded: &mut Vec<DegradedNeighborhood>,
) -> Result<()> {
    let mut edges_by_source: BTreeMap<TriId, BTreeSet<(usize, usize)>> = BTreeMap::new();
    let mut edge_faces_by_source: BTreeMap<(TriId, usize, usize), usize> = BTreeMap::new();
    for face in faces {
        let points = face.nodes.map(|node| vertices[node]);
        if orient2d_axis(
            points[0],
            points[1],
            points[2],
            best_projection_axis(points[0], points[1], points[2]),
        ) == 0.0
        {
            return Err(RustMsptError::InvalidMesh(
                "[ARR-RESID] arranged child is degenerate".to_string(),
            ));
        }
        for source_triangle in &face.source_triangles {
            for edge in triangle_edges(face.nodes) {
                let edge = sorted_pair(edge[0], edge[1]);
                edges_by_source
                    .entry(*source_triangle)
                    .or_default()
                    .insert(edge);
                *edge_faces_by_source
                    .entry((*source_triangle, edge.0, edge.1))
                    .or_default() += 1;
            }
        }
    }
    for segment in &registry.segments {
        let edge = sorted_pair(segment.nodes[0], segment.nodes[1]);
        for triangle in segment.triangles {
            if !edges_by_source
                .get(&triangle)
                .map(|edges| edges.contains(&edge))
                .unwrap_or(false)
            {
                return Err(RustMsptError::InvalidMesh(format!(
                    "[ARR-RESID] registry segment {:?} is absent from source triangle {triangle}",
                    segment.key
                )));
            }
            let incidence = edge_faces_by_source
                .get(&(triangle, edge.0, edge.1))
                .copied()
                .unwrap_or(0);
            if incidence != 2 {
                return Err(RustMsptError::InvalidMesh(format!(
                    "[ARR-RESID] registry segment {:?} has {incidence} incident child faces in source triangle {triangle}, expected 2",
                    segment.key
                )));
            }
        }
    }
    let face_edges: BTreeSet<(usize, usize)> = faces
        .iter()
        .flat_map(|face| triangle_edges(face.nodes))
        .map(|edge| sorted_pair(edge[0], edge[1]))
        .collect();
    for curve in curves {
        for edge in curve.nodes.windows(2) {
            let edge = sorted_pair(edge[0], edge[1]);
            if !face_edges.contains(&edge) {
                let incident: Vec<(usize, usize)> = face_edges
                    .iter()
                    .copied()
                    .filter(|candidate| {
                        candidate.0 == edge.0
                            || candidate.1 == edge.0
                            || candidate.0 == edge.1
                            || candidate.1 == edge.1
                    })
                    .collect();
                let incident_points: Vec<(usize, Vec3)> = incident
                    .iter()
                    .flat_map(|candidate| [candidate.0, candidate.1])
                    .collect::<BTreeSet<_>>()
                    .into_iter()
                    .map(|node| (node, vertices[node]))
                    .collect();
                return Err(RustMsptError::InvalidMesh(format!(
                    "[ARR-RESID] arranged curve edge {edge:?} is absent from the atomic face complex; curve points={:?}; incident face edges={incident:?}; incident points={incident_points:?}",
                    [vertices[edge.0], vertices[edge.1]],
                )));
            }
        }
    }
    // Node membership per source triangle, built once. The obvious spelling - scan
    // every face per point feature - is O(features x faces): 50,640 features against
    // 12,183 faces is 1.2 billion checks, and it was 1.2 s of a 3 s run.
    let mut nodes_by_source: BTreeMap<TriId, BTreeSet<usize>> = BTreeMap::new();
    for face in faces {
        for source in &face.source_triangles {
            let entry = nodes_by_source.entry(*source).or_default();
            for node in face.nodes {
                entry.insert(node);
            }
        }
    }
    for feature in point_features {
        for source in &feature.triangles {
            if !nodes_by_source
                .get(source)
                .is_some_and(|nodes| nodes.contains(&feature.node))
            {
                return Err(RustMsptError::InvalidMesh(format!(
                    "[ARR-RESID] {:?} point node {} is absent from source triangle {}",
                    feature.case, feature.node, source
                )));
            }
        }
    }
    let validation_triangles: Vec<SourceTriangle> = faces
        .iter()
        .enumerate()
        .map(|(index, face)| SourceTriangle {
            id: index as TriId,
            nodes: face.nodes,
            stable_nodes: face.nodes.map(|node| node as u32),
            component: face.component,
        })
        .collect();
    let mut explicit_pairs = degraded_triangle_pairs(degraded);
    let candidates = triangle_candidate_pairs(&validation_triangles, vertices, candidate_inflation)?;
    // R-P1/R-P2: the residual-crossing test is evaluated per candidate pair in
    // parallel, then applied serially in candidate order. The serial pass is what
    // keeps `explicit_pairs` - which suppresses repeat reports of one source pair -
    // order-dependent in exactly the way the sequential loop was. `c1_cache` is a pure
    // memo, so a per-thread copy changes hit rate and nothing else.
    let evaluated: Vec<Result<Option<((TriId, TriId), usize, usize)>>> = candidates
        .par_iter()
        .map_init(
            BTreeMap::<IsectProv, C1Construction>::new,
            |c1_cache, (i, j)| {
                let (i, j) = (*i, *j);
                let a = &validation_triangles[i];
                let b = &validation_triangles[j];
                let source_pair = (
                    faces[i].source_triangle.min(faces[j].source_triangle),
                    faces[i].source_triangle.max(faces[j].source_triangle),
                );
                if faces[i].source_triangle == faces[j].source_triangle
                    || faces[i]
                        .source_triangles
                        .iter()
                        .any(|source| faces[j].source_triangles.contains(source))
                    || source_triangles_share_only_conforming_edge(a, b, vertices)
                {
                    return Ok(None);
                }
                let intersection = if triangle_aabbs_overlap(a, b, vertices) {
                    intersect_source_triangles(a, b, vertices, weld_step, c1_cache)?
                } else {
                    PairIntersection::Disjoint
                };
                let residual = matches!(intersection, PairIntersection::Proper(_))
                    || (matches!(intersection, PairIntersection::Coplanar)
                        && coplanar_triangles_overlap_area(a, b, vertices));
                Ok(residual.then_some((source_pair, i, j)))
            },
        )
        .collect();

    for result in evaluated {
        let Some((source_pair, i, j)) = result? else {
            continue;
        };
        if explicit_pairs.contains(&source_pair) {
            continue;
        }
        degraded.push(degraded_neighborhood(
            DegradedReason::ResidualCrossing,
            &[source_pair.0, source_pair.1],
            Vec::new(),
            faces[i]
                .nodes
                .into_iter()
                .chain(faces[j].nodes)
                .map(|node| vertices[node])
                .collect(),
            None,
        ));
        explicit_pairs.insert(source_pair);
    }
    Ok(())
}

// AI-FUNC-SUMMARY: Split S1 curves only at source-triangle/provenance-incident nodes, preserve S1 component ownership, add intersection curves, and map corners/junctions; returns curves and corner-node ids; side effects: appends degraded records.
#[allow(clippy::too_many_arguments)]
fn build_arranged_curves(
    features: &FeatureSet,
    canonical: &CanonicalInput,
    source_node_map: &[usize],
    registry: &IntersectionRegistry,
    faces: &[ArrangedFace],
    vertices: &[Vec3],
    weld_step: f64,
    overlay_curves: &[ArrangedCurve],
    degraded: &mut Vec<DegradedNeighborhood>,
) -> Result<(Vec<ArrangedCurve>, BTreeSet<usize>)> {
    let mut surface_to_stable = vec![0u32; canonical.stable_to_surface.len()];
    for (stable, source) in canonical.stable_to_surface.iter().copied().enumerate() {
        surface_to_stable[source] = stable as u32;
    }

    let mut curves = Vec::new();
    for curve in &features.curves {
        let mut nodes = Vec::new();
        for source_edge in curve.vertices.windows(2) {
            let source_a = source_edge[0];
            let source_b = source_edge[1];
            let start = source_node_map[source_a];
            let end = source_node_map[source_b];
            let start_point = vertices[start];
            let end_point = vertices[end];
            let edge = EdgeId::new(surface_to_stable[source_a], surface_to_stable[source_b]);
            let incident_triangles: BTreeSet<TriId> = canonical
                .triangles
                .iter()
                .filter(|triangle| {
                    triangle.nodes.contains(&source_a) && triangle.nodes.contains(&source_b)
                })
                .map(|triangle| triangle.id)
                .collect();
            let mut edge_nodes: Vec<usize> = faces
                .iter()
                .filter(|face| {
                    face.source_triangles
                        .iter()
                        .any(|triangle| incident_triangles.contains(triangle))
                })
                .flat_map(|face| face.nodes)
                .filter(|node| point_on_segment(vertices[*node], start_point, end_point))
                .collect();
            edge_nodes.extend(registry.vertices.iter().filter_map(|vertex| {
                (vertex
                    .aliases
                    .iter()
                    .any(|provenance| provenance_uses_edge(provenance, edge))
                    && point_within_segment_weld_band(
                        vertices[vertex.node],
                        start_point,
                        end_point,
                        weld_step,
                    ))
                .then_some(vertex.node)
            }));
            edge_nodes.extend([start, end]);
            edge_nodes.sort_by(|left, right| {
                segment_parameter_key(vertices[*left], start_point, end_point)
                    .total_cmp(&segment_parameter_key(
                        vertices[*right],
                        start_point,
                        end_point,
                    ))
                    .then_with(|| left.cmp(right))
            });
            edge_nodes.dedup();
            for node in edge_nodes {
                if nodes.last().copied() != Some(node) {
                    nodes.push(node);
                }
            }
        }
        if nodes.len() < 2 {
            continue;
        }
        let mut components = curve.components.clone();
        components.sort_unstable();
        components.dedup();
        curves.push(ArrangedCurve {
            kind: match curve.kind {
                FeatureEdgeKind::Rim => ArrangedCurveKind::Rim,
                FeatureEdgeKind::Sharp | FeatureEdgeKind::NonManifold => ArrangedCurveKind::Sharp,
            },
            components: components.into_iter().collect(),
            nodes,
            radial_patches: Vec::new(),
        });
    }
    for segment in &registry.segments {
        let mut components: SmallVec<[i32; 2]> = segment.components.into_iter().collect();
        components.sort_unstable();
        components.dedup();
        curves.push(ArrangedCurve {
            kind: ArrangedCurveKind::Intersection,
            components,
            nodes: segment.nodes.to_vec(),
            radial_patches: radial_patch_order(segment, faces, vertices, weld_step, degraded)?,
        });
    }
    curves.extend(overlay_curves.iter().cloned());
    curves.sort_by(|left, right| {
        left.kind
            .cmp(&right.kind)
            .then_with(|| left.components.cmp(&right.components))
            .then_with(|| left.nodes.cmp(&right.nodes))
    });
    let corner_nodes = features
        .corners
        .iter()
        .chain(&features.junctions)
        .map(|node| source_node_map[*node])
        .collect();
    Ok((curves, corner_nodes))
}

// AI-FUNC-SUMMARY: Order child patches around one proper segment with exact orient3d signs, retaining a deterministic sorted cycle plus typed degradation for radial coplanarity; returns arranged-face ids; side effects: appends degraded records.
fn radial_patch_order(
    segment: &RegistrySegment,
    faces: &[ArrangedFace],
    vertices: &[Vec3],
    weld_step: f64,
    degraded: &mut Vec<DegradedNeighborhood>,
) -> Result<Vec<TriId>> {
    let edge = sorted_pair(segment.nodes[0], segment.nodes[1]);
    let mut incident = Vec::new();
    for (face_index, face) in faces.iter().enumerate() {
        if !segment
            .triangles
            .iter()
            .any(|source| face.source_triangles.contains(source))
            || !face.nodes.contains(&edge.0)
            || !face.nodes.contains(&edge.1)
        {
            continue;
        }
        let Some(third) = face
            .nodes
            .iter()
            .copied()
            .find(|node| *node != edge.0 && *node != edge.1)
        else {
            return Err(RustMsptError::InvalidMesh(format!(
                "[ARR-RESID] intersection segment {:?} has a child without a radial third node",
                segment.key
            )));
        };
        for source in segment
            .triangles
            .iter()
            .filter(|source| face.source_triangles.contains(source))
        {
            incident.push((face_index, *source, third));
        }
    }
    if incident.len() < 4 || incident.len() % 2 != 0 {
        return Err(RustMsptError::InvalidMesh(format!(
            "[ARR-RESID] intersection segment {:?} has {} incident child patches; a segment carries \
             two per source surface, so the count must be even and at least 4",
            segment.key,
            incident.len()
        )));
    }
    for source in segment.triangles {
        if incident
            .iter()
            .filter(|(_, triangle, _)| *triangle == source)
            .count()
            != 2
        {
            return Err(RustMsptError::InvalidMesh(format!(
                "[ARR-RESID] intersection segment {:?} is not two-sided in source triangle {source}",
                segment.key
            )));
        }
    }
    incident.sort_by_key(|(face, triangle, third)| {
        (*triangle, node_key(vertices[*third], weld_step), *face)
    });
    let reference = incident[0];
    let Some(sibling) = incident
        .iter()
        .copied()
        .find(|entry| entry.1 == reference.1 && entry.0 != reference.0)
    else {
        return Err(RustMsptError::InvalidMesh(format!(
            "[ARR-RESID] intersection segment {:?} has no opposite patch",
            segment.key
        )));
    };
    let a = vertices[segment.nodes[0]];
    let b = vertices[segment.nodes[1]];
    let reference_point = vertices[reference.2];
    let mut positive = Vec::new();
    let mut negative = Vec::new();
    for entry in incident
        .iter()
        .copied()
        .filter(|entry| entry.1 != reference.1)
    {
        match orient3d_filtered(a, b, reference_point, vertices[entry.2]).0 {
            1 => positive.push(entry),
            -1 => negative.push(entry),
            _ => {
                degraded.push(degraded_neighborhood(
                    DegradedReason::RadiallyCoplanar,
                    &segment.triangles,
                    Vec::new(),
                    vec![a, b, reference_point, vertices[entry.2]],
                    None,
                ));
                let mut fallback: Vec<TriId> =
                    incident.iter().map(|(face, _, _)| *face as TriId).collect();
                fallback.sort_unstable();
                fallback.dedup();
                return Ok(fallback);
            }
        }
    }
    // Sort each half by angle about the segment. Within a half the patches span less
    // than a half-turn, so `orient3d(a, b, third_i, third_j) > 0` - "j is the way round
    // from i" - is a total order there, and ties (two patches at the same angle, which
    // is a coincident pair S2 handles elsewhere) fall back to the node key so the cycle
    // never depends on the order the grid produced candidates in.
    let by_angle = |left: &(usize, TriId, usize), right: &(usize, TriId, usize)| {
        match orient3d_filtered(a, b, vertices[left.2], vertices[right.2]).0 {
            1 => std::cmp::Ordering::Less,
            -1 => std::cmp::Ordering::Greater,
            _ => node_key(vertices[left.2], weld_step)
                .cmp(&node_key(vertices[right.2], weld_step))
                .then(left.0.cmp(&right.0)),
        }
    };
    positive.sort_by(by_angle);
    negative.sort_by(by_angle);

    // The cycle: the reference patch at angle 0, everything in the half-turn after it,
    // its own sibling at angle pi (they share a source triangle, so they are coplanar
    // and the half test cannot place it), then the remaining half-turn.
    //
    // Generalised from the four-patch case on 2026-08-07. Two surfaces crossing
    // transversally give exactly one patch in each half, which is what the previous code
    // required outright - and it rejected the whole arrangement otherwise. Two other
    // configurations are legitimate and were unreachable: **three or more surfaces along
    // one curve**, where each additional surface adds one patch to each half; and a
    // **tangential contact**, where two surfaces meet along a curve without crossing, so
    // both of the other surface's patches land in the same half. The `TestCaseIntersect1`
    // reference dataset - two particles and a void - failed at S2 with "cannot form a
    // four-patch radial cycle" for the second of those, and could not be meshed at all.
    let mut cycle: Vec<usize> = Vec::with_capacity(incident.len());
    cycle.push(reference.0);
    cycle.extend(positive.iter().map(|entry| entry.0));
    cycle.push(sibling.0);
    cycle.extend(negative.iter().map(|entry| entry.0));
    cycle
        .into_iter()
        .map(|face| {
            TriId::try_from(face).map_err(|_| {
                RustMsptError::InvalidMesh(
                    "[ARR-RESID] arranged face id exceeds UInt32".to_string(),
                )
            })
        })
        .collect()
}

// AI-FUNC-SUMMARY: Return component metadata in ascending X order, inferring missing rows from S0 source ids; returns Vec<ArrangeComponent>; side effects: none.
fn sorted_components(
    options: &ArrangeOptions,
    surface: &ConditionedSurface,
) -> Vec<ArrangeComponent> {
    let mut components: BTreeMap<i32, ArrangeComponent> = options
        .components
        .iter()
        .map(|component| (component.x, *component))
        .collect();
    // Closure is a per-component property, so it is tested once per distinct
    // component. `source_component` holds one entry per face, and `or_insert` takes a
    // value rather than a closure - so the obvious spelling ran an O(faces) scan twice
    // per face, which is quadratic and was 26 s of a 30 s run on a 12k-face input.
    let distinct: BTreeSet<i32> = surface.source_component.iter().copied().collect();
    for x in distinct {
        if components.contains_key(&x) {
            continue;
        }
        let closed = source_component_is_closed(surface, x);
        components.insert(
            x,
            ArrangeComponent {
                x,
                priority: x.max(0) as u32,
                kind: u8::from(!closed),
                closed,
            },
        );
    }
    components.into_values().collect()
}

// AI-FUNC-SUMMARY: Append one mixed VTU cell and its end offset; side effects: mutates VtuDoc connectivity/offsets/types.
fn append_cell(doc: &mut VtuDoc, nodes: &[usize], cell_type: u8) {
    doc.connectivity
        .extend(nodes.iter().map(|node| *node as i64));
    doc.offsets.push(doc.connectivity.len() as i64);
    doc.types.push(cell_type);
}

// AI-FUNC-SUMMARY: Append one field-data array with an explicit component count; side effects: mutates VtuDoc field_data.
fn push_field(doc: &mut VtuDoc, name: &str, components: usize, data: ArrayData) {
    doc.field_data.push(DataArray {
        name: name.to_string(),
        components,
        data,
    });
}

// AI-FUNC-SUMMARY: Return the three directed edges of one triangle; returns [[usize;2];3]; side effects: none.
fn triangle_edges(nodes: [usize; 3]) -> [[usize; 2]; 3] {
    [
        [nodes[0], nodes[1]],
        [nodes[1], nodes[2]],
        [nodes[2], nodes[0]],
    ]
}

// AI-FUNC-SUMMARY: Sort one pair of usize values; returns tuple; side effects: none.
fn sorted_pair(a: usize, b: usize) -> (usize, usize) {
    if a <= b {
        (a, b)
    } else {
        (b, a)
    }
}

const BOX_SOURCE_TRIANGLE: TriId = TriId::MAX;

// AI-FUNC-SUMMARY: Return the coordinate of one Vec3 along the given axis without indexing; returns f64; side effects: none.
fn axis_coord(point: Vec3, axis: usize) -> f64 {
    match axis {
        0 => point.x,
        1 => point.y,
        _ => point.z,
    }
}

// AI-FUNC-SUMMARY: Test whether a point is inside one axis-aligned half-space using exact f64 comparison; returns bool; side effects: none.
fn point_inside_halfspace(point: Vec3, axis: usize, value: f64, keep_greater: bool) -> bool {
    let coord = axis_coord(point, axis);
    if keep_greater {
        coord >= value
    } else {
        coord <= value
    }
}

// AI-FUNC-SUMMARY: Compute the exact intersection of a segment with one axis-aligned plane; returns Vec3; side effects: none.
fn intersect_axis_aligned(a: Vec3, b: Vec3, axis: usize, value: f64) -> Vec3 {
    let ca = axis_coord(a, axis);
    let cb = axis_coord(b, axis);
    let t = (value - ca) / (cb - ca);
    a.add(b.sub(a).scale(t))
}

// AI-FUNC-SUMMARY: Clip a convex polygon against one axis-aligned half-space using Sutherland-Hodgman; returns clipped polygon points; side effects: none.
fn clip_polygon_against_halfspace(
    polygon: &[Vec3],
    axis: usize,
    value: f64,
    keep_greater: bool,
) -> Vec<Vec3> {
    if polygon.is_empty() {
        return Vec::new();
    }
    let mut result = Vec::with_capacity(polygon.len());
    for index in 0..polygon.len() {
        let current = polygon[index];
        let next = polygon[(index + 1) % polygon.len()];
        let current_inside = point_inside_halfspace(current, axis, value, keep_greater);
        let next_inside = point_inside_halfspace(next, axis, value, keep_greater);
        if current_inside {
            result.push(current);
            if !next_inside {
                result.push(intersect_axis_aligned(current, next, axis, value));
            }
        } else if next_inside {
            result.push(intersect_axis_aligned(current, next, axis, value));
        }
    }
    result
}

// AI-FUNC-SUMMARY: Clip a triangle against the six half-spaces of an axis-aligned box; returns the clipped convex polygon; side effects: none.
fn clip_triangle_to_box(triangle: [Vec3; 3], domain_min: Vec3, domain_max: Vec3) -> Vec<Vec3> {
    let mut polygon: Vec<Vec3> = triangle.to_vec();
    for (axis, value, keep_greater) in [
        (0usize, domain_min.x, true),
        (0, domain_max.x, false),
        (1, domain_min.y, true),
        (1, domain_max.y, false),
        (2, domain_min.z, true),
        (2, domain_max.z, false),
    ] {
        polygon = clip_polygon_against_halfspace(&polygon, axis, value, keep_greater);
        if polygon.len() < 3 {
            return Vec::new();
        }
    }
    polygon
}

// AI-FUNC-SUMMARY: Fan-triangulate a convex polygon from its first vertex; returns oriented triangles; side effects: none.
fn fan_triangulate(polygon: &[Vec3]) -> Vec<[Vec3; 3]> {
    if polygon.len() < 3 {
        return Vec::new();
    }
    (1..polygon.len() - 1)
        .map(|index| [polygon[0], polygon[index], polygon[index + 1]])
        .collect()
}

// AI-FUNC-SUMMARY: Return the existing or newly inserted vertex index for one point using quantized weld deduplication; returns vertex index; side effects: mutates vertices and node_to_index.
fn get_or_insert_vertex(
    point: Vec3,
    vertices: &mut Vec<Vec3>,
    node_to_index: &mut BTreeMap<(i64, i64, i64), usize>,
    weld_step: f64,
) -> usize {
    let key = node_key(point, weld_step);
    if let Some(&index) = node_to_index.get(&key) {
        index
    } else {
        let index = vertices.len();
        vertices.push(point);
        node_to_index.insert(key, index);
        index
    }
}

// AI-FUNC-SUMMARY: Test whether all three vertices of a face lie exactly on one axis-aligned domain plane; returns the plane index or None; side effects: none.
fn face_on_domain_plane(
    nodes: [usize; 3],
    vertices: &[Vec3],
    domain_min: Vec3,
    domain_max: Vec3,
) -> Option<usize> {
    let planes = [
        (0usize, 0usize, domain_min.x),
        (1, 0, domain_max.x),
        (2, 1, domain_min.y),
        (3, 1, domain_max.y),
        (4, 2, domain_min.z),
        (5, 2, domain_max.z),
    ];
    for (plane_id, axis, value) in planes {
        let all_on = nodes.iter().all(|node| {
            let coord = axis_coord(vertices[*node], axis);
            coord == value
        });
        if all_on {
            return Some(plane_id);
        }
    }
    None
}

// AI-FUNC-SUMMARY: Chain unordered edges into closed loops using adjacency traversal; returns one node sequence per loop; side effects: none.
fn chain_edge_loops(edges: &[(usize, usize)]) -> Vec<Vec<usize>> {
    if edges.is_empty() {
        return Vec::new();
    }
    let mut adjacency: BTreeMap<usize, BTreeSet<usize>> = BTreeMap::new();
    for (a, b) in edges {
        adjacency.entry(*a).or_default().insert(*b);
        adjacency.entry(*b).or_default().insert(*a);
    }
    let mut used: BTreeSet<(usize, usize)> = BTreeSet::new();
    let mut loops = Vec::new();
    let starts: Vec<usize> = adjacency.keys().copied().collect();
    for start in starts {
        while let Some(first) = adjacency.get(&start).and_then(|neighbors| {
            neighbors
                .iter()
                .copied()
                .find(|n| !used.contains(&sorted_pair(start, *n)))
        }) {
            let mut nodes = vec![start];
            let mut prev = start;
            let mut current = first;
            loop {
                used.insert(sorted_pair(prev, current));
                if current == start {
                    break;
                }
                nodes.push(current);
                let next = adjacency.get(&current).and_then(|neighbors| {
                    neighbors
                        .iter()
                        .copied()
                        .find(|n| !used.contains(&sorted_pair(current, *n)))
                });
                let Some(n) = next else {
                    break;
                };
                prev = current;
                current = n;
            }
            if nodes.len() >= 3 {
                loops.push(nodes);
            }
        }
    }
    loops
}

// AI-FUNC-SUMMARY: Compute the signed area of one closed node loop projected onto the plane perpendicular to an axis; returns f64; side effects: none.
fn loop_signed_area(nodes: &[usize], axis: usize, vertices: &[Vec3]) -> f64 {
    if nodes.len() < 3 {
        return 0.0;
    }
    let project = |point: Vec3| -> [f64; 2] {
        match axis {
            0 => [point.y, point.z],
            1 => [point.x, point.z],
            _ => [point.x, point.y],
        }
    };
    let mut area = 0.0;
    for i in 0..nodes.len() {
        let j = (i + 1) % nodes.len();
        let a = project(vertices[nodes[i]]);
        let b = project(vertices[nodes[j]]);
        area += a[0] * b[1] - b[0] * a[1];
    }
    area * 0.5
}

// AI-FUNC-SUMMARY: Clip a polyline against one axis-aligned half-space, inserting new vertices at plane crossings; returns zero or more sub-polylines; side effects: mutates vertices and node_to_index.
fn clip_polyline_against_halfspace(
    polyline: &[usize],
    vertices: &mut Vec<Vec3>,
    node_to_index: &mut BTreeMap<(i64, i64, i64), usize>,
    axis: usize,
    value: f64,
    keep_greater: bool,
    weld_step: f64,
) -> Vec<Vec<usize>> {
    if polyline.is_empty() {
        return Vec::new();
    }
    let first_point = vertices[polyline[0]];
    let mut prev_inside = point_inside_halfspace(first_point, axis, value, keep_greater);
    let mut sub_polylines: Vec<Vec<usize>> = Vec::new();
    let mut current: Vec<usize> = Vec::new();
    for i in 0..polyline.len() {
        let node = polyline[i];
        let point = vertices[node];
        let is_inside = point_inside_halfspace(point, axis, value, keep_greater);
        if is_inside {
            if !prev_inside && i > 0 {
                let prev_point = vertices[polyline[i - 1]];
                let crossing = intersect_axis_aligned(prev_point, point, axis, value);
                let new_node = get_or_insert_vertex(crossing, vertices, node_to_index, weld_step);
                current.push(new_node);
            }
            current.push(node);
        } else if prev_inside && i > 0 {
            let prev_point = vertices[polyline[i - 1]];
            let crossing = intersect_axis_aligned(prev_point, point, axis, value);
            let new_node = get_or_insert_vertex(crossing, vertices, node_to_index, weld_step);
            current.push(new_node);
            if current.len() >= 2 {
                sub_polylines.push(std::mem::take(&mut current));
            } else {
                current.clear();
            }
        }
        prev_inside = is_inside;
    }
    if current.len() >= 2 {
        sub_polylines.push(current);
    }
    sub_polylines
}

// AI-FUNC-SUMMARY:
// Purpose: Execute the G2-5b box clip of one arranged surface against the axis-aligned domain box.
// Inputs: arranged surface, domain min/max corners, eps (envelope tolerance in model units).
// Returns: A new ArrangedSurface clipped to the box with solid cap faces tagged box, C10 sheet faces tagged box, clipped curves, and box-clip cap boundary curves.
// Side effects: None (pure computation returning a new surface).
// Notes: Solids are capped on cut planes (FaceTagKind=2, source_triangles=[u32::MAX]); sheets are clipped open without caps; outside-box geometry is dropped; all existing coincidence events and degraded neighborhoods are preserved.
pub fn clip_arranged_to_box(
    surface: &ArrangedSurface,
    domain_min: Vec3,
    domain_max: Vec3,
    eps: f64,
) -> Result<ArrangedSurface> {
    let weld_step = 0.1 * eps;
    if !weld_step.is_finite() || weld_step <= 0.0 {
        return Err(RustMsptError::InvalidConfig(
            "clip_arranged_to_box requires a finite positive weld step q = 0.1 * eps".to_string(),
        ));
    }
    let mut vertices: Vec<Vec3> = surface.vertices.clone();
    let mut node_to_index: BTreeMap<(i64, i64, i64), usize> = BTreeMap::new();
    for (index, &point) in surface.vertices.iter().enumerate() {
        node_to_index
            .entry(node_key(point, weld_step))
            .or_insert(index);
    }
    let component_by_x: BTreeMap<i32, ArrangeComponent> = surface
        .components
        .iter()
        .map(|component| (component.x, *component))
        .collect();

    let mut faces: Vec<ArrangedFace> = Vec::new();
    for face in &surface.faces {
        let triangle = face.nodes.map(|node| surface.vertices[node]);
        let polygon = clip_triangle_to_box(triangle, domain_min, domain_max);
        if polygon.len() < 3 {
            continue;
        }
        let children = fan_triangulate(&polygon);
        for child in &children {
            let nodes = child.map(|point| {
                get_or_insert_vertex(point, &mut vertices, &mut node_to_index, weld_step)
            });
            if nodes[0] == nodes[1] || nodes[1] == nodes[2] || nodes[0] == nodes[2] {
                continue;
            }
            faces.push(ArrangedFace {
                nodes,
                source_triangle: face.source_triangle,
                component: face.component,
                source_triangles: face.source_triangles.clone(),
                source_orientations: face.source_orientations.clone(),
                components: face.components.clone(),
                tag_orientations: face.tag_orientations.clone(),
                box_tagged: false,
            });
        }
    }

    let mut edge_faces: BTreeMap<(usize, usize), Vec<usize>> = BTreeMap::new();
    for (face_index, face) in faces.iter().enumerate() {
        for edge in triangle_edges(face.nodes) {
            edge_faces
                .entry(sorted_pair(edge[0], edge[1]))
                .or_default()
                .push(face_index);
        }
    }

    let planes = [
        (0usize, 0usize, domain_min.x, true),
        (1, 0, domain_max.x, false),
        (2, 1, domain_min.y, true),
        (3, 1, domain_max.y, false),
        (4, 2, domain_min.z, true),
        (5, 2, domain_max.z, false),
    ];

    let mut box_curves: Vec<ArrangedCurve> = Vec::new();
    for component_meta in &surface.components {
        if component_meta.kind != 0 {
            continue;
        }
        let component_x = component_meta.x;
        for (plane_id, axis, value, is_min) in planes {
            let mut boundary_edges: Vec<(usize, usize)> = Vec::new();
            for (edge, incident) in &edge_faces {
                if incident.len() != 1 {
                    continue;
                }
                let face = &faces[incident[0]];
                if face.component != component_x {
                    continue;
                }
                let a = vertices[edge.0];
                let b = vertices[edge.1];
                if axis_coord(a, axis) == value && axis_coord(b, axis) == value {
                    boundary_edges.push(*edge);
                }
            }
            if boundary_edges.is_empty() {
                continue;
            }
            let loops = chain_edge_loops(&boundary_edges);
            for mut node_loop in loops {
                if node_loop.len() < 3 {
                    continue;
                }
                let area = loop_signed_area(&node_loop, axis, &vertices);
                let desired_positive = !((axis == 1) ^ is_min);
                let is_positive = area > 0.0;
                if is_positive != desired_positive {
                    node_loop.reverse();
                }
                let loop_nodes = node_loop.clone();
                for i in 1..node_loop.len() - 1 {
                    let nodes = [node_loop[0], node_loop[i], node_loop[i + 1]];
                    faces.push(ArrangedFace {
                        nodes,
                        source_triangle: BOX_SOURCE_TRIANGLE,
                        component: component_x,
                        source_triangles: SmallVec::from_slice(&[BOX_SOURCE_TRIANGLE]),
                        source_orientations: SmallVec::from_slice(&[1]),
                        components: SmallVec::from_slice(&[component_x]),
                        tag_orientations: SmallVec::from_slice(&[1]),
                        box_tagged: true,
                    });
                }
                box_curves.push(ArrangedCurve {
                    kind: ArrangedCurveKind::Box,
                    components: SmallVec::from_slice(&[component_x]),
                    nodes: loop_nodes,
                    radial_patches: Vec::new(),
                });
                let _ = plane_id;
            }
        }
    }

    for face in &mut faces {
        if face.box_tagged {
            continue;
        }
        let Some(component_meta) = component_by_x.get(&face.component) else {
            continue;
        };
        if component_meta.kind != 1 {
            continue;
        }
        if face_on_domain_plane(face.nodes, &vertices, domain_min, domain_max).is_some() {
            face.box_tagged = true;
        }
    }

    let mut curves: Vec<ArrangedCurve> = Vec::new();
    for curve in &surface.curves {
        let mut sub_polylines: Vec<Vec<usize>> = vec![curve.nodes.clone()];
        for (axis, value, keep_greater) in [
            (0usize, domain_min.x, true),
            (0, domain_max.x, false),
            (1, domain_min.y, true),
            (1, domain_max.y, false),
            (2, domain_min.z, true),
            (2, domain_max.z, false),
        ] {
            let mut next_polylines = Vec::new();
            for polyline in &sub_polylines {
                let clipped = clip_polyline_against_halfspace(
                    polyline,
                    &mut vertices,
                    &mut node_to_index,
                    axis,
                    value,
                    keep_greater,
                    weld_step,
                );
                next_polylines.extend(clipped);
            }
            sub_polylines = next_polylines;
        }
        for sub in &sub_polylines {
            if sub.len() < 2 {
                continue;
            }
            curves.push(ArrangedCurve {
                kind: curve.kind,
                components: curve.components.clone(),
                nodes: sub.clone(),
                radial_patches: Vec::new(),
            });
        }
    }
    curves.extend(box_curves);
    curves.sort_by(|left, right| {
        left.kind
            .cmp(&right.kind)
            .then_with(|| left.components.cmp(&right.components))
            .then_with(|| left.nodes.cmp(&right.nodes))
    });

    let mut corner_nodes = BTreeSet::new();
    for node in &surface.corner_nodes {
        let point = surface.vertices.get(*node).copied();
        if let Some(point) = point {
            if point_inside_halfspace(point, 0, domain_min.x, true)
                && point_inside_halfspace(point, 0, domain_max.x, false)
                && point_inside_halfspace(point, 1, domain_min.y, true)
                && point_inside_halfspace(point, 1, domain_max.y, false)
                && point_inside_halfspace(point, 2, domain_min.z, true)
                && point_inside_halfspace(point, 2, domain_max.z, false)
            {
                corner_nodes.insert(*node);
            }
        }
    }

    let point_features: Vec<ArrangedPointFeature> = surface
        .point_features
        .iter()
        .filter(|feature| {
            let point = vertices.get(feature.node).copied().unwrap_or(Vec3::new(
                f64::INFINITY,
                f64::INFINITY,
                f64::INFINITY,
            ));
            point_inside_halfspace(point, 0, domain_min.x, true)
                && point_inside_halfspace(point, 0, domain_max.x, false)
                && point_inside_halfspace(point, 1, domain_min.y, true)
                && point_inside_halfspace(point, 1, domain_max.y, false)
                && point_inside_halfspace(point, 2, domain_min.z, true)
                && point_inside_halfspace(point, 2, domain_max.z, false)
        })
        .cloned()
        .collect();

    let mut stats = surface.stats.clone();
    stats.split_faces = faces.iter().filter(|face| face.box_tagged).count();
    stats.point_features = point_features.len();

    Ok(ArrangedSurface {
        vertices,
        faces,
        curves,
        corner_nodes,
        point_features,
        registry: surface.registry.clone(),
        components: surface.components.clone(),
        coincidence_events: surface.coincidence_events.clone(),
        warnings: surface.warnings.clone(),
        degraded: surface.degraded.clone(),
        stats,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    // AI-FUNC-SUMMARY: Build one synthetic single-source arranged face for private fallback diagnostics; returns ArrangedFace; side effects: none.
    fn diagnostic_face(nodes: [usize; 3], source_triangle: TriId, component: i32) -> ArrangedFace {
        ArrangedFace {
            nodes,
            source_triangle,
            component,
            source_triangles: SmallVec::from_slice(&[source_triangle]),
            source_orientations: SmallVec::from_slice(&[1]),
            components: SmallVec::from_slice(&[component]),
            tag_orientations: SmallVec::from_slice(&[1]),
            box_tagged: false,
        }
    }

    // AI-FUNC-SUMMARY: Exercise deterministic residual-crossing degradation without widening the production API; returns nothing; side effects: none.
    #[test]
    fn residual_crossing_degradation_trigger_is_covered() {
        let vertices = vec![
            Vec3::new(-1.0, -1.0, 0.0),
            Vec3::new(1.0, -1.0, 0.0),
            Vec3::new(0.2, 1.0, 0.0),
            Vec3::new(0.0, -0.5, -1.0),
            Vec3::new(0.0, -0.5, 1.0),
            Vec3::new(0.0, 0.8, 1.0),
        ];
        let faces = vec![
            diagnostic_face([0, 1, 2], 0, 1),
            diagnostic_face([3, 4, 5], 1, 2),
        ];
        let run = || {
            let mut degraded = Vec::new();
            validate_arrangement(
                &faces,
                &vertices,
                &IntersectionRegistry::default(),
                &[],
                &[],
                1.0e-6,
                1.0e-6,
                &mut degraded,
            )
            .unwrap();
            degraded
        };
        let first = run();
        assert_eq!(first, run());
        let residual = first
            .iter()
            .find(|item| item.reason == DegradedReason::ResidualCrossing)
            .expect("crossing diagnostic faces did not trigger residual degradation");
        assert_eq!(residual.triangles.as_slice(), &[0, 1]);
    }

    // AI-FUNC-SUMMARY: Exercise deterministic radial-coplanarity degradation without widening the production API; returns nothing; side effects: none.
    #[test]
    fn radially_coplanar_degradation_trigger_is_covered() {
        let vertices = vec![
            Vec3::new(0.0, 0.0, 0.0),
            Vec3::new(1.0, 0.0, 0.0),
            Vec3::new(0.0, -1.0, 0.0),
            Vec3::new(0.0, 1.0, 0.0),
            Vec3::new(0.0, -2.0, 0.0),
            Vec3::new(0.0, 2.0, 0.0),
        ];
        let faces = vec![
            diagnostic_face([0, 1, 2], 0, 1),
            diagnostic_face([0, 1, 3], 0, 1),
            diagnostic_face([0, 1, 4], 1, 2),
            diagnostic_face([0, 1, 5], 1, 2),
        ];
        let segment = RegistrySegment {
            key: SegKey::new(0, 1, 0, 1),
            nodes: [0, 1],
            triangles: [0, 1],
            components: [1, 2],
        };
        let run = || {
            let mut degraded = Vec::new();
            let order =
                radial_patch_order(&segment, &faces, &vertices, 1.0e-6, &mut degraded).unwrap();
            (order, degraded)
        };
        let first = run();
        assert_eq!(first, run());
        assert_eq!(first.0, vec![0, 1, 2, 3]);
        let radial = first
            .1
            .iter()
            .find(|item| item.reason == DegradedReason::RadiallyCoplanar)
            .expect("coplanar radial fan did not trigger degradation");
        assert_eq!(radial.triangles.as_slice(), &[0, 1]);
    }
}
