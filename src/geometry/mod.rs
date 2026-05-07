pub mod bbox;
pub mod collision;
pub mod forging;
pub mod mesh_ops;
pub mod s2;
pub mod spatial;
pub mod volume;

pub use bbox::{bbox_distance, bbox_overlaps, check_boundary_constraints_mode, mesh_bbox};
pub use collision::{
    generate_periodic_ghosts, mesh_collision_exact, mesh_collision_exact_prepared,
    mesh_distance_exact, mesh_distance_exact_prepared, to_parry_trimesh,
};
pub use forging::{simulate_forging_ffd, simulate_forging_ffd_with_tracking};
pub use mesh_ops::{
    box_mesh, merge_meshes, mesh_centroid, mesh_surface_area, move_mesh_to_target_center,
    rotate_mesh_around_center, scale_mesh, split_mesh_into_granules, translate_mesh, vec_norm,
    wrap_mesh_centroid_to_box,
};
pub use s2::{approximate_s2, calculate_s2, l2_norm};
pub use volume::{
    clip_mesh_by_bbox, mesh_signed_volume, mesh_volume, orient_components_to_positive_volume,
    particle_volume_in_bbox, volume_fraction_in_bbox, volume_fraction_of_meshes_in_bbox,
};
