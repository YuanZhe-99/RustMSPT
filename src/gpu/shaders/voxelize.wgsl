// AI-FUNC-SUMMARY: WGSL compute shader for voxelization.
// Each invocation tests one voxel center for mesh containment via ray-casting.
// Coordinates are normalized to bbox origin (0,0,0).

struct Params {
    num_triangles: u32,
    nx: u32,
    ny: u32,
    nz: u32,
    pitch: f32,
    _pad0: u32,
    ray_dir: vec3<f32>,
    _pad1: u32,
};

@group(0) @binding(0) var<storage, read> triangles: array<f32>;
@group(0) @binding(1) var<storage, read> params: Params;
@group(0) @binding(2) var<storage, read_write> occupancy: array<u32>;

const MAX_HITS: u32 = 64u;

fn ray_triangle(origin: vec3<f32>, dir: vec3<f32>, a: vec3<f32>, b: vec3<f32>, c: vec3<f32>) -> f32 {
    let edge1 = b - a;
    let edge2 = c - a;
    let h = cross(dir, edge2);
    let det = dot(edge1, h);
    if (abs(det) <= 1e-10) { return -1.0; }
    let inv_det = 1.0 / det;
    let s = origin - a;
    let u = inv_det * dot(s, h);
    if (u < -1e-6 || u > 1.0 + 1e-6) { return -1.0; }
    let q = cross(s, edge1);
    let v = inv_det * dot(dir, q);
    if (v < -1e-6 || u + v > 1.0 + 1e-6) { return -1.0; }
    let t = inv_det * dot(edge2, q);
    return select(-1.0, t, t > 1e-10);
}

fn point_inside(point: vec3<f32>) -> bool {
    let dir = params.ray_dir;
    var ts: array<f32, 64>;
    var hit_count: u32 = 0u;
    let n = params.num_triangles;

    for (var i: u32 = 0u; i < n; i++) {
        let base = i * 9u;
        let a = vec3<f32>(triangles[base], triangles[base + 1u], triangles[base + 2u]);
        let b = vec3<f32>(triangles[base + 3u], triangles[base + 4u], triangles[base + 5u]);
        let c = vec3<f32>(triangles[base + 6u], triangles[base + 7u], triangles[base + 8u]);
        let t = ray_triangle(point, dir, a, b, c);
        if (t > 0.0 && hit_count < MAX_HITS) {
            ts[hit_count] = t;
            hit_count++;
        }
    }

    if (hit_count == 0u) { return false; }

    for (var i: u32 = 1u; i < hit_count; i++) {
        let key = ts[i];
        var j: u32 = i;
        while (j > 0u && ts[j - 1u] > key) {
            ts[j] = ts[j - 1u];
            j--;
        }
        ts[j] = key;
    }

    var unique: u32 = 1u;
    var last_t = ts[0];
    for (var i: u32 = 1u; i < hit_count; i++) {
        if (ts[i] - last_t > 1e-6) {
            unique++;
            last_t = ts[i];
        }
    }

    return (unique & 1u) == 1u;
}

@compute @workgroup_size(64)
fn main(@builtin(global_invocation_id) gid: vec3<u32>) {
    let idx = gid.x;
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

    if (point_inside(center)) {
        occupancy[idx] = 1u;
    } else {
        occupancy[idx] = 0u;
    }
}
