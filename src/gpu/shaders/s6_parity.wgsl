// AI-FUNC-SUMMARY: WGSL compute shader for S6's vertex parity (plan M-7, GK-1). Each invocation shoots one ray
// from one point through the triangles of its projection-grid bucket (the host supplies the bucket, computed in
// f64) and counts crossings with every orient3d certified under numerics §6.1 G1 rev 1.4 - the f32 operands'
// own error bounds included. It writes 0 (outside) or 1 (inside) when every test was certified and 2 when any
// was not, which sends the point to the CPU path; certified answers equal the CPU's by construction
// (`classify::parity_f32_certified` is the reference).

struct Params {
    point_count: u32,
    row_stride: u32,
    pad0: u32,
    pad1: u32,
    // The ray: its direction; w is unused (the kernel forms no far endpoint).
    dir: vec4<f32>,
};

@group(0) @binding(0) var<storage, read> tris: array<f32>;
@group(0) @binding(1) var<storage, read> bucket_start: array<u32>;
@group(0) @binding(2) var<storage, read> bucket_items: array<u32>;
@group(0) @binding(3) var<storage, read> points: array<f32>;
@group(0) @binding(4) var<storage, read> point_bucket: array<u32>;
@group(0) @binding(5) var<uniform> params: Params;
@group(0) @binding(6) var<storage, read_write> result: array<u32>;

const U: f32 = 5.9604645e-8;
// G1's c_flt for f32, (7 + 56u)u.
const C_FLT: f32 = 4.1723279e-7;
// Slack on the bound for evaluating it in f32 and for any fused multiply-add the driver applies.
const SLACK: f32 = 1.0001;

// Sign of orient3d(a, b, c, d) in the project convention when certified, 0 when not.
fn orient(a: vec3<f32>, b: vec3<f32>, c: vec3<f32>, d: vec3<f32>,
          ea: vec3<f32>, eb: vec3<f32>, ec: vec3<f32>, ed: vec3<f32>) -> i32 {
    let ad = a - d;
    let bd = b - d;
    let cd = c - d;
    let raw = ad.z * (bd.x * cd.y - cd.x * bd.y) + bd.z * (cd.x * ad.y - ad.x * cd.y) + cd.z * (ad.x * bd.y - bd.x * ad.y);
    let perm = (abs(bd.x * cd.y) + abs(cd.x * bd.y)) * abs(ad.z)
        + (abs(cd.x * ad.y) + abs(ad.x * cd.y)) * abs(bd.z)
        + (abs(ad.x * bd.y) + abs(bd.x * ad.y)) * abs(cd.z);
    let sa = ea + ed;
    let sb = eb + ed;
    let sc = ec + ed;
    let w = 1.0 / (1.0 - U);
    let ma = abs(ad) * w;
    let mb = abs(bd) * w;
    let mc = abs(cd) * w;
    let t0 = (mb.x + sb.x) * (mc.y + sc.y) * (ma.z + sa.z) - mb.x * mc.y * ma.z;
    let t1 = (mc.x + sc.x) * (mb.y + sb.y) * (ma.z + sa.z) - mc.x * mb.y * ma.z;
    let t2 = (mc.x + sc.x) * (ma.y + sa.y) * (mb.z + sb.z) - mc.x * ma.y * mb.z;
    let t3 = (ma.x + sa.x) * (mc.y + sc.y) * (mb.z + sb.z) - ma.x * mc.y * mb.z;
    let t4 = (ma.x + sa.x) * (mb.y + sb.y) * (mc.z + sc.z) - ma.x * mb.y * mc.z;
    let t5 = (mb.x + sb.x) * (ma.y + sa.y) * (mc.z + sc.z) - mb.x * ma.y * mc.z;
    // The products' differences are computed in f32 too; widen them by a relative rounding term.
    let operands = (t0 + t1 + t2 + t3 + t4 + t5) * SLACK
        + 16.0 * U * ((mb.x + sb.x) * (mc.y + sc.y) * (ma.z + sa.z) + (mc.x + sc.x) * (mb.y + sb.y) * (ma.z + sa.z)
            + (mc.x + sc.x) * (ma.y + sa.y) * (mb.z + sb.z) + (ma.x + sa.x) * (mc.y + sc.y) * (mb.z + sb.z)
            + (ma.x + sa.x) * (mb.y + sb.y) * (mc.z + sc.z) + (mb.x + sb.x) * (ma.y + sa.y) * (mc.z + sc.z));
    let bound = (C_FLT * perm * SLACK + operands) * SLACK;
    if (abs(raw) > bound) {
        if (raw > 0.0) {
            return -1;
        }
        return 1;
    }
    return 0;
}

fn conv(v: vec3<f32>) -> vec3<f32> {
    return abs(v) * (U / (1.0 - U)) * SLACK;
}

fn tri_point(t: u32, k: u32) -> vec3<f32> {
    let base = t * 9u + k * 3u;
    return vec3<f32>(tris[base], tris[base + 1u], tris[base + 2u]);
}

// A difference of two operands and its distance from the true difference.
fn sub_err(ex: vec3<f32>, ey: vec3<f32>, d: vec3<f32>) -> vec3<f32> {
    return (ex + ey + abs(d) * (U / (1.0 - U))) * SLACK;
}

@compute @workgroup_size(64)
fn main(@builtin(global_invocation_id) gid: vec3<u32>) {
    let i = gid.x + gid.y * params.row_stride;
    if (i >= params.point_count) {
        return;
    }
    let p = vec3<f32>(points[i * 3u], points[i * 3u + 1u], points[i * 3u + 2u]);
    let dir = params.dir.xyz;
    let ep = conv(p);
    let edir = conv(dir);
    let zero = vec3<f32>(0.0, 0.0, 0.0);
    let bucket = point_bucket[i];
    let first = bucket_start[bucket];
    let last = bucket_start[bucket + 1u];
    var crossings: u32 = 0u;
    for (var s: u32 = first; s < last; s = s + 1u) {
        let t = bucket_items[s];
        let a = tri_point(t, 0u);
        let b = tri_point(t, 1u);
        let c = tri_point(t, 2u);
        let ea = conv(a);
        let eb = conv(b);
        let ec = conv(c);
        // The point's side of the plane, and the side the ray ends on beyond the box:
        // orient3d(a, b, c, p + t*dir) tends to t*det[dir, b - a, c - a].
        let side_p = orient(a, b, c, p, ea, eb, ec, ep);
        let ba = b - a;
        let ca = c - a;
        let side_far = orient(dir, ba, ca, zero, edir, sub_err(eb, ea, ba), sub_err(ec, ea, ca), zero);
        if (side_p == 0 || side_far == 0) {
            result[i] = 2u;
            return;
        }
        if (side_p == side_far) {
            continue;
        }
        // orient3d(p, q, x, y) = reach*det[p - y, dir, x - y]: no far endpoint is formed.
        let pb = p - b;
        let ab = a - b;
        let pc = p - c;
        let bc = b - c;
        let pa = p - a;
        let s0 = orient(pb, dir, ab, zero, sub_err(ep, eb, pb), edir, sub_err(ea, eb, ab), zero);
        let s1 = orient(pc, dir, bc, zero, sub_err(ep, ec, pc), edir, sub_err(eb, ec, bc), zero);
        let s2 = orient(pa, dir, ca, zero, sub_err(ep, ea, pa), edir, sub_err(ec, ea, ca), zero);
        if (s0 == 0 || s1 == 0 || s2 == 0) {
            result[i] = 2u;
            return;
        }
        if (s0 == s1 && s1 == s2) {
            crossings = crossings + 1u;
        }
    }
    result[i] = crossings % 2u;
}
