// AI-FUNC-SUMMARY: WGSL compute shader for direct shell S2 computation.
// Each workgroup handles one offset, cooperatively scans overlap voxels and reduces exact integer hits.

struct Params {
    total_offsets: u32,
    nx: u32,
    ny: u32,
    nz: u32,
};

struct OffsetEntry {
    radius_idx: u32,
    dx: i32,
    dy: i32,
    dz: i32,
};

@group(0) @binding(0) var<storage, read> occupancy: array<u32>;
@group(0) @binding(1) var<storage, read> offsets: array<OffsetEntry>;
@group(0) @binding(2) var<storage, read> params: Params;
@group(0) @binding(3) var<storage, read_write> out_valid: array<u32>;
@group(0) @binding(4) var<storage, read_write> out_hits: array<u32>;

var<workgroup> partial_hits: array<u32, 256>;

@compute @workgroup_size(256)
fn main(@builtin(local_invocation_index) lane: u32,
        @builtin(workgroup_id) group: vec3<u32>,
        @builtin(num_workgroups) groups: vec3<u32>) {
    let idx = group.x + group.y * groups.x;
    if (idx >= params.total_offsets) { return; }
    let entry = offsets[idx];
    let shift = vec3<i32>(entry.dx, entry.dy, entry.dz);
    let magnitude = select(vec3<u32>(shift), vec3<u32>(0u) - vec3<u32>(shift), shift < vec3<i32>(0));
    let dims = vec3<u32>(params.nx, params.ny, params.nz);
    if (any(magnitude >= dims)) {
        if (lane == 0u) { out_valid[idx] = 0u; out_hits[idx] = 0u; }
        return;
    }
    let extent = dims - magnitude;
    let start = select(vec3<u32>(0u), magnitude, shift < vec3<i32>(0));
    let target_start = select(magnitude, vec3<u32>(0u), shift < vec3<i32>(0));
    // Host checks the full grid product, bounding all indices and integer counts.
    let valid = extent.x * extent.y * extent.z;
    var hits = 0u;
    var local = lane;
    loop {
        if (local >= valid) { break; }
        let z = local % extent.z;
        let row = local / extent.z;
        let relative = vec3<u32>(row / extent.y, row % extent.y, z);
        let a = start + relative;
        let b = target_start + relative;
        let ia = (a.x * dims.y + a.y) * dims.z + a.z;
        let ib = (b.x * dims.y + b.y) * dims.z + b.z;
        hits += select(0u, 1u, occupancy[ia] == 1u && occupancy[ib] == 1u);
        if (valid - local <= 256u) { break; }
        local += 256u;
    }
    partial_hits[lane] = hits;
    workgroupBarrier();
    var stride = 128u;
    loop {
        if (lane < stride) { partial_hits[lane] += partial_hits[lane + stride]; }
        workgroupBarrier();
        if (stride == 1u) { break; }
        stride /= 2u;
    }
    if (lane == 0u) {
        out_valid[idx] = valid;
        out_hits[idx] = partial_hits[0];
    }
}
