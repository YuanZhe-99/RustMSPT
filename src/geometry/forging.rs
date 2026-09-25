use super::bbox::mesh_bbox;
use crate::types::{BoundingBox, Mesh, Vec3};

// AI-FUNC-SUMMARY:
// Purpose: Apply FFD-style forging deformation: compress along Z and bulge laterally around the mesh center.
// Inputs: mesh, compression_ratio (0..1), bulge_factor (0..1 controlling lateral expansion).
// Returns: Deformed mesh clone.
// Side effects: None.
// Notes: Assumes Z-axis compression. Uses axis_scale = 1 - compression_ratio, lateral_scale = (1/axis_scale)^bulge_factor.
pub fn simulate_forging_ffd(mesh: &Mesh, compression_ratio: f64, bulge_factor: f64) -> Mesh {
    let bbox = mesh_bbox(mesh).unwrap_or(BoundingBox {
        min: Vec3::new(0.0, 0.0, 0.0),
        max: Vec3::new(1.0, 1.0, 1.0),
    });
    let center = Vec3::new(
        (bbox.min.x + bbox.max.x) * 0.5,
        (bbox.min.y + bbox.max.y) * 0.5,
        (bbox.min.z + bbox.max.z) * 0.5,
    );

    let mut out = mesh.clone();
    let axis_scale = (1.0 - compression_ratio).clamp(0.01, 1.0);
    let lateral_scale = (1.0 / axis_scale.sqrt()).powf(bulge_factor.clamp(0.0, 1.0));

    super::mesh_ops::map_vertices(&mut out.vertices, |v| {
        let local = v.sub(center);
        Vec3::new(
            center.x + local.x * lateral_scale,
            center.y + local.y * lateral_scale,
            center.z + local.z * axis_scale,
        )
    });

    out
}

// AI-FUNC-SUMMARY:
// Purpose: Apply FFD forging deformation with configurable compression axis, void mesh densification, and ROI tracking.
// Inputs: mesh, lattice_bbox (defines center), track_bbox (optional ROI to track through deformation), compression_ratio, compression_axis (0=x,1=y,2=z), bulge_factor, mesh_type ("void" triggers densification), void_densification (scaling factor).
// Returns: Tuple of (deformed mesh, optionally tracked and transformed ROI bounding box).
// Side effects: None.
// Notes: Void-type meshes get a centroid-based closure scaling. ROI tracking transforms all 8 corners and recomputes the AABB.
pub fn simulate_forging_ffd_with_tracking(
    mesh: &Mesh,
    lattice_bbox: BoundingBox,
    track_bbox: Option<BoundingBox>,
    compression_ratio: f64,
    compression_axis: usize,
    bulge_factor: f64,
    mesh_type: &str,
    void_densification: f64,
) -> (Mesh, Option<BoundingBox>) {
    forge_owned(
        mesh.clone(),
        lattice_bbox,
        track_bbox,
        compression_ratio,
        compression_axis,
        bulge_factor,
        mesh_type,
        void_densification,
    )
}

// AI-FUNC-SUMMARY: Consume a mesh and apply the existing FFD/void/ROI mapping in place; preserves the public clone-returning wrapper and exact post-transform centroid accumulation order, fusing the void centroid sum into the transform pass when that pass is serial.
pub fn forge_owned(
    mut out: Mesh,
    lattice_bbox: BoundingBox,
    track_bbox: Option<BoundingBox>,
    compression_ratio: f64,
    compression_axis: usize,
    bulge_factor: f64,
    mesh_type: &str,
    void_densification: f64,
) -> (Mesh, Option<BoundingBox>) {
    let center = Vec3::new(
        (lattice_bbox.min.x + lattice_bbox.max.x) * 0.5,
        (lattice_bbox.min.y + lattice_bbox.max.y) * 0.5,
        (lattice_bbox.min.z + lattice_bbox.max.z) * 0.5,
    );

    let axis_scale = (1.0 - compression_ratio).clamp(0.01, 1.0);
    let lateral_scale = (1.0 / axis_scale.sqrt()).powf(bulge_factor.clamp(0.0, 1.0));

    let transform_point = |p: Vec3| {
        let local = p.sub(center);
        match compression_axis {
            0 => Vec3::new(
                center.x + local.x * axis_scale,
                center.y + local.y * lateral_scale,
                center.z + local.z * lateral_scale,
            ),
            1 => Vec3::new(
                center.x + local.x * lateral_scale,
                center.y + local.y * axis_scale,
                center.z + local.z * lateral_scale,
            ),
            _ => Vec3::new(
                center.x + local.x * lateral_scale,
                center.y + local.y * lateral_scale,
                center.z + local.z * axis_scale,
            ),
        }
    };

    if mesh_type.eq_ignore_ascii_case("void") {
        let closure = (1.0 - 0.05 * compression_ratio * void_densification).clamp(0.85, 1.0);
        let c = super::mesh_ops::map_vertices_centroid(&mut out.vertices, transform_point);
        super::mesh_ops::map_vertices(&mut out.vertices, |v| c.add(v.sub(c).scale(closure)));
    } else {
        super::mesh_ops::map_vertices(&mut out.vertices, transform_point);
    }

    let tracked = track_bbox.map(|tb| {
        let corners = [
            Vec3::new(tb.min.x, tb.min.y, tb.min.z),
            Vec3::new(tb.min.x, tb.min.y, tb.max.z),
            Vec3::new(tb.min.x, tb.max.y, tb.min.z),
            Vec3::new(tb.min.x, tb.max.y, tb.max.z),
            Vec3::new(tb.max.x, tb.min.y, tb.min.z),
            Vec3::new(tb.max.x, tb.min.y, tb.max.z),
            Vec3::new(tb.max.x, tb.max.y, tb.min.z),
            Vec3::new(tb.max.x, tb.max.y, tb.max.z),
        ];

        let mut min_p = transform_point(corners[0]);
        let mut max_p = min_p;
        for p in corners.iter().skip(1).copied().map(transform_point) {
            min_p.x = min_p.x.min(p.x);
            min_p.y = min_p.y.min(p.y);
            min_p.z = min_p.z.min(p.z);
            max_p.x = max_p.x.max(p.x);
            max_p.y = max_p.y.max(p.y);
            max_p.z = max_p.z.max(p.z);
        }
        BoundingBox {
            min: min_p,
            max: max_p,
        }
    });

    (out, tracked)
}
