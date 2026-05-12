// AI-FUNC-SUMMARY: WGSL compute shader for Monte Carlo S2 two-point correlation.
// Each invocation processes one (radius, sample) pair: casts a ray from a random point
// through the mesh, counts intersections to determine inside/outside, checks partner point.
// Output: per-invocation hit and valid counts, summed on CPU.

struct Params {
    num_triangles: u32,
    num_radii: u32,
    samples_per_radius: u32,
    seed: u32,
    bbox_min: vec3<f32>,
    _pad0: u32,
    bbox_max: vec3<f32>,
    _pad1: u32,
    ray_dir: vec3<f32>,
    _pad2: u32,
    radii: array<f32, 128>,
};

@group(0) @binding(0) var<storage, read> triangles: array<f32>;
@group(0) @binding(1) var<storage, read> params: Params;
@group(0) @binding(2) var<storage, read_write> out_hits: array<u32>;
@group(0) @binding(3) var<storage, read_write> out_valids: array<u32>;

fn pcg_hash(input: u32) -> u32 {
    var state = input * 747796405u + 2891336453u;
    let word = ((state >> ((state >> 28u) + 4u)) ^ state) * 277803737u;
    return (word >> 22u) ^ word;
}

fn rand_f32(state: ptr<function, u32>) -> f32 {
    *state = pcg_hash(*state);
    return f32(*state) / 4294967295.0;
}

fn ray_triangle(origin: vec3<f32>, dir: vec3<f32>, a: vec3<f32>, b: vec3<f32>, c: vec3<f32>) -> f32 {
    let edge1 = b - a;
    let edge2 = c - a;
    let h = cross(dir, edge2);
    let det = dot(edge1, h);
    if (abs(det) <= 1e-10) {
        return -1.0;
    }
    let inv_det = 1.0 / det;
    let s = origin - a;
    let u = inv_det * dot(s, h);
    if (u < -1e-6 || u > 1.0 + 1e-6) {
        return -1.0;
    }
    let q = cross(s, edge1);
    let v = inv_det * dot(dir, q);
    if (v < -1e-6 || u + v > 1.0 + 1e-6) {
        return -1.0;
    }
    let t = inv_det * dot(edge2, q);
    return select(-1.0, t, t > 1e-10);
}

const MAX_HITS: u32 = 64u;

fn point_inside(point: vec3<f32>) -> bool {
    let eps = 1e-6;
    if (point.x < params.bbox_min.x - eps || point.y < params.bbox_min.y - eps || point.z < params.bbox_min.z - eps ||
        point.x > params.bbox_max.x + eps || point.y > params.bbox_max.y + eps || point.z > params.bbox_max.z + eps) {
        return false;
    }

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

    if (hit_count == 0u) {
        return false;
    }

    // Insertion sort by distance
    for (var i: u32 = 1u; i < hit_count; i++) {
        let key = ts[i];
        var j: u32 = i;
        while (j > 0u && ts[j - 1u] > key) {
            ts[j] = ts[j - 1u];
            j--;
        }
        ts[j] = key;
    }

    // Deduplicate near-equal hits (matches CPU tolerance 1e-8)
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

@compute @workgroup_size(256)
fn main(@builtin(global_invocation_id) gid: vec3<u32>) {
    let total = params.num_radii * params.samples_per_radius;
    let idx = gid.x;
    if (idx >= total) {
        return;
    }

    let r_idx = idx / params.samples_per_radius;
    let s_idx = idx % params.samples_per_radius;

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
        out_hits[idx] = 0u;
        out_valids[idx] = 0u;
        return;
    }
    let inv_n = inverseSqrt(n2);
    let dir = vec3<f32>(dx * inv_n, dy * inv_n, dz * inv_n);

    let r = params.radii[r_idx];
    let q = p + dir * r;

    if (q.x < bb_min.x || q.y < bb_min.y || q.z < bb_min.z ||
        q.x > bb_max.x || q.y > bb_max.y || q.z > bb_max.z) {
        out_hits[idx] = 0u;
        out_valids[idx] = 0u;
        return;
    }

    let p_in = point_inside(p);
    let q_in = point_inside(q);

    out_hits[idx] = select(0u, 1u, p_in && q_in);
    out_valids[idx] = 1u;
}
