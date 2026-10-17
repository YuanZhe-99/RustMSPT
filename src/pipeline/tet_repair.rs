use crate::config::TetRepairConfig;
use crate::error::{Result, RustMsptError};
use crate::geometry::tet_repair::{repair, RepairParams};
use crate::pipeline::{run_in_cpu_pool, Pipeline};
use crate::transfer::npy::{read_npy, write_npy_f64, write_npy_i64};
use std::path::Path;
use std::time::Instant;

pub const METHOD_VERSION: &str = "tet-repair-v1";

pub struct TetRepairPipeline {
    pub config: TetRepairConfig,
}

impl Pipeline for TetRepairPipeline {
    // AI-FUNC-SUMMARY:
    // Purpose: Repair inverted cells and cells over the limit (flips, node moves, the joint pass) and write the mesh and a report.
    // Inputs: self.config (paths, limit, sweeps, wall_tol, padding, joint_tol, seed, worker cap).
    // Returns: Ok, or an error for unreadable or ragged arrays, an index out of range, or an I/O failure.
    // Side effects: Reads four or five .npy files; creates output_dir and writes points.npy, tets.npy, density.npy, report.json; prints one summary line.
    // Notes: See docs/en-us/algorithms/tet-repair.md. Deterministic for a given seed.
    fn run(&self) -> Result<()> {
        let c = &self.config.tet_repair;
        let started = Instant::now();
        let points = read_npy(Path::new(&c.points))?;
        let tets = read_npy(Path::new(&c.tets))?;
        let density = read_npy(Path::new(&c.density))?;
        let fixed = read_npy(Path::new(&c.fixed))?;
        if points.shape.last() != Some(&3) || tets.shape.last() != Some(&4) {
            return Err(RustMsptError::InvalidConfig("points must be (n, 3) and tets (m, 4)".into()));
        }
        let layer = match &c.layer {
            Some(path) => read_npy(Path::new(path))?.to_index()?,
            None => Vec::new(),
        };
        let pts = points.to_f64()?;
        let cells = tets.to_index()?;
        let dens: Vec<i64> = density.to_index()?.into_iter().map(i64::from).collect();
        let pinned: Vec<bool> = fixed.to_index()?.into_iter().map(|v| v != 0).collect();
        let prm = RepairParams {
            limit: c.limit,
            sweeps: c.sweeps,
            wall_tol: c.wall_tol,
            padding: c.padding,
            joint_tol: c.joint_tol,
            seed: c.seed,
        };
        let read_s = started.elapsed().as_secs_f64();
        let t = Instant::now();
        let (out, workers) = run_in_cpu_pool("tet-repair", c.cpu_max, || {
            Ok((repair(&pts, &cells, &dens, &pinned, &layer, &prm)?, rayon::current_num_threads()))
        })?;
        let repair_s = t.elapsed().as_secs_f64();
        let dir = Path::new(&c.output_dir);
        std::fs::create_dir_all(dir)?;
        let flat_p: Vec<f64> = out.points.iter().flatten().copied().collect();
        let flat_t: Vec<i64> = out.tets.iter().flatten().map(|&v| i64::from(v)).collect();
        write_npy_f64(&dir.join("points.npy"), &[out.points.len(), 3], &flat_p)?;
        write_npy_i64(&dir.join("tets.npy"), &[out.tets.len(), 4], &flat_t)?;
        write_npy_i64(&dir.join("density.npy"), &[out.density.len()], &out.density)?;
        let identity = crate::version::build_identity();
        let report = serde_json::json!({
            "method_version": METHOD_VERSION,
            "tool": serde_json::to_value(&identity).unwrap_or(serde_json::Value::Null),
            "params": {
                "limit": prm.limit, "sweeps": prm.sweeps, "wall_tol": prm.wall_tol,
                "padding": prm.padding, "joint_tol": prm.joint_tol, "seed": prm.seed,
            },
            "points": out.points.len(),
            "cells": out.tets.len(),
            "repair": out.report,
            "workers": workers,
            "wall_time_s": {"read": read_s, "repair": repair_s, "total": started.elapsed().as_secs_f64()},
        });
        std::fs::write(dir.join("report.json"), serde_json::to_string_pretty(&report).unwrap_or_default())?;
        println!(
            "[tet-repair] {} cells: {} bad before, {} flips, {} nodes moved; {} inverted and {} over {} after; {:.2} s",
            out.tets.len(),
            out.report.bad_before,
            out.report.flips.len(),
            out.report.nodes_moved.len(),
            out.report.inverted_after,
            out.report.over_limit_after,
            prm.limit,
            repair_s
        );
        Ok(())
    }
}
