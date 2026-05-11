use crate::types::{BoundingBox, Mesh, Triangle, Vec3};
use rayon::prelude::*;
use super::mesh_ops::{merge_meshes, split_mesh_into_granules};

// AI-FUNC-SUMMARY: Compute the absolute volume of a closed mesh using the divergence theorem (sum of signed tetrahedra volumes); returns f64; side effects: None.
pub fn mesh_volume(mesh: &Mesh) -> f64 {
    let mut total = 0.0;
    for f in &mesh.faces {
        let a = mesh.vertices[f.a];
        let b = mesh.vertices[f.b];
        let c = mesh.vertices[f.c];
        total += a.dot(b.cross(c)) / 6.0;
    }
    total.abs()
}

// AI-FUNC-SUMMARY: Compute the signed volume of a closed mesh (negative for inward-facing normals); returns f64; side effects: None.
pub fn mesh_signed_volume(mesh: &Mesh) -> f64 {
    let mut total = 0.0;
    for f in &mesh.faces {
        let a = mesh.vertices[f.a];
        let b = mesh.vertices[f.b];
        let c = mesh.vertices[f.c];
        total += a.dot(b.cross(c)) / 6.0;
    }
    total
}

// AI-FUNC-SUMMARY:
// Purpose: Split mesh into components and flip face winding for any component with negative signed volume.
// Inputs: mesh reference.
// Returns: Tuple of (oriented mesh, count of flipped components, total component count).
// Side effects: None.
// Notes: Used to normalize mesh orientation before volume/collision computations.
pub fn orient_components_to_positive_volume(mesh: &Mesh) -> (Mesh, usize, usize) {
    let mut parts = split_mesh_into_granules(mesh);
    if parts.is_empty() {
        return (mesh.clone(), 0, 0);
    }

    let mut flipped = 0usize;
    for part in &mut parts {
        if mesh_signed_volume(part) < 0.0 {
            for face in &mut part.faces {
                std::mem::swap(&mut face.b, &mut face.c);
            }
            flipped += 1;
        }
    }

    (merge_meshes(&parts), flipped, parts.len())
}

// AI-FUNC-SUMMARY: Compute the signed distance from point p to a plane defined by origin and normal; returns f64; side effects: None.
fn clip_plane_signed_distance(p: Vec3, origin: Vec3, normal: Vec3) -> f64 {
    p.sub(origin).dot(normal)
}

// AI-FUNC-SUMMARY: Compute the intersection point of a line segment (a,b) with a clip plane using signed distances da, db; returns interpolated Vec3; side effects: None.
fn clip_segment_plane_intersection(a: Vec3, b: Vec3, da: f64, db: f64) -> Vec3 {
    let denom = da - db;
    if denom.abs() <= 1e-12 {
        return a;
    }
    let t = (da / denom).clamp(0.0, 1.0);
    a.add(b.sub(a).scale(t))
}

// AI-FUNC-SUMMARY:
// Purpose: Clip a convex polygon against a half-plane defined by origin and normal using Sutherland-Hodgman algorithm.
// Inputs: polygon vertices, plane origin, plane normal, epsilon for near-plane classification.
// Returns: Clipped polygon vertices (may be empty if fully clipped).
// Side effects: None.
// Notes: Deduplicates near-coincident output vertices.
fn clip_polygon_with_plane(poly: &[Vec3], origin: Vec3, normal: Vec3, eps: f64) -> Vec<Vec3> {
    if poly.is_empty() {
        return Vec::new();
    }

    let mut out = Vec::new();
    for i in 0..poly.len() {
        let c = poly[i];
        let n = poly[(i + 1) % poly.len()];
        let dc = clip_plane_signed_distance(c, origin, normal);
        let dn = clip_plane_signed_distance(n, origin, normal);
        let in_c = dc >= -eps;
        let in_n = dn >= -eps;

        match (in_c, in_n) {
            (true, true) => out.push(n),
            (true, false) => out.push(clip_segment_plane_intersection(c, n, dc, dn)),
            (false, true) => {
                out.push(clip_segment_plane_intersection(c, n, dc, dn));
                out.push(n);
            }
            (false, false) => {}
        }
    }

    if out.len() <= 2 {
        return out;
    }

    let mut dedup = Vec::with_capacity(out.len());
    for p in out {
        let keep = dedup
            .last()
            .map(|q: &Vec3| p.sub(*q).dot(p.sub(*q)) > 1e-20)
            .unwrap_or(true);
        if keep {
            dedup.push(p);
        }
    }
    if dedup.len() > 1 {
        let first = dedup[0];
        let last = *dedup.last().unwrap_or(&first);
        if first.sub(last).dot(first.sub(last)) <= 1e-20 {
            dedup.pop();
        }
    }
    dedup
}

// AI-FUNC-SUMMARY: Quantize a Vec3 point to a fixed-precision integer key for deduplication; returns (i64, i64, i64); side effects: None.
fn quantize_point_key(v: Vec3) -> (i64, i64, i64) {
    const SCALE: f64 = 1_000_000.0;
    (
        (v.x * SCALE).round() as i64,
        (v.y * SCALE).round() as i64,
        (v.z * SCALE).round() as i64,
    )
}

// AI-FUNC-SUMMARY:
// Purpose: Extract the line segment where a triangle intersects a clip plane.
// Inputs: triangle vertices, plane origin, plane normal, epsilon.
// Returns: Some((point_a, point_b)) segment on the plane, or None if no intersection.
// Side effects: None.
fn collect_triangle_plane_segment(
    tri: [Vec3; 3],
    origin: Vec3,
    normal: Vec3,
    eps: f64,
) -> Option<(Vec3, Vec3)> {
    let mut pts: Vec<Vec3> = Vec::new();
    for (i, j) in [(0usize, 1usize), (1, 2), (2, 0)] {
        let a = tri[i];
        let b = tri[j];
        let da = clip_plane_signed_distance(a, origin, normal);
        let db = clip_plane_signed_distance(b, origin, normal);

        if da.abs() <= eps {
            pts.push(a);
        }
        if (da > eps && db < -eps) || (da < -eps && db > eps) {
            pts.push(clip_segment_plane_intersection(a, b, da, db));
        }
    }

    let mut uniq: Vec<Vec3> = Vec::new();
    for p in pts {
        if !uniq.iter().any(|q| p.sub(*q).dot(p.sub(*q)) <= 1e-16) {
            uniq.push(p);
        }
    }

    if uniq.len() >= 2 {
        Some((uniq[0], uniq[1]))
    } else {
        None
    }
}

// AI-FUNC-SUMMARY: Compute an orthonormal basis (u, v) in the plane perpendicular to the given normal; returns (Vec3, Vec3); side effects: None.
fn plane_basis(normal: Vec3) -> (Vec3, Vec3) {
    let tangent = if normal.x.abs() < 0.5 {
        Vec3::new(1.0, 0.0, 0.0)
    } else {
        Vec3::new(0.0, 1.0, 0.0)
    };
    let u_raw = normal.cross(tangent);
    let un = (u_raw.dot(u_raw)).sqrt();
    let u = if un > 1e-12 {
        u_raw.scale(1.0 / un)
    } else {
        Vec3::new(0.0, 0.0, 1.0)
    };
    let v_raw = normal.cross(u);
    let vn = (v_raw.dot(v_raw)).sqrt();
    let v = if vn > 1e-12 {
        v_raw.scale(1.0 / vn)
    } else {
        Vec3::new(1.0, 0.0, 0.0)
    };
    (u, v)
}

// AI-FUNC-SUMMARY:
// Purpose: Triangulate a planar cap from a set of edge segments using ring-finding and fan triangulation.
// Inputs: edge segments and the plane normal for orientation.
// Returns: Mesh representing the cap surface.
// Side effects: None.
// Notes: Uses quantized point deduplication and angular sorting around the centroid.
fn triangulate_cap_from_segments(segments: &[(Vec3, Vec3)], normal: Vec3) -> Mesh {
    if segments.is_empty() {
        return Mesh::empty();
    }

    let mut points: Vec<Vec3> = Vec::new();
    let mut point_map: std::collections::HashMap<(i64, i64, i64), usize> = std::collections::HashMap::new();
    let mut adjacency: std::collections::HashMap<usize, Vec<usize>> = std::collections::HashMap::new();
    let mut edges: std::collections::HashSet<(usize, usize)> = std::collections::HashSet::new();

    let add_point = |p: Vec3,
                     points: &mut Vec<Vec3>,
                     point_map: &mut std::collections::HashMap<(i64, i64, i64), usize>| {
        let k = quantize_point_key(p);
        if let Some(&idx) = point_map.get(&k) {
            idx
        } else {
            let idx = points.len();
            points.push(p);
            point_map.insert(k, idx);
            idx
        }
    };

    for (a, b) in segments {
        let ia = add_point(*a, &mut points, &mut point_map);
        let ib = add_point(*b, &mut points, &mut point_map);
        if ia == ib {
            continue;
        }
        let e = if ia < ib { (ia, ib) } else { (ib, ia) };
        if edges.insert(e) {
            adjacency.entry(ia).or_default().push(ib);
            adjacency.entry(ib).or_default().push(ia);
        }
    }

    let mut used: std::collections::HashSet<(usize, usize)> = std::collections::HashSet::new();
    let (u_axis, v_axis) = plane_basis(normal);
    let mut cap = Mesh::empty();

    for (a, b) in edges {
        if used.contains(&(a, b)) {
            continue;
        }

        let mut ring = vec![a, b];
        used.insert((a, b));
        let mut prev = a;
        let mut curr = b;
        loop {
            if curr == a {
                break;
            }
            let Some(neigh) = adjacency.get(&curr) else {
                break;
            };
            let mut next_opt = None;
            for &n in neigh {
                if n == prev {
                    continue;
                }
                let e = if curr < n { (curr, n) } else { (n, curr) };
                if !used.contains(&e) {
                    next_opt = Some(n);
                    break;
                }
            }
            let Some(next) = next_opt else {
                break;
            };
            let e = if curr < next { (curr, next) } else { (next, curr) };
            used.insert(e);
            ring.push(next);
            prev = curr;
            curr = next;
        }

        if ring.len() < 4 || *ring.last().unwrap_or(&usize::MAX) != a {
            continue;
        }
        ring.pop();
        if ring.len() < 3 {
            continue;
        }

        let mut center = Vec3::new(0.0, 0.0, 0.0);
        for &i in &ring {
            center = center.add(points[i]);
        }
        center = center.scale(1.0 / ring.len() as f64);

        let mut ordered = ring.clone();
        ordered.sort_by(|&i, &j| {
            let pi = points[i].sub(center);
            let pj = points[j].sub(center);
            let ai = pi.dot(v_axis).atan2(pi.dot(u_axis));
            let aj = pj.dot(v_axis).atan2(pj.dot(u_axis));
            ai.partial_cmp(&aj).unwrap_or(std::cmp::Ordering::Equal)
        });

        let center_idx = cap.vertices.len();
        cap.vertices.push(center);
        let mut ring_idx = Vec::with_capacity(ordered.len());
        for &pid in &ordered {
            ring_idx.push(cap.vertices.len());
            cap.vertices.push(points[pid]);
        }

        let mut area_sign = 0.0;
        for i in 0..ordered.len() {
            let p = points[ordered[i]].sub(center);
            let q = points[ordered[(i + 1) % ordered.len()]].sub(center);
            area_sign += p.cross(q).dot(normal);
        }
        let ccw = area_sign >= 0.0;

        for i in 0..ring_idx.len() {
            let a = ring_idx[i];
            let b = ring_idx[(i + 1) % ring_idx.len()];
            if ccw {
                cap.faces.push(Triangle { a: center_idx, b, c: a });
            } else {
                cap.faces.push(Triangle {
                    a: center_idx,
                    b: a,
                    c: b,
                });
            }
        }
    }

    cap
}

// AI-FUNC-SUMMARY:
// Purpose: Clip a mesh against a single plane and cap the resulting open boundary with a triangulated surface.
// Inputs: mesh, plane origin, plane normal.
// Returns: Clipped and capped mesh.
// Side effects: None.
// Notes: Uses Sutherland-Hodgman polygon clipping per triangle, then collects cross-plane segments to form a watertight cap.
fn clip_mesh_by_plane_with_cap(mesh: &Mesh, origin: Vec3, normal: Vec3) -> Mesh {
    let eps = 1e-9;
    let mut body = Mesh::empty();
    let mut segments: Vec<(Vec3, Vec3)> = Vec::new();

    for f in &mesh.faces {
        let tri = [mesh.vertices[f.a], mesh.vertices[f.b], mesh.vertices[f.c]];
        let clipped_poly = clip_polygon_with_plane(&tri, origin, normal, eps);
        if clipped_poly.len() >= 3 {
            let base = body.vertices.len();
            body.vertices.extend(clipped_poly.iter().copied());
            for i in 1..(clipped_poly.len() - 1) {
                body.faces.push(Triangle {
                    a: base,
                    b: base + i,
                    c: base + i + 1,
                });
            }
        }

        if let Some(seg) = collect_triangle_plane_segment(tri, origin, normal, eps) {
            segments.push(seg);
        }
    }

    if segments.is_empty() {
        body
    } else {
        merge_meshes(&[body, triangulate_cap_from_segments(&segments, normal)])
    }
}

// AI-FUNC-SUMMARY:
// Purpose: Clip a mesh to fit within an axis-aligned bounding box by successively clipping against all 6 face planes.
// Inputs: mesh and bounding box.
// Returns: Clipped mesh (may be empty if entirely outside).
// Side effects: None.
// Notes: Clipping is done one plane at a time, capping each open boundary to maintain watertightness.
pub fn clip_mesh_by_bbox(mesh: &Mesh, bbox: BoundingBox) -> Mesh {
    let planes = [
        (Vec3::new(bbox.min.x, 0.0, 0.0), Vec3::new(1.0, 0.0, 0.0)),
        (Vec3::new(bbox.max.x, 0.0, 0.0), Vec3::new(-1.0, 0.0, 0.0)),
        (Vec3::new(0.0, bbox.min.y, 0.0), Vec3::new(0.0, 1.0, 0.0)),
        (Vec3::new(0.0, bbox.max.y, 0.0), Vec3::new(0.0, -1.0, 0.0)),
        (Vec3::new(0.0, 0.0, bbox.min.z), Vec3::new(0.0, 0.0, 1.0)),
        (Vec3::new(0.0, 0.0, bbox.max.z), Vec3::new(0.0, 0.0, -1.0)),
    ];

    let mut out = mesh.clone();
    for (origin, normal) in planes {
        out = clip_mesh_by_plane_with_cap(&out, origin, normal);
        if out.is_empty() {
            break;
        }
    }
    out
}

// AI-FUNC-SUMMARY: Compute the volume of a mesh clipped to a bounding box; returns f64; side effects: None.
pub fn particle_volume_in_bbox(mesh: &Mesh, bbox: BoundingBox) -> f64 {
    let clipped = clip_mesh_by_bbox(mesh, bbox);
    mesh_volume(&clipped)
}

// AI-FUNC-SUMMARY: Compute the volume fraction of a single mesh within a bounding box; returns f64 in [0,1]; side effects: None.
pub fn volume_fraction_in_bbox(mesh: &Mesh, bbox: BoundingBox) -> f64 {
    volume_fraction_of_meshes_in_bbox(std::slice::from_ref(mesh), bbox)
}

// AI-FUNC-SUMMARY:
// Purpose: Compute the total volume fraction of multiple meshes within a bounding box, using parallel iteration.
// Inputs: slice of meshes and bounding box.
// Returns: Volume fraction in [0,1] clamped.
// Side effects: None.
// Notes: Splits each mesh into granules before clipping for correct volume computation.
pub fn volume_fraction_of_meshes_in_bbox(meshes: &[Mesh], bbox: BoundingBox) -> f64 {
    let box_volume = bbox.volume().max(1e-12);

    let in_box_volume: f64 = meshes
        .par_iter()
        .map(|mesh| {
            let parts = split_mesh_into_granules(mesh);
            if parts.is_empty() {
                particle_volume_in_bbox(mesh, bbox)
            } else {
                parts.iter().map(|p| particle_volume_in_bbox(p, bbox)).sum::<f64>()
            }
        })
        .sum();

    (in_box_volume / box_volume).clamp(0.0, 1.0)
}
