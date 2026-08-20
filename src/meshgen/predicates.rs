//! Exact geometric predicates and tetrahedron metrics for the mesh pipeline.
//!
//! Frozen contract: `SPEC_meshgen_numerics.md` §3 (arithmetic tiers) and §7.1
//! (`robust` adoption). The single most important rule here is N10: the `robust`
//! crate's `orient3d` is **sign-opposite** to this project's convention, and this
//! module is the only place that negation is allowed to appear.

use crate::types::Vec3;
use std::cmp::Ordering;

const UNIT_ROUNDOFF: f64 = f64::EPSILON * 0.5;
const DD_UNIT_ROUNDOFF: f64 = UNIT_ROUNDOFF * UNIT_ROUNDOFF;
pub const ORIENT3D_FILTER_CONSTANT: f64 = (7.0 + 56.0 * UNIT_ROUNDOFF) * UNIT_ROUNDOFF;

// AI-FUNC-SUMMARY: Euclidean length of a vector (types::Vec3 exposes only dot/cross); returns f64; side effects: none.
fn len(v: Vec3) -> f64 {
    v.dot(v).sqrt()
}

// AI-FUNC-SUMMARY:
// Purpose: Exact orientation of triangle (a,b,c) projected onto the best-conditioned 2D plane.
// Inputs: three 3D points.
// Returns: a value whose SIGN is exact (>0 CCW, <0 CW, 0 degenerate) in the projection
//   whose normal component is largest (minimises conditioning); magnitude is filtered.
// Side effects: None.
// Notes: Used by S0's degenerate-triangle check (SPEC_meshgen_numerics §2: class X).
//   The projection drops the axis of the largest absolute normal component.
pub fn orient2d_3d(a: Vec3, b: Vec3, c: Vec3) -> f64 {
    let n = b.sub(a).cross(c.sub(a));
    let (nx, ny, nz) = (n.x.abs(), n.y.abs(), n.z.abs());
    if nx >= ny && nx >= nz {
        robust::orient2d(
            robust::Coord { x: a.y, y: a.z },
            robust::Coord { x: b.y, y: b.z },
            robust::Coord { x: c.y, y: c.z },
        )
    } else if ny >= nz {
        robust::orient2d(
            robust::Coord { x: a.x, y: a.z },
            robust::Coord { x: b.x, y: b.z },
            robust::Coord { x: c.x, y: c.z },
        )
    } else {
        robust::orient2d(
            robust::Coord { x: a.x, y: a.y },
            robust::Coord { x: b.x, y: b.y },
            robust::Coord { x: c.x, y: c.y },
        )
    }
}

// AI-FUNC-SUMMARY: Projection axis dropped by exact 2D predicates on a 3D plane; side effects: none.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ProjectionAxis {
    X,
    Y,
    Z,
}

// AI-FUNC-SUMMARY: Select the coordinate axis with the largest absolute triangle-normal component; returns ProjectionAxis; side effects: none.
pub fn best_projection_axis(a: Vec3, b: Vec3, c: Vec3) -> ProjectionAxis {
    let n = b.sub(a).cross(c.sub(a));
    let (nx, ny, nz) = (n.x.abs(), n.y.abs(), n.z.abs());
    if nx >= ny && nx >= nz {
        ProjectionAxis::X
    } else if ny >= nz {
        ProjectionAxis::Y
    } else {
        ProjectionAxis::Z
    }
}

// AI-FUNC-SUMMARY: Project a 3D point by dropping one axis; returns robust::Coord<f64>; side effects: none.
pub fn project_to_2d(p: Vec3, axis: ProjectionAxis) -> robust::Coord<f64> {
    match axis {
        ProjectionAxis::X => robust::Coord { x: p.y, y: p.z },
        ProjectionAxis::Y => robust::Coord { x: p.x, y: p.z },
        ProjectionAxis::Z => robust::Coord { x: p.x, y: p.y },
    }
}

// AI-FUNC-SUMMARY: Exact 2D orientation after dropping a caller-selected axis; returns sign-exact f64; side effects: none.
pub fn orient2d_axis(a: Vec3, b: Vec3, c: Vec3, axis: ProjectionAxis) -> f64 {
    robust::orient2d(
        project_to_2d(a, axis),
        project_to_2d(b, axis),
        project_to_2d(c, axis),
    )
}

// AI-FUNC-SUMMARY: f64 projected orient2d value and matching absolute-product permanent after local translation to a; returns (det, permanent); side effects: none.
pub fn orient2d_axis_value_permanent(
    a: Vec3,
    b: Vec3,
    c: Vec3,
    axis: ProjectionAxis,
) -> (f64, f64) {
    let a = project_to_2d(a, axis);
    let b = project_to_2d(b, axis);
    let c = project_to_2d(c, axis);
    let bax = b.x - a.x;
    let bay = b.y - a.y;
    let cax = c.x - a.x;
    let cay = c.y - a.y;
    let left = bax * cay;
    let right = bay * cax;
    (left - right, left.abs() + right.abs())
}

// AI-FUNC-SUMMARY: Error-free Knuth sum; returns rounded sum plus exact residual; side effects: none.
pub fn two_sum(a: f64, b: f64) -> (f64, f64) {
    let s = a + b;
    let bb = s - a;
    (s, (a - (s - bb)) + (b - bb))
}

// AI-FUNC-SUMMARY: Error-free product using one correctly-rounded FMA; returns product plus exact residual; side effects: none.
pub fn two_prod(a: f64, b: f64) -> (f64, f64) {
    let p = a * b;
    (p, a.mul_add(b, -p))
}

// AI-FUNC-SUMMARY: Minimal double-double value used by frozen C1/C2/C3 constructions; side effects: none.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct DoubleDouble {
    pub hi: f64,
    pub lo: f64,
}

impl DoubleDouble {
    // AI-FUNC-SUMMARY: Promote one f64 to double-double exactly; returns DoubleDouble; side effects: none.
    pub fn from_f64(value: f64) -> Self {
        Self { hi: value, lo: 0.0 }
    }

    // AI-FUNC-SUMMARY: Add two double-double values; returns normalized DoubleDouble; side effects: none.
    pub fn add_dd(self, other: Self) -> Self {
        let (s, e) = two_sum(self.hi, other.hi);
        let (t, f) = two_sum(self.lo, other.lo);
        let (s, e) = two_sum(s, e + t);
        let (hi, lo) = two_sum(s, e + f);
        Self { hi, lo }
    }

    // AI-FUNC-SUMMARY: Negate a double-double value; returns DoubleDouble; side effects: none.
    pub fn negated(self) -> Self {
        Self {
            hi: -self.hi,
            lo: -self.lo,
        }
    }

    // AI-FUNC-SUMMARY: Subtract two double-double values; returns normalized DoubleDouble; side effects: none.
    pub fn sub_dd(self, other: Self) -> Self {
        self.add_dd(other.negated())
    }

    // AI-FUNC-SUMMARY: Multiply two double-double values using FMA-backed two_prod; returns normalized DoubleDouble; side effects: none.
    pub fn mul_dd(self, other: Self) -> Self {
        let (p, e) = two_prod(self.hi, other.hi);
        let cross = self.hi * other.lo + self.lo * other.hi;
        let (hi, lo) = two_sum(p, e + cross + self.lo * other.lo);
        Self { hi, lo }
    }

    // AI-FUNC-SUMMARY: Collapse a double-double value once to f64; returns hi + lo; side effects: none.
    pub fn to_f64(self) -> f64 {
        self.hi + self.lo
    }

    // AI-FUNC-SUMMARY: Exact-zero test for the stored expansion; returns bool; side effects: none.
    pub fn is_zero(self) -> bool {
        self.hi == 0.0 && self.lo == 0.0
    }

    // AI-FUNC-SUMMARY: Return the absolute value of a normalized double-double expansion; returns DoubleDouble; side effects: none.
    fn abs_dd(self) -> Self {
        if self.hi < 0.0 || (self.hi == 0.0 && self.lo < 0.0) {
            self.negated()
        } else {
            self
        }
    }
}

// AI-FUNC-SUMMARY: Double-double projected orient2d determinant rebuilt from source-coordinate differences; returns DoubleDouble; side effects: none.
pub fn orient2d_axis_dd_value(a: Vec3, b: Vec3, c: Vec3, axis: ProjectionAxis) -> DoubleDouble {
    orient2d_axis_dd_value_permanent(a, b, c, axis).0
}

// AI-FUNC-SUMMARY: Double-double projected orient2d determinant and matching permanent rebuilt from source-coordinate differences; returns (det, permanent); side effects: none.
fn orient2d_axis_dd_value_permanent(
    a: Vec3,
    b: Vec3,
    c: Vec3,
    axis: ProjectionAxis,
) -> (DoubleDouble, DoubleDouble) {
    let a = project_to_2d(a, axis);
    let b = project_to_2d(b, axis);
    let c = project_to_2d(c, axis);
    let dd = DoubleDouble::from_f64;
    let bax = dd(b.x).sub_dd(dd(a.x));
    let bay = dd(b.y).sub_dd(dd(a.y));
    let cax = dd(c.x).sub_dd(dd(a.x));
    let cay = dd(c.y).sub_dd(dd(a.y));
    let left = bax.mul_dd(cay);
    let right = bay.mul_dd(cax);
    (left.sub_dd(right), left.abs_dd().add_dd(right.abs_dd()))
}

// AI-FUNC-SUMMARY: Determinant ratio retained for deterministic ordering along a registered edge; side effects: none.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DeterminantRatio {
    pub numerator: DoubleDouble,
    pub denominator: DoubleDouble,
}

impl DeterminantRatio {
    // AI-FUNC-SUMMARY: Normalize denominator sign so equivalent ratios compare consistently; returns DeterminantRatio; side effects: none.
    pub fn normalized(mut self) -> Self {
        if self.denominator.to_f64().is_sign_negative() {
            self.numerator = self.numerator.negated();
            self.denominator = self.denominator.negated();
        }
        self
    }

    // AI-FUNC-SUMMARY: Convert edge parameter t to 1-t while retaining determinant form; returns DeterminantRatio; side effects: none.
    pub fn complement(self) -> Self {
        Self {
            numerator: self.denominator.sub_dd(self.numerator),
            denominator: self.denominator,
        }
        .normalized()
    }

    // AI-FUNC-SUMMARY: Compare two determinant ratios using double-double cross products; returns Ordering; side effects: none.
    pub fn compare(self, other: Self) -> Ordering {
        let lhs = self.numerator.mul_dd(other.denominator);
        let rhs = other.numerator.mul_dd(self.denominator);
        lhs.sub_dd(rhs)
            .to_f64()
            .partial_cmp(&0.0)
            .unwrap_or(Ordering::Equal)
    }
}

// AI-FUNC-SUMMARY: Precision tier that produced a committed construction value; side effects: none.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PrecisionTier {
    F64,
    DoubleDouble,
}

// AI-FUNC-SUMMARY: A resolved C1 edge-triangle intersection with its exact-ordering ratio; side effects: none.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct EdgeTriPoint {
    pub point: Vec3,
    pub parameter: f64,
    pub ratio: DeterminantRatio,
}

// AI-FUNC-SUMMARY: A resolved C3 coplanar segment intersection with parameters and DD ordering ratios on both supplied edges; side effects: none.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CoplanarSegmentPoint {
    pub point: Vec3,
    pub first_parameter: f64,
    pub second_parameter: f64,
    pub first_ratio: DeterminantRatio,
    pub second_ratio: DeterminantRatio,
}

// AI-FUNC-SUMMARY: A resolved construction or an explicit precision-deferred route; side effects: none.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum ConstructionOutcome<T> {
    Resolved {
        value: T,
        tier: PrecisionTier,
        rho: f64,
    },
    Deferred {
        rho: f64,
    },
}

// AI-FUNC-SUMMARY: f64 orient3d value and matching Shewchuk permanent in the project sign convention; returns (det, permanent); side effects: none.
pub fn orient3d_value_permanent(a: Vec3, b: Vec3, c: Vec3, d: Vec3) -> (f64, f64) {
    let adx = a.x - d.x;
    let bdx = b.x - d.x;
    let cdx = c.x - d.x;
    let ady = a.y - d.y;
    let bdy = b.y - d.y;
    let cdy = c.y - d.y;
    let adz = a.z - d.z;
    let bdz = b.z - d.z;
    let cdz = c.z - d.z;
    let raw = adz * (bdx * cdy - cdx * bdy)
        + bdz * (cdx * ady - adx * cdy)
        + cdz * (adx * bdy - bdx * ady);
    let permanent = ((bdx * cdy).abs() + (cdx * bdy).abs()) * adz.abs()
        + ((cdx * ady).abs() + (adx * cdy).abs()) * bdz.abs()
        + ((adx * bdy).abs() + (bdx * ady).abs()) * cdz.abs();
    (-raw, permanent)
}

// AI-FUNC-SUMMARY: Certified orient3d sign using the frozen static filter then exact robust fallback; returns (sign, filter_passed, f64_det, permanent); side effects: none.
pub fn orient3d_filtered(a: Vec3, b: Vec3, c: Vec3, d: Vec3) -> (i8, bool, f64, f64) {
    let (det, permanent) = orient3d_value_permanent(a, b, c, d);
    if det.abs() > ORIENT3D_FILTER_CONSTANT * permanent {
        (sign_i8(det), true, det, permanent)
    } else {
        (sign_i8(orient3d(a, b, c, d)), false, det, permanent)
    }
}

// AI-FUNC-SUMMARY:
// Purpose: Exact in-sphere test — is `e` inside the circumsphere of the tet (a, b, c, d)?
// Inputs: five 3D points.
// Returns: +1 strictly inside, -1 strictly outside, 0 cospherical or (a,b,c,d) coplanar.
// Side effects: None.
// Notes: **The answer does not depend on the caller's winding, and that is the point of the
//   wrapper.** `robust::insphere` requires (a, b, c, d) to be positively oriented *in the robust
//   crate's convention*, which N10 records as sign-opposite to this project's, and it returns the
//   wrong sign for a negatively oriented tet rather than an error. Swapping two vertices flips both
//   the orientation and the sign of the result, so normalising the orientation here makes the
//   result a function of the five points alone. A caller that had to remember the convention would
//   get it wrong exactly once, silently, in the tie cases that matter most.
//
//   Four coplanar points have no circumsphere, so that is 0 rather than a guess: a degenerate tet
//   must never enter a tetrahedralisation, and returning a side for one would hide it.
//
//   Needed by `SPEC_meshgen_geometry.md` §7.4's constrained incremental tetrahedralisation, which
//   is the one primitive this module did not already have.
pub fn insphere(a: Vec3, b: Vec3, c: Vec3, d: Vec3, e: Vec3) -> i8 {
    let at = |p: Vec3| robust::Coord3D { x: p.x, y: p.y, z: p.z };
    let orientation = robust::orient3d(at(a), at(b), at(c), at(d));
    if orientation == 0.0 {
        return 0;
    }
    let value = if orientation > 0.0 {
        robust::insphere(at(a), at(b), at(c), at(d), at(e))
    } else {
        robust::insphere(at(b), at(a), at(c), at(d), at(e))
    };
    sign_i8(value)
}

// AI-FUNC-SUMMARY: Double-double orient3d determinant value in the project sign convention; returns DoubleDouble; side effects: none.
pub fn orient3d_dd_value(a: Vec3, b: Vec3, c: Vec3, d: Vec3) -> DoubleDouble {
    orient3d_dd_value_permanent(a, b, c, d).0
}

// AI-FUNC-SUMMARY: Double-double orient3d determinant and matching Shewchuk permanent in the project sign convention; returns (det, permanent); side effects: none.
fn orient3d_dd_value_permanent(a: Vec3, b: Vec3, c: Vec3, d: Vec3) -> (DoubleDouble, DoubleDouble) {
    let dd = DoubleDouble::from_f64;
    let adx = dd(a.x).sub_dd(dd(d.x));
    let bdx = dd(b.x).sub_dd(dd(d.x));
    let cdx = dd(c.x).sub_dd(dd(d.x));
    let ady = dd(a.y).sub_dd(dd(d.y));
    let bdy = dd(b.y).sub_dd(dd(d.y));
    let cdy = dd(c.y).sub_dd(dd(d.y));
    let adz = dd(a.z).sub_dd(dd(d.z));
    let bdz = dd(b.z).sub_dd(dd(d.z));
    let cdz = dd(c.z).sub_dd(dd(d.z));
    let bdx_cdy = bdx.mul_dd(cdy);
    let cdx_bdy = cdx.mul_dd(bdy);
    let cdx_ady = cdx.mul_dd(ady);
    let adx_cdy = adx.mul_dd(cdy);
    let adx_bdy = adx.mul_dd(bdy);
    let bdx_ady = bdx.mul_dd(ady);
    let raw = adz
        .mul_dd(bdx_cdy.sub_dd(cdx_bdy))
        .add_dd(bdz.mul_dd(cdx_ady.sub_dd(adx_cdy)))
        .add_dd(cdz.mul_dd(adx_bdy.sub_dd(bdx_ady)));
    let permanent = bdx_cdy
        .abs_dd()
        .add_dd(cdx_bdy.abs_dd())
        .mul_dd(adz.abs_dd())
        .add_dd(
            cdx_ady
                .abs_dd()
                .add_dd(adx_cdy.abs_dd())
                .mul_dd(bdz.abs_dd()),
        )
        .add_dd(
            adx_bdy
                .abs_dd()
                .add_dd(bdx_ady.abs_dd())
                .mul_dd(cdz.abs_dd()),
        );
    (raw.negated(), permanent)
}

// AI-FUNC-SUMMARY: Form a dimensionless stability ratio from DD numerator and permanent values using one final f64 division; returns zero for a zero permanent; side effects: none.
fn dd_stability_ratio(numerator: DoubleDouble, permanent: DoubleDouble) -> f64 {
    let numerator = numerator.abs_dd().to_f64();
    let permanent = permanent.to_f64();
    if permanent > 0.0 {
        numerator / permanent
    } else {
        0.0
    }
}

// AI-FUNC-SUMMARY:
// Purpose: Frozen C1 edge-triangle determinant-ratio construction with staged f64/DD resolution.
// Inputs: defining triangle, edge endpoints, and the normalized weld step.
// Returns: Resolved point with a DD ordering ratio, or Deferred below the DD resolvability floor or for an invalid result.
// Side effects: None.
// Notes: Every committed ratio is recomputed in DD; coordinate construction performs no DD division.
pub fn construct_edge_triangle_intersection(
    triangle: [Vec3; 3],
    p: Vec3,
    q: Vec3,
    weld_step: f64,
) -> ConstructionOutcome<EdgeTriPoint> {
    let (dp, perm_p) = orient3d_value_permanent(triangle[0], triangle[1], triangle[2], p);
    let (dq, perm_q) = orient3d_value_permanent(triangle[0], triangle[1], triangle[2], q);
    let denominator = dp - dq;
    let permanent = perm_p + perm_q;
    let f64_rho = if permanent > 0.0 {
        denominator.abs() / permanent
    } else {
        0.0
    };
    if !weld_step.is_finite() || weld_step <= 0.0 || !f64_rho.is_finite() {
        return ConstructionOutcome::Deferred { rho: f64_rho };
    }
    let kappa = 4.0 * 7.0 * UNIT_ROUNDOFF / weld_step;
    let kappa_dd = kappa * DD_UNIT_ROUNDOFF / UNIT_ROUNDOFF;
    let (dp_dd, perm_p_dd) = orient3d_dd_value_permanent(triangle[0], triangle[1], triangle[2], p);
    let (dq_dd, perm_q_dd) = orient3d_dd_value_permanent(triangle[0], triangle[1], triangle[2], q);
    let denominator_dd = dp_dd.sub_dd(dq_dd);
    let denominator_dd_f64 = denominator_dd.to_f64();
    let (t, tier, rho) = if f64_rho >= kappa {
        if !denominator.is_finite() || denominator == 0.0 {
            return ConstructionOutcome::Deferred { rho: f64_rho };
        }
        (dp / denominator, PrecisionTier::F64, f64_rho)
    } else {
        let dd_rho = dd_stability_ratio(denominator_dd, perm_p_dd.add_dd(perm_q_dd));
        if !dd_rho.is_finite()
            || dd_rho < kappa_dd
            || denominator_dd.is_zero()
            || !denominator_dd_f64.is_finite()
            || denominator_dd_f64 == 0.0
        {
            return ConstructionOutcome::Deferred { rho: dd_rho };
        }
        (
            dp_dd.to_f64() / denominator_dd_f64,
            PrecisionTier::DoubleDouble,
            dd_rho,
        )
    };
    if denominator_dd.is_zero() || !denominator_dd_f64.is_finite() || denominator_dd_f64 == 0.0 {
        return ConstructionOutcome::Deferred { rho };
    }
    let point = p.add(q.sub(p).scale(t));
    if !t.is_finite() || !point.x.is_finite() || !point.y.is_finite() || !point.z.is_finite() {
        return ConstructionOutcome::Deferred { rho };
    }
    ConstructionOutcome::Resolved {
        value: EdgeTriPoint {
            point,
            parameter: t,
            ratio: DeterminantRatio {
                numerator: dp_dd,
                denominator: denominator_dd,
            }
            .normalized(),
        },
        tier,
        rho,
    }
}

// AI-FUNC-SUMMARY:
// Purpose: Frozen C3 coplanar segment-segment determinant-ratio construction with staged f64/DD resolution.
// Inputs: canonically oriented first and second edges, their explicit common-plane projection axis, and the normalized weld step.
// Returns: Resolved point with parameters and DD ordering ratios on both edges, or Deferred below the DD resolvability floor or for an invalid result.
// Side effects: None.
// Notes: The caller establishes crossing topology separately with exact orient2d signs. The 3-D point is formed once as second[0] + t * (second[1] - second[0]).
pub fn construct_coplanar_segment_intersection(
    first: [Vec3; 2],
    second: [Vec3; 2],
    axis: ProjectionAxis,
    weld_step: f64,
) -> ConstructionOutcome<CoplanarSegmentPoint> {
    let [a, b] = first;
    let [p, q] = second;
    let (op, perm_p) = orient2d_axis_value_permanent(a, b, p, axis);
    let (oq, perm_q) = orient2d_axis_value_permanent(a, b, q, axis);
    let denominator = op - oq;
    let second_permanent = perm_p + perm_q;
    let second_rho = if second_permanent > 0.0 {
        denominator.abs() / second_permanent
    } else {
        0.0
    };
    let (oa, perm_a) = orient2d_axis_value_permanent(p, q, a, axis);
    let (ob, perm_b) = orient2d_axis_value_permanent(p, q, b, axis);
    let first_denominator = oa - ob;
    let first_permanent = perm_a + perm_b;
    let first_rho = if first_permanent > 0.0 {
        first_denominator.abs() / first_permanent
    } else {
        0.0
    };
    let f64_rho = first_rho.min(second_rho);
    if !weld_step.is_finite() || weld_step <= 0.0 || !f64_rho.is_finite() {
        return ConstructionOutcome::Deferred { rho: f64_rho };
    }
    let kappa = 4.0 * 7.0 * UNIT_ROUNDOFF / weld_step;
    let kappa_dd = kappa * DD_UNIT_ROUNDOFF / UNIT_ROUNDOFF;
    let (op_dd, perm_p_dd) = orient2d_axis_dd_value_permanent(a, b, p, axis);
    let (oq_dd, perm_q_dd) = orient2d_axis_dd_value_permanent(a, b, q, axis);
    let second_denominator_dd = op_dd.sub_dd(oq_dd);
    let second_denominator_dd_f64 = second_denominator_dd.to_f64();
    let (oa_dd, perm_a_dd) = orient2d_axis_dd_value_permanent(p, q, a, axis);
    let (ob_dd, perm_b_dd) = orient2d_axis_dd_value_permanent(p, q, b, axis);
    let first_denominator_dd = oa_dd.sub_dd(ob_dd);
    let first_denominator_dd_f64 = first_denominator_dd.to_f64();
    let (first_parameter, second_parameter, tier, rho) = if f64_rho >= kappa {
        if !first_denominator.is_finite()
            || first_denominator == 0.0
            || !denominator.is_finite()
            || denominator == 0.0
        {
            return ConstructionOutcome::Deferred { rho: f64_rho };
        }
        (
            oa / first_denominator,
            op / denominator,
            PrecisionTier::F64,
            f64_rho,
        )
    } else {
        let first_dd_rho = dd_stability_ratio(first_denominator_dd, perm_a_dd.add_dd(perm_b_dd));
        let second_dd_rho = dd_stability_ratio(second_denominator_dd, perm_p_dd.add_dd(perm_q_dd));
        let dd_rho = first_dd_rho.min(second_dd_rho);
        if !first_dd_rho.is_finite()
            || !second_dd_rho.is_finite()
            || first_dd_rho < kappa_dd
            || second_dd_rho < kappa_dd
            || first_denominator_dd.is_zero()
            || second_denominator_dd.is_zero()
            || !first_denominator_dd_f64.is_finite()
            || !second_denominator_dd_f64.is_finite()
            || first_denominator_dd_f64 == 0.0
            || second_denominator_dd_f64 == 0.0
        {
            return ConstructionOutcome::Deferred { rho: dd_rho };
        }
        (
            oa_dd.to_f64() / first_denominator_dd_f64,
            op_dd.to_f64() / second_denominator_dd_f64,
            PrecisionTier::DoubleDouble,
            dd_rho,
        )
    };
    if first_denominator_dd.is_zero()
        || second_denominator_dd.is_zero()
        || !first_denominator_dd_f64.is_finite()
        || !second_denominator_dd_f64.is_finite()
        || first_denominator_dd_f64 == 0.0
        || second_denominator_dd_f64 == 0.0
    {
        return ConstructionOutcome::Deferred { rho };
    }
    let point = p.add(q.sub(p).scale(second_parameter));
    if !first_parameter.is_finite()
        || !second_parameter.is_finite()
        || !point.x.is_finite()
        || !point.y.is_finite()
        || !point.z.is_finite()
    {
        return ConstructionOutcome::Deferred { rho };
    }

    ConstructionOutcome::Resolved {
        value: CoplanarSegmentPoint {
            point,
            first_parameter,
            second_parameter,
            first_ratio: DeterminantRatio {
                numerator: oa_dd,
                denominator: first_denominator_dd,
            }
            .normalized(),
            second_ratio: DeterminantRatio {
                numerator: op_dd,
                denominator: second_denominator_dd,
            }
            .normalized(),
        },
        tier,
        rho,
    }
}

// AI-FUNC-SUMMARY:
// Purpose: Frozen C2 three-plane Cramer construction with staged f64/DD resolution in a deterministic local frame.
// Inputs: three defining triangles and the normalized weld step.
// Returns: Resolved global point, or Deferred below the DD resolvability floor or for an invalid result.
// Side effects: None.
// Notes: DD escalation rebuilds source-coordinate differences, normals, rhs values, determinant, and numerators before one final f64 division per coordinate.
pub fn construct_three_triangle_intersection(
    triangles: [[Vec3; 3]; 3],
    weld_step: f64,
) -> ConstructionOutcome<Vec3> {
    let origin = triangles[0][0];
    let local_triangles = triangles.map(|triangle| triangle.map(|point| point.sub(origin)));
    let planes = local_triangles.map(|tri| {
        let n = tri[1].sub(tri[0]).cross(tri[2].sub(tri[0]));
        (n, n.dot(tri[0]))
    });
    let rows = [planes[0].0, planes[1].0, planes[2].0];
    let rhs = [planes[0].1, planes[1].1, planes[2].1];
    let (denominator, permanent) = det3_value_permanent(rows);
    let f64_rho = if permanent > 0.0 {
        denominator.abs() / permanent
    } else {
        0.0
    };
    if !weld_step.is_finite() || weld_step <= 0.0 || !f64_rho.is_finite() {
        return ConstructionOutcome::Deferred { rho: f64_rho };
    }
    let kappa = 4.0 * 25.0 * UNIT_ROUNDOFF / weld_step;
    let kappa_dd = kappa * DD_UNIT_ROUNDOFF / UNIT_ROUNDOFF;
    if f64_rho >= kappa {
        if !denominator.is_finite() || denominator == 0.0 {
            return ConstructionOutcome::Deferred { rho: f64_rho };
        }
        let point = origin.add(cramer_point_f64(rows, rhs, denominator));
        if point.x.is_finite() && point.y.is_finite() && point.z.is_finite() {
            return ConstructionOutcome::Resolved {
                value: point,
                tier: PrecisionTier::F64,
                rho: f64_rho,
            };
        }
        return ConstructionOutcome::Deferred { rho: f64_rho };
    }

    let planes_dd = triangles.map(|triangle| triangle_plane_dd(triangle, origin));
    let rows_dd = planes_dd.map(|plane| plane.0);
    let rhs_dd = planes_dd.map(|plane| plane.1);
    let (denominator_dd, permanent_dd) = det3_dd_value_permanent(rows_dd);
    let dd_rho = dd_stability_ratio(denominator_dd, permanent_dd);
    if !dd_rho.is_finite() || dd_rho < kappa_dd || denominator_dd.is_zero() {
        return ConstructionOutcome::Deferred { rho: dd_rho };
    }
    let Some(local_point) = cramer_point_dd(rows_dd, rhs_dd, denominator_dd) else {
        return ConstructionOutcome::Deferred { rho: dd_rho };
    };
    let point = origin.add(local_point);
    if !point.x.is_finite() || !point.y.is_finite() || !point.z.is_finite() {
        return ConstructionOutcome::Deferred { rho: dd_rho };
    }
    ConstructionOutcome::Resolved {
        value: point,
        tier: PrecisionTier::DoubleDouble,
        rho: dd_rho,
    }
}

// AI-FUNC-SUMMARY: Sign as -1/0/+1; returns i8; side effects: none.
fn sign_i8(value: f64) -> i8 {
    if value > 0.0 {
        1
    } else if value < 0.0 {
        -1
    } else {
        0
    }
}

// AI-FUNC-SUMMARY: 3x3 determinant and matching absolute-product permanent; returns (det, permanent); side effects: none.
fn det3_value_permanent(rows: [Vec3; 3]) -> (f64, f64) {
    let det = rows[0].x * (rows[1].y * rows[2].z - rows[1].z * rows[2].y)
        - rows[0].y * (rows[1].x * rows[2].z - rows[1].z * rows[2].x)
        + rows[0].z * (rows[1].x * rows[2].y - rows[1].y * rows[2].x);
    let permanent = (rows[0].x * rows[1].y * rows[2].z).abs()
        + (rows[0].x * rows[1].z * rows[2].y).abs()
        + (rows[0].y * rows[1].x * rows[2].z).abs()
        + (rows[0].y * rows[1].z * rows[2].x).abs()
        + (rows[0].z * rows[1].x * rows[2].y).abs()
        + (rows[0].z * rows[1].y * rows[2].x).abs();
    (det, permanent)
}

// AI-FUNC-SUMMARY: Recompute one local-frame triangle plane entirely in DD from source coordinates; returns (normal, rhs); side effects: none.
fn triangle_plane_dd(triangle: [Vec3; 3], origin: Vec3) -> ([DoubleDouble; 3], DoubleDouble) {
    let dd = DoubleDouble::from_f64;
    let difference = |a: Vec3, b: Vec3| {
        [
            dd(a.x).sub_dd(dd(b.x)),
            dd(a.y).sub_dd(dd(b.y)),
            dd(a.z).sub_dd(dd(b.z)),
        ]
    };
    let local_a = difference(triangle[0], origin);
    let ab = difference(triangle[1], triangle[0]);
    let ac = difference(triangle[2], triangle[0]);
    let normal = [
        ab[1].mul_dd(ac[2]).sub_dd(ab[2].mul_dd(ac[1])),
        ab[2].mul_dd(ac[0]).sub_dd(ab[0].mul_dd(ac[2])),
        ab[0].mul_dd(ac[1]).sub_dd(ab[1].mul_dd(ac[0])),
    ];
    let rhs = normal[0]
        .mul_dd(local_a[0])
        .add_dd(normal[1].mul_dd(local_a[1]))
        .add_dd(normal[2].mul_dd(local_a[2]));
    (normal, rhs)
}

// AI-FUNC-SUMMARY: 3x3 determinant over double-double rows; returns DoubleDouble; side effects: none.
fn det3_dd(rows: [[DoubleDouble; 3]; 3]) -> DoubleDouble {
    det3_dd_value_permanent(rows).0
}

// AI-FUNC-SUMMARY: 3x3 double-double determinant and the sum of its six absolute triple products; returns (det, permanent); side effects: none.
fn det3_dd_value_permanent(rows: [[DoubleDouble; 3]; 3]) -> (DoubleDouble, DoubleDouble) {
    let terms = [
        rows[0][0].mul_dd(rows[1][1]).mul_dd(rows[2][2]),
        rows[0][0].mul_dd(rows[1][2]).mul_dd(rows[2][1]),
        rows[0][1].mul_dd(rows[1][0]).mul_dd(rows[2][2]),
        rows[0][1].mul_dd(rows[1][2]).mul_dd(rows[2][0]),
        rows[0][2].mul_dd(rows[1][0]).mul_dd(rows[2][1]),
        rows[0][2].mul_dd(rows[1][1]).mul_dd(rows[2][0]),
    ];
    let determinant = terms[0]
        .sub_dd(terms[1])
        .sub_dd(terms[2])
        .add_dd(terms[3])
        .add_dd(terms[4])
        .sub_dd(terms[5]);
    let permanent = terms
        .into_iter()
        .fold(DoubleDouble::default(), |sum, term| {
            sum.add_dd(term.abs_dd())
        });
    (determinant, permanent)
}

// AI-FUNC-SUMMARY: Cramer's rule in f64 with one division per coordinate; returns Vec3; side effects: none.
fn cramer_point_f64(rows: [Vec3; 3], rhs: [f64; 3], denominator: f64) -> Vec3 {
    let column = |x: [f64; 3], y: [f64; 3], z: [f64; 3]| {
        det3_value_permanent([
            Vec3::new(x[0], y[0], z[0]),
            Vec3::new(x[1], y[1], z[1]),
            Vec3::new(x[2], y[2], z[2]),
        ])
        .0
    };
    let x = column(
        rhs,
        [rows[0].y, rows[1].y, rows[2].y],
        [rows[0].z, rows[1].z, rows[2].z],
    );
    let y = column(
        [rows[0].x, rows[1].x, rows[2].x],
        rhs,
        [rows[0].z, rows[1].z, rows[2].z],
    );
    let z = column(
        [rows[0].x, rows[1].x, rows[2].x],
        [rows[0].y, rows[1].y, rows[2].y],
        rhs,
    );
    Vec3::new(x / denominator, y / denominator, z / denominator)
}

// AI-FUNC-SUMMARY: Cramer's rule over DD numerators/denominator with one final f64 division per coordinate; returns None for an unusable collapsed denominator; side effects: none.
fn cramer_point_dd(
    rows: [[DoubleDouble; 3]; 3],
    rhs: [DoubleDouble; 3],
    denominator: DoubleDouble,
) -> Option<Vec3> {
    let column = |x: [DoubleDouble; 3], y: [DoubleDouble; 3], z: [DoubleDouble; 3]| {
        det3_dd([[x[0], y[0], z[0]], [x[1], y[1], z[1]], [x[2], y[2], z[2]]])
    };
    let x = column(
        rhs,
        [rows[0][1], rows[1][1], rows[2][1]],
        [rows[0][2], rows[1][2], rows[2][2]],
    );
    let y = column(
        [rows[0][0], rows[1][0], rows[2][0]],
        rhs,
        [rows[0][2], rows[1][2], rows[2][2]],
    );
    let z = column(
        [rows[0][0], rows[1][0], rows[2][0]],
        [rows[0][1], rows[1][1], rows[2][1]],
        rhs,
    );
    let den = denominator.to_f64();
    if !den.is_finite() || den == 0.0 {
        return None;
    }
    Some(Vec3::new(
        x.to_f64() / den,
        y.to_f64() / den,
        z.to_f64() / den,
    ))
}

// AI-FUNC-SUMMARY:
// Purpose: Exact sign of the tetrahedron orientation determinant det[b-a, c-a, d-a].
// Inputs: four points.
// Returns: a value whose SIGN is exact (> 0 for a positively oriented tet, per
//   SPEC_meshgen_geometry §1.1 / VTK_TETRA); the magnitude is filtered, not exact.
// Side effects: None.
// Notes: Rule N10 — `robust::orient3d` uses the opposite convention (it computes
//   det[a-d, b-d, c-d]), so the result is negated here and nowhere else. Never call
//   `robust::orient3d` directly from other modules; `predicates::orient3d_sign_test`
//   pins this convention.
pub fn orient3d(a: Vec3, b: Vec3, c: Vec3, d: Vec3) -> f64 {
    let cv = |p: Vec3| robust::Coord3D {
        x: p.x,
        y: p.y,
        z: p.z,
    };
    -robust::orient3d(cv(a), cv(b), cv(c), cv(d))
}

// AI-FUNC-SUMMARY: Signed volume of tet (a,b,c,d) = orient3d/6; positive when positively oriented; side effects: none.
pub fn tet_signed_volume(a: Vec3, b: Vec3, c: Vec3, d: Vec3) -> f64 {
    orient3d(a, b, c, d) / 6.0
}

// AI-FUNC-SUMMARY: Reference case pinning the orientation convention (unit tet is positive); returns f64 > 0; side effects: none.
pub fn orient3d_sign_test() -> f64 {
    orient3d(
        Vec3::new(0.0, 0.0, 0.0),
        Vec3::new(1.0, 0.0, 0.0),
        Vec3::new(0.0, 1.0, 0.0),
        Vec3::new(0.0, 0.0, 1.0),
    )
}

// AI-FUNC-SUMMARY:
// Purpose: Shape-quality metrics of one tetrahedron, as reported by verifier check [V4].
// Notes: `aspect_ratio` = R/(3·r_in) (1 for a regular tet, grows without bound as the
//   element degenerates); `radius_ratio` is its reciprocal; dihedral extremes are stored
//   as cosines so gates stay algebraic (SPEC_meshgen_numerics §8.1 rule 4) with degrees
//   provided for reporting only.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TetQuality {
    pub volume: f64,
    pub aspect_ratio: f64,
    pub radius_ratio: f64,
    pub min_dihedral_deg: f64,
    pub max_dihedral_deg: f64,
    pub max_dihedral_cos: f64,
    pub scaled_jacobian: f64,
    pub min_altitude: f64,
}

// AI-FUNC-SUMMARY:
// Purpose: Compute the [V4] quality metrics of a tetrahedron.
// Inputs: the four corner points, in any orientation.
// Returns: TetQuality; degenerate elements yield f64::INFINITY aspect ratio and zero radius ratio.
// Side effects: None.
// Notes: `max_dihedral_cos` is the cosine of the SMALLEST dihedral angle (cosine decreases
//   with angle), so a "min dihedral below θ" gate is `max_dihedral_cos > cos(θ)` with no
//   transcendental in the decision path.
pub fn tet_quality(p: [Vec3; 4]) -> TetQuality {
    let volume = tet_signed_volume(p[0], p[1], p[2], p[3]);
    let av = volume.abs();
    const FACES: [[usize; 3]; 4] = [[1, 2, 3], [0, 3, 2], [0, 1, 3], [0, 2, 1]];
    let mut area_sum = 0.0;
    let mut areas = [0.0f64; 4];
    for (i, f) in FACES.iter().enumerate() {
        let a = p[f[1]].sub(p[f[0]]).cross(p[f[2]].sub(p[f[0]]));
        areas[i] = len(a) * 0.5;
        area_sum += areas[i];
    }
    let r_in = if area_sum > 0.0 {
        3.0 * av / area_sum
    } else {
        0.0
    };
    let r_circ = circumradius(p);
    let (aspect_ratio, radius_ratio) = if r_in > 0.0 && r_circ.is_finite() {
        (r_circ / (3.0 * r_in), 3.0 * r_in / r_circ)
    } else {
        (f64::INFINITY, 0.0)
    };

    // dihedral angles: for the edge (i,j), project the two opposite vertices onto the
    // plane perpendicular to the edge and take the angle between them
    let mut cos_min = 1.0f64; // cosine of the largest angle
    let mut cos_max = -1.0f64; // cosine of the smallest angle
    for i in 0..4 {
        for j in (i + 1)..4 {
            let (k, l) = {
                let rest: Vec<usize> = (0..4).filter(|&x| x != i && x != j).collect();
                (rest[0], rest[1])
            };
            let e = p[j].sub(p[i]);
            let el = len(e);
            if el <= 0.0 {
                continue;
            }
            let e = e.scale(1.0 / el);
            let u = {
                let w = p[k].sub(p[i]);
                w.sub(e.scale(w.dot(e)))
            };
            let v = {
                let w = p[l].sub(p[i]);
                w.sub(e.scale(w.dot(e)))
            };
            let (ul, vl) = (len(u), len(v));
            if ul <= 0.0 || vl <= 0.0 {
                continue;
            }
            let c = (u.dot(v) / (ul * vl)).clamp(-1.0, 1.0);
            cos_min = cos_min.min(c);
            cos_max = cos_max.max(c);
        }
    }

    // scaled Jacobian: min over corners of the normalized triple product, √2-scaled
    // so a regular tetrahedron scores 1
    let mut scaled_jacobian = f64::INFINITY;
    const CORNERS: [[usize; 4]; 4] = [[0, 1, 2, 3], [1, 0, 3, 2], [2, 0, 1, 3], [3, 0, 2, 1]];
    for c in CORNERS.iter() {
        let e1 = p[c[1]].sub(p[c[0]]);
        let e2 = p[c[2]].sub(p[c[0]]);
        let e3 = p[c[3]].sub(p[c[0]]);
        let denom = len(e1) * len(e2) * len(e3);
        if denom > 0.0 {
            scaled_jacobian =
                scaled_jacobian.min(e1.cross(e2).dot(e3) / denom * std::f64::consts::SQRT_2);
        }
    }
    if !scaled_jacobian.is_finite() {
        scaled_jacobian = 0.0;
    }

    let min_altitude = areas
        .iter()
        .filter(|&&a| a > 0.0)
        .map(|&a| 3.0 * av / a)
        .fold(f64::INFINITY, f64::min);

    TetQuality {
        volume,
        aspect_ratio,
        radius_ratio,
        min_dihedral_deg: cos_max.clamp(-1.0, 1.0).acos().to_degrees(),
        max_dihedral_deg: cos_min.clamp(-1.0, 1.0).acos().to_degrees(),
        max_dihedral_cos: cos_max,
        scaled_jacobian,
        min_altitude: if min_altitude.is_finite() {
            min_altitude
        } else {
            0.0
        },
    }
}

// AI-FUNC-SUMMARY: Circumradius of a tetrahedron via the standard determinant form; returns f64::INFINITY when degenerate; side effects: none.
fn circumradius(p: [Vec3; 4]) -> f64 {
    let a = p[1].sub(p[0]);
    let b = p[2].sub(p[0]);
    let c = p[3].sub(p[0]);
    let det = a.dot(b.cross(c));
    if det.abs() <= 0.0 {
        return f64::INFINITY;
    }
    let num = b
        .cross(c)
        .scale(a.dot(a))
        .add(c.cross(a).scale(b.dot(b)))
        .add(a.cross(b).scale(c.dot(c)));
    len(num) / (2.0 * det.abs())
}

// AI-FUNC-SUMMARY:
// Purpose: Quantized node key on a scale-relative grid (SPEC_meshgen_geometry §1.2).
// Inputs: point and the quantization step q (model units).
// Returns: integer triple; two points sharing a key are the same node by the weld invariant.
// Side effects: None.
pub fn node_key(p: Vec3, q: f64) -> (i64, i64, i64) {
    let f = |x: f64| (x / q).round() as i64;
    (f(p.x), f(p.y), f(p.z))
}

#[cfg(test)]
mod tests {
    use super::*;

    // The tet the rest of these use: the unit corner tet, positively oriented in the usual
    // right-handed sense. Its circumcentre is (0.5, 0.5, 0.5) and its circumradius sqrt(3)/2.
    fn unit_tet() -> [Vec3; 4] {
        [
            Vec3::new(0.0, 0.0, 0.0),
            Vec3::new(1.0, 0.0, 0.0),
            Vec3::new(0.0, 1.0, 0.0),
            Vec3::new(0.0, 0.0, 1.0),
        ]
    }

    #[test]
    fn a_point_inside_the_circumsphere_reads_inside() {
        let t = unit_tet();
        // The centroid is 0.433 from the circumcentre against a radius of 0.866.
        assert_eq!(insphere(t[0], t[1], t[2], t[3], Vec3::new(0.25, 0.25, 0.25)), 1);
        // And the circumcentre itself, which is as inside as a point gets.
        assert_eq!(insphere(t[0], t[1], t[2], t[3], Vec3::new(0.5, 0.5, 0.5)), 1);
    }

    #[test]
    fn a_point_outside_the_circumsphere_reads_outside() {
        let t = unit_tet();
        assert_eq!(insphere(t[0], t[1], t[2], t[3], Vec3::new(10.0, 10.0, 10.0)), -1);
        // A tet's own vertices are ON its circumsphere, so a point just beyond one is outside.
        assert_eq!(insphere(t[0], t[1], t[2], t[3], Vec3::new(1.5, 0.0, 0.0)), -1);
    }

    // **The property the wrapper exists for.** `robust::insphere` requires its first four points to
    // be positively oriented in the robust crate's own convention and returns the WRONG SIGN
    // otherwise - silently. A caller that had to remember that would get it wrong once, in a tie
    // case, and the failure would look like a meshing bug rather than a predicate bug.
    #[test]
    fn the_answer_does_not_depend_on_the_tets_winding() {
        let t = unit_tet();
        let inside = Vec3::new(0.25, 0.25, 0.25);
        let outside = Vec3::new(10.0, 10.0, 10.0);
        for probe in [inside, outside] {
            let reference = insphere(t[0], t[1], t[2], t[3], probe);
            assert_eq!(insphere(t[1], t[0], t[2], t[3], probe), reference, "one swap");
            assert_eq!(insphere(t[0], t[2], t[1], t[3], probe), reference, "another swap");
            assert_eq!(insphere(t[3], t[2], t[1], t[0], probe), reference, "reversed");
            assert_eq!(insphere(t[1], t[2], t[0], t[3], probe), reference, "a rotation");
        }
    }

    // A tie is reported as a tie, not rounded to a side. §7.4 breaks cospherical ties by smallest
    // `NodeKey`, which it can only do if the predicate says a tie happened.
    #[test]
    fn five_cospherical_points_are_a_tie() {
        let sphere = [
            Vec3::new(1.0, 0.0, 0.0),
            Vec3::new(0.0, 1.0, 0.0),
            Vec3::new(0.0, 0.0, 1.0),
            Vec3::new(-1.0, 0.0, 0.0),
        ];
        assert_eq!(
            insphere(sphere[0], sphere[1], sphere[2], sphere[3], Vec3::new(0.0, -1.0, 0.0)),
            0
        );
    }

    // Four coplanar points have no circumsphere, so there is no side to report. Returning one would
    // hide a degenerate tet inside a triangulation that must never contain it.
    #[test]
    fn a_coplanar_tet_has_no_circumsphere() {
        assert_eq!(
            insphere(
                Vec3::new(0.0, 0.0, 0.0),
                Vec3::new(1.0, 0.0, 0.0),
                Vec3::new(0.0, 1.0, 0.0),
                Vec3::new(1.0, 1.0, 0.0),
                Vec3::new(0.5, 0.5, 1.0)
            ),
            0
        );
    }

    // Exactness, stated where it matters: a point four ULP inside the sphere and one four ULP
    // outside it must read differently. This is the regime a naive determinant cannot resolve, and
    // the regime a Delaunay insertion spends all its time in.
    #[test]
    fn the_test_resolves_a_point_a_few_ulp_from_the_sphere() {
        let sphere = [
            Vec3::new(1.0, 0.0, 0.0),
            Vec3::new(0.0, 1.0, 0.0),
            Vec3::new(0.0, 0.0, 1.0),
            Vec3::new(-1.0, 0.0, 0.0),
        ];
        let inside = insphere(
            sphere[0],
            sphere[1],
            sphere[2],
            sphere[3],
            Vec3::new(0.0, -(1.0 - 1.0e-15), 0.0),
        );
        let outside = insphere(
            sphere[0],
            sphere[1],
            sphere[2],
            sphere[3],
            Vec3::new(0.0, -(1.0 + 1.0e-15), 0.0),
        );
        assert_eq!(inside, 1, "a point inside the sphere by 1e-15");
        assert_eq!(outside, -1, "a point outside the sphere by 1e-15");
    }
}
