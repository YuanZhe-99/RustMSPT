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

use crate::error::Result;
use indicatif::{ProgressBar, ProgressStyle};
use std::io::IsTerminal;

pub trait Pipeline {
    // AI-FUNC-SUMMARY: Run one pipeline end-to-end; returns Ok(()) or error; side effects: varies by pipeline (reads input, writes output, prints progress).
    fn run(&self) -> Result<()>;
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
