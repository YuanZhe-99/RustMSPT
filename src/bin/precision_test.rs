use rustmspt::geometry::{calculate_s2, l2_norm, mesh_bbox, volume_fraction_in_bbox};
use rustmspt::io::load_stl;
use std::path::Path;

fn main() {
    let mesh = load_stl(Path::new("data/input/particles.stl")).expect("load mesh");
    let bbox = mesh_bbox(&mesh).expect("mesh bbox");
    let r_max = 10usize;
    let vf = volume_fraction_in_bbox(&mesh, bbox);
    let n = 50000usize;

    println!("VF = {:.6}\n", vf);

    let pitches = [0.5, 1.0, 2.0, 4.0, 8.0];

    // CPU exact at different pitches
    println!("=== CPU Exact vs Pitch ===");
    let ref_exact = calculate_s2(&mesh, bbox, r_max, 0.5, "exact", n);
    for &p in &pitches {
        let s = calculate_s2(&mesh, bbox, r_max, p, "exact", n);
        let l2 = l2_norm(&s, &ref_exact);
        println!("  pitch={:.1}: S2[0..5]={:.6} {:.6} {:.6} {:.6} {:.6}  L2 vs p=0.5: {:.6}",
            p, s[0], s[1], s[2], s[3], s[4], l2);
    }
    println!();

    // CPU MC voxel at different pitches
    println!("=== CPU MC Voxel vs Pitch (N={}) ===", n);
    for &p in &pitches {
        let s = calculate_s2(&mesh, bbox, r_max, p, "monte_carlo", n);
        let l2 = l2_norm(&s, &ref_exact);
        println!("  pitch={:.1}: S2[0..5]={:.6} {:.6} {:.6} {:.6} {:.6}  L2 vs exact: {:.6}",
            p, s[0], s[1], s[2], s[3], s[4], l2);
    }
    println!();

    // GPU MC (no voxelization, pitch-independent)
    #[cfg(feature = "gpu")]
    {
        let mut gpu = match rustmspt::gpu::s2::GpuS2Pipeline::new(&mesh, bbox) {
            Ok(p) => p,
            Err(e) => { println!("[GPU] Init failed: {}", e); return; }
        };
        println!("=== GPU MC (N={}, no voxelization) ===", n);
        let t0 = std::time::Instant::now();
        let mut s2g = gpu.calculate_s2_gpu(bbox, r_max, n);
        let dt = t0.elapsed().as_secs_f64();
        s2g[0] = vf;
        let l2 = l2_norm(&s2g, &ref_exact);
        println!("  S2[0..5]={:.6} {:.6} {:.6} {:.6} {:.6}  L2 vs exact: {:.6}  time: {:.3}s",
            s2g[0], s2g[1], s2g[2], s2g[3], s2g[4], l2, dt);
    }
    println!();

    // Summary
    println!("=== Analysis ===");
    println!("GPU MC does continuous ray-casting (no voxels), so voxel_pitch does not affect it.");
    println!("CPU MC voxel uses occupancy grid, so voxel_pitch controls discretization granularity.");
    println!("CPU exact uses occupancy grid + FFT, so voxel_pitch affects both voxelization and correlation.");
}
