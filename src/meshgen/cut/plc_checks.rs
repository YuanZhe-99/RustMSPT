//! S8's post-assembly checks and repair (plan M-3.3): the per-path censuses that charge leaks,
//! T-junctions, duplicates, `[V6]` steps, off-surface area and degenerate elements to the arm that
//! emitted them (print-only, under `RUSTMSPT_PLC_DIAG`), and the T-junction repair, which does change
//! the mesh. Moved out of `cut_lattice` unchanged.

use super::*;

// AI-FUNC-SUMMARY:
// Purpose: After S8's assembly, when §7.4 meshed or fanned any cell: the print-only censuses and the
//   T-junction repair to a fixed point.
// Inputs: the cut mesh and its node origins, the lattice, the classifier, every component, the
//   per-cell path, the §7.4 promises (face triangulations, owners, boundaries, edge points,
//   refusals), the pending interface records, the cut maps, the parent-node count, the quantum and
//   the S8 lap timer.
// Returns: None.
// Side effects: Repairs T-junctions in `mesh` (tets replaced, records moved); pushes warnings.
// Notes: Byte-for-byte the block it replaced; the repair's order is unchanged.
#[allow(clippy::too_many_arguments)]
pub(super) fn plc_checks(
    mesh: &mut CutMesh,
    origin: &mut Vec<&'static str>,
    trace_faces: &BTreeMap<u32, BTreeSet<[u32; 3]>>,
    lattice: &Lattice,
    classifier: &PointClassifier,
    all_components: &[i32],
    path_of: &[u8],
    plc_face_tris: &BTreeMap<[u32; 3], Vec<[u32; 3]>>,
    plc_face_owners_n: &BTreeMap<[u32; 3], usize>,
    plc_boundary: &[Option<Vec<[u32; 3]>>],
    plc_edge_points: &BTreeMap<[u32; 2], Vec<u32>>,
    plc_decline: &[Option<&'static str>],
    pending_interfaces: &[(usize, [u32; 3], i32)],
    cut_index: &BTreeMap<([u32; 2], i32), u32>,
    second_index: &BTreeMap<([u32; 2], i32), u32>,
    n_parent_nodes: u32,
    quantum: f64,
    s8_lap: &mut dyn FnMut(&str),
) {
    // Print-only censuses: they build maps over every face and edge of the mesh (~400 MB on
    // a4), so they run under `RUSTMSPT_PLC_DIAG` only. Nothing below reads their results.
    let plc_diag = std::env::var_os("RUSTMSPT_PLC_DIAG").is_some();
    let mut unmatched = 0usize;
    let mut unmatched_augmented = 0usize;
    let mut flat_by_path = [0usize; 5];
    let mut inverted_by_path = [0usize; 5];
    let mut worst_flat = f64::INFINITY;
    if plc_diag {
    // **Did the mesh actually take the faces the faces promised?** Every triangle of an
    // augmented face should be carried by exactly as many tets as the face has owners - two
    // inside the lattice, one on the domain boundary. A triangle carried by fewer is a leak
    // whose cause is on the FACE side; one the mesh never produced at all means an owner
    // ignored the triangulation it was handed. Counting it here rather than inferring it from
    // `[V3]`'s totals is what separates the two.
    let mut carried: BTreeMap<[u32; 3], usize> = BTreeMap::new();
    for tet in &mesh.tets {
        for slots in [[0usize, 1, 2], [0, 1, 3], [0, 2, 3], [1, 2, 3]] {
            let mut face = [tet[slots[0]], tet[slots[1]], tet[slots[2]]];
            face.sort_unstable();
            *carried.entry(face).or_insert(0) += 1;
        }
    }
    let (mut short, mut absent, mut over) = (0usize, 0usize, 0usize);
    for (face, tris) in plc_face_tris {
        let expected = plc_face_owners_n.get(face).copied().unwrap_or(2);
        for triangle in tris {
            let mut key = *triangle;
            key.sort_unstable();
            match carried.get(&key).copied().unwrap_or(0) {
                0 => absent += 1,
                n if n < expected => short += 1,
                n if n > expected => over += 1,
                _ => {}
            }
        }
    }
    mesh.warnings.push(format!(
        "[PLC] of the augmented faces' triangles: {absent} appear in no tet at all, {short} in \
         fewer tets than the face has owners, {over} in more. The first two are the face side's \
         leak and the third is a face emitted twice by one cell"
    ));
    // The same question of the boundary a §7.4 cell was actually handed, which is the augmented
    // faces plus the ones taken from §5.2. A triangle short here and sound above means the
    // mismatch is on a face §7.4 left alone - between `face_split` and whatever the neighbour
    // that took §6's table or escalated actually emitted for it.
    let mut lone: BTreeMap<[u32; 3], usize> = BTreeMap::new();
    for boundary in plc_boundary.iter().flatten() {
        for triangle in boundary {
            let mut key = *triangle;
            key.sort_unstable();
            *lone.entry(key).or_insert(0) += 1;
        }
    }
    for (triangle, promised) in &lone {
        let got = carried.get(triangle).copied().unwrap_or(0);
        if got >= *promised {
            continue;
        }
        unmatched += 1;
        if plc_face_tris.values().any(|tris| {
            tris.iter().any(|t| {
                let mut k = *t;
                k.sort_unstable();
                k == *triangle
            })
        }) {
            unmatched_augmented += 1;
        }
    }
    s8_lap("t-carried");
    // **Which path emitted each leaking face.** A face carried by exactly one tet, whose three
    // nodes are not all on one domain plane, is `[V3]`'s boundary leak; charging it to the path
    // its tet's parent took says which of them is wrong, and stops the diagnosis being a
    // sequence of guesses.
    let mut face_tet: BTreeMap<[u32; 3], usize> = BTreeMap::new();
    for (at, tet) in mesh.tets.iter().enumerate() {
        for slots in [[0usize, 1, 2], [0, 1, 3], [0, 2, 3], [1, 2, 3]] {
            let mut face = [tet[slots[0]], tet[slots[1]], tet[slots[2]]];
            face.sort_unstable();
            face_tet.entry(face).or_insert(at);
        }
    }
    let (mut lo, mut hi) = (mesh.nodes[0], mesh.nodes[0]);
    for p in &mesh.nodes {
        lo = Vec3::new(lo.x.min(p.x), lo.y.min(p.y), lo.z.min(p.z));
        hi = Vec3::new(hi.x.max(p.x), hi.y.max(p.y), hi.z.max(p.z));
    }
    let span = hi.sub(lo).dot(hi.sub(lo)).sqrt().max(f64::MIN_POSITIVE);
    let on_domain = |face: &[u32; 3]| -> bool {
        let p = [
            mesh.nodes[face[0] as usize],
            mesh.nodes[face[1] as usize],
            mesh.nodes[face[2] as usize],
        ];
        (0..3).any(|axis| {
            let get = |q: Vec3| match axis {
                0 => (q.x, lo.x, hi.x),
                1 => (q.y, lo.y, hi.y),
                _ => (q.z, lo.z, hi.z),
            };
            p.iter().all(|q| {
                let (v, a, b) = get(*q);
                (v - a).abs() <= span * 1.0e-9 || (v - b).abs() <= span * 1.0e-9
            })
        })
    };
    let mut leak_by_path = [0usize; 5];
    for (face, count) in &carried {
        if *count != 1 || on_domain(face) {
            continue;
        }
        let at = face_tet.get(face).copied().unwrap_or(0);
        let parent = mesh.parent_of.get(at).copied().unwrap_or(0) as usize;
        leak_by_path[path_of.get(parent).copied().unwrap_or(4).min(4) as usize] += 1;
    }
    mesh.warnings.push(format!(
        "[PLC] leaking faces by the path that emitted them: {} §6's table, {} §7.4-meshed, \
         {} §7.4-fanned, {} escalated, {} other",
        leak_by_path[0], leak_by_path[1], leak_by_path[2], leak_by_path[3], leak_by_path[4]
    ));
    s8_lap("t-leak");
    // **The same question asked of the hanging nodes, and asked as an EDGE question.**
    // A leak is a face two cells disagree about; a T-junction need not be. A cell can carry a
    // lattice edge whole while the point another cell put on that edge sits in the middle of
    // it - every face involved still has two owners, so `carried` is balanced and the leak
    // count says nothing. The test is therefore not about faces at all: an interned edge point
    // is a hanging node exactly when some tet still has both of that edge's endpoints as
    // vertices. Charging that tet to its parent's path says which arm kept the edge.
    let mut tet_edges: BTreeMap<[u32; 2], usize> = BTreeMap::new();
    for (at, tet) in mesh.tets.iter().enumerate() {
        for pair in [[0usize, 1], [0, 2], [0, 3], [1, 2], [1, 3], [2, 3]] {
            let (x, y) = (tet[pair[0]], tet[pair[1]]);
            tet_edges.entry(if x <= y { [x, y] } else { [y, x] }).or_insert(at);
        }
    }
    let mut referenced = vec![false; mesh.nodes.len()];
    for tet in &mesh.tets {
        for node in tet {
            referenced[*node as usize] = true;
        }
    }
    let mut tjunction_by_path = [0usize; 5];
    let (mut split_edges, mut orphan_points) = (0usize, 0usize);
    let (mut tjunction_known, mut tjunction_second, mut tjunction_unknown) = (0usize, 0usize, 0usize);
    for (edge, ids) in plc_edge_points {
        let interior: Vec<u32> =
            ids.iter().copied().filter(|id| !edge.contains(id)).collect();
        if interior.is_empty() {
            continue;
        }
        split_edges += 1;
        orphan_points += interior
            .iter()
            .filter(|id| !referenced[**id as usize])
            .count();
        let Some(at) = tet_edges.get(edge).copied() else { continue };
        let parent = mesh.parent_of.get(at).copied().unwrap_or(0) as usize;
        let path = path_of.get(parent).copied().unwrap_or(4).min(4) as usize;
        tjunction_by_path[path] += interior.len();
        // **Does §6's table know about the point at all?** A trace point that is ALSO one of
        // §6's own edge crossings is a point every cell around the edge already splits, and it
        // cannot be the T-junction. One the face trace interned and `cut_index` has never heard
        // of is invisible to every cell that reads §5.2 - and that is a different defect, at a
        // different place, from the exclusion propagating too narrowly. a8 is the only case with
        // either, so the two have never been told apart.
        if path == 0 {
            for id in &interior {
                if all_components
                    .iter()
                    .any(|c| cut_index.get(&(*edge, *c)) == Some(id))
                {
                    tjunction_known += 1;
                } else if all_components
                    .iter()
                    .any(|c| second_index.get(&(*edge, *c)) == Some(id))
                {
                    tjunction_second += 1;
                } else {
                    tjunction_unknown += 1;
                }
            }
        }
    }
    mesh.warnings.push(format!(
        "[PLC] of {split_edges} lattice edge(s) an interned point splits, the tets that still \
         carry the whole edge are: {} §6's table, {} §7.4-meshed, {} §7.4-fanned, {} \
         escalated, {} other - and {orphan_points} of those points are in no tet at all",
        tjunction_by_path[0],
        tjunction_by_path[1],
        tjunction_by_path[2],
        tjunction_by_path[3],
        tjunction_by_path[4]
    ));
    if tjunction_by_path[0] > 0 {
        mesh.warnings.push(format!(
            "[PLC] of the points §6's table carries whole, {tjunction_known} are §6's OWN edge \
             crossing (it splits them anyway), {tjunction_second} are a second crossing on the \
             same edge, and {tjunction_unknown} are points only the face trace has - those last \
             are invisible to §5.2 and are the T-junction"
        ));
    }
    s8_lap("t-tjunction");
    // **The same tet emitted twice, by the path that emitted it.** `[V1]` calls it a duplicate
    // cell and `[V3]` sees the faces around it carried four, eight or ten times; both are one
    // defect, and it is a cell whose pieces overlap rather than partition. Charged by path
    // because a duplicate from the kernel and a duplicate from a fan are different bugs.
    {
        let mut seen: BTreeMap<[u32; 4], usize> = BTreeMap::new();
        let mut duplicate_by_path = [0usize; 5];
        let mut duplicate_parents: BTreeSet<u32> = BTreeSet::new();
        for (at, tet) in mesh.tets.iter().enumerate() {
            let mut key = *tet;
            key.sort_unstable();
            match seen.entry(key) {
                std::collections::btree_map::Entry::Vacant(slot) => {
                    slot.insert(at);
                }
                std::collections::btree_map::Entry::Occupied(_) => {
                    let parent = mesh.parent_of.get(at).copied().unwrap_or(0);
                    duplicate_parents.insert(parent);
                    duplicate_by_path
                        [path_of.get(parent as usize).copied().unwrap_or(4).min(4) as usize] += 1;
                }
            }
        }
        let total: usize = duplicate_by_path.iter().sum();
        if total > 0 {
            mesh.warnings.push(format!(
                "[PLC] {total} duplicate tet(s) over {} parent cell(s): {} §6's table, {} \
                 §7.4-meshed, {} §7.4-fanned, {} escalated, {} facet-split - a cell whose \
                 pieces overlap rather than partition",
                duplicate_parents.len(),
                duplicate_by_path[0],
                duplicate_by_path[1],
                duplicate_by_path[2],
                duplicate_by_path[3],
                duplicate_by_path[4]
            ));
        }
    }
    // Faces carried by three or more tets, by the paths that carried them. A face emitted by
    // both of its owners AND by a third cell is not a tolerance question: one of the three
    // built a triangle on a face that is not its own.
    let mut over_by_path = [0usize; 5];
    let mut over_faces = 0usize;
    for (face, count) in &carried {
        if *count < 3 {
            continue;
        }
        over_faces += 1;
        for (at, tet) in mesh.tets.iter().enumerate() {
            for slots in [[0usize, 1, 2], [0, 1, 3], [0, 2, 3], [1, 2, 3]] {
                let mut key = [tet[slots[0]], tet[slots[1]], tet[slots[2]]];
                key.sort_unstable();
                if key == *face {
                    let parent = mesh.parent_of.get(at).copied().unwrap_or(0) as usize;
                    over_by_path
                        [path_of.get(parent).copied().unwrap_or(4).min(4) as usize] += 1;
                }
            }
        }
    }
    mesh.warnings.push(format!(
        "[PLC] {over_faces} face(s) carried by three or more tets, whose carriers are: {} \
         §6's table, {} §7.4-meshed, {} §7.4-fanned, {} escalated, {} other",
        over_by_path[0],
        over_by_path[1],
        over_by_path[2],
        over_by_path[3],
        over_by_path[4]
    ));
    s8_lap("t-dup");
    // **`[V6]`'s adjacency violations, charged the same way.** A face between two tets whose
    // records name different materials is a material boundary and must carry an interface tag.
    // Whether the two tets are in ONE cell or in two says which half is at fault: inside a cell
    // the regions are separated by a constraint face and the cap list should already name it,
    // while across a lattice face the two cells classified independently and disagreed.
    {
        let mut tagged: BTreeSet<[u32; 3]> = BTreeSet::new();
        for (_, triangle, _) in pending_interfaces {
            let mut key = *triangle;
            key.sort_unstable();
            tagged.insert(key);
        }
        let mut sides: BTreeMap<[u32; 3], SmallVec<[usize; 2]>> = BTreeMap::new();
        for (at, tet) in mesh.tets.iter().enumerate() {
            for slots in [[0usize, 1, 2], [0, 1, 3], [0, 2, 3], [1, 2, 3]] {
                let mut face = [tet[slots[0]], tet[slots[1]], tet[slots[2]]];
                face.sort_unstable();
                sides.entry(face).or_default().push(at);
            }
        }
        let inside_set = |at: usize| -> Vec<i32> {
            let mut out: Vec<i32> = mesh
                .records
                .get(at)
                .map(|r| {
                    r.entries
                        .iter()
                        .filter(|(_, side)| *side == Side::Inside)
                        .map(|(x, _)| *x)
                        .collect()
                })
                .unwrap_or_default();
            out.sort_unstable();
            out
        };
        let (mut same_cell, mut cross_cell) = (0usize, 0usize);
        let mut cross_by_path = [0usize; 5];
        let mut same_by_path = [0usize; 5];
        // **And which refusal stranded each one**, ranked the way P3's stranding is: a
        // two-component step is a region the cell could not separate, so the class of decline
        // that sent the cell to the fan is the thing to fix, not the step itself.
        let mut step_by_reason: BTreeMap<&'static str, usize> = BTreeMap::new();
        for (face, at) in &sides {
            if at.len() != 2 || tagged.contains(face) {
                continue;
            }
            // **`[V6]`'s own rule, not a broader one.** A face may change the inside-set by as
            // many components as it is tagged with, and an untagged face is allowed one - a
            // single body's surface passing through. Counting every disagreement instead reads
            // 9,368 where `[V6]` reads 792, and a diagnostic that does not agree with the check
            // it is diagnosing is worse than none.
            let (a, b) = (inside_set(at[0]), inside_set(at[1]));
            let difference = a.iter().filter(|x| !b.contains(x)).count()
                + b.iter().filter(|x| !a.contains(x)).count();
            if difference <= 1 {
                continue;
            }
            let (a, b) = (
                mesh.parent_of.get(at[0]).copied().unwrap_or(0) as usize,
                mesh.parent_of.get(at[1]).copied().unwrap_or(0) as usize,
            );
            for parent in [a, b] {
                if let Some(reason) = plc_decline.get(parent).copied().flatten() {
                    *step_by_reason.entry(reason).or_insert(0) += 1;
                }
            }
            if a == b {
                same_cell += 1;
                same_by_path[path_of.get(a).copied().unwrap_or(4).min(4) as usize] += 1;
            } else {
                cross_cell += 1;
                for parent in [a, b] {
                    cross_by_path
                        [path_of.get(parent).copied().unwrap_or(4).min(4) as usize] += 1;
                }
            }
        }
        mesh.warnings.push(format!(
            "[PLC] `[V6]` steps of two or more components across an untagged face: \
             {same_cell} inside one cell ({} table / {} meshed / {} fanned / {} escalated / \
             {} other) and {cross_cell} across a lattice face ({} / {} / {} / {} / {} by side)",
            same_by_path[0],
            same_by_path[1],
            same_by_path[2],
            same_by_path[3],
            same_by_path[4],
            cross_by_path[0],
            cross_by_path[1],
            cross_by_path[2],
            cross_by_path[3],
            cross_by_path[4]
        ));
        let mut ranked: Vec<(&'static str, usize)> = step_by_reason.into_iter().collect();
        ranked.sort_by_key(|(_, count)| std::cmp::Reverse(*count));
        for (reason, count) in ranked {
            mesh.warnings.push(format!(
                "[PLC]   `[V6]` step sides in a cell §7.4 refused, {count} - {reason}"
            ));
        }
    }

    s8_lap("t-v6");
    // **The histogram never built: off-surface AREA charged to the emitting path.** Every
    // previous cause in this step was found by charging a defect to the arm that produced it,
    // and P3 - the property the whole project is for - had never been charged to anything. The
    // criterion is `[V13]`'s: a material-boundary face is on the surface when its corners are,
    // within 2 % of the face's own size. Measured here rather than in the verifier because only
    // this side knows which arm emitted the face.
    {
        let mut sides: BTreeMap<[u32; 3], SmallVec<[usize; 2]>> = BTreeMap::new();
        for (at, tet) in mesh.tets.iter().enumerate() {
            for slots in [[0usize, 1, 2], [0, 1, 3], [0, 2, 3], [1, 2, 3]] {
                let mut face = [tet[slots[0]], tet[slots[1]], tet[slots[2]]];
                face.sort_unstable();
                sides.entry(face).or_default().push(at);
            }
        }
        let inside_set = |at: usize| -> Vec<i32> {
            let mut out: Vec<i32> = mesh
                .records
                .get(at)
                .map(|r| {
                    r.entries
                        .iter()
                        .filter(|(_, side)| *side == Side::Inside)
                        .map(|(x, _)| *x)
                        .collect()
                })
                .unwrap_or_default();
            out.sort_unstable();
            out
        };
        let mut off_by_reason: BTreeMap<&'static str, (f64, usize)> = BTreeMap::new();
        let mut on_area = [0.0f64; 5];
        let mut off_area = [0.0f64; 5];
        let mut faces_by_path = [0usize; 5];
        // The material-boundary faces in key order, then each one's worst corner distance to
        // the nearest input triangle computed in parallel - a scan of every input triangle per
        // corner, and most of the gated S8's print-only cost on a8 - and read back in the same
        // order, so the sums below are the serial ones.
        let boundary_faces: Vec<(&[u32; 3], &SmallVec<[usize; 2]>)> = sides
            .iter()
            .filter(|(_, at)| at.len() == 2 && inside_set(at[0]) != inside_set(at[1]))
            .collect();
        let surface = crate::meshgen::verify::TriIndex::build(
            all_components
                .iter()
                .filter_map(|component| classifier.slot_of(*component))
                .flat_map(|slot| classifier.triangles_of(slot).iter().copied())
                .collect(),
        );
        let worsts: Vec<f64> = boundary_faces
            .par_iter()
            .map(|(face, _)| {
                let mut worst = 0.0f64;
                for node in face.iter() {
                    let q = mesh.nodes[*node as usize];
                    let best = surface.nearest(q).map_or(f64::INFINITY, |(d, _)| d);
                    worst = worst.max(best);
                }
                worst
            })
            .collect();
        for ((face, at), worst) in boundary_faces.iter().zip(worsts) {
            let p = [
                mesh.nodes[face[0] as usize],
                mesh.nodes[face[1] as usize],
                mesh.nodes[face[2] as usize],
            ];
            let cross = p[1].sub(p[0]).cross(p[2].sub(p[0]));
            let area = cross.dot(cross).sqrt() * 0.5;
            let mut longest = 0.0f64;
            for slot in 0..3 {
                let d = p[(slot + 1) % 3].sub(p[slot]);
                longest = longest.max(d.dot(d).sqrt());
            }
            let parent = mesh.parent_of.get(at[0]).copied().unwrap_or(0) as usize;
            let path = path_of.get(parent).copied().unwrap_or(4).min(4) as usize;
            faces_by_path[path] += 1;
            let off = worst > longest * 0.02;
            if off {
                off_area[path] += area;
                // **And which REFUSAL stranded it.** §6.46 established that the fan carries
                // essentially all of P3's damage; ranking the decline classes by the interface
                // area they strand - rather than by how many cells are in them - is what says
                // which to fix first. A class of few cells on the sharp edges can outweigh a
                // class of many in flat regions.
                if let Some(reason) = plc_decline.get(parent).copied().flatten() {
                    let entry = off_by_reason.entry(reason).or_insert((0.0, 0usize));
                    entry.0 += area;
                    entry.1 += 1;
                }
            } else {
                on_area[path] += area;
            }
        }
        let stranded: f64 = off_by_reason.values().map(|(a, _)| *a).sum();
        let mut ranked: Vec<(&'static str, (f64, usize))> =
            off_by_reason.into_iter().collect();
        ranked.sort_by(|a, b| b.1 .0.total_cmp(&a.1 .0));
        for (reason, (area, faces)) in ranked {
            mesh.warnings.push(format!(
                "[PLC] P3 stranded by refusal: {area:.4e} ({:.1} % of all stranded) over \
                 {faces} face(s) - {reason}",
                100.0 * area / stranded.max(f64::MIN_POSITIVE)
            ));
        }
        let total: f64 = off_area.iter().sum::<f64>() + on_area.iter().sum::<f64>();
        let name = [
            "§6's table",
            "§7.4-meshed",
            "§7.4-fanned (whole cell)",
            "escalated",
            "§7.4-fanned (facet-split)",
        ];
        for path in 0..5 {
            if faces_by_path[path] == 0 {
                continue;
            }
            let both = on_area[path] + off_area[path];
            mesh.warnings.push(format!(
                "[PLC] P3 by path: {} carries {} interface face(s), {:.4e} of {:.4e} total \
                 boundary area, of which {:.4e} is OFF the surface ({:.1} % of that arm, \
                 {:.1} % of all off-surface area)",
                name[path],
                faces_by_path[path],
                both,
                total,
                off_area[path],
                100.0 * off_area[path] / both.max(f64::MIN_POSITIVE),
                100.0 * off_area[path] / off_area.iter().sum::<f64>().max(f64::MIN_POSITIVE)
            ));
        }
    }

    s8_lap("t-area");
    // **And the same charge for the degenerate elements.** `[V1]` reads a signed volume and
    // says nothing about where it came from; normalising by the tet's own longest edge cubed
    // makes "flat" a scale-free statement, and the path histogram says which arm to look in.
    for (at, tet) in mesh.tets.iter().enumerate() {
        let p = [
            mesh.nodes[tet[0] as usize],
            mesh.nodes[tet[1] as usize],
            mesh.nodes[tet[2] as usize],
            mesh.nodes[tet[3] as usize],
        ];
        // **The same volume `[V1]` computes, which is the EXACT one.** A cross-and-dot in
        // double disagrees with it precisely in the regime this row is about, so charging the
        // paths with the fp formula reported zero inverted tets in a run where `[V1]` found 16.
        // A diagnostic that does not use the check's own arithmetic is measuring a different
        // question (PLAN §6.45).
        let volume = crate::meshgen::predicates::tet_signed_volume(p[0], p[1], p[2], p[3]);
        let mut longest = 0.0f64;
        for a in 0..4 {
            for b in a + 1..4 {
                let d = p[b].sub(p[a]);
                longest = longest.max(d.dot(d).sqrt());
            }
        }
        let shape = volume / longest.powi(3).max(f64::MIN_POSITIVE);
        let parent = mesh.parent_of.get(at).copied().unwrap_or(0) as usize;
        let path = path_of.get(parent).copied().unwrap_or(4).min(4) as usize;
        if shape <= 0.0 {
            inverted_by_path[path] += 1;
        } else if shape < 1.0e-9 {
            flat_by_path[path] += 1;
            worst_flat = worst_flat.min(shape);
        }
    }
    }
    s8_lap("t-flat");
    // **Detect and repair, to a fixed point.** One pass is not enough: a node hangs on several
    // faces, and repairing one changes the ring the next is judged against - so the pass is run
    // again until it stops finding anything. Exclusion only ever removes hanging nodes here, so
    // the loop is decreasing and the budget bounds it rather than guessing at it.
    // **Which two arms computed the same point?** Print-only. `[V2]` names the duplicates and
    // `[V3]` reports one of each pair hanging on the other's faces; what neither says is where
    // they came from, and that is what decides where the identity rule belongs. Measured on a3:
    // 368 pairs within `eps`, of which 312 are trace-endpoint against trace-endpoint - one arm
    // computing one intersection twice, and the pairs are 2.98e-8 apart, one float32 ULP at 0.5
    // (PLAN §6.60).
    if std::env::var_os("RUSTMSPT_PLC_DIAG").is_some() {
        origin.resize(mesh.nodes.len(), "unknown");
        let coarse = quantum.max(f64::MIN_POSITIVE);
        let cell = |p: Vec3| {
            (
                (p.x / coarse).floor() as i64,
                (p.y / coarse).floor() as i64,
                (p.z / coarse).floor() as i64,
            )
        };
        let mut grid: BTreeMap<(i64, i64, i64), Vec<u32>> = BTreeMap::new();
        for (id, point) in mesh.nodes.iter().enumerate() {
            grid.entry(cell(*point)).or_default().push(id as u32);
        }
        let mut by_arms: BTreeMap<(&'static str, &'static str), usize> = BTreeMap::new();
        let mut by_relation: BTreeMap<&'static str, usize> = BTreeMap::new();
        let mut closest: Vec<(u64, u32, u32)> = Vec::new();
        for (id, point) in mesh.nodes.iter().enumerate() {
            let home = cell(*point);
            for dx in -1..=1 {
                for dy in -1..=1 {
                    for dz in -1..=1 {
                        let key = (home.0 + dx, home.1 + dy, home.2 + dz);
                        for other in grid.get(&key).into_iter().flatten() {
                            if *other as usize <= id {
                                continue;
                            }
                            let d = mesh.nodes[*other as usize].sub(*point);
                            let d = d.dot(d).sqrt();
                            if d > coarse {
                                continue;
                            }
                            let (a, b) = (origin[id], origin[*other as usize]);
                            let pair = if a <= b { (a, b) } else { (b, a) };
                            *by_arms.entry(pair).or_insert(0) += 1;
                            closest.push((d.to_bits(), id as u32, *other));
                            // **What do the two faces share?** The strongest relation any pair
                            // of their faces has: the same face, an edge, a corner, or nothing.
                            let (here, there) = (
                                trace_faces.get(&(id as u32)),
                                trace_faces.get(other),
                            );
                            if let (Some(here), Some(there)) = (here, there) {
                                let mut best = 0usize;
                                for f in here {
                                    for g in there {
                                        let shared =
                                            f.iter().filter(|c| g.contains(c)).count();
                                        best = best.max(shared);
                                    }
                                }
                                let label = match best {
                                    3 => "same face",
                                    2 => "faces share an edge",
                                    1 => "faces share a corner",
                                    _ => "faces share nothing",
                                };
                                *by_relation.entry(label).or_insert(0) += 1;
                            }
                        }
                    }
                }
            }
        }
        closest.sort_unstable();
        let total: usize = by_arms.values().sum();
        mesh.warnings.push(format!(
            "[PLC] coincident node pairs within eps ({coarse:.3e}): {total}, by the arms that \
             interned them: {by_arms:?}; and for the trace-endpoint pairs, by what their faces \
             share: {by_relation:?}"
        ));
        for (bits, a, b) in closest.iter().take(6) {
            mesh.warnings.push(format!(
                "[PLC]   pair {a} ({}) / {b} ({}) at {:.4e}; faces {:?} vs {:?}; at {:?} and {:?}",
                origin[*a as usize],
                origin[*b as usize],
                f64::from_bits(*bits),
                trace_faces.get(a).map(|f| f.iter().copied().collect::<Vec<_>>()),
                trace_faces.get(b).map(|f| f.iter().copied().collect::<Vec<_>>()),
                mesh.nodes[*a as usize],
                mesh.nodes[*b as usize],
            ));
        }
    }
    s8_lap("t-plcdiag");
    // **A repaired state the pass has already seen means the repair is undoing itself.** The
    // measured case is a pair of node chains between the same two endpoints, 1.85e-7 apart:
    // splitting an edge of one puts the other chain's node on the face that split just made,
    // and splitting that one puts the first back. No split can separate two spellings of one
    // point, so the pass stops on the repeat rather than spending its budget oscillating - and
    // on the BEST phase of the cycle, not whichever one it was caught in, so the result is the
    // same every run instead of depending on where the round budget happened to fall.
    let mut seen: BTreeSet<(Vec<u32>, usize, usize)> = BTreeSet::new();
    let mut fewest = usize::MAX;
    for _round in 0..12 {
        let mut absorbed = 0usize;
    // **Find the T-junctions the way `[V3]` reports them**, with `[V3]`'s own test: a node
        // within `1e-9 x diagonal` of a face it is not a vertex of. Every attempt to prevent these
        // before the mesh exists has either moved geometry or excluded cells (PLAN §6.56), so they
        // are found here instead - and the detector reproduces the verifier's count before any
        // repair is built, because a repair aimed at a proxy fixes the proxy.
        //
        // The first version of this asked for `orient3d == 0` exactly and found NOTHING, which was
        // the useful failure: these nodes are not exactly on the face, they are a few times 1e-12
        // off it. The bound is the verifier's, not a new one.
        {
            let mut lo = Vec3::new(f64::INFINITY, f64::INFINITY, f64::INFINITY);
            let mut hi = Vec3::new(f64::NEG_INFINITY, f64::NEG_INFINITY, f64::NEG_INFINITY);
            for p in &mesh.nodes {
                lo = Vec3::new(lo.x.min(p.x), lo.y.min(p.y), lo.z.min(p.z));
                hi = Vec3::new(hi.x.max(p.x), hi.y.max(p.y), hi.z.max(p.z));
            }
            let diagonal = hi.sub(lo);
            let tol = 1.0e-9 * diagonal.dot(diagonal).sqrt();
            let step = (diagonal.dot(diagonal).sqrt() / (mesh.nodes.len() as f64).cbrt().max(1.0))
                .max(f64::MIN_POSITIVE);
            let at = |p: Vec3| {
                (
                    (p.x / step).floor() as i64,
                    (p.y / step).floor() as i64,
                    (p.z / step).floor() as i64,
                )
            };
            let mut grid: std::collections::HashMap<(i64, i64, i64), Vec<u32>> =
                std::collections::HashMap::new();
            for (id, p) in mesh.nodes.iter().enumerate() {
                grid.entry(at(*p)).or_default().push(id as u32);
            }
            let mut faces: Vec<[u32; 3]> = mesh
                .tets
                .par_iter()
                .flat_map_iter(|tet| {
                    [
                        [tet[1], tet[2], tet[3]],
                        [tet[0], tet[2], tet[3]],
                        [tet[0], tet[1], tet[3]],
                        [tet[0], tet[1], tet[2]],
                    ]
                    .into_iter()
                    .map(|mut key| {
                        key.sort_unstable();
                        key
                    })
                })
                .collect();
            faces.par_sort_unstable();
            faces.dedup();
            // Per face, the nodes within `tol` of it (ascending id) and whether each sits on the
            // face's rim. The repair below walks exactly these pairs, in (face, node) order.
            let near_face: Vec<Vec<(u32, bool)>> = faces
                .par_iter()
                .map(|face| {
                    let p = [
                        mesh.nodes[face[0] as usize],
                        mesh.nodes[face[1] as usize],
                        mesh.nodes[face[2] as usize],
                    ];
                    let bl = Vec3::new(
                        p.iter().map(|q| q.x).fold(f64::INFINITY, f64::min) - tol,
                        p.iter().map(|q| q.y).fold(f64::INFINITY, f64::min) - tol,
                        p.iter().map(|q| q.z).fold(f64::INFINITY, f64::min) - tol,
                    );
                    let br = Vec3::new(
                        p.iter().map(|q| q.x).fold(f64::NEG_INFINITY, f64::max) + tol,
                        p.iter().map(|q| q.y).fold(f64::NEG_INFINITY, f64::max) + tol,
                        p.iter().map(|q| q.z).fold(f64::NEG_INFINITY, f64::max) + tol,
                    );
                    let (l, h) = (at(bl), at(br));
                    let mut here: Vec<(u32, bool)> = Vec::new();
                    for gx in l.0..=h.0 {
                        for gy in l.1..=h.1 {
                            for gz in l.2..=h.2 {
                                for id in grid.get(&(gx, gy, gz)).into_iter().flatten() {
                                    if face.contains(id) {
                                        continue;
                                    }
                                    let q = mesh.nodes[*id as usize];
                                    if crate::meshgen::verify::point_triangle_dist2(q, p[0], p[1], p[2])
                                        > tol * tol
                                    {
                                        continue;
                                    }
                                    // On the face's RIM the repair is an edge split - every tet
                                    // around that edge takes the node - and strictly inside it is a
                                    // face split, which touches only the two tets sharing it.
                                    let rim = (0..3).any(|slot| {
                                        let (u, v) = (p[slot], p[(slot + 1) % 3]);
                                        let along = v.sub(u);
                                        let len2 = along.dot(along);
                                        if len2 <= 0.0 {
                                            return false;
                                        }
                                        let t = (q.sub(u).dot(along) / len2).clamp(0.0, 1.0);
                                        let off = q.sub(u).sub(along.scale(t));
                                        off.dot(off) <= tol * tol
                                    });
                                    here.push((*id, rim));
                                }
                            }
                        }
                    }
                    here.sort_unstable();
                    here
                })
                .collect();
            let mut hanging: BTreeSet<u32> = BTreeSet::new();
            let mut incidences = 0usize;
            let mut on_rim = 0usize;
            for here in &near_face {
                for (id, rim) in here {
                    hanging.insert(*id);
                    incidences += 1;
                    if *rim {
                        on_rim += 1;
                    }
                }
            }
            if !hanging.is_empty() {
                mesh.warnings.push(format!(
                    "[PLC] T-junction detector: {} node(s) hang, over {incidences} (node, face) \
                     incidence(s), of which {on_rim} put the node on the face's RIM - an edge split \
                     - and the rest strictly inside it, a face split",
                    hanging.len()
                ));
            }
            // Print-only: where each hanging node is, the face it hangs on and which cells emit
            // the tets carrying the node and the face - so a J1 gap can be traced to the two
            // cells that disagree about one edge.
            if std::env::var_os("RUSTMSPT_PLC_DIAG").is_some() && _round == 0 {
                let mut shown = 0usize;
                for (face, here) in faces.iter().zip(&near_face) {
                    for (id, rim) in here {
                        if shown >= 12 {
                            break;
                        }
                        shown += 1;
                        let carriers = |n: u32| -> Vec<u32> {
                            let mut cells: Vec<u32> = mesh
                                .tets
                                .iter()
                                .enumerate()
                                .filter(|(_, t)| t.contains(&n))
                                .map(|(at, _)| mesh.parent_of[at])
                                .collect();
                            cells.sort_unstable();
                            cells.dedup();
                            cells
                        };
                        let face_cells: Vec<u32> = {
                            let mut cells: Vec<u32> = mesh
                                .tets
                                .iter()
                                .enumerate()
                                .filter(|(_, t)| face.iter().all(|n| t.contains(n)))
                                .map(|(at, _)| mesh.parent_of[at])
                                .collect();
                            cells.sort_unstable();
                            cells
                        };
                        if shown == 1 {
                            for n in face {
                                eprintln!(
                                    "[TJ-NODE]   face node {n} ({}) lattice {} at {:?}",
                                    origin.get(*n as usize).copied().unwrap_or("?"),
                                    *n < n_parent_nodes,
                                    mesh.nodes[*n as usize]
                                );
                            }
                            for cell in carriers(*id).iter().chain(face_cells.iter()) {
                                let corners = lattice.tets[*cell as usize];
                                eprintln!(
                                    "[TJ-NODE]   cell {cell} corners {:?} at {:?}",
                                    corners,
                                    corners.map(|c| mesh.nodes[c as usize])
                                );
                            }
                        }
                        eprintln!(
                            "[TJ-NODE] node {id} ({}) at {:?} on face {:?} rim {rim}; node in cells {:?}, face in cells {:?}",
                            origin.get(*id as usize).copied().unwrap_or("?"),
                            mesh.nodes[*id as usize],
                            face,
                            carriers(*id),
                            face_cells
                        );
                    }
                }
            }
            let repeated =
                !seen.insert((hanging.iter().copied().collect(), incidences, on_rim));
            fewest = fewest.min(incidences);
            if repeated && incidences <= fewest {
                mesh.warnings.push(format!(
                    "[PLC] T-junction repair: STOPPED on a state already seen - {} node(s) over \
                     {incidences} incidence(s) came back after being repaired. A node that \
                     returns is not a T-junction: it is one point the cut spelled twice, and a \
                     split cannot separate two spellings",
                    hanging.len()
                ));
                break;
            }

            // **The repair: replace the flat sliver AND its neighbour together.**
            //
            // The configuration is exact, and measuring is what found it. A hanging node `q` sits on
            // a face `F = (a, b, c)`; `F` is carried by two tets, and one of them is `U = F + {q}` -
            // a tet whose fourth vertex lies on its own base plane to about 1e-12, so a sliver of
            // essentially no volume. The other, `T = F + {d}`, is the one that does not have `q`,
            // and that is why `q` hangs on it.
            //
            // Splitting `T` alone fails, and the failure is instructive: it creates `(a, b, q)`,
            // `(b, c, q)`, `(c, a, q)` - which are already `U`'s faces - so those faces acquire a
            // third owner (a8: 0 multi-shared faces -> 22). Replacing **both** tets by the same three
            // pieces is exactly conforming instead:
            //
            //   before   T: abc abd acd bcd      U: abc abq acq bcq
            //   after    (a,b,q,d) (b,c,q,d) (c,a,q,d)
            //            -> abd acd bcd kept, abq acq bcq kept, abc gone with BOTH its owners,
            //               and (a,q,d) (b,q,d) (c,q,d) new and internal
            //
            // and the volumes sum, because `U` had none. **Nothing moves**, which removes the other
            // hazard entirely - a node is a vertex of every tet around it, and displacing it is a
            // global edit with a local motive.
            //
            // `U` is overwritten in place rather than deleted, so every tet index already recorded
            // elsewhere stays valid, and it inherits `T`'s record: it had no volume to own.
            let mut present: BTreeSet<[u32; 4]> = mesh
                .tets
                .iter()
                .map(|tet| {
                    let mut key = *tet;
                    key.sort_unstable();
                    key
                })
                .collect();
            let mut repaired = 0usize;
            let mut refused_shape = 0usize;
            let mut refused_rim = 0usize;
            let mut refused_tagged = 0usize;
        let (mut why_clash, mut why_degenerate) = (0usize, 0usize);
        let (mut why_no_carrier, mut why_all_slivers) = (0usize, 0usize);
            let tagged: BTreeSet<[u32; 3]> = pending_interfaces
                .iter()
                .map(|(_, triangle, _)| {
                    let mut key = *triangle;
                    key.sort_unstable();
                    key
                })
                .collect();
            for (face, here) in faces.iter().zip(&near_face) {
                if here.is_empty() {
                    continue;
                }
                let p = [
                    mesh.nodes[face[0] as usize],
                    mesh.nodes[face[1] as usize],
                    mesh.nodes[face[2] as usize],
                ];
                for &(id, _) in here {
                    let q = mesh.nodes[id as usize];
                    if crate::meshgen::verify::point_triangle_dist2(q, p[0], p[1], p[2]) > tol * tol
                    {
                        continue;
                    }
                    // On the face's RIM the node is on an EDGE, shared by every tet around it, and
                    // one of the three pieces would have three collinear vertices. That is a
                    // different operation and it is not this one.
                    let mut rim = None;
                    for slot in 0..3 {
                        let (u, v) = (p[slot], p[(slot + 1) % 3]);
                        let along = v.sub(u);
                        let len2 = along.dot(along);
                        if len2 <= 0.0 {
                            continue;
                        }
                        let t = (q.sub(u).dot(along) / len2).clamp(0.0, 1.0);
                        let off = q.sub(u).sub(along.scale(t));
                        if off.dot(off) <= tol * tol {
                            let (x, y) = (face[slot], face[(slot + 1) % 3]);
                            rim = Some(if x <= y { [x, y] } else { [y, x] });
                            break;
                        }
                    }
                    // **On the rim the node is on an EDGE, and an edge belongs to every tet around
                    // it.** Splitting one face's two owners would leave the rest of the ring holding
                    // the edge whole. So the whole ring is taken at once: a tet that does not have
                    // the node splits in two, and a tet that DOES is a zero-volume wedge - its three
                    // vertices `a`, `b`, `q` are collinear - which cannot be split and is removed.
                    // Removing it is exactly right rather than merely tolerable: its faces
                    // `(a, q, z)` and `(q, b, z)` tile `(a, b, z)`, which is the face the split of
                    // its neighbour produces on the other side, so the two halves meet.
                    if let Some(edge) = rim {
                        if tagged.contains(face) {
                            refused_tagged += 1;
                            continue;
                        }
                        let ring: Vec<usize> = (0..mesh.tets.len())
                            .filter(|at| {
                                mesh.tets[*at].contains(&edge[0]) && mesh.tets[*at].contains(&edge[1])
                            })
                            .collect();
                        let wedges: Vec<usize> = ring
                            .iter()
                            .copied()
                            .filter(|at| mesh.tets[*at].contains(&id))
                            .collect();
                        let solids: Vec<usize> = ring
                            .iter()
                            .copied()
                            .filter(|at| !mesh.tets[*at].contains(&id))
                            .collect();
                        if wedges.is_empty() || solids.is_empty() {
                            refused_rim += 1;
                            continue;
                        }
                        let mut plan: Vec<(usize, [u32; 4])> = Vec::new();
                        let mut ok = true;
                        for at in &solids {
                            let tet = mesh.tets[*at];
                            let rest: Vec<u32> = tet
                                .iter()
                                .copied()
                                .filter(|x| *x != edge[0] && *x != edge[1])
                                .collect();
                            if rest.len() != 2 {
                                ok = false;
                                break;
                            }
                            for piece in [
                                [edge[0], id, rest[0], rest[1]],
                                [id, edge[1], rest[0], rest[1]],
                            ] {
                                match orient_positively(piece, &mesh.nodes) {
                                    Some(good) => {
                                        let mut key = good;
                                        key.sort_unstable();
                                        if present.contains(&key) {
                                            ok = false;
                                        }
                                        plan.push((*at, good));
                                    }
                                    None => ok = false,
                                }
                            }
                            if !ok {
                                break;
                            }
                        }
                        // Every wedge slot is reused and every solid is replaced, so the ring's
                        // element count goes from `solids + wedges` to `2 x solids`; anything left
                        // over is appended, which keeps all recorded tet indices valid.
                        if !ok || plan.len() < ring.len() {
                            refused_rim += 1;
                            continue;
                        }
                        for at in &ring {
                            let mut key = mesh.tets[*at];
                            key.sort_unstable();
                            present.remove(&key);
                        }
                        let mut slots: Vec<usize> = wedges.clone();
                        slots.extend(solids.iter().copied());
                        for (slot, (source, piece)) in plan.iter().enumerate() {
                            let mut key = *piece;
                            key.sort_unstable();
                            present.insert(key);
                            match slots.get(slot) {
                                Some(at) => {
                                    mesh.tets[*at] = *piece;
                                    mesh.records[*at] = mesh.records[*source].clone();
                                    mesh.parent_of[*at] = mesh.parent_of[*source];
                                    mesh.regime[*at] = mesh.regime[*source];
                                    mesh.band_region[*at] = mesh.band_region[*source];
                                }
                                None => {
                                    mesh.tets.push(*piece);
                                    let record = mesh.records[*source].clone();
                                    mesh.records.push(record);
                                    let parent = mesh.parent_of[*source];
                                    mesh.parent_of.push(parent);
                                    let regime = mesh.regime[*source];
                                    mesh.regime.push(regime);
                                    let band = mesh.band_region[*source];
                                    mesh.band_region.push(band);
                                }
                            }
                        }
                        repaired += 1;
                absorbed += 1;
                        continue;
                    }
                    if tagged.contains(face) {
                        refused_tagged += 1;
                        continue;
                    }
                    let carriers: Vec<usize> = (0..mesh.tets.len())
                        .filter(|at| face.iter().all(|n| mesh.tets[*at].contains(n)))
                        .collect();
                    // **Every carrier that lacks the node splits into three; a carrier that
                    // HAS it is the flat sliver and is absorbed.** One rule rather than three
                    // cases, because the mesh presents all three: a face with one sliver and one
                    // solid, a face whose BOTH owners lack the node - `q` is then a vertex of
                    // some tet further out and both sides simply split - and a hull face with a
                    // single owner. The pieces go back into the carriers' own slots and the
                    // remainder is appended, so a sliver disappears without any tet index moving.
                    let mut plan: Vec<(usize, [u32; 4])> = Vec::new();
                    let mut ok = !carriers.is_empty();
                    for at in &carriers {
                        let tet = mesh.tets[*at];
                        if tet.contains(&id) {
                            continue;
                        }
                        let Some(apex) = tet.iter().copied().find(|x| !face.contains(x)) else {
                            ok = false;
                            break;
                        };
                        for piece in [
                            [face[0], face[1], id, apex],
                            [face[1], face[2], id, apex],
                            [face[2], face[0], id, apex],
                        ] {
                            match orient_positively(piece, &mesh.nodes) {
                                Some(good) => {
                                    // **A piece the mesh already has is a refusal, not a piece to
                                    // skip.** Skipping it and writing only the rest was tried:
                                    // it leaves a hole, because the tet already there does not
                                    // cover the same region - a3 went from 0 boundary leaks to 6
                                    // and 0 non-manifold edges to 15. Whatever that
                                    // configuration is, this operation is not for it.
                                    let mut key = good;
                                    key.sort_unstable();
                                    if present.contains(&key) {
                                        ok = false;
                                        why_clash += 1;
                                    }
                                    plan.push((*at, good));
                                }
                                None => {
                                    ok = false;
                                    why_degenerate += 1;
                                }
                            }
                        }
                        if !ok {
                            break;
                        }
                    }
                    // Fewer pieces than carriers would leave a slot with nothing to put in it,
                    // and a tet cannot be deleted here without moving every index after it.
                    if carriers.is_empty() {
                        why_no_carrier += 1;
                    } else if ok && plan.len() < carriers.len() {
                        why_all_slivers += 1;
                    }
                    if !ok || plan.len() < carriers.len() {
                        refused_shape += 1;
                        continue;
                    }
                    for at in &carriers {
                        let mut key = mesh.tets[*at];
                        key.sort_unstable();
                        present.remove(&key);
                    }
                    for (slot, (source, piece)) in plan.iter().enumerate() {
                        let mut key = *piece;
                        key.sort_unstable();
                        present.insert(key);
                        match carriers.get(slot) {
                            Some(at) => {
                                mesh.tets[*at] = *piece;
                                mesh.records[*at] = mesh.records[*source].clone();
                                mesh.parent_of[*at] = mesh.parent_of[*source];
                                mesh.regime[*at] = mesh.regime[*source];
                                mesh.band_region[*at] = mesh.band_region[*source];
                            }
                            None => {
                                mesh.tets.push(*piece);
                                let record = mesh.records[*source].clone();
                                mesh.records.push(record);
                                let parent = mesh.parent_of[*source];
                                mesh.parent_of.push(parent);
                                let regime = mesh.regime[*source];
                                mesh.regime.push(regime);
                                let band = mesh.band_region[*source];
                                mesh.band_region.push(band);
                            }
                        }
                    }
                    repaired += 1;
                    absorbed += 1;
                }
            }
            if repaired + refused_shape + refused_rim + refused_tagged > 0 {
                mesh.warnings.push(format!(
                    "[PLC] T-junction repair: {repaired} node(s) absorbed by replacing the flat \
                     sliver and its neighbour with three tets; refused {refused_rim} on a face's \
                     rim (an edge, not a face), {refused_tagged} on a tagged interface face and \
                     {refused_shape} for shape - {why_clash} because a piece is already in the \
                 mesh, {why_degenerate} because a piece is flat, {why_no_carrier} with no \
                 carrier and {why_all_slivers} where every carrier already has the node"
                ));
            }
            // An earlier version of this split only the tets that own the face, leaving `U` to
            // claim `(a, b, q)` as a third owner - a8 went to 22 multi-shared faces. Replacing both
            // is what makes it conforming, and the duplicate guard above is what makes it safe where
            // the cavity is not the clean pair.
        }

        if absorbed == 0 {
            break;
        }
    }
    s8_lap("t-repair");
    if plc_diag {
    // **Charge each BAD element to the arm that emitted it**, on the measure `[V4]` actually
    // reports: the smallest dihedral angle. The goal names three properties and this is the
    // third - *no bad elements* - and it is the only one the gated path still loses on. The
    // technique is the one that found every previous cause; the buckets are `[V4]`'s own floor
    // (5°) and its share gate (10°) so the row can be read against the verifier without
    // translation.
    {
        let mut band = [[0usize; 4]; 5];
        let mut worst = [180.0f64; 5];
        let mut inherited = [0usize; 5];
        let mut genuine = [0usize; 5];
        for (at, tet) in mesh.tets.iter().enumerate() {
            let p = [
                mesh.nodes[tet[0] as usize],
                mesh.nodes[tet[1] as usize],
                mesh.nodes[tet[2] as usize],
                mesh.nodes[tet[3] as usize],
            ];
            // The dihedral on edge (a, b) is the angle between the two faces meeting there,
            // read off their outward normals.
            let mut least = 180.0f64;
            for (a, b, c, d) in [
                (0, 1, 2, 3),
                (0, 2, 1, 3),
                (0, 3, 1, 2),
                (1, 2, 0, 3),
                (1, 3, 0, 2),
                (2, 3, 0, 1),
            ] {
                let axis = p[b].sub(p[a]);
                let length = axis.dot(axis).sqrt();
                if length <= 0.0 {
                    continue;
                }
                let axis = axis.scale(1.0 / length);
                let drop = |q: Vec3| {
                    let rel = q.sub(p[a]);
                    rel.sub(axis.scale(rel.dot(axis)))
                };
                let (u, v) = (drop(p[c]), drop(p[d]));
                let (nu, nv) = (u.dot(u).sqrt(), v.dot(v).sqrt());
                if nu <= 0.0 || nv <= 0.0 {
                    continue;
                }
                let cosine = (u.dot(v) / (nu * nv)).clamp(-1.0, 1.0);
                least = least.min(cosine.acos().to_degrees());
            }
            // **Is the badness created in 3D, or inherited from a thin triangle already on a
            // face?** A tet whose worst dihedral is small AND one of whose faces is already a
            // needle in 2D cannot be fixed by any three-dimensional operation - the face is
            // shared with a neighbour and frozen by J1, so the cure would have to be at the face
            // level. A tet with four well-shaped faces and a small dihedral is a true sliver,
            // which is the case Steiner points and exudation are for. The two want opposite
            // work, so they are counted apart.
            let mut face_least = 180.0f64;
            for tri in [[0usize, 1, 2], [0, 1, 3], [0, 2, 3], [1, 2, 3]] {
                let q = [p[tri[0]], p[tri[1]], p[tri[2]]];
                for corner in 0..3 {
                    let u = q[(corner + 1) % 3].sub(q[corner]);
                    let v = q[(corner + 2) % 3].sub(q[corner]);
                    let (nu, nv) = (u.dot(u).sqrt(), v.dot(v).sqrt());
                    if nu <= 0.0 || nv <= 0.0 {
                        face_least = 0.0;
                        continue;
                    }
                    face_least = face_least
                        .min((u.dot(v) / (nu * nv)).clamp(-1.0, 1.0).acos().to_degrees());
                }
            }
            let parent = mesh.parent_of.get(at).copied().unwrap_or(0) as usize;
            let path = path_of.get(parent).copied().unwrap_or(4).min(4) as usize;
            if least < 10.0 {
                if face_least < 10.0 {
                    inherited[path] += 1;
                } else {
                    genuine[path] += 1;
                }
            }
            let slot = if least < 1.0 {
                0
            } else if least < 5.0 {
                1
            } else if least < 10.0 {
                2
            } else {
                3
            };
            band[path][slot] += 1;
            worst[path] = worst[path].min(least);
        }
        let name = [
            "§6's table",
            "§7.4-meshed",
            "§7.4-fanned (whole cell)",
            "escalated",
            "§7.4-fanned (facet-split)",
        ];
        for path in 0..5 {
            let total: usize = band[path].iter().sum();
            if total == 0 {
                continue;
            }
            let bad = band[path][0] + band[path][1] + band[path][2];
            mesh.warnings.push(format!(
                "[PLC] dihedral by path: {} has {total} tet(s), {bad} below 10° ({:.2} %) - \
                 {} under 1°, {} in 1-5°, {} in 5-10°; worst {:.4}°. Of the bad ones {} \
                 INHERIT a needle face ({:.1} %) and {} are true slivers with four good faces",
                name[path],
                100.0 * bad as f64 / total as f64,
                band[path][0],
                band[path][1],
                band[path][2],
                worst[path],
                inherited[path],
                100.0 * inherited[path] as f64 / bad.max(1) as f64,
                genuine[path]
            ));
        }
    }
    mesh.warnings.push(format!(
        "[PLC] tets with volume/longest-edge^3 at or below zero: {} §6's table, {} \
         §7.4-meshed, {} §7.4-fanned, {} escalated, {} other; and merely flat (< 1e-9): {} / \
         {} / {} / {} / {}, worst {worst_flat:.3e}",
        inverted_by_path[0],
        inverted_by_path[1],
        inverted_by_path[2],
        inverted_by_path[3],
        inverted_by_path[4],
        flat_by_path[0],
        flat_by_path[1],
        flat_by_path[2],
        flat_by_path[3],
        flat_by_path[4]
    ));
    mesh.warnings.push(format!(
        "[PLC] of the boundaries §7.4 cells were handed: {unmatched} triangle(s) are carried by \
         fewer tets than the cells that promised them, {unmatched_augmented} of those on an \
         augmented face - so the rest are faces §7.4 left to §5.2"
    ));
    }
}
