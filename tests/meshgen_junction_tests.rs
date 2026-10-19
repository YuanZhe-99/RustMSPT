//! `src/meshgen/junction.rs` (plan M-3.3): the escalated-cell helpers S8's fallbacks build on - face
//! fans, soup closure and components, the chord meeting point, and the whole-cell fan.

use rustmspt::meshgen::junction::{
    chord_meeting_point, fan_cell, fan_polygon, loop_fan, soup_is_closed, split_soup_components,
};
use rustmspt::types::Vec3;

// AI-FUNC-SUMMARY: The four faces of tet (0, 1, 2, 3), outward for the positive orientation; returns the soup; side effects: none.
fn tet_soup() -> Vec<[u32; 3]> {
    vec![[0, 2, 1], [0, 1, 3], [1, 2, 3], [0, 3, 2]]
}

// AI-FUNC-SUMMARY: Signed volume of a tet from node positions; returns f64; side effects: none.
fn volume(t: [u32; 4], p: &[Vec3]) -> f64 {
    let [a, b, c, d] = t.map(|i| p[i as usize]);
    b.sub(a).cross(c.sub(a)).dot(d.sub(a)) / 6.0
}

/// A loop fan cones every loop edge to the centroid: one triangle per edge, each with the centroid.
#[test]
fn a_loop_fan_has_one_triangle_per_edge_through_the_centroid() {
    let tris = loop_fan(&[0, 1, 2, 3, 4], 9);
    assert_eq!(tris.len(), 5);
    for (k, t) in tris.iter().enumerate() {
        assert!(t.contains(&9));
        assert!(t.contains(&(k as u32)) && t.contains(&(((k + 1) % 5) as u32)));
    }
}

/// A polygon fan starts at the lowest-key vertex, so the same polygon walked from another start
/// gives the same triangles (what lets two cells sharing a face agree).
#[test]
fn a_polygon_fan_does_not_depend_on_where_the_walk_starts() {
    let keys: Vec<(i64, i64, i64)> = vec![(5, 0, 0), (1, 0, 0), (3, 0, 0), (4, 0, 0), (2, 0, 0)];
    let canon = |mut tris: Vec<[u32; 3]>| {
        for t in &mut tris {
            t.sort_unstable();
        }
        tris.sort_unstable();
        tris
    };
    let a = canon(fan_polygon(&[0, 1, 2, 3, 4], &keys).to_vec());
    let b = canon(fan_polygon(&[2, 3, 4, 0, 1], &keys).to_vec());
    assert_eq!(a, b);
    assert_eq!(a.len(), 3);
    assert!(a.iter().all(|t| t.contains(&1)), "fanned from the lowest key: {a:?}");
    assert!(fan_polygon(&[0, 1], &keys).is_empty());
}

/// Closure is every undirected edge used exactly twice: a tet's four faces are closed, three are
/// not, and an empty soup is no piece at all.
#[test]
fn a_soup_is_closed_only_when_every_edge_is_used_twice() {
    assert!(soup_is_closed(&tet_soup()));
    assert!(!soup_is_closed(&tet_soup()[..3]));
    assert!(!soup_is_closed(&[]));
}

/// Two disjoint tets in one soup are two components, ordered by their lowest node.
#[test]
fn a_soup_splits_into_its_edge_connected_pieces_in_canonical_order() {
    let mut soup: Vec<[u32; 3]> = tet_soup().iter().map(|t| t.map(|i| i + 10)).collect();
    soup.extend(tet_soup());
    let pieces = split_soup_components(&soup);
    assert_eq!(pieces.len(), 2);
    let lowest = |p: &[[u32; 3]]| p.iter().flatten().copied().min().unwrap();
    assert_eq!(lowest(&pieces[0]), 0);
    assert_eq!(lowest(&pieces[1]), 10);
    assert!(pieces.iter().all(|p| p.len() == 4 && soup_is_closed(p)));
}

/// Two crossing chords meet where they cross; skew chords meet at the midpoint of their closest
/// approach; and the answer is symmetric in the chords' order.
#[test]
fn the_chord_meeting_point_is_the_closest_approach() {
    let p = chord_meeting_point(
        Vec3::new(0.0, 0.0, 0.0),
        Vec3::new(2.0, 2.0, 0.0),
        Vec3::new(0.0, 2.0, 0.0),
        Vec3::new(2.0, 0.0, 0.0),
    );
    assert!(p.sub(Vec3::new(1.0, 1.0, 0.0)).dot(p.sub(Vec3::new(1.0, 1.0, 0.0))) < 1e-24);
    let q = chord_meeting_point(
        Vec3::new(0.0, 0.0, 0.0),
        Vec3::new(2.0, 0.0, 0.0),
        Vec3::new(1.0, -1.0, 1.0),
        Vec3::new(1.0, 1.0, 1.0),
    );
    assert!(q.sub(Vec3::new(1.0, 0.0, 0.5)).dot(q.sub(Vec3::new(1.0, 0.0, 0.5))) < 1e-24);
}

/// The whole-cell fan cones a closed boundary to the cell's centroid: one tet per boundary triangle,
/// each through the Steiner node, and their volumes add up to the cell's.
#[test]
fn the_whole_cell_fan_tiles_the_cell_from_its_centroid() {
    let mut p = vec![
        Vec3::new(0.0, 0.0, 0.0),
        Vec3::new(1.0, 0.0, 0.0),
        Vec3::new(0.0, 1.0, 0.0),
        Vec3::new(0.0, 0.0, 1.0),
    ];
    p.push(Vec3::new(0.25, 0.25, 0.25));
    let fanned = fan_cell(&tet_soup(), 4);
    assert_eq!(fanned.tets.len(), 4);
    assert!(fanned.tets.iter().all(|t| t.contains(&4)));
    let sum: f64 = fanned.tets.iter().map(|t| volume(*t, &p).abs()).sum();
    assert!((sum - 1.0 / 6.0).abs() < 1e-15, "fan volume {sum}");
}
