use clap::{Parser, Subcommand};
use rustmspt::config::{
    load_pack_document, load_yaml, CropConfig, ForgingConfig,
    MeasurementConfig, MeshRenderConfig, OptimizationConfig,
    ClosestConfig, PackDocument, RenderConfig, ScaleConfig, SplitFilterConfig, StateMapConfig,
};
use rustmspt::pipeline::closest::ClosestPipeline;
use rustmspt::pipeline::crop::CropPipeline;
use rustmspt::pipeline::forge::ForgePipeline;
use rustmspt::pipeline::measure::MeasurePipeline;
use rustmspt::pipeline::mesh_render::MeshRenderPipeline;
use rustmspt::pipeline::optimize::OptimizePipeline;
use rustmspt::pipeline::pack::PackPipeline;
use rustmspt::pipeline::placement::PlacementPipeline;
use rustmspt::pipeline::render::RenderPipeline;
use rustmspt::pipeline::scale::ScalePipeline;
use rustmspt::pipeline::split_filter::SplitFilterPipeline;
use rustmspt::pipeline::state_map::StateMapPipeline;
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
        /// Seed the placement engine. Only the `placement:` engine is reproducible;
        /// passing this with a `packing:` config is an error rather than a no-op.
        #[arg(long)]
        seed: Option<u64>,
        /// Worker threads for the placement engine; -1 uses every available core.
        #[arg(long)]
        threads: Option<i32>,
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
    /// Locate a remesh's sampling points in the last frame's tetrahedral mesh (state mapping).
    StateMap {
        #[arg(long)]
        config: Option<PathBuf>,
        /// Replace `state_map.output_dir`.
        #[arg(long)]
        output: Option<PathBuf>,
    },
    /// The nearest point on a set of triangles or segments for every query point.
    Closest {
        #[arg(long)]
        config: Option<PathBuf>,
        /// Replace `closest.output_dir`.
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

// AI-FUNC-SUMMARY:
// Purpose: Re-express a path given on the command line so it survives config-relative resolution.
// Inputs: the config file's directory, the path as typed.
// Returns: the path as a string that resolve_against(dir, _) maps back to the same file.
// Side effects: None.
// Notes: A command-line path means what a shell means by it - relative to the working directory -
// while everything in the config resolves against the config's directory. Making the CLI path
// absolute first is what keeps both rules true at once; without it, `--input shapes/a.stl` run from
// elsewhere would silently resolve beside the config instead.
fn cli_path_as_config_relative(given: &Path) -> String {
    if given.is_absolute() {
        given.to_string_lossy().to_string()
    } else {
        match std::env::current_dir() {
            Ok(cwd) => cwd.join(given).to_string_lossy().to_string(),
            Err(_) => given.to_string_lossy().to_string(),
        }
    }
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
// Notes: Invoked once as the program entry point. With the gpu feature, a run that created a GPU device prints one `[Timing] gpu ...` transfer line, then cached shared GPU devices are released so logical devices are destroyed before process exit.
fn main() -> anyhow::Result<()> {
    let result = run_cli();
    #[cfg(feature = "gpu")]
    {
        if rustmspt::gpu::gpu_device_creation_count() > 0 {
            println!("{}", rustmspt::gpu::gpu_transfer_stats().describe());
        }
        rustmspt::gpu::release_shared_gpu_devices();
    }
    result
}

// AI-FUNC-SUMMARY: Parse CLI arguments, load the selected config and run its pipeline; returns the pipeline/config result; side effects: file I/O and progress output of the selected pipeline.
fn run_cli() -> anyhow::Result<()> {
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
            seed,
            threads,
        } => {
            let explicit_config = config.is_some();
            let path = pick_config_path(config, "pack_config.yaml");
            match load_pack_document(&path)? {
                PackDocument::Placement(mut params) => {
                    if !explicit_config {
                        // The legacy default path is resolved against the working
                        // directory. The placement engine promises that no path it
                        // reads depends on where it was launched from, so it refuses
                        // to be reached that way at all rather than quietly honouring
                        // a CWD-relative default.
                        return Err(anyhow::anyhow!(
                            "a placement config must be given with --config: {} was found relative to the \
                             current directory, and the placement engine resolves every path \
                             against its config file, never the working directory",
                            path.display()
                        ));
                    }
                    if let Some(seed) = seed {
                        params.seed = seed;
                    }
                    if let Some(threads) = threads {
                        params.threads = threads;
                    }
                    // A path typed on the command line is a shell argument, so it means
                    // what the shell means: relative to the working directory. A path
                    // written in the config means relative to the config. Both halves
                    // of that rule are load-bearing, and this is where they meet.
                    if let Some(input) = input {
                        params.shapes.files = vec![cli_path_as_config_relative(&input)];
                    }
                    if let Some(output) = output {
                        params.outputs.dir = cli_path_as_config_relative(&output);
                    }
                    let resolved = params.validate(&path)?;
                    let _signals = rustmspt::pipeline::placement_control::SignalGuard::install()?;
                    PlacementPipeline { config: resolved }.run()?;
                }
                PackDocument::Legacy(mut conf) => {
                    if seed.is_some() {
                        return Err(anyhow::anyhow!(
                            "--seed applies to the placement engine, and {} selects the original packing \
                             engine, which draws from an unseeded thread-local generator. Ignoring \
                             the flag would report a run as reproducible when it is not.",
                            path.display()
                        ));
                    }
                    if threads.is_some() {
                        return Err(anyhow::anyhow!(
                            "--threads applies to the placement engine; the original packing engine takes \
                             its worker count from packing.cpu_max in {}",
                            path.display()
                        ));
                    }
                    if let Some(input) = input {
                        conf.input.path = input.to_string_lossy().to_string();
                    }
                    if let Some(output) = output {
                        conf.output.path = output.to_string_lossy().to_string();
                    }
                    PackPipeline { config: *conf }.run()?;
                }
            }
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
        Commands::StateMap { config, output } => {
            let path = pick_config_path(config, "state_map_config.yaml");
            let mut conf: StateMapConfig = load_yaml(&path)?;
            if let Some(output) = output {
                conf.state_map.output_dir = output.to_string_lossy().to_string();
            }
            StateMapPipeline { config: conf }.run()?;
        }
        Commands::Closest { config, output } => {
            let path = pick_config_path(config, "closest_config.yaml");
            let mut conf: ClosestConfig = load_yaml(&path)?;
            if let Some(output) = output {
                conf.closest.output_dir = output.to_string_lossy().to_string();
            }
            ClosestPipeline { config: conf }.run()?;
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
