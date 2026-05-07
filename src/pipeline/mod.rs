pub mod forge;
pub mod measure;
pub mod optimize;
pub mod pack;
pub mod crop;
pub mod rotation;
pub mod scale;
pub mod split_filter;

use crate::error::Result;
use indicatif::{ProgressBar, ProgressStyle};
use std::io::IsTerminal;

pub trait Pipeline {
    // Purpose: Run one pipeline end-to-end.
    // Inputs: pipeline instance state/config.
    // Outputs: success or pipeline error.
    fn run(&self) -> Result<()>;
}

pub fn create_progress_bar(length: u64, template: &str, chars: &str) -> ProgressBar {
    // Purpose: Build a standardized progress bar with tty-aware visibility and fallback style.
    // Inputs: total length, template string, and progress characters.
    // Outputs: configured ProgressBar.
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
