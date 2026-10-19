//! S8's assembly (plan M-3.3): every lattice cell's result taken in cell order - §7.4's plan, an
//! escalated cell's split or fan, or §6's table children - with the faces it shares triangulated
//! once through the face cache, §7.5's labels settled, and the interface records queued. Moved out
//! of `cut_lattice` unchanged; it is serial by design (node interning and face Steiner points).

use super::*;

/// What the assembly hands back to `cut_lattice` for the checks and reports after it.
pub(super) struct Assembly {
    pub face_cache: crate::meshgen::facecache::FaceTriCache,
    pub face_cache_conflicts: (usize, Option<String>),
    pub crease_faces: BTreeSet<[u32; 3]>,
    pub crease_faces_expressible: BTreeSet<[u32; 3]>,
    pub crease_faces_both_escalated: BTreeSet<[u32; 3]>,
    pub crease_faces_fanned: BTreeSet<[u32; 3]>,
    pub plc_meshed: usize,
    pub plc_fanned: usize,
    pub plc_split_fanned: usize,
    pub plc_meshed_tets: usize,
    pub plc_split_fanned_tets: usize,
    pub plc_fanned_tets: usize,
    pub fan_repeated: usize,
    pub path_of: Vec<u8>,
    pub seed_uncertain: usize,
    pub seeded_pieces: usize,
    pub hidden_recovered: usize,
    pub spoke_probe: SpokeProbe,
}

// AI-FUNC-SUMMARY:
// Purpose: Assemble S8's output mesh from the per-cell results, in cell order.
// Inputs: the cut mesh with its keys, origins, face Steiner map and pending interfaces (all grown
//   here), §7.4's per-cell plans (taken), the per-cell table results (consumed), and the read-only
//   lattice, snap, classification, classifier, options, components, cut maps, curve and face maps,
//   quanta, probe flag and timer.
// Returns: the counters, census sets and face cache the later checks and reports read.
// Side effects: Pushes tets, records, nodes and interface records into `mesh`; prints the
//   assembly-by-arm timing under `RUSTMSPT_TIME_STAGES`.
// Notes: Byte-for-byte the block it replaced; the order of every step is unchanged (R-P2).
#[allow(clippy::too_many_arguments)]
pub(super) fn assemble(
    mesh: &mut CutMesh,
    keys: &mut Vec<NodeKey>,
    origin: &mut Vec<&'static str>,
    face_steiner: &mut BTreeMap<[u32; 3], u32>,
    pending_interfaces: &mut Vec<(usize, [u32; 3], i32)>,
    plc: &mut [Option<PlcPlan>],
    per_cell: Vec<CellResult>,
    lattice: &Lattice,
    snapped: &Snapped,
    classification: &Classification,
    classifier: &PointClassifier,
    options: &CutOptions,
    all_components: &[i32],
    cut_index: &BTreeMap<([u32; 2], i32), u32>,
    second_index: &BTreeMap<([u32; 2], i32), u32>,
    on_cut: &BTreeMap<(u32, i32), ()>,
    curve_pierce: &BTreeMap<[u32; 3], Vec3>,
    pierced_owners: &BTreeMap<[u32; 3], (u8, u8)>,
    pierce_components: &BTreeMap<[u32; 3], SmallVec<[i32; 2]>>,
    plc_face_owners: &BTreeMap<[u32; 3], (u16, u16)>,
    plc_decline: &[Option<&'static str>],
    region_of_cut: &BTreeMap<u32, i32>,
    component_of_cut: &BTreeMap<u32, i32>,
    curve_nodes: &BTreeSet<u32>,
    n_parent_nodes: u32,
    quantum: f64,
    order_quantum: f64,
    probing_spokes: bool,
    s8_timing: bool,
    s8_lap: &mut dyn FnMut(&str),
) -> Assembly {
    let mut seed_uncertain = 0usize;
    let mut seeded_pieces = 0usize;
    let mut hidden_recovered = 0usize;
    let mut spoke_probe = SpokeProbe::default();
let mut face_cache = crate::meshgen::facecache::FaceTriCache::new();
let mut face_cache_conflicts: (usize, Option<String>) = (0, None);
// P-3.3's census, print-only: how many faces a locked curve pierces are nevertheless
// *expressible*, i.e. take §5.2's table and get a straight chord drawn across the kink.
// That is the crease damage's face-level signature, and its size decides whether the
// face-first route is worth the change. Keyed by face so a shared one counts once.
let mut crease_faces: BTreeSet<[u32; 3]> = BTreeSet::new();
let mut crease_faces_expressible: BTreeSet<[u32; 3]> = BTreeSet::new();
let mut crease_faces_both_escalated: BTreeSet<[u32; 3]> = BTreeSet::new();
let mut crease_faces_fanned: BTreeSet<[u32; 3]> = BTreeSet::new();
let mut plc_meshed = 0usize;
let mut plc_fanned = 0usize;
let mut plc_split_fanned = 0usize;
// **P2 is a gate, not an outcome, so the element cost of each arm is measured beside its cell
// count.** On a1 the same ~1,900 cells cost 9,544 tets under §5.2's table and 39,533 under
// §7.4; a per-cell figure per arm is what says which arm to work on.
let mut plc_meshed_tets = 0usize;
let mut plc_split_fanned_tets = 0usize;
let mut plc_fanned_tets = 0usize;
let mut fan_repeated = 0usize;
// Which path each cell took, so a leak can be charged to one instead of guessed at.
let mut path_of: Vec<u8> = vec![0; lattice.tets.len()];
s8_lap("face-trace");
let mut asm_time = [std::time::Duration::ZERO; 3];
// §7.5's per-region labels of the §7.4-meshed cells read only nodes interned before this
// loop, so they are computed in parallel here and consumed in cell order below.
let n_nodes_before_assembly = mesh.nodes.len();
let mut meshed_records: Vec<Option<(BTreeMap<u32, OwnershipRecord>, usize)>> = plc
    .par_iter()
    .enumerate()
    .map(|(index, plan)| match plan {
        Some(PlcPlan::Meshed { tets, regions, .. })
            if tets
                .iter()
                .flatten()
                .all(|node| (*node as usize) < n_nodes_before_assembly) =>
        {
            let mut uncertain = 0usize;
            let records = region_records(
                tets,
                regions,
                &classification.records[index],
                &all_components,
                &mesh.nodes,
                classifier,
                &mut uncertain,
            );
            Some((records, uncertain))
        }
        _ => None,
    })
    .collect();
for (index, cell) in per_cell.into_iter().enumerate() {
    let asm_clock = std::time::Instant::now();
    // **§7.1 as amended (rev 1.5): a cell the surface's trace changes is meshed by §7.2/§7.4
    // ahead of §6's table.** Both arms keep the augmented faces, which is what lets a cell that
    // took the mesher and a cell that fell back to the fan share one and still meet.
    if let Some(plan) = plc[index].take() {
        let _asm = AsmLap(&mut asm_time[0], asm_clock);
        let parent_record = &classification.records[index];
        match plan {
            PlcPlan::Meshed {
                tets,
                regions,
                caps,
            } => {
                // Tagged against this cell's first tet - the same convention §7.6's path uses.
                // **A cap is tagged as the faces the cell actually emits.** A cap triangle
                // with a node exactly on one of its edges is split there by the tets, and the
                // unsplit triangle then belongs to no element: a3's `(7467, 24483, 36381)`,
                // with 36380 on the edge 7467-36381 once S2 stopped moving constructed points
                // off their facets - four `[V12]` side_elems failures, because the tet that
                // carried it was flat and `orient_positively` drops it below. Only surviving
                // tets count, and such a cap is replaced by their faces lying in its plane and
                // inside it.
                let first_tet = mesh.tets.len();
                let mut emitted_faces: BTreeSet<[u32; 3]> = BTreeSet::new();
                for tet in tets.iter().filter(|t| orient_positively(**t, &mesh.nodes).is_some()) {
                    for slots in [[0usize, 1, 2], [0, 1, 3], [0, 2, 3], [1, 2, 3]] {
                        let mut f = [tet[slots[0]], tet[slots[1]], tet[slots[2]]];
                        f.sort_unstable();
                        emitted_faces.insert(f);
                    }
                }
                for (triangle, component) in caps {
                    let mut key = triangle;
                    key.sort_unstable();
                    if emitted_faces.contains(&key) {
                        pending_interfaces.push((first_tet, triangle, component));
                        continue;
                    }
                    if std::env::var_os("RUSTMSPT_CUT_DIAG").is_some() {
                        eprintln!("[CAP-REPLACED] cell {index} cap {:?}", triangle);
                    }
                    let [a, b, c] = [
                        mesh.nodes[triangle[0] as usize],
                        mesh.nodes[triangle[1] as usize],
                        mesh.nodes[triangle[2] as usize],
                    ];
                    let normal = b.sub(a).cross(c.sub(a));
                    let area2 = normal.dot(normal);
                    if area2 <= 0.0 {
                        continue;
                    }
                    let scale = area2.sqrt();
                    for f in &emitted_faces {
                        let p = [
                            mesh.nodes[f[0] as usize],
                            mesh.nodes[f[1] as usize],
                            mesh.nodes[f[2] as usize],
                        ];
                        let flat = p.iter().all(|q| {
                            (normal.dot(q.sub(a)) / scale).abs() <= scale.sqrt() * 1.0e-9
                        });
                        if !flat {
                            continue;
                        }
                        let centre = p[0].add(p[1]).add(p[2]).scale(1.0 / 3.0);
                        let inside = [(a, b), (b, c), (c, a)].iter().all(|(u, v)| {
                            normal.dot(v.sub(*u).cross(centre.sub(*u))) >= 0.0
                        });
                        if inside {
                            pending_interfaces.push((first_tet, *f, component));
                        }
                    }
                }
                // **§7.5: one classification per sub-region, not per tet.** A region is a
                // connected run of tets no constraint face separates, so every tet in it holds
                // the same material; sampling each one over again would spend classifier
                // queries to re-derive that, and would disagree with itself wherever a
                // centroid lands on the surface.
                // Sampled at each region's LARGEST tet, not its first: a region's first tet
                // can be a sliver along a facet whose centroid lies within rounding of the
                // surface on the wrong side, and one bad sample labels the whole region -
                // reference case 1's cell 17271 lost component 2 around an intersection-curve
                // node that way (`[V9]`). Ties by index keep it deterministic.
                let region_record = match meshed_records[index].take() {
                    Some((records, uncertain)) => {
                        seed_uncertain += uncertain;
                        records
                    }
                    None => region_records(
                        &tets,
                        &regions,
                        parent_record,
                        &all_components,
                        &mesh.nodes,
                        classifier,
                        &mut seed_uncertain,
                    ),
                };
                for (tet, region) in tets.iter().zip(regions.iter()) {
                    let Some(oriented) = orient_positively(*tet, &mesh.nodes) else {
                        mesh.stats.n_degenerate_fan_pieces += 1;
                        continue;
                    };
                    mesh.tets.push(oriented);
                    mesh.records
                        .push(region_record.get(region).cloned().unwrap_or_default());
                    seeded_pieces += 1;
                    mesh.parent_of.push(index as u32);
                    mesh.regime.push(REGIME_NORMAL);
                    mesh.band_region.push(-1);
                }
                // Separated in the census: a cell the kernel meshed and a cell the facet-split
                // fan filled are both conforming and both keep the surface, but only the first
                // is §7.4 doing its job, and the two must not average together.
                if plc_decline[index].is_some() {
                    plc_split_fanned += 1;
                    plc_split_fanned_tets += tets.len();
                    path_of[index] = 4;
                } else {
                    plc_meshed += 1;
                    plc_meshed_tets += tets.len();
                    path_of[index] = 1;
                }
            }
            PlcPlan::Fan { boundary } => {
                // **A triangle the boundary lists TWICE is not a face, and coning it makes two
                // identical tets.** `constrained_tets` refuses such a soup outright, so only the
                // fan is exposed to it - and `[V1]` reads the result as a duplicate cell while
                // `[V3]` reads the faces around it as carried four, eight or ten times. The two
                // copies bound zero volume between them, so dropping BOTH keeps the soup closed
                // (every edge count falls by two) where dropping one would open it. Counted, not
                // silent: the soup should not contain them and this says how often it does.
                let mut times: BTreeMap<[u32; 3], usize> = BTreeMap::new();
                for triangle in &boundary {
                    let mut key = *triangle;
                    key.sort_unstable();
                    *times.entry(key).or_insert(0) += 1;
                }
                let boundary: Vec<[u32; 3]> = boundary
                    .iter()
                    .filter(|triangle| {
                        let mut key = **triangle;
                        key.sort_unstable();
                        times.get(&key).copied().unwrap_or(1) % 2 == 1
                    })
                    .copied()
                    .collect();
                fan_repeated += times.values().filter(|n| **n > 1).count();
                let centroid = polygon_soup_centroid(&boundary, &mesh.nodes);
                let centroid_id = mesh.nodes.len() as u32;
                mesh.nodes.push(centroid);
                keys.push(node_key(centroid, quantum));
                origin.push("cell-fan-centroid");
                let fanned = fan_cell(&boundary, centroid_id);
                for piece in &fanned.tets {
                    let Some(oriented) = orient_positively(*piece, &mesh.nodes) else {
                        mesh.stats.n_degenerate_fan_pieces += 1;
                        continue;
                    };
                    mesh.tets.push(oriented);
                    mesh.records.push(seed_record(
                        parent_record,
                        oriented,
                        &all_components,
                        &mesh.nodes,
                        classifier,
                        &mut seed_uncertain,
                    ));
                    seeded_pieces += 1;
                    mesh.parent_of.push(index as u32);
                    mesh.regime.push(REGIME_NORMAL);
                    mesh.band_region.push(-1);
                }
                plc_fanned += 1;
                plc_fanned_tets += fanned.tets.len();
                path_of[index] = 2;
            }
        }
        continue;
    }
    if let Some(reason) = cell.escalation {
        let _asm = AsmLap(&mut asm_time[1], asm_clock);
        assemble_escalated(
            index,
            reason,
            mesh,
            keys,
            origin,
            face_steiner,
            pending_interfaces,
            &mut face_cache,
            &mut face_cache_conflicts,
            &mut crease_faces,
            &mut crease_faces_expressible,
            &mut crease_faces_both_escalated,
            &mut crease_faces_fanned,
            &mut seed_uncertain,
            &mut seeded_pieces,
            &mut spoke_probe,
            &mut path_of,
            lattice,
            snapped,
            classification,
            classifier,
            options,
            all_components,
            cut_index,
            second_index,
            on_cut,
            curve_pierce,
            pierced_owners,
            pierce_components,
            plc_face_owners,
            region_of_cut,
            component_of_cut,
            curve_nodes,
            n_parent_nodes,
            quantum,
            order_quantum,
            probing_spokes,
        );
        continue;
    }
    let _asm = AsmLap(&mut asm_time[2], asm_clock);
    if cell.tets.len() == 1 && cell.case == 0 {
        mesh.stats.n_uncut_cells += 1;
    } else {
        mesh.stats.n_cut_cells += 1;
        match cell.case {
            b'A' => mesh.stats.n_case_a += 1,
            b'B' | b'b' => mesh.stats.n_case_b += 1,
            b'C' | b'c' => mesh.stats.n_case_c += 1,
            b'D' => mesh.stats.n_case_d += 1,
            b'i' => mesh.stats.n_interior_sampled_cells += 1,
            _ => {}
        }
        if cell.welded {
            mesh.stats.n_welded_sheet_cells += 1;
        }
        mesh.stats.min_dihedral_deg = mesh.stats.min_dihedral_deg.min(cell.min_dihedral);
        mesh.stats.worst_volume_error = mesh.stats.worst_volume_error.max(cell.volume_error);
    }
    let parent_record = &classification.records[index];
    // The §6 table decides the **driving** component's side exactly and `cut_record`
    // inherits every other component's entry verbatim - including an `Outside` S6 read
    // off vertices that were all snapped *onto* that other body's surface. At an edge or
    // a corner that is the ordinary case, and the child then carries background where it
    // holds material: A-6a's 321 and A-7b's 90 remaining misattributed cells, all
    // `provenance = 1`, after the same defect was closed on the §7.6 path.
    //
    // Scoped by `on_cut`, which is the signal that the other body's surface runs through
    // this cell's vertices at all. Without it this would sample every component on every
    // cut tet - millions of classifier queries on A-4 - to change a few hundred.
    let hidden: SmallVec<[i32; 2]> = all_components
        .iter()
        .filter(|component| {
            **component != cell.component
                && Some(**component) != cell.welded_with
                && !parent_record
                    .entries
                    .iter()
                    .any(|(x, side)| x == *component && *side == Side::Inside)
                && lattice.tets[index]
                    .iter()
                    .any(|node| on_cut.contains_key(&(*node, **component)))
        })
        .copied()
        .collect();
    for (tet, inside) in cell.tets.iter().zip(cell.inside.iter()) {
        let mut record = cut_record(
            parent_record,
            cell.component,
            cell.welded_with,
            *inside,
            if cell.welded { 0 } else { cell.case },
        );
        if !hidden.is_empty() {
            let centroid = tet
                .iter()
                .fold(Vec3::new(0.0, 0.0, 0.0), |acc, node| {
                    acc.add(mesh.nodes[*node as usize])
                })
                .scale(0.25);
            for component in &hidden {
                let Some(slot) = classifier.slot_of(*component) else {
                    continue;
                };
                if !classifier.inside(centroid, slot, &mut seed_uncertain) {
                    continue;
                }
                match record.entries.iter_mut().find(|(x, _)| x == component) {
                    Some(entry) => entry.1 = Side::Inside,
                    None => record.entries.push((*component, Side::Inside)),
                }
                hidden_recovered += 1;
            }
            record.entries.sort_by_key(|(x, _)| *x);
        }
        mesh.records.push(record);
        mesh.tets.push(*tet);
        mesh.parent_of.push(index as u32);
        mesh.regime.push(REGIME_NORMAL);
        mesh.band_region.push(-1);
    }
    for triangle in &cell.interface {
        pending_interfaces.push((mesh.tets.len(), *triangle, cell.component));
    }
}
mesh.plc_path = path_of.clone();
mesh.stats.n_tets = mesh.tets.len();
if s8_timing {
    println!("[S8-TIME] assembly by arm: plc {:?} escalated {:?} table {:?}", asm_time[0], asm_time[1], asm_time[2]);
}
    Assembly {
        face_cache,
        face_cache_conflicts,
        crease_faces,
        crease_faces_expressible,
        crease_faces_both_escalated,
        crease_faces_fanned,
        plc_meshed,
        plc_fanned,
        plc_split_fanned,
        plc_meshed_tets,
        plc_split_fanned_tets,
        plc_fanned_tets,
        fan_repeated,
        path_of,
        seed_uncertain,
        seeded_pieces,
        hidden_recovered,
        spoke_probe,
    }
}


// AI-FUNC-SUMMARY:
// Purpose: Assemble one escalated cell: its faces through the face cache (crease hubs, crossed faces,
//   loop fans), then §7.4's plan, the facet-split fan, or the whole-cell fan, with §7.5 labels.
// Inputs: the cell index and escalation reason, the cut state and maps; the counters and caches it updates.
// Returns: None.
// Side effects: Appends tets, nodes, keys and interfaces to the mesh; updates the face cache,
//   face Steiner points, crease sets and counters.
// Notes: Split out of `assemble` unchanged (plan M-3.3).
#[allow(clippy::too_many_arguments)]
fn assemble_escalated(
    index: usize,
    reason: Escalation,
    mesh: &mut CutMesh,
    keys: &mut Vec<NodeKey>,
    origin: &mut Vec<&'static str>,
    face_steiner: &mut BTreeMap<[u32; 3], u32>,
    pending_interfaces: &mut Vec<(usize, [u32; 3], i32)>,
    mut face_cache: &mut crate::meshgen::facecache::FaceTriCache,
    mut face_cache_conflicts: &mut (usize, Option<String>),
    crease_faces: &mut BTreeSet<[u32; 3]>,
    crease_faces_expressible: &mut BTreeSet<[u32; 3]>,
    crease_faces_both_escalated: &mut BTreeSet<[u32; 3]>,
    crease_faces_fanned: &mut BTreeSet<[u32; 3]>,
    mut seed_uncertain: &mut usize,
    seeded_pieces: &mut usize,
    mut spoke_probe: &mut SpokeProbe,
    path_of: &mut [u8],
    lattice: &Lattice,
    snapped: &Snapped,
    classification: &Classification,
    classifier: &PointClassifier,
    options: &CutOptions,
    all_components: &[i32],
    cut_index: &BTreeMap<([u32; 2], i32), u32>,
    second_index: &BTreeMap<([u32; 2], i32), u32>,
    on_cut: &BTreeMap<(u32, i32), ()>,
    curve_pierce: &BTreeMap<[u32; 3], Vec3>,
    pierced_owners: &BTreeMap<[u32; 3], (u8, u8)>,
    pierce_components: &BTreeMap<[u32; 3], SmallVec<[i32; 2]>>,
    plc_face_owners: &BTreeMap<[u32; 3], (u16, u16)>,
    region_of_cut: &BTreeMap<u32, i32>,
    component_of_cut: &BTreeMap<u32, i32>,
    curve_nodes: &BTreeSet<u32>,
    n_parent_nodes: u32,
    quantum: f64,
    order_quantum: f64,
    probing_spokes: bool,
) {
        path_of[index] = 3;
        mesh.escalated.push((index as u32, reason));
        *mesh.stats.n_escalated.entry(reason).or_insert(0) += 1;
        // The cell cannot take §6's path, but it still has to *conform*: its
        // neighbours have split the faces they share with it. G6-0's outcome is
        // to re-mesh it as a fan from its centroid over the same face
        // triangulations its neighbours use (`junction.rs`).
        let tet = lattice.tets[index];
        let faces = face_states(
            tet,
            &cut_index,
            &second_index,
            &on_cut,
            &all_components,
            &keys,
            &mesh.nodes,
            &curve_pierce,
        );
        let mut boundary: SmallVec<[[u32; 3]; 16]> = SmallVec::new();
        // Where a face's two chords meet, the node sits on **both** surfaces - it is
        // a point of the intersection curve. The split below has to know that, or it
        // asks the classifier for a side at a point exactly on the surface, gets an
        // arbitrary answer, and the cap triangles come out with mixed sides.
        let mut meeting_nodes: Vec<(u32, [i32; 2])> = Vec::new();
        for (state, loop_nodes, crossed) in &faces {
            // Where this face's triangles start, so they can be lifted back out below
            // whichever branch produced them and checked against the cache.
            let face_begin = boundary.len();
            if let Some(state) = state.as_ref() {
                if curve_pierce.contains_key(&state.nodes) {
                    crease_faces.insert(state.nodes);
                    crease_faces_expressible.insert(state.nodes);
                    if let Some((owners, escalated)) = pierced_owners.get(&state.nodes) {
                        if owners == escalated {
                            crease_faces_both_escalated.insert(state.nodes);
                        }
                    }
                }
            } else if let Some(face) = crossed.as_ref() {
                let mut corners = [face.loop_nodes[0]; 3];
                let mut slot = 0usize;
                for node in &face.loop_nodes {
                    if (*node as usize) < n_parent_nodes as usize && slot < 3 {
                        corners[slot] = *node;
                        slot += 1;
                    }
                }
                corners.sort_by_key(|node| keys[*node as usize]);
                if curve_pierce.contains_key(&corners) {
                    crease_faces.insert(corners);
                }
            }
            // G7-1's doubly-cut face rule is applied here, to the *face*, before
            // anything cell-level is decided - and that is load-bearing rather
            // than tidy. A face two walls cross is shared by two cells, and only
            // one of them may turn out to be a sandwich the cell-level split
            // covers; if the rule ran only inside that split, the two cells would
            // triangulate their shared face differently and the mesh would grow
            // hanging nodes exactly where the gap is. Run per face, both cells
            // get the same triangles whatever either of them does next.
            if let Some((split, _)) = band_face_split(loop_nodes, n_parent_nodes, &keys) {
                mesh.stats.n_doubly_cut_faces += 1;
                boundary.extend(split.into_iter().map(|(_, triangle)| triangle));
                check_face_cache(
                    &mut face_cache,
                    &mut face_cache_conflicts,
                    loop_nodes,
                    n_parent_nodes,
                    &keys,
                    &mesh.nodes,
                    crossed.as_ref(),
                    false,
                    &boundary[face_begin..],
                );
                continue;
            }
            // **P-3.3: the kink becomes an edge.** A face a locked curve pierces is still
            // *expressible* whenever one component leaves it two cut nodes - which is what a
            // crease looks like, since both patches belong to the same surface - so §5.2 draws
            // one straight chord between them and the material boundary chamfers across the
            // sharp edge. That is 85.7 % of the P3 damage (§6.1), and no amount of
            // tetrahedralising the cell can repair it: the crease has to be an *edge*, so the
            // pierce point has to be a *node* first.
            //
            // The condition is `every owning cell escalated`, and it is not timidity. A face
            // shared with a cell that took §6's table would keep the straight chord on that
            // side while this side fanned, and the two triangulations of one shared face is
            // precisely how the two earlier crease attempts cracked the mesh. It is a pure
            // function of the *face* - both owners compute the same predicate from the same
            // map - so J1 survives it, and §6's path can be brought along later without
            // reworking this. Of the expressible pierced faces it admits a8 626 of 1,035,
            // a7b 107 of 113, a6b 62 of 72, a6a 37 of 84, a3 109 of 285, a7a 3 of 10.
            let crease_hub = state.as_ref().and_then(|state| {
                let point = curve_pierce.get(&state.nodes)?;
                let (owners, escalated) = pierced_owners.get(&state.nodes)?;
                if owners != escalated {
                    return None;
                }
                // A crease only: every component meeting along the piercing curve has to
                // actually **cut an edge** of this face, or the new node claims a junction
                // the mesh has no material for (see `pierce_components`). `on_cut` does not
                // count towards it: a corner S7 snapped onto a patch says the node is on that
                // surface, not that the surface crosses this face, and admitting it let A-3's
                // cube-sphere intersection curves through - all six of `[V9]`'s
                // `curve_node_id` failures and a fourfold rise in `[V5]`'s misattributed
                // slivers, for +0.002 of P3.
                let cutting: SmallVec<[i32; 2]> = all_components
                    .iter()
                    .copied()
                    .filter(|component| {
                        (0..3).any(|slot| {
                            let (a, b) = (state.nodes[slot], state.nodes[(slot + 1) % 3]);
                            let edge = if a <= b { [a, b] } else { [b, a] };
                            cut_index.contains_key(&(edge, *component))
                                || second_index.contains_key(&(edge, *component))
                        })
                    })
                    .collect();
                let members = pierce_components.get(&state.nodes)?;
                if !members.iter().all(|component| cutting.contains(component)) {
                    return None;
                }
                Some((state.nodes, *point))
            });
            if let Some((corners, point)) = crease_hub {
                // Interned per face by the same map the crossed and centroid paths use, so
                // the second cell finds the node rather than making a coincident duplicate.
                let id = match face_steiner.get(&corners) {
                    Some(id) => *id,
                    None => {
                        let id = mesh.nodes.len() as u32;
                        mesh.nodes.push(point);
                        keys.push(node_key(point, order_quantum));
                        origin.push("crease-hub");
                        face_steiner.insert(corners, id);
                        id
                    }
                };
                // On a locked curve, so on every surface that meets along it - the same
                // statement the crossed and loop-fan paths make about a pierce point, and
                // §7.6 needs it or its split asks the classifier for a side at a point
                // exactly on a surface it was not told about. Narrowing this to the
                // components that actually cut the face was tried and measured **neutral**
                // on every case, so the broader statement stands as the simpler one.
                for component in all_components {
                    meeting_nodes.push((id, [*component, *component]));
                }
                crease_faces_fanned.insert(corners);
                boundary.extend(loop_fan(loop_nodes, id));
                if let Some(shared) = check_face_cache(
                    &mut face_cache,
                    &mut face_cache_conflicts,
                    loop_nodes,
                    n_parent_nodes,
                    &keys,
                    &mesh.nodes,
                    crossed.as_ref(),
                    true,
                    &boundary[face_begin..],
                ) {
                    boundary.truncate(face_begin);
                    boundary.extend(orient_face_outward(&shared, tet, &mesh.nodes));
                }
                continue;
            }
            match face_mesh(state.as_ref(), loop_nodes, &keys, crossed.as_ref()) {
                FaceMesh::Table(triangles) => boundary.extend(triangles),
                FaceMesh::Crossed(face) => {
                    mesh.stats.n_crossed_faces += 1;
                    // A meeting point is needed only where the chords actually cross;
                    // when they do it is interned per *face* like the centroid Steiner
                    // point below, and for the same reason: the two cells sharing this
                    // face must cone to the same node or the shared face stops
                    // matching. The key is the face's three parent corners, which both
                    // cells derive from the face alone.
                    let mut on_curve = false;
                    let id = match face.point {
                        None => u32::MAX,
                        Some(point) => {
                            // G6-0: the chords' closest approach is where the two cut
                            // *chords* meet, which is only an estimate of where the two
                            // surfaces do. When the curve itself pierces this face, that
                            // is the answer, and it is exact.
                            let mut corners = [face.loop_nodes[0]; 3];
                            let mut slot = 0usize;
                            for node in &face.loop_nodes {
                                if (*node as usize) < snapped.nodes.len() && slot < 3 {
                                    corners[slot] = *node;
                                    slot += 1;
                                }
                            }
                            corners.sort_by_key(|node| keys[*node as usize]);
                            let point = match curve_pierce.get(&corners) {
                                Some(pierce) => {
                                    on_curve = true;
                                    *pierce
                                }
                                None => point,
                            };
                            match face_steiner.get(&corners) {
                                Some(id) => *id,
                                None => {
                                    let id = mesh.nodes.len() as u32;
                                    mesh.nodes.push(point);
                                    keys.push(node_key(point, order_quantum));
                                    origin.push("face-steiner");
                                    face_steiner.insert(corners, id);
                                    id
                                }
                            }
                        }
                    };
                    // A point on a locked curve lies on **every** surface that meets along
                    // it, not just on the two whose chords happened to name it. Telling
                    // the split only the guessed pair is what leaves `mixed sides on one
                    // triangle` on a triangle whose corner is exactly on a third surface:
                    // A-6a's `[42299, 27586, 11041]`, where 42299 sits on the limb's rim
                    // at x = 0.5817 *and* z = 0.2977 and the chords that found it both
                    // belong to component 2. Where the point is only the chords' closest
                    // approach it is on those two surfaces and no others, so the pair
                    // stands. `LoopFan` already draws this distinction; the two branches
                    // differing on it was an oversight, not a decision.
                    if id != u32::MAX {
                        if on_curve {
                            for component in all_components {
                                meeting_nodes.push((id, [*component, *component]));
                            }
                        } else {
                            meeting_nodes.push((id, [face.components[0], face.components[1]]));
                        }
                    }
                    boundary.extend(crossed_face_mesh(&face, &keys, id));
                }
                FaceMesh::LoopFan(loop_nodes) => {
                    mesh.stats.n_generic_faces += 1;
                    // One Steiner point per *face*, not per cell: the two cells
                    // sharing it must use the same node, or the coincident
                    // duplicates make the shared face non-conforming again.
                    let mut corners = [loop_nodes[0]; 3];
                    let mut slot = 0usize;
                    for node in &loop_nodes {
                        if (*node as usize) < snapped.nodes.len() && slot < 3 {
                            corners[slot] = *node;
                            slot += 1;
                        }
                    }
                    corners.sort_by_key(|node| keys[*node as usize]);
                    // G6-0, the second way a curve reaches a face: through one of the
                    // walk's own nodes rather than through the interior. S7 snaps nodes
                    // onto locked curves on purpose, so this is the *common* case near a
                    // rim, and `segment_pierces_triangle` reports nothing for it by
                    // design. Every trace on such a face terminates at that node, so a
                    // fan from it keeps all of them as edges and interns nothing - both
                    // cells derive the same apex from the face alone, so J1 holds exactly
                    // as it does for the piercing point. Read off A-6a's remaining 21:
                    // face `(11057, 27364, 42321)`, where 11057 sits at x = 0.5817 *and*
                    // z = 0.2977 - on the rim - while 42321 is the face centroid the fan
                    // used instead, 3.2e-4 off the contact plane. More than one such node
                    // means the curve runs *along* a walk edge, which is a different
                    // shape and is left to the centroid.
                    let hub = if curve_pierce.contains_key(&corners) {
                        None
                    } else {
                        let on: SmallVec<[usize; 2]> = loop_nodes
                            .iter()
                            .enumerate()
                            .filter(|(_, node)| curve_nodes.contains(node))
                            .map(|(slot, _)| slot)
                            .collect();
                        (on.len() == 1)
                            .then(|| fan_from_walk_node(&loop_nodes, on[0], &mesh.nodes))
                            .flatten()
                            .map(|fan| (loop_nodes[on[0]], fan))
                    };
                    if let Some((hub_id, fan)) = hub {
                        for component in all_components {
                            meeting_nodes.push((hub_id, [*component, *component]));
                        }
                        boundary.extend(fan);
                        check_face_cache(
                            &mut face_cache,
                            &mut face_cache_conflicts,
                            &loop_nodes,
                            n_parent_nodes,
                            &keys,
                            &mesh.nodes,
                            crossed.as_ref(),
                            false,
                            &boundary[face_begin..],
                        );
                        continue;
                    }
                    let id = match face_steiner.get(&corners) {
                        Some(id) => *id,
                        None => {
                            // G6-0: on the curve where one pierces this face, and at the
                            // face's own centre otherwise. Both are pure functions of the
                            // face, so the two cells sharing it still agree; the
                            // difference is that the first puts a mesh node on locked
                            // geometry, which is the only way the material boundary can
                            // follow the curve instead of chamfering past it.
                            let point = curve_pierce
                                .get(&corners)
                                .copied()
                                .unwrap_or_else(|| face_centroid(corners, &mesh.nodes));
                            let id = mesh.nodes.len() as u32;
                            mesh.nodes.push(point);
                            keys.push(node_key(point, order_quantum));
                            origin.push("curve-pierce");
                            face_steiner.insert(corners, id);
                            id
                        }
                    };
                    // A point on a locked curve lies on **every** surface that meets
                    // along it, and §7.6's split has to be told so for each of them, not
                    // for a guessed pair. Otherwise the split asks the classifier for a
                    // side at a point exactly on a surface it was not told about, gets
                    // whichever way the predicate rounds, and declines.
                    if curve_pierce.contains_key(&corners) {
                        for component in all_components {
                            meeting_nodes.push((id, [*component, *component]));
                        }
                    }
                    boundary.extend(loop_fan(&loop_nodes, id));
                }
            }
            // **The cache is authoritative from here (§7.3).** Its triangles replace this
            // cell's own, re-wound outward from this cell. Today that is provably a no-op -
            // zero conflicts across the nine cases, winding included - and that is the point:
            // the switch is made while it changes nothing, so that when crease chords are
            // added to the *cache* they reach both cells identically. Adding them per cell is
            // what cracked the mesh twice.
            if let Some(shared) = check_face_cache(
                &mut face_cache,
                &mut face_cache_conflicts,
                loop_nodes,
                n_parent_nodes,
                &keys,
                &mesh.nodes,
                crossed.as_ref(),
                false,
                &boundary[face_begin..],
            ) {
                boundary.truncate(face_begin);
                boundary.extend(orient_face_outward(&shared, tet, &mesh.nodes));
            }
        }
        // G7-1: a cell whose two walls sandwich a thin gap is *not* one blob. If
        // the doubly-cut face rule covers all four of its faces, it splits into
        // three closed slabs and the gap survives as elements of its own; the
        // single fan below would have chamfered it away.
        let band: Option<BandCellPlan> = if options.bands {
            let loops: [&[u32]; 4] = [
                &faces[0].1,
                &faces[1].1,
                &faces[2].1,
                &faces[3].1,
            ];
            match split_band_cell(loops, n_parent_nodes, &keys, &mesh.nodes) {
                Ok(plan) => Some(plan),
                Err(reason) => {
                    *mesh.stats.n_band_declined.entry(reason).or_insert(0) += 1;
                    // "Declined" says the rule did not claim the cell and not which
                    // shape it was offered. The signature is (loop length, cut nodes)
                    // per face, which is exactly what a missing §8.2 row looks like.
                    if std::env::var_os("RUSTMSPT_THIN_DIAG").is_some() {
                        let signature: Vec<(usize, usize)> = loops
                            .iter()
                            .map(|face| {
                                (
                                    face.len(),
                                    face.iter()
                                        .filter(|node| **node >= n_parent_nodes)
                                        .count(),
                                )
                            })
                            .collect();
                        eprintln!("[S8b-DIAG] band declined {reason:?}: {signature:?}");
                    }
                    None
                }
            }
        } else {
            None
        };
        // G7-2: which thin region this cell's gap belongs to. Taken as the lowest
        // id any of the six wall nodes is attributed to rather than, say, wall A's
        // - a region owns both walls, so the two lids agree whenever S3 attributed
        // them at all, and `min` makes the answer independent of which lid the
        // three-slab split happened to orient first.
        let band_region_id: i32 = band
            .as_ref()
            .map(|plan| {
                plan.pairs
                    .iter()
                    .flat_map(|pair| [pair.a, pair.b])
                    .filter_map(|node| region_of_cut.get(&node).copied())
                    .filter(|id| *id >= 0)
                    .min()
                    .unwrap_or(-1)
            })
            .unwrap_or(-1);
        let parent_record = &classification.records[index];
        // Each slab is a closed boundary; the *gap* slab is additionally a band
        // cell, so it goes to the frozen §8.2 table first and falls back to a fan
        // only when the table or the §4.4 floor refuses it - which is the ladder's
        // last rung, and is what `regime = 2` means in the contract arrays.
        let mut slabs: CellSlabs = SmallVec::new();
        // Which slabs §7.6's split produced, and the nodes each component's surface is
        // known to pass through - both only for the spoke probe below, which is off
        // unless `RUSTMSPT_SPOKE_PROBE` is set.
        let mut split_slabs: std::ops::Range<usize> = 0..0;
        let mut on_surface: BTreeMap<i32, BTreeSet<u32>> = BTreeMap::new();
        if probing_spokes {
            for component in all_components {
                let mut here: BTreeSet<u32> = BTreeSet::new();
                for a in 0..4 {
                    for b in (a + 1)..4 {
                        let (x, y) = (lattice.tets[index][a], lattice.tets[index][b]);
                        let edge = if x <= y { [x, y] } else { [y, x] };
                        if let Some(node) = cut_index.get(&(edge, *component)) {
                            here.insert(*node);
                        }
                        if let Some(node) = second_index.get(&(edge, *component)) {
                            here.insert(*node);
                        }
                    }
                }
                for node in &lattice.tets[index] {
                    if on_cut.contains_key(&(*node, *component)) {
                        here.insert(*node);
                    }
                }
                for (node, owners) in &meeting_nodes {
                    if owners.contains(component) {
                        here.insert(*node);
                    }
                }
                on_surface.insert(*component, here);
            }
        }
        match band {
            Some(plan) => {
                mesh.stats.n_band_cells += 1;
                for (slot, slab) in plan.slabs.into_iter().enumerate() {
                    let pairs = (slot == Slab::Band as usize).then_some(plan.pairs);
                    slabs.push((slab, pairs));
                }
            }
            None => {
                // §7.6, the cut *inside* an escalated cell. The centroid fan below
                // is total but gives the patch up as a face of the mesh, which is
                // what chamfers the material boundary by up to one cell and what
                // leaves adjacent tets stepping two components across an undeclared
                // face. Split the conforming boundary soup by each crossing
                // component first, cap both halves with the surface polygon, and
                // fan the pieces instead; every rejection path falls straight back
                // to the single fan, and the whole split is discarded unless the
                // pieces' volumes still sum to the parent's.
                // §7.6 is about **junctions** - cells where surfaces meet. Two walls
                // crossing one cell without ever meeting inside it is a *gap*, and a
                // gap belongs to S8b's band ladder, which knows its regime and its
                // thickness. Requiring at least one face whose chords actually cross
                // is what tells the two apart, and the acceptance data separates
                // cleanly on it: A-3's junction cells carry crossing chords, while
                // A-7a's 512 parallel-plate cells carry none and splitting them cost
                // 0.36/0.10 % -> 0.57/0.44 % of volume.
                // Only *two* surfaces sharing a cell raise the gap-versus-junction
                // question; a cell one surface crosses has no gap to mistake, and
                // there the cut is simply the one §6 refused - which is the whole
                // 4.1 % A-8 loses, 3,687 cells chamfered by the fan because nothing
                // ever cut them.
                // "Two surfaces" means two *components*, not two chords. One component
                // crossing a face twice is a lattice face straddling a thin plate -
                // the plate's own two walls - and that is a gap S8b owns rather than
                // two bodies approaching each other, so the meeting-point test must
                // not be asked of it.
                // **Every escalated cell is offered to the split (P-3.2).** Until
                // 2026-08-15 a two-surface cell was offered only when the two surfaces
                // *met inside it* - the reasoning being that two walls crossing a cell
                // without meeting is a *gap*, and a gap belongs to S8b's band ladder,
                // which knows its regime and its thickness. The reasoning is sound and
                // the gate was wrong anyway, because it fires on cells S8b **declined**:
                // A-7a's 0.006 gap is below the sheet threshold, so the ladder passes,
                // and its 2,176 parallel-plate cells then fell through to the fan with
                // nothing to catch them.
                //
                // It survived two reviews because both measured `[V6]` and volume error,
                // and neither can see P3 - a boundary displaced half a cell off the
                // surface scores exactly like one lying on it. Measured with `[V13]`,
                // removing the gate takes A-7a from **73.4 % to 88.7 %** of material-
                // boundary area on the surface (off-surface area -58 %) and A-3 from
                // 89.4 % to 91.5 %, for +3.6 % / +0.9 % elements. Six of the nine cases
                // are bit-identical - no cell reaches the path. Volume error stays inside
                // its 1 % gate, `[V1]`/`[V3]` are unchanged, `[V6]`'s adjacency count
                // improves, min dihedral is identical to three decimals, and R-P2 holds.
                //
                // Nothing replaces it, and nothing may: which cells get a conforming cut
                // is not a setting (R3). The split's own guards decline safely where it
                // cannot work - on A-7a they hard-fail zero times - and every rejection
                // path still falls back to the fan.
                let split = Some(split_escalated_cell(
                    index,
                    &boundary,
                    tet,
                    &cut_index,
                    &second_index,
                    &on_cut,
                    &all_components,
                    classifier,
                    &mesh.nodes,
                    &keys,
                    options.volume_tolerance,
                    options.eps,
                    &meeting_nodes,
                    &plc_face_owners,
                ))
                .flatten();
                match split {
                    Some((pieces, caps)) => {
                        mesh.stats.n_junction_splits += 1;
                        mesh.stats.n_junction_split_pieces += pieces.len();
                        for (triangle, component) in caps {
                            for node in triangle {
                                on_surface.entry(component).or_default().insert(node);
                            }
                            pending_interfaces.push((mesh.tets.len(), triangle, component));
                        }
                        split_slabs = slabs.len()..slabs.len() + pieces.len();
                        for piece in pieces {
                            slabs.push((piece.to_vec(), None));
                        }
                    }
                    None => {
                        mesh.stats.n_steiner_fans += 1;
                        slabs.push((boundary.to_vec(), None));
                    }
                }
            }
        }
        // The gap slab's two lids are the walls themselves: tag them, with the
        // component whose crossings they are built from.
        if let Some(pairs) = slabs.iter().find_map(|(_, pairs)| *pairs) {
            for wall in [
                [pairs[0].a, pairs[1].a, pairs[2].a],
                [pairs[0].b, pairs[1].b, pairs[2].b],
            ] {
                if let Some(component) = component_of_cut.get(&wall[0]) {
                    pending_interfaces.push((mesh.tets.len(), wall, *component));
                }
            }
        }
        for (slot, (slab, pairs)) in slabs.iter().enumerate() {
            if probing_spokes && pairs.is_none() {
                probe_spoke_cut(
                    slab,
                    &mesh.nodes,
                    &all_components,
                    classifier,
                    &on_surface,
                    split_slabs.contains(&slot),
                    &mut spoke_probe,
                );
            }
            if let Some(pairs) = pairs {
                let diagonals = snk_cell_diagonals(*pairs, &keys);
                if let Ok(meshed) =
                    mesh_band_cell(*pairs, diagonals, &mesh.nodes, options.min_dihedral_deg, 0)
                {
                    *mesh
                        .stats
                        .n_band_templates
                        .entry(meshed.template)
                        .or_insert(0) += 1;
                    mesh.stats.band_min_dihedral_deg = mesh
                        .stats
                        .band_min_dihedral_deg
                        .min(meshed.min_dihedral_deg);
                    for piece in &meshed.tets {
                        mesh.tets.push(*piece);
                        mesh.records.push(seed_record(
                            parent_record,
                            *piece,
                            &all_components,
                            &mesh.nodes,
                            classifier,
                            &mut seed_uncertain,
                        ));
                        *seeded_pieces += 1;
                        mesh.parent_of.push(index as u32);
                        mesh.regime.push(meshed.template.regime_code());
                        mesh.band_region.push(band_region_id);
                    }
                    continue;
                }
            }
            // A split piece that is not star-shaped from its centroid was accepted by the split
            // only because the constrained kernel can mesh it; mesh it that way here.
            if pairs.is_none()
                && split_slabs.contains(&slot)
                && !fan_is_sound(slab, &mesh.nodes)
            {
                if let Some(tets) = cdt_piece(slab, &mesh.nodes) {
                    mesh.stats.n_cdt_pieces += 1;
                    for oriented in tets {
                        mesh.tets.push(oriented);
                        mesh.records.push(seed_record(
                            parent_record,
                            oriented,
                            &all_components,
                            &mesh.nodes,
                            classifier,
                            &mut seed_uncertain,
                        ));
                        *seeded_pieces += 1;
                        mesh.parent_of.push(index as u32);
                        mesh.regime.push(REGIME_NORMAL);
                        mesh.band_region.push(-1);
                    }
                    continue;
                }
            }
            let centroid = polygon_soup_centroid(slab, &mesh.nodes);
            let centroid_id = mesh.nodes.len() as u32;
            mesh.nodes.push(centroid);
            keys.push(node_key(centroid, quantum));
            origin.push("band-slab-centroid");
            let fanned = fan_cell(slab, centroid_id);
            if pairs.is_some() {
                *mesh
                    .stats
                    .n_band_templates
                    .entry(BandTemplate::Steiner)
                    .or_insert(0) += 1;
            }
            for piece in &fanned.tets {
                let oriented = orient_positively(*piece, &mesh.nodes);
                let Some(oriented) = oriented else {
                    mesh.stats.n_degenerate_fan_pieces += 1;
                    continue;
                };
                mesh.tets.push(oriented);
                mesh.records.push(seed_record(
                    parent_record,
                    oriented,
                    &all_components,
                    &mesh.nodes,
                    classifier,
                    &mut seed_uncertain,
                ));
                *seeded_pieces += 1;
                mesh.parent_of.push(index as u32);
                mesh.regime
                    .push(if pairs.is_some() { 2 } else { REGIME_NORMAL });
                mesh.band_region
                    .push(if pairs.is_some() { band_region_id } else { -1 });
            }
        }
        mesh.stats.n_uncut_cells += 1;
}