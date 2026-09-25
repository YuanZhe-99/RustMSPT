// AI-FUNC-SUMMARY: Frozen pre-certification MC shader (raw f32 parity, 1e-6 tolerances) kept only as the benchmark baseline for certification overhead.
// Each invocation processes one (radius, sample) pair: casts a ray from a random point
// through the mesh, counts intersections to determine inside/outside, checks partner point.
// Output: per-invocation hit and valid counts, summed on CPU.

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

// AI-FUNC-SUMMARY: Recover overflow parity by repeatedly selecting the next distinct positive hit; uses constant storage and the fast path's anchored 1e-6 deduplication tolerance.
fn point_inside_overflow(point: vec3<f32>, dir: vec3<f32>) -> bool {
    var last_t: f32 = -1.0;
    var unique: u32 = 0u;
    for (var step: u32 = 0u; step < params.num_triangles; step++) {
        var nearest: f32 = -1.0;
        for (var i: u32 = 0u; i < params.num_triangles; i++) {
            let base = i * 9u;
            let a = vec3<f32>(triangles[base], triangles[base + 1u], triangles[base + 2u]);
            let b = vec3<f32>(triangles[base + 3u], triangles[base + 4u], triangles[base + 5u]);
            let c = vec3<f32>(triangles[base + 6u], triangles[base + 7u], triangles[base + 8u]);
            let t = ray_triangle(point, dir, a, b, c);
            if (t > 0.0 && (unique == 0u || t - last_t > 1e-6)) {
                if (nearest < 0.0 || t < nearest) { nearest = t; }
            }
        }
        if (nearest < 0.0) { break; }
        unique++;
        last_t = nearest;
    }
    return (unique & 1u) == 1u;
}

// AI-FUNC-SUMMARY: Classify containment with a 64-hit sorted fast path; recover the full ray on its 65th positive triangle hit.
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
        if (t > 0.0) {
            if (hit_count == MAX_HITS) { return point_inside_overflow(point, dir); }
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

// AI-FUNC-SUMMARY: Evaluate one global logical sample id for a batch-local radius slot, returning hit/valid flags without output writes or workgroup synchronization.
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

    let p_in = point_inside(p);
    let q_in = point_inside(q);

    return vec2<u32>(select(0u, 1u, p_in && q_in), 1u);
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
