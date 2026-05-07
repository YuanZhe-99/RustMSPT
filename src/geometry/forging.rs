use crate::types::{BoundingBox, Mesh, Vec3};
use super::bbox::mesh_bbox;

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

    for v in &mut out.vertices {
        let local = v.sub(center);
        *v = Vec3::new(
            center.x + local.x * lateral_scale,
            center.y + local.y * lateral_scale,
            center.z + local.z * axis_scale,
        );
    }

    out
}

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

    let mut out = mesh.clone();
    for v in &mut out.vertices {
        *v = transform_point(*v);
    }

    if mesh_type.eq_ignore_ascii_case("void") {
        let closure = (1.0 - 0.05 * compression_ratio * void_densification).clamp(0.85, 1.0);
        let c = super::mesh_ops::mesh_centroid(&out);
        for v in &mut out.vertices {
            let local = v.sub(c);
            *v = c.add(local.scale(closure));
        }
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
        BoundingBox { min: min_p, max: max_p }
    });

    (out, tracked)
}
