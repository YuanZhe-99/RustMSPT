// AI-FUNC-SUMMARY: WGSL render shader for offscreen STL visualization.
// Vertex stage transforms expanded triangle vertices by the view-projection matrix; the fragment
// stage applies two-sided headlight shading (ambient + diffuse toward the camera).

struct Uniforms {
    mvp: mat4x4<f32>,
    camera_forward: vec4<f32>,
    camera_eye: vec4<f32>,
    base_color: vec4<f32>,
    // x: ambient intensity, y: projection flag (0 = orthographic, 1 = perspective)
    params: vec4<f32>,
};

@group(0) @binding(0) var<uniform> uniforms: Uniforms;

struct VertexInput {
    @location(0) position: vec3<f32>,
    @location(1) normal: vec3<f32>,
};

struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) world_normal: vec3<f32>,
    @location(1) world_position: vec3<f32>,
};

@vertex
fn vs_main(input: VertexInput) -> VertexOutput {
    var out: VertexOutput;
    out.clip_position = uniforms.mvp * vec4<f32>(input.position, 1.0);
    out.world_normal = input.normal;
    out.world_position = input.position;
    return out;
}

@fragment
fn fs_main(input: VertexOutput) -> @location(0) vec4<f32> {
    let ambient = uniforms.params.x;
    let is_perspective = uniforms.params.y > 0.5;

    var to_camera = -uniforms.camera_forward.xyz;
    if (is_perspective) {
        to_camera = uniforms.camera_eye.xyz - input.world_position;
    }

    let n_len = length(input.world_normal);
    let t_len = length(to_camera);
    var ndl = 0.0;
    if (n_len > 1e-12 && t_len > 1e-12) {
        ndl = abs(dot(input.world_normal / n_len, to_camera / t_len));
    }

    let intensity = ambient + (1.0 - ambient) * clamp(ndl, 0.0, 1.0);
    return vec4<f32>(uniforms.base_color.rgb * intensity, 1.0);
}
