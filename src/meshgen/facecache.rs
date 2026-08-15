//! `SPEC_meshgen_geometry.md` §7.3 — the shared-face triangulation cache, and invariant J1.
//!
//! > **Invariant J1.** The triangulation of a tet face is a pure function of the face's node keys
//! > and of the constraint segments crossing it, computed **once** and consumed identically by
//! > both incident cells.
//!
//! **Why this is the blocker rather than a nicety.** Phase P-3 built the §7.2–§7.4 cell mesher and
//! could not wire it in: a cell that cuts along a crease puts an edge on its own boundary that its
//! neighbour — which decided not to cut — does not have. Same vertices, different edges, and the
//! shared face stops matching (measured: a8 at 12 boundary leaks, 6 hanging nodes, 2 non-manifold
//! edges). Two cell-local safety rules were tried and neither was sufficient, because the decision
//! is being taken at the wrong level. **The face has to be triangulated before either cell meshes
//! its interior, once, from the face alone.** That is what this module is.
//!
//! The construction is the same idea the cut's cap splitter already uses, lifted to the face and
//! made total: a constraint crossing a face is a **chord** between two nodes of the face's own
//! boundary walk, and splitting a polygon at a chord is exact — walk the boundary one way for one
//! side, the other way for the other. Apply every chord in a canonical order and fan each region
//! from its smallest key. No Steiner point, no geometric predicate beyond the walk, and the result
//! depends on nothing but the loop and the chord set.
//!
//! **Why the answer cannot depend on the winding.** Two cells see the same face wound opposite
//! ways, so a triangulation that depended on the walk direction would satisfy J1's letter and break
//! it in practice. The loop is therefore *canonicalised* first — rotated to start at the smallest
//! key and oriented so its second node is the smaller of the two neighbours — and everything else
//! is derived from that. `triangulation_is_winding_independent` is the test that holds this down,
//! and it is the one that matters.
//!
//! The cache stores a **fingerprint** beside the triangles, per §7.3: the sorted constraint entity
//! ids the caller used. A hit whose fingerprint differs is a hard error and never a recompute — a
//! mismatch means the two cells disagree about the geometry on the face they share, which is a
//! defect to surface, not a race to resolve.

use crate::meshgen::NodeKey;
use std::collections::BTreeMap;

/// A face's identity: its three corner `NodeKey`s, sorted. Both incident cells derive it from
/// the face alone, which is what lets them find the same entry without talking to each other.
pub type FaceKey = [NodeKey; 3];

/// The constraint set a triangulation was computed for: sorted entity ids.
pub type Fingerprint = Vec<i64>;

/// A face whose two cells disagree about what crosses it. `[V9]` exists to catch this class;
/// reaching it here means it was caught earlier and more cheaply.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FingerprintMismatch {
    pub face: FaceKey,
    pub cached: Fingerprint,
    pub requested: Fingerprint,
}

#[derive(Clone, Debug)]
struct Entry {
    fingerprint: Fingerprint,
    triangles: Vec<[u32; 3]>,
}

/// `FaceKey → (constraint fingerprint, triangles)`, per §7.3.
#[derive(Clone, Debug, Default)]
pub struct FaceTriCache {
    entries: BTreeMap<FaceKey, Entry>,
    pub hits: usize,
    pub misses: usize,
}

impl FaceTriCache {
    // AI-FUNC-SUMMARY: Empty cache; returns FaceTriCache; side effects: none.
    pub fn new() -> FaceTriCache {
        FaceTriCache::default()
    }

    // AI-FUNC-SUMMARY:
    // Purpose: The triangulation of one face, computed once and shared by both incident cells.
    // Inputs: the face key, the caller's constraint fingerprint, and a closure computing the
    //   triangles on a miss.
    // Returns: the triangles, or the mismatch when the cached entry was built for a different
    //   constraint set.
    // Side effects: Inserts on a miss; counts hits and misses.
    // Notes: A hit with a different fingerprint is a **hard error, not a recompute** (§7.3). The
    //   two cells sharing this face have disagreed about the geometry on it, and recomputing would
    //   hand the second caller a triangulation the first has already consumed - turning a
    //   detectable disagreement into a crack somewhere else.
    pub fn get_or_insert<F>(
        &mut self,
        face: FaceKey,
        fingerprint: Fingerprint,
        compute: F,
    ) -> Result<&[[u32; 3]], FingerprintMismatch>
    where
        F: FnOnce() -> Vec<[u32; 3]>,
    {
        if let Some(existing) = self.entries.get(&face) {
            if existing.fingerprint != fingerprint {
                return Err(FingerprintMismatch {
                    face,
                    cached: existing.fingerprint.clone(),
                    requested: fingerprint,
                });
            }
            self.hits += 1;
            return Ok(&self.entries[&face].triangles);
        }
        self.misses += 1;
        let triangles = compute();
        self.entries.insert(
            face,
            Entry {
                fingerprint,
                triangles,
            },
        );
        Ok(&self.entries[&face].triangles)
    }

    // AI-FUNC-SUMMARY: How many faces the cache holds; returns usize; side effects: none.
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    // AI-FUNC-SUMMARY: Whether the cache is empty; returns bool; side effects: none.
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

// AI-FUNC-SUMMARY:
// Purpose: Put a face's boundary walk into the one order both incident cells will derive.
// Inputs: the walk and the key table.
// Returns: the same cycle, rotated to its smallest key and oriented by its smaller neighbour.
// Side effects: None.
// Notes: The two cells sharing a face walk it in **opposite** directions, so any rule that reads
//   the walk as given is winding-dependent and breaks J1 in exactly the case it is meant to
//   protect. Rotating to the smallest key fixes the start; choosing the direction whose *second*
//   node is smaller fixes the sense. Both are functions of node keys alone, so the two cells land
//   on the identical sequence.
fn canonical_walk(walk: &[u32], keys: &[NodeKey]) -> Vec<u32> {
    let count = walk.len();
    if count < 3 {
        return walk.to_vec();
    }
    let start = (0..count)
        .min_by_key(|slot| keys[walk[*slot] as usize])
        .unwrap_or(0);
    let forward: Vec<u32> = (0..count).map(|step| walk[(start + step) % count]).collect();
    let backward: Vec<u32> = (0..count)
        .map(|step| walk[(start + count - step) % count])
        .collect();
    if keys[backward[1] as usize] < keys[forward[1] as usize] {
        backward
    } else {
        forward
    }
}

// AI-FUNC-SUMMARY:
// Purpose: Triangulate one face so that every constraint chord is an edge of the result.
// Inputs: the face's boundary walk, the chords (node pairs, both endpoints on the walk), and keys.
// Returns: the triangles, or None when a chord is not a usable chord of this walk.
// Side effects: None.
// Notes: **This is the operation the crease needs.** A planar cap cannot lie on a surface that
//   creases inside the cell; the crease has to be an edge, and on a shared face it has to be the
//   *same* edge for both cells. Splitting a polygon at a chord is exact and needs no predicate:
//   the two sides are the two boundary runs between the chord's endpoints. Chords are applied in a
//   canonical order and each region is fanned from its own smallest key, so the output is a pure
//   function of `(walk, chords)` - J1 stated as a signature.
//
//   A chord whose endpoints are **adjacent** on the walk is skipped rather than refused: it lies
//   along an existing edge, so it is already an edge of any triangulation and splitting by it
//   would leave a degenerate sliver. That is the same finding the cut records for its own
//   `chord lies along a walk edge` case, reached from the other side.
pub fn triangulate_face(
    walk: &[u32],
    chords: &[[u32; 2]],
    hub: Option<u32>,
    keys: &[NodeKey],
) -> Option<Vec<[u32; 3]>> {
    if walk.len() < 3 {
        return None;
    }
    // **The crease's own shape, and why it needs a hub rather than a chord.** Two patches of one
    // component leave one chord each, and they meet where the crease pierces this face. If that
    // pierce point is not a node of the walk the two chords *cross* in the interior, and no
    // sequence of polygon splits can make both survive - split by one and the other's endpoints
    // land in different regions. The pierce point has to be a node first.
    //
    // It is interned per *face* by the caller (the cut's `face_steiner`/`curve_pierce` map), never
    // here: §7.4 forbids the interior mesher putting a point on a shared face, and the whole point
    // of this module is that the face is settled before any cell looks at it. Given the hub, the
    // triangulation is a fan from it - every chord from the hub to a walk node is then an edge for
    // free, and both cells derive it identically because the hub and the walk are both shared.
    if let Some(hub) = hub {
        if walk.contains(&hub) {
            return None;
        }
        let canonical = canonical_walk(walk, keys);
        let triangles: Vec<[u32; 3]> = (0..canonical.len())
            .map(|slot| [hub, canonical[slot], canonical[(slot + 1) % canonical.len()]])
            .filter(|t| t[0] != t[1] && t[1] != t[2] && t[0] != t[2])
            .collect();
        return (!triangles.is_empty()).then_some(triangles);
    }
    let canonical = canonical_walk(walk, keys);
    // Canonical chord order, by the pair's keys, so the split sequence does not depend on the
    // order the caller happened to collect them in.
    let mut ordered: Vec<[u32; 2]> = chords
        .iter()
        .map(|c| {
            if keys[c[0] as usize] <= keys[c[1] as usize] {
                *c
            } else {
                [c[1], c[0]]
            }
        })
        .collect();
    ordered.sort_by_key(|c| (keys[c[0] as usize], keys[c[1] as usize]));
    ordered.dedup();

    let mut regions: Vec<Vec<u32>> = vec![canonical];
    for chord in &ordered {
        let mut next: Vec<Vec<u32>> = Vec::with_capacity(regions.len() + 1);
        for region in &regions {
            let at = |node: u32| region.iter().position(|id| *id == node);
            match (at(chord[0]), at(chord[1])) {
                (Some(lo), Some(hi)) if lo != hi => {
                    let (lo, hi) = (lo.min(hi), lo.max(hi));
                    // Adjacent on the walk: already an edge, nothing to split.
                    if hi == lo + 1 || (lo == 0 && hi + 1 == region.len()) {
                        next.push(region.clone());
                        continue;
                    }
                    let near: Vec<u32> = region[lo..=hi].to_vec();
                    let mut far: Vec<u32> = region[hi..].to_vec();
                    far.extend(region[..=lo].iter().copied());
                    if near.len() < 3 || far.len() < 3 {
                        next.push(region.clone());
                        continue;
                    }
                    next.push(near);
                    next.push(far);
                }
                _ => next.push(region.clone()),
            }
        }
        regions = next;
    }

    let mut triangles = Vec::new();
    for region in &regions {
        if region.len() < 3 {
            continue;
        }
        let pivot = *region
            .iter()
            .min_by_key(|id| keys[**id as usize])
            .expect("non-empty");
        let at = region.iter().position(|id| *id == pivot).unwrap_or(0);
        for step in 1..region.len() - 1 {
            let b = region[(at + step) % region.len()];
            let c = region[(at + step + 1) % region.len()];
            if pivot == b || b == c || pivot == c {
                continue;
            }
            triangles.push([pivot, b, c]);
        }
    }
    (!triangles.is_empty()).then_some(triangles)
}

// AI-FUNC-SUMMARY: A face's cache key from its three corners; returns FaceKey; side effects: none.
pub fn face_key(corners: [u32; 3], keys: &[NodeKey]) -> FaceKey {
    let mut k = [
        keys[corners[0] as usize],
        keys[corners[1] as usize],
        keys[corners[2] as usize],
    ];
    k.sort_unstable();
    k
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::meshgen::predicates::node_key;
    use crate::types::Vec3;

    fn keys_for(points: &[Vec3]) -> Vec<NodeKey> {
        points.iter().map(|p| node_key(*p, 1.0e-12)).collect()
    }

    // A square face, its four corners plus two midpoints on opposite edges - the shape a face
    // gets once the cut has put nodes on it.
    fn square() -> (Vec<Vec3>, Vec<NodeKey>, Vec<u32>) {
        let points = vec![
            Vec3::new(0.0, 0.0, 0.0),
            Vec3::new(1.0, 0.0, 0.0),
            Vec3::new(1.0, 1.0, 0.0),
            Vec3::new(0.0, 1.0, 0.0),
            Vec3::new(0.5, 0.0, 0.0),
            Vec3::new(0.5, 1.0, 0.0),
        ];
        let keys = keys_for(&points);
        // walk: 0, 4, 1, 2, 5, 3
        (points, keys, vec![0, 4, 1, 2, 5, 3])
    }

    #[test]
    fn no_chords_fans_the_face() {
        let (_, keys, walk) = square();
        let tris = triangulate_face(&walk, &[], None, &keys).expect("triangulates");
        assert_eq!(tris.len(), walk.len() - 2, "a fan of an n-gon is n-2 triangles");
    }

    // The property the crease needs: after triangulation the chord is an *edge*, on both sides.
    #[test]
    fn a_chord_becomes_an_edge_of_the_triangulation() {
        let (_, keys, walk) = square();
        let chord = [4u32, 5];
        let tris = triangulate_face(&walk, &[chord], None, &keys).expect("triangulates");
        let has_edge = tris.iter().any(|t| {
            let e = [[t[0], t[1]], [t[1], t[2]], [t[2], t[0]]];
            e.iter()
                .any(|[a, b]| (*a == chord[0] && *b == chord[1]) || (*a == chord[1] && *b == chord[0]))
        });
        assert!(has_edge, "the constraint must survive as a mesh edge");
        assert_eq!(tris.len(), 4, "two quads, two triangles each");
    }

    // **The test that matters.** Two cells see the same face wound opposite ways. If the
    // triangulation depended on that, J1 would hold on paper and crack in the mesh - which is
    // exactly how the cell-local CDT integration failed.
    #[test]
    fn triangulation_is_winding_independent() {
        let (_, keys, walk) = square();
        let chord = [4u32, 5];
        let mut reversed = walk.clone();
        reversed.reverse();
        // ...and started somewhere else, as the neighbour's own walk would be.
        reversed.rotate_left(2);

        let normalise = |tris: Vec<[u32; 3]>| {
            let mut out: Vec<[u32; 3]> = tris
                .into_iter()
                .map(|mut t| {
                    t.sort_unstable();
                    t
                })
                .collect();
            out.sort_unstable();
            out
        };
        let from_one = normalise(triangulate_face(&walk, &[chord], None, &keys).unwrap());
        let from_other = normalise(triangulate_face(&reversed, &[chord], None, &keys).unwrap());
        assert_eq!(
            from_one, from_other,
            "J1: both cells must derive the same triangles for the face they share"
        );
    }

    // **The crease case proper.** Two patches of one component cross this face and meet at the
    // point where the crease pierces it. That point is interned by the caller as a hub, and the
    // fan from it puts every chord on a mesh edge - which is what a planar cap could never do.
    #[test]
    fn a_hub_puts_every_crease_chord_on_an_edge() {
        let points = vec![
            Vec3::new(0.0, 0.0, 0.0),
            Vec3::new(1.0, 0.0, 0.0),
            Vec3::new(1.0, 1.0, 0.0),
            Vec3::new(0.0, 1.0, 0.0),
            Vec3::new(0.5, 0.0, 0.0),
            Vec3::new(1.0, 0.5, 0.0),
            Vec3::new(0.5, 1.0, 0.0),
            Vec3::new(0.0, 0.5, 0.0),
        ];
        let walk = vec![0u32, 4, 1, 5, 2, 6, 3, 7];
        // The pierce point, interned per face by the caller — node 8.
        let points = {
            let mut with_hub = points;
            with_hub.push(Vec3::new(0.5, 0.5, 0.0));
            with_hub
        };
        let keys = keys_for(&points);
        let hub = 8u32;
        let tris = triangulate_face(&walk, &[], Some(hub), &keys).expect("triangulates");
        for chord in [[hub, 4u32], [hub, 6], [hub, 5], [hub, 7]] {
            let has_edge = tris.iter().any(|t| {
                let e = [[t[0], t[1]], [t[1], t[2]], [t[2], t[0]]];
                e.iter().any(|[a, b]| {
                    (*a == chord[0] && *b == chord[1]) || (*a == chord[1] && *b == chord[0])
                })
            });
            assert!(has_edge, "chord {chord:?} must survive as an edge");
        }
        assert_eq!(tris.len(), walk.len(), "a hub fan is one triangle per boundary edge");
    }

    // A chord along an existing edge is already an edge; splitting by it would leave a
    // zero-area sliver, and the cut records that exact failure from its own side.
    #[test]
    fn a_chord_along_an_existing_edge_is_skipped() {
        let (_, keys, walk) = square();
        let plain = triangulate_face(&walk, &[], None, &keys).unwrap();
        let with_edge_chord = triangulate_face(&walk, &[[0, 4]], None, &keys).unwrap();
        assert_eq!(plain, with_edge_chord, "an existing edge changes nothing");
    }

    // §7.3's contract: computed once, consumed twice, and a disagreement is a hard error.
    #[test]
    fn the_cache_computes_once_and_rejects_a_changed_fingerprint() {
        let (_, keys, walk) = square();
        let mut cache = FaceTriCache::new();
        let key = face_key([0, 1, 2], &keys);

        let mut computed = 0;
        let first = cache
            .get_or_insert(key, vec![7], || {
                computed += 1;
                triangulate_face(&walk, &[[4, 5]], None, &keys).unwrap()
            })
            .expect("first call inserts")
            .to_vec();
        let second = cache
            .get_or_insert(key, vec![7], || {
                computed += 1;
                Vec::new()
            })
            .expect("second call hits")
            .to_vec();
        assert_eq!(computed, 1, "the face is triangulated once, not once per cell");
        assert_eq!(first, second, "both cells consume identical triangles");
        assert_eq!((cache.hits, cache.misses), (1, 1));

        // The neighbour thinks a different constraint set crosses this face.
        let clash = cache.get_or_insert(key, vec![9], Vec::new);
        assert_eq!(
            clash,
            Err(FingerprintMismatch {
                face: key,
                cached: vec![7],
                requested: vec![9],
            }),
            "a fingerprint mismatch is a hard error, never a recompute"
        );
    }

    // **Invariant J2, the frozen spec's own test obligation (T-J2).**
    //
    // > The generic constrained face triangulator MUST reproduce §5.2's tables exactly on the
    // > inputs those tables cover. There is one shared routine; the kirigami tables are its
    // > closed-form values, not a parallel implementation.
    //
    // This decides whether the face cache can be wired in at all. Integration replaces per-cell
    // face meshing with the cache, so `triangulate_face` takes over from §5.2's table on **every**
    // ordinary face, not only creased ones — and §5.2 is verified at zero violations over 4,800
    // randomised cuts. A disagreement anywhere means the integration is dead on arrival, and it is
    // very much cheaper to learn that here than from a8's boundary leaks.
    //
    // Compared as triangle *sets* with each triangle's nodes sorted: §5.2 re-orients its output to
    // the calling cell's winding, so winding is the caller's business and not the table's identity.
    #[test]
    fn j2_reproduces_the_frozen_face_split_table() {
        use crate::meshgen::cut::{face_split, FaceCutState};

        // a, b, c are the parent corners in canonical (key) order; 3..6 are cut nodes on the
        // edges (a,b), (b,c), (c,a); 6 is an interior rim point.
        let points = vec![
            Vec3::new(0.0, 0.0, 0.0),
            Vec3::new(1.0, 0.0, 0.0),
            Vec3::new(0.0, 1.0, 0.0),
            Vec3::new(0.5, 0.0, 0.0),
            Vec3::new(0.5, 0.5, 0.0),
            Vec3::new(0.0, 0.5, 0.0),
            Vec3::new(0.25, 0.25, 0.0),
        ];
        let keys = keys_for(&points);
        let (a, b, c) = (0u32, 1, 2);
        let (m_ab, m_bc, m_ca, rim) = (3u32, 4, 5, 6);

        // The face's boundary walk with each present cut node inserted on its edge — the same
        // loop both incident cells derive from the face.
        let walk_of = |cut: [Option<u32>; 3]| -> Vec<u32> {
            let nodes = [a, b, c];
            let mut walk = Vec::new();
            for edge in 0..3 {
                walk.push(nodes[edge]);
                if let Some(node) = cut[edge] {
                    walk.push(node);
                }
            }
            walk
        };
        let normalise = |tris: Vec<[u32; 3]>| {
            let mut out: Vec<[u32; 3]> = tris
                .into_iter()
                .map(|mut t| {
                    t.sort_unstable();
                    t
                })
                .collect();
            out.sort_unstable();
            out
        };

        // (name, cut nodes per edge, on-cut vertices, rim, the constraint chords, the hub)
        let cases: Vec<(&str, [Option<u32>; 3], [bool; 3], Option<u32>, Vec<[u32; 2]>, Option<u32>)> = vec![
            ("uncut", [None, None, None], [false; 3], None, vec![], None),
            // split_2: one cut edge, the opposite vertex on the patch. The constraint runs from
            // the cut node to that vertex.
            ("split_2", [Some(m_ab), None, None], [false, false, true], None, vec![[m_ab, c]], None),
            // split_3: two cut edges; the constraint is the chord between the two cut nodes.
            ("split_3", [Some(m_ab), Some(m_bc), None], [false; 3], None, vec![[m_ab, m_bc]], None),
            // split_4: the medial split, three cut edges and three chords.
            (
                "split_4",
                [Some(m_ab), Some(m_bc), Some(m_ca)],
                [false; 3],
                None,
                vec![[m_ab, m_bc], [m_bc, m_ca], [m_ca, m_ab]],
                None,
            ),
            // split_R: the cut front ends at an interior point of the face, which is a shared
            // node of both cells — a hub, exactly as the crease case needs.
            ("split_R", [Some(m_ab), None, None], [false; 3], Some(rim), vec![], Some(rim)),
        ];

        for (name, cut, on_cut, rim_node, chords, hub) in cases {
            let state = FaceCutState {
                nodes: [a, b, c],
                cut,
                on_cut,
                rim: rim_node,
            };
            let frozen = face_split(&state, &keys)
                .unwrap_or_else(|| panic!("{name}: §5.2 must accept this state"));
            let walk = walk_of(cut);
            let generic = triangulate_face(&walk, &chords, hub, &keys)
                .unwrap_or_else(|| panic!("{name}: the generic triangulator must accept it too"));

            assert_eq!(
                normalise(frozen.to_vec()),
                normalise(generic),
                "J2 violated on {name}: the generic triangulator disagrees with §5.2's frozen table"
            );
        }
    }

    // Chord order is the caller's accident; the triangulation may not depend on it.
    #[test]
    fn chord_order_does_not_change_the_result() {
        let points = vec![
            Vec3::new(0.0, 0.0, 0.0),
            Vec3::new(1.0, 0.0, 0.0),
            Vec3::new(1.0, 1.0, 0.0),
            Vec3::new(0.0, 1.0, 0.0),
            Vec3::new(0.5, 0.0, 0.0),
            Vec3::new(1.0, 0.5, 0.0),
            Vec3::new(0.5, 1.0, 0.0),
            Vec3::new(0.0, 0.5, 0.0),
        ];
        let keys = keys_for(&points);
        let walk = vec![0u32, 4, 1, 5, 2, 6, 3, 7];
        let one = triangulate_face(&walk, &[[4, 6], [5, 7]], None, &keys).unwrap();
        let other = triangulate_face(&walk, &[[5, 7], [4, 6]], None, &keys).unwrap();
        assert_eq!(one, other);
    }
}
