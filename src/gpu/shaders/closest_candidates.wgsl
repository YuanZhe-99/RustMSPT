// AI-FUNC-SUMMARY: WGSL compute shader for the closest pipeline. Each invocation walks the primitive BVH for one
// query point in f32 (coordinates relative to the host's origin), nearer child first, pruning boxes farther than the
// best distance so far, and records the primitive it found nearest. The host only uses it as a seed: it computes the
// seed's exact f64 distance and searches within it, so f32 rounding can never change the answer.

struct Params {
    point_count: u32,
    triangles: u32,
    row_stride: u32,
    pad: u32,
};

@group(0) @binding(0) var<storage, read> node_box: array<f32>;
@group(0) @binding(1) var<storage, read> node_link: array<u32>;
@group(0) @binding(2) var<storage, read> slot_corner: array<f32>;
@group(0) @binding(3) var<storage, read> slot_prim: array<u32>;
@group(0) @binding(4) var<storage, read> points: array<f32>;
@group(0) @binding(5) var<uniform> params: Params;
@group(0) @binding(6) var<storage, read_write> seeds: array<u32>;

fn box_distance2(base: u32, p: vec3<f32>) -> f32 {
    let lo = vec3<f32>(node_box[base], node_box[base + 1u], node_box[base + 2u]);
    let hi = vec3<f32>(node_box[base + 3u], node_box[base + 4u], node_box[base + 5u]);
    let g = max(max(lo - p, p - hi), vec3<f32>(0.0));
    return dot(g, g);
}

fn on_segment(p: vec3<f32>, a: vec3<f32>, b: vec3<f32>) -> vec3<f32> {
    let ab = b - a;
    let len2 = dot(ab, ab);
    if (len2 <= 0.0) {
        return a;
    }
    let t = clamp(dot(p - a, ab) / len2, 0.0, 1.0);
    return a + t * ab;
}

fn on_triangle(p: vec3<f32>, a: vec3<f32>, b: vec3<f32>, c: vec3<f32>) -> vec3<f32> {
    let ab = b - a;
    let ac = c - a;
    let ap = p - a;
    let d1 = dot(ab, ap);
    let d2 = dot(ac, ap);
    if (d1 <= 0.0 && d2 <= 0.0) {
        return a;
    }
    let bp = p - b;
    let d3 = dot(ab, bp);
    let d4 = dot(ac, bp);
    if (d3 >= 0.0 && d4 <= d3) {
        return b;
    }
    let vc = d1 * d4 - d3 * d2;
    if (vc <= 0.0 && d1 >= 0.0 && d3 <= 0.0) {
        return a + (d1 / (d1 - d3)) * ab;
    }
    let cp = p - c;
    let d5 = dot(ab, cp);
    let d6 = dot(ac, cp);
    if (d6 >= 0.0 && d5 <= d6) {
        return c;
    }
    let vb = d5 * d2 - d1 * d6;
    if (vb <= 0.0 && d2 >= 0.0 && d6 <= 0.0) {
        return a + (d2 / (d2 - d6)) * ac;
    }
    let va = d3 * d6 - d5 * d4;
    if (va <= 0.0 && (d4 - d3) >= 0.0 && (d5 - d6) >= 0.0) {
        return b + ((d4 - d3) / ((d4 - d3) + (d5 - d6))) * (c - b);
    }
    let denom = va + vb + vc;
    if (!(denom > 0.0)) {
        return on_segment(p, a, b);
    }
    return a + ab * (vb / denom) + ac * (vc / denom);
}

@compute @workgroup_size(64)
fn main(@builtin(global_invocation_id) gid: vec3<u32>) {
    let i = gid.x + gid.y * params.row_stride;
    if (i >= params.point_count) {
        return;
    }
    let p = vec3<f32>(points[i * 3u], points[i * 3u + 1u], points[i * 3u + 2u]);
    var stack: array<u32, 64>;
    var top: u32 = 1u;
    stack[0] = 0u;
    var best: f32 = 3.0e38;
    var chosen: u32 = 0xffffffffu;
    loop {
        if (top == 0u) {
            break;
        }
        top = top - 1u;
        let node = stack[top];
        if (box_distance2(node * 6u, p) > best) {
            continue;
        }
        let first = node_link[node * 2u];
        let count = node_link[node * 2u + 1u];
        if (count == 0u) {
            let da = box_distance2(first * 6u, p);
            let db = box_distance2((first + 1u) * 6u, p);
            if (da <= db) {
                stack[top] = first + 1u;
                stack[top + 1u] = first;
            } else {
                stack[top] = first;
                stack[top + 1u] = first + 1u;
            }
            top = top + 2u;
            continue;
        }
        for (var s: u32 = first; s < first + count; s = s + 1u) {
            let base = s * 9u;
            let a = vec3<f32>(slot_corner[base], slot_corner[base + 1u], slot_corner[base + 2u]);
            let b = vec3<f32>(slot_corner[base + 3u], slot_corner[base + 4u], slot_corner[base + 5u]);
            let c = vec3<f32>(slot_corner[base + 6u], slot_corner[base + 7u], slot_corner[base + 8u]);
            var q: vec3<f32>;
            if (params.triangles != 0u) {
                q = on_triangle(p, a, b, c);
            } else {
                q = on_segment(p, a, b);
            }
            let d = dot(p - q, p - q);
            if (d < best) {
                best = d;
                chosen = slot_prim[s];
            }
        }
    }
    seeds[i] = chosen;
}
