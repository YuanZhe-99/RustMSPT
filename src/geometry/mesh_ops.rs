use crate::types::{BoundingBox, Mesh, Triangle, Vec3};
#[cfg(test)]
use std::collections::{HashMap, VecDeque};

// AI-FUNC-SUMMARY: Compute the arithmetic centroid of all mesh vertices; returns Vec3 (zero for empty mesh); side effects: None.
pub fn mesh_centroid(mesh: &Mesh) -> Vec3 {
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

// AI-FUNC-SUMMARY: Compute the Euclidean norm (length) of a Vec3; returns f64; side effects: None.
pub fn vec_norm(v: Vec3) -> f64 {
    (v.x * v.x + v.y * v.y + v.z * v.z).sqrt()
}

// AI-FUNC-SUMMARY: Merge multiple meshes into one by combining vertices and remapping face indices; returns merged Mesh; side effects: None.
pub fn merge_meshes(meshes: &[Mesh]) -> Mesh {
    let mut out = Mesh {
        vertices: Vec::with_capacity(meshes.iter().map(|m| m.vertices.len()).sum()),
        faces: Vec::with_capacity(meshes.iter().map(|m| m.faces.len()).sum()),
    };
    for m in meshes {
        let offset = out.vertices.len();
        out.vertices.extend(m.vertices.iter().copied());
        out.faces.extend(m.faces.iter().map(|f| Triangle {
            a: f.a + offset,
            b: f.b + offset,
            c: f.c + offset,
        }));
    }
    out
}

// AI-FUNC-SUMMARY:
// Purpose: Split a mesh into separate connected components (granules) using BFS over shared vertices.
// Inputs: mesh reference.
// Returns: Vec<Mesh> where each element is one connected component with remapped vertex indices.
// Side effects: None.
// Notes: Returns empty vec for empty mesh. Each granule has its own independent vertex buffer. Vertex-to-face adjacency is a CSR (count, prefix sum, contiguous face ids in face order), the BFS queue doubles as the component face list, and the vertex remap is a stamped array; component order, first-face order and remap are identical to the former Vec<Vec>/VecDeque/HashMap version kept as a test oracle.
pub fn split_mesh_into_granules(mesh: &Mesh) -> Vec<Mesh> {
    if mesh.faces.is_empty() || mesh.vertices.is_empty() {
        return Vec::new();
    }

    let vertex_count = mesh.vertices.len();
    let mut offsets = vec![0usize; vertex_count + 1];
    for face in &mesh.faces {
        offsets[face.a + 1] += 1;
        offsets[face.b + 1] += 1;
        offsets[face.c + 1] += 1;
    }
    for i in 0..vertex_count {
        offsets[i + 1] += offsets[i];
    }
    let mut cursor = offsets[..vertex_count].to_vec();
    let mut adjacency = vec![0usize; offsets[vertex_count]];
    for (face_index, face) in mesh.faces.iter().enumerate() {
        for vertex in [face.a, face.b, face.c] {
            adjacency[cursor[vertex]] = face_index;
            cursor[vertex] += 1;
        }
    }
    drop(cursor);

    let mut visited = vec![false; mesh.faces.len()];
    let mut stamp = vec![usize::MAX; vertex_count];
    let mut local_index = vec![0usize; vertex_count];
    let mut queue: Vec<usize> = Vec::new();
    let mut parts: Vec<Mesh> = Vec::new();

    for start_face in 0..mesh.faces.len() {
        if visited[start_face] {
            continue;
        }

        queue.clear();
        visited[start_face] = true;
        queue.push(start_face);
        let mut head = 0;
        while head < queue.len() {
            let f = &mesh.faces[queue[head]];
            head += 1;
            for vertex_index in [f.a, f.b, f.c] {
                for &adjacent_face in &adjacency[offsets[vertex_index]..offsets[vertex_index + 1]] {
                    if !visited[adjacent_face] {
                        visited[adjacent_face] = true;
                        queue.push(adjacent_face);
                    }
                }
            }
        }

        let component = parts.len();
        let mut local_vertices: Vec<Vec3> = Vec::new();
        let mut local_faces = Vec::with_capacity(queue.len());
        for &face_index in &queue {
            let f = &mesh.faces[face_index];
            let mut map_vertex = |global: usize| {
                if stamp[global] != component {
                    stamp[global] = component;
                    local_index[global] = local_vertices.len();
                    local_vertices.push(mesh.vertices[global]);
                }
                local_index[global]
            };
            let a = map_vertex(f.a);
            let b = map_vertex(f.b);
            let c = map_vertex(f.c);
            local_faces.push(Triangle { a, b, c });
        }

        parts.push(Mesh {
            vertices: local_vertices,
            faces: local_faces,
        });
    }

    parts
}

#[cfg(test)]
// AI-FUNC-SUMMARY:
// Purpose: Former Vec<Vec>/VecDeque/HashMap granule split, kept only as the test oracle and benchmark baseline for split_mesh_into_granules.
// Inputs: mesh reference.
// Returns: Vec<Mesh> where each element is one connected component with remapped vertex indices.
// Side effects: None.
// Notes: Returns empty vec for empty mesh. Each granule has its own independent vertex buffer.
pub(crate) fn split_mesh_into_granules_reference(mesh: &Mesh) -> Vec<Mesh> {
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
            local_faces.push(Triangle { a, b, c });
        }

        parts.push(Mesh {
            vertices: local_vertices,
            faces: local_faces,
        });
    }

    parts
}

// AI-FUNC-SUMMARY: Translate all mesh vertices by a delta vector; mutates mesh in place; side effects: None.
pub fn translate_mesh(mesh: &mut Mesh, delta: Vec3) {
    map_vertices(&mut mesh.vertices, |v| v.add(delta));
}

// AI-FUNC-SUMMARY: Move a mesh so its centroid aligns with the target position; mutates mesh in place; side effects: None.
pub fn move_mesh_to_target_center(mesh: &mut Mesh, target: Vec3) {
    let center = mesh_centroid(mesh);
    translate_mesh(mesh, target.sub(center));
}

// AI-FUNC-SUMMARY:
// Purpose: Wrap a mesh centroid into the box using Euclidean modulo, then re-center the mesh.
// Inputs: mutable mesh and box bounds.
// Returns: None (mutates mesh in place).
// Side effects: Mutates mesh vertex positions.
// Notes: Used for periodic boundary conditions to keep particles inside the domain.
pub fn wrap_mesh_centroid_to_box(mesh: &mut Mesh, box_bounds: BoundingBox) {
    let center = mesh_centroid(mesh);
    let size = box_bounds.size();

    let wrap_axis = |value: f64, min: f64, len: f64| -> f64 {
        if len <= 0.0 {
            return value;
        }
        min + (value - min).rem_euclid(len)
    };

    let wrapped = Vec3::new(
        wrap_axis(center.x, box_bounds.min.x, size.x),
        wrap_axis(center.y, box_bounds.min.y, size.y),
        wrap_axis(center.z, box_bounds.min.z, size.z),
    );

    move_mesh_to_target_center(mesh, wrapped);
}

// AI-FUNC-SUMMARY: Scale all mesh vertices by a uniform factor (centered at origin); mutates mesh in place; side effects: None.
pub fn scale_mesh(mesh: &mut Mesh, factor: f64) {
    map_vertices(&mut mesh.vertices, |v| v.scale(factor));
}

// AI-FUNC-SUMMARY: Apply an independent vertex map in place, using Rayon chunks only above 65536 vertices per worker (at least 131072 total) and a serial loop for small/single-worker inputs; face topology is unchanged.
pub(crate) fn map_vertices(vertices: &mut [Vec3], transform: impl Fn(Vec3) -> Vec3 + Sync + Send) {
    use rayon::prelude::*;
    let workers = rayon::current_num_threads();
    let parallel_min = workers.saturating_mul(65536).max(131072);
    if workers == 1 || vertices.len() < parallel_min {
        for v in vertices { *v = transform(*v); }
    } else {
        vertices.par_chunks_mut(8192).for_each(|chunk| {
            for v in chunk { *v = transform(*v); }
        });
    }
}

// AI-FUNC-SUMMARY: Apply map_vertices' transform and return the arithmetic centroid of the transformed vertices, bit-identical to mesh_centroid after map_vertices; the serial branch fuses both into one pass, the parallel branch maps in chunks then sums in index order; returns Vec3 (zero when empty); side effects: mutates vertices.
pub(crate) fn map_vertices_centroid(vertices: &mut [Vec3], transform: impl Fn(Vec3) -> Vec3 + Sync + Send) -> Vec3 {
    let workers = rayon::current_num_threads();
    let parallel_min = workers.saturating_mul(65536).max(131072);
    let mut sum = Vec3::new(0.0, 0.0, 0.0);
    if workers == 1 || vertices.len() < parallel_min {
        for v in vertices.iter_mut() {
            *v = transform(*v);
            sum = sum.add(*v);
        }
    } else {
        map_vertices(vertices, transform);
        for v in vertices.iter() {
            sum = sum.add(*v);
        }
    }
    let count = vertices.len() as f64;
    if count > 0.0 {
        sum.scale(1.0 / count)
    } else {
        sum
    }
}

// AI-FUNC-SUMMARY: Compute the total surface area of a mesh by summing triangle areas via cross product; returns f64; side effects: None.
pub fn mesh_surface_area(mesh: &Mesh) -> f64 {
    let mut area = 0.0;
    for f in &mesh.faces {
        let a = mesh.vertices[f.a];
        let b = mesh.vertices[f.b];
        let c = mesh.vertices[f.c];
        let ab = b.sub(a);
        let ac = c.sub(a);
        area += 0.5 * vec_norm(ab.cross(ac));
    }
    area
}

// AI-FUNC-SUMMARY:
// Purpose: Rotate a mesh around its centroid using Rodrigues' rotation formula.
// Inputs: mutable mesh, rotation axis (need not be normalized), angle in radians.
// Returns: None (mutates mesh in place).
// Side effects: Mutates mesh vertex positions.
// Notes: No-op if axis length is near zero.
pub fn rotate_mesh_around_center(mesh: &mut Mesh, axis: Vec3, angle: f64) {
    let axis_len = vec_norm(axis);
    if axis_len <= 1e-12 {
        return;
    }

    let k = axis.scale(1.0 / axis_len);
    let center = mesh_centroid(mesh);
    let cos_t = angle.cos();
    let sin_t = angle.sin();

    for v in &mut mesh.vertices {
        let p = v.sub(center);
        let term1 = p.scale(cos_t);
        let term2 = k.cross(p).scale(sin_t);
        let term3 = k.scale(k.dot(p) * (1.0 - cos_t));
        *v = center.add(term1.add(term2).add(term3));
    }
}

// AI-FUNC-SUMMARY: Generate a triangle mesh representing a rectangular box from a BoundingBox; returns Mesh with 8 vertices and 12 triangles; side effects: None.
pub fn box_mesh(bbox: BoundingBox) -> Mesh {
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
        .map(|(a, b, c)| Triangle {
            a: *a,
            b: *b,
            c: *c,
        })
        .collect();

    Mesh { vertices: v, faces }
}

// AI-FUNC-SUMMARY:
// Purpose: Build a closed, outward-oriented triangulated sphere by recursive subdivision of an icosahedron.
// Inputs: centre, radius, and subdivision level (0 = the bare icosahedron).
// Returns: a Mesh with 20 * 4^level faces and no duplicate vertices.
// Side effects: None.
// Notes: A sphere, unlike a box, has a well-defined distance to another sphere at every point, so
// gaps and overlap volumes between two of them can be checked against a closed-form answer rather
// than against this crate's own arithmetic. Vertices are deduplicated by quantized midpoint key, so
// the result is watertight and mesh_is_closed accepts it. Level grows the face count fourfold each
// step; level 3 (1280 faces) is about the largest that stays comfortable in a unit test.
pub fn icosphere_mesh(center: Vec3, radius: f64, level: u32) -> Mesh {
    let phi = (1.0 + 5.0_f64.sqrt()) / 2.0;
    let mut verts: Vec<Vec3> = vec![
        Vec3::new(-1.0, phi, 0.0),
        Vec3::new(1.0, phi, 0.0),
        Vec3::new(-1.0, -phi, 0.0),
        Vec3::new(1.0, -phi, 0.0),
        Vec3::new(0.0, -1.0, phi),
        Vec3::new(0.0, 1.0, phi),
        Vec3::new(0.0, -1.0, -phi),
        Vec3::new(0.0, 1.0, -phi),
        Vec3::new(phi, 0.0, -1.0),
        Vec3::new(phi, 0.0, 1.0),
        Vec3::new(-phi, 0.0, -1.0),
        Vec3::new(-phi, 0.0, 1.0),
    ];
    let mut faces: Vec<[usize; 3]> = vec![
        [0, 11, 5],
        [0, 5, 1],
        [0, 1, 7],
        [0, 7, 10],
        [0, 10, 11],
        [1, 5, 9],
        [5, 11, 4],
        [11, 10, 2],
        [10, 7, 6],
        [7, 1, 8],
        [3, 9, 4],
        [3, 4, 2],
        [3, 2, 6],
        [3, 6, 8],
        [3, 8, 9],
        [4, 9, 5],
        [2, 4, 11],
        [6, 2, 10],
        [8, 6, 7],
        [9, 8, 1],
    ];

    for _ in 0..level {
        let mut midpoints: std::collections::BTreeMap<(usize, usize), usize> =
            std::collections::BTreeMap::new();
        let mut next: Vec<[usize; 3]> = Vec::with_capacity(faces.len() * 4);
        for tri in &faces {
            let mut mid = [0usize; 3];
            for (edge, slot) in [(0usize, 1usize), (1, 2), (2, 0)].iter().zip(mid.iter_mut()) {
                let (i, j) = (tri[edge.0], tri[edge.1]);
                let key = if i < j { (i, j) } else { (j, i) };
                *slot = *midpoints.entry(key).or_insert_with(|| {
                    let m = verts[i].add(verts[j]).scale(0.5);
                    verts.push(m);
                    verts.len() - 1
                });
            }
            next.push([tri[0], mid[0], mid[2]]);
            next.push([mid[0], tri[1], mid[1]]);
            next.push([mid[2], mid[1], tri[2]]);
            next.push([mid[0], mid[1], mid[2]]);
        }
        faces = next;
    }

    let vertices = verts
        .into_iter()
        .map(|v| {
            let n = vec_norm(v);
            let unit = if n > 0.0 { v.scale(1.0 / n) } else { v };
            center.add(unit.scale(radius))
        })
        .collect();

    Mesh {
        vertices,
        faces: faces
            .into_iter()
            .map(|t| Triangle {
                a: t[0],
                b: t[1],
                c: t[2],
            })
            .collect(),
    }
}

#[cfg(test)]
mod split_tests {
    use super::*;
    use std::hint::black_box;
    use std::time::Instant;

    // AI-FUNC-SUMMARY: Deterministically permute a mesh's faces with a fixed LCG so components interleave in face order; returns the permuted mesh.
    fn shuffled(mut mesh: Mesh, seed: u64) -> Mesh {
        let mut state = seed;
        for i in (1..mesh.faces.len()).rev() {
            state = state.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
            let j = (state >> 33) as usize % (i + 1);
            mesh.faces.swap(i, j);
        }
        mesh
    }

    // AI-FUNC-SUMMARY: Build a grid of many small disjoint boxes merged into one mesh; returns the merged mesh.
    fn many_boxes(count: usize) -> Mesh {
        let boxes: Vec<Mesh> = (0..count)
            .map(|i| {
                let o = Vec3::new((i % 50) as f64 * 2.0, (i / 50 % 50) as f64 * 2.0, (i / 2500) as f64 * 2.0);
                box_mesh(BoundingBox { min: o, max: o.add(Vec3::new(1.0, 1.0, 1.0)) })
            })
            .collect();
        merge_meshes(&boxes)
    }

    // AI-FUNC-SUMMARY: Compare the CSR granule split with the retained HashMap/VecDeque oracle on empty, isolated-vertex, shared-vertex, degenerate, many-small, one-large and shuffled meshes.
    #[test]
    fn csr_split_matches_reference() {
        let mut cases: Vec<Mesh> = vec![
            Mesh { vertices: vec![], faces: vec![] },
            Mesh { vertices: vec![Vec3::new(0.0, 0.0, 0.0)], faces: vec![] },
        ];
        let p = |x: f64| Vec3::new(x, x * 0.5, -x);
        cases.push(Mesh {
            vertices: (0..9).map(|i| p(i as f64)).collect(),
            faces: vec![
                Triangle { a: 4, b: 1, c: 2 },
                Triangle { a: 7, b: 8, c: 6 },
                Triangle { a: 2, b: 3, c: 0 },
                Triangle { a: 5, b: 5, c: 5 },
                Triangle { a: 6, b: 5, c: 5 },
            ],
        });
        cases.push(many_boxes(300));
        cases.push(icosphere_mesh(Vec3::new(1.0, 2.0, 3.0), 2.0, 4));
        cases.push(shuffled(many_boxes(300), 7));
        let mut mixed = merge_meshes(&[many_boxes(40), icosphere_mesh(Vec3::new(-5.0, 0.0, 0.0), 1.0, 3)]);
        mixed.vertices.push(Vec3::new(9.0, 9.0, 9.0));
        mixed.vertices.insert(0, Vec3::new(8.0, 8.0, 8.0));
        for f in &mut mixed.faces {
            f.a += 1;
            f.b += 1;
            f.c += 1;
        }
        cases.push(shuffled(mixed, 11));
        for (index, mesh) in cases.iter().enumerate() {
            assert_eq!(split_mesh_into_granules(mesh), split_mesh_into_granules_reference(mesh), "case {index}");
        }
        assert_eq!(split_mesh_into_granules(&cases[2]).len(), 2);
        assert_eq!(split_mesh_into_granules(&cases[3]).len(), 300);
        assert_eq!(split_mesh_into_granules(&cases[4]).len(), 1);
    }

    // AI-FUNC-SUMMARY: Check the fused map-and-centroid equals map_vertices followed by mesh_centroid bit for bit across worker counts and the parallel cutoff.
    #[test]
    fn fused_centroid_matches_map_then_centroid() {
        for n in [0usize, 1, 13, 131071, 131073, 600001] {
            let source: Vec<Vec3> = (0..n)
                .map(|i| Vec3::new(1e8 + i as f64 * 0.01, -2.0 + (i % 37) as f64, (i % 113) as f64 * 0.07))
                .collect();
            let transform = |v: Vec3| Vec3::new(3.0 + (v.x - 3.0) * 0.8, v.y * 1.1, v.z * 1.1 - 0.5);
            for workers in [1, 2, 8] {
                let pool = rayon::ThreadPoolBuilder::new().num_threads(workers).build().unwrap();
                let (expected, expected_c, actual, actual_c) = pool.install(|| {
                    let mut expected = Mesh { vertices: source.clone(), faces: vec![] };
                    map_vertices(&mut expected.vertices, transform);
                    let expected_c = mesh_centroid(&expected);
                    let mut actual = source.clone();
                    let actual_c = map_vertices_centroid(&mut actual, transform);
                    (expected.vertices, expected_c, actual, actual_c)
                });
                assert_eq!(actual, expected);
                assert_eq!(actual_c.x.to_bits(), expected_c.x.to_bits());
                assert_eq!(actual_c.y.to_bits(), expected_c.y.to_bits());
                assert_eq!(actual_c.z.to_bits(), expected_c.z.to_bits());
            }
        }
    }

    // AI-FUNC-SUMMARY: Time reference versus CSR split on many small components and one large component (warmup plus five samples, release); prints medians.
    #[test]
    #[ignore = "release performance measurement"]
    fn split_benchmark() {
        let cases = [
            ("many_small_20000_boxes_shuffled", shuffled(many_boxes(20000), 3)),
            ("many_small_20000_boxes_ordered", many_boxes(20000)),
            ("one_large_icosphere_l7", icosphere_mesh(Vec3::new(0.0, 0.0, 0.0), 1.0, 7)),
        ];
        for (name, mesh) in &cases {
            let mut samples = [Vec::new(), Vec::new()];
            for round in 0..6 {
                let start = Instant::now();
                black_box(split_mesh_into_granules_reference(black_box(mesh)));
                let reference = start.elapsed().as_secs_f64();
                let start = Instant::now();
                black_box(split_mesh_into_granules(black_box(mesh)));
                let csr = start.elapsed().as_secs_f64();
                if round > 0 {
                    samples[0].push(reference);
                    samples[1].push(csr);
                }
            }
            for s in &mut samples {
                s.sort_by(f64::total_cmp);
            }
            println!(
                "split_benchmark {name} faces={} reference_median={:.6} csr_median={:.6} reference_samples={:?} csr_samples={:?}",
                mesh.faces.len(), samples[0][2], samples[1][2], samples[0], samples[1]
            );
        }
    }

    // AI-FUNC-SUMMARY: Time map_vertices+mesh_centroid versus fused map_vertices_centroid on one worker (warmup plus five samples, release); prints medians.
    #[test]
    #[ignore = "release performance measurement"]
    fn fused_centroid_benchmark() {
        let pool = rayon::ThreadPoolBuilder::new().num_threads(1).build().unwrap();
        for n in [100_000usize, 2_000_000] {
            let source: Vec<Vec3> = (0..n).map(|i| Vec3::new(i as f64, (i % 7) as f64, 0.5)).collect();
            let transform = |v: Vec3| Vec3::new(v.x * 0.8, v.y * 1.1, v.z * 1.1);
            let mut separate = Vec::new();
            let mut fused = Vec::new();
            for round in 0..6 {
                let mut a = Mesh { vertices: source.clone(), faces: vec![] };
                let mut b = source.clone();
                let (t0, t1) = pool.install(|| {
                    let start = Instant::now();
                    map_vertices(&mut a.vertices, transform);
                    black_box(mesh_centroid(&a));
                    let t0 = start.elapsed().as_secs_f64();
                    let start = Instant::now();
                    black_box(map_vertices_centroid(&mut b, transform));
                    (t0, start.elapsed().as_secs_f64())
                });
                if round > 0 {
                    separate.push(t0);
                    fused.push(t1);
                }
            }
            separate.sort_by(f64::total_cmp);
            fused.sort_by(f64::total_cmp);
            println!("fused_centroid_benchmark n={n} separate_median={:.6} fused_median={:.6} separate={separate:?} fused={fused:?}", separate[2], fused[2]);
        }
    }
}
