use crate::compute::backend::AccelerationMode;
use crate::config::StateMapConfig;
use crate::error::{Result, RustMsptError};
use crate::pipeline::{run_in_cpu_pool, Pipeline};
use crate::transfer::locate::{locate_cpu, Located, TetBvh, TetMesh, BARYCENTRIC_TOLERANCE, BOX_RELATIVE_PAD};
use crate::transfer::npy::{read_npy, write_npy_f64, write_npy_i64};
use std::path::Path;
use std::time::Instant;

pub const METHOD_VERSION: &str = "state-map-locate-v1";

pub struct StateMapPipeline {
    pub config: StateMapConfig,
}

// AI-FUNC-SUMMARY: Which backend located the points and why: "cpu" or "gpu", the adapter, and the reason a requested GPU was not used; carries no state.
#[derive(Debug, Clone, serde::Serialize)]
struct Backend {
    used: &'static str,
    requested: String,
    adapter: Option<String>,
    fallback_reason: Option<String>,
}

impl Pipeline for StateMapPipeline {
    // AI-FUNC-SUMMARY:
    // Purpose: Locate a remesh's sampling points in the last frame's mesh and write donors, distances and a report.
    // Inputs: self.config (paths, fallback distance, worker cap, acceleration).
    // Returns: Ok, or an error for unreadable/ragged inputs, a bad bound, a GPU failure when CPU fallback is off, or an I/O failure.
    // Side effects: Reads three .npy files; creates output_dir and writes donors.npy, distances.npy, report.json; prints one summary line.
    // Notes: GPU and CPU give identical answers (the GPU only proposes a superset of candidates). See docs/en-us/algorithms/state-map-location.md.
    fn run(&self) -> Result<()> {
        let p = &self.config.state_map;
        let started = Instant::now();
        let source_points = read_npy(Path::new(&p.source_points))?;
        let source_cells = read_npy(Path::new(&p.source_cells))?;
        let query = read_npy(Path::new(&p.query_points))?;
        if query.shape.last() != Some(&3) || source_points.shape.last() != Some(&3) {
            return Err(RustMsptError::InvalidConfig(
                "source_points and query_points must have a last axis of 3".into(),
            ));
        }
        let query_shape: Vec<usize> = query.shape[..query.shape.len() - 1].to_vec();
        let points = source_points.to_f64()?;
        let cells = source_cells.to_index()?;
        let queries = query.to_f64()?;
        let read_s = started.elapsed().as_secs_f64();
        let requested = crate::compute::policy::configured_mode(&self.config.acceleration)?;
        let fallback_allowed = self.config.acceleration.cpu_fallback;
        let (located, backend, build_s, locate_s, depth, nodes, workers) =
            run_in_cpu_pool("state-map", p.cpu_max, || {
                let t = Instant::now();
                let mesh = TetMesh::new(&points, &cells)?;
                let bvh = TetBvh::build(&mesh);
                let build_s = t.elapsed().as_secs_f64();
                let t = Instant::now();
                let (located, backend) = locate(&mesh, &bvh, &queries, p, requested, fallback_allowed)?;
                Ok((located, backend, build_s, t.elapsed().as_secs_f64(), bvh.depth, bvh.link.len(), rayon::current_num_threads()))
            })?;
        let out = Path::new(&p.output_dir);
        std::fs::create_dir_all(out)?;
        let t = Instant::now();
        write_npy_i64(&out.join("donors.npy"), &query_shape, &located.donors)?;
        write_npy_f64(&out.join("distances.npy"), &query_shape, &located.distances)?;
        let write_s = t.elapsed().as_secs_f64();
        let identity = crate::version::build_identity();
        let report = serde_json::json!({
            "method_version": METHOD_VERSION,
            "tool": serde_json::to_value(&identity).unwrap_or(serde_json::Value::Null),
            "rule": "a containing source tet wins (nearest centroid among several, then lower index); else the nearest tet within fallback_distance (then lower index); else none",
            "barycentric_tolerance": BARYCENTRIC_TOLERANCE,
            "box_relative_pad": BOX_RELATIVE_PAD,
            "fallback_distance": p.fallback_distance,
            "source_points": points.len() / 3,
            "source_cells": cells.len() / 4,
            "query_shape": query_shape,
            "counts": located.counts,
            "backend": backend,
            "workers": workers,
            "bvh": {"depth": depth, "nodes": nodes},
            "wall_time_s": {
                "read": read_s,
                "build": build_s,
                "locate": locate_s,
                "write": write_s,
                "total": started.elapsed().as_secs_f64(),
            },
        });
        std::fs::write(
            out.join("report.json"),
            serde_json::to_string_pretty(&report).unwrap_or_default(),
        )?;
        println!(
            "[state-map] {} points: {} contained, {} fallback, {} unlocated; backend {}; {} workers; locate {:.2} s",
            located.counts.points,
            located.counts.contained,
            located.counts.fallback,
            located.counts.unlocated,
            backend.used,
            workers,
            locate_s
        );
        Ok(())
    }
}

// AI-FUNC-SUMMARY:
// Purpose: Run the location on the requested backend, falling back to the CPU when allowed.
// Inputs: mesh, BVH, query points, parameters, requested mode, whether CPU fallback is allowed.
// Returns: (Located, Backend) or the error that stopped a GPU run with fallback forbidden.
// Side effects: May create the shared GPU device.
fn locate(
    mesh: &TetMesh,
    bvh: &TetBvh,
    queries: &[f64],
    p: &crate::config::StateMapParams,
    requested: AccelerationMode,
    fallback_allowed: bool,
) -> Result<(Located, Backend)> {
    let cpu = |reason: Option<String>| -> Result<(Located, Backend)> {
        Ok((
            locate_cpu(mesh, bvh, queries, p.fallback_distance)?,
            Backend {
                used: "cpu",
                requested: requested.to_string(),
                adapter: None,
                fallback_reason: reason,
            },
        ))
    };
    if requested == AccelerationMode::Cpu {
        return cpu(None);
    }
    match gpu_locate(mesh, bvh, queries, p) {
        Ok((located, adapter)) => Ok((
            located,
            Backend {
                used: "gpu",
                requested: requested.to_string(),
                adapter: Some(adapter),
                fallback_reason: None,
            },
        )),
        Err(reason) if requested == AccelerationMode::Auto || fallback_allowed => cpu(Some(reason)),
        Err(reason) => Err(RustMsptError::Gpu(reason)),
    }
}

// AI-FUNC-SUMMARY: GPU candidate pass then exact host completion; returns (Located, adapter description) or the reason the GPU could not run.
#[cfg(feature = "gpu")]
fn gpu_locate(
    mesh: &TetMesh,
    bvh: &TetBvh,
    queries: &[f64],
    p: &crate::config::StateMapParams,
) -> std::result::Result<(Located, String), String> {
    let gpu = crate::gpu::GpuTetCandidatePipeline::new()?;
    let (counts, slots) = gpu.candidates(bvh, queries, p.gpu_slots)?;
    let located = crate::transfer::locate::locate_from_candidates(
        mesh,
        bvh,
        queries,
        p.fallback_distance,
        &counts,
        &slots,
        p.gpu_slots,
    )
    .map_err(|e| e.to_string())?;
    Ok((located, gpu.adapter().to_string()))
}

// AI-FUNC-SUMMARY: Stand-in when the gpu feature is off; returns the reason the GPU path is unavailable.
#[cfg(not(feature = "gpu"))]
fn gpu_locate(
    _mesh: &TetMesh,
    _bvh: &TetBvh,
    _queries: &[f64],
    _p: &crate::config::StateMapParams,
) -> std::result::Result<(Located, String), String> {
    Err("cargo feature 'gpu' is not enabled".into())
}
