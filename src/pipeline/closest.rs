use crate::compute::backend::AccelerationMode;
use crate::config::ClosestConfig;
use crate::error::{Result, RustMsptError};
use crate::geometry::closest::{nearest_all, Kind, Nearest, PrimBvh, Primitives, DISTANCE_TIE};
use crate::pipeline::{run_in_cpu_pool, Pipeline};
use crate::transfer::npy::{read_npy, write_npy_f64, write_npy_i64};
use std::path::Path;
use std::time::Instant;

pub const METHOD_VERSION: &str = "closest-point-v1";

pub struct ClosestPipeline {
    pub config: ClosestConfig,
}

// AI-FUNC-SUMMARY: Which backend proposed the seeds and why: "cpu" or "gpu", the adapter, and the reason a requested GPU was not used; carries no state.
#[derive(Debug, Clone, serde::Serialize)]
struct Backend {
    used: &'static str,
    requested: String,
    adapter: Option<String>,
    fallback_reason: Option<String>,
}

impl Pipeline for ClosestPipeline {
    // AI-FUNC-SUMMARY:
    // Purpose: The nearest point on a set of triangles or segments for every query point; write primitive, closest point, distance and a report.
    // Inputs: self.config (paths, worker cap, acceleration).
    // Returns: Ok, or an error for unreadable/ragged inputs, both or neither of triangles and segments, a GPU failure when CPU fallback is off, or an I/O failure.
    // Side effects: Reads three .npy files; creates output_dir and writes primitive.npy, closest.npy, distance.npy, report.json; prints one summary line.
    // Notes: The least exact f64 distance wins, ties within DISTANCE_TIE (relative) going to the lowest primitive index; the GPU only proposes a seed that bounds the exact search, so GPU and CPU answers are identical. See docs/en-us/algorithms/closest-point.md.
    fn run(&self) -> Result<()> {
        let p = &self.config.closest;
        let started = Instant::now();
        let (kind, corners_path) = match (&p.triangles, &p.segments) {
            (Some(t), None) => (Kind::Triangles, t),
            (None, Some(s)) => (Kind::Segments, s),
            _ => {
                return Err(RustMsptError::InvalidConfig(
                    "closest needs exactly one of triangles and segments".into(),
                ))
            }
        };
        let vertices = read_npy(Path::new(&p.vertices))?;
        let corners = read_npy(Path::new(corners_path))?;
        let query = read_npy(Path::new(&p.queries))?;
        if vertices.shape.last() != Some(&3) || query.shape.last() != Some(&3) {
            return Err(RustMsptError::InvalidConfig(
                "vertices and queries must have a last axis of 3".into(),
            ));
        }
        if corners.shape.len() != 2 || corners.shape[1] != kind.corners() {
            return Err(RustMsptError::InvalidConfig(format!(
                "{corners_path} must be shaped (m, {})",
                kind.corners()
            )));
        }
        let query_shape: Vec<usize> = query.shape[..query.shape.len() - 1].to_vec();
        let prims = Primitives::new(kind, &vertices.to_f64()?, &corners.to_index()?)?;
        let queries = query.to_f64()?;
        let read_s = started.elapsed().as_secs_f64();
        let requested = crate::compute::policy::configured_mode(&self.config.acceleration)?;
        let fallback_allowed = self.config.acceleration.cpu_fallback;
        let (nearest, backend, build_s, seed_s, search_s, depth, workers) =
            run_in_cpu_pool("closest", p.cpu_max, || {
                let t = Instant::now();
                let bvh = PrimBvh::build(&prims);
                let build_s = t.elapsed().as_secs_f64();
                let t = Instant::now();
                let (seeds, backend) = seeds(&prims, &bvh, &queries, requested, fallback_allowed)?;
                let seed_s = t.elapsed().as_secs_f64();
                let t = Instant::now();
                let nearest: Nearest = nearest_all(&prims, &bvh, &queries, seeds.as_deref())?;
                Ok((nearest, backend, build_s, seed_s, t.elapsed().as_secs_f64(), bvh.depth, rayon::current_num_threads()))
            })?;
        let out = Path::new(&p.output_dir);
        std::fs::create_dir_all(out)?;
        let t = Instant::now();
        let mut point_shape = query_shape.clone();
        point_shape.push(3);
        write_npy_i64(&out.join("primitive.npy"), &query_shape, &nearest.primitive)?;
        write_npy_f64(&out.join("closest.npy"), &point_shape, &nearest.closest)?;
        write_npy_f64(&out.join("distance.npy"), &query_shape, &nearest.distance)?;
        let write_s = t.elapsed().as_secs_f64();
        let identity = crate::version::build_identity();
        let report = serde_json::json!({
            "method_version": METHOD_VERSION,
            "tool": serde_json::to_value(&identity).unwrap_or(serde_json::Value::Null),
            "rule": "the primitive at the least exact f64 distance wins; primitives within distance_tie (relative) of it tie and the lowest index wins",
            "distance_tie": DISTANCE_TIE,
            "kind": kind,
            "vertices": prims.vertices.len(),
            "primitives": prims.len(),
            "query_shape": query_shape,
            "backend": backend,
            "workers": workers,
            "bvh": {"depth": depth},
            "wall_time_s": {
                "read": read_s,
                "build": build_s,
                "seed": seed_s,
                "search": search_s,
                "write": write_s,
                "total": started.elapsed().as_secs_f64(),
            },
        });
        std::fs::write(out.join("report.json"), serde_json::to_string_pretty(&report).unwrap_or_default())?;
        println!(
            "[closest] {} points on {} {:?}; backend {}; {} workers; seed {:.2} s, search {:.2} s",
            nearest.distance.len(),
            prims.len(),
            kind,
            backend.used,
            workers,
            seed_s,
            search_s
        );
        Ok(())
    }
}

// AI-FUNC-SUMMARY:
// Purpose: GPU seeds for the exact search on the requested backend; none on the CPU (the search then starts unbounded).
// Inputs: primitives, BVH, query points, requested mode, whether CPU fallback is allowed.
// Returns: (optional seeds, Backend) or the error that stopped a GPU run with fallback forbidden.
// Side effects: May create the shared GPU device.
fn seeds(
    prims: &Primitives,
    bvh: &PrimBvh,
    queries: &[f64],
    requested: AccelerationMode,
    fallback_allowed: bool,
) -> Result<(Option<Vec<u32>>, Backend)> {
    let cpu = |reason: Option<String>| Backend {
        used: "cpu",
        requested: requested.to_string(),
        adapter: None,
        fallback_reason: reason,
    };
    if requested == AccelerationMode::Cpu {
        return Ok((None, cpu(None)));
    }
    match gpu_seeds(prims, bvh, queries) {
        Ok((seeds, adapter)) => Ok((
            Some(seeds),
            Backend {
                used: "gpu",
                requested: requested.to_string(),
                adapter: Some(adapter),
                fallback_reason: None,
            },
        )),
        Err(reason) if requested == AccelerationMode::Auto || fallback_allowed => Ok((None, cpu(Some(reason)))),
        Err(reason) => Err(RustMsptError::Gpu(reason)),
    }
}

// AI-FUNC-SUMMARY: The GPU's f32 nearest primitive per point; returns (seeds, adapter description) or the reason the GPU could not run.
#[cfg(feature = "gpu")]
fn gpu_seeds(prims: &Primitives, bvh: &PrimBvh, queries: &[f64]) -> std::result::Result<(Vec<u32>, String), String> {
    let gpu = crate::gpu::GpuClosestSeedPipeline::new()?;
    let seeds = gpu.seeds(prims, bvh, queries)?;
    Ok((seeds, gpu.adapter().to_string()))
}

// AI-FUNC-SUMMARY: Stand-in when the gpu feature is off; returns the reason the GPU path is unavailable.
#[cfg(not(feature = "gpu"))]
fn gpu_seeds(_prims: &Primitives, _bvh: &PrimBvh, _queries: &[f64]) -> std::result::Result<(Vec<u32>, String), String> {
    Err("cargo feature 'gpu' is not enabled".into())
}
