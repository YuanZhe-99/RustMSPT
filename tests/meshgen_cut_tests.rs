//! G6-2/G6-3 acceptance tests: Rule SNK and Theorem T2, the frozen prism table,
//! the §5.2 face-split table, the §6 tet case table, the conformity claim that ties
//! the two together, the guarded dry-run, the derived interface index, and `s08_cut`.

use rustmspt::meshgen::cut::{
    cut_tet, face_split, guarded_dry_run, orient_positively, prism_tets, prism_tets_with_diagonals,
    snk_split_quad, CutOptions, FaceCutState, NodeKey, NodeSide,
};
use rustmspt::meshgen::predicates::node_key;
use rustmspt::types::Vec3;
use std::collections::BTreeSet;

const QUANTUM: f64 = 1.0e-9;

// AI-FUNC-SUMMARY: A reproducible pseudo-random stream; returns a closure yielding [0,1); side effects: mutates its own state.
fn rng(seed: u64) -> impl FnMut() -> f64 {
    let mut state = seed.wrapping_mul(6364136223846793005).wrapping_add(1);
    move || {
        state = state
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        ((state >> 11) as f64) / ((1u64 << 53) as f64)
    }
}

// AI-FUNC-SUMMARY: Node keys for a point list; returns the key table; side effects: none.
fn keys_of(points: &[Vec3]) -> Vec<NodeKey> {
    points.iter().map(|p| node_key(*p, QUANTUM)).collect()
}

// AI-FUNC-SUMMARY: Signed volume of a tet from a point table; returns f64; side effects: none.
fn volume(tet: [u32; 4], points: &[Vec3]) -> f64 {
    let a = points[tet[0] as usize];
    let u = points[tet[1] as usize].sub(a);
    let v = points[tet[2] as usize].sub(a);
    let w = points[tet[3] as usize].sub(a);
    u.cross(v).dot(w) / 6.0
}

// AI-FUNC-SUMMARY: A triangle as a sorted node triple, so triangle sets compare regardless of winding; returns [u32;3]; side effects: none.
fn canonical(triangle: [u32; 3]) -> [u32; 3] {
    let mut out = triangle;
    out.sort_unstable();
    out
}

#[test]
// AI-FUNC-SUMMARY: Rule SNK depends only on the node keys, so every presentation of one quad agrees.
fn snk_is_a_function_of_the_keys_alone() {
    let points = vec![
        Vec3::new(0.0, 0.0, 0.0),
        Vec3::new(1.0, 0.0, 0.0),
        Vec3::new(1.0, 1.0, 0.0),
        Vec3::new(0.0, 1.0, 0.0),
    ];
    let keys = keys_of(&points);
    let reference: BTreeSet<[u32; 3]> = snk_split_quad([0, 1, 2, 3], &keys)
        .into_iter()
        .map(canonical)
        .collect();
    // Every rotation and both directions describe the same quad.
    for rotation in 0..4u32 {
        let forward = [
            rotation % 4,
            (rotation + 1) % 4,
            (rotation + 2) % 4,
            (rotation + 3) % 4,
        ];
        let backward = [forward[0], forward[3], forward[2], forward[1]];
        for quad in [forward, backward] {
            let split: BTreeSet<[u32; 3]> = snk_split_quad(quad, &keys)
                .into_iter()
                .map(canonical)
                .collect();
            assert_eq!(split, reference, "quad {quad:?} disagreed with the reference");
        }
    }
}

#[test]
// AI-FUNC-SUMMARY: Theorem T2 - Rule SNK never produces a cyclic diagonal set, over 20,000 key orders.
fn snk_never_produces_a_cyclic_prism() {
    let mut next = rng(0x7212);
    for _ in 0..20_000 {
        // Random *keys* are what the theorem is about, so the coordinates are
        // random too and the geometry is irrelevant.
        let points: Vec<Vec3> = (0..6)
            .map(|_| Vec3::new(next(), next(), next()))
            .collect();
        let keys = keys_of(&points);
        assert!(
            prism_tets([0, 1, 2], [3, 4, 5], &keys).is_some(),
            "SNK produced a cyclic set on keys {keys:?}"
        );
    }
}

#[test]
// AI-FUNC-SUMMARY: The two cyclic diagonal sets are the only ones the frozen table rejects.
fn only_the_two_cyclic_sets_are_undecomposable() {
    let mut rejected = 0usize;
    for mask in 0..8u8 {
        let diagonals = [mask & 1 != 0, mask & 2 != 0, mask & 4 != 0];
        if prism_tets_with_diagonals([0, 1, 2], [3, 4, 5], diagonals).is_none() {
            rejected += 1;
            assert!(
                diagonals == [true, true, true] || diagonals == [false, false, false],
                "{diagonals:?} was rejected but is not cyclic"
            );
        }
    }
    assert_eq!(rejected, 2, "the frozen table has exactly two cyclic rows");
}

#[test]
// AI-FUNC-SUMMARY:
// The three prism tets tile the prism: their union's boundary is exactly the prism's own faces,
// with every interior face shared by two of them.
//
// This is a combinatorial check, deliberately. A volume comparison needs an independent volume for
// the prism, and there is no independent volume - a prism with non-planar quads is only a solid
// *once its diagonals are chosen*, so any reference decomposition would be the thing under test.
// The boundary identity has no such circularity: if the three tets tile the prism, every internal
// face cancels in pairs and what survives is the prism's boundary triangulation.
fn the_prism_table_tiles_the_prism() {
    let mut next = rng(0x4203);
    for _ in 0..4_000 {
        let cap: Vec<Vec3> = (0..3)
            .map(|_| Vec3::new(next() * 2.0 - 1.0, next() * 2.0 - 1.0, 0.0))
            .collect();
        let sweep = Vec3::new(next() * 0.4 - 0.2, next() * 0.4 - 0.2, 1.0 + next());
        let mut points = cap.clone();
        for corner in &cap {
            // Per-vertex jitter, so the quads are genuinely non-planar - the case
            // §4.2 exists for.
            points.push(corner.add(sweep).add(Vec3::new(
                (next() - 0.5) * 0.1,
                (next() - 0.5) * 0.1,
                (next() - 0.5) * 0.1,
            )));
        }
        let keys = keys_of(&points);
        let tets = prism_tets([0, 1, 2], [3, 4, 5], &keys).expect("T2 violated");

        let mut seen: std::collections::BTreeMap<[u32; 3], usize> =
            std::collections::BTreeMap::new();
        for tet in &tets {
            for triangle in [
                [tet[0], tet[1], tet[2]],
                [tet[0], tet[1], tet[3]],
                [tet[0], tet[2], tet[3]],
                [tet[1], tet[2], tet[3]],
            ] {
                *seen.entry(canonical(triangle)).or_insert(0) += 1;
            }
        }
        let boundary: BTreeSet<[u32; 3]> = seen
            .iter()
            .filter(|(_, count)| **count == 1)
            .map(|(triangle, _)| *triangle)
            .collect();
        let interior = seen.values().filter(|count| **count == 2).count();
        assert!(
            seen.values().all(|count| *count <= 2),
            "a face is shared by three tets - they overlap"
        );
        assert_eq!(interior, 2, "a 3-tet prism has exactly two interior faces");

        let mut expected: BTreeSet<[u32; 3]> = BTreeSet::new();
        expected.insert(canonical([0, 1, 2]));
        expected.insert(canonical([3, 4, 5]));
        for (i, j) in [(0u32, 1u32), (1, 2), (2, 0)] {
            for triangle in snk_split_quad([i, j, j + 3, i + 3], &keys) {
                expected.insert(canonical(triangle));
            }
        }
        assert_eq!(
            boundary, expected,
            "the union's boundary is not the prism's own faces"
        );

        // Volumes are deliberately *not* asserted here. §4.4 says in as many
        // words that combinatorial validity does not imply positive volumes: a
        // twisted or near-degenerate prism is a geometric failure that the runtime
        // ladder exists to catch, not something the table can prevent. The next
        // test covers the geometric half on prisms that are actually solids.
        let _ = &points;
    }
}

#[test]
// AI-FUNC-SUMMARY:
// On a prism that is a genuine solid, the frozen decomposition is geometrically valid: after the
// canonical orientation fix every tet is positive and the three fill the prism.
//
// The fix is not a detail. §4.3 gives *node lists* and says "emit under the canonical orientation
// fix" - row 6's `(a0,a1,b1,b2)` is negatively oriented on a plain straight prism - so a caller that
// emits the table verbatim ships inverted elements. `orient_positively` is what the pipeline runs,
// so it is what this test runs.
fn a_well_shaped_prism_decomposes_positively() {
    let mut next = rng(0x4204);
    let mut mixed_before_fix = 0usize;
    let mut twisted = 0usize;
    for _ in 0..4_000 {
        let angle = next() * std::f64::consts::TAU;
        let cap: Vec<Vec3> = (0..3)
            .map(|k| {
                let theta = angle + k as f64 * std::f64::consts::TAU / 3.0;
                Vec3::new(theta.cos(), theta.sin(), 0.0)
            })
            .collect();
        let sweep = Vec3::new(next() * 0.2 - 0.1, next() * 0.2 - 0.1, 0.8 + next());
        let mut points = cap.clone();
        for corner in &cap {
            points.push(corner.add(sweep).add(Vec3::new(
                (next() - 0.5) * 0.04,
                (next() - 0.5) * 0.04,
                (next() - 0.5) * 0.04,
            )));
        }
        let keys = keys_of(&points);
        let tets = prism_tets([0, 1, 2], [3, 4, 5], &keys).expect("T2 violated");
        let raw: Vec<f64> = tets.iter().map(|tet| volume(*tet, &points)).collect();
        if !(raw.iter().all(|v| *v > 0.0) || raw.iter().all(|v| *v < 0.0)) {
            mixed_before_fix += 1;
        }
        let mut total = 0.0;
        for tet in &tets {
            let fixed = orient_positively(*tet, &points).expect("a degenerate piece");
            let value = volume(fixed, &points);
            assert!(value > 0.0, "the orientation fix left a negative tet");
            total += value;
        }
        // The prism's volume, measured independently of the diagonals: the two caps
        // are congruent up to the jitter, so compare against the swept prism the
        // caps and sweep define, allowing for that jitter.
        let nominal = {
            let a = points[0];
            let normal = points[1].sub(a).cross(points[2].sub(a));
            normal.dot(sweep).abs() / 2.0
        };
        assert!(
            (total - nominal).abs() <= 0.1 * nominal,
            "the decomposition ({total:e}) is not the swept prism ({nominal:e})"
        );
    }
    assert!(
        mixed_before_fix > 0,
        "the orientation fix was never needed here, so this test does not exercise it"
    );

    // Deliberately twisted prisms: the geometric failures §4.4's runtime ladder is
    // there for. Asserting the ladder has a job keeps a future change that silently
    // removes the check visible.
    let mut next = rng(0x4205);
    for _ in 0..4_000 {
        let cap: Vec<Vec3> = (0..3)
            .map(|_| Vec3::new(next() * 2.0 - 1.0, next() * 2.0 - 1.0, 0.0))
            .collect();
        let sweep = Vec3::new(next() * 0.4 - 0.2, next() * 0.4 - 0.2, 1.0 + next());
        let mut points = cap.clone();
        for corner in &cap {
            points.push(corner.add(sweep).add(Vec3::new(
                (next() - 0.5) * 0.9,
                (next() - 0.5) * 0.9,
                (next() - 0.5) * 0.9,
            )));
        }
        let keys = keys_of(&points);
        let tets = prism_tets([0, 1, 2], [3, 4, 5], &keys).expect("T2 violated");
        let fixed: Vec<[u32; 4]> = tets
            .iter()
            .filter_map(|tet| orient_positively(*tet, &points))
            .collect();
        let total: f64 = fixed.iter().map(|tet| volume(*tet, &points)).sum();
        let nominal = {
            let a = points[0];
            let normal = points[1].sub(a).cross(points[2].sub(a));
            normal.dot(sweep).abs() / 2.0
        };
        if fixed.len() < 3 || (total - nominal).abs() > 0.5 * nominal.max(1.0e-12) {
            twisted += 1;
        }
    }
    assert!(
        twisted > 0,
        "no twisted prism was produced, so this test proves nothing about §4.4"
    );
}

/// A parent tet with one node assigned to each of the four slots, plus the cut
/// nodes on its inside-outside edges.
struct Cut {
    points: Vec<Vec3>,
    keys: Vec<NodeKey>,
    tet: [u32; 4],
    sides: [NodeSide; 4],
    cut: Vec<Option<u32>>,
}

impl Cut {
    // AI-FUNC-SUMMARY: The cut node on a parent edge, in the `cut_tet` callback's shape; returns Option<u32>; side effects: none.
    fn node_of(&self, a: usize, b: usize) -> Option<u32> {
        self.cut[a * 4 + b]
    }
}

// AI-FUNC-SUMMARY:
// Purpose: Build a random parent tet with the given side pattern and a cut node on every
//   inside-outside edge.
// Inputs: the side pattern and a random stream.
// Returns: Cut.
// Side effects: Advances the stream.
// Notes: On-cut nodes are placed on the plane the cut nodes interpolate to, which is what S7
//   guarantees; the cut parameter is kept away from the ends so the pieces stay non-degenerate.
fn random_cut(sides: [NodeSide; 4], next: &mut impl FnMut() -> f64) -> Cut {
    let mut points: Vec<Vec3> = (0..4)
        .map(|_| Vec3::new(next() * 2.0 - 1.0, next() * 2.0 - 1.0, next() * 2.0 - 1.0))
        .collect();
    // A positively oriented parent.
    let tet = [0u32, 1, 2, 3];
    if volume(tet, &points) < 0.0 {
        points.swap(0, 1);
    }
    let mut cut = vec![None; 16];
    for a in 0..4 {
        for b in 0..4 {
            if a == b {
                continue;
            }
            let straddles = matches!(
                (sides[a], sides[b]),
                (NodeSide::Inside, NodeSide::Outside) | (NodeSide::Outside, NodeSide::Inside)
            );
            if !straddles || cut[a * 4 + b].is_some() {
                continue;
            }
            let t = 0.2 + 0.6 * next();
            let point = points[a].add(points[b].sub(points[a]).scale(t));
            points.push(point);
            let id = points.len() as u32 - 1;
            cut[a * 4 + b] = Some(id);
            cut[b * 4 + a] = Some(id);
        }
    }
    let keys = keys_of(&points);
    Cut {
        points,
        keys,
        tet,
        sides,
        cut,
    }
}

// AI-FUNC-SUMMARY: Every legal side pattern of §6, one per case; returns the patterns with a label; side effects: none.
fn case_patterns() -> Vec<(&'static str, [NodeSide; 4])> {
    use NodeSide::{Inside as I, OnCut as C, Outside as O};
    vec![
        ("A", [I, O, C, C]),
        ("B", [I, O, O, C]),
        ("B'", [I, I, O, C]),
        ("C", [I, O, O, O]),
        ("C'", [I, I, I, O]),
        ("D", [I, I, O, O]),
    ]
}

#[test]
// AI-FUNC-SUMMARY: Every §6 case partitions the parent exactly - not to 1 %, to rounding.
fn every_case_partitions_the_parent_volume() {
    let mut next = rng(0x6001);
    for (label, sides) in case_patterns() {
        for _ in 0..2_000 {
            let cut = random_cut(sides, &mut next);
            let cell = cut_tet(
                cut.tet,
                cut.sides,
                &|a, b| cut.node_of(a, b),
                &cut.keys,
            )
            .unwrap_or_else(|| panic!("case {label} was rejected by the table"));
            let parent = volume(cut.tet, &cut.points).abs();
            let total: f64 = cell
                .tets
                .iter()
                .map(|tet| volume(*tet, &cut.points).abs())
                .sum();
            assert!(
                (total - parent).abs() <= 1.0e-9 * parent,
                "case {label}: children {total:e} vs parent {parent:e}"
            );
            let expected = match label {
                "A" => 2,
                "B" | "B'" => 3,
                "C" | "C'" => 4,
                _ => 6,
            };
            assert_eq!(cell.tets.len(), expected, "case {label} tet count");
        }
    }
}

#[test]
// AI-FUNC-SUMMARY: The pieces are split by the cut - each is wholly inside or wholly outside, never both.
fn the_pieces_are_separated_by_the_cut() {
    let mut next = rng(0x6002);
    for (label, sides) in case_patterns() {
        for _ in 0..500 {
            let cut = random_cut(sides, &mut next);
            let cell =
                cut_tet(cut.tet, cut.sides, &|a, b| cut.node_of(a, b), &cut.keys).unwrap();
            let parent_inside: Vec<u32> = (0..4)
                .filter(|slot| cut.sides[*slot] == NodeSide::Inside)
                .map(|slot| cut.tet[slot])
                .collect();
            let parent_outside: Vec<u32> = (0..4)
                .filter(|slot| cut.sides[*slot] == NodeSide::Outside)
                .map(|slot| cut.tet[slot])
                .collect();
            for (tet, inside) in cell.tets.iter().zip(cell.inside.iter()) {
                let touches_inside = tet.iter().any(|node| parent_inside.contains(node));
                let touches_outside = tet.iter().any(|node| parent_outside.contains(node));
                assert!(
                    !(touches_inside && touches_outside),
                    "case {label}: a piece spans the cut"
                );
                if touches_inside {
                    assert!(*inside, "case {label}: a piece on the inside is labelled outside");
                }
                if touches_outside {
                    assert!(!*inside, "case {label}: a piece on the outside is labelled inside");
                }
            }
        }
    }
}

// AI-FUNC-SUMMARY:
// Purpose: The §5.2 face state of one parent face of a cut.
// Inputs: the cut and the three parent slots of the face.
// Returns: FaceCutState in the canonical frame (nodes ascending by key).
// Side effects: None.
fn face_state(cut: &Cut, slots: [usize; 3]) -> FaceCutState {
    let mut order = slots;
    order.sort_by_key(|slot| cut.keys[cut.tet[*slot] as usize]);
    let nodes = [
        cut.tet[order[0]],
        cut.tet[order[1]],
        cut.tet[order[2]],
    ];
    let mut state = FaceCutState {
        nodes,
        ..Default::default()
    };
    for edge in 0..3 {
        let a = order[edge];
        let b = order[(edge + 1) % 3];
        state.cut[edge] = cut.node_of(a, b);
    }
    for slot in 0..3 {
        state.on_cut[slot] = cut.sides[order[slot]] == NodeSide::OnCut;
    }
    state
}

#[test]
// AI-FUNC-SUMMARY:
// The conformity claim of §6: a piece's boundary triangles on a parent face are exactly what §5.2
// produces for that face. This is what makes the cut conforming with no inter-cell communication.
fn the_cut_agrees_with_the_face_split_table_on_every_parent_face() {
    let mut next = rng(0x6003);
    let mut checked = 0usize;
    for (label, sides) in case_patterns() {
        for _ in 0..800 {
            let cut = random_cut(sides, &mut next);
            let cell =
                cut_tet(cut.tet, cut.sides, &|a, b| cut.node_of(a, b), &cut.keys).unwrap();
            for face in [[0usize, 1, 2], [0, 1, 3], [0, 2, 3], [1, 2, 3]] {
                let state = face_state(&cut, face);
                let expected: BTreeSet<[u32; 3]> = face_split(&state, &cut.keys)
                    .unwrap_or_else(|| {
                        panic!("case {label}: §5.2 rejected a face the cut accepted")
                    })
                    .into_iter()
                    .map(canonical)
                    .collect();
                // Which nodes lie on this parent face: its three corners and the
                // cut nodes on its three edges.
                let mut on_face: BTreeSet<u32> = state.nodes.iter().copied().collect();
                for slot in state.cut.iter().flatten() {
                    on_face.insert(*slot);
                }
                let mut produced: BTreeSet<[u32; 3]> = BTreeSet::new();
                for tet in &cell.tets {
                    for triangle in [
                        [tet[0], tet[1], tet[2]],
                        [tet[0], tet[1], tet[3]],
                        [tet[0], tet[2], tet[3]],
                        [tet[1], tet[2], tet[3]],
                    ] {
                        if triangle.iter().all(|node| on_face.contains(node)) {
                            produced.insert(canonical(triangle));
                        }
                    }
                }
                assert_eq!(
                    produced, expected,
                    "case {label}: the cut's triangles on face {face:?} differ from §5.2's"
                );
                checked += 1;
            }
        }
    }
    assert!(checked > 10_000, "only {checked} faces were checked");
}

#[test]
// AI-FUNC-SUMMARY: Two cells sharing a face triangulate it identically - conformity across a cell boundary.
fn two_cells_sharing_a_face_agree_on_it() {
    let mut next = rng(0x6004);
    for _ in 0..2_000 {
        // A shared face plus one apex on each side.
        let shared: Vec<Vec3> = (0..3)
            .map(|_| Vec3::new(next() * 2.0 - 1.0, next() * 2.0 - 1.0, 0.0))
            .collect();
        let mut points = shared.clone();
        points.push(Vec3::new(next() - 0.5, next() - 0.5, 1.0 + next()));
        points.push(Vec3::new(next() - 0.5, next() - 0.5, -1.0 - next()));
        // The patch cuts the shared face across edge (0,1) and edge (0,2), so the
        // shared node 0 is inside and nodes 1, 2 are outside.
        let mut cut_nodes = vec![None; 25];
        for (a, b) in [(0usize, 1usize), (0, 2), (0, 3), (0, 4)] {
            let t = 0.25 + 0.5 * next();
            let point = points[a].add(points[b].sub(points[a]).scale(t));
            points.push(point);
            let id = points.len() as u32 - 1;
            cut_nodes[a * 5 + b] = Some(id);
            cut_nodes[b * 5 + a] = Some(id);
        }
        let keys = keys_of(&points);
        let mut shared_triangles: Vec<BTreeSet<[u32; 3]>> = Vec::new();
        for apex in [3u32, 4u32] {
            let tet = [0u32, 1, 2, apex];
            let sides = [
                NodeSide::Inside,
                NodeSide::Outside,
                NodeSide::Outside,
                NodeSide::Outside,
            ];
            let lookup = |a: usize, b: usize| -> Option<u32> {
                cut_nodes[tet[a] as usize * 5 + tet[b] as usize]
            };
            let cell = cut_tet(tet, sides, &lookup, &keys).unwrap();
            let mut on_face: BTreeSet<u32> = [0u32, 1, 2].into_iter().collect();
            on_face.insert(cut_nodes[1].unwrap());
            on_face.insert(cut_nodes[2].unwrap());
            let mut produced = BTreeSet::new();
            for piece in &cell.tets {
                for triangle in [
                    [piece[0], piece[1], piece[2]],
                    [piece[0], piece[1], piece[3]],
                    [piece[0], piece[2], piece[3]],
                    [piece[1], piece[2], piece[3]],
                ] {
                    if triangle.iter().all(|node| on_face.contains(node)) {
                        produced.insert(canonical(triangle));
                    }
                }
            }
            shared_triangles.push(produced);
        }
        assert_eq!(
            shared_triangles[0], shared_triangles[1],
            "the two cells triangulated their shared face differently"
        );
        assert_eq!(shared_triangles[0].len(), 3, "split_3 emits three triangles");
    }
}

#[test]
// AI-FUNC-SUMMARY: The §5.2 table tiles the parent face exactly, in every case including the rim case.
fn the_face_table_tiles_the_face() {
    let mut next = rng(0x6005);
    for _ in 0..2_000 {
        let mut points: Vec<Vec3> = (0..3)
            .map(|_| Vec3::new(next() * 2.0 - 1.0, next() * 2.0 - 1.0, 0.0))
            .collect();
        let area_of = |triangle: [u32; 3], points: &[Vec3]| -> f64 {
            let a = points[triangle[0] as usize];
            let b = points[triangle[1] as usize];
            let c = points[triangle[2] as usize];
            b.sub(a).cross(c.sub(a)).dot(Vec3::new(0.0, 0.0, 1.0)).abs() / 2.0
        };
        let parent_area = area_of([0, 1, 2], &points);
        // One cut edge with the opposite vertex on the patch: split_2.
        let t = 0.2 + 0.6 * next();
        points.push(points[0].add(points[1].sub(points[0]).scale(t)));
        let m = points.len() as u32 - 1;
        let keys = keys_of(&points);
        let mut nodes = [0u32, 1, 2];
        nodes.sort_by_key(|node| keys[*node as usize]);
        let mut state = FaceCutState {
            nodes,
            ..Default::default()
        };
        for edge in 0..3 {
            let (a, b) = (nodes[edge], nodes[(edge + 1) % 3]);
            if (a == 0 && b == 1) || (a == 1 && b == 0) {
                state.cut[edge] = Some(m);
            }
            state.on_cut[edge] = nodes[edge] == 2;
        }
        let split = face_split(&state, &keys).expect("split_2 is a legal state");
        assert_eq!(split.len(), 2);
        let total: f64 = split.iter().map(|tri| area_of(*tri, &points)).sum();
        assert!(
            (total - parent_area).abs() <= 1.0e-9 * parent_area,
            "split_2 does not tile the face"
        );

        // The same face with a rim endpoint instead: split_R, four triangles.
        let centroid = points[0]
            .add(points[1])
            .add(points[2])
            .scale(1.0 / 3.0);
        points.push(centroid);
        let rim = points.len() as u32 - 1;
        let keys = keys_of(&points);
        let mut rim_state = state;
        rim_state.on_cut = [false; 3];
        rim_state.rim = Some(rim);
        let split = face_split(&rim_state, &keys).expect("split_R is a legal state");
        assert_eq!(split.len(), 4);
        let total: f64 = split.iter().map(|tri| area_of(*tri, &points)).sum();
        assert!(
            (total - parent_area).abs() <= 1.0e-9 * parent_area,
            "split_R does not tile the face"
        );
    }
}

#[test]
// AI-FUNC-SUMMARY: A cut segment with a dangling end is illegal and is refused, not guessed at.
fn a_dangling_cut_is_refused() {
    let points = vec![
        Vec3::new(0.0, 0.0, 0.0),
        Vec3::new(1.0, 0.0, 0.0),
        Vec3::new(0.0, 1.0, 0.0),
        Vec3::new(0.5, 0.0, 0.0),
    ];
    let keys = keys_of(&points);
    let state = FaceCutState {
        nodes: [0, 1, 2],
        cut: [Some(3), None, None],
        on_cut: [false; 3],
        rim: None,
    };
    assert!(
        face_split(&state, &keys).is_none(),
        "one cut edge with no on-cut vertex and no rim is an illegal state (§5.2)"
    );
}

#[test]
// AI-FUNC-SUMMARY: The guarded dry-run accepts a correct cut and rejects a deliberately broken one (ARB-15).
fn the_dry_run_catches_a_broken_cut() {
    let points = vec![
        Vec3::new(0.0, 0.0, 0.0),
        Vec3::new(1.0, 0.0, 0.0),
        Vec3::new(0.0, 1.0, 0.0),
        Vec3::new(0.0, 0.0, 1.0),
        Vec3::new(0.5, 0.0, 0.0),
        Vec3::new(0.0, 0.5, 0.0),
        Vec3::new(0.0, 0.0, 0.5),
    ];
    let parent = [0u32, 1, 2, 3];
    let options = CutOptions::default();
    // The real case-C children: the corner tet plus the prism over it.
    let keys = keys_of(&points);
    let mut children = vec![[0u32, 4, 5, 6]];
    children.extend(prism_tets([4, 5, 6], [1, 2, 3], &keys).unwrap());
    let (oriented, _, error) =
        guarded_dry_run(parent, &children, &points, &options).expect("a correct cut must pass");
    assert_eq!(oriented.len(), 4);
    assert!(error < 1.0e-12, "volume error {error:e}");
    for tet in &oriented {
        assert!(volume(*tet, &points) > 0.0, "the dry run must orient its output");
    }
    // Drop one child: the volume no longer adds up, which is exactly the failure a
    // mis-assembled table row produces - every piece is fine on its own.
    let short = &children[..3];
    assert!(
        guarded_dry_run(parent, short, &points, &options).is_err(),
        "a cut that does not fill the parent must be rejected"
    );
}

#[test]
// AI-FUNC-SUMMARY: A configuration with nothing inside or nothing outside is not a cut at all.
fn an_uncut_configuration_is_rejected_by_the_table() {
    let points = vec![
        Vec3::new(0.0, 0.0, 0.0),
        Vec3::new(1.0, 0.0, 0.0),
        Vec3::new(0.0, 1.0, 0.0),
        Vec3::new(0.0, 0.0, 1.0),
    ];
    let keys = keys_of(&points);
    let none = |_: usize, _: usize| None;
    for sides in [
        [NodeSide::Inside; 4],
        [NodeSide::Outside; 4],
        [
            NodeSide::Inside,
            NodeSide::Inside,
            NodeSide::OnCut,
            NodeSide::OnCut,
        ],
    ] {
        assert!(
            cut_tet([0, 1, 2, 3], sides, &none, &keys).is_none(),
            "{sides:?} has no cut and must not produce pieces"
        );
    }
}

/// Everything S8 needs, built by running S0..S7 for real.
struct Scene {
    lattice: rustmspt::meshgen::lattice::Lattice,
    snapped: rustmspt::meshgen::snap::Snapped,
    classification: rustmspt::meshgen::classify::Classification,
    classifier: rustmspt::meshgen::classify::PointClassifier,
    components: Vec<rustmspt::meshgen::ArrangeComponent>,
}

// AI-FUNC-SUMMARY:
// Purpose: Run the pipeline up to and including S7, so S8 is tested against real upstream output.
// Inputs: the input mesh and the target element size.
// Returns: Scene.
// Side effects: None.
fn scene(mesh: rustmspt::types::Mesh, h_max: f64) -> Scene {
    scene_of_kind(mesh, h_max, 0)
}

// AI-FUNC-SUMMARY: `scene` with an explicit component kind (0 solid, 1 sheet); returns Scene; side effects: none.
fn scene_of_kind(mesh: rustmspt::types::Mesh, h_max: f64, kind: u8) -> Scene {
    use rustmspt::config::meshgen::{CoincidencePolicy, RepairLevel};
    use rustmspt::meshgen::classify::{classify_lattice_with, ClassifyOptions, PointClassifier};
    use rustmspt::meshgen::lattice::{balance_octree, build_lattice, LatticeOptions};
    use rustmspt::meshgen::sizing::{build_sizing_field, SizingLookup, SizingOptions};
    use rustmspt::meshgen::snap::{snap_lattice, SnapOptions};
    use rustmspt::meshgen::{
        arrange_surface, clip_arranged_to_box, condition_surface, detect_features,
        rebuild_topology, ArrangeComponent, ArrangeOptions,
    };

    let eps = 1.0e-4;
    let dmin = Vec3::new(0.0, 0.0, 0.0);
    let dmax = Vec3::new(1.0, 1.0, 1.0);
    let components = vec![ArrangeComponent {
        x: 1,
        priority: 10,
        kind,
        closed: kind == 0,
    }];
    let cs = condition_surface(&[mesh], eps, RepairLevel::Conservative).unwrap();
    let fs = detect_features(&cs, 45.0);
    let arranged = arrange_surface(
        &cs,
        &fs,
        &ArrangeOptions {
            domain_min: dmin,
            domain_max: dmax,
            eps,
            coincidence: CoincidencePolicy::Merge,
            components,
        },
    )
    .unwrap();
    let mut clipped = clip_arranged_to_box(&arranged, dmin, dmax, eps).unwrap();
    let topo = rebuild_topology(&clipped);
    clipped.components = topo.components.clone();
    let sizing = SizingOptions {
        domain_min: dmin,
        domain_max: dmax,
        h_max,
        h_min: h_max / 4.0,
        eps,
        ..Default::default()
    };
    let field = build_sizing_field(&SizingLookup::build(Vec::new(), &sizing), &sizing);
    let (balanced, _) = balance_octree(&field);
    let lattice = build_lattice(&balanced, &LatticeOptions::default()).unwrap();
    let classifier = PointClassifier::build(
        &clipped,
        &topo,
        &ClassifyOptions {
            domain_min: dmin,
            domain_max: dmax,
        },
    );
    let classification = classify_lattice_with(
        &lattice,
        &classifier,
        &clipped,
        &ClassifyOptions {
            domain_min: dmin,
            domain_max: dmax,
        },
    );
    let snapped = snap_lattice(
        &lattice,
        &clipped,
        &classification,
        &SnapOptions {
            domain_min: dmin,
            domain_max: dmax,
            eps,
        },
    );
    Scene {
        lattice,
        snapped,
        classification,
        classifier,
        components: clipped.components.clone(),
    }
}

// AI-FUNC-SUMMARY: A closed UV sphere at the domain centre; returns Mesh; side effects: none.
fn sphere(radius: f64, bands: usize) -> rustmspt::types::Mesh {
    use rustmspt::types::{Mesh, Triangle};
    let mut mesh = Mesh::empty();
    let at = |lat: usize, lon: usize| -> Vec3 {
        let theta = std::f64::consts::PI * lat as f64 / bands as f64;
        let phi = 2.0 * std::f64::consts::PI * lon as f64 / (2 * bands) as f64;
        Vec3::new(0.5, 0.5, 0.5).add(Vec3::new(
            radius * theta.sin() * phi.cos(),
            radius * theta.sin() * phi.sin(),
            radius * theta.cos(),
        ))
    };
    for lat in 0..bands {
        for lon in 0..2 * bands {
            let base = mesh.vertices.len();
            for p in [
                at(lat, lon),
                at(lat + 1, lon),
                at(lat + 1, lon + 1),
                at(lat, lon + 1),
            ] {
                mesh.vertices.push(p);
            }
            mesh.faces.push(Triangle {
                a: base,
                b: base + 1,
                c: base + 2,
            });
            mesh.faces.push(Triangle {
                a: base,
                b: base + 2,
                c: base + 3,
            });
        }
    }
    mesh
}

// AI-FUNC-SUMMARY: An axis-aligned box thinner than one element - the K1 escalation case; returns Mesh; side effects: none.
fn thin_plate() -> rustmspt::types::Mesh {
    use rustmspt::types::{Mesh, Triangle};
    let lo = Vec3::new(0.15, 0.15, 0.49);
    let hi = Vec3::new(0.85, 0.85, 0.512);
    let v = |i: usize| {
        Vec3::new(
            if i & 1 == 0 { lo.x } else { hi.x },
            if i & 2 == 0 { lo.y } else { hi.y },
            if i & 4 == 0 { lo.z } else { hi.z },
        )
    };
    let mut mesh = Mesh::empty();
    for quad in [
        [0usize, 2, 3, 1],
        [4, 5, 7, 6],
        [0, 1, 5, 4],
        [2, 6, 7, 3],
        [0, 4, 6, 2],
        [1, 3, 7, 5],
    ] {
        let base = mesh.vertices.len();
        for k in quad {
            mesh.vertices.push(v(k));
        }
        mesh.faces.push(Triangle {
            a: base,
            b: base + 1,
            c: base + 2,
        });
        mesh.faces.push(Triangle {
            a: base,
            b: base + 2,
            c: base + 3,
        });
    }
    mesh
}

// AI-FUNC-SUMMARY:
// Purpose: Every face of the cut mesh with the tets that carry it.
// Returns: the map from a canonical triangle to its owning tets.
// Side effects: None.
fn face_owners(
    mesh: &rustmspt::meshgen::cut::CutMesh,
) -> std::collections::BTreeMap<[u32; 3], Vec<usize>> {
    let mut faces: std::collections::BTreeMap<[u32; 3], Vec<usize>> =
        std::collections::BTreeMap::new();
    for (index, tet) in mesh.tets.iter().enumerate() {
        for triangle in [
            [tet[0], tet[1], tet[2]],
            [tet[0], tet[1], tet[3]],
            [tet[0], tet[2], tet[3]],
            [tet[1], tet[2], tet[3]],
        ] {
            faces.entry(canonical(triangle)).or_default().push(index);
        }
    }
    faces
}

#[test]
// AI-FUNC-SUMMARY:
// The end-to-end conformity property: after S8 every interior face is shared by exactly two tets.
//
// This is the claim the whole design rests on - the cut is computed per cell with no communication,
// so nothing but Invariant C makes the pieces meet. It is checked on a smooth surface *and* on a
// plate thinner than one element, which is the case that escalates cells (K1) and therefore
// exercises the junction fallback next to ordinary cut cells.
fn the_cut_is_conforming() {
    use rustmspt::meshgen::cut::{cut_lattice, CutOptions};
    for (label, mesh, h) in [
        ("sphere", sphere(0.28, 12), 0.1),
        ("thin plate", thin_plate(), 0.1),
    ] {
        let scene = scene(mesh, h);
        let cut = cut_lattice(
            &scene.lattice,
            &scene.snapped,
            &scene.classification,
            &scene.classifier,
            &scene.components,
            &CutOptions {
                eps: 1.0e-4,
                ..Default::default()
            },
        );
        let on_box = |node: u32| -> bool {
            let p = cut.nodes[node as usize];
            p.x == 0.0 || p.x == 1.0 || p.y == 0.0 || p.y == 1.0 || p.z == 0.0 || p.z == 1.0
        };
        let mut leaks = 0usize;
        let mut multi = 0usize;
        for (triangle, owners) in face_owners(&cut) {
            match owners.len() {
                1 => {
                    if !triangle.iter().all(|node| on_box(*node)) {
                        leaks += 1;
                    }
                }
                2 => {}
                _ => multi += 1,
            }
        }
        assert_eq!(leaks, 0, "{label}: {leaks} interior face(s) have one owner");
        assert_eq!(multi, 0, "{label}: {multi} face(s) are shared by three or more tets");
        assert!(cut.stats.n_cut_cells > 0, "{label}: nothing was cut");
    }
}

#[test]
// AI-FUNC-SUMMARY: The cut preserves the domain volume exactly and leaves no inverted element.
fn the_cut_preserves_volume_and_orientation() {
    use rustmspt::meshgen::cut::{cut_lattice, CutOptions};
    let scene = scene(sphere(0.28, 12), 0.1);
    let before: f64 = scene
        .lattice
        .tets
        .iter()
        .map(|tet| {
            let a = scene.snapped.nodes[tet[0] as usize];
            let u = scene.snapped.nodes[tet[1] as usize].sub(a);
            let v = scene.snapped.nodes[tet[2] as usize].sub(a);
            let w = scene.snapped.nodes[tet[3] as usize].sub(a);
            (u.cross(v).dot(w) / 6.0).abs()
        })
        .sum();
    let cut = cut_lattice(
        &scene.lattice,
        &scene.snapped,
        &scene.classification,
        &scene.classifier,
        &scene.components,
        &CutOptions {
            eps: 1.0e-4,
            ..Default::default()
        },
    );
    let after: f64 = cut.tets.iter().map(|tet| volume(*tet, &cut.nodes)).sum();
    assert!(
        (after - before).abs() <= 1.0e-9 * before,
        "the cut changed the meshed volume: {before:e} -> {after:e}"
    );
    for (index, tet) in cut.tets.iter().enumerate() {
        assert!(
            volume(*tet, &cut.nodes) > 0.0,
            "tet {index} is not positively oriented"
        );
    }
}

#[test]
// AI-FUNC-SUMMARY: Every interface face is a real material boundary: two elements, one owning the component and one not.
fn the_interface_index_is_derivable_and_two_sided() {
    use rustmspt::meshgen::classify::Side;
    use rustmspt::meshgen::cut::{cut_lattice, CutOptions};
    let scene = scene(sphere(0.28, 12), 0.1);
    let cut = cut_lattice(
        &scene.lattice,
        &scene.snapped,
        &scene.classification,
        &scene.classifier,
        &scene.components,
        &CutOptions {
            eps: 1.0e-4,
            ..Default::default()
        },
    );
    assert!(!cut.interfaces.is_empty(), "the cut produced no interface");
    let faces = face_owners(&cut);
    for face in &cut.interfaces {
        let owners = faces
            .get(&canonical(face.nodes))
            .unwrap_or_else(|| panic!("interface face {:?} is not a face of any tet", face.nodes));
        assert_eq!(
            owners.len(),
            2,
            "an interface face must separate exactly two elements"
        );
        assert!(
            face.side_elems[0] >= 0 && face.side_elems[1] >= 0,
            "both side elements must be derivable"
        );
        assert_eq!(
            cut.records[face.side_elems[0] as usize].side_of(face.component),
            Side::Inside,
            "side_elems[0] must be the element inside the component"
        );
        assert_ne!(
            cut.records[face.side_elems[1] as usize].side_of(face.component),
            Side::Inside,
            "side_elems[1] must be the element outside it"
        );
    }
}

#[test]
// AI-FUNC-SUMMARY: S8 is bit-identical across runs and thread counts (R-P2).
fn the_cut_is_deterministic() {
    use rustmspt::meshgen::cut::{cut_lattice, CutOptions};
    let scene = scene(sphere(0.28, 12), 0.15);
    let run = || {
        cut_lattice(
            &scene.lattice,
            &scene.snapped,
            &scene.classification,
            &scene.classifier,
            &scene.components,
            &CutOptions {
                eps: 1.0e-4,
                ..Default::default()
            },
        )
    };
    let first = run();
    let second = rayon::ThreadPoolBuilder::new()
        .num_threads(1)
        .build()
        .unwrap()
        .install(run);
    assert_eq!(first.tets, second.tets);
    assert_eq!(first.parent_of, second.parent_of);
    assert_eq!(first.interfaces, second.interfaces);
    assert_eq!(first.stats, second.stats);
    for (a, b) in first.nodes.iter().zip(second.nodes.iter()) {
        assert_eq!(
            (a.x.to_bits(), a.y.to_bits(), a.z.to_bits()),
            (b.x.to_bits(), b.y.to_bits(), b.z.to_bits())
        );
    }
}

#[test]
// AI-FUNC-SUMMARY: `s08_cut` satisfies the contract verifier with no FAIL item, tagged faces and all.
fn the_snapshot_verifies() {
    use rustmspt::meshgen::cut::{cut_lattice, cut_to_doc, CutOptions};
    use rustmspt::meshgen::snapshot::{SnapshotMeta, Stage};
    use rustmspt::meshgen::{stamp_metadata, verify, Severity, VerifyGates};
    let scene = scene(sphere(0.28, 12), 0.1);
    let cut = cut_lattice(
        &scene.lattice,
        &scene.snapped,
        &scene.classification,
        &scene.classifier,
        &scene.components,
        &CutOptions {
            eps: 1.0e-4,
            ..Default::default()
        },
    );
    let mut doc = cut_to_doc(&cut, &scene.components);
    let meta = SnapshotMeta::new(
        Stage::Cut,
        0x6202,
        (Vec3::new(0.0, 0.0, 0.0), Vec3::new(1.0, 1.0, 1.0)),
    );
    stamp_metadata(&mut doc, &meta);
    let report = verify(&doc, &VerifyGates::default());
    let failures: Vec<String> = report
        .sections
        .iter()
        .flat_map(|section| section.items.iter())
        .filter(|item| item.severity == Severity::Fail)
        .map(|item| format!("{}: {}", item.code, item.message))
        .collect();
    assert!(failures.is_empty(), "s08 verification failed: {failures:?}");
}

// AI-FUNC-SUMMARY: A flat open square in the z = 0.5 plane - an embedded sheet; returns Mesh; side effects: none.
fn sheet_square() -> rustmspt::types::Mesh {
    use rustmspt::types::{Mesh, Triangle};
    let mut mesh = Mesh::empty();
    let n = 6usize;
    let (lo, hi) = (0.2, 0.8);
    for i in 0..n {
        for j in 0..n {
            let x0 = lo + (hi - lo) * i as f64 / n as f64;
            let x1 = lo + (hi - lo) * (i + 1) as f64 / n as f64;
            let y0 = lo + (hi - lo) * j as f64 / n as f64;
            let y1 = lo + (hi - lo) * (j + 1) as f64 / n as f64;
            let base = mesh.vertices.len();
            for p in [
                Vec3::new(x0, y0, 0.52),
                Vec3::new(x1, y0, 0.52),
                Vec3::new(x1, y1, 0.52),
                Vec3::new(x0, y1, 0.52),
            ] {
                mesh.vertices.push(p);
            }
            mesh.faces.push(Triangle {
                a: base,
                b: base + 1,
                c: base + 2,
            });
            mesh.faces.push(Triangle {
                a: base,
                b: base + 2,
                c: base + 3,
            });
        }
    }
    mesh
}

#[test]
// AI-FUNC-SUMMARY:
// G6-5: a welded sheet is cut into element faces without changing any element's material.
//
// A sheet has no inside, so §6's sides cannot come from S6 - it refuses to classify one, and must.
// They come from the cut: two nodes are on the same side iff the edge between them is uncut. What
// makes the result a *welded* sheet is that both sides keep the parent's record; only the tagged
// interface tells them apart, which is exactly the C0 semantics of PLAN §10.13.
fn a_welded_sheet_is_cut_without_changing_ownership() {
    use rustmspt::meshgen::cut::{cut_lattice, CutOptions};
    let scene = scene_of_kind(sheet_square(), 0.15, 1);
    assert!(
        scene.classification.solid_components.is_empty(),
        "a sheet must never be classified as a solid"
    );
    let cut = cut_lattice(
        &scene.lattice,
        &scene.snapped,
        &scene.classification,
        &scene.classifier,
        &scene.components,
        &CutOptions {
            eps: 1.0e-4,
            ..Default::default()
        },
    );
    assert!(
        cut.stats.n_welded_sheet_cells > 0,
        "no cell was cut by the sheet"
    );
    assert!(!cut.interfaces.is_empty(), "the sheet produced no tagged faces");
    for face in &cut.interfaces {
        assert_eq!(face.component, 1);
    }
    // Ownership is unchanged: the sheet claims no volume on either side.
    for record in &cut.records {
        assert!(
            record.entries.is_empty(),
            "a welded sheet must not give any element an ownership entry"
        );
    }
    // And the result is still conforming, and still fills the domain.
    let on_box = |node: u32| -> bool {
        let p = cut.nodes[node as usize];
        p.x == 0.0 || p.x == 1.0 || p.y == 0.0 || p.y == 1.0 || p.z == 0.0 || p.z == 1.0
    };
    for (triangle, owners) in face_owners(&cut) {
        if owners.len() == 1 {
            assert!(
                triangle.iter().all(|node| on_box(*node)),
                "the sheet cut leaked an interior face"
            );
        } else {
            assert_eq!(owners.len(), 2, "a face is shared by three or more tets");
        }
    }
    let total: f64 = cut.tets.iter().map(|tet| volume(*tet, &cut.nodes)).sum();
    assert!(
        (total - 1.0).abs() < 1.0e-9,
        "the meshed volume is no longer the domain: {total}"
    );
}

#[test]
// AI-FUNC-SUMMARY:
// The conformity claim of §6 over **every** side assignment, not one canonical ordering per case.
// `case_patterns()` fixes each case's sides to particular tet slots; a real cell meets every
// permutation of them, and a row that mis-handles one produces a face its neighbour does not agree
// with. That is a pair of single-sided faces in the mesh and nothing else - which is exactly the
// residue `[V3]` was still reporting on the reference dataset after the triage fix.
fn every_side_assignment_agrees_with_the_face_split_table() {
    use rustmspt::meshgen::cut::NodeSide::{Inside as I, OnCut as C, Outside as O};
    let mut next = rng(0x6007);
    let mut checked = 0usize;
    let mut accepted = 0usize;
    let mut disagreements: Vec<String> = Vec::new();
    for mask in 0..81u32 {
        let sides: [NodeSide; 4] = std::array::from_fn(|slot| {
            match (mask / 3u32.pow(slot as u32)) % 3 {
                0 => I,
                1 => O,
                _ => C,
            }
        });
        for _ in 0..200 {
            let cut = random_cut(sides, &mut next);
            let Some(cell) = cut_tet(cut.tet, cut.sides, &|a, b| cut.node_of(a, b), &cut.keys)
            else {
                continue;
            };
            accepted += 1;
            for face in [[0usize, 1, 2], [0, 1, 3], [0, 2, 3], [1, 2, 3]] {
                let state = face_state(&cut, face);
                let Some(expected) = face_split(&state, &cut.keys) else {
                    disagreements.push(format!(
                        "sides {sides:?}: §5.2 rejected face {face:?} that the cut accepted"
                    ));
                    continue;
                };
                let expected: BTreeSet<[u32; 3]> = expected.into_iter().map(canonical).collect();
                let mut on_face: BTreeSet<u32> = state.nodes.iter().copied().collect();
                for slot in state.cut.iter().flatten() {
                    on_face.insert(*slot);
                }
                let mut produced: BTreeSet<[u32; 3]> = BTreeSet::new();
                for tet in &cell.tets {
                    for triangle in [
                        [tet[0], tet[1], tet[2]],
                        [tet[0], tet[1], tet[3]],
                        [tet[0], tet[2], tet[3]],
                        [tet[1], tet[2], tet[3]],
                    ] {
                        if triangle.iter().all(|node| on_face.contains(node)) {
                            produced.insert(canonical(triangle));
                        }
                    }
                }
                checked += 1;
                if produced != expected {
                    disagreements.push(format!(
                        "sides {sides:?}, face {face:?}: cut made {produced:?}, §5.2 makes {expected:?}"
                    ));
                }
            }
        }
    }
    assert!(accepted > 1_000, "only {accepted} assignments were meshed");
    let patterns: BTreeSet<&str> = disagreements
        .iter()
        .map(|line| line.split(':').next().unwrap_or(line))
        .collect();
    assert!(
        disagreements.is_empty(),
        "{} of {checked} faces disagree over {} distinct side patterns: {:?}",
        disagreements.len(),
        patterns.len(),
        disagreements.iter().take(4).collect::<Vec<_>>()
    );
}
