pub mod crop;
pub mod forge;
pub mod measure;
pub mod mesh_render;
pub mod mesh_verify;
pub mod meshgen;
pub mod optimize;
mod optimize_execution;
mod optimize_volume;
pub mod pack;
pub mod pack_targets;
pub mod placement;
pub mod placement_feasibility;
pub mod placement_labels;
pub mod placement_library;
pub mod placement_outputs;
pub mod placement_sizes;
pub mod render;
pub mod rng;
pub mod rotation;
pub mod scale;
pub mod split_filter;
pub mod timing;

use crate::error::Result;
use indicatif::{ProgressBar, ProgressStyle};
use std::io::IsTerminal;

pub trait Pipeline {
    // AI-FUNC-SUMMARY: Run one pipeline end-to-end; returns Ok(()) or error; side effects: varies by pipeline (reads input, writes output, prints progress).
    fn run(&self) -> Result<()>;
}

// AI-FUNC-SUMMARY: Run `work` inside a dedicated Rayon pool sized from a `cpu_max` setting (absent or -1: every available worker, otherwise clamped to 1..available); returns work's result or a pool-construction error; side effects: builds and installs the pool.
// Notes: Every parallel section of the run, and any CPU fallback inside it, then shares one worker budget
// instead of the global pool (PERF-02).
pub(crate) fn run_in_cpu_pool<T: Send>(label: &str, cpu_max: Option<i32>, work: impl FnOnce() -> Result<T> + Send) -> Result<T> {
    let available = std::thread::available_parallelism().map(|n| n.get()).unwrap_or(1);
    let requested = cpu_max.unwrap_or(-1);
    let workers = if requested == -1 { available } else { (requested.max(1) as usize).min(available) };
    let pool = rayon::ThreadPoolBuilder::new()
        .num_threads(workers)
        .build()
        .map_err(|e| crate::error::RustMsptError::InvalidConfig(format!("{label} worker pool: {e}")))?;
    pool.install(work)
}

// AI-FUNC-SUMMARY:
// Purpose: Build a standardized progress bar with tty-aware visibility.
// Inputs: total length, template string, and progress characters.
// Returns: Configured ProgressBar (hidden when stderr is not a terminal).
// Side effects: None.
pub fn create_progress_bar(length: u64, template: &str, chars: &str) -> ProgressBar {
    let bar = ProgressBar::new(length);
    if !std::io::stderr().is_terminal() {
        bar.set_draw_target(indicatif::ProgressDrawTarget::hidden());
    }
    bar.set_style(
        ProgressStyle::with_template(template)
            .unwrap_or_else(|_| ProgressStyle::default_bar())
            .progress_chars(chars),
    );
    bar
}
