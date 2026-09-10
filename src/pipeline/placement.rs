use crate::config::ResolvedPlacement;
use crate::error::{Result, RustMsptError};
use crate::pipeline::Pipeline;

/// Runs the seeded, recorded, void-aware placement engine.
///
/// Holds an already-validated config: every cross-field rule has been applied and
/// every path resolved by `PlacementParams::validate`, so nothing here decides what
/// a field means.
pub struct PlacementPipeline {
    pub config: ResolvedPlacement,
}

impl Pipeline for PlacementPipeline {
    // AI-FUNC-SUMMARY:
    // Purpose: Place particles into the domain under the resolved placement config.
    // Inputs: self.config, already validated.
    // Returns: Ok(()) once every output has been written.
    // Side effects: Reads the shape and void files; writes the geometry, record, report and CSV.
    // Notes: Not implemented yet - the engine lands with its own subtask. Returning NotAvailable
    // rather than a partial run means a config that parses today cannot be mistaken for a run.
    fn run(&self) -> Result<()> {
        Err(RustMsptError::NotAvailable(format!(
            "the placement engine is not implemented yet; the config at {} validated and resolved \
             cleanly, with seed {} and {} shape file(s)",
            self.config.config_path.display(),
            self.config.seed,
            self.config.shape_files.len()
        )))
    }
}
