// Probe: convex-decomposition part counts and volume vs hull for the shape library.
use parry3d_f64::na::Point3;
use parry3d_f64::shape::{ConvexPolyhedron, Shape};
use parry3d_f64::transformation::vhacd::{VHACDParameters, VHACD};
use std::time::Instant;
fn main() {
    let args: Vec<String> = std::env::args().collect();
    let conc: f64 = args[1].parse().unwrap();
    let res: u32 = args[2].parse().unwrap();
    for f in &args[3..] {
        let m = rustmspt::io::stl::load_stl(std::path::Path::new(f)).unwrap();
        let pts: Vec<Point3<f64>> = m.vertices.iter().map(|v| Point3::new(v.x, v.y, v.z)).collect();
        let idx: Vec<[u32; 3]> = m.faces.iter().map(|t| [t.a as u32, t.b as u32, t.c as u32]).collect();
        let tri = parry3d_f64::shape::TriMesh::new(pts.clone(), idx.clone()).unwrap();
        let vol = tri.mass_properties(1.0).mass();
        let hull = ConvexPolyhedron::from_convex_hull(&pts).unwrap();
        let hv = hull.mass_properties(1.0).mass();
        let t = Instant::now();
        let p = VHACDParameters { concavity: conc, resolution: res, ..Default::default() };
        let d = VHACD::decompose(&p, &pts, &idx, true);
        let parts = d.compute_exact_convex_hulls(&pts, &idx);
        let mut pv = 0.0;
        let mut nv = 0;
        for (v, i) in &parts {
            if let Some(c) = ConvexPolyhedron::from_convex_mesh(v.clone(), i) { pv += c.mass_properties(1.0).mass(); }
            nv += v.len();
        }
        println!("{} faces {} vol/hull {:.3} parts {} sumparts/vol {:.3} verts {} {:.2}s", f.rsplit('/').next().unwrap(), idx.len(), vol.abs() / hv, parts.len(), pv / vol.abs(), nv, t.elapsed().as_secs_f64());
    }
}
