// AI-FUNC-SUMMARY: WGSL shader for the GPU scene preview (PLAN §9.2/§9.5 GPU path).
// Two pipelines share these uniforms: a TriangleList pass with a per-vertex colour
// attribute and two-sided headlight shading matching the CPU reference exactly, and a
// LineList pass for curve/wireframe overlays. Both honour an optional clip plane via
// fragment discard. The GPU path is opaque by design — exact transparency lives in the
// CPU reference renderer.

struct Uniforms {
    mvp: mat4x4<f32>,
    camera_forward: vec4<f32>,
    camera_eye: vec4<f32>,
    // xyz: plane normal, w: offset; a fragment is discarded when dot(n, p) + w > 0
    clip_plane: vec4<f32>,
    // x: ambient intensity, y: projection flag (0 = orthographic, 1 = perspective),
    // z: clip-plane enable, w: line depth bias in clip units
    params: vec4<f32>,
};

@group(0) @binding(0) var<uniform> uniforms: Uniforms;

// AI-FUNC-SUMMARY: True when the clip plane is enabled and the point is on its positive side.
fn clipped(world_position: vec3<f32>) -> bool {
    if (uniforms.params.z < 0.5) {
        return false;
    }
    return dot(uniforms.clip_plane.xyz, world_position) + uniforms.clip_plane.w > 0.0;
}

// AI-FUNC-SUMMARY: Two-sided headlight intensity; mirrors geometry::scene_render::shade.
fn headlight(world_normal: vec3<f32>, world_position: vec3<f32>) -> f32 {
    let ambient = uniforms.params.x;
    var to_camera = -uniforms.camera_forward.xyz;
    if (uniforms.params.y > 0.5) {
        to_camera = uniforms.camera_eye.xyz - world_position;
    }
    let n_len = length(world_normal);
    let t_len = length(to_camera);
    var ndl = 0.0;
    if (n_len > 1e-12 && t_len > 1e-12) {
        ndl = abs(dot(world_normal / n_len, to_camera / t_len));
    }
    return ambient + (1.0 - ambient) * clamp(ndl, 0.0, 1.0);
}

struct TriangleInput {
    @location(0) position: vec3<f32>,
    @location(1) normal: vec3<f32>,
    @location(2) color: vec4<f32>,
};

struct TriangleOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) world_normal: vec3<f32>,
    @location(1) world_position: vec3<f32>,
    @location(2) color: vec4<f32>,
};

@vertex
fn vs_tri(input: TriangleInput) -> TriangleOutput {
    var out: TriangleOutput;
    out.clip_position = uniforms.mvp * vec4<f32>(input.position, 1.0);
    out.world_normal = input.normal;
    out.world_position = input.position;
    out.color = input.color;
    return out;
}

@fragment
fn fs_tri(input: TriangleOutput) -> @location(0) vec4<f32> {
    if (clipped(input.world_position)) {
        discard;
    }
    let intensity = headlight(input.world_normal, input.world_position);
    return vec4<f32>(input.color.rgb * intensity, 1.0);
}

struct LineInput {
    @location(0) position: vec3<f32>,
    @location(1) color: vec4<f32>,
};

struct LineOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) world_position: vec3<f32>,
    @location(1) color: vec4<f32>,
};

@vertex
fn vs_line(input: LineInput) -> LineOutput {
    var out: LineOutput;
    var clip = uniforms.mvp * vec4<f32>(input.position, 1.0);
    // Pull overlay lines toward the camera so curves lying exactly on a surface stay
    // visible, matching the CPU renderer's depth bias. Scaling by w keeps the bias
    // uniform in NDC across the depth range.
    clip.z = clip.z - uniforms.params.w * clip.w;
    out.clip_position = clip;
    out.world_position = input.position;
    out.color = input.color;
    return out;
}

@fragment
fn fs_line(input: LineOutput) -> @location(0) vec4<f32> {
    if (clipped(input.world_position)) {
        discard;
    }
    return vec4<f32>(input.color.rgb, 1.0);
}
