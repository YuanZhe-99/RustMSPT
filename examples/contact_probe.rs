use parry3d_f64::math::Isometry;
use parry3d_f64::na::{Point3, Vector3, UnitQuaternion};
use parry3d_f64::query::details::contact_support_map_support_map;
use parry3d_f64::shape::ConvexPolyhedron;
fn main() {
    let mut pts = vec![];
    for x in [-0.5, 0.5] { for y in [-0.5, 0.5] { for z in [-0.5, 0.5] { pts.push(Point3::new(x, y, z)); } } }
    let cube = ConvexPolyhedron::from_convex_hull(&pts).unwrap();
    for (dx, rot) in [(0.9, 0.0), (0.5, 0.0), (0.2, 0.0), (0.95, 0.3), (0.6, 0.7)] {
        let ia = Isometry::from_parts(Vector3::new(1.0, 2.0, 3.0).into(), UnitQuaternion::from_euler_angles(0.1, 0.2, 0.0));
        let ib = Isometry::from_parts((Vector3::new(1.0 + dx, 2.0, 3.0)).into(), UnitQuaternion::from_euler_angles(0.0, 0.0, rot));
        let pos12 = ia.inv_mul(&ib);
        let c = contact_support_map_support_map(&pos12, &cube, &cube, 0.0);
        match c {
            Some(c) => println!("dx {dx} rot {rot}: dist {:.4} n_world {:?} p1 {:?}", c.dist, (ia * c.normal1.into_inner()).as_slice(), (ia * c.point1).coords.as_slice()),
            None => println!("dx {dx}: none"),
        }
    }
}
