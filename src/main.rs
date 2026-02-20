use clap::{Parser, Subcommand};
use rustmspt::config::{
    load_yaml, CropConfig, ForgingConfig, MeasurementConfig, OptimizationConfig, PackingConfig,
    ScaleConfig, SplitFilterConfig,
};
use rustmspt::pipeline::crop::CropPipeline;
use rustmspt::pipeline::forge::ForgePipeline;
use rustmspt::pipeline::measure::MeasurePipeline;
use rustmspt::pipeline::optimize::OptimizePipeline;
use rustmspt::pipeline::pack::PackPipeline;
use rustmspt::pipeline::scale::ScalePipeline;
use rustmspt::pipeline::split_filter::SplitFilterPipeline;
use rustmspt::pipeline::Pipeline;
use std::path::{Path, PathBuf};

#[derive(Debug, Parser)]
#[command(name = "rustmspt")]
#[command(about = "Rust Microstructure Processing Toolbox", long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Debug, Subcommand)]
enum Commands {
    Forge {
        #[arg(long)]
        config: Option<PathBuf>,
        #[arg(long)]
        input: Option<PathBuf>,
        #[arg(long)]
        output: Option<PathBuf>,
    },
    Measure {
        #[arg(long)]
        config: Option<PathBuf>,
        #[arg(long)]
        input: Option<PathBuf>,
        #[arg(long)]
        output: Option<PathBuf>,
    },
    Optimize {
        #[arg(long)]
        config: Option<PathBuf>,
        #[arg(long)]
        input: Option<PathBuf>,
        #[arg(long)]
        output: Option<PathBuf>,
    },
    Pack {
        #[arg(long)]
        config: Option<PathBuf>,
        #[arg(long)]
        input: Option<PathBuf>,
        #[arg(long)]
        output: Option<PathBuf>,
    },
    Scale {
        #[arg(long)]
        config: Option<PathBuf>,
        #[arg(long)]
        input: Option<PathBuf>,
        #[arg(long)]
        output: Option<PathBuf>,
    },
    Crop {
        #[arg(long)]
        config: Option<PathBuf>,
        #[arg(long)]
        input: Option<PathBuf>,
        #[arg(long)]
        output: Option<PathBuf>,
    },
    SplitFilter {
        #[arg(long)]
        config: Option<PathBuf>,
        #[arg(long)]
        input: Option<PathBuf>,
        #[arg(long)]
        output: Option<PathBuf>,
    },
}

fn default_config_path(file_name: &str) -> PathBuf {
    // Purpose: Build the default config file path under data/input.
    // Inputs: config file name.
    // Outputs: resolved default path.
    Path::new("data").join("input").join(file_name)
}

fn pick_config_path(config: Option<PathBuf>, file_name: &str) -> PathBuf {
    // Purpose: Select user-provided config path or fallback default path.
    // Inputs: optional config path and default file name.
    // Outputs: final config path to load.
    config.unwrap_or_else(|| default_config_path(file_name))
}

fn main() -> anyhow::Result<()> {
    // Purpose: Parse CLI arguments, load config, and execute selected pipeline.
    // Inputs: process CLI arguments.
    // Outputs: success or pipeline/config error.
    let cli = Cli::parse();

    match cli.command {
        Commands::Forge {
            config,
            input,
            output,
        } => {
            let path = pick_config_path(config, "forge_config.yaml");
            let mut conf: ForgingConfig = load_yaml(&path)?;
            if let Some(input) = input {
                conf.forging.input_stl_path = input.to_string_lossy().to_string();
            }
            if let Some(output) = output {
                conf.forging.output_stl_path = Some(output.to_string_lossy().to_string());
            }
            ForgePipeline { config: conf }.run()?;
        }
        Commands::Measure {
            config,
            input,
            output,
        } => {
            let path = pick_config_path(config, "measure_config.yaml");
            let mut conf: MeasurementConfig = load_yaml(&path)?;
            if let Some(input) = input {
                conf.measurement.stl_path = input.to_string_lossy().to_string();
            }
            if let Some(output) = output {
                conf.measurement.output_path = output.to_string_lossy().to_string();
            }
            MeasurePipeline { config: conf }.run()?;
        }
        Commands::Optimize {
            config,
            input,
            output,
        } => {
            let path = pick_config_path(config, "optimize_config.yaml");
            let mut conf: OptimizationConfig = load_yaml(&path)?;
            if let Some(input) = input {
                conf.input.stl_path = input.to_string_lossy().to_string();
            }
            if let Some(output) = output {
                conf.output.path = output.to_string_lossy().to_string();
            }
            OptimizePipeline { config: conf }.run()?;
        }
        Commands::Pack {
            config,
            input,
            output,
        } => {
            let path = pick_config_path(config, "pack_config.yaml");
            let mut conf: PackingConfig = load_yaml(&path)?;
            if let Some(input) = input {
                conf.input.path = input.to_string_lossy().to_string();
            }
            if let Some(output) = output {
                conf.output.path = output.to_string_lossy().to_string();
            }
            PackPipeline { config: conf }.run()?;
        }
        Commands::Scale {
            config,
            input,
            output,
        } => {
            let path = pick_config_path(config, "scale_config.yaml");
            let mut conf: ScaleConfig = load_yaml(&path)?;
            if let Some(input) = input {
                conf.input.stl_path = input.to_string_lossy().to_string();
            }
            if let Some(output) = output {
                conf.output.stl_path = output.to_string_lossy().to_string();
            }
            ScalePipeline { config: conf }.run()?;
        }
        Commands::Crop {
            config,
            input,
            output,
        } => {
            let path = pick_config_path(config, "crop_config.yaml");
            let mut conf: CropConfig = load_yaml(&path)?;
            if let Some(input) = input {
                conf.input.path = input.to_string_lossy().to_string();
            }
            if let Some(output) = output {
                conf.output.path = output.to_string_lossy().to_string();
            }
            CropPipeline { config: conf }.run()?;
        }
        Commands::SplitFilter {
            config,
            input,
            output,
        } => {
            let path = pick_config_path(config, "split_filter_config.yaml");
            let mut conf: SplitFilterConfig = load_yaml(&path)?;
            if let Some(input) = input {
                conf.input.path = input.to_string_lossy().to_string();
            }
            if let Some(output) = output {
                conf.output.folder = output.to_string_lossy().to_string();
            }
            SplitFilterPipeline { config: conf }.run()?;
        }
    }

    Ok(())
}
