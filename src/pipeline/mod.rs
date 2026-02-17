pub mod forge;
pub mod measure;
pub mod optimize;
pub mod pack;
pub mod scale;

use crate::error::Result;

pub trait Pipeline {
    // Purpose: Run one pipeline end-to-end.
    // Inputs: pipeline instance state/config.
    // Outputs: success or pipeline error.
    fn run(&self) -> Result<()>;
}
