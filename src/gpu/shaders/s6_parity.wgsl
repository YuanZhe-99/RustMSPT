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
    // The ray: direction and far-endpoint distance (any point beyond the domain box).
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

@compute @workgroup_size(64)
fn main(@builtin(global_invocation_id) gid: vec3<u32>) {
    let i = gid.x + gid.y * params.row_stride;
    if (i >= params.point_count) {
        return;
    }
    let p = vec3<f32>(points[i * 3u], points[i * 3u + 1u], points[i * 3u + 2u]);
    let dir = params.dir.xyz;
    let reach = params.dir.w;
    let q = p + dir * reach;
    let ep = conv(p);
    let eq = 5.0 * U * (abs(p) + abs(dir * reach)) * SLACK * SLACK;
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
        let side_p = orient(a, b, c, p, ea, eb, ec, ep);
        let side_q = orient(a, b, c, q, ea, eb, ec, eq);
        if (side_p == 0 || side_q == 0) {
            result[i] = 2u;
            return;
        }
        if (side_p == side_q) {
            continue;
        }
        let s0 = orient(p, q, a, b, ep, eq, ea, eb);
        let s1 = orient(p, q, b, c, ep, eq, eb, ec);
        let s2 = orient(p, q, c, a, ep, eq, ec, ea);
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
