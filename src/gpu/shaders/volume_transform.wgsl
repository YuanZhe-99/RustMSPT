// AI-FUNC-SUMMARY: WGSL compute shader for volume rotate-and-crop transform.
// Each invocation computes one output voxel by inverse-rotating to source coordinates
// and sampling with nearest or trilinear interpolation.

struct Params {
    src_width: u32,
    src_height: u32,
    src_depth: u32,
    out_width: u32,
    out_height: u32,
    out_depth: u32,
    interp_mode: u32,  // 0=nearest, 1=trilinear
    background: i32,
    // Rotation matrix (3x3, row-major, stored as 3 vec4 for alignment)
    rot_row0: vec4<f32>,
    rot_row1: vec4<f32>,
    rot_row2: vec4<f32>,
    // Centroid
    centroid: vec4<f32>,
    // Output origin (x0, y0, z0, pad)
    origin: vec4<f32>,
};

@group(0) @binding(0) var<storage, read> src_volume: array<i32>;
@group(0) @binding(1) var<storage, read> params: Params;
@group(0) @binding(2) var<storage, read_write> out_volume: array<i32>;

fn idx3d(x: u32, y: u32, z: u32) -> u32 {
    return z * params.src_width * params.src_height + y * params.src_width + x;
}

// AI-FUNC-SUMMARY: Round ties away from zero like Rust f64::round without perturbing exactly represented large integers.
fn round_away(value: f32) -> f32 {
    let whole = trunc(value);
    return select(whole, whole + sign(value), abs(value - whole) >= 0.5);
}

fn sample_nearest(sx: f32, sy: f32, sz: f32) -> i32 {
    let x = i32(round_away(sx));
    let y = i32(round_away(sy));
    let z = i32(round_away(sz));
    if (x < 0 || y < 0 || z < 0 ||
        x >= i32(params.src_width) || y >= i32(params.src_height) || z >= i32(params.src_depth)) {
        return params.background;
    }
    return src_volume[idx3d(u32(x), u32(y), u32(z))];
}

fn sample_trilinear(sx: f32, sy: f32, sz: f32) -> i32 {
    let x0 = i32(floor(sx));
    let y0 = i32(floor(sy));
    let z0 = i32(floor(sz));
    let x1 = x0 + 1;
    let y1 = y0 + 1;
    let z1 = z0 + 1;

    let tx = sx - f32(x0);
    let ty = sy - f32(y0);
    let tz = sz - f32(z0);

    let sw = i32(params.src_width);
    let sh = i32(params.src_height);
    let sd = i32(params.src_depth);
    let bg = f32(params.background);

    var c000 = bg;
    if (x0 >= 0 && y0 >= 0 && z0 >= 0 && x0 < sw && y0 < sh && z0 < sd) {
        c000 = f32(src_volume[idx3d(u32(x0), u32(y0), u32(z0))]);
    }
    var c100 = bg;
    if (x1 >= 0 && y0 >= 0 && z0 >= 0 && x1 < sw && y0 < sh && z0 < sd) {
        c100 = f32(src_volume[idx3d(u32(x1), u32(y0), u32(z0))]);
    }
    var c010 = bg;
    if (x0 >= 0 && y1 >= 0 && z0 >= 0 && x0 < sw && y1 < sh && z0 < sd) {
        c010 = f32(src_volume[idx3d(u32(x0), u32(y1), u32(z0))]);
    }
    var c110 = bg;
    if (x1 >= 0 && y1 >= 0 && z0 >= 0 && x1 < sw && y1 < sh && z0 < sd) {
        c110 = f32(src_volume[idx3d(u32(x1), u32(y1), u32(z0))]);
    }
    var c001 = bg;
    if (x0 >= 0 && y0 >= 0 && z1 >= 0 && x0 < sw && y0 < sh && z1 < sd) {
        c001 = f32(src_volume[idx3d(u32(x0), u32(y0), u32(z1))]);
    }
    var c101 = bg;
    if (x1 >= 0 && y0 >= 0 && z1 >= 0 && x1 < sw && y0 < sh && z1 < sd) {
        c101 = f32(src_volume[idx3d(u32(x1), u32(y0), u32(z1))]);
    }
    var c011 = bg;
    if (x0 >= 0 && y1 >= 0 && z1 >= 0 && x0 < sw && y1 < sh && z1 < sd) {
        c011 = f32(src_volume[idx3d(u32(x0), u32(y1), u32(z1))]);
    }
    var c111 = bg;
    if (x1 >= 0 && y1 >= 0 && z1 >= 0 && x1 < sw && y1 < sh && z1 < sd) {
        c111 = f32(src_volume[idx3d(u32(x1), u32(y1), u32(z1))]);
    }

    let c00 = c000 * (1.0 - tx) + c100 * tx;
    let c10 = c010 * (1.0 - tx) + c110 * tx;
    let c01 = c001 * (1.0 - tx) + c101 * tx;
    let c11 = c011 * (1.0 - tx) + c111 * tx;
    let c0 = c00 * (1.0 - ty) + c10 * ty;
    let c1 = c01 * (1.0 - ty) + c11 * ty;
    let value = c0 * (1.0 - tz) + c1 * tz;

    return i32(round_away(value));
}

@compute @workgroup_size(64)
fn main(@builtin(global_invocation_id) gid: vec3<u32>, @builtin(num_workgroups) groups: vec3<u32>) {
    let index = gid.x + gid.y * groups.x * 64u;
    let total = params.out_width * params.out_height * params.out_depth;
    if (index >= total) { return; }
    let x = index % params.out_width;
    let y = (index / params.out_width) % params.out_height;
    let z = index / (params.out_width * params.out_height);

    let local = vec3<f32>(
        params.origin.x + f32(x),
        params.origin.y + f32(y),
        params.origin.z + f32(z),
    );

    // Inverse rotation: src = rot * local + centroid
    let src_x = params.rot_row0.x * local.x + params.rot_row0.y * local.y + params.rot_row0.z * local.z + params.centroid.x;
    let src_y = params.rot_row1.x * local.x + params.rot_row1.y * local.y + params.rot_row1.z * local.z + params.centroid.y;
    let src_z = params.rot_row2.x * local.x + params.rot_row2.y * local.y + params.rot_row2.z * local.z + params.centroid.z;

    var value: i32;
    if (params.interp_mode == 0u) {
        value = sample_nearest(src_x, src_y, src_z);
    } else {
        value = sample_trilinear(src_x, src_y, src_z);
    }

    let out_idx = z * params.out_width * params.out_height + y * params.out_width + x;
    out_volume[out_idx] = value;
}
