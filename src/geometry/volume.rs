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
// `edges`/`used` are BTreeSet, not HashSet, and must stay that way: ring traversal order decides
// each ring's vertex-mean centre, which is a float sum, which perturbs the capped volume in its
// last bits. HashSet's RandomState is seeded per process AND bumped per instance, so the same mesh
// clipped twice would not give the same number. Two known limitations remain, which is why the
// placement engine uses mesh_volume_in_bbox_exact instead of this path: a ring that is not
// star-shaped about its own vertex mean is re-ordered into a different polygon, and a ring nested
// inside another is emitted with the same winding, so a hole is filled rather than subtracted.
fn triangulate_cap_from_segments(segments: &[(Vec3, Vec3)], normal: Vec3) -> Mesh {
    if segments.is_empty() {
        return Mesh::empty();
    }

    let mut points: Vec<Vec3> = Vec::new();
    let mut point_map: std::collections::HashMap<(i64, i64, i64), usize> = std::collections::HashMap::new();
    let mut adjacency: std::collections::HashMap<usize, Vec<usize>> = std::collections::HashMap::new();
    let mut edges: std::collections::BTreeSet<(usize, usize)> = std::collections::BTreeSet::new();

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

    let mut used: std::collections::BTreeSet<(usize, usize)> = std::collections::BTreeSet::new();
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
// Inputs: owned mesh, plane origin, plane normal.
// Returns: Unchanged allocation if entirely inside (including tangency), empty if outside, otherwise clipped and capped mesh.
// Side effects: None.
// Notes: Uses Sutherland-Hodgman polygon clipping per triangle, then collects cross-plane segments to form a watertight cap.
fn clip_mesh_by_plane_with_cap(mesh: Mesh, origin: Vec3, normal: Vec3) -> Mesh {
    let eps = 1e-9;
    let mut inside = false;
    let mut outside = false;
    for vertex in mesh.faces.iter().flat_map(|f| [mesh.vertices[f.a], mesh.vertices[f.b], mesh.vertices[f.c]]) {
        if clip_plane_signed_distance(vertex, origin, normal) >= -eps {
            inside = true;
        } else {
            outside = true;
        }
        if inside && outside { break; }
    }
    // A touching plane opens no hole: retain topology, winding and allocation.
    if !outside { return mesh; }
    if !inside { return Mesh::empty(); }
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
        out = clip_mesh_by_plane_with_cap(out, origin, normal);
        if out.is_empty() {
            break;
        }
    }
    out
}

// AI-FUNC-SUMMARY: Compute in-box volume, directly summing wholly contained geometry without allocation and otherwise clipping; returns f64; no external side effects.
pub fn particle_volume_in_bbox(mesh: &Mesh, bbox: BoundingBox) -> f64 {
    if mesh.vertices.iter().all(|v| v.x >= bbox.min.x && v.x <= bbox.max.x
        && v.y >= bbox.min.y && v.y <= bbox.max.y && v.z >= bbox.min.z && v.z <= bbox.max.z) {
        return mesh_volume(mesh);
    }
    let clipped = clip_mesh_by_bbox(mesh, bbox);
    mesh_volume(&clipped)
}

/// Granules in one mesh from which their in-box volumes are measured in parallel.
const VF_PARALLEL_MIN_PARTS: usize = 32;

// AI-FUNC-SUMMARY: Compute the volume fraction of a single mesh within a bounding box; returns f64 in [0,1]; side effects: None.
pub fn volume_fraction_in_bbox(mesh: &Mesh, bbox: BoundingBox) -> f64 {
    volume_fraction_of_meshes_in_bbox(std::slice::from_ref(mesh), bbox)
}

// AI-FUNC-SUMMARY:
// Purpose: Compute the total volume fraction of multiple meshes within a bounding box, using parallel iteration.
// Inputs: slice of meshes and bounding box.
// Returns: Volume fraction in [0,1] clamped.
// Side effects: None.
// Notes: Splits each mesh into granules before clipping for correct volume computation. A mesh with at least
// VF_PARALLEL_MIN_PARTS granules measures them in parallel into an indexed buffer and sums it in granule
// order, so its volume is bit-identical to the serial sum; forge passes one merged mesh, so without this
// its whole VF ran on one worker.
pub fn volume_fraction_of_meshes_in_bbox(meshes: &[Mesh], bbox: BoundingBox) -> f64 {
    let box_volume = bbox.volume().max(1e-12);

    let in_box_volume: f64 = meshes
        .par_iter()
        .map(|mesh| {
            let parts = split_mesh_into_granules(mesh);
            if parts.is_empty() {
                particle_volume_in_bbox(mesh, bbox)
            } else if parts.len() >= VF_PARALLEL_MIN_PARTS {
                let volumes: Vec<f64> = parts.par_iter().map(|p| particle_volume_in_bbox(p, bbox)).collect();
                volumes.iter().sum::<f64>()
            } else {
                parts.iter().map(|p| particle_volume_in_bbox(p, bbox)).sum::<f64>()
            }
        })
        .sum();

    (in_box_volume / box_volume).clamp(0.0, 1.0)
}

/// A polygon mid-clip: each vertex paired with the id of the clip plane that
/// created the edge arriving at it, or `None` when that edge came from the mesh.
type TaggedPolygon = Vec<(Vec3, Option<usize>)>;

/// A tagged polygon plus whether it is a cap this routine built rather than a
/// piece of the original surface. Only the latter decides which faces were cut.
type ClipPiece = (TaggedPolygon, bool);

/// Which axis-aligned domain face a clip cut against, in the names the record uses.
pub const DOMAIN_FACE_NAMES: [&str; 6] = ["xmin", "xmax", "ymin", "ymax", "zmin", "zmax"];

// AI-FUNC-SUMMARY:
// Purpose: Compute the volume centroid of a closed mesh (the centre of mass at unit density).
// Inputs: mesh reference.
// Returns: Some(centroid), or None when the signed volume is zero or non-finite.
// Side effects: None.
// Notes: NOT mesh_centroid, which is the mean of the vertices and therefore depends on how finely
// each region happens to be tessellated. This is the quantity the placement record calls
// shell_centroid and translation: it is the pivot the published transform rotates about, so a
// reader that recomputed a vertex mean instead would reconstruct a different particle.
// Sums signed tetrahedra from the origin, so it is exact for any closed orientable mesh and
// independent of where the origin sits.
pub fn mesh_volume_centroid(mesh: &Mesh) -> Option<Vec3> {
    let mut volume = 0.0;
    let mut moment = Vec3::new(0.0, 0.0, 0.0);
    for f in &mesh.faces {
        let a = mesh.vertices[f.a];
        let b = mesh.vertices[f.b];
        let c = mesh.vertices[f.c];
        let v = a.dot(b.cross(c)) / 6.0;
        volume += v;
        moment = moment.add(a.add(b).add(c).scale(v / 4.0));
    }
    if !volume.is_finite() || volume.abs() <= f64::MIN_POSITIVE {
        return None;
    }
    let centroid = moment.scale(1.0 / volume);
    if centroid.x.is_finite() && centroid.y.is_finite() && centroid.z.is_finite() {
        Some(centroid)
    } else {
        None
    }
}

// AI-FUNC-SUMMARY:
// Purpose: Report the signed volume of every edge-connected shell of a mesh, in shell order.
// Inputs: mesh reference.
// Returns: one signed volume per shell, in the order split_mesh_into_granules yields them.
// Side effects: None.
// Notes: The signs are the point. mesh_volume takes the absolute value of the whole sum, so a mesh
// whose shells disagree about orientation reports |V1 - V2| rather than V1 + V2 - a void made of
// two pores, one of them inverted, would silently report almost nothing. Callers that need a total
// must check the signs agree first.
pub fn shell_signed_volumes(mesh: &Mesh) -> Vec<f64> {
    split_mesh_into_granules(mesh)
        .iter()
        .map(mesh_signed_volume)
        .collect()
}

// AI-FUNC-SUMMARY: Signed distance from a point to a plane, positive on the normal's side; returns f64; side effects: none.
fn plane_signed_distance(p: Vec3, origin: Vec3, normal: Vec3) -> f64 {
    p.sub(origin).dot(normal)
}

// AI-FUNC-SUMMARY:
// Purpose: Clip a tagged polygon against one half-space, tagging every edge the clip itself created.
// Inputs: polygon as (vertex, tag of the edge arriving at that vertex), plane origin/normal, plane id, tolerance.
// Returns: the clipped polygon in the same tagged representation.
// Side effects: None.
// Notes: Sutherland-Hodgman, with one addition: the single edge the clip introduces along the plane
// is tagged `Some(plane_id)`, and inherited edges keep their tag. Tagging during the clip rather
// than detecting coplanar edges afterwards is what makes a face lying flush against the plane
// behave correctly - all its vertices count as inside, no edge is tagged, and the face is treated
// as the genuine boundary it is instead of as the boundary of a phantom cap.
fn clip_tagged_polygon(
    poly: &[(Vec3, Option<usize>)],
    origin: Vec3,
    normal: Vec3,
    plane_id: usize,
    eps: f64,
) -> TaggedPolygon {
    if poly.is_empty() {
        return Vec::new();
    }
    let mut out: TaggedPolygon = Vec::with_capacity(poly.len() + 2);
    for i in 0..poly.len() {
        let (c, _) = poly[i];
        let (n, tag) = poly[(i + 1) % poly.len()];
        let dc = plane_signed_distance(c, origin, normal);
        let dn = plane_signed_distance(n, origin, normal);
        let in_c = dc >= -eps;
        let in_n = dn >= -eps;
        match (in_c, in_n) {
            (true, true) => out.push((n, tag)),
            (true, false) => {
                out.push((clip_segment_plane_intersection(c, n, dc, dn), tag));
            }
            (false, true) => {
                out.push((
                    clip_segment_plane_intersection(c, n, dc, dn),
                    Some(plane_id),
                ));
                out.push((n, tag));
            }
            (false, false) => {}
        }
    }
    out
}

// AI-FUNC-SUMMARY:
// Purpose: Compute a closed mesh's volume restricted to an axis-aligned box, and which box faces cut it.
// Inputs: the mesh (closed, consistently oriented outward) and the box.
// Returns: (volume inside the box, one flag per box face in DOMAIN_FACE_NAMES order marking a real cut).
// Side effects: None.
// Notes: The placement engine's volume routine, and the reason it does not call
// particle_volume_in_bbox. Planes are applied one at a time; after each, the hole that plane opened
// is closed by fanning every edge the clip created back to one fixed apex on that plane. A fan
// reproduces a closed loop's signed area exactly no matter what shape the loop has - the overlapping
// pieces cancel - so nothing here assumes a cap is star-shaped, and a loop nested inside another
// arrives with the opposite winding and subtracts itself. That is what triangulate_cap_from_segments
// gets wrong. Capping before moving to the next plane is also what supplies each later cap with its
// corner edges: the segment where two box faces meet bounds both caps but lies on neither's surface,
// so a routine that clipped all six planes first would leave every multi-plane cap loop open.
// Every sum runs in face order with no set iteration, so the result is bit-reproducible.
// The mesh must be closed and outward-oriented; an inward-oriented one returns a negative volume,
// which the caller should read as an orientation error rather than clamp. Note that box_mesh emits
// inward-facing triangles, so a box fixture must be flipped before it is measured here.
pub fn mesh_volume_in_bbox_exact(mesh: &Mesh, bbox: BoundingBox) -> (f64, [bool; 6]) {
    let centre = Vec3::new(
        (bbox.min.x + bbox.max.x) * 0.5,
        (bbox.min.y + bbox.max.y) * 0.5,
        (bbox.min.z + bbox.max.z) * 0.5,
    );
    // (plane origin, inward normal, fan apex on that plane)
    let planes: [(Vec3, Vec3, Vec3); 6] = [
        (
            Vec3::new(bbox.min.x, 0.0, 0.0),
            Vec3::new(1.0, 0.0, 0.0),
            Vec3::new(bbox.min.x, centre.y, centre.z),
        ),
        (
            Vec3::new(bbox.max.x, 0.0, 0.0),
            Vec3::new(-1.0, 0.0, 0.0),
            Vec3::new(bbox.max.x, centre.y, centre.z),
        ),
        (
            Vec3::new(0.0, bbox.min.y, 0.0),
            Vec3::new(0.0, 1.0, 0.0),
            Vec3::new(centre.x, bbox.min.y, centre.z),
        ),
        (
            Vec3::new(0.0, bbox.max.y, 0.0),
            Vec3::new(0.0, -1.0, 0.0),
            Vec3::new(centre.x, bbox.max.y, centre.z),
        ),
        (
            Vec3::new(0.0, 0.0, bbox.min.z),
            Vec3::new(0.0, 0.0, 1.0),
            Vec3::new(centre.x, centre.y, bbox.min.z),
        ),
        (
            Vec3::new(0.0, 0.0, bbox.max.z),
            Vec3::new(0.0, 0.0, -1.0),
            Vec3::new(centre.x, centre.y, bbox.max.z),
        ),
    ];
    let eps = 1e-9;

    // Each entry is (tagged polygon, whether it came from a cap rather than the mesh).
    let mut polys: Vec<ClipPiece> = mesh
        .faces
        .iter()
        .map(|f| {
            (
                vec![
                    (mesh.vertices[f.a], None),
                    (mesh.vertices[f.b], None),
                    (mesh.vertices[f.c], None),
                ],
                false,
            )
        })
        .collect();

    for (plane_id, (origin, normal, apex)) in planes.iter().enumerate() {
        let mut next: Vec<ClipPiece> = Vec::with_capacity(polys.len());
        let mut cap_edges: Vec<(Vec3, Vec3)> = Vec::new();
        for (poly, is_cap) in &polys {
            let clipped = clip_tagged_polygon(poly, *origin, *normal, plane_id, eps);
            if clipped.len() < 3 {
                continue;
            }
            for i in 0..clipped.len() {
                let (a, _) = clipped[i];
                let (b, tag) = clipped[(i + 1) % clipped.len()];
                if tag == Some(plane_id) {
                    cap_edges.push((a, b));
                }
            }
            next.push((clipped, *is_cap));
        }
        // The cap's boundary runs opposite to the surface edges that opened it, so
        // the fan triangle for edge a -> b is (apex, b, a).
        for (a, b) in cap_edges {
            next.push((vec![(*apex, None), (b, None), (a, None)], true));
        }
        polys = next;
        if polys.is_empty() {
            break;
        }
    }

    let mut volume = 0.0;
    let mut cut = [false; 6];
    for (poly, is_cap) in &polys {
        if poly.len() >= 3 {
            let p0 = poly[0].0;
            for i in 1..(poly.len() - 1) {
                volume += p0.dot(poly[i].0.cross(poly[i + 1].0)) / 6.0;
            }
        }
        // A face counts as cut only when a surviving piece of the original surface
        // still carries an edge the clip created. A cap's own edges do not count,
        // and neither does a polygon that a later plane removed entirely.
        if !is_cap {
            for (_, tag) in poly {
                if let Some(plane_id) = tag {
                    cut[*plane_id] = true;
                }
            }
        }
    }

    (volume, cut)
}

// AI-FUNC-SUMMARY:
// Purpose: Name the domain faces a clip actually cut, for the placement record's `clipped.faces`.
// Inputs: the per-face cut flags from mesh_volume_in_bbox_exact.
// Returns: face names in DOMAIN_FACE_NAMES order.
// Side effects: None.
// Notes: The names match the vocabulary the consuming project already uses for domain faces.
pub fn cut_face_names(cut: [bool; 6]) -> Vec<&'static str> {
    DOMAIN_FACE_NAMES
        .iter()
        .zip(cut.iter())
        .filter(|(_, hit)| **hit)
        .map(|(name, _)| *name)
        .collect()
}

#[cfg(test)]
mod clip_fast_path_tests {
    use super::*;
    use crate::geometry::box_mesh;

    // AI-FUNC-SUMMARY: Verify tangent/contained boxes preserve topology, signed volume and owned buffers for both windings at multiple origins; fully outside geometry clips to empty.
    #[test]
    fn touching_planes_preserve_closed_mesh_and_volume() {
        for origin in [Vec3::new(0.0, 0.0, 0.0), Vec3::new(11.0, -7.0, 3.0)] {
            let bbox = BoundingBox { min: origin, max: origin.add(Vec3::new(1.0, 2.0, 3.0)) };
            for flip in [false, true] {
                let mut mesh = box_mesh(bbox);
                if flip { for face in &mut mesh.faces { std::mem::swap(&mut face.b, &mut face.c); } }
                let volume = mesh_signed_volume(&mesh);
                assert!((volume.abs() - 6.0).abs() < 1e-10);
                let clipped = clip_mesh_by_bbox(&mesh, bbox);
                assert_eq!(clipped, mesh);
                assert_eq!(mesh_signed_volume(&clipped), volume);
                assert!((particle_volume_in_bbox(&mesh, bbox) - 6.0).abs() < 1e-10);
                assert!((volume_fraction_in_bbox(&mesh, bbox) - 1.0).abs() < 1e-10);
                let pointer = mesh.vertices.as_ptr();
                let faces = mesh.faces.as_ptr();
                let same = clip_mesh_by_plane_with_cap(mesh, origin, Vec3::new(1.0, 0.0, 0.0));
                assert_eq!(same.vertices.as_ptr(), pointer);
                assert_eq!(same.faces.as_ptr(), faces);
                assert!(clip_mesh_by_plane_with_cap(same, origin.add(Vec3::new(2.0, 0.0, 0.0)), Vec3::new(1.0, 0.0, 0.0)).is_empty());
            }
        }
    }
    // AI-FUNC-SUMMARY: Check partial clipping of an outward box against analytic intersections, including tangent side planes and an unused outside vertex that must not create a cap.
    #[test]
    fn partial_box_cuts_and_unused_vertices() {
        let bbox = BoundingBox::from_size(Vec3::new(1.0, 1.0, 1.0));
        let mut mesh = box_mesh(bbox);
        for f in &mut mesh.faces { std::mem::swap(&mut f.b, &mut f.c); }
        for (max, expected) in [(Vec3::new(0.5,1.0,1.0),0.5), (Vec3::new(0.5,0.5,0.5),0.125)] {
            let domain = BoundingBox { min: bbox.min, max };
            let actual = particle_volume_in_bbox(&mesh, domain);
            assert!((actual - expected).abs() < 1e-10, "{actual} != {expected}");
        }
        mesh.vertices.push(Vec3::new(10.0,10.0,10.0));
        assert!((particle_volume_in_bbox(&mesh, bbox) - 1.0).abs() < 1e-10);
    }

    // AI-FUNC-SUMMARY: Preserve the pre-optimization plane clipper as a test-only timing reference.
fn legacy_plane_clip(mesh: &Mesh, origin: Vec3, normal: Vec3) -> Mesh {
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

    // AI-FUNC-SUMMARY: Apply the original six clipping passes to benchmark unchanged strictly-contained volume semantics.
    fn legacy_box_volume(mesh: &Mesh, bbox: BoundingBox) -> f64 {
        let planes = [
            (Vec3::new(bbox.min.x,0.0,0.0),Vec3::new(1.0,0.0,0.0)),
            (Vec3::new(bbox.max.x,0.0,0.0),Vec3::new(-1.0,0.0,0.0)),
            (Vec3::new(0.0,bbox.min.y,0.0),Vec3::new(0.0,1.0,0.0)),
            (Vec3::new(0.0,bbox.max.y,0.0),Vec3::new(0.0,-1.0,0.0)),
            (Vec3::new(0.0,0.0,bbox.min.z),Vec3::new(0.0,0.0,1.0)),
            (Vec3::new(0.0,0.0,bbox.max.z),Vec3::new(0.0,0.0,-1.0)),
        ];
        let mut out = mesh.clone();
        for (origin, normal) in planes { out = legacy_plane_clip(&out, origin, normal); }
        mesh_volume(&out)
    }

    // AI-FUNC-SUMMARY: Measure original six-pass clipping versus the allocation-free contained-volume path with alternating order, one warmup and five raw release samples.
    #[test]
    #[ignore = "release volume microbenchmark"]
    fn contained_volume_benchmark() {
        let bbox = BoundingBox::from_size(Vec3::new(1.0,1.0,1.0));
        let mesh = box_mesh(BoundingBox { min: Vec3::new(0.2,0.2,0.2), max: Vec3::new(0.8,0.8,0.8) });
        assert!((legacy_box_volume(&mesh,bbox) - particle_volume_in_bbox(&mesh,bbox)).abs() < 1e-12);
        let run = |old: bool| {
            let start = std::time::Instant::now();
            let mut sum = 0.0;
            for _ in 0..10_000 {
                let input = std::hint::black_box(&mesh);
                sum += if old { legacy_box_volume(input,bbox) } else { particle_volume_in_bbox(input,bbox) };
            }
            std::hint::black_box(sum);
            start.elapsed().as_secs_f64()
        };
        for sample in 0..6 {
            let (old,new) = if sample % 2 == 0 { (run(true),run(false)) } else { let new=run(false); (run(true),new) };
            eprintln!("VOLUME_BENCH sample={sample} repeats=10000 legacy={old:.9} candidate={new:.9}");
        }
    }

}
