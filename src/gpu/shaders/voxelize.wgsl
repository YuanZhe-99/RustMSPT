// AI-FUNC-SUMMARY: WGSL compute shader for certified voxelization.
// Each invocation tests one voxel center (f32(i) + 0.5) * pitch in the bbox-origin frame with the certified
// f32 ray-parity test. Proven cells store 0/1; a cell whose f32 classification cannot be proven equal to the
// CPU f64 reference stores 0 and appends its flat index to the uncertain list for host re-evaluation.

struct Params {
    num_triangles: u32,
    nx: u32,
    ny: u32,
    nz: u32,
    pitch: f32,
    _pad0: u32,
    ray_dir: vec3<f32>,
    _pad1: u32,
    mesh_lo: vec3<f32>,
    flags: u32,
    mesh_hi: vec3<f32>,
    _pad2: u32,
};

@group(0) @binding(0) var<storage, read> triangles: array<f32>;
@group(0) @binding(1) var<storage, read> params: Params;
@group(0) @binding(2) var<storage, read_write> occupancy: array<u32>;
@group(0) @binding(3) var<storage, read_write> uncertain: array<atomic<u32>>;
@group(0) @binding(4) var<storage, read> tri_const: array<vec4<f32>>;

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
fn cert_ray_triangle(o: vec3<f32>, d: vec3<f32>, i: u32, pm: f32) -> CertHit {
    var r: CertHit;
    r.t = -1.0;
    r.err = 0.0;
    r.state = CERT_FALSE;
    // Query-independent terms, precomputed per triangle on the host (certify::triangle_constants) for the
    // one fixed ray direction; the shader formerly recomputed them for every (query, triangle) pair.
    let k0 = tri_const[i * 4u];
    let k1 = tri_const[i * 4u + 1u];
    let k2 = tri_const[i * 4u + 2u];
    let k3 = tri_const[i * 4u + 3u];
    let a = k0.xyz;
    let m = k0.w;
    let e1 = k1.xyz;
    let es = k1.w;
    let e2 = k2.xyz;
    let eps_e = 4.0 * CERT_U * m;
    let s = o - a;
    let eps_s = CERT_U * (pm + 2.0 * m);
    let ss = max3(abs(s)) + eps_s;
    let h = k3.xyz;
    let eps_h = 6.0 * CERT_U * es + 2.0 * eps_e;
    let hs = 2.0 * (1.0 + CERT_U) * es + eps_h;
    let det = k3.w;
    let eps_det = k2.w;
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

// AI-FUNC-SUMMARY: Run the certified test for triangle i from its precomputed constants with the precomputed query magnitude pm; returns CertHit.
fn cert_triangle(i: u32, point: vec3<f32>, dir: vec3<f32>, pm: f32) -> CertHit {
    return cert_ray_triangle(point, dir, i, pm);
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

@compute @workgroup_size(64)
fn main(@builtin(global_invocation_id) gid: vec3<u32>, @builtin(num_workgroups) groups: vec3<u32>) {
    let idx = gid.x + gid.y * groups.x * 64u;
    let total = params.nx * params.ny * params.nz;
    if (idx >= total) { return; }

    let z = idx % params.nz;
    let y = (idx / params.nz) % params.ny;
    let x = idx / (params.nz * params.ny);

    let pitch = params.pitch;
    let center = vec3<f32>(
        (f32(x) + 0.5) * pitch,
        (f32(y) + 0.5) * pitch,
        (f32(z) + 0.5) * pitch,
    );

    let state = point_inside(center);
    if (state == CERT_UNKNOWN) {
        occupancy[idx] = 0u;
        let slot = atomicAdd(&uncertain[0], 1u);
        if (slot + 1u < arrayLength(&uncertain)) {
            atomicStore(&uncertain[slot + 1u], idx);
        }
    } else {
        occupancy[idx] = state;
    }
}
