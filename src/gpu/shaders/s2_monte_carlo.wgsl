// AI-FUNC-SUMMARY: WGSL compute shader for certified Monte Carlo S2 two-point correlation.
// Each invocation processes one (radius, sample) pair: generates p and its partner q, classifies both with the
// certified f32 ray-parity test, and reduces hit/valid flags per workgroup. A sample whose f32 classification cannot
// be proven equal to the CPU f64 reference contributes valid=1, hit=0 and is appended (id, p, q) to the uncertain
// list so the host re-evaluates exactly those points with the CPU predicate.

struct Params {
    num_triangles: u32,
    num_radii: u32,
    samples_per_radius: u32,
    seed: u32,
    bbox_min: vec3<f32>,
    radius_base: u32,
    bbox_max: vec3<f32>,
    _pad1: u32,
    ray_dir: vec3<f32>,
    _pad2: u32,
    radii: array<f32, 128>,
    mesh_lo: vec3<f32>,
    flags: u32,
    mesh_hi: vec3<f32>,
    _pad3: u32,
};

@group(0) @binding(0) var<storage, read> triangles: array<f32>;
@group(0) @binding(1) var<storage, read> params: Params;
@group(0) @binding(2) var<storage, read_write> out_hits: array<u32>;
@group(0) @binding(3) var<storage, read_write> out_valids: array<u32>;
@group(0) @binding(4) var<storage, read_write> uncertain: array<atomic<u32>>;

const UNCERTAIN_WORDS: u32 = 7u;

fn pcg_hash(input: u32) -> u32 {
    var state = input * 747796405u + 2891336453u;
    let word = ((state >> ((state >> 28u) + 4u)) ^ state) * 277803737u;
    return (word >> 22u) ^ word;
}

fn rand_f32(state: ptr<function, u32>) -> f32 {
    *state = pcg_hash(*state);
    return f32(*state) / 4294967295.0;
}

const CERT_U: f32 = 5.9604645e-8;
const CERT_SAFETY: f32 = 2.0;
const CERT_TINY: f32 = 1e-36;
const CPU_EPS: f32 = 1e-10;
const CPU_DEDUP: f32 = 1e-8;
const CERT_FALSE: u32 = 0u;
const CERT_TRUE: u32 = 1u;
const CERT_UNKNOWN: u32 = 2u;
const MAX_HITS: u32 = 64u;

struct CertHit {
    t: f32,
    err: f32,
    state: u32,
};

// AI-FUNC-SUMMARY: Largest component of a vector; returns f32; side effects: None.
fn max3(v: vec3<f32>) -> f32 {
    return max(v.x, max(v.y, v.z));
}

// AI-FUNC-SUMMARY: Decide the CPU comparison `x_cpu >= thr` (equivalently `> thr`) from an f32 estimate and its error bound; returns CERT_TRUE/CERT_FALSE when proven, CERT_UNKNOWN otherwise (including NaN or infinite bounds).
fn cert_ge(x: f32, err: f32, thr: f32) -> u32 {
    let slack = CERT_U * (abs(thr) + abs(x) + err) + CERT_TINY;
    if (x - err - slack >= thr) { return CERT_TRUE; }
    if (x + err + slack < thr) { return CERT_FALSE; }
    return CERT_UNKNOWN;
}

// AI-FUNC-SUMMARY: Error bound of a Moller-Trumbore ratio N/det from its numerator bound, the determinant bound and the certified denominator floor; returns f32; side effects: None.
fn cert_ratio_err(eps_n: f32, r: f32, eps_det: f32, den: f32) -> f32 {
    return (eps_n + 1.01 * abs(r) * eps_det) / den + CERT_SAFETY * 7.0 * CERT_U * abs(r);
}

// AI-FUNC-SUMMARY: Certified Moller-Trumbore test mirroring the CPU thresholds (1e-10 on det, u, v, u+v, t); a division-free numerator test first proves |u| or |v| exceeds 1 for near-parallel far triangles; returns a certain miss, a certain hit with distance and its error bound, or CERT_UNKNOWN.
fn cert_ray_triangle(o: vec3<f32>, d: vec3<f32>, a: vec3<f32>, b: vec3<f32>, c: vec3<f32>, pm: f32) -> CertHit {
    var r: CertHit;
    r.t = -1.0;
    r.err = 0.0;
    r.state = CERT_FALSE;
    let m = max3(max(max(abs(a), abs(b)), abs(c))) * (1.0 + CERT_U);
    let e1 = b - a;
    let e2 = c - a;
    let eps_e = 4.0 * CERT_U * m;
    let es = max(max3(abs(e1)), max3(abs(e2))) + eps_e;
    let s = o - a;
    let eps_s = CERT_U * (pm + 2.0 * m);
    let ss = max3(abs(s)) + eps_s;
    let h = cross(d, e2);
    let eps_h = 6.0 * CERT_U * es + 2.0 * eps_e;
    let hs = 2.0 * (1.0 + CERT_U) * es + eps_h;
    let det = dot(e1, h);
    let eps_det = CERT_SAFETY * (9.0 * CERT_U * es * hs + 3.0 * (eps_e * hs + es * eps_h)) + CERT_TINY;
    let adet = abs(det);
    let det_state = cert_ge(adet, eps_det, CPU_EPS);
    if (det_state == CERT_FALSE) { return r; }
    let un = dot(s, h);
    let eps_un = CERT_SAFETY * (9.0 * CERT_U * ss * hs + 3.0 * (eps_s * hs + ss * eps_h)) + CERT_TINY;
    let det_hi = (adet + eps_det) * (1.0 + 16.0 * CERT_U) + CERT_TINY;
    let solvable = adet > eps_det;
    let den = adet - eps_det;
    let inv_det = 1.0 / det;
    var ub: f32 = 0.0;
    var eu: f32 = 0.0;
    var s_lo = CERT_UNKNOWN;
    var s_hi = CERT_UNKNOWN;
    if (abs(un) - eps_un > det_hi) { return r; }
    if (solvable) {
        ub = un * inv_det;
        eu = cert_ratio_err(eps_un, ub, eps_det, den);
        s_lo = cert_ge(ub, eu, -CPU_EPS);
        s_hi = cert_ge(-ub, eu, -(1.0 + CPU_EPS));
        if (s_lo == CERT_FALSE || s_hi == CERT_FALSE) { return r; }
    }
    let q = cross(s, e1);
    let eps_q = 4.0 * CERT_U * ss * es + 2.0 * (eps_s * es + ss * eps_e);
    let qs = 2.0 * ss * es + eps_q;
    let vn = dot(d, q);
    let eps_vn = CERT_SAFETY * (12.0 * CERT_U * qs + 3.0 * eps_q) + CERT_TINY;
    if (abs(vn) - eps_vn > det_hi) { return r; }
    if (!solvable) {
        r.state = CERT_UNKNOWN;
        return r;
    }
    let vb = vn * inv_det;
    let ev = cert_ratio_err(eps_vn, vb, eps_det, den);
    let s_v = cert_ge(vb, ev, -CPU_EPS);
    let uv = ub + vb;
    let s_uv = cert_ge(-uv, eu + ev + CERT_U * abs(uv), -(1.0 + CPU_EPS));
    if (s_v == CERT_FALSE || s_uv == CERT_FALSE) { return r; }
    let tb = dot(e2, q) * inv_det;
    let et = cert_ratio_err(CERT_SAFETY * (9.0 * CERT_U * es * qs + 3.0 * (eps_e * qs + es * eps_q)) + CERT_TINY, tb, eps_det, den);
    let s_t = cert_ge(tb, et, CPU_EPS);
    if (s_t == CERT_FALSE) { return r; }
    if (det_state == CERT_TRUE && s_lo == CERT_TRUE && s_hi == CERT_TRUE && s_v == CERT_TRUE && s_uv == CERT_TRUE && s_t == CERT_TRUE) {
        r.t = tb;
        r.err = et;
        r.state = CERT_TRUE;
        return r;
    }
    r.state = CERT_UNKNOWN;
    return r;
}

// AI-FUNC-SUMMARY: Load triangle i from the flat f32 buffer and run the certified test with the precomputed query magnitude pm; returns CertHit.
fn cert_triangle(i: u32, point: vec3<f32>, dir: vec3<f32>, pm: f32) -> CertHit {
    let base = i * 9u;
    let a = vec3<f32>(triangles[base], triangles[base + 1u], triangles[base + 2u]);
    let b = vec3<f32>(triangles[base + 3u], triangles[base + 4u], triangles[base + 5u]);
    let c = vec3<f32>(triangles[base + 6u], triangles[base + 7u], triangles[base + 8u]);
    return cert_ray_triangle(point, dir, a, b, c, pm);
}

// AI-FUNC-SUMMARY: Recover certified parity after more than 64 raw hits with constant storage: one bound pass, then repeated nearest selection that proves every consecutive gap exceeds the CPU 1e-8 dedup band; returns 0/1 or CERT_UNKNOWN.
fn point_inside_overflow(point: vec3<f32>, dir: vec3<f32>, pm: f32) -> u32 {
    let n = params.num_triangles;
    var emax: f32 = 0.0;
    for (var i: u32 = 0u; i < n; i++) {
        let hit = cert_triangle(i, point, dir, pm);
        if (hit.state == CERT_UNKNOWN) { return CERT_UNKNOWN; }
        if (hit.state == CERT_TRUE) { emax = max(emax, hit.err); }
    }
    let band = CPU_DEDUP + 2.0 * emax;
    var last: f32 = -1.0;
    var unique: u32 = 0u;
    for (var step: u32 = 0u; step < n; step++) {
        var nearest: f32 = -1.0;
        for (var i: u32 = 0u; i < n; i++) {
            let hit = cert_triangle(i, point, dir, pm);
            if (hit.state == CERT_TRUE && hit.t > last && (nearest < 0.0 || hit.t < nearest)) {
                nearest = hit.t;
            }
        }
        if (nearest < 0.0) { break; }
        var close: u32 = 0u;
        for (var i: u32 = 0u; i < n; i++) {
            let hit = cert_triangle(i, point, dir, pm);
            if (hit.state == CERT_TRUE && hit.t >= nearest && !(hit.t - nearest > band + CERT_U * abs(hit.t))) {
                close++;
            }
        }
        if (close != 1u) { return CERT_UNKNOWN; }
        unique++;
        last = nearest;
    }
    return unique & 1u;
}

// AI-FUNC-SUMMARY: Certified point containment matching the CPU reference: exact host-rounded mesh-bbox early-out, certified per-triangle hits, then a 64-hit sorted path proving all hits distinct beyond the dedup band (overflow recovery above 64); returns 0 outside, 1 inside, CERT_UNKNOWN when f32 cannot prove the CPU decision.
fn point_inside(point: vec3<f32>) -> u32 {
    if (point.x < params.mesh_lo.x || point.y < params.mesh_lo.y || point.z < params.mesh_lo.z ||
        point.x > params.mesh_hi.x || point.y > params.mesh_hi.y || point.z > params.mesh_hi.z) {
        return 0u;
    }
    let dir = params.ray_dir;
    let pm = max3(abs(point));
    var ts: array<f32, 64>;
    var hit_count: u32 = 0u;
    var emax: f32 = 0.0;
    let n = params.num_triangles;
    for (var i: u32 = 0u; i < n; i++) {
        let hit = cert_triangle(i, point, dir, pm);
        if (hit.state == CERT_UNKNOWN) { return CERT_UNKNOWN; }
        if (hit.state == CERT_TRUE) {
            if (hit_count == MAX_HITS) { return point_inside_overflow(point, dir, pm); }
            ts[hit_count] = hit.t;
            emax = max(emax, hit.err);
            hit_count++;
        }
    }
    if (hit_count == 0u) { return 0u; }
    for (var i: u32 = 1u; i < hit_count; i++) {
        let key = ts[i];
        var j: u32 = i;
        while (j > 0u && ts[j - 1u] > key) {
            ts[j] = ts[j - 1u];
            j--;
        }
        ts[j] = key;
    }
    let band = CPU_DEDUP + 2.0 * emax;
    for (var i: u32 = 1u; i < hit_count; i++) {
        if (!(ts[i] - ts[i - 1u] > band + CERT_U * abs(ts[i]))) { return CERT_UNKNOWN; }
    }
    return hit_count & 1u;
}

// AI-FUNC-SUMMARY: Append one sample (logical id, exact f32 p and q bits) to the uncertain list; the counter always advances so the host can detect and regrow an overflowed list.
fn record_uncertain(idx: u32, p: vec3<f32>, q: vec3<f32>) {
    let slot = atomicAdd(&uncertain[0], 1u);
    let capacity = (arrayLength(&uncertain) - 1u) / UNCERTAIN_WORDS;
    if (slot < capacity) {
        let base = 1u + slot * UNCERTAIN_WORDS;
        atomicStore(&uncertain[base], idx);
        atomicStore(&uncertain[base + 1u], bitcast<u32>(p.x));
        atomicStore(&uncertain[base + 2u], bitcast<u32>(p.y));
        atomicStore(&uncertain[base + 3u], bitcast<u32>(p.z));
        atomicStore(&uncertain[base + 4u], bitcast<u32>(q.x));
        atomicStore(&uncertain[base + 5u], bitcast<u32>(q.y));
        atomicStore(&uncertain[base + 6u], bitcast<u32>(q.z));
    }
}

// AI-FUNC-SUMMARY: Evaluate one global logical sample id for a batch-local radius slot, returning certain hit/valid flags; uncertain (or, with flags bit 0, every valid) samples are recorded and contribute hit=0.
fn sample_counts(r_idx: u32, idx: u32) -> vec2<u32> {

    var rng = pcg_hash(params.seed ^ idx ^ 0x9e3779b9u);

    let bb_min = params.bbox_min;
    let bb_max = params.bbox_max;
    let size = bb_max - bb_min;

    let px = bb_min.x + rand_f32(&rng) * size.x;
    let py = bb_min.y + rand_f32(&rng) * size.y;
    let pz = bb_min.z + rand_f32(&rng) * size.z;
    let p = vec3<f32>(px, py, pz);

    let dx = rand_f32(&rng) * 2.0 - 1.0;
    let dy = rand_f32(&rng) * 2.0 - 1.0;
    let dz = rand_f32(&rng) * 2.0 - 1.0;
    let n2 = dx * dx + dy * dy + dz * dz;
    if (n2 < 1e-12 || n2 > 1.0) {
        return vec2<u32>(0u, 0u);
    }
    let inv_n = inverseSqrt(n2);
    let dir = vec3<f32>(dx * inv_n, dy * inv_n, dz * inv_n);

    let r = params.radii[r_idx];
    let q = p + dir * r;

    if (q.x < bb_min.x || q.y < bb_min.y || q.z < bb_min.z ||
        q.x > bb_max.x || q.y > bb_max.y || q.z > bb_max.z) {
        return vec2<u32>(0u, 0u);
    }

    if ((params.flags & 1u) != 0u) {
        record_uncertain(idx, p, q);
        return vec2<u32>(0u, 1u);
    }
    let p_in = point_inside(p);
    let q_in = point_inside(q);
    if (p_in == 0u || q_in == 0u) {
        return vec2<u32>(0u, 1u);
    }
    if (p_in == 1u && q_in == 1u) {
        return vec2<u32>(1u, 1u);
    }
    record_uncertain(idx, p, q);
    return vec2<u32>(0u, 1u);
}

// AI-FUNC-SUMMARY: Reduce one radius/sample block to bounded u32 hit/valid partials using uniform barriers; padded lanes contribute zero.
// Each workgroup owns one radius/sample block of the current radius batch. Padded lanes contribute zero;
// logical sample ids use the global radius (radius_base + slot), so the RNG stream is independent of padding and batching.
var<workgroup> block_hits: array<u32, 256>;
var<workgroup> block_valids: array<u32, 256>;

@compute @workgroup_size(256)
fn main(@builtin(local_invocation_index) lane: u32, @builtin(workgroup_id) group: vec3<u32>, @builtin(num_workgroups) groups: vec3<u32>) {
    let blocks = (params.samples_per_radius - 1u) / 256u + 1u;
    let block = group.x + group.y * groups.x;
    // Uniform for every lane: padded workgroups return before any barrier.
    if (block >= params.num_radii * blocks) { return; }
    let radius = block / blocks;
    let sample = (block % blocks) * 256u + lane;
    var counts = vec2<u32>(0u, 0u);
    if (sample < params.samples_per_radius) {
        counts = sample_counts(radius, (params.radius_base + radius) * params.samples_per_radius + sample);
    }
    block_hits[lane] = counts.x;
    block_valids[lane] = counts.y;
    workgroupBarrier();
    for (var stride = 128u; stride > 0u; stride = stride / 2u) {
        if (lane < stride) {
            block_hits[lane] += block_hits[lane + stride];
            block_valids[lane] += block_valids[lane + stride];
        }
        workgroupBarrier();
    }
    if (lane == 0u) {
        out_hits[block] = block_hits[0];
        out_valids[block] = block_valids[0];
    }
}
