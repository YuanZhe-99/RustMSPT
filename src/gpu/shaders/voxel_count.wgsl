// AI-FUNC-SUMMARY: Count binary voxel occupancy on device with bounded strided reads and integer workgroup reduction; emit one u32.
@group(0) @binding(0) var<storage, read> occupancy: array<u32>;
@group(0) @binding(1) var<storage, read> params: array<u32>;
@group(0) @binding(2) var<storage, read_write> count: array<u32>;
var<workgroup> partials: array<u32, 256>;
@compute @workgroup_size(256)
fn main(@builtin(local_invocation_index) lane: u32) {
    let total = params[1] * params[2] * params[3];
    var sum = 0u;
    var cell = lane;
    loop {
        if (cell >= total) { break; }
        sum += occupancy[cell];
        if (total - cell <= 256u) { break; }
        cell += 256u;
    }
    partials[lane] = sum;
    workgroupBarrier();
    var stride = 128u;
    loop {
        if (lane < stride) { partials[lane] += partials[lane + stride]; }
        workgroupBarrier();
        if (stride == 1u) { break; }
        stride /= 2u;
    }
    if (lane == 0u) { count[0] = partials[0]; }
}
