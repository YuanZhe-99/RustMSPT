// AI-FUNC-SUMMARY: WGSL compute shader for state-map point location. Each invocation walks the tet BVH for one
// point and records every leaf slot whose outward-rounded f32 box holds the point's f32 coordinates, up to k
// slots; the count is always the full number found, so a count above k tells the host to settle the point itself.

struct Params {
    point_count: u32,
    k: u32,
    row_stride: u32,
    pad: u32,
};

@group(0) @binding(0) var<storage, read> node_box: array<f32>;
@group(0) @binding(1) var<storage, read> node_link: array<u32>;
@group(0) @binding(2) var<storage, read> slot_box: array<f32>;
@group(0) @binding(3) var<storage, read> points: array<f32>;
@group(0) @binding(4) var<uniform> params: Params;
@group(0) @binding(5) var<storage, read_write> counts: array<u32>;
@group(0) @binding(6) var<storage, read_write> slots: array<u32>;

fn node_holds(base: u32, p: vec3<f32>) -> bool {
    return p.x >= node_box[base] && p.y >= node_box[base + 1u] && p.z >= node_box[base + 2u]
        && p.x <= node_box[base + 3u] && p.y <= node_box[base + 4u] && p.z <= node_box[base + 5u];
}

fn slot_holds(base: u32, p: vec3<f32>) -> bool {
    return p.x >= slot_box[base] && p.y >= slot_box[base + 1u] && p.z >= slot_box[base + 2u]
        && p.x <= slot_box[base + 3u] && p.y <= slot_box[base + 4u] && p.z <= slot_box[base + 5u];
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
    var found: u32 = 0u;
    loop {
        if (top == 0u) {
            break;
        }
        top = top - 1u;
        let node = stack[top];
        if (!node_holds(node * 6u, p)) {
            continue;
        }
        let first = node_link[node * 2u];
        let count = node_link[node * 2u + 1u];
        if (count == 0u) {
            stack[top] = first + 1u;
            stack[top + 1u] = first;
            top = top + 2u;
            continue;
        }
        for (var s: u32 = first; s < first + count; s = s + 1u) {
            if (slot_holds(s * 6u, p)) {
                if (found < params.k) {
                    slots[i * params.k + found] = s;
                }
                found = found + 1u;
            }
        }
    }
    counts[i] = found;
}
