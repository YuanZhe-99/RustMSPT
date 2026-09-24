// AI-FUNC-SUMMARY: Merge tile counts to one exact integer pair per offset; the checked grid size bounds each sum by u32.
struct Params {
    total_offsets: u32,
    nx: u32,
    ny: u32,
    nz: u32,
    tiles_per_offset: u32,
    tile_voxels: u32,
};
@group(0) @binding(0) var<storage, read> valid_tiles: array<u32>;
@group(0) @binding(1) var<storage, read> hit_tiles: array<u32>;
@group(0) @binding(2) var<storage, read> params: Params;
@group(0) @binding(3) var<storage, read_write> valid_offsets: array<u32>;
@group(0) @binding(4) var<storage, read_write> hit_offsets: array<u32>;

@compute @workgroup_size(256)
fn main(@builtin(global_invocation_id) gid: vec3<u32>, @builtin(num_workgroups) groups: vec3<u32>) {
    let offset = gid.x + gid.y * groups.x * 256u;
    if (offset >= params.total_offsets) { return; }
    let begin = offset * params.tiles_per_offset;
    var valid = 0u;
    var hits = 0u;
    for (var tile = 0u; tile < params.tiles_per_offset; tile++) {
        valid += valid_tiles[begin + tile];
        hits += hit_tiles[begin + tile];
    }
    valid_offsets[offset] = valid;
    hit_offsets[offset] = hits;
}
