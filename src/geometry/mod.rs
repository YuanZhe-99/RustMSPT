pub mod bbox;
pub mod collision;
pub mod forging;
pub mod mesh_ops;
pub mod mesh_query;
pub mod metrics;
pub mod quaternion;
pub mod render;
pub mod s2;
pub mod scene_render;
pub mod spatial;
pub mod void_index;
pub mod volume;

pub use bbox::{bbox_distance, bbox_overlaps, check_boundary_constraints_mode, mesh_bbox};
pub use collision::{
    generate_periodic_ghosts, mesh_collision_exact, mesh_collision_exact_prepared,
    mesh_closer_than_prepared, mesh_distance_exact, mesh_distance_exact_prepared, mesh_solids_nested_prepared,
    mesh_surfaces_intersect_prepared, to_parry_trimesh, trimesh_contains_point,
};
pub use forging::{simulate_forging_ffd, simulate_forging_ffd_with_tracking};
pub use mesh_ops::{
    box_mesh, icosphere_mesh, merge_meshes, mesh_centroid, mesh_surface_area, move_mesh_to_target_center,
    rotate_mesh_around_center, scale_mesh, split_mesh_into_granules, translate_mesh, vec_norm,
    wrap_mesh_centroid_to_box,
};
pub use metrics::{
    mesh_closedness, mesh_is_closed, mesh_metrics, scale_mesh_to_equivalent_diameter, MeshMetrics,
};
pub use quaternion::{sample_uniform_quaternion, transform_shell, UnitQuat};
pub use void_index::{VoidIndex, VoidVolumeMethod};
pub use render::{
    build_render_camera, parse_render_projection, parse_render_vec3, render_mesh_cpu,
    RenderCamera, RenderCameraSpec, RenderProjection, RenderSettings,
};
pub use s2::{approximate_s2, calculate_s2, calculate_s2_seeded, l2_norm, point_inside_mesh, shell_offsets_for_distance};
pub use scene_render::{named_view, render_scene_cpu, SceneRenderSettings};
#[cfg(feature = "gpu")]
pub use s2::{calculate_s2_with_gpu, calculate_s2_gpu_exact};
pub use volume::{
    cut_face_names, mesh_volume_centroid, mesh_volume_in_bbox_exact, shell_signed_volumes,
    DOMAIN_FACE_NAMES,
    clip_mesh_by_bbox, mesh_signed_volume, mesh_volume, orient_components_to_positive_volume,
    particle_volume_in_bbox, volume_fraction_in_bbox, volume_fraction_of_meshes_in_bbox,
};

pub use mesh_query::{MeshQueryScratch, PreparedMeshQuery};
