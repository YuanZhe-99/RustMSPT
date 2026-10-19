//! S8's local PLC pass (plan M-3.3): every lattice face the surface crosses is given its trace and
//! triangulated once, and every cell whose faces that changed, or that escalated, is offered §7.4's
//! constrained tetrahedralisation (`plc_attempt`) - with the facet-split fan where it declines. It
//! fills `plc` (what each cell takes), the promised face triangulations and boundaries the
//! post-assembly checks read, and the per-cell refusal. Moved out of `cut_lattice` unchanged.

use super::*;

// AI-FUNC-SUMMARY:
// Purpose: The §7.1/§7.4 pass of S8: trace interning on every crossed lattice face, the augmented
//   face triangulations, the exclusion fixed point, and the per-cell PLC attempt with its fallback.
// Inputs: the cut mesh and its node keys/origins (interned into), the lattice, the snap result,
//   the classifier, the cut options, every component, the per-cell table results, the global cut
//   and second-cut node maps, the on-cut set, the curve pierce points, the two quanta, and the S8
//   lap timer.
// Returns: None.
// Side effects: Interns nodes into `mesh`/`keys`/`origin`; fills `plc`, `plc_face_tris`,
//   `plc_face_owners_n`, `plc_boundary`, `plc_edge_points`, `plc_decline` and `trace_faces`; pushes
//   warnings; prints `[PLC-PASS]` lines.
// Notes: Byte-for-byte the block it replaced in `cut_lattice`; the order of every interning and
//   every serial step is unchanged, so R-P2 and the matrix are unaffected.
#[allow(clippy::too_many_arguments)]
pub(super) fn plc_pass(
    mesh: &mut CutMesh,
    keys: &mut Vec<NodeKey>,
    origin: &mut Vec<&'static str>,
    trace_faces: &mut BTreeMap<u32, BTreeSet<[u32; 3]>>,
    plc: &mut [Option<PlcPlan>],
    plc_face_tris: &mut BTreeMap<[u32; 3], Vec<[u32; 3]>>,
    plc_face_owners_n: &mut BTreeMap<[u32; 3], usize>,
    plc_boundary: &mut Vec<Option<Vec<[u32; 3]>>>,
    plc_edge_points: &mut BTreeMap<[u32; 2], Vec<u32>>,
    plc_decline: &mut [Option<&'static str>],
    lattice: &Lattice,
    snapped: &Snapped,
    classifier: &PointClassifier,
    options: &CutOptions,
    all_components: &[i32],
    per_cell: &[CellResult],
    cut_index: &BTreeMap<([u32; 2], i32), u32>,
    second_index: &BTreeMap<([u32; 2], i32), u32>,
    on_cut: &BTreeMap<(u32, i32), ()>,
    curve_pierce: &BTreeMap<[u32; 3], Vec3>,
    order_quantum: f64,
    quantum: f64,
    s8_lap: &mut dyn FnMut(&str),
) {
    // The arena quantum has to be the one the node keys were built with, or a point that
    // coincides with an existing node interns as a second node at the same place.
    let tol = order_quantum;
    // Every lattice face the surface crosses, with its trace, and every crossing node already
    // on it. Both are functions of the face alone - which is what lets the two cells sharing it
    // derive the same triangulation without communicating (invariant J1).
    let mut chords_of: BTreeMap<[u32; 3], SmallVec<[[Vec3; 2]; 4]>> = BTreeMap::new();
    let mut face_nodes: BTreeMap<[u32; 3], SmallVec<[u32; 4]>> = BTreeMap::new();
    let mut owners: BTreeMap<[u32; 3], SmallVec<[u32; 2]>> = BTreeMap::new();
    let mut edge_nodes: BTreeMap<[u32; 2], SmallVec<[u32; 6]>> = BTreeMap::new();
    s8_lap("plc-setup");
    let trace_probe: Option<Vec3> = std::env::var("RUSTMSPT_TRACE_PROBE").ok().and_then(|v| {
        let c: Vec<f64> = v.split(',').filter_map(|x| x.parse().ok()).collect();
        (c.len() == 3).then(|| Vec3::new(c[0], c[1], c[2]))
    });
    // Each lattice face once, in first-visit order; the trace of each is a pure function of
    // the face and the surface, so it is computed in parallel and consumed in that order.
    let mut unique_faces: Vec<([u32; 3], [u32; 3])> = Vec::new();
    for (index, tet) in lattice.tets.iter().enumerate() {
        for slots in TET_FACES {
            let raw = [tet[slots[0]], tet[slots[1]], tet[slots[2]]];
            let mut face = raw;
            face.sort_by_key(|node| keys[*node as usize]);
            match owners.entry(face) {
                std::collections::btree_map::Entry::Vacant(slot) => {
                    slot.insert(SmallVec::from_slice(&[index as u32]));
                    unique_faces.push((face, raw));
                }
                std::collections::btree_map::Entry::Occupied(mut slot) => {
                    if !slot.get().contains(&(index as u32)) {
                        slot.get_mut().push(index as u32);
                    }
                }
            }
        }
    }
    // Sparse: only the faces the surface actually crosses, as (face slot, chords) in slot
    // order. One inline chord list per lattice face cost a9 ~400 MB of peak memory.
    let face_chords: Vec<(usize, SmallVec<[[Vec3; 2]; 4]>)> = unique_faces
        .par_iter()
        .enumerate()
        .filter_map(|(slot, (_, raw))| {
            let geometry = [
                mesh.nodes[raw[0] as usize],
                mesh.nodes[raw[1] as usize],
                mesh.nodes[raw[2] as usize],
            ];
            // **Relative to the face, not absolute.** `trace_on_face` clips the chord to the
            // face within its tolerance, so an absolute one lets an endpoint sit outside a
            // small face by whatever fraction of it that tolerance happens to be: measured at
            // 2.9e-2 of the face - three percent - with `options.eps` on a6a, which is not a
            // rounding artefact and no threshold downstream should be widened to swallow it.
            let shortest = (0..3)
                .map(|slot| {
                    let d = geometry[(slot + 1) % 3].sub(geometry[slot]);
                    d.dot(d).sqrt()
                })
                .fold(f64::INFINITY, f64::min);
            let mut chords: SmallVec<[[Vec3; 2]; 4]> = SmallVec::new();
            for component in all_components {
                // A sheet's trace too (A-11): it owns no volume but the cut runs along it.
                let (lo, hi) = polygon_query_box(&geometry, shortest * 1.0e-9);
                let tris = classifier.polygons_near(*component, lo, hi);
                if tris.is_empty() {
                    continue;
                }
                for chord in crate::meshgen::cdt::trace_on_face(
                    geometry,
                    &tris,
                    shortest * 1.0e-9,
                ) {
                    chords.push(chord);
                }
            }
            (!chords.is_empty()).then_some((slot, chords))
        })
        .collect();
    let mut face_chords = face_chords.into_iter().peekable();
    for (slot, (face, raw)) in unique_faces.into_iter().enumerate() {
        let chords: SmallVec<[[Vec3; 2]; 4]> = match face_chords.peek() {
            Some((at, _)) if *at == slot => face_chords.next().map(|(_, c)| c).unwrap_or_default(),
            _ => SmallVec::new(),
        };
        {
            let geometry = [
                mesh.nodes[raw[0] as usize],
                mesh.nodes[raw[1] as usize],
                mesh.nodes[raw[2] as usize],
            ];
            let mut on_face: SmallVec<[u32; 4]> = SmallVec::new();
            for slot in 0..3 {
                let (x, y) = (raw[slot], raw[(slot + 1) % 3]);
                let key = if x <= y { [x, y] } else { [y, x] };
                // The same crossings kept per EDGE as well. A trace endpoint that coincides
                // with one is that node, and the decision has to be a function of the edge
                // rather than of the face, or the two faces sharing the edge make it
                // differently.
                // Kept only for edges a cut node splits; an edge without one is its two
                // endpoints, which the readers supply (a map entry per lattice edge was
                // hundreds of MB on the large cases).
                let mut cuts: SmallVec<[u32; 4]> = SmallVec::new();
                for component in all_components {
                    for map in [&cut_index, &second_index] {
                        if let Some(node) = map.get(&(key, *component)) {
                            if !on_face.contains(node) {
                                on_face.push(*node);
                            }
                            if !cuts.contains(node) {
                                cuts.push(*node);
                            }
                        }
                    }
                }
                if !cuts.is_empty() {
                    let entry = edge_nodes.entry(key).or_default();
                    if !entry.contains(&key[0]) {
                        entry.push(key[0]);
                    }
                    if !entry.contains(&key[1]) {
                        entry.push(key[1]);
                    }
                    for node in cuts {
                        if !entry.contains(&node) {
                            entry.push(node);
                        }
                    }
                }
            }
            if let Some(probe) = trace_probe {
                let n = geometry[1].sub(geometry[0]).cross(geometry[2].sub(geometry[0]));
                let nl = n.dot(n).sqrt();
                let off = (probe.sub(geometry[0]).dot(n) / nl).abs();
                let near = (0..3).all(|k| {
                    let a = geometry[k];
                    let b = geometry[(k + 1) % 3];
                    let c = geometry[(k + 2) % 3];
                    let e = b.sub(a);
                    let side = e.cross(probe.sub(a)).dot(n);
                    let other = e.cross(c.sub(a)).dot(n);
                    side * other >= -1.0e-12 * nl * nl
                });
                if off <= 1.0e-9 && near {
                    let kinds: Vec<(u32, u8, bool)> = raw
                        .iter()
                        .map(|v| {
                            (
                                *v,
                                snapped.constraint_kind.get(*v as usize).copied().unwrap_or(9),
                                on_cut.keys().any(|(node, _)| *node == *v),
                            )
                        })
                        .collect();
                    eprintln!(
                        "[TRACE-PROBE] face {:?} corners {:?} kinds(id,kind,on_cut) {:?} chords {:?} edge_nodes {:?}",
                        raw, geometry, kinds, chords, on_face
                    );
                }
            }
            if !chords.is_empty() {
                chords_of.insert(face, chords);
            }
            if !on_face.is_empty() {
                face_nodes.insert(face, on_face);
            }
        }
    }

    s8_lap("plc-face-scan");
    let mut global: BTreeMap<NodeKey, u32> = BTreeMap::new();
    for (id, key) in keys.iter().enumerate() {
        global.entry(*key).or_insert(id as u32);
    }

    // **A trace point on a lattice EDGE belongs to six cells, not two.** Delivering a face's
    // trace to that face's two owners is right for a point strictly inside it and wrong for one
    // on its boundary: the edge is shared by every cell around it, and the four that never see
    // this face keep the edge whole, leaving the node sitting on their faces with nothing
    // referencing it. That is 1,437 boundary leaks and 7,025 hanging nodes on a6a, and the
    // count was invariant under every other bisection precisely because it depends on the
    // augmented FACE SET rather than on how any cell is meshed (PLAN §6.32).
    //
    // So an edge point is interned per EDGE, exactly as `cut_index` interns a crossing, and
    // every face carrying that edge takes it - including faces the surface never touches.
    let mut chord_id: BTreeMap<NodeKey, u32> = BTreeMap::new();
    let mut rim_decade = [0usize; 16];
    // Print-only: for every trace endpoint the snap did NOT reuse, how far the nearest
    // candidate actually was. A bar at 1e-8 means the bound refused a node it could see; a bar
    // at 1e-2 means the candidate set never held one.
    let mut nearest_decade = [0usize; 16];
    let mut probe_decade = [0usize; 16];
    let mut second_stage = 0usize;
    let mut edge_points: BTreeMap<[u32; 2], SmallVec<[u32; 4]>> = BTreeMap::new();
    let mut face_interior: BTreeMap<[u32; 3], SmallVec<[u32; 4]>> = BTreeMap::new();
    // **The candidate set the snap was missing, and it is a publication and not a bound.**
    // Measured (PLAN §6.62): for every trace endpoint the snap declines to reuse, the nearest
    // candidate it can see is 1e-1 to 1e-3 away and never closer - so the rule was never
    // refusing a node, it was never offered one. 296 of a3's 312 trace/trace duplicate pairs
    // are two faces meeting at a lattice edge, each cutting the input's feature edge with its
    // own plane and landing one float32 ULP apart; both endpoints are strictly INSIDE their own
    // face, so each goes into its own `face_interior` and neither is published anywhere the
    // other can look.
    //
    // Each interior point is published against the ONE edge of its face it is nearest to. That
    // is a total function of the point and the face - no threshold decides it - and it is what
    // keeps this from repeating the failure of publishing everything within `quantum` of an
    // edge, which put thousands of legitimately distinct points in one bucket and took a3's
    // traced cells from 16,883 to 190.
    // **And these candidates are judged at `[V2]`'s bound, not at `eps`.** That is the whole
    // difference between this and §6.63: publishing and then snapping at `quantum` takes a3's
    // traced cells from 16,883 to 190, because `eps` is the width of a whole feature here.
    // `1e-6 x diagonal` is the tolerance the contract itself calls coincident and the config's
    // `coincidence: merge` already names - a hundred times tighter - and the measured near-edge
    // distances put 290 points inside it against some 24,000 inside `eps`.
    let coincident = {
        let (mut lo, mut hi) = (
            Vec3::new(f64::INFINITY, f64::INFINITY, f64::INFINITY),
            Vec3::new(f64::NEG_INFINITY, f64::NEG_INFINITY, f64::NEG_INFINITY),
        );
        for p in &mesh.nodes {
            lo = Vec3::new(lo.x.min(p.x), lo.y.min(p.y), lo.z.min(p.z));
            hi = Vec3::new(hi.x.max(p.x), hi.y.max(p.y), hi.z.max(p.z));
        }
        let span = hi.sub(lo);
        1.0e-6 * span.dot(span).sqrt()
    };
    let mut near_edge: BTreeMap<[u32; 2], SmallVec<[u32; 4]>> = BTreeMap::new();
    s8_lap("plc-global");
    for (face, chords) in &chords_of {
        // Copied out before the loop: interning below takes `mesh.nodes` mutably.
        let corners = [
            mesh.nodes[face[0] as usize],
            mesh.nodes[face[1] as usize],
            mesh.nodes[face[2] as usize],
        ];
        let corner = |slot: usize| corners[slot];
        for chord in chords {
            for point in chord {
                // **Which of the face's three edges it lies on, and if it does, it is put
                // exactly there.** A trace endpoint is a clipped chord and lands within
                // rounding of the edge it ends on; leaving it a few ULP off means the face on
                // the other side of that edge reads it as lying OUTSIDE itself and refuses to
                // triangulate - 38 of the 51 faces that failed, each one leaving its cell's
                // boundary open. Projecting onto the edge is a function of the point and the
                // edge, so both faces get the same point and neither can disagree.
                let mut on_edge = None;
                let mut placed = *point;
                // **How far is this endpoint from the nearest rim, in the face's own units?**
                // The `on_edge` test below accepts 1e-9 of the edge's length, and a3's duplicate
                // trace points sit 1.85e-7 apart near a shared edge - so something lands between
                // the two. If the histogram has a GAP, the geometry names the scale; if it is a
                // smear, any bound put here is a threshold being tuned and the cure is elsewhere.
                {
                    let mut nearest = f64::INFINITY;
                    for slot in 0..3 {
                        let (a, b) = (corner(slot), corner((slot + 1) % 3));
                        let along = b.sub(a);
                        let len2 = along.dot(along);
                        if len2 <= 0.0 {
                            continue;
                        }
                        let rel = point.sub(a);
                        let t = (rel.dot(along) / len2).clamp(0.0, 1.0);
                        let off = rel.sub(along.scale(t));
                        nearest = nearest.min(off.dot(off).sqrt() / len2.sqrt());
                    }
                    if nearest.is_finite() {
                        let decade = if nearest <= 0.0 {
                            0
                        } else {
                            ((-nearest.log10()).floor() as i64).clamp(0, 15) as usize
                        };
                        rim_decade[decade] += 1;
                    }
                }
                if let Some((slot, at)) = trace_point_on_edge(*point, corners, coincident) {
                    let (x, y) = (face[slot], face[(slot + 1) % 3]);
                    on_edge = Some(if x <= y { [x, y] } else { [y, x] });
                    placed = at;
                }
                // **A trace point that coincides with a node already there IS that node.**
                // The pass interns on `order_quantum`, which is 1e-6 of the coincidence
                // tolerance the rest of the pipeline merges nodes with; a trace endpoint and
                // the §5.2 crossing at the same place are two computations of one intersection
                // and land a few times 1e-8 apart, so at that quantum they become two nodes at
                // one point. Four of them on a6a's edge (10788, 10795), and the face
                // triangulation then had three collinear points to triangulate: it emitted a
                // zero-area sliver, both faces sharing the edge emitted the SAME sliver, and
                // the cell's boundary listed one triangle twice. That is the whole of a6a's
                // residual - 18 faces carried by four tets, and the holes beside them.
                //
                // The candidates are the nodes on the point's own EDGE, not on its face:
                // restricted that way the decision is a function of the edge, so the two faces
                // sharing it cannot decide differently (invariant J1). `quantum` is
                // `options.eps`, the tolerance `coincidence: merge` already uses - the rule is
                // the pipeline's own, applied where the pass had been ignoring it.
                let mut snapped = None::<u32>;
                let mut nearest_candidate = f64::INFINITY;
                // Print-only: the nodes the near-edge publication makes visible, measured and
                // not yet used. `quantum` cannot be the bound against them - it takes a3's
                // traced cells to 190 - so what decides is whether their distances have a gap,
                // and at what scale.
                let mut probe: SmallVec<[u32; 16]> = SmallVec::new();
                {
                    // The candidates. For a point ON an edge they are that edge's nodes and
                    // nothing else, so the decision is a function of the edge and the two faces
                    // sharing it cannot differ (invariant J1). For a point in the face's
                    // INTERIOR the face's whole node set is fair game - including the points on
                    // its three edges, which is the case that was missing: a trace endpoint
                    // that stops 2e-7 short of an edge is not "on" it by any relative test, yet
                    // it is the same intersection as the edge point already there. Leaving the
                    // pair distinct forced the face triangulation to cover the strip between
                    // them with slivers, and the segment recovery to route an edge past a
                    // vertex it cannot route past - 689 faces refused on a6a (PLAN §6.40).
                    // The interior point belongs to this face alone, so widening its candidates
                    // costs J1 nothing.
                    let mut candidates: SmallVec<[u32; 16]> = SmallVec::new();
                    match &on_edge {
                        Some(edge) => {
                            match edge_nodes.get(edge) {
                                Some(nodes) => candidates.extend(nodes.iter().copied()),
                                None => candidates.extend(edge.iter().copied()),
                            }
                            candidates.extend(
                                edge_points.get(edge).into_iter().flatten().copied(),
                            );
                        }
                        None => {
                            candidates.extend(face.iter().copied());
                            candidates
                                .extend(face_nodes.get(face).into_iter().flatten().copied());
                            candidates
                                .extend(face_interior.get(face).into_iter().flatten().copied());
                            for slot in 0..3 {
                                let (x, y) = (face[slot], face[(slot + 1) % 3]);
                                let key = if x <= y { [x, y] } else { [y, x] };
                                candidates.extend(
                                    edge_points.get(&key).into_iter().flatten().copied(),
                                );
                                probe.extend(near_edge.get(&key).into_iter().flatten().copied());
                            }
                        }
                    }
                    let mut best = quantum;
                    for other in &candidates {
                        let d = mesh.nodes[*other as usize].sub(placed);
                        let d = d.dot(d).sqrt();
                        // Print-only: how close the NEAREST candidate is, whatever the bound
                        // says. A point that ends up with a coincident partner and whose
                        // nearest candidate is far away is one the candidate set never
                        // contained - which is a different defect from a bound being too tight.
                        nearest_candidate = nearest_candidate.min(d);
                        // `<` and not `<=`, plus the id tie-break, so the nearest node wins and
                        // an exact tie resolves the same way on every run.
                        if d < best || (d == best && snapped.is_some_and(|s| *other < s)) {
                            best = d;
                            snapped = Some(*other);
                        }
                    }
                }
                let key = node_key(placed, order_quantum);
                let id = match snapped {
                    Some(id) => id,
                    None => *global.entry(key).or_insert_with(|| {
                        let id = mesh.nodes.len() as u32;
                        mesh.nodes.push(placed);
                        keys.push(key);
                        origin.push("trace-endpoint");
                        id
                    }),
                };
                chord_id.insert(node_key(*point, order_quantum), id);
                trace_faces.entry(id).or_default().insert(*face);
                {
                    let mut nearest = f64::INFINITY;
                    for other in &probe {
                        if *other == id {
                            continue;
                        }
                        let d = mesh.nodes[*other as usize].sub(placed);
                        nearest = nearest.min(d.dot(d).sqrt());
                    }
                    if nearest.is_finite() {
                        let decade = if nearest <= 0.0 {
                            0
                        } else {
                            ((-nearest.log10()).floor() as i64).clamp(0, 15) as usize
                        };
                        probe_decade[decade] += 1;
                    }
                }
                if snapped.is_none() && nearest_candidate.is_finite() {
                    let decade = if nearest_candidate <= 0.0 {
                        0
                    } else {
                        ((-nearest_candidate.log10()).floor() as i64).clamp(0, 15) as usize
                    };
                    nearest_decade[decade] += 1;
                }
                match on_edge {
                    Some(edge) => {
                        let entry = edge_points.entry(edge).or_default();
                        if !entry.contains(&id) {
                            entry.push(id);
                        }
                    }
                    None => {
                        let entry = face_interior.entry(*face).or_default();
                        if !entry.contains(&id) {
                            entry.push(id);
                        }
                        // Published against the nearest of the face's three edges, so the face
                        // on the other side of that edge finds it. Not projected onto it -
                        // projecting MOVES the point, and that is what cost two points of
                        // on-surface area when it was tried (PLAN §6.40).
                        let here = mesh.nodes[id as usize];
                        let mut nearest = (f64::INFINITY, [0u32; 2]);
                        for slot in 0..3 {
                            let (a, b) = (corner(slot), corner((slot + 1) % 3));
                            let along = b.sub(a);
                            let len2 = along.dot(along);
                            if len2 <= 0.0 {
                                continue;
                            }
                            let rel = here.sub(a);
                            let t = (rel.dot(along) / len2).clamp(0.0, 1.0);
                            let off = rel.sub(along.scale(t));
                            let (x, y) = (face[slot], face[(slot + 1) % 3]);
                            let key = if x <= y { [x, y] } else { [y, x] };
                            if off.dot(off) < nearest.0 {
                                nearest = (off.dot(off), key);
                            }
                        }
                        if nearest.0.is_finite() {
                            let entry = near_edge.entry(nearest.1).or_default();
                            if !entry.contains(&id) {
                                entry.push(id);
                            }
                        }
                    }
                }
            }
        }
    }

    println!(
        "[PLC-PASS] trace endpoint distance to the nearest rim, by decade of the face's own \
         edge length (1e0, 1e-1, ... 1e-15): {rim_decade:?}"
    );
    println!(
        "[PLC-PASS] distance to the NEAREST candidate node for every trace endpoint the snap \
         did not reuse, by decade (1e0, 1e-1, ... 1e-15): {nearest_decade:?}"
    );
    println!(
        "[PLC-PASS] distance to the nearest node the NEAR-EDGE publication makes visible, by \
         decade (1e0, 1e-1, ... 1e-15): {probe_decade:?}; the near-edge stage reused a node \
         {second_stage} time(s)"
    );
    // **How many of the interned points are, by the pipeline's own coincidence tolerance,
    // an existing node?** The pass interns on `order_quantum`, which is 1e-6 of `eps` - fine
    // enough to preserve ordering and far too fine to decide identity. A trace endpoint and
    // the §5.2 crossing at the same place are computed by different routes and land microns
    // apart in the last digits; at this quantum they become two nodes.
    {
        let coarse = quantum.max(f64::MIN_POSITIVE);
        let cell = |p: Vec3| {
            (
                (p.x / coarse).floor() as i64,
                (p.y / coarse).floor() as i64,
                (p.z / coarse).floor() as i64,
            )
        };
        let created: Vec<u32> = edge_points
            .values()
            .flatten()
            .chain(face_interior.values().flatten())
            .copied()
            .collect();
        let mut grid: BTreeMap<(i64, i64, i64), Vec<u32>> = BTreeMap::new();
        for (id, point) in mesh.nodes.iter().enumerate() {
            grid.entry(cell(*point)).or_default().push(id as u32);
        }
        let mut collides = 0usize;
        let mut worst = f64::INFINITY;
        let mut decades = [0usize; 12];
        for id in &created {
            let point = mesh.nodes[*id as usize];
            let home = cell(point);
            let mut near = None::<f64>;
            for dx in -1..=1 {
                for dy in -1..=1 {
                    for dz in -1..=1 {
                        let key = (home.0 + dx, home.1 + dy, home.2 + dz);
                        for other in grid.get(&key).into_iter().flatten() {
                            if other == id {
                                continue;
                            }
                            let d = mesh.nodes[*other as usize].sub(point);
                            let d = d.dot(d).sqrt();
                            if d <= coarse {
                                near = Some(near.map_or(d, |n: f64| n.min(d)));
                            }
                        }
                    }
                }
            }
            if let Some(d) = near {
                collides += 1;
                worst = worst.min(d);
                let decade = if d <= 0.0 {
                    0
                } else {
                    ((d.log10().floor() as i64) + 14).clamp(0, 11) as usize
                };
                decades[decade] += 1;
            }
        }
        println!(
            "[PLC-PASS] {collides} of {} interned point(s) sit within eps of another node - \
             the same point by the pipeline's own coincidence rule, two nodes by the pass's \
             ordering quantum; closest pair {:.3e} against eps {:.3e}",
            created.len(),
            worst,
            coarse
        );
        println!(
            "[PLC-PASS]   by separation, 1e-14 up to eps: {decades:?} - a gap in this row is \
             an identity scale the geometry itself names, a smear is a threshold being tuned"
        );
    }

    // Every face's full point set: its corners, the crossings already on it, the points its own
    // trace put inside it, and every edge point on any of its three edges - whoever put them
    // there.
    //
    // A note on what makes a face's point set hard: points that are distinct but nearly
    // collinear triangulate into slivers, and a sliver on the boundary forces a flat TET on
    // whichever cell owns it. The census after `face_tris` measures exactly that.
    s8_lap("plc-edge-points");
    let mut face_ids: BTreeMap<[u32; 3], Vec<u32>> = BTreeMap::new();
    for face in owners.keys() {
        let mut ids: Vec<u32> = face.to_vec();
        for id in face_nodes.get(face).into_iter().flatten() {
            if !ids.contains(id) {
                ids.push(*id);
            }
        }
        for id in face_interior.get(face).into_iter().flatten() {
            if !ids.contains(id) {
                ids.push(*id);
            }
        }
        for slot in 0..3 {
            let (x, y) = (face[slot], face[(slot + 1) % 3]);
            let edge = if x <= y { [x, y] } else { [y, x] };
            for id in edge_points.get(&edge).into_iter().flatten() {
                if !ids.contains(id) {
                    ids.push(*id);
                }
            }
        }
        if ids.len() > 3 || chords_of.contains_key(face) {
            face_ids.insert(*face, ids);
        }
    }

    // **Augmented only where it actually differs from §5.2.** Marking a face on the mere
    // presence of a trace forced every cell the surface grazes onto the fan - 44,324 of a8's
    // declines were "no facet in this cell", a cell with nothing for §7.4 to constrain and
    // every reason to keep taking §6's table.
    // Print-only: `RUSTMSPT_FACE_PROBE=a,b,c` (three lattice corner ids) prints that face's
    // point set, its edges' recorded points and, below, its triangulation.
    let face_probe: Option<BTreeSet<u32>> = std::env::var("RUSTMSPT_FACE_PROBE")
        .ok()
        .map(|v| v.split(',').filter_map(|x| x.trim().parse().ok()).collect());
    if let Some(probe) = &face_probe {
        for (face, ids) in &face_ids {
            if face.iter().copied().collect::<BTreeSet<u32>>() == *probe {
                eprintln!("[FACE-PROBE] face {face:?} ids {ids:?} chords {:?}", chords_of.get(face));
                for slot in 0..3 {
                    let (x, y) = (face[slot], face[(slot + 1) % 3]);
                    let edge = if x <= y { [x, y] } else { [y, x] };
                    eprintln!("[FACE-PROBE]   edge {edge:?} points {:?} nodes {:?}", edge_points.get(&edge), edge_nodes.get(&edge));
                }
            }
        }
    }
    let mut face_why: BTreeMap<&'static str, usize> = BTreeMap::new();
    let mut failed_faces: BTreeSet<[u32; 3]> = BTreeSet::new();
    let mut face_outside_worst = 0.0f64;
    let mut face_tris: BTreeMap<[u32; 3], Vec<[u32; 3]>> = BTreeMap::new();
    let mut trace: BTreeMap<[u32; 3], SmallVec<[[Vec3; 2]; 4]>> = BTreeMap::new();
    s8_lap("plc-face-ids");
    for (face, ids) in &face_ids {
        let raw = *face;
        let geometry = [
            mesh.nodes[raw[0] as usize],
            mesh.nodes[raw[1] as usize],
            mesh.nodes[raw[2] as usize],
        ];
        let mut segments: Vec<[u32; 2]> = Vec::new();
        for chord in chords_of.get(face).into_iter().flatten() {
            let mut ends = [0u32; 2];
            let mut whole = true;
            for (slot, point) in chord.iter().enumerate() {
                // The endpoint may have been snapped onto an edge when it was interned, so the
                // raw point's key need not find it; `chord_id` records where each one went.
                // **A missing entry means the point was DROPPED**, because it would have split
                // a lattice edge §6's cut has no node on. Aiming the chord at the face's first
                // corner instead - which is what this did - invents a constraint the surface
                // does not have; the chord is simply not expressible on this face.
                match chord_id.get(&node_key(*point, order_quantum)) {
                    Some(id) => ends[slot] = *id,
                    None => whole = false,
                }
            }
            if whole && ends[0] != ends[1] {
                segments.push(ends);
            }
        }
        let points: Vec<Vec3> = ids.iter().map(|id| mesh.nodes[*id as usize]).collect();
        let face_keys: Vec<NodeKey> = ids.iter().map(|id| keys[*id as usize]).collect();
        let at = |id: u32| ids.iter().position(|x| *x == id).unwrap_or(0) as u32;
        let local: Vec<[u32; 2]> = segments.iter().map(|s| [at(s[0]), at(s[1])]).collect();
        if face_probe.as_ref().is_some_and(|p| face.iter().all(|c| p.contains(c))) {
            eprintln!("[FACE-INPUT] geometry {:?}", geometry.map(|v| [v.x, v.y, v.z]));
            eprintln!("[FACE-INPUT] points {:?}", points.iter().map(|v| [v.x, v.y, v.z]).collect::<Vec<_>>());
            eprintln!("[FACE-INPUT] segments {:?} eps {:e}", local, options.eps);
        }
        let tris = match crate::meshgen::cdt::constrained_face_triangulation(
            geometry,
            &points,
            &face_keys,
            &local,
            options.eps,
        ) {
            Ok(tris) => {
                if face_probe.as_ref().is_some_and(|p| face.iter().all(|c| p.contains(c))) {
                    eprintln!("[FACE-PROBE] face {face:?} triangulated {:?}", tris.iter().map(|t| t.map(|i| ids[i as usize])).collect::<Vec<_>>());
                }
                tris
            }
            Err(reason) => {
                if face_probe.as_ref().is_some_and(|p| face.iter().all(|c| p.contains(c))) {
                    eprintln!("[FACE-PROBE] face {face:?} FAILED: {reason}");
                }
                *face_why.entry(reason).or_insert(0) += 1;
                failed_faces.insert(raw);
                if reason == "a point lies outside the face" {
                    // How far outside, in the face's own barycentric units - the only scale
                    // where "outside" means anything. A few ULP is a rounding artefact and a
                    // threshold fixes it; anything larger is a point that has no business on
                    // this face and a threshold would only hide it.
                    let axis = crate::meshgen::predicates::best_projection_axis(
                        geometry[0], geometry[1], geometry[2],
                    );
                    let outward = crate::meshgen::predicates::orient2d_axis(
                        geometry[0], geometry[1], geometry[2], axis,
                    );
                    let mut worst = 0.0f64;
                    for point in &points {
                        for slot in 0..3 {
                            let side = crate::meshgen::predicates::orient2d_axis(
                                geometry[slot],
                                geometry[(slot + 1) % 3],
                                *point,
                                axis,
                            );
                            worst = worst.min(side / outward).min(0.0);
                        }
                    }
                    face_outside_worst = face_outside_worst.min(worst);
                }
                continue;
            }
        };
        if std::env::var_os("RUSTMSPT_PLC_DUMP").is_some() {
            let mut edges: BTreeMap<[u32; 2], usize> = BTreeMap::new();
            for triangle in &tris {
                for slot in 0..3 {
                    let (x, y) = (triangle[slot], triangle[(slot + 1) % 3]);
                    *edges.entry(if x <= y { [x, y] } else { [y, x] }).or_insert(0) += 1;
                }
            }
            let bad = edges.iter().any(|(edge, count)| {
                let on_rim = (0..3).any(|slot| {
                    let (p, q) = (geometry[slot], geometry[(slot + 1) % 3]);
                    let along = q.sub(p);
                    let len2 = along.dot(along);
                    len2 > 0.0
                        && edge.iter().all(|node| {
                            let rel = points[*node as usize].sub(p);
                            let t = rel.dot(along) / len2;
                            let off = rel.sub(along.scale(t));
                            off.dot(off) <= 1.0e-18 * len2
                                && (-1.0e-9..=1.0 + 1.0e-9).contains(&t)
                        })
                });
                *count != usize::from(!on_rim) + 1
            });
            if bad {
                static SHOWN: std::sync::atomic::AtomicUsize =
                    std::sync::atomic::AtomicUsize::new(0);
                if SHOWN.fetch_add(1, std::sync::atomic::Ordering::Relaxed) < 2 {
                    println!("[PLC-DUMP] face {:?}", geometry.map(|p| (p.x, p.y, p.z)));
                    for (slot, p) in points.iter().enumerate() {
                        println!("[PLC-DUMP]   point {slot} = {:?}", (p.x, p.y, p.z));
                    }
                    println!("[PLC-DUMP]   segments {local:?}");
                    println!("[PLC-DUMP]   tris {tris:?}");
                }
            }
        }
        let mine: Vec<[u32; 3]> = tris
            .iter()
            .map(|t| [ids[t[0] as usize], ids[t[1] as usize], ids[t[2] as usize]])
            .collect();
        let frozen = frozen_face_tris(
            raw,
            &cut_index,
            &second_index,
            &on_cut,
            &all_components,
            &keys,
        );
        let canon = |tris: &[[u32; 3]]| -> Vec<[u32; 3]> {
            let mut out: Vec<[u32; 3]> = tris
                .iter()
                .map(|t| {
                    let mut k = *t;
                    k.sort_unstable();
                    k
                })
                .collect();
            out.sort_unstable();
            out.dedup();
            out
        };
        // Measured and refuted: augmenting only where §5.2's node SET is inadequate, rather
        // than wherever the triangulations differ, cuts the traced population 13x (a6a 36,256
        // -> 2,759) and makes the off-surface area WORSE on both cases - a6a's away-from-curve
        // 1.30e-2 -> 1.57e-2, a3's 1.61e-1 -> 2.55e-1. So the damage is not simply proportional
        // to how many cells §7.4 is handed, and narrowing the triage is not the lever
        // (PLAN §6.45).
        let differs = match &frozen {
            Some(frozen) => canon(&mine) != canon(frozen),
            None => true,
        };
        if differs {
            face_tris.insert(raw, mine);
            if let Some(chords) = chords_of.get(face) {
                trace.insert(raw, chords.clone());
            }
        }
    }

    // A face that carries points §5.2 has not got and could not be triangulated is the one hole
    // left in the argument: its owners read §5.2 for it and miss those points.
    // **Is every point the pass puts on a face actually ON it?** The cell's region is a
    // convex tet, so `delaunay_tets` puts on its hull exactly the points that lie on the tet's
    // surface. A trace point a hair INSIDE becomes an interior point instead, the hull keeps
    // the bare face, and the prescribed boundary cannot be matched at all - which would be a
    // structural mismatch rather than the combinatorial one flip recovery addresses. Measured
    // in the face's own units, as §6.34 had to learn to do.
    {
        let mut worst_off = 0.0f64;
        let mut off_decades = [0usize; 8];
        let mut total = 0usize;
        for (face, ids) in &face_interior {
            let corners = [
                mesh.nodes[face[0] as usize],
                mesh.nodes[face[1] as usize],
                mesh.nodes[face[2] as usize],
            ];
            let normal = corners[1].sub(corners[0]).cross(corners[2].sub(corners[0]));
            let length = normal.dot(normal).sqrt();
            if length <= 0.0 {
                continue;
            }
            let normal = normal.scale(1.0 / length);
            let shortest = (0..3)
                .map(|slot| {
                    let d = corners[(slot + 1) % 3].sub(corners[slot]);
                    d.dot(d).sqrt()
                })
                .fold(f64::INFINITY, f64::min);
            for id in ids {
                total += 1;
                let off = normal.dot(mesh.nodes[*id as usize].sub(corners[0])).abs() / shortest;
                worst_off = worst_off.max(off);
                let decade = if off <= 0.0 {
                    0
                } else {
                    ((off.log10().floor() as i64) + 16).clamp(0, 7) as usize
                };
                off_decades[decade] += 1;
            }
        }
        println!(
            "[PLC-PASS] {total} face-interior point(s); off the face's own plane by at worst \
             {worst_off:.3e} of its shortest edge, by decade from 1e-16: {off_decades:?} - a \
             point off the plane is not on the convex hull, so the prescribed boundary cannot \
             be matched however the interior is flipped"
        );
    }

    // **Is each face's triangulation a triangulation?** Every edge strictly inside the face
    // must be shared by exactly two of its triangles and every edge on its rim by exactly one.
    // A face that fails this hands both its owners an open boundary, and the fan then cones the
    // hole - which is 1,258 leaks on a3 (PLAN §6.39). Checked on `face_tris` alone, so a
    // failure here names `constrained_face_triangulation` and clears §5.2's table, or the
    // reverse.
    {
        let (mut broken, mut worst_edges) = (0usize, 0usize);
        for (face, tris) in &face_tris {
            let mut edges: BTreeMap<[u32; 2], usize> = BTreeMap::new();
            for triangle in tris {
                for slot in 0..3 {
                    let (x, y) = (triangle[slot], triangle[(slot + 1) % 3]);
                    *edges.entry(if x <= y { [x, y] } else { [y, x] }).or_insert(0) += 1;
                }
            }
            let corners = [
                mesh.nodes[face[0] as usize],
                mesh.nodes[face[1] as usize],
                mesh.nodes[face[2] as usize],
            ];
            let mut bad = 0usize;
            for (edge, count) in &edges {
                // On the face's rim if both ends lie on one of its three sides.
                let on_rim = (0..3).any(|slot| {
                    let (p, q) = (corners[slot], corners[(slot + 1) % 3]);
                    let along = q.sub(p);
                    let len2 = along.dot(along);
                    len2 > 0.0
                        && edge.iter().all(|node| {
                            let rel = mesh.nodes[*node as usize].sub(p);
                            let t = rel.dot(along) / len2;
                            let off = rel.sub(along.scale(t));
                            off.dot(off) <= 1.0e-18 * len2
                                && (-1.0e-9..=1.0 + 1.0e-9).contains(&t)
                        })
                });
                if *count != usize::from(!on_rim) + 1 {
                    bad += 1;
                }
            }
            if bad > 0 {
                broken += 1;
                worst_edges = worst_edges.max(bad);
            }
        }
        println!(
            "[PLC-PASS] {broken} of {} augmented face(s) are not a triangulation of their face \
             - an edge inside one carried by other than two triangles, or a rim edge by other \
             than one; worst {worst_edges} such edge(s) on one face",
            face_tris.len()
        );
    }

    // How flat the emitted face triangles are, in the face's own units. A tet cannot be
    // rounder than the boundary triangle it stands on, so a sliver here is a flat element
    // there - which is what `[V1]` reads as a signed volume of +-0.
    {
        let mut slivers = 0usize;
        let mut total = 0usize;
        let mut worst = f64::INFINITY;
        for tris in face_tris.values() {
            for triangle in tris {
                total += 1;
                let p = [
                    mesh.nodes[triangle[0] as usize],
                    mesh.nodes[triangle[1] as usize],
                    mesh.nodes[triangle[2] as usize],
                ];
                let cross = p[1].sub(p[0]).cross(p[2].sub(p[0]));
                let area = cross.dot(cross).sqrt() * 0.5;
                let longest = (0..3)
                    .map(|slot| {
                        let d = p[(slot + 1) % 3].sub(p[slot]);
                        d.dot(d).sqrt()
                    })
                    .fold(0.0f64, f64::max);
                let shape = area / longest.powi(2).max(f64::MIN_POSITIVE);
                if shape < 1.0e-6 {
                    slivers += 1;
                    worst = worst.min(shape);
                }
            }
        }
        println!(
            "[PLC-PASS] {slivers} of {total} emitted face triangle(s) are slivers \
             (area/longest^2 < 1e-6), worst {worst:.3e} - a tet cannot be rounder than the \
             boundary triangle it stands on"
        );
    }

    // Faces that FAILED to triangulate - not merely faces `face_tris` has no entry for. A face
    // whose triangulation came out equal to §5.2's is deliberately left out of `face_tris`, and
    // counting those as unusable reported 8,868 broken faces on a6a in a run where every face
    // triangulated. A measured row that says the opposite of the truth is worse than no row.
    s8_lap("plc-face-tris");
    let unusable = failed_faces.len();
    if unusable > 0 {
        println!(
            "[PLC-PASS] {unusable} face(s) of {} carry points §5.2 has not got and could not \
             be triangulated - their owners read §5.2 for them and miss those points, which is \
             a hanging node each",
            face_ids.len()
        );
        for (reason, count) in &face_why {
            println!("[PLC-PASS]   face declined {count}: {reason}");
        }
        println!(
            "[PLC-PASS]   worst point-outside-face, in barycentric units: {face_outside_worst:.3e}"
        );
    }

    // Pass A, in parallel: each cell answered from its own geometry and the shared faces.
    let has_augmented_face = |tet: &[u32; 4]| -> bool {
        TET_FACES.iter().any(|slots| {
            let mut face = [tet[slots[0]], tet[slots[1]], tet[slots[2]]];
            face.sort_by_key(|node| keys[*node as usize]);
            face_tris.contains_key(&face)
        })
    };
    // **An escalated cell is offered §7.4 whether or not a trace augmented one of its faces**
    // (§7.1 rev 1.5: every cell the surface crosses is offered §7.2 first). Gating on an
    // augmented face made the offer depend on accidents of the input file: an STL rectangle's
    // diagonal pierced a lattice face and pulled its cells in, and merging coplanar facets
    // (M-5.2) took that away and sent 15 of a3's cells back to §7.6, 11 of them to its fan.
    let escalates = |index: usize| per_cell[index].escalation.is_some();
    let offered = |index: usize, tet: &[u32; 4]| has_augmented_face(tet) || escalates(index);
    let cell_boundary = |tet: &[u32; 4], force: bool| -> Option<Vec<[u32; 3]>> {
        // **The un-augmented faces come from `face_states`, the very function §6 and §7.6 use.**
        // Not augmented does not mean bare: §5.2 splits a face wherever its edges carry
        // crossings, and handing this cell the whole triangle presents a face its neighbour has
        // split in two - 75,797 boundary leaks on the first emitting run, every one of them on
        // a face §7.4 left alone. Reconstructing §5.2 by hand got most of the way and still
        // differed on second crossings and rims, so the pipeline's own function does it.
        let states = face_states(
            *tet,
            &cut_index,
            &second_index,
            &on_cut,
            &all_components,
            &keys,
            &mesh.nodes,
            &curve_pierce,
        );
        let mut out: Vec<[u32; 3]> = Vec::new();
        let mut any = false;
        for (slot, slots) in TET_FACES.iter().enumerate() {
            let mut face = [tet[slots[0]], tet[slots[1]], tet[slots[2]]];
            face.sort_by_key(|node| keys[*node as usize]);
            if let Some(tris) = face_tris.get(&face) {
                any = true;
                out.extend(tris.iter().copied());
                continue;
            }
            let (state, _, _) = &states[slot];
            let Some(state) = state.as_ref() else {
                // A face the frozen table does not express, and not one the trace changed
                // either - this cell is not one §7.4 can be handed.
                return None;
            };
            let Some(tris) = face_split(state, &keys) else {
                return None;
            };
            out.extend(tris.iter().copied());
        }
        (any || force).then_some(out)
    };
    // **The 117 faces that could not be triangulated poison their owners, and the poison
    // spreads exactly one way.** A cell that cannot build a boundary takes §6's table, whose
    // faces are §5.2's - so any neighbour of it across an AUGMENTED face would be presenting a
    // triangulation that cell does not have. The neighbour therefore cannot take §7.4 either,
    // and so on. Iterated to a fixed point, which is affordable here precisely because the seed
    // is 117 faces rather than the whole traced shell (contrast §6.30, where seeding the same
    // iteration with 12 % of cells erased all of them).
    //
    // **And a cell that will still ESCALATE is not a neighbour §7.4 can share a face with.**
    // The escalated path does not use §5.2's chord on every face: P-3.3's crease fan
    // triangulates a curve-pierced face from the pierce point instead, and the FaceTriCache
    // makes that authoritative for both its owners. A §7.4 cell next door, reading §5.2 for
    // the same face, would present a different split - which is a crack, and was 1,437
    // boundary leaks on a6a. A cell that would have escalated but carries an augmented face is
    // not in this set: it takes §7.4 instead of escalating, so there is nothing to disagree
    // with.
    // **Where the exclusions come from**, because the three clauses want different work and a8
    // has 46,596 of them against zero on every other case.
    s8_lap("plc-boundary-fns");
    let seed_no_boundary = std::sync::atomic::AtomicUsize::new(0);
    let seed_failed_face = std::sync::atomic::AtomicUsize::new(0);
    let seed_curve_pierce = std::sync::atomic::AtomicUsize::new(0);
    let mut excluded: Vec<bool> = lattice
        .tets
        .par_iter()
        .enumerate()
        .map(|(index, tet)| {
            if !offered(index, tet) {
                return false;
            }
            if cell_boundary(tet, escalates(index)).is_none() {
                seed_no_boundary.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                return true;
            }
            // **A face that could not be triangulated leaves this cell's boundary OPEN.** It
            // carries points §5.2 has not got, so the cell reads §5.2 for it and keeps an edge
            // whole that the neighbouring face has split - and a boundary whose edges are not
            // each shared by two triangles is not a closed surface. `constrained_tets` catches
            // that and declines, which sends the cell to the fan, and the fan cones an open
            // surface into a cell with holes in it: every one of a6a's 464 leaks came from the
            // fanned arm and none from any other (PLAN §6.33).
            if TET_FACES.iter().any(|slots| {
                let mut face = [tet[slots[0]], tet[slots[1]], tet[slots[2]]];
                face.sort_by_key(|node| keys[*node as usize]);
                failed_faces.contains(&face)
            }) {
                seed_failed_face.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                return true;
            }
            // **Kept, and measured inert.** Removing this clause and nothing else left a8
            // bit-identical - 85,537 traced cells, 8,930 T-junctions, 2,268 leaks - so it is not
            // where a8's exclusions come from; those are the untriangulable-face seed above and
            // its propagation. It guards a real disagreement (P-3.3's crease fan triangulates a
            // curve-pierced face from the pierce point, not by §5.2's chord) and costs nothing
            // measured, so it stays.
            TET_FACES.iter().any(|slots| {
                let mut face = [tet[slots[0]], tet[slots[1]], tet[slots[2]]];
                face.sort_by_key(|node| keys[*node as usize]);
                if !curve_pierce.contains_key(&face) {
                    return false;
                }
                owners
                    .get(&face)
                    .map(|list| {
                        list.iter().any(|other| {
                            *other as usize != index
                                && per_cell[*other as usize].escalation.is_some()
                                && !offered(*other as usize, &lattice.tets[*other as usize])
                        })
                    })
                    .unwrap_or(false)
            })
        })
        .inspect(|hit| {
            if *hit {
                seed_curve_pierce.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            }
        })
        .collect();
    s8_lap("plc-excluded");
    for _round in 0..64 {
        let mut spread = 0usize;
        for (index, tet) in lattice.tets.iter().enumerate() {
            if excluded[index] || !offered(index, tet) {
                continue;
            }
            let poisoned = TET_FACES.iter().any(|slots| {
                let mut face = [tet[slots[0]], tet[slots[1]], tet[slots[2]]];
                face.sort_by_key(|node| keys[*node as usize]);
                if !face_tris.contains_key(&face) {
                    return false;
                }
                owners
                    .get(&face)
                    .map(|list| list.iter().any(|other| excluded[*other as usize]))
                    .unwrap_or(false)
            });
            if poisoned {
                excluded[index] = true;
                spread += 1;
            }
        }
        if spread == 0 {
            break;
        }
    }
    let poisoned = excluded.iter().filter(|e| **e).count();
    if poisoned > 0 {
        use std::sync::atomic::Ordering::Relaxed;
        let no_boundary = seed_no_boundary.load(Relaxed);
        let failed_face = seed_failed_face.load(Relaxed);
        let pierced = seed_curve_pierce.load(Relaxed).saturating_sub(failed_face + no_boundary);
        println!(
            "[PLC-PASS] {poisoned} cell(s) excluded, of which the seed is {no_boundary} with no \
             boundary at all, {failed_face} owning a face that would not triangulate, \
             {pierced} next to an escalating cell across a curve-pierced face - and the rest \
             are the spread"
        );
    }

    // The boundary first, so a declined cell still has one: it is fanned over exactly the same
    // augmented faces, and that is what keeps it conforming with a neighbour that took §7.4.
    s8_lap("plc-exclusion-spread");
    let boundaries: Vec<Option<Vec<[u32; 3]>>> = lattice
        .tets
        .par_iter()
        .zip(excluded.par_iter())
        .enumerate()
        .map(|(index, (tet, out))| (!out).then(|| cell_boundary(tet, escalates(index))).flatten())
        .collect();
    // Which (node, component) pairs the cut already puts ON that component's surface: every
    // crossing node, and every parent node S7 snapped onto it (invariant K2's promotions).
    // `plc_attempt` adopts these where a facet's rim meets a cell edge (plan M-2.2).
    let mut on_surface: BTreeSet<(u32, i32)> = on_cut.keys().copied().collect();
    for map in [&cut_index, &second_index] {
        for ((_, component), node) in map.iter() {
            on_surface.insert((*node, *component));
        }
    }
    // Print-only: `RUSTMSPT_PLC_CELL=<lattice index>[,<index>...]` turns the cdt diagnostics
    // on for those cells.
    // The lattice hull is the domain box (each axis is stretched to whole cells, plan M-6.2).
    let lattice_hull = lattice.nodes.iter().fold(
        (
            Vec3::new(f64::INFINITY, f64::INFINITY, f64::INFINITY),
            Vec3::new(f64::NEG_INFINITY, f64::NEG_INFINITY, f64::NEG_INFINITY),
        ),
        |(lo, hi), p| {
            (
                Vec3::new(lo.x.min(p.x), lo.y.min(p.y), lo.z.min(p.z)),
                Vec3::new(hi.x.max(p.x), hi.y.max(p.y), hi.z.max(p.z)),
            )
        },
    );
    let focus_cells: BTreeSet<usize> = std::env::var("RUSTMSPT_PLC_CELL")
        .map(|v| v.split(',').filter_map(|s| s.trim().parse().ok()).collect())
        .unwrap_or_default();
    s8_lap("plc-boundaries");
    let attempt: Vec<Option<Result<PlcCell, &'static str>>> = lattice
        .tets
        .par_iter()
        .zip(boundaries.par_iter())
        .enumerate()
        .map(|(index, (tet, boundary))| {
            boundary.as_ref().map(|boundary| {
                let focus = focus_cells.contains(&index);
                crate::meshgen::cdt::DIAG_CELL.with(|c| c.set(focus));
                HULL_CELL.with(|c| c.set(index));
                if focus {
                    eprintln!("[PLC-CELL] {index}: corners {:?}", tet);
                }
                let outcome = plc_attempt(
                    *tet,
                    boundary,
                    &mesh.nodes,
                    &all_components,
                    classifier,
                    tol,
                    options.eps,
                    &on_surface,
                    lattice_hull,
                );
                crate::meshgen::cdt::DIAG_CELL.with(|c| c.set(false));
                outcome
            })
        })
        .collect();

    s8_lap("plc-attempt");
    let attempted = attempt.iter().filter(|a| a.is_some()).count();
    let taken = attempt.iter().filter(|a| matches!(a, Some(Ok(_)))).count();
    let tets: usize = attempt
        .iter()
        .flatten()
        .filter_map(|a| a.as_ref().ok())
        .map(|c| c.tets.len())
        .sum();
    let mut why: BTreeMap<&'static str, usize> = BTreeMap::new();
    for outcome in attempt.iter().flatten() {
        if let Err(reason) = outcome {
            *why.entry(reason).or_insert(0) += 1;
        }
    }
    println!(
        "[PLC-PASS] {attempted} traced cell(s) of {}; §7.4 takes {taken} of them and emits \
         {tets} tet(s) for those, {:.2} per cell - the rest keep the augmented faces and are \
         fanned",
        lattice.tets.len(),
        tets as f64 / taken.max(1) as f64
    );
    for (reason, count) in &why {
        println!("[PLC-PASS]   {count} declined: {reason}");
    }
    // **What the fan is actually handed.** The fan cones its boundary and asks nothing of it,
    // so every property the cell ends up with is a property the boundary already had: a
    // triangle listed twice becomes two tets carrying the same face, and an edge with one
    // triangle on it becomes a hole. `constrained_tets` refuses both, which is why the meshed
    // arm is clean and the fanned arm carries all of the residual - the defect is in
    // `cell_boundary`, and this counts it there rather than at the far end.
    // **Does a face-interior point predict the decline?** 2,393 of them against 2,028
    // declines is close enough to be worth asking directly rather than inferring. A point on
    // a lattice EDGE stays on the tet's surface however it rounds - the edge is the
    // intersection of two faces - but a point in a face's INTERIOR is on the hull only if it
    // rounds to the outside of that one plane, and `delaunay_tets` decides that with an exact
    // predicate, which has no rounding level to hide in.
    let mut has_interior_point: Vec<bool> = vec![false; lattice.tets.len()];
    for face in face_interior.keys() {
        for owner in owners.get(face).into_iter().flatten() {
            has_interior_point[*owner as usize] = true;
        }
    }
    let mut table = [[0usize; 2]; 2];
    for (index, outcome) in attempt.iter().enumerate() {
        let Some(outcome) = outcome else { continue };
        table[usize::from(has_interior_point[index])][usize::from(outcome.is_err())] += 1;
    }
    println!(
        "[PLC-PASS] cells by whether any of their faces carries an interior trace point, \
         against whether §7.4 declined: no point {} took / {} declined, has one {} took / {} \
         declined",
        table[0][0], table[0][1], table[1][0], table[1][1]
    );

    let (mut open_on_tet_edge, mut open_in_face) = (0usize, 0usize);
    let mut open_taken = 0usize;
    let mut open_fanned = 0usize;
    let mut dup_taken = 0usize;
    let mut dup_fanned = 0usize;
    for (index, (boundary, outcome)) in
        boundaries.iter().zip(attempt.iter()).enumerate()
    {
        let (Some(boundary), Some(outcome)) = (boundary, outcome) else { continue };
        let mut seen: BTreeSet<[u32; 3]> = BTreeSet::new();
        let mut duplicated = false;
        let mut edges: BTreeMap<[u32; 2], usize> = BTreeMap::new();
        for triangle in boundary {
            let mut key = *triangle;
            key.sort_unstable();
            if !seen.insert(key) {
                duplicated = true;
            }
            for slot in 0..3 {
                let (x, y) = (triangle[slot], triangle[(slot + 1) % 3]);
                *edges.entry(if x <= y { [x, y] } else { [y, x] }).or_insert(0) += 1;
            }
        }
        let open = edges.values().any(|n| *n != 2);
        if open {
            // **Where it opens.** An unpaired edge lying on one of the tet's own six edges
            // means the two faces meeting there disagree about splitting it - a quarrel between
            // `face_tris` and §5.2's `face_split`. An unpaired edge in a face's interior means
            // that one face's triangulation is not a triangulation at all. The two have nothing
            // to do with each other and only the count can say which is the population.
            let tet = lattice.tets[index];
            let corners = [
                mesh.nodes[tet[0] as usize],
                mesh.nodes[tet[1] as usize],
                mesh.nodes[tet[2] as usize],
                mesh.nodes[tet[3] as usize],
            ];
            for (edge, count) in &edges {
                if *count == 2 {
                    continue;
                }
                let on_tet_edge = (0..4).any(|a| {
                    ((a + 1)..4).any(|b| {
                        let (p, q) = (corners[a], corners[b]);
                        let along = q.sub(p);
                        let len2 = along.dot(along);
                        len2 > 0.0
                            && edge.iter().all(|node| {
                                let rel = mesh.nodes[*node as usize].sub(p);
                                let t = rel.dot(along) / len2;
                                let off = rel.sub(along.scale(t));
                                off.dot(off) <= 1.0e-18 * len2
                                    && (-1.0e-9..=1.0 + 1.0e-9).contains(&t)
                            })
                    })
                });
                if on_tet_edge {
                    open_on_tet_edge += 1;
                } else {
                    open_in_face += 1;
                }
            }
        }
        match outcome.is_ok() {
            true => {
                open_taken += usize::from(open);
                dup_taken += usize::from(duplicated);
            }
            false => {
                open_fanned += usize::from(open);
                dup_fanned += usize::from(duplicated);
            }
        }
    }
    if open_taken + open_fanned + dup_taken + dup_fanned > 0 {
        println!(
            "[PLC-PASS] the boundaries themselves: {open_fanned} fanned cell(s) and \
             {open_taken} meshed one(s) were handed a boundary that is not closed, \
             {dup_fanned} fanned and {dup_taken} meshed one(s) a boundary listing some \
             triangle twice; of the unpaired edges {open_on_tet_edge} lie on one of the tet's \
             own edges (two faces disagreeing about a split) and {open_in_face} inside a face \
             (a face triangulation that is not one)"
        );
    }
    // **One row per traced cell, for offline analysis.** Print-only and behind an env var, so
    // it changes nothing that is meshed. The decline reasons partition the population but say
    // nothing about what the declining cells *are*; this pairs each reason with the cell's own
    // geometry so the question "what is left" can be answered from data rather than from the
    // next hypothesis.
    if let Some(path) = std::env::var_os("RUSTMSPT_PLC_CSV") {
        let mut rows = String::from(
            "index,reason,short_edge,long_edge,shape,aug_faces,boundary_tris,trace_pts,                 components,fragment_tris,curve,rim_pts,rim_worst\n",
        );
        for (index, outcome) in attempt.iter().enumerate() {
            let Some(outcome) = outcome else { continue };
            let tet = lattice.tets[index];
            let p = [
                mesh.nodes[tet[0] as usize],
                mesh.nodes[tet[1] as usize],
                mesh.nodes[tet[2] as usize],
                mesh.nodes[tet[3] as usize],
            ];
            let (mut short, mut long) = (f64::INFINITY, 0.0f64);
            for a in 0..4 {
                for b in (a + 1)..4 {
                    let d = p[b].sub(p[a]);
                    let d = d.dot(d).sqrt();
                    short = short.min(d);
                    long = long.max(d);
                }
            }
            let volume = p[1].sub(p[0]).cross(p[2].sub(p[0])).dot(p[3].sub(p[0])).abs() / 6.0;
            let shape = volume / long.powi(3).max(f64::MIN_POSITIVE);
            let mut aug = 0usize;
            let mut curve = 0usize;
            for slots in TET_FACES {
                let mut face = [tet[slots[0]], tet[slots[1]], tet[slots[2]]];
                face.sort_by_key(|node| keys[*node as usize]);
                aug += usize::from(face_tris.contains_key(&face));
                curve += usize::from(curve_pierce.contains_key(&face));
            }
            let boundary = boundaries[index].as_ref();
            let tris = boundary.map(|b| b.len()).unwrap_or(0);
            let mut pts: BTreeSet<u32> = BTreeSet::new();
            for triangle in boundary.into_iter().flatten() {
                pts.extend(triangle.iter().copied());
            }
            let mut fragment = 0usize;
            let mut components = 0usize;
            // The fragment's RIM - its vertices lying on the cell's own surface. Those are the
            // points that end up on the convex hull, and the ones the frozen boundary must
            // already have or the two are not the same point set. For each, the distance to
            // the nearest boundary node, relative to the cell's shortest edge: a few ULP means
            // one point spelled twice, anything larger means the rim really is somewhere else.
            let mut rim = 0usize;
            let mut rim_worst = 0.0f64;
            let corner_of = |q: Vec3| -> f64 {
                pts.iter()
                    .map(|id| {
                        let d = mesh.nodes[*id as usize].sub(q);
                        d.dot(d).sqrt()
                    })
                    .fold(f64::INFINITY, f64::min)
            };
            // **`fragment_facets_in_cell`, which is what `plc_attempt` actually interns.**
            // `fragment_in_cell` hands back the ORIGINAL triangles - its own note says so, and
            // its only consumer wants their supporting planes - so measuring the rim from it
            // reads points far outside the cell and means nothing.
            let planes = crate::meshgen::cdt::tet_halfspaces_of(p);
            for component in all_components {
                let Some(slot) = classifier.slot_of(*component) else { continue };
                let facets = crate::meshgen::cdt::fragment_facets_in_cell(
                    p,
                    classifier.triangles_of(slot),
                    short * 1.0e-6,
                );
                if facets.is_empty() {
                    continue;
                }
                components += 1;
                fragment += facets.len();
                for facet in &facets {
                    for q in facet {
                        let on_surface = planes.as_ref().is_some_and(|hs| {
                            hs.iter().any(|(n, d)| (n.dot(*q) - d).abs() <= short * 1.0e-6)
                        });
                        if !on_surface {
                            continue;
                        }
                        rim += 1;
                        rim_worst = rim_worst.max(corner_of(*q) / short);
                    }
                }
            }
            let reason = match outcome {
                Ok(_) => "taken",
                Err(reason) => reason,
            };
            rows.push_str(&format!(
                "{index},\"{reason}\",{short:.6e},{long:.6e},{shape:.6e},{aug},{tris},{},\
                 {components},{fragment},{curve},{rim},{rim_worst:.3e}\n",
                pts.len()
            ));
        }
        if let Err(e) = std::fs::write(&path, rows) {
            println!("[PLC-PASS] could not write the cell census: {e}");
        } else {
            println!("[PLC-PASS] cell census written to {}", path.to_string_lossy());
        }
    }
    // A cell §7.4 declined is recorded here whether the facet-split fan then saved it or not:
    // the census ranks what the KERNEL could not take, and the split fan is the fallback rather
    // than an acceptance.
    for (index, outcome) in attempt.iter().enumerate() {
        match outcome {
            Some(Err(reason)) => plc_decline[index] = Some(reason),
            Some(Ok(cell)) => plc_decline[index] = cell.fanned,
            None => {}
        }
    }
    *plc_boundary = boundaries.clone();
    for (edge, ids) in &edge_points {
        plc_edge_points.insert(*edge, ids.to_vec());
    }
    for (face, tris) in &face_tris {
        plc_face_tris.insert(*face, tris.clone());
        plc_face_owners_n.insert(*face, owners.get(face).map(|o| o.len()).unwrap_or(0));
    }

    // **Out of the per-cell arenas and into the mesh.** A tet's vertices below `seed.len()` are
    // nodes the boundary already had; the rest are the facet's own vertices, private to this
    // cell, and interned now that the cell is known to be kept. Interning them earlier would
    // leave a node behind for every cell that declined, and a node nothing references is what
    // `[V3]` calls a hanging node.
    for (index, outcome) in attempt.into_iter().enumerate() {
        let Some(outcome) = outcome else { continue };
        plc[index] = Some(match outcome {
            Ok(cell) => {
                // **Only the points some tet actually uses are interned.** A cell's arena can end
                // up holding a point nothing references - a facet vertex the recovery welded
                // away, or a piece centroid whose fan tets were all dropped as degenerate - and
                // interning it puts a node in the mesh that no element mentions. `[V2]` calls
                // that an unreferenced node and `[V3]` calls it a hanging node wherever it
                // happens to land on someone's face, which is exactly the 57 `[V3]` reported on
                // a3. The cure is not to hunt every producer of a stray point but to stop
                // publishing points the cell did not use.
                let used: BTreeSet<u32> = cell.tets.iter().flatten().copied().collect();
                let mut map: Vec<u32> = Vec::with_capacity(cell.points.len());
                for (local, point) in cell.points.iter().enumerate() {
                    if local < cell.seed.len() {
                        map.push(cell.seed[local]);
                        continue;
                    }
                    if !used.contains(&(local as u32)) {
                        map.push(u32::MAX);
                        continue;
                    }
                    let key = node_key(*point, order_quantum);
                    let id = *global.entry(key).or_insert_with(|| {
                        let id = mesh.nodes.len() as u32;
                        mesh.nodes.push(*point);
                        keys.push(key);
                        origin.push("plc-arena");
                        id
                    });
                    map.push(id);
                }
                PlcPlan::Meshed {
                    tets: cell
                        .tets
                        .iter()
                        .map(|t| {
                            [
                                map[t[0] as usize],
                                map[t[1] as usize],
                                map[t[2] as usize],
                                map[t[3] as usize],
                            ]
                        })
                        .collect(),
                    regions: cell.regions,
                    caps: cell
                        .caps
                        .iter()
                        .map(|(t, component)| {
                            (
                                [map[t[0] as usize], map[t[1] as usize], map[t[2] as usize]],
                                *component,
                            )
                        })
                        .collect(),
                }
            }
            Err(_) => PlcPlan::Fan {
                boundary: boundaries[index].clone().unwrap_or_default(),
            },
        });
    }
}

// AI-FUNC-SUMMARY:
// Purpose: S8's on-edge rule for a trace endpoint: which of the face's three edges it lies on, and
//   the point placed exactly on that edge.
// Inputs: the trace endpoint, the face's corners (in the face's own corner order), the coincidence
//   tolerance (`[V2]`'s duplicate bound, 1e-6 of the diagonal).
// Returns: Some((slot, placed)) for the first edge slot (corner slot -> slot + 1) the point is on -
//   within 1e-9 of that edge's length or within `coincident` of its line, and inside its span -
//   with the point projected onto it; None for a point in the face's interior.
// Side effects: None.
// Notes: A function of the point and the EDGE, so the two faces sharing an edge place the point at
//   the same place and neither reads it as outside itself (38 of 51 failing faces before it; PLAN
//   §6.35). The coincidence half catches endpoints a few 1e-7 off the rim that the relative test
//   misses; the projection moves a point by at most that.
pub(super) fn trace_point_on_edge(point: Vec3, corners: [Vec3; 3], coincident: f64) -> Option<(usize, Vec3)> {
    for slot in 0..3 {
        let (a, b) = (corners[slot], corners[(slot + 1) % 3]);
        let along = b.sub(a);
        let len2 = along.dot(along);
        if len2 <= 0.0 {
            continue;
        }
        let rel = point.sub(a);
        let t = rel.dot(along) / len2;
        let off = rel.sub(along.scale(t));
        let close = off.dot(off) <= 1.0e-18 * len2 || off.dot(off) <= coincident * coincident;
        if close && (-1.0e-9..=1.0 + 1.0e-9).contains(&t) {
            return Some((slot, a.add(along.scale(t.clamp(0.0, 1.0)))));
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn face() -> [Vec3; 3] {
        [Vec3::new(0.0, 0.0, 0.0), Vec3::new(1.0, 0.0, 0.0), Vec3::new(0.0, 1.0, 0.0)]
    }

    /// A trace endpoint within rounding of an edge is placed exactly on it, and the same point
    /// against the neighbouring face (sharing that edge, corners in another order) lands on the
    /// same spot - the property that keeps the two faces' boundaries from cracking.
    #[test]
    fn a_rim_endpoint_is_placed_on_its_edge_the_same_from_both_faces() {
        let p = Vec3::new(0.3, 1.0e-17, 0.0);
        let (slot, at) = trace_point_on_edge(p, face(), 1e-12).expect("on the edge");
        assert_eq!(slot, 0);
        assert_eq!(at.y, 0.0);
        let other = [Vec3::new(1.0, 0.0, 0.0), Vec3::new(0.0, 0.0, 0.0), Vec3::new(0.5, -1.0, 0.0)];
        let (slot2, at2) = trace_point_on_edge(p, other, 1e-12).expect("on the shared edge");
        assert_eq!(slot2, 0);
        assert!((at2.x - at.x).abs() < 1e-15 && at2.y == 0.0);
    }

    /// The coincidence half: a point 2e-7 off the rim is not on it by the relative test but is by
    /// the coincidence bound, and a point in the interior is never on an edge.
    #[test]
    fn the_coincidence_bound_catches_near_rim_endpoints_and_nothing_else() {
        let near = Vec3::new(0.4, 2.0e-7, 0.0);
        assert!(trace_point_on_edge(near, face(), 0.0).is_none());
        assert_eq!(trace_point_on_edge(near, face(), 1.0e-6).map(|(s, _)| s), Some(0));
        let inside = Vec3::new(0.25, 0.25, 0.0);
        assert!(trace_point_on_edge(inside, face(), 1.0e-6).is_none());
        // Beyond the edge's span is not on it, however close to its line.
        let past = Vec3::new(1.5, 0.0, 0.0);
        assert!(trace_point_on_edge(past, face(), 1.0e-6).map(|(s, _)| s) != Some(0));
    }
}
