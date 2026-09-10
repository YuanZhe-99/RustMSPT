use clap::{Parser, Subcommand};
use rustmspt::config::{
    load_yaml, CropConfig, ForgingConfig, MeasurementConfig, MeshGenConfig, MeshRenderConfig,
    MeshVerifyConfig, OptimizationConfig, PackingConfig, RenderConfig, ScaleConfig,
    SplitFilterConfig,
};
use rustmspt::pipeline::crop::CropPipeline;
use rustmspt::pipeline::forge::ForgePipeline;
use rustmspt::pipeline::measure::MeasurePipeline;
use rustmspt::pipeline::mesh_render::MeshRenderPipeline;
use rustmspt::pipeline::mesh_verify::MeshVerifyPipeline;
use rustmspt::pipeline::meshgen::MeshGenPipeline;
use rustmspt::pipeline::optimize::OptimizePipeline;
use rustmspt::pipeline::pack::PackPipeline;
use rustmspt::pipeline::render::RenderPipeline;
use rustmspt::pipeline::scale::ScalePipeline;
use rustmspt::pipeline::split_filter::SplitFilterPipeline;
use rustmspt::pipeline::Pipeline;
use rustmspt::version::{build_identity, identity_json};
use std::path::{Path, PathBuf};

#[derive(Debug, Parser)]
#[command(name = "rustmspt")]
#[command(about = "Rust Microstructure Processing Toolbox", long_about = None)]
#[command(version = build_identity_version_line())]
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
    /// Report what this binary is: version, git commit, worktree state, features.
    Version {
        /// Print a machine-readable JSON object instead of one line.
        #[arg(long)]
        json: bool,
    },
    Render {
        #[arg(long)]
        config: Option<PathBuf>,
        #[arg(long)]
        input: Option<PathBuf>,
        #[arg(long)]
        output: Option<PathBuf>,
    },
    MeshRender {
        #[arg(long)]
        config: Option<PathBuf>,
        #[arg(long)]
        input: Option<PathBuf>,
        #[arg(long)]
        output: Option<PathBuf>,
    },
    /// Generate a tetrahedral volume mesh from input STL surfaces (S0/S1/G2-1..G2-5; S3-S11 pending).
    Mesh {
        #[arg(long)]
        config: Option<PathBuf>,
        /// Replace `meshgen.inputs` with a single STL.
        #[arg(long)]
        input: Option<PathBuf>,
        /// Replace `meshgen.output.vtu`.
        #[arg(long)]
        output: Option<PathBuf>,
    },
    /// Verify a contract or external tetrahedral VTU against the check catalog.
    MeshVerify {
        #[arg(long)]
        config: Option<PathBuf>,
        #[arg(long)]
        input: Option<PathBuf>,
        /// Human-readable report destination (also printed to stdout).
        #[arg(long)]
        report: Option<PathBuf>,
        /// JSON report destination (SPEC_meshgen_contracts §4.1).
        #[arg(long)]
        json: Option<PathBuf>,
        /// Write a copy of the mesh carrying the quality arrays and `verify_flags`.
        #[arg(long)]
        annotate: Option<PathBuf>,
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

// AI-FUNC-SUMMARY:
// Purpose: Provide clap's --version string, which is the same line the version subcommand prints.
// Inputs: None.
// Returns: A leaked static string holding the identity line.
// Side effects: Leaks one small allocation for the lifetime of the process.
// Notes: clap needs a &'static str and prepends the program name itself, so this omits the name.
fn build_identity_version_line() -> &'static str {
    Box::leak(build_identity().version_detail().into_boxed_str())
}

// AI-FUNC-SUMMARY: Build the default config file path under data/input; returns PathBuf; side effects: None.
fn default_config_path(file_name: &str) -> PathBuf {
    Path::new("data").join("input").join(file_name)
}

// AI-FUNC-SUMMARY: Select user-provided config path or fallback to default under data/input; returns PathBuf; side effects: None.
fn pick_config_path(config: Option<PathBuf>, file_name: &str) -> PathBuf {
    config.unwrap_or_else(|| default_config_path(file_name))
}

// AI-FUNC-SUMMARY:
// Purpose: Parse CLI arguments, load config, and execute the selected pipeline subcommand.
// Inputs: process CLI arguments (via clap).
// Returns: Ok(()) on success, or pipeline/config error.
// Side effects: Reads YAML config from disk; may write output files via pipeline execution; prints progress to stdout.
// Notes: Invoked once as the program entry point. CLI overrides (--input/--output) mutate the loaded config before running.
fn main() -> anyhow::Result<()> {
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
        Commands::Version { json } => {
            let identity = build_identity();
            if json {
                println!("{}", identity_json(&identity));
            } else {
                println!("{}", identity.version_line());
            }
        }
        Commands::Render {
            config,
            input,
            output,
        } => {
            let path = pick_config_path(config, "render_config.yaml");
            let mut conf: RenderConfig = load_yaml(&path)?;
            if let Some(input) = input {
                conf.render.stl_path = input.to_string_lossy().to_string();
            }
            if let Some(output) = output {
                conf.render.output_path = output.to_string_lossy().to_string();
            }
            RenderPipeline { config: conf }.run()?;
        }
        Commands::MeshRender {
            config,
            input,
            output,
        } => {
            let path = pick_config_path(config, "mesh_render_config.yaml");
            let mut conf: MeshRenderConfig = load_yaml(&path)?;
            if let Some(input) = input {
                conf.mesh_render.input = input.to_string_lossy().to_string();
            }
            if let Some(output) = output {
                conf.mesh_render.output_dir = output.to_string_lossy().to_string();
            }
            MeshRenderPipeline { config: conf }.run()?;
        }
        Commands::Mesh {
            config,
            input,
            output,
        } => {
            let path = pick_config_path(config, "meshgen_config.yaml");
            let mut conf: MeshGenConfig = load_yaml(&path)?;
            if let Some(input) = input {
                conf.meshgen.inputs = vec![rustmspt::config::meshgen::MeshGenInput {
                    stl: input.to_string_lossy().to_string(),
                    priority: None,
                    kind: rustmspt::config::meshgen::InputKind::default(),
                }];
            }
            if let Some(output) = output {
                conf.meshgen.output.vtu = output.to_string_lossy().to_string();
            }
            MeshGenPipeline { config: conf }.run()?;
        }
        Commands::MeshVerify {
            config,
            input,
            report,
            json,
            annotate,
        } => {
            let path = pick_config_path(config, "mesh_verify_config.yaml");
            let mut conf: MeshVerifyConfig = load_yaml(&path)?;
            if let Some(input) = input {
                conf.mesh_verify.input = input.to_string_lossy().to_string();
            }
            if let Some(report) = report {
                conf.mesh_verify.report = Some(report.to_string_lossy().to_string());
            }
            if let Some(json) = json {
                conf.mesh_verify.json = Some(json.to_string_lossy().to_string());
            }
            if let Some(annotate) = annotate {
                conf.mesh_verify.annotate = Some(annotate.to_string_lossy().to_string());
            }
            MeshVerifyPipeline { config: conf }.run()?;
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
