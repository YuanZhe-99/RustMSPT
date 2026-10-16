use parry3d_f64::na::Point3;
use parry3d_f64::transformation::vhacd::{VHACDParameters, VHACD};
fn main() {
    let (mut hv, mut pv, mut np, mut n) = (0usize, 0usize, 0usize, 0usize);
    for f in std::env::args().skip(1) {
        let m = rustmspt::io::stl::load_stl(std::path::Path::new(&f)).unwrap();
        let pts: Vec<Point3<f64>> = m.vertices.iter().map(|v| Point3::new(v.x, v.y, v.z)).collect();
        let idx: Vec<[u32; 3]> = m.faces.iter().map(|t| [t.a as u32, t.b as u32, t.c as u32]).collect();
        hv += parry3d_f64::transformation::convex_hull(&pts).0.len();
        let d = VHACD::decompose(&VHACDParameters { concavity: 0.03, ..Default::default() }, &pts, &idx, true);
        let parts = d.compute_exact_convex_hulls(&pts, &idx);
        np += parts.len();
        pv += parts.iter().map(|p| p.0.len()).sum::<usize>();
        n += 1;
    }
    println!("shells {n} mean hull verts {:.0} mean parts {:.1} mean verts per part {:.0}", hv as f64 / n as f64, np as f64 / n as f64, pv as f64 / np as f64);
}
