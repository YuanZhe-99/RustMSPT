use clap::{Parser, Subcommand};
use rustmspt::config::{
    load_yaml, ForgingConfig, MeasurementConfig, OptimizationConfig, PackingConfig, ScaleConfig,
};
use rustmspt::pipeline::forging::ForgingPipeline;
use rustmspt::pipeline::measurement::MeasurementPipeline;
use rustmspt::pipeline::optimization::OptimizationPipeline;
use rustmspt::pipeline::packing::PackingPipeline;
use rustmspt::pipeline::scale::ScalePipeline;
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
    Forging {
        #[arg(long)]
        config: Option<PathBuf>,
        #[arg(long)]
        input: Option<PathBuf>,
        #[arg(long)]
        output: Option<PathBuf>,
    },
    Measurement {
        #[arg(long)]
        config: Option<PathBuf>,
        #[arg(long)]
        input: Option<PathBuf>,
        #[arg(long)]
        output: Option<PathBuf>,
    },
    Optimization {
        #[arg(long)]
        config: Option<PathBuf>,
        #[arg(long)]
        input: Option<PathBuf>,
        #[arg(long)]
        output: Option<PathBuf>,
    },
    Packing {
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
        Commands::Forging {
            config,
            input,
            output,
        } => {
            let path = pick_config_path(config, "forging_config.yaml");
            let mut conf: ForgingConfig = load_yaml(&path)?;
            if let Some(input) = input {
                conf.forging.input_stl_path = input.to_string_lossy().to_string();
            }
            if let Some(output) = output {
                conf.forging.output_stl_path = Some(output.to_string_lossy().to_string());
            }
            ForgingPipeline { config: conf }.run()?;
        }
        Commands::Measurement {
            config,
            input,
            output,
        } => {
            let path = pick_config_path(config, "measurement_config.yaml");
            let mut conf: MeasurementConfig = load_yaml(&path)?;
            if let Some(input) = input {
                conf.measurement.stl_path = input.to_string_lossy().to_string();
            }
            if let Some(output) = output {
                conf.measurement.output_path = output.to_string_lossy().to_string();
            }
            MeasurementPipeline { config: conf }.run()?;
        }
        Commands::Optimization {
            config,
            input,
            output,
        } => {
            let path = pick_config_path(config, "optimization_config.yaml");
            let mut conf: OptimizationConfig = load_yaml(&path)?;
            if let Some(input) = input {
                conf.input.stl_path = input.to_string_lossy().to_string();
            }
            if let Some(output) = output {
                conf.output.path = output.to_string_lossy().to_string();
            }
            OptimizationPipeline { config: conf }.run()?;
        }
        Commands::Packing {
            config,
            input,
            output,
        } => {
            let path = pick_config_path(config, "packing_config.yaml");
            let mut conf: PackingConfig = load_yaml(&path)?;
            if let Some(input) = input {
                conf.input.path = input.to_string_lossy().to_string();
            }
            if let Some(output) = output {
                conf.output.path = output.to_string_lossy().to_string();
            }
            PackingPipeline { config: conf }.run()?;
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
    }

    Ok(())
}
