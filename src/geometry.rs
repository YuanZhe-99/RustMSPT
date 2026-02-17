use crate::types::{BoundingBox, Mesh, Vec3};
use rand::Rng;
use rayon::prelude::*;
use rustfft::num_complex::Complex;
use rustfft::FftPlanner;
use std::collections::{HashMap, VecDeque};

pub fn mesh_bbox(mesh: &Mesh) -> Option<BoundingBox> {
    // Purpose: Compute axis-aligned bounding box of a mesh.
    // Inputs: mesh vertices/faces.
    // Outputs: bounding box or None for empty vertex list.
    if mesh.vertices.is_empty() {
        return None;
    }

    let mut min = mesh.vertices[0];
    let mut max = mesh.vertices[0];
    for v in &mesh.vertices {
        min.x = min.x.min(v.x);
        min.y = min.y.min(v.y);
        min.z = min.z.min(v.z);
        max.x = max.x.max(v.x);
        max.y = max.y.max(v.y);
        max.z = max.z.max(v.z);
    }

    Some(BoundingBox { min, max })
}

pub fn mesh_centroid(mesh: &Mesh) -> Vec3 {
    // Purpose: Compute arithmetic centroid of mesh vertices.
    // Inputs: mesh vertices.
    // Outputs: centroid point.
    let mut sum = Vec3::new(0.0, 0.0, 0.0);
    let count = mesh.vertices.len() as f64;
    for v in &mesh.vertices {
        sum = sum.add(*v);
    }
    if count > 0.0 {
        sum.scale(1.0 / count)
    } else {
        sum
    }
}

pub fn mesh_volume(mesh: &Mesh) -> f64 {
    // Purpose: Compute absolute mesh volume from triangle tetrahedralization.
    // Inputs: triangle mesh.
    // Outputs: non-negative volume.
    let mut total = 0.0;
    for f in &mesh.faces {
        let a = mesh.vertices[f.a];
        let b = mesh.vertices[f.b];
        let c = mesh.vertices[f.c];
        total += a.dot(b.cross(c)) / 6.0;
    }
    total.abs()
}

/// Compute signed volume from triangle winding.
/// Input: mesh with triangle faces. Output: signed volume (negative means inverted orientation).
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

/// Enforce positive orientation for each connected component independently.
/// Input: mesh. Output: (reoriented mesh, flipped component count, total component count).
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

pub fn merge_meshes(meshes: &[Mesh]) -> Mesh {
    // Purpose: Merge multiple meshes into one mesh with remapped indices.
    // Inputs: mesh slice.
    // Outputs: merged mesh.
    let mut out = Mesh::empty();
    for m in meshes {
        let offset = out.vertices.len();
        out.vertices.extend(m.vertices.iter().copied());
        out.faces.extend(m.faces.iter().map(|f| crate::types::Triangle {
            a: f.a + offset,
            b: f.b + offset,
            c: f.c + offset,
        }));
    }
    out
}

pub fn split_mesh_into_granules(mesh: &Mesh) -> Vec<Mesh> {
    // Purpose: Split mesh into connected components by shared vertices.
    // Inputs: source mesh.
    // Outputs: vector of component meshes.
    if mesh.faces.is_empty() || mesh.vertices.is_empty() {
        return Vec::new();
    }

    let mut vertex_to_faces: Vec<Vec<usize>> = vec![Vec::new(); mesh.vertices.len()];
    for (face_index, face) in mesh.faces.iter().enumerate() {
        vertex_to_faces[face.a].push(face_index);
        vertex_to_faces[face.b].push(face_index);
        vertex_to_faces[face.c].push(face_index);
    }

    let mut visited = vec![false; mesh.faces.len()];
    let mut parts: Vec<Mesh> = Vec::new();

    for start_face in 0..mesh.faces.len() {
        if visited[start_face] {
            continue;
        }

        let mut queue: VecDeque<usize> = VecDeque::new();
        let mut component_faces: Vec<usize> = Vec::new();
        visited[start_face] = true;
        queue.push_back(start_face);

        while let Some(current) = queue.pop_front() {
            component_faces.push(current);
            let f = &mesh.faces[current];
            for &vertex_index in &[f.a, f.b, f.c] {
                for &adjacent_face in &vertex_to_faces[vertex_index] {
                    if !visited[adjacent_face] {
                        visited[adjacent_face] = true;
                        queue.push_back(adjacent_face);
                    }
                }
            }
        }

        let mut local_vertices: Vec<Vec3> = Vec::new();
        let mut local_faces = Vec::new();
        let mut remap: HashMap<usize, usize> = HashMap::new();

        for face_index in component_faces {
            let f = &mesh.faces[face_index];
            let map_vertex = |global: usize,
                              local_vertices: &mut Vec<Vec3>,
                              remap: &mut HashMap<usize, usize>| {
                if let Some(&idx) = remap.get(&global) {
                    idx
                } else {
                    let idx = local_vertices.len();
                    local_vertices.push(mesh.vertices[global]);
                    remap.insert(global, idx);
                    idx
                }
            };

            let a = map_vertex(f.a, &mut local_vertices, &mut remap);
            let b = map_vertex(f.b, &mut local_vertices, &mut remap);
            let c = map_vertex(f.c, &mut local_vertices, &mut remap);
            local_faces.push(crate::types::Triangle { a, b, c });
        }

        parts.push(Mesh {
            vertices: local_vertices,
            faces: local_faces,
        });
    }

    parts
}

pub fn translate_mesh(mesh: &mut Mesh, delta: Vec3) {
    // Purpose: Translate every vertex by a delta vector.
    // Inputs: mutable mesh and translation delta.
    // Outputs: mesh updated in place.
    for v in &mut mesh.vertices {
        *v = v.add(delta);
    }
}

pub fn move_mesh_to_target_center(mesh: &mut Mesh, target: Vec3) {
    // Purpose: Move mesh so centroid matches target point.
    // Inputs: mutable mesh and target center.
    // Outputs: mesh translated in place.
    let center = mesh_centroid(mesh);
    translate_mesh(mesh, target.sub(center));
}

pub fn scale_mesh(mesh: &mut Mesh, factor: f64) {
    // Purpose: Uniformly scale mesh vertices around origin.
    // Inputs: mutable mesh and scalar factor.
    // Outputs: scaled mesh in place.
    for v in &mut mesh.vertices {
        *v = v.scale(factor);
    }
}

fn clip_plane_signed_distance(p: Vec3, origin: Vec3, normal: Vec3) -> f64 {
    // Purpose: Evaluate signed distance from point to plane.
    // Inputs: point, plane origin, plane normal.
    // Outputs: signed distance value.
    p.sub(origin).dot(normal)
}

fn clip_segment_plane_intersection(a: Vec3, b: Vec3, da: f64, db: f64) -> Vec3 {
    // Purpose: Compute segment-plane intersection using endpoint signed distances.
    // Inputs: segment endpoints and their plane distances.
    // Outputs: intersection point on segment.
    let denom = da - db;
    if denom.abs() <= 1e-12 {
        return a;
    }
    let t = (da / denom).clamp(0.0, 1.0);
    a.add(b.sub(a).scale(t))
}

fn clip_polygon_with_plane(poly: &[Vec3], origin: Vec3, normal: Vec3, eps: f64) -> Vec<Vec3> {
    // Purpose: Clip polygon against half-space defined by a plane.
    // Inputs: polygon vertices, plane origin/normal, tolerance.
    // Outputs: clipped polygon vertices.
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

fn quantize_point_key(v: Vec3) -> (i64, i64, i64) {
    // Purpose: Quantize point coordinates for tolerant hashing.
    // Inputs: 3D point.
    // Outputs: integer key tuple.
    const SCALE: f64 = 1_000_000.0;
    (
        (v.x * SCALE).round() as i64,
        (v.y * SCALE).round() as i64,
        (v.z * SCALE).round() as i64,
    )
}

fn collect_triangle_plane_segment(
    tri: [Vec3; 3],
    origin: Vec3,
    normal: Vec3,
    eps: f64,
) -> Option<(Vec3, Vec3)> {
    // Purpose: Extract triangle-plane intersection segment when available.
    // Inputs: triangle vertices, plane origin/normal, tolerance.
    // Outputs: optional segment endpoints.
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

fn plane_basis(normal: Vec3) -> (Vec3, Vec3) {
    // Purpose: Build orthonormal tangent basis on a plane.
    // Inputs: plane normal.
    // Outputs: two tangent unit vectors.
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

fn triangulate_cap_from_segments(segments: &[(Vec3, Vec3)], normal: Vec3) -> Mesh {
    // Purpose: Triangulate clipping cap from plane intersection segments.
    // Inputs: unordered boundary segments and cap normal.
    // Outputs: cap mesh.
    if segments.is_empty() {
        return Mesh::empty();
    }

    let mut points: Vec<Vec3> = Vec::new();
    let mut point_map: HashMap<(i64, i64, i64), usize> = HashMap::new();
    let mut adjacency: HashMap<usize, Vec<usize>> = HashMap::new();
    let mut edges: std::collections::HashSet<(usize, usize)> = std::collections::HashSet::new();

    let add_point = |p: Vec3,
                     points: &mut Vec<Vec3>,
                     point_map: &mut HashMap<(i64, i64, i64), usize>| {
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
                cap.faces.push(crate::types::Triangle { a: center_idx, b, c: a });
            } else {
                cap.faces.push(crate::types::Triangle {
                    a: center_idx,
                    b: a,
                    c: b,
                });
            }
        }
    }

    cap
}

fn clip_mesh_by_plane_with_cap(mesh: &Mesh, origin: Vec3, normal: Vec3) -> Mesh {
    // Purpose: Clip mesh by plane and close open boundary with cap triangles.
    // Inputs: mesh, plane origin, plane normal.
    // Outputs: clipped watertight mesh approximation.
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
                body.faces.push(crate::types::Triangle {
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

fn clip_mesh_by_bbox_precise(mesh: &Mesh, bbox: BoundingBox) -> Mesh {
    // Purpose: Clip mesh by all six bbox planes with capping.
    // Inputs: source mesh and target bounding box.
    // Outputs: clipped mesh.
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

pub fn clip_mesh_by_bbox(mesh: &Mesh, bbox: BoundingBox) -> Mesh {
    // Purpose: Public bbox clipping entry point.
    // Inputs: source mesh and target bounding box.
    // Outputs: clipped mesh.
    clip_mesh_by_bbox_precise(mesh, bbox)
}

/// Compute volume of one particle within bbox by clipping then measuring.
/// Inputs: particle mesh and bbox.
/// Outputs: positive in-box volume.
pub fn particle_volume_in_bbox(mesh: &Mesh, bbox: BoundingBox) -> f64 {
    let clipped = clip_mesh_by_bbox(mesh, bbox);
    mesh_volume(&clipped)
}

pub fn box_mesh(bbox: BoundingBox) -> Mesh {
    // Purpose: Build triangulated box mesh from bounding box corners.
    // Inputs: axis-aligned bounding box.
    // Outputs: closed box mesh.
    let min = bbox.min;
    let max = bbox.max;
    let v = vec![
        Vec3::new(min.x, min.y, min.z),
        Vec3::new(max.x, min.y, min.z),
        Vec3::new(max.x, max.y, min.z),
        Vec3::new(min.x, max.y, min.z),
        Vec3::new(min.x, min.y, max.z),
        Vec3::new(max.x, min.y, max.z),
        Vec3::new(max.x, max.y, max.z),
        Vec3::new(min.x, max.y, max.z),
    ];

    let idx = [
        (0, 1, 2), (0, 2, 3),
        (4, 6, 5), (4, 7, 6),
        (0, 4, 5), (0, 5, 1),
        (1, 5, 6), (1, 6, 2),
        (2, 6, 7), (2, 7, 3),
        (3, 7, 4), (3, 4, 0),
    ];

    let faces = idx
        .iter()
        .map(|(a, b, c)| crate::types::Triangle {
            a: *a,
            b: *b,
            c: *c,
        })
        .collect();

    Mesh { vertices: v, faces }
}

pub fn simulate_forging_ffd(mesh: &Mesh, compression_ratio: f64, bulge_factor: f64) -> Mesh {
    // Purpose: Apply simplified forging deformation to mesh.
    // Inputs: source mesh, compression ratio, bulge factor.
    // Outputs: deformed mesh.
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
    let z_scale = (1.0 - compression_ratio).clamp(0.01, 1.0);
    let xy_scale = (1.0 / z_scale.sqrt()).powf(bulge_factor.clamp(0.0, 1.0));

    for v in &mut out.vertices {
        let local = v.sub(center);
        *v = Vec3::new(
            center.x + local.x * xy_scale,
            center.y + local.y * xy_scale,
            center.z + local.z * z_scale,
        );
    }

    out
}

/// Apply simplified forging affine transform with optional ROI tracking.
/// Inputs: source mesh, lattice bbox (deformation frame), optional ROI bbox, compression, bulge, mesh type, void densification factor.
/// Outputs: deformed mesh and optional transformed ROI bbox.
pub fn simulate_forging_ffd_with_tracking(
    mesh: &Mesh,
    lattice_bbox: BoundingBox,
    track_bbox: Option<BoundingBox>,
    compression_ratio: f64,
    bulge_factor: f64,
    mesh_type: &str,
    void_densification: f64,
) -> (Mesh, Option<BoundingBox>) {
    let center = Vec3::new(
        (lattice_bbox.min.x + lattice_bbox.max.x) * 0.5,
        (lattice_bbox.min.y + lattice_bbox.max.y) * 0.5,
        (lattice_bbox.min.z + lattice_bbox.max.z) * 0.5,
    );

    let z_scale = (1.0 - compression_ratio).clamp(0.01, 1.0);
    let xy_scale = (1.0 / z_scale.sqrt()).powf(bulge_factor.clamp(0.0, 1.0));

    let transform_point = |p: Vec3| {
        let local = p.sub(center);
        Vec3::new(
            center.x + local.x * xy_scale,
            center.y + local.y * xy_scale,
            center.z + local.z * z_scale,
        )
    };

    let mut out = mesh.clone();
    for v in &mut out.vertices {
        *v = transform_point(*v);
    }

    if mesh_type.eq_ignore_ascii_case("void") {
        let closure = (1.0 - 0.05 * compression_ratio * void_densification).clamp(0.85, 1.0);
        let c = mesh_centroid(&out);
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

fn index_3d_to_flat(x: usize, y: usize, z: usize, ny: usize, nz: usize) -> usize {
    // Purpose: Convert 3D voxel index to flat array index.
    // Inputs: x/y/z index and grid strides.
    // Outputs: flat index.
    x * ny * nz + y * nz + z
}

fn ray_intersects_triangle(origin: Vec3, dir: Vec3, a: Vec3, b: Vec3, c: Vec3) -> Option<f64> {
    // Purpose: Ray-triangle intersection using Moller-Trumbore method.
    // Inputs: ray origin/direction and triangle vertices.
    // Outputs: hit distance t when intersecting.
    let eps = 1e-10;
    let edge1 = b.sub(a);
    let edge2 = c.sub(a);
    let h = dir.cross(edge2);
    let det = edge1.dot(h);
    if det.abs() <= eps {
        return None;
    }

    let inv_det = 1.0 / det;
    let s = origin.sub(a);
    let u = inv_det * s.dot(h);
    if !(0.0 - eps..=1.0 + eps).contains(&u) {
        return None;
    }

    let q = s.cross(edge1);
    let v = inv_det * dir.dot(q);
    if v < -eps || (u + v) > 1.0 + eps {
        return None;
    }

    let t = inv_det * edge2.dot(q);
    if t > eps {
        Some(t)
    } else {
        None
    }
}

fn point_inside_mesh(mesh: &Mesh, point: Vec3) -> bool {
    // Purpose: Test point inclusion in mesh by ray casting.
    // Inputs: mesh and query point.
    // Outputs: true when point is inside.
    let Some(bb) = mesh_bbox(mesh) else {
        return false;
    };
    let eps = 1e-9;
    if point.x < bb.min.x - eps
        || point.y < bb.min.y - eps
        || point.z < bb.min.z - eps
        || point.x > bb.max.x + eps
        || point.y > bb.max.y + eps
        || point.z > bb.max.z + eps
    {
        return false;
    }

    let dir = Vec3::new(0.9428090415820634, 0.2705980500730985, 0.19611613513818402);
    let mut ts: Vec<f64> = Vec::new();
    for f in &mesh.faces {
        let a = mesh.vertices[f.a];
        let b = mesh.vertices[f.b];
        let c = mesh.vertices[f.c];
        if let Some(t) = ray_intersects_triangle(point, dir, a, b, c) {
            ts.push(t);
        }
    }

    if ts.is_empty() {
        return false;
    }

    ts.sort_by(|x, y| x.partial_cmp(y).unwrap_or(std::cmp::Ordering::Equal));
    let mut unique_hits = 0usize;
    let mut last_t = f64::NEG_INFINITY;
    for t in ts {
        if (t - last_t).abs() > 1e-8 {
            unique_hits += 1;
            last_t = t;
        }
    }

    unique_hits % 2 == 1
}

fn build_bbox_occupancy(mesh: &Mesh, bbox: BoundingBox, voxel_pitch: f64) -> (Vec<bool>, [usize; 3]) {
    // Purpose: Build voxel occupancy grid within bbox.
    // Inputs: mesh, bbox, voxel pitch.
    // Outputs: occupancy flags and grid dimensions.
    let size = bbox.size();
    let nx = ((size.x / voxel_pitch).ceil() as usize).max(1);
    let ny = ((size.y / voxel_pitch).ceil() as usize).max(1);
    let nz = ((size.z / voxel_pitch).ceil() as usize).max(1);
    let mut occ = vec![false; nx * ny * nz];

    let parts = split_mesh_into_granules(mesh);
    for p in &parts {
        let Some(pb) = mesh_bbox(p) else {
            continue;
        };

        let x0 = (((pb.min.x - bbox.min.x) / voxel_pitch).floor() as isize).max(0) as usize;
        let y0 = (((pb.min.y - bbox.min.y) / voxel_pitch).floor() as isize).max(0) as usize;
        let z0 = (((pb.min.z - bbox.min.z) / voxel_pitch).floor() as isize).max(0) as usize;

        let x1 = (((pb.max.x - bbox.min.x) / voxel_pitch).ceil() as isize).min(nx as isize) as usize;
        let y1 = (((pb.max.y - bbox.min.y) / voxel_pitch).ceil() as isize).min(ny as isize) as usize;
        let z1 = (((pb.max.z - bbox.min.z) / voxel_pitch).ceil() as isize).min(nz as isize) as usize;

        if x0 >= x1 || y0 >= y1 || z0 >= z1 {
            continue;
        }

        for x in x0..x1 {
            for y in y0..y1 {
                for z in z0..z1 {
                    let center = Vec3::new(
                        bbox.min.x + (x as f64 + 0.5) * voxel_pitch,
                        bbox.min.y + (y as f64 + 0.5) * voxel_pitch,
                        bbox.min.z + (z as f64 + 0.5) * voxel_pitch,
                    );
                    if point_inside_mesh(p, center) {
                        let idx = index_3d_to_flat(x, y, z, ny, nz);
                        occ[idx] = true;
                    }
                }
            }
        }
    }

    (occ, [nx, ny, nz])
}

fn shell_offsets_for_r(r: usize) -> Vec<[isize; 3]> {
    // Purpose: Enumerate integer offsets near shell radius r.
    // Inputs: shell radius index.
    // Outputs: voxel offset vectors.
    if r == 0 {
        return vec![[0, 0, 0]];
    }

    let rr = r as f64;
    let low2 = (rr - 0.5).max(0.0).powi(2);
    let high2 = (rr + 0.5).powi(2);
    let lim = (rr + 1.5).ceil() as isize;
    let mut offsets = Vec::new();

    for dx in -lim..=lim {
        for dy in -lim..=lim {
            for dz in -lim..=lim {
                if dx == 0 && dy == 0 && dz == 0 {
                    continue;
                }
                let d2 = (dx * dx + dy * dy + dz * dz) as f64;
                if d2 >= low2 && d2 < high2 {
                    offsets.push([dx, dy, dz]);
                }
            }
        }
    }

    if offsets.is_empty() {
        offsets.push([r as isize, 0, 0]);
    }
    offsets
}

fn fft_index_3d(x: usize, y: usize, z: usize, ny: usize, nz: usize) -> usize {
    // Purpose: Convert 3D FFT-grid index to flat index.
    // Inputs: x/y/z and y/z strides.
    // Outputs: flat index.
    x * ny * nz + y * nz + z
}

fn fft_3d_in_place(data: &mut [Complex<f64>], nx: usize, ny: usize, nz: usize, inverse: bool) {
    // Purpose: Perform separable 3D FFT/IFFT in place.
    // Inputs: complex data buffer, dimensions, inverse flag.
    // Outputs: transformed data buffer.
    let mut planner = FftPlanner::<f64>::new();
    let fft_z = if inverse {
        planner.plan_fft_inverse(nz)
    } else {
        planner.plan_fft_forward(nz)
    };
    data.par_chunks_mut(nz).for_each(|line| {
        fft_z.process(line);
    });

    let fft_y = if inverse {
        planner.plan_fft_inverse(ny)
    } else {
        planner.plan_fft_forward(ny)
    };
    data.par_chunks_mut(ny * nz).for_each(|x_slab| {
        let mut tmp_y = vec![Complex::<f64>::new(0.0, 0.0); ny];
        for z in 0..nz {
            for y in 0..ny {
                tmp_y[y] = x_slab[y * nz + z];
            }
            fft_y.process(&mut tmp_y);
            for y in 0..ny {
                x_slab[y * nz + z] = tmp_y[y];
            }
        }
    });

    let fft_x = if inverse {
        planner.plan_fft_inverse(nx)
    } else {
        planner.plan_fft_forward(nx)
    };
    let mut tmp_x = vec![Complex::<f64>::new(0.0, 0.0); nx];
    for y in 0..ny {
        for z in 0..nz {
            for x in 0..nx {
                tmp_x[x] = data[fft_index_3d(x, y, z, ny, nz)];
            }
            fft_x.process(&mut tmp_x);
            for x in 0..nx {
                data[fft_index_3d(x, y, z, ny, nz)] = tmp_x[x];
            }
        }
    }

    if inverse {
        let norm = (nx * ny * nz) as f64;
        for v in data.iter_mut() {
            *v /= norm;
        }
    }
}

fn autocorrelation_counts_fft(occ: &[bool], nx: usize, ny: usize, nz: usize) -> (Vec<f64>, [usize; 3]) {
    // Purpose: Compute occupancy autocorrelation counts via FFT convolution.
    // Inputs: occupancy grid and dimensions.
    // Outputs: correlation grid and padded FFT dimensions.
    let fx = (2 * nx).saturating_sub(1).max(1);
    let fy = (2 * ny).saturating_sub(1).max(1);
    let fz = (2 * nz).saturating_sub(1).max(1);
    let mut grid = vec![Complex::<f64>::new(0.0, 0.0); fx * fy * fz];

    for x in 0..nx {
        for y in 0..ny {
            for z in 0..nz {
                if occ[index_3d_to_flat(x, y, z, ny, nz)] {
                    grid[fft_index_3d(x, y, z, fy, fz)] = Complex::new(1.0, 0.0);
                }
            }
        }
    }

    fft_3d_in_place(&mut grid, fx, fy, fz, false);
    for v in grid.iter_mut() {
        *v *= v.conj();
    }
    fft_3d_in_place(&mut grid, fx, fy, fz, true);

    let corr = grid.iter().map(|v| v.re.max(0.0)).collect::<Vec<_>>();
    (corr, [fx, fy, fz])
}

fn calculate_s2_exact_direct(occ: &[bool], nx: usize, ny: usize, nz: usize, r_max: usize, vf: f64) -> Vec<f64> {
    // Purpose: Compute exact S2 by direct offset pair enumeration.
    // Inputs: occupancy grid, dimensions, max radius, VF at r=0.
    // Outputs: S2 values for r=0..r_max.
    let mut out: Vec<f64> = (0..=r_max)
        .into_par_iter()
        .map(|r| {
            if r == 0 {
                return vf;
            }

            let offsets = shell_offsets_for_r(r);
            let mut shell_sum = 0.0;

            for off in &offsets {
                let dx = off[0];
                let dy = off[1];
                let dz = off[2];

                let x_start = if dx < 0 { (-dx) as usize } else { 0 };
                let y_start = if dy < 0 { (-dy) as usize } else { 0 };
                let z_start = if dz < 0 { (-dz) as usize } else { 0 };

                let x_end = if dx > 0 { nx.saturating_sub(dx as usize) } else { nx };
                let y_end = if dy > 0 { ny.saturating_sub(dy as usize) } else { ny };
                let z_end = if dz > 0 { nz.saturating_sub(dz as usize) } else { nz };

                if x_start >= x_end || y_start >= y_end || z_start >= z_end {
                    continue;
                }

                let mut valid_pairs = 0usize;
                let mut hit_pairs = 0usize;

                for x in x_start..x_end {
                    for y in y_start..y_end {
                        for z in z_start..z_end {
                            let x2 = (x as isize + dx) as usize;
                            let y2 = (y as isize + dy) as usize;
                            let z2 = (z as isize + dz) as usize;

                            let i1 = index_3d_to_flat(x, y, z, ny, nz);
                            let i2 = index_3d_to_flat(x2, y2, z2, ny, nz);
                            valid_pairs += 1;
                            if occ[i1] && occ[i2] {
                                hit_pairs += 1;
                            }
                        }
                    }
                }

                if valid_pairs > 0 {
                    shell_sum += hit_pairs as f64 / valid_pairs as f64;
                }
            }

            if offsets.is_empty() {
                0.0
            } else {
                shell_sum / offsets.len() as f64
            }
        })
        .collect();
    out[0] = vf;
    out
}

fn calculate_s2_exact_fft(occ: &[bool], nx: usize, ny: usize, nz: usize, r_max: usize, vf: f64) -> Vec<f64> {
    // Purpose: Compute exact S2 using FFT-based autocorrelation.
    // Inputs: occupancy grid, dimensions, max radius, VF at r=0.
    // Outputs: S2 values for r=0..r_max.
    let (corr, fdims) = autocorrelation_counts_fft(occ, nx, ny, nz);
    let [fx, fy, fz] = fdims;

    let get_corr = |dx: isize, dy: isize, dz: isize| {
        let ix = if dx >= 0 { dx as usize } else { (fx as isize + dx) as usize };
        let iy = if dy >= 0 { dy as usize } else { (fy as isize + dy) as usize };
        let iz = if dz >= 0 { dz as usize } else { (fz as isize + dz) as usize };
        corr[fft_index_3d(ix, iy, iz, fy, fz)]
    };

    let mut out: Vec<f64> = (0..=r_max)
        .into_par_iter()
        .map(|r| {
            if r == 0 {
                return vf;
            }

            let offsets = shell_offsets_for_r(r);
            let mut shell_sum = 0.0;
            let mut used = 0usize;

            for off in &offsets {
                let dx = off[0];
                let dy = off[1];
                let dz = off[2];

                let adx = dx.unsigned_abs();
                let ady = dy.unsigned_abs();
                let adz = dz.unsigned_abs();
                if adx >= nx || ady >= ny || adz >= nz {
                    continue;
                }

                let valid_pairs = (nx - adx) * (ny - ady) * (nz - adz);
                if valid_pairs == 0 {
                    continue;
                }

                let hits = get_corr(dx, dy, dz);
                shell_sum += hits / valid_pairs as f64;
                used += 1;
            }

            if used == 0 {
                0.0
            } else {
                shell_sum / used as f64
            }
        })
        .collect();
    out[0] = vf;
    out
}

pub fn calculate_s2(
    mesh: &Mesh,
    bbox: BoundingBox,
    r_max: usize,
    voxel_pitch: f64,
    method: &str,
    samples: usize,
) -> Vec<f64> {
    // Purpose: Compute S2 by configured method (exact or monte_carlo).
    // Inputs: mesh, bbox, r_max, voxel pitch, method name, sample count.
    // Outputs: S2 values indexed by radius.
    let (occ, dims) = build_bbox_occupancy(mesh, bbox, voxel_pitch.max(1e-9));
    let [nx, ny, nz] = dims;
    let total_vox = (nx * ny * nz).max(1);
    let occupied_count = occ.iter().filter(|&&v| v).count();
    let vf = occupied_count as f64 / total_vox as f64;

    if occupied_count == 0 {
        return vec![0.0; r_max + 1];
    }

    match method {
        "exact" => {
            let fx = (2 * nx).saturating_sub(1).max(1);
            let fy = (2 * ny).saturating_sub(1).max(1);
            let fz = (2 * nz).saturating_sub(1).max(1);
            let fft_cells = fx.saturating_mul(fy).saturating_mul(fz);
            let max_fft_cells = 24_000_000usize;

            if fft_cells > max_fft_cells {
                calculate_s2_exact_direct(&occ, nx, ny, nz, r_max, vf)
            } else {
                calculate_s2_exact_fft(&occ, nx, ny, nz, r_max, vf)
            }
        }
        _ => {
            let mc_samples = samples.max(200);
            let shells: Vec<Vec<[isize; 3]>> = (1..=r_max).map(shell_offsets_for_r).collect();
            let mut out: Vec<f64> = (0..=r_max)
                .into_par_iter()
                .map(|r| {
                    if r == 0 {
                        return vf;
                    }

                    let offsets = &shells[r - 1];
                    let mut valid = 0usize;
                    let mut hits = 0usize;
                    let mut rng = rand::thread_rng();

                    for _ in 0..mc_samples {
                        let x = rng.gen_range(0..nx);
                        let y = rng.gen_range(0..ny);
                        let z = rng.gen_range(0..nz);
                        let off = &offsets[rng.gen_range(0..offsets.len())];

                        let x2 = x as isize + off[0];
                        let y2 = y as isize + off[1];
                        let z2 = z as isize + off[2];
                        if x2 < 0 || y2 < 0 || z2 < 0 {
                            continue;
                        }
                        let x2u = x2 as usize;
                        let y2u = y2 as usize;
                        let z2u = z2 as usize;
                        if x2u >= nx || y2u >= ny || z2u >= nz {
                            continue;
                        }

                        valid += 1;
                        let i1 = index_3d_to_flat(x, y, z, ny, nz);
                        let i2 = index_3d_to_flat(x2u, y2u, z2u, ny, nz);
                        if occ[i1] && occ[i2] {
                            hits += 1;
                        }
                    }

                    if valid == 0 {
                        0.0
                    } else {
                        hits as f64 / valid as f64
                    }
                })
                .collect();
            out[0] = vf;
            out
        }
    }
}

pub fn approximate_s2(mesh: &Mesh, bbox: BoundingBox, r_max: usize, samples: usize) -> Vec<f64> {
    // Purpose: Convenience wrapper for Monte Carlo S2 estimation.
    // Inputs: mesh, bbox, max radius, sample count.
    // Outputs: S2 values.
    calculate_s2(mesh, bbox, r_max, 1.0, "monte_carlo", samples)
}

pub fn l2_norm(a: &[f64], b: &[f64]) -> f64 {
    // Purpose: Compute L2 norm between two vectors over common length.
    // Inputs: two numeric slices.
    // Outputs: non-negative L2 distance.
    let n = a.len().min(b.len());
    if n == 0 {
        return 0.0;
    }
    let sum = (0..n).map(|i| {
        let d = a[i] - b[i];
        d * d
    });
    sum.sum::<f64>().sqrt()
}
