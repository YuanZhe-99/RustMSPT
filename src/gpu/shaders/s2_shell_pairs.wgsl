// AI-FUNC-SUMMARY: WGSL compute shader for direct shell S2 computation.
// Each invocation handles one (radius, offset) pair: counts valid and hit pairs
// in the occupancy grid for that displacement.

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

@compute @workgroup_size(256)
fn main(@builtin(global_invocation_id) gid: vec3<u32>) {
    let idx = gid.x;
    if (idx >= params.total_offsets) { return; }

    let entry = offsets[idx];
    let dx = entry.dx;
    let dy = entry.dy;
    let dz = entry.dz;
    let nx = params.nx;
    let ny = params.ny;
    let nz = params.nz;

    var x_start: u32 = 0u;
    var x_end: u32 = nx;
    var y_start: u32 = 0u;
    var y_end: u32 = ny;
    var z_start: u32 = 0u;
    var z_end: u32 = nz;

    if (dx < 0) { x_start = u32(-dx); }
    else if (dx > 0) { x_end = nx - u32(dx); }
    if (dy < 0) { y_start = u32(-dy); }
    else if (dy > 0) { y_end = ny - u32(dy); }
    if (dz < 0) { z_start = u32(-dz); }
    else if (dz > 0) { z_end = nz - u32(dz); }

    if (x_start >= x_end || y_start >= y_end || z_start >= z_end) {
        out_valid[idx] = 0u;
        out_hits[idx] = 0u;
        return;
    }

    var valid: u32 = 0u;
    var hits: u32 = 0u;

    for (var x = x_start; x < x_end; x++) {
        for (var y = y_start; y < y_end; y++) {
            for (var z = z_start; z < z_end; z++) {
                let i1 = x * ny * nz + y * nz + z;
                let x2 = u32(i32(x) + dx);
                let y2 = u32(i32(y) + dy);
                let z2 = u32(i32(z) + dz);
                let i2 = x2 * ny * nz + y2 * nz + z2;
                valid++;
                if (occupancy[i1] == 1u && occupancy[i2] == 1u) {
                    hits++;
                }
            }
        }
    }

    out_valid[idx] = valid;
    out_hits[idx] = hits;
}
