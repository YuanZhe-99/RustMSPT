pub mod forging;
pub mod measurement;
pub mod optimization;
pub mod packing;
pub mod scale;

use crate::error::Result;

pub trait Pipeline {
    // Purpose: Run one pipeline end-to-end.
    // Inputs: pipeline instance state/config.
    // Outputs: success or pipeline error.
    fn run(&self) -> Result<()>;
}
