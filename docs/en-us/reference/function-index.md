# Function Index

Master index of every documented function, struct, enum, and constant across `src/`, compiled from the `## Index` table at the top of each [reference](.) document. Each row links to the item's full entry.

| Item | Module | Source | Summary |
|---|---|---|---|
| `GpuVoxelPipeline::resize_grid_buffers` | GPU | `src/gpu/voxel.rs:251` | Manage retained output/readback capacity without rebuilding the pipeline. |
| `GpuVoxelPipeline::release_grid_capacity` | GPU | `src/gpu/voxel.rs:265` | Manage retained output/readback capacity without rebuilding the pipeline. |
| `GpuVolumeTransformPipeline::resize_output_buffers` | GPU | `src/gpu/volume_transform.rs:269` | Manage retained output/readback capacity without rebuilding the pipeline. |
| `GpuVolumeTransformPipeline::release_output_capacity` | GPU | `src/gpu/volume_transform.rs:284` | Manage retained output/readback capacity without rebuilding the pipeline. |
| `read_u32_prefix` | GPU | `src/gpu/runtime.rs:34` | Validate and read only the live staging-buffer prefix. |
| `GpuS2Pipeline::resize_output_buffers` | GPU | `src/gpu/s2.rs:309` | Replace MC output and staging capacity together. |
| `GpuS2Pipeline::release_output_capacity` | GPU | `src/gpu/s2.rs:338` | Release retained MC output/readback peak capacity. |
| `shared_instance` | GPU | `src/gpu/context.rs:84` | Retain one backend instance for non-GL selection. |
| `select_adapter` | GPU | `src/gpu/context.rs:99` | Serialize selection and isolate selected GL adapters. |
| `select_from_instance` | GPU | `src/gpu/context.rs:118` | Apply name/index/default selection within an instance. |
| `request_device` | GPU | `src/gpu/context.rs` | Request one logical device from a selected adapter without memoizing failure. |
| `SharedGpuDevice` | GPU | `src/gpu/context.rs` | Process-wide shared device/queue/adapter info and compiled-pipeline cache for one selector. |
| `SharedGpuDevice::device` / `queue` / `info` / `is_lost` | GPU | `src/gpu/context.rs` | Accessors for the shared device, queue, adapter info and device-lost flag. |
| `SharedGpuDevice::cached_pipeline` | GPU | `src/gpu/context.rs` | Compile a pipeline bundle once per (kind, WGSL source); failures are not cached. |
| `shared_gpu_device` | GPU | `src/gpu/context.rs` | Return (creating lazily) the shared device for `RUSTMSPT_GPU_DEVICE`; lost devices are evicted. |
| `shared_device` | GPU | `src/gpu/context.rs` | String-error wrapper of `shared_gpu_device` for pipeline constructors. |
| `shared_device_for` | GPU | `src/gpu/context.rs` | Cache lookup/creation for one selector under the cache lock; never caches failures. |
| `device_cache` | GPU | `src/gpu/context.rs` | Process-wide selector-to-device map. |
| `release_shared_gpu_devices` | GPU | `src/gpu/context.rs` | Drop cached devices; called at the end of the CLI entry point. |
| `gpu_device_creation_count` | GPU | `src/gpu/context.rs` | Count logical devices created by the shared cache. |
| `gpu_pipeline_build_count` | GPU | `src/gpu/context.rs` | Count pipeline bundles compiled by the shared cache. |
| `foreground_blocks` | Pipeline | `src/pipeline/crop.rs:422` | Fixed-block foreground scan with ordered partial results. |
| `ParticleMetrics` | Pipeline | `src/pipeline/split_filter.rs:305` | Cached component volume/aspect/area. |
| `prepare_particle_metrics` | Pipeline | `src/pipeline/split_filter.rs:312` | Prepare requested metrics in stable component order. |
| `SplitFilterPipeline::run_in_pool` | Pipeline | `src/pipeline/split_filter.rs:349` | Execute split-filter inside the configured pool. |
| `GpuScenePipeline::render_views_to` | GPU | `src/gpu/scene_render.rs:387` | Stream owned views in order while reusing scene and target resources. |
| `PreparedScene` | Geometry | `src/geometry/scene_render.rs:108` | Immutable CPU scene accelerator shared across cameras. |
| `PreparedScene::new` | Geometry | `src/geometry/scene_render.rs:115` | Prepare QBVH and materials once. |
| `PreparedScene::render` | Geometry | `src/geometry/scene_render.rs:120` | Render a view with retained preparation and per-task hit scratch. |
| `RenderPipeline::run_in_pool` | Pipeline | `src/pipeline/render.rs:59` | Execute render stages and fallback within the configured pool. |
| `gpu_crop_values_supported` | Pipeline | `src/pipeline/crop.rs:25` | Check exact integer representation for GPU interpolation. |
| `CropPipeline::run_in_pool` | Pipeline | `src/pipeline/crop.rs:621` | Execute crop stages within the configured pool and report completed-stage wall times. |
| `PlacementParams` | Config | `src/config/placement.rs:22` | The `placement:` block as written in YAML, before validation. |
| `PlacementParams::validate` | Config | `src/config/placement.rs:584` | Applies every cross-field rule and resolves every path into a `ResolvedPlacement`. |
| `ResolvedPlacement` | Config | `src/config/placement.rs:386` | A checked placement block with paths resolved and nothing left optional. |
| `PackDocument` | Config | `src/config/placement.rs:469` | Which engine a `pack` config selected: `Placement` or `Legacy`. |
| `load_pack_document` | Config | `src/config/placement.rs:498` | Two-pass probe deciding which packing engine a config selects. |
| `resolve_against` | Config | `src/config/placement.rs:531` | Lexically joins a config-written path onto the config's directory. |
| `config_dir` | Config | `src/config/placement.rs:547` | The directory a config's relative paths resolve against; `.` when it has none. |
| `require_positive` | Config | `src/config/placement.rs:555` | Refuses a non-finite or non-positive value by field name. |
| `require_non_negative` | Config | `src/config/placement.rs:565` | Refuses a non-finite or negative value by field name. |
| `missing` (placement.rs) | Config | `src/config/placement.rs:902` | Builds the error for a distribution parameter a kind requires. |
| `default_threads` | Config | `src/config/placement.rs:47` | Default thread setting, `-1` meaning every available core. |
| `default_vf_tolerance` | Config | `src/config/placement.rs:279` | Default relative tolerance on the target volume fraction. |
| `load_yaml` | Config | `src/config/mod.rs:52` | Reads a file and deserializes it as YAML into a typed config struct. |
| `parse_box_dimensions` | Config | `src/config/mod.rs:63` | Converts a 3- or 6-element dimensions slice into a `BoundingBox`. |
| `parse_usize_like` | Config | `src/config/deserialize.rs:4` | Parses a `usize` from a string, stripping underscore separators. |
| `deserialize_usize_flexible` | Config | `src/config/deserialize.rs:17` | Serde `deserialize_with` helper: accepts a YAML number or numeric string as `usize`. |
| `deserialize_option_usize_flexible` | Config | `src/config/deserialize.rs:39` | Serde `deserialize_with` helper: accepts a YAML number/string/null as `Option<usize>`. |
| `parse_i32_like` | Config | `src/config/deserialize.rs:61` | Parses an `i32` from a string, stripping underscore separators. |
| `deserialize_option_i32_flexible` | Config | `src/config/deserialize.rs:73` | Serde `deserialize_with` helper: accepts a YAML number/string/null as `Option<i32>`. |
| `default_backend` | Config | `src/config/acceleration.rs:23` | Serde default for `backend`: `"wgpu"`. |
| `default_true` | Config | `src/config/acceleration.rs:27` | Serde default for `cpu_fallback`: `true`. |
| `default_gpu_min_voxels` | Config | `src/config/acceleration.rs:31` | Serde default for `gpu_min_voxels`: `250_000`. |
| `default_gpu_precision` | Config | `src/config/acceleration.rs:40` | Serde default for `gpu_precision`: `"f32"`. |
| `AccelerationConfig::default` | Config | `src/config/acceleration.rs:38` | Rust-level `Default` impl matching the serde defaults. |
| `RustMsptError` | Core & Compute | `src/error.rs:4` | Crate-wide error enum including image encoding failures. |
| `Result` | Core & Compute | `src/error.rs:27` | Type alias `Result<T> = std::result::Result<T, RustMsptError>` used throughout the crate. |
| `cli_path_as_config_relative` | Core & Compute | `src/main.rs:157` | Re-expresses a command-line path so config-relative resolution keeps its meaning. |
| `BoundingBox::intersects_domain` | Core & Compute | `src/types.rs:128` | Whether two boxes meet at all; touching counts. |
| `BuildIdentity` | Core & Compute | `src/version.rs:18` | What this binary is: version, git commit, worktree dirtiness, features, build platform. |
| `build_identity` | Core & Compute | `src/version.rs:36` | Returns the compiled-in build identity; the single source of truth for tool identity. |
| `BuildIdentity::version_detail` | Core & Compute | `src/version.rs:64` | Identity as one line without the program name, for clap's `--version`. |
| `BuildIdentity::version_line` | Core & Compute | `src/version.rs:92` | Identity as one line including the program name. |
| `identity_json` | Core & Compute | `src/version.rs:103` | Serializes the identity as pretty-printed JSON with `null` for undetermined values. |
| `non_empty` (version.rs) | Core & Compute | `src/version.rs:4` | Maps an empty build-script env string to `None`. |
| `build_identity_version_line` | Core & Compute | `src/main.rs:174` | Leaks the version detail as a `&'static str` for clap. |
| `git_output` | Core & Compute | `build.rs:11` | Runs a git command at build time, returning `None` on any failure. |
| `rerun_if_exists` | Core & Compute | `build.rs:25` | Emits a `rerun-if-changed` line only for paths that exist. |
| `emit_rerun_triggers` | Core & Compute | `build.rs:37` | Emits every rerun trigger that can change the recorded identity. |
| `enabled_features` | Core & Compute | `build.rs:62` | Reads enabled cargo features from `CARGO_FEATURE_*`, sorted. |
| `main` (build.rs) | Core & Compute | `build.rs:80` | Stamps the build identity into compile-time environment variables. |
| `Cli` | Core & Compute | `src/main.rs:27` | Top-level clap CLI struct wrapping a `Commands` subcommand. |
| `Commands` | Core & Compute | `src/main.rs:33` | Enum of the 8 CLI subcommands, including Render. |
| `default_config_path` | Core & Compute | `src/main.rs:179` | Builds the default config path under `data/input/`. |
| `pick_config_path` | Core & Compute | `src/main.rs:184` | Chooses a user-supplied config path or falls back to the default. |
| `main` (main.rs) | Core & Compute | `src/main.rs:194` | CLI entry point: parses args, loads config, applies overrides, runs the selected pipeline. |
| `main` (precision_test.rs) | Core & Compute | `src/bin/precision_test.rs:6` | Standalone diagnostic binary comparing S2 computation precision/performance across CPU exact, CPU Monte Carlo, and GPU Monte Carlo methods. |
| `BoundingBox::expanded` | Core & Compute | `src/types.rs:105` | Grows or shrinks a box by the same margin on every side. |
| `Vec3` | Core & Compute | `src/types.rs:2` | 3D vector of `f64` components with basic vector algebra methods. |
| `Vec3::new` | Core & Compute | `src/types.rs:10` | Constructs a vector from x/y/z components. |
| `Vec3::add` | Core & Compute | `src/types.rs:15` | Vector addition. |
| `Vec3::sub` | Core & Compute | `src/types.rs:20` | Vector subtraction. |
| `Vec3::scale` | Core & Compute | `src/types.rs:25` | Scalar multiplication. |
| `Vec3::dot` | Core & Compute | `src/types.rs:30` | Dot product. |
| `Vec3::cross` | Core & Compute | `src/types.rs:35` | Cross product. |
| `BoundingBox` | Core & Compute | `src/types.rs:45` | Axis-aligned box defined by `min`/`max` corners. |
| `BoundingBox::from_size` | Core & Compute | `src/types.rs:52` | Builds a box from the origin to a given size. |
| `BoundingBox::size` | Core & Compute | `src/types.rs:60` | Returns the box's side lengths. |
| `BoundingBox::volume` | Core & Compute | `src/types.rs:65` | Returns the box's (non-negative) volume. |
| `BoundingBox::contains_point` | Core & Compute | `src/types.rs:71` | Tests whether a point lies inside or on the box boundary. |
| `Triangle` | Core & Compute | `src/types.rs:114` | Index triple `(a, b, c)` referencing a mesh's vertex array. |
| `Mesh` | Core & Compute | `src/types.rs:121` | Vertex/face container: `vertices: Vec<Vec3>`, `faces: Vec<Triangle>`. |
| `Mesh::empty` | Core & Compute | `src/types.rs:128` | Constructs an empty mesh. |
| `Mesh::is_empty` | Core & Compute | `src/types.rs:136` | True if the mesh has no vertices or no faces. |
| `AccelerationMode` | Core & Compute | `src/compute/backend.rs:5` | Enum of requested compute modes: `Auto` (default), `Cpu`, `Gpu`. |
| `AccelerationMode::fmt` (Display) | Core & Compute | `src/compute/backend.rs:12` | Formats the mode as `"auto"`/`"cpu"`/`"gpu"`. |
| `BackendCaps` | Core & Compute | `src/compute/backend.rs:23` | Reported capabilities of a selected backend (name, GPU support, buffer size limits). |
| `ComputeBackend` | Core & Compute | `src/compute/backend.rs:31` | Enum of concrete backends actually selected: `Cpu`, or `Gpu { .. }` (feature-gated). |
| `ComputeBackend::name` | Core & Compute | `src/compute/backend.rs:43` | Human-readable backend name. |
| `ComputeBackend::is_gpu` | Core & Compute | `src/compute/backend.rs:52` | Whether the backend is a GPU backend. |
| `ComputeBackend::caps` | Core & Compute | `src/compute/backend.rs:64` | Returns a `BackendCaps` summary for the backend. |
| `ComputeBackend::fmt` (Display) | Core & Compute | `src/compute/backend.rs:87` | Formats as `"cpu"` or `"gpu/wgpu/{adapter_name}"`. |
| `FallbackReason` | Core & Compute | `src/compute/policy.rs:4` | Records why a requested backend could not be honored and what was requested instead. |
| `BackendSelection` | Core & Compute | `src/compute/policy.rs:10` | Result of backend selection: chosen `ComputeBackend` plus optional `FallbackReason`. |
| `select_backend` | Core & Compute | `src/compute/policy.rs:22` | Central CPU/GPU/Auto dispatch policy used by compute-heavy pipelines. |
| `mesh_closedness` | Geometry — Analysis | `src/geometry/metrics.rs:37` | Decides whether a mesh is a closed oriented manifold, and if not, says why. |
| `mesh_is_closed` | Geometry — Analysis | `src/geometry/metrics.rs:23` | Boolean wrapper over `mesh_closedness`. |
| `MeshMetrics` | Geometry — Analysis | `src/geometry/metrics.rs:8` | Struct holding volume, surface area, equivalent diameter, and sphericity. |
| `mesh_is_closed` | Geometry — Analysis | `src/geometry/metrics.rs:23` | Validates that a mesh is a manifold, consistently-oriented, nonzero-volume shell (or set of shells). |
| `mesh_metrics` | Geometry — Analysis | `src/geometry/metrics.rs:149` | Computes volume, surface area, equivalent diameter, and sphericity for a closed mesh. |
| `scale_mesh_to_equivalent_diameter` | Geometry — Analysis | `src/geometry/metrics.rs:182` | Rescales a mesh in place so its equivalent-volume diameter matches a target. |
| `RAY_DIR_GPU` | Geometry — Analysis | `src/geometry/s2.rs:13` | Fixed non-axis-aligned unit ray direction constant, shared with the GPU ray-casting kernels. |
| `index_3d_to_flat` | Geometry — Analysis | `src/geometry/s2.rs:16` | Converts a 3D voxel index to a flat array index (y/z-major strides). |
| `ray_intersects_triangle` | Geometry — Analysis | `src/geometry/s2.rs:26` | Möller–Trumbore ray-triangle intersection test. |
| `point_inside_mesh` | Geometry — Analysis | `src/geometry/s2.rs:63` | Ray-casting point-in-mesh containment test (odd-hit rule). |
| `build_bbox_occupancy` | Geometry — Analysis | `src/geometry/s2.rs:112` | Parallel voxelization of a mesh into a boolean occupancy grid. |
| `occupancy_dims` | Geometry — Analysis | `src/geometry/s2.rs` | Occupancy grid dimensions for a domain and pitch. |
| `part_voxel_ranges` | Geometry — Analysis | `src/geometry/s2.rs` | Prepared query and clamped voxel range per connected component; shared by full and incremental voxelization. |
| `particle_voxel_coverage` | Geometry — Analysis | `src/geometry/s2.rs` | Flat voxel indices whose centres lie inside one particle, once per containing component. |
| `voxel_mc_rng` | Geometry — Analysis | `src/geometry/s2.rs` | Fresh unseeded Xoshiro256++ (`SmallRng`) for one voxel MC radius, seeded from `thread_rng`; never for seeded paths. |
| `cached_mc_shells` | Geometry — Analysis | `src/geometry/s2.rs` | Shell offsets for radii 1..=r_max, reused for an identical `(r_max, pitch)` key. |
| `voxel_mc_radii` | Geometry — Analysis | `src/geometry/s2.rs` | Voxel MC hit ratios per radius; parallel over radii only when asked, same estimator either way. |
| `VOXEL_MC_PARALLEL_MIN_SAMPLES` | Geometry — Analysis | `src/geometry/s2.rs` | 65,536 total MC samples below which radii run serially (measured). |
| `particle_voxel_coverage_in` | Geometry — Analysis | `src/geometry/s2.rs` | Containment queries over prepared component ranges; parallel over x columns or serial, identical output order. |
| `COVERAGE_PARALLEL_MIN_VOXELS` | Geometry — Analysis | `src/geometry/s2.rs` | 1,024 candidate voxels below which one particle's coverage queries run serially (measured). |
| `VoxelCoverage` | Geometry — Analysis | `src/geometry/s2.rs` | Per-voxel coverage counts and matching occupancy for SA; occupied iff count > 0. |
| `VoxelCoverage::new` | Geometry — Analysis | `src/geometry/s2.rs` | Build counts for a population; grid equals VoxelS2::new over the merged mesh. |
| `VoxelCoverage::replace` | Geometry — Analysis | `src/geometry/s2.rs` | Re-query one moved particle, swap its list and return the old one for rollback. |
| `VoxelCoverage::restore` | Geometry — Analysis | `src/geometry/s2.rs` | Restore a rejected move’s previous list without containment queries. |
| `VoxelCoverage::grid` | Geometry — Analysis | `src/geometry/s2.rs` | Borrow the maintained occupancy as an evaluable VoxelS2. |
| `shell_offsets_for_distance` | Geometry — Analysis | `src/geometry/s2.rs:184` | Enumerates integer voxel offsets lying within a spherical shell annulus. |
| `fill_missing_s2_with_smooth_interpolation` | Geometry — Analysis | `src/geometry/s2.rs:217` | Fills unsupported S2 radii via linear or cubic-spline interpolation. |
| `fft_index_3d` | Geometry — Analysis | `src/geometry/s2.rs:328` | Converts a 3D FFT-grid index to a flat index (identical logic to `index_3d_to_flat`). |
| `FftWorkspace::transform` | Geometry — Analysis | `src/geometry/s2.rs:369` | Separable 3D FFT/IFFT performed in place on a complex buffer. |
| `autocorrelation_counts_fft` | Geometry — Analysis | `src/geometry/s2.rs:549` | Computes occupancy autocorrelation counts via FFT convolution. |
| `calculate_s2_exact_direct` | Geometry — Analysis | `src/geometry/s2.rs:561` | Exact S2 by direct pair enumeration per shell offset (no FFT). |
| `calculate_s2_exact_fft` | Geometry — Analysis | `src/geometry/s2.rs:651` | Exact S2 using FFT-based autocorrelation. |
| `calculate_s2_monte_carlo_mesh` | Geometry — Analysis | `src/geometry/s2.rs:730` | Monte Carlo S2 estimation sampling directly on the mesh (no voxelization). |
| `calculate_s2` | Geometry — Analysis | `src/geometry/s2.rs:785` | Top-level S2 dispatcher; routes to exact (FFT or direct) or voxelized Monte Carlo. |
| `calculate_s2_seeded` | Geometry — Analysis | `src/geometry/s2.rs` | `calculate_s2` with an optional seed: mesh MC via `calculate_s2_mesh_mc_seeded`, voxel MC with one fixed stream per radius; `None` is `calculate_s2` exactly. |
| `VoxelS2::calculate_seeded` | Geometry — Analysis | `src/geometry/s2.rs` | `VoxelS2::calculate` with an optional Monte Carlo seed. |
| `approximate_s2` | Geometry — Analysis | `src/geometry/s2.rs:924` | Convenience wrapper for Monte Carlo S2 estimation with a default voxel pitch. |
| `l2_norm` | Geometry — Analysis | `src/geometry/s2.rs:929` | Euclidean distance between two S2 vectors over their common-length prefix. |
| `calculate_s2_with_gpu` | Geometry — Analysis | `src/geometry/s2.rs:948` | GPU-accelerated S2 for Monte Carlo/"both" methods, with CPU fallback. *(feature `gpu`)* |
| `calculate_s2_gpu_exact` | Geometry — Analysis | `src/geometry/s2.rs:984` | GPU-accelerated exact S2 (GPU voxelization + GPU shell pair counting). *(feature `gpu`)* |
| `UnitQuat` | Geometry — Core | `src/geometry/quaternion.rs:17` | Scalar-first unit quaternion `[w, x, y, z]`, canonicalised to `w >= 0`. |
| `UnitQuat::identity` | Geometry — Core | `src/geometry/quaternion.rs:26` | The identity rotation. |
| `UnitQuat::new` | Geometry — Core | `src/geometry/quaternion.rs:42` | Normalizes and sign-canonicalizes raw components. |
| `UnitQuat::to_wxyz` | Geometry — Core | `src/geometry/quaternion.rs:58` | The four components in the record's published order. |
| `UnitQuat::norm` | Geometry — Core | `src/geometry/quaternion.rs:63` | Euclidean norm of the components, for verifying unitness. |
| `UnitQuat::rotate_point` | Geometry — Core | `src/geometry/quaternion.rs:75` | Rotates a point about the origin. |
| `UnitQuat::to_matrix` | Geometry — Core | `src/geometry/quaternion.rs:89` | Row-major 3x3 rotation matrix derived from the quaternion. |
| `sample_uniform_quaternion` | Geometry — Core | `src/geometry/quaternion.rs:123` | Shoemake's Haar-uniform rotation from exactly three uniforms. |
| `transform_shell` | Geometry — Core | `src/geometry/quaternion.rs:148` | The single definition of scale, then rotate, then translate. |
| `icosphere_mesh` | Geometry — Core | `src/geometry/mesh_ops.rs:261` | Closed outward-oriented subdivided icosahedron sphere. |
| `mesh_bbox` | Geometry — Core | `src/geometry/bbox.rs:8` | Axis-aligned bounding box of a mesh. |
| `bbox_overlaps` | Geometry — Core | `src/geometry/bbox.rs:28` | Strict overlap test between two bounding boxes. |
| `bbox_distance` | Geometry — Core | `src/geometry/bbox.rs:38` | Minimum Euclidean distance between two bounding boxes. |
| `check_boundary_constraints_mode` | Geometry — Core | `src/geometry/bbox.rs:72` | Validates a mesh's placement against packing boundary mode rules. |
| `mesh_centroid` | Geometry — Core | `src/geometry/mesh_ops.rs:5` | Arithmetic centroid of mesh vertices. |
| `vec_norm` | Geometry — Core | `src/geometry/mesh_ops.rs:19` | Euclidean length of a vector. |
| `merge_meshes` | Geometry — Core | `src/geometry/mesh_ops.rs:24` | Combines multiple meshes into one, remapping face indices. |
| `split_mesh_into_granules` | Geometry — Core | `src/geometry/mesh_ops.rs:48` | Splits a mesh into connected components (CSR vertex-to-face BFS over shared vertices). |
| `translate_mesh` | Geometry — Core | `src/geometry/mesh_ops.rs:120` | Translates all mesh vertices by a delta vector, in place. |
| `move_mesh_to_target_center` | Geometry — Core | `src/geometry/mesh_ops.rs:125` | Moves a mesh so its centroid matches a target position. |
| `wrap_mesh_centroid_to_box` | Geometry — Core | `src/geometry/mesh_ops.rs:136` | Wraps a mesh's centroid into a box under periodic boundary conditions. |
| `scale_mesh` | Geometry — Core | `src/geometry/mesh_ops.rs:157` | Uniformly scales mesh vertices about the origin, in place. |
| `mesh_surface_area` | Geometry — Core | `src/geometry/mesh_ops.rs:176` | Total surface area of a mesh (sum of triangle areas). |
| `rotate_mesh_around_center` | Geometry — Core | `src/geometry/mesh_ops.rs:195` | Rotates a mesh about its centroid using Rodrigues' rotation formula. |
| `box_mesh` | Geometry — Core | `src/geometry/mesh_ops.rs:216` | Builds a triangulated box mesh from a `BoundingBox`. |
| `SpatialGrid::new` | Geometry — Core | `src/geometry/spatial.rs:22` | Constructs an empty uniform grid over a box with given cell size. |
| `SpatialGrid::insert` | Geometry — Core | `src/geometry/spatial.rs:40` | Inserts an item index into every cell its bbox overlaps. |
| `SpatialGrid::build` | Geometry — Core | `src/geometry/spatial.rs:80` | Constructs and populates a grid from a batch of (index, bbox) pairs. |
| `SpatialGrid::query_neighbors` | Geometry — Core | `src/geometry/spatial.rs:89` | Finds candidate neighbor indices overlapping a query bbox. |
| `SpatialGrid::query_neighbors_with_margin` | Geometry — Core | `src/geometry/spatial.rs:99` | Margin query with first-encounter ordering and adaptive linear/hash deduplication. |
| `SpatialGrid::point_to_cell_clamped` | Geometry — Core | `src/geometry/spatial.rs:157` | Maps a point to grid cell coordinates, clamped to grid bounds. |
| `SpatialGrid::point_to_cell` | Geometry — Core | `src/geometry/spatial.rs:167` | Maps a point to grid cell coordinates, unclamped. |
| `estimate_cell_size` | Geometry — Core | `src/geometry/spatial.rs:176` | Heuristically picks a `SpatialGrid` cell size from a set of bboxes. |
| `VoidVolumeMethod` | Geometry — Volume & Collision | `src/geometry/void_index.rs:18` | Which method produced a void's in-domain volume. |
| `VoidIndex` | Geometry — Volume & Collision | `src/geometry/void_index.rs:32` | A frozen void, indexed for the queries a placement run makes. |
| `VoidIndex::build` | Geometry — Volume & Collision | `src/geometry/void_index.rs:56` | Validates a void mesh and builds its index; refuses mixed orientation. |
| `VoidIndex::bbox` | Geometry — Volume & Collision | `src/geometry/void_index.rs:118` | The void's bounding box. |
| `VoidIndex::shells` | Geometry — Volume & Collision | `src/geometry/void_index.rs:123` | How many closed shells the void has. |
| `VoidIndex::is_outward` | Geometry — Volume & Collision | `src/geometry/void_index.rs:128` | Whether the void's shells wind outward. |
| `VoidIndex::total_volume` | Geometry — Volume & Collision | `src/geometry/void_index.rs:133` | The void's total closed volume over shells with agreeing signs. |
| `VoidIndex::contains_point` | Geometry — Volume & Collision | `src/geometry/void_index.rs:150` | Ray-parity point-in-void test over the hierarchy; correct for nested shells. |
| `VoidIndex::near_box` | Geometry — Volume & Collision | `src/geometry/void_index.rs:166` | Box prefilter: false means far from the void and not nested in it. |
| `VoidIndex::intersects` | Geometry — Volume & Collision | `src/geometry/void_index.rs:186` | Whether a particle's surface intersects the void's. |
| `VoidIndex::min_distance_to` | Geometry — Volume & Collision | `src/geometry/void_index.rs:202` | Minimum surface-to-surface distance from a particle to the void. |
| `VoidIndex::surface_distance` | Geometry — Volume & Collision | `src/geometry/void_index.rs:219` | Unsigned distance from a point to the void surface. |
| `VoidIndex::any_vertex_inside` | Geometry — Volume & Collision | `src/geometry/void_index.rs:233` | Whether any of a mesh's vertices lies inside the void. |
| `VoidIndex::any_void_vertex_inside` | Geometry — Volume & Collision | `src/geometry/void_index.rs:245` | Whether any void vertex lies inside a particle. |
| `VoidIndex::volume_in_domain` | Geometry — Volume & Collision | `src/geometry/void_index.rs:264` | The void's volume inside a domain, and which method produced it. |
| `VoidIndex::sample_surface_point` | Geometry — Volume & Collision | `src/geometry/void_index.rs:288` | Area-weighted point on the void surface with its outward normal. |
| `VoidIndex::overlap_volume` | Geometry — Volume & Collision | `src/geometry/void_index.rs:330` | Volume of a particle inside the void, by domain-anchored voxel count. |
| `point_inside_mesh_local` | Geometry — Volume & Collision | `src/geometry/void_index.rs:375` | Ray-parity point-in-mesh test for a small mesh with no hierarchy. |
| `DOMAIN_FACE_NAMES` | Geometry — Volume & Collision | `src/geometry/volume.rs:454` | The six domain-face names, in clip-plane order. |
| `mesh_volume_centroid` | Geometry — Volume & Collision | `src/geometry/volume.rs:467` | Volume centroid of a closed mesh (not the vertex mean). |
| `shell_signed_volumes` | Geometry — Volume & Collision | `src/geometry/volume.rs:498` | Signed volume per shell, exposing per-shell orientation. |
| `plane_signed_distance` | Geometry — Volume & Collision | `src/geometry/volume.rs:506` | Signed distance from a point to a plane. |
| `clip_tagged_polygon` | Geometry — Volume & Collision | `src/geometry/volume.rs:520` | Sutherland-Hodgman clip that tags the edge the clip created. |
| `mesh_volume_in_bbox_exact` | Geometry — Volume & Collision | `src/geometry/volume.rs:574` | Exact in-box volume and which faces cut, capping plane by plane. |
| `cut_face_names` | Geometry — Volume & Collision | `src/geometry/volume.rs:689` | Names the domain faces a clip actually cut. |
| `mesh_volume` | Geometry — Volume & Collision | `src/geometry/volume.rs:6` | Absolute volume of a closed mesh via the divergence theorem. |
| `mesh_signed_volume` | Geometry — Volume & Collision | `src/geometry/volume.rs:18` | Signed volume of a closed mesh (sign reflects face winding). |
| `orient_components_to_positive_volume` | Geometry — Volume & Collision | `src/geometry/volume.rs:35` | Flips winding of any connected component with negative signed volume. |
| `clip_plane_signed_distance` | Geometry — Volume & Collision | `src/geometry/volume.rs:55` | Signed distance of a point from a plane. |
| `clip_segment_plane_intersection` | Geometry — Volume & Collision | `src/geometry/volume.rs:60` | Interpolated intersection point of a segment with a plane. |
| `clip_polygon_with_plane` | Geometry — Volume & Collision | `src/geometry/volume.rs:75` | Sutherland-Hodgman clip of a convex polygon against a half-plane. |
| `quantize_point_key` | Geometry — Volume & Collision | `src/geometry/volume.rs:125` | Rounds a point to a fixed-precision integer key for dedup/hashing. |
| `collect_triangle_plane_segment` | Geometry — Volume & Collision | `src/geometry/volume.rs:139` | Extracts the segment where a triangle crosses a clip plane. |
| `plane_basis` | Geometry — Volume & Collision | `src/geometry/volume.rs:175` | Builds an orthonormal (u, v) basis in the plane perpendicular to a normal. |
| `triangulate_cap_from_segments` | Geometry — Volume & Collision | `src/geometry/volume.rs:211` | Triangulates a planar cap from cross-plane edge segments (ring-finding + fan). |
| `clip_mesh_by_plane_with_cap` | Geometry — Volume & Collision | `src/geometry/volume.rs:352` | Clips a mesh against one plane and caps the resulting opening. |
| `clip_mesh_by_bbox` | Geometry — Volume & Collision | `src/geometry/volume.rs:390` | Clips a mesh to an axis-aligned box via six successive plane clips. |
| `particle_volume_in_bbox` | Geometry — Volume & Collision | `src/geometry/volume.rs:411` | Volume of a mesh after clipping it to a bounding box. |
| `volume_fraction_in_bbox` | Geometry — Volume & Collision | `src/geometry/volume.rs:417` | Volume fraction of a single mesh within a bounding box. |
| `volume_fraction_of_meshes_in_bbox` | Geometry — Volume & Collision | `src/geometry/volume.rs:427` | Total volume fraction of multiple meshes within a bounding box (parallel). |
| `to_parry_trimesh` | Geometry — Volume & Collision | `src/geometry/collision.rs:29` | Converts a `Mesh` into a parry3d `TriMesh`. |
| `trimesh_contains_point` | Geometry — Volume & Collision | `src/geometry/collision.rs:61` | Ray-parity point-in-solid test over a shape's bounding-volume hierarchy. |
| `mesh_surfaces_intersect_prepared` | Geometry — Volume & Collision | `src/geometry/collision.rs:99` | Bbox-filtered exact test for whether two mesh surfaces cross. |
| `mesh_solids_nested_prepared` | Geometry — Volume & Collision | `src/geometry/collision.rs:150` | Whether one closed solid lies wholly inside the other. |
| `mesh_collision_exact_prepared` | Geometry — Volume & Collision | `src/geometry/collision.rs:196` | Whether two mesh solids overlap: surfaces cross, or one contains the other. |
| `mesh_distance_exact_prepared` | Geometry — Volume & Collision | `src/geometry/collision.rs:214` | Bbox-filtered exact distance query given pre-built bboxes/shapes. |
| `mesh_closer_than_prepared` | Geometry — Volume & Collision | `src/geometry/collision.rs:267` | Screened `distance < gap` test (dual-BVH margin screen, then exact distance). |
| `mesh_collision_exact` | Geometry — Volume & Collision | `src/geometry/collision.rs:402` | Convenience wrapper: builds bbox/shape then tests collision. |
| `mesh_distance_exact` | Geometry — Volume & Collision | `src/geometry/collision.rs:411` | Convenience wrapper: builds bbox/shape then computes distance. |
| `generate_periodic_ghosts` | Geometry — Volume & Collision | `src/geometry/collision.rs:425` | Generates translated ghost copies of a mesh for periodic boundary collision. |
| `simulate_forging_ffd` | Geometry — Volume & Collision | `src/geometry/forging.rs:10` | Simple Z-axis FFD compression with lateral bulge. |
| `simulate_forging_ffd_with_tracking` | Geometry — Volume & Collision | `src/geometry/forging.rs:43` | Axis-configurable FFD forging with void densification and ROI bbox tracking. |
| `GpuContext` | GPU | `src/gpu/context.rs:3` | Holds adapter name and buffer-size capabilities after successful GPU init. |
| `GpuContext::caps` | GPU | `src/gpu/context.rs:11` | Returns `BackendCaps` describing this GPU context. |
| `GpuInitError` | GPU | `src/gpu/context.rs:22` | Error type wrapping a GPU initialization failure message. |
| `GpuInitError` (`Display` impl) | GPU | `src/gpu/context.rs:22` | Formats the error message. |
| `try_init_gpu` | GPU | `src/gpu/context.rs` | Probes the shared device for the current selector and returns a `GpuContext`; used by `compute::policy::select_backend`. |
| `GpuS2Pipeline` | GPU | `src/gpu/s2.rs:10` | GPU pipeline state for Monte Carlo S2 two-point correlation. |
| `build_triangle_buffer` (s2.rs) | GPU | `src/gpu/s2.rs:29` | Builds a normalized `f32` triangle position buffer for the S2 Monte Carlo pipeline. |
| `pack_params` | GPU | `src/gpu/s2.rs` | Packs Monte Carlo S2 shader parameters (including the batch `radius_base`) into the WGSL `Params` layout. |
| `dispatch_plan` | GPU | `src/gpu/s2.rs` | Validate global logical MC ids and the largest radius batch's partial buffers and dispatch. |
| `check_buffer_size` | GPU | `src/gpu/s2.rs:115` | Check single-buffer and storage limits. |
| `check_mesh_capacity` | GPU | `src/gpu/s2.rs:125` | Check triangle count and upload capacity. |
| `scoped` | GPU | `src/gpu/runtime.rs` | Capture scoped GPU errors and balance all scopes under a process-wide reentrant scope lock. |
| `ScopeDepth` (`Drop` impl) | GPU | `src/gpu/runtime.rs` | Decrement the thread-local error-scope nesting depth, including on unwind. |
| `read_u32` | GPU | `src/gpu/runtime.rs:29` | Check mapping completion before copying and unmapping u32 readback. |
| `GpuS2Pipeline::new` | GPU | `src/gpu/s2.rs:141` | Initializes the wgpu device and Monte Carlo S2 compute pipeline. |
| `GpuS2Pipeline::update_mesh` | GPU | `src/gpu/s2.rs` | Makes the triangle buffer equal to a new mesh, uploading only triangles that differ from the resident copy. |
| `GpuS2Pipeline::upload_stats` | GPU | `src/gpu/s2.rs` | Return full/partial/unchanged upload counts and bytes. |
| `GpuUploadStats` | GPU | `src/gpu/s2.rs` | Triangle-buffer upload traffic counters of one MC pipeline. |
| `changed_face_runs` | GPU | `src/gpu/s2.rs` | Diff resident vs new triangle bits into coalesced face runs, or `None` for a full write. |
| `triangle_usage` | GPU | `src/gpu/s2.rs` | Triangle storage usage flags (storage, copy dst/src). |
| `GpuS2Pipeline::ensure_output_capacity` | GPU | `src/gpu/s2.rs:302` | Grows the output buffers if the invocation count exceeds current capacity. |
| `GpuS2Pipeline::calculate_s2_gpu` | GPU | `src/gpu/s2.rs` | Dispatches the Monte Carlo S2 kernel in radius batches of at most 128 and reads back results. |
| `GpuS2Pipeline::calculate_s2_gpu_seeded` | GPU | `src/gpu/s2.rs` | `calculate_s2_gpu` with an optional seed folded to the kernel's 32-bit seed. |
| `OffsetEntry` | GPU | `src/gpu/s2_shell.rs:6` | Packed `(radius_idx, dx, dy, dz)` shell-offset record matching the WGSL layout. |
| `point_inside` (s2_monte_carlo.wgsl) | GPU | `src/gpu/shaders/s2_monte_carlo.wgsl` | Certified parity returning 0/1/uncertain (exact bbox early-out, proven-distinct hits). |
| `point_inside_overflow` (s2_monte_carlo.wgsl) | GPU | `src/gpu/shaders/s2_monte_carlo.wgsl` | Certified >64-hit recovery proving every gap exceeds the CPU dedup band. |
| `point_inside` (voxelize.wgsl) | GPU | `src/gpu/shaders/voxelize.wgsl` | Certified parity returning 0/1/uncertain (exact bbox early-out, proven-distinct hits). |
| `point_inside_overflow` (voxelize.wgsl) | GPU | `src/gpu/shaders/voxelize.wgsl` | Certified >64-hit recovery proving every gap exceeds the CPU dedup band. |
| `cert_ray_triangle` / `cert_triangle` / `cert_ge` / `cert_ratio_err` / `max3` (MC and voxel shaders) | GPU | `src/gpu/shaders/*.wgsl` | Moller-Trumbore with forward error bounds deciding each CPU threshold as true/false/unknown. |
| `record_uncertain` (s2_monte_carlo.wgsl) | GPU | `src/gpu/shaders/s2_monte_carlo.wgsl` | Append (logical id, exact p, exact q) to the uncertain list. |
| `AdapterClass` / `classify_adapter` | GPU | `src/gpu/context.rs` | Software (CPU device type or a known software rasterizer name: llvmpipe, lavapipe, SwiftShader, softpipe, Microsoft Basic Render) vs hardware adapter. |
| `GpuContext::adapter_class` / `GpuContext::describe` | GPU | `src/gpu/context.rs` | The probed adapter's class and a one-line `name= backend= class=` description. |
| `GpuTransferStats` / `gpu_transfer_stats` | GPU | `src/gpu/runtime.rs` | Process-wide upload bytes/calls, readback bytes/calls, blocked readback wait (execution plus transfer, not kernel time) and device init time; `describe()` is the `[Timing] gpu ...` line `main` prints after any GPU run. |
| `CountedWrite::write_counted` | GPU | `src/gpu/runtime.rs` | `queue.write_buffer` that counts the upload; every GPU upload in the crate uses it. |
| `GpuCertificationStats` (+ `recompute_ratio`, `describe`, `accumulate`) | GPU | `src/gpu/certify.rs` | Cumulative certification counters and CPU recompute ratio. |
| `CertReference` (+ `new`, `params_tail`, `classify`) | GPU | `src/gpu/certify.rs` | Origin-shifted f64 CPU reference and exact f32 early-out bounds. |
| `f32_at_least` / `f32_at_most` | GPU | `src/gpu/certify.rs` | Directed f64-to-f32 rounding. |
| `triangle_constants` / `TRI_CONST_FLOATS` | GPU | `src/gpu/certify.rs` | Host-precomputed query-independent certified-test terms, 16 f32 per triangle (`a, m, e1, es, e2, eps_det, h, det`) for the fixed ray direction. |
| `GpuS2Pipeline::certification_stats` | GPU | `src/gpu/s2.rs` | Cumulative MC certification counters. |
| `GpuS2Pipeline::dispatch_batch` | GPU | `src/gpu/s2.rs` | Dispatch one radius batch and read the uncertain counter. |
| `GpuS2Pipeline::resolve_uncertain` | GPU | `src/gpu/s2.rs` | CPU re-evaluation of uncertain samples at exact GPU points. |
| `uncertain_buffers` (s2.rs) | GPU | `src/gpu/s2.rs` | Allocate the MC uncertain list and staging. |
| `GpuVoxelPipeline::certification_stats` | GPU | `src/gpu/voxel.rs` | Cumulative voxel certification counters. |
| `GpuVoxelPipeline::dispatch_voxels` | GPU | `src/gpu/voxel.rs` | Dispatch voxelization (and reducer) and read the uncertain counter. |
| `GpuVoxelPipeline::new_with_shader` | GPU | `src/gpu/voxel.rs` | Construct from supplied WGSL. |
| `voxel_center` / `voxel_uncertain_buffers` / `occupancy_usage` | GPU | `src/gpu/voxel.rs` | Exact f32 cell center, voxel list allocation, patchable occupancy usage. |
| `mc_uncertain_bytes` | Compute | `src/compute/mc_memory.rs` | MC uncertain-list bytes. |
| `voxel_uncertain_entries` / `exact_cert_bytes` | Compute | `src/compute/exact_memory.rs` | Planned voxel uncertain-list capacity and logical bytes. |
| `OptimizeS2::gpu_certification_summary` | Pipeline | `src/pipeline/optimize_execution.rs` | Describe the shared GPU MC certification counters. |
| `GpuShellS2Pipeline` | GPU | `src/gpu/s2_shell.rs:13` | GPU pipeline state for exact shell-pair S2 computation. |
| `build_offset_buffer` | GPU | `src/gpu/s2_shell.rs:30` | Converts `(radius_idx, [dx,dy,dz])` tuples into `OffsetEntry` records. |
| `GpuShellS2Pipeline::new` | GPU | `src/gpu/s2_shell.rs:48` | Initializes the wgpu device and shell S2 compute pipeline. |
| `GpuShellS2Pipeline::compute_s2_shell` | GPU | `src/gpu/s2_shell.rs:181` | Dispatches exact shell-pair counting over an occupancy grid and reads back S2(r). |
| `GpuVoxelPipeline` | GPU | `src/gpu/voxel.rs:5` | GPU pipeline state for mesh voxelization. |
| `build_triangle_buffer` (voxel.rs) | GPU | `src/gpu/voxel.rs:17` | Builds a normalized `f32` triangle position buffer for the voxelization pipeline (separate copy from `s2.rs`). |
| `pack_params` (voxel.rs) | GPU | `src/gpu/voxel.rs` | Serialize the 80-byte voxel parameters: ray at byte 32, certification tail from byte 48. |
| `GpuVoxelPipeline::new` | GPU | `src/gpu/voxel.rs:57` | Initializes the wgpu device and voxelization compute pipeline. |
| `GpuVoxelPipeline::voxelize` | GPU | `src/gpu/voxel.rs:168` | Dispatches ray-casting voxelization and reads back the occupancy grid. |
| `GpuVolumeTransformPipeline` | GPU | `src/gpu/volume_transform.rs:5` | GPU pipeline state for volume rotate-and-crop. |
| `GpuVolumeTransformPipeline::new` | GPU | `src/gpu/volume_transform.rs:23` | Initializes the wgpu device and volume-transform compute pipeline. |
| `GpuVolumeTransformPipeline::rotate_and_crop` | GPU | `src/gpu/volume_transform.rs:123` | Dispatches the rotate/crop/resample kernel and reads back the transformed volume. |
| `parse_ascii_vertex` | I/O | `src/io/stl.rs:9` | Parses one ASCII STL `vertex x y z` line into a `Vec3`. |
| `quantize_key` | I/O | `src/io/stl.rs:21` | Quantizes a vertex to a fixed-precision integer key for tolerant deduplication. |
| `WeldMap` | I/O | `src/io/stl.rs:10` | Vertex-weld map type (quantized key to first index) with a fast non-SipHash hasher; never iterated. |
| `WeldHasher` | I/O | `src/io/stl.rs:15` | Multiply-xor hasher with 64-bit finalizer for quantized vertex keys. |
| `dedup_vertex` | I/O | `src/io/stl.rs:65` | Deduplicates a vertex against an existing list via quantized key lookup. |
| `parse_f32_le` | I/O | `src/io/stl.rs:93` | Parses little-endian `f32` bytes and upcasts to `f64`. |
| `parse_binary_stl` | I/O | `src/io/stl.rs:104` | Parses binary STL bytes into a `Mesh` with deduplicated vertices. |
| `looks_ascii_stl` | I/O | `src/io/stl.rs:151` | Heuristically detects whether bytes represent ASCII STL. |
| `load_stl` | I/O | `src/io/stl.rs:170` | Loads an STL file with automatic ASCII/binary detection. |
| `load_folder_stls` | I/O | `src/io/stl.rs:206` | Loads all STL files in a folder. |
| `load_stl_or_merge_folder` | I/O | `src/io/stl.rs:233` | Loads a single STL file, or merges all STLs in a directory into one mesh. |
| `save_stl` | I/O | `src/io/stl.rs:262` | Saves a mesh as a binary STL file. |
| `collect_sorted_files` | I/O | `src/io/volume.rs:50` | Collects regular files in a folder, sorted by name, optionally filtered by extension. |
| `resolve_slice_range` | I/O | `src/io/volume.rs:78` | Resolves an inclusive slice range from start/end indices, treating `-1` as "from beginning"/"to end". |
| `decode_raw_slice` | I/O | `src/io/volume.rs:107` | Decodes one raw image slice into `i64` values per bit depth, sign, and byte order. |
| `load_raw_folder` | I/O | `src/io/volume.rs:227` | Loads ordered RAW slices with checked sizing, bounded decoding and one final-output reservation. |
| `tiff_decoding_to_i64` | I/O | `src/io/volume.rs:266` | Converts a TIFF `DecodingResult` into a `Vec<i64>` buffer plus its numeric type. |
| `load_tiff_file_with_range` | I/O | `src/io/volume.rs:286` | Loads a multi-page TIFF file into a `Volume3D` over an inclusive page range. |
| `load_tiff_file` | I/O | `src/io/volume.rs:354` | Loads a TIFF file (all pages) into a `Volume3D`. |
| `is_tiff_path` | I/O | `src/io/volume.rs:359` | Checks whether a path has a `.tif`/`.tiff` extension. |
| `load_tiff_or_folder` | I/O | `src/io/volume.rs:369` | Loads a TIFF volume from a file or folder (all pages/slices). |
| `load_tiff_or_folder_with_range` | I/O | `src/io/volume.rs:379` | Loads a TIFF volume from a file or folder over an inclusive slice range. |
| `write_tiff_slice` | I/O | `src/io/volume.rs:446` | Writes one z-slice of volume data into a TIFF encoder page. |
| `save_tiff_or_folder_with_ext` | I/O | `src/io/volume.rs:524` | Saves a `Volume3D` as a multi-page TIFF file or a folder of per-slice TIFF files, with configurable extension. |
| `save_tiff_or_folder` | I/O | `src/io/volume.rs:586` | Saves a `Volume3D` to TIFF file or folder sequence with the default `.tiff` extension. |
| `Pipeline::run` (trait) | Pipeline — Core | `src/pipeline/mod.rs:17` | Trait method every pipeline struct implements to execute end-to-end. |
| `PlacementPipeline` | Pipeline — Core | `src/pipeline/placement.rs:39` | Pipeline struct holding a validated `ResolvedPlacement`. |
| `seeded_rng` | Pipeline — Core | `src/pipeline/rng.rs:13` | Builds the ChaCha12 stream a seeded run draws every variate from. |
| `u01` | Pipeline — Core | `src/pipeline/rng.rs:32` | The single uniform primitive: one draw in `[0, 1)`. |
| `uniform_range` | Pipeline — Core | `src/pipeline/rng.rs:45` | One uniform in `[lo, hi)`; degenerate ranges yield `lo`. |
| `uniform_index` | Pipeline — Core | `src/pipeline/rng.rs:62` | One index uniformly from `0..n`. |
| `create_progress_bar` | Pipeline — Core | `src/pipeline/mod.rs:37` | Builds a tty-aware indicatif progress bar with a given template and fill characters. |
| `run_in_cpu_pool` | Pipeline — Core | `src/pipeline/mod.rs` | Runs work in a dedicated Rayon pool sized from a `cpu_max` setting (absent/-1: all workers); used by forge and scale so every parallel section shares one budget. |
| `RotationMode` (enum) | Pipeline — Core | `src/pipeline/rotation.rs:6` | Represents no rotation, a fixed axis, or a random axis. |
| `parse_rotation_mode` | Pipeline — Core | `src/pipeline/rotation.rs:18` | Parses `none/x/y/z/vector/any` config strings into a `RotationMode`. |
| `sample_rotation_axis` | Pipeline — Core | `src/pipeline/rotation.rs:57` | Draws a concrete rotation axis vector for a given `RotationMode`. |
| `ScalePipeline` (struct) | Pipeline — Core | `src/pipeline/scale.rs:8` | Holds `ScaleConfig` for the scaling pipeline. |
| `ScalePipeline::run` | Pipeline — Core | `src/pipeline/scale.rs:19` | Loads an STL, applies unit-conversion/factor scaling, optionally fixes orientation, saves output. |
| `ForgePipeline` (struct) | Pipeline — Core | `src/pipeline/forge.rs:12` | Holds `ForgingConfig` for the FFD forging pipeline. |
| `ForgePipeline::parse_roi_bbox` | Pipeline — Core | `src/pipeline/forge.rs:18` | Parses an optional 6-element ROI bounding box from config. |
| `ForgePipeline::parse_compression_axis` | Pipeline — Core | `src/pipeline/forge.rs:37` | Parses the compression axis string (`x`/`y`/`z`) into an index and label. |
| `ForgePipeline::run` | Pipeline — Core | `src/pipeline/forge.rs:58` | Runs FFD-based compression/forging, tracks ROI, writes forged STL and a text report. |
| `MeasurePipeline` (struct) | Pipeline — Core | `src/pipeline/measure.rs:12` | Holds `MeasurementConfig` for the S2/volume-fraction measurement pipeline. |
| `MeasurePipeline::parse_optional_bbox` | Pipeline — Core | `src/pipeline/measure.rs:18` | Parses an optional bounding box (3-element size or 6-element min/max) from config. |
| `MeasurePipeline::l2_error` | Pipeline — Core | `src/pipeline/measure.rs:27` | Computes the L2 distance between two S2 value vectors over their common prefix length. |
| `MeasurePipeline::run` | Pipeline — Core | `src/pipeline/measure.rs:53` | Loads an STL, computes volume fraction and S2 correlation (exact/MC/both, CPU or GPU), writes a report. |
| `CropPipeline` (struct) | Pipeline — Crop & Split-Filter | `src/pipeline/crop.rs:14` | Holds `CropConfig` for the crop pipeline. |
| `InterpolationMode` (enum) | Pipeline — Crop & Split-Filter | `src/pipeline/crop.rs:19` | Nearest vs. trilinear resampling mode used during rotate+crop. |
| `parse_byte_order` | Pipeline — Crop & Split-Filter | `src/pipeline/crop.rs:34` | Parses `little`/`big` (or `le`/`be`) into a `ByteOrder`. |
| `parse_interpolation_mode` | Pipeline — Crop & Split-Filter | `src/pipeline/crop.rs:50` | Parses `nearest`/`trilinear` into an `InterpolationMode`, defaulting to trilinear. |
| `load_input_volume` | Pipeline — Crop & Split-Filter | `src/pipeline/crop.rs:71` | Loads the input CT volume from a raw folder or TIFF/TIFF-folder per config. |
| `voxel_index` | Pipeline — Crop & Split-Filter | `src/pipeline/crop.rs:105` | Computes the flat data index for `(x, y, z)` voxel coordinates. |
| `sample_voxel_or_background` | Pipeline — Crop & Split-Filter | `src/pipeline/crop.rs:110` | Reads a voxel at integer coordinates, returning the background value if out of bounds. |
| `sample_nearest` | Pipeline — Crop & Split-Filter | `src/pipeline/crop.rs:137` | Nearest-neighbor sample at fractional source coordinates. |
| `sample_trilinear` | Pipeline — Crop & Split-Filter | `src/pipeline/crop.rs:145` | Trilinear-interpolated sample at fractional source coordinates. |
| `stabilize_bound` | Pipeline — Crop & Split-Filter | `src/pipeline/crop.rs:178` | Snaps a near-integer float to its exact integer within an epsilon. |
| `float_bounds_to_inclusive_i64` | Pipeline — Crop & Split-Filter | `src/pipeline/crop.rs:188` | Converts float min/max bounds to an inclusive integer `[start, end]` range. |
| `boundary_non_bg_ratio` | Pipeline — Crop & Split-Filter | `src/pipeline/crop.rs:201` | Fraction of non-background voxels within a boundary shell of given thickness. |
| `infer_trim_pixels` | Pipeline — Crop & Split-Filter | `src/pipeline/crop.rs:242` | Heuristically infers 0/1/2 pixels of edge trim from boundary artifact intensity. |
| `resolve_trim_pixels` | Pipeline — Crop & Split-Filter | `src/pipeline/crop.rs:260` | Resolves the effective edge-trim pixel count from config, supporting `-1` for auto. |
| `trim_volume_border` | Pipeline — Crop & Split-Filter | `src/pipeline/crop.rs:287` | Trims a fixed number of border voxels from the XY faces of a volume. |
| `detect_background_mode` | Pipeline — Crop & Split-Filter | `src/pipeline/crop.rs:317` | Detects the background value as the modal voxel value on the volume boundary. |
| `estimate_pca_bbox` | Pipeline — Crop & Split-Filter | `src/pipeline/crop.rs:351` | Computes PCA rotation, centroid, and rotated-frame foreground bounding box. |
| `MomentState` | Pipeline — Crop & Split-Filter | `src/pipeline/crop.rs` | Running count, mean and centered second moments. |
| `MomentState::from_row` | Pipeline — Crop & Split-Filter | `src/pipeline/crop.rs` | Exact moments of one foreground row segment. |
| `MomentState::merge` | Pipeline — Crop & Split-Filter | `src/pipeline/crop.rs` | Chan parallel merge of moment states. |
| `pca_frame` | Pipeline — Crop & Split-Filter | `src/pipeline/crop.rs` | Sorted, sign-fixed, right-handed PCA frame with canonical near-degenerate eigenspaces. |
| `projected_bounds` | Pipeline — Crop & Split-Filter | `src/pipeline/crop.rs` | Fixed-block rotated-frame foreground bounds. |
| `foreground_row_blocks` | Pipeline — Crop & Split-Filter | `src/pipeline/crop.rs` | Fixed-block scan over contiguous row segments. |
| `estimate_pca_bbox_three_pass` | Pipeline — Crop & Split-Filter | `src/pipeline/crop.rs` | Test-only previous three-pass PCA oracle. |
| `rotate_and_crop` | Pipeline — Crop & Split-Filter | `src/pipeline/crop.rs:459` | CPU, rayon-parallel rotate-and-crop of the volume into an axis-aligned output. |
| `rotate_and_crop_gpu` | Pipeline — Crop & Split-Filter | `src/pipeline/crop.rs` | Budget-planned, output-tiled GPU rotate-and-crop (feature `gpu`). |
| `CropSourceBlock` | Pipeline — Crop & Split-Filter | `src/pipeline/crop.rs` | Clamped source sub-block one output tile may read. |
| `CropTilePlan` | Pipeline — Crop & Split-Filter | `src/pipeline/crop.rs` | Chosen tile shape, count, retained maxima and peak GPU bytes. |
| `CropTilePlanError` | Pipeline — Crop & Split-Filter | `src/pipeline/crop.rs` | Planning refusal with an optional lower bound on required bytes. |
| `crop_tile_source_block` | Pipeline — Crop & Split-Filter | `src/pipeline/crop.rs` | Tile corner source AABB plus interpolation halo and f32 margin. |
| `crop_gpu_peak_bytes` | Pipeline — Crop & Split-Filter | `src/pipeline/crop.rs` | Logical GPU peak for retained block/tile, upload, staging, params, guard. |
| `for_each_crop_tile` | Pipeline — Crop & Split-Filter | `src/pipeline/crop.rs` | Visits whole-output tiles in z, y, x order. |
| `evaluate_crop_tiling` | Pipeline — Crop & Split-Filter | `src/pipeline/crop.rs` | Checks one tile shape against budget and device buffer limit. |
| `plan_crop_gpu_tiles` | Pipeline — Crop & Split-Filter | `src/pipeline/crop.rs` | Largest z-slab / row / x-run tiling fitting the budget and limits. |
| `TransformTile` | GPU | `src/gpu/volume_transform.rs` | Output tile plus uploaded source sub-block descriptor. |
| `GpuVolumeTransformPipeline::device_limits` | GPU | `src/gpu/volume_transform.rs` | Device limits bounding per-tile buffers. |
| `GpuVolumeTransformPipeline::reserve_capacity` | GPU | `src/gpu/volume_transform.rs` | Pre-size source and output/staging buffers to a plan's maxima. |
| `GpuVolumeTransformPipeline::transform_tile` | GPU | `src/gpu/volume_transform.rs` | Transform one output tile from a halo source block with single-dispatch arithmetic and a halo guard. |
| `GpuVolumeTransformPipeline::resize_source_buffer` | GPU | `src/gpu/volume_transform.rs` | Replace the retained source-block buffer. |
| `CropPipeline::run` | Pipeline — Crop & Split-Filter | `src/pipeline/crop.rs:598` | Orchestrates load → background detect → PCA bbox → rotate+crop (GPU or CPU) → edge trim → save TIFF. |
| `SplitFilterPipeline` (struct) | Pipeline — Crop & Split-Filter | `src/pipeline/split_filter.rs:13` | Holds `SplitFilterConfig` for the split-filter pipeline. |
| `VolumeStats` (struct) | Pipeline — Crop & Split-Filter | `src/pipeline/split_filter.rs:18` | Min/max/mean/median summary of kept-particle volumes. |
| `volume_stats_for_kept` | Pipeline — Crop & Split-Filter | `src/pipeline/split_filter.rs:26` | Computes `VolumeStats` over particles whose `keep` flag is true. |
| `count_kept` | Pipeline — Crop & Split-Filter | `src/pipeline/split_filter.rs:54` | Counts `true` entries in a `keep` boolean slice. |
| `report_step` | Pipeline — Crop & Split-Filter | `src/pipeline/split_filter.rs:59` | Appends a before/after/removed summary line for one filter step. |
| `append_volume_histogram` | Pipeline — Crop & Split-Filter | `src/pipeline/split_filter.rs:71` | Appends a single text histogram of volume values. |
| `append_volume_histogram_comparison` | Pipeline — Crop & Split-Filter | `src/pipeline/split_filter.rs:119` | Appends a side-by-side before/after text histogram of volume values. |
| `normal_cdf` | Pipeline — Crop & Split-Filter | `src/pipeline/split_filter.rs:202` | Standard normal CDF, computed via `erf_approx`. |
| `erf_approx` | Pipeline — Crop & Split-Filter | `src/pipeline/split_filter.rs:208` | Abramowitz & Stegun 7.1.26 approximation of the error function. |
| `apply_lognormal_rebalance` | Pipeline — Crop & Split-Filter | `src/pipeline/split_filter.rs:225` | Drops excess particles from over-represented log-volume bins relative to a fitted lognormal. |
| `SplitFilterPipeline::run` | Pipeline — Crop & Split-Filter | `src/pipeline/split_filter.rs:300` | Orchestrates split → aspect-ratio/sharpness/volume filters → save STLs → report. |
| `OptimizePipeline` | Pipeline — Optimize | `src/pipeline/optimize.rs:27` | Pipeline configuration and bounded execution entry point. |
| `ParticlePrepared` | Pipeline — Optimize | `src/pipeline/optimize.rs:32` | Cached mesh, bbox and collision shape. |
| `IslandResult` | Pipeline — Optimize | `src/pipeline/optimize.rs:39` | Best geometry/loss/S2 snapshot and candidate-stage timings. |
| `GlobalBest` | Pipeline — Optimize | `src/pipeline/optimize.rs:47` | Mutex-protected coherent geometry/loss/S2 migration snapshot. |
| `prepare_particle` | Pipeline — Optimize | `src/pipeline/optimize.rs:54` | Prepare one particle for collision queries. |
| `format_s2_series` | Pipeline — Optimize | `src/pipeline/optimize.rs:61` | Format a curve at six decimal places. |
| `push_history_s2` | Pipeline — Optimize | `src/pipeline/optimize.rs:86` | Append a labeled curve to history. |
| `prune_progress_message` | Pipeline — Optimize | `src/pipeline/optimize.rs:91` | Format pruning loss, VF and particle count. |
| `selective_prune_to_target_vf` | Pipeline — Optimize | `src/pipeline/optimize.rs:103` | Prune with the run-wide S2 definition under the installed pool. |
| `run_sa_island` | Pipeline — Optimize | `src/pipeline/optimize.rs:296` | Run one SA island with the fixed evaluator and coherent migration. |
| `stage_rng` / `fixed_eval_seed` | Pipeline — Optimize | `src/pipeline/optimize.rs` | Per-stage ChaCha12 stream fixed by (`optimization.seed`, stage) or seeded from `thread_rng`; one-off S2 seeds for target/input/final. |
| `OptimizePipeline::run` | Pipeline — Optimize | `src/pipeline/optimize.rs:741` | Install every optimize stage in one configured Rayon pool. |
| `OptimizePipeline::run_in_pool` | Pipeline — Optimize | `src/pipeline/optimize.rs:768` | Resolve execution, load/prepare, prune, batch islands and verify/save the winner. |
| `S2Method` | Pipeline — Optimize | `src/pipeline/optimize_execution.rs:12` | Internal voxel_exact, voxel_mc and mesh_mc definitions. |
| `S2Method::resolve` | Pipeline — Optimize | `src/pipeline/optimize_execution.rs:20` | Preserve existing exact/non-exact and pitch routing. |
| `S2Method::name` | Pipeline — Optimize | `src/pipeline/optimize_execution.rs:31` | Return the actual method name for diagnostics. |
| `resolve_mode` | Pipeline — Optimize | `src/pipeline/optimize_execution.rs:41` | Resolve the optional environment override above YAML; reject invalid values. |
| `select_s2_backend` | Pipeline — Optimize | `src/pipeline/optimize_execution.rs:57` | Apply method/CPU/auto/capacity gates before any GPU probe; enforce fallback policy. |
| `OptimizeS2` | Pipeline — Optimize | `src/pipeline/optimize_execution.rs:130` | Fixed per-run method, pitch and optional shared GPU evaluator. |
| `OptimizeS2::new` | Pipeline — Optimize | `src/pipeline/optimize_execution.rs:144` | Resolve execution once and initialize at most one persistent GPU MC instance. |
| `OptimizeS2::evaluate` | Pipeline — Optimize | `src/pipeline/optimize_execution.rs:220` | Evaluate any stage consistently; serialize GPU buffers and release the lock before VF work. |
| `run_island_batches` | Pipeline — Optimize | `src/pipeline/optimize_execution.rs:275` | Run ordered batches bounded by active Rayon worker count. |
| `PHASE_MATRIX` | Pipeline — Packing | `src/pipeline/placement_labels.rs:13` | Phase code 0 in the written label field. |
| `VoxelLabelsHeader` | Pipeline — Packing | `src/pipeline/placement_labels.rs:22` | What the label stacks are: spacing, origin, layout, phase table. |
| `PhaseLabel` | Pipeline — Packing | `src/pipeline/placement_labels.rs:39` | One phase code and its name. |
| `write_voxel_labels` | Pipeline — Packing | `src/pipeline/placement_labels.rs:60` | Writes the three-phase label field and the per-voxel particle id field. |
| `particle_at` | Pipeline — Packing | `src/pipeline/placement_labels.rs:233` | Finds which placed particle, if any, contains a point. |
| `point_in_particle` | Pipeline — Packing | `src/pipeline/placement_labels.rs:245` | Ray-parity containment for one particle mesh. |
| `VoidReport` | Pipeline — Packing | `src/pipeline/placement_outputs.rs:278` | What the run did with the frozen void, and how it measured it. |
| `build_void_report` | Pipeline — Packing | `src/pipeline/placement.rs:1309` | Describes the frozen void for the report, including its volume method. |
| `PlacementPipeline` | Pipeline — Packing | `src/pipeline/placement.rs:39` | Pipeline struct holding a validated `ResolvedPlacement`. |
| `PlacementOutcome` | Pipeline — Packing | `src/pipeline/placement.rs:62` | What a completed run produced, for in-process callers. |
| `run_placement` | Pipeline — Packing | `src/pipeline/placement.rs:77` | Runs the engine in a dedicated configured Rayon pool and writes every output file. |
| `with_placement_pool` | Pipeline — Packing | `src/pipeline/placement.rs:82` | Creates and installs a dedicated Rayon pool for one operation, propagating creation/work errors. |
| `run_placement_in_pool` | Pipeline — Packing | `src/pipeline/placement.rs:91` | Runs all placement stages in the active pool and records its actual worker count. |
| `resolve_threads` | Pipeline — Packing | `src/pipeline/placement.rs:276` | Turns a thread setting into a worker count, at least 1. |
| `EngineState` | Pipeline — Packing | `src/pipeline/placement.rs:288` | Everything the placement loop accumulates. |
| `place_all` | Pipeline — Packing | `src/pipeline/placement.rs:370` | Attempts every planned size in order, accepting what fits. |
| `try_place_one` | Pipeline — Packing | `src/pipeline/placement.rs:608` | Tries one size within its per-particle attempt budget. |
| `Proposal` | Pipeline — Packing | `src/pipeline/placement.rs:415` | One attempt's variates and the stream position after them. |
| `Evaluation` | Pipeline — Packing | `src/pipeline/placement.rs:434` | A proposal's outcome: rejection reason, or the accepted candidate with its check result. |
| `draw_proposal` | Pipeline — Packing | `src/pipeline/placement.rs:447` | Draws one attempt's variates in the fixed schedule and records the stream position. |
| `evaluate_proposal` | Pipeline — Packing | `src/pipeline/placement.rs:531` | Runs every check for one proposal against the unchanged placed set; read-only. |
| `SPECULATIVE_BATCH_PER_WORKER` / `SERIAL_ATTEMPTS_BEFORE_BATCHING` | Pipeline — Packing | `src/pipeline/placement.rs:407` | Speculative batch cap (8 per worker) and serial attempts before batching (4), both measured. |
| `accept` | Pipeline — Packing | `src/pipeline/placement.rs:697` | Commits an accepted candidate into the geometry and the record. |
| `run_top_up` | Pipeline — Packing | `src/pipeline/placement.rs:758` | Draws further batches when clipping alone left the target short. |
| `decide_stop` | Pipeline — Packing | `src/pipeline/placement.rs:825` | Decides which of the four stop reasons a run ended with. |
| `write_outputs` | Pipeline — Packing | `src/pipeline/placement.rs:941` | Writes the geometry, the per-particle record and the size CSV. |
| `entity_id` | Pipeline — Packing | `src/pipeline/placement.rs:1086` | The stable id of a placed particle. |
| `particle_record` | Pipeline — Packing | `src/pipeline/placement.rs:1091` | Turns one placed particle into its record entry. |
| `size_class_rows` | Pipeline — Packing | `src/pipeline/placement.rs:1139` | Builds the per-class target-against-actual rows. |
| `blank_report` | Pipeline — Packing | `src/pipeline/placement.rs:1165` | The report as it stands before placement starts. |
| `describe_input` | Pipeline — Packing | `src/pipeline/placement.rs:1344` | Describes an input file with its digest for the report. |
| `finish_report` | Pipeline — Packing | `src/pipeline/placement.rs:1363` | Fills in everything the finished run knows. |
| `summary` (placement.rs) | Pipeline — Packing | `src/pipeline/placement.rs:1427` | Builds the human-readable stdout summary. |
| `read_record` | Pipeline — Packing | `src/pipeline/placement.rs:1488` | Reads a written per-particle record back. |
| `read_report` | Pipeline — Packing | `src/pipeline/placement.rs:1496` | Reads a written run report back. |
| `RejectReason` | Pipeline — Packing | `src/pipeline/placement_feasibility.rs:17` | Why a proposed placement was not accepted; the report's keys. |
| `RejectReason::as_str` | Pipeline — Packing | `src/pipeline/placement_feasibility.rs:44` | The stable report key for a rejection reason. |
| `PlacedParticle` | Pipeline — Packing | `src/pipeline/placement_feasibility.rs:76` | A particle that cleared every check, with its cached shape. |
| `PlacedParticle::volume_in_domain_solid` | Pipeline — Packing | `src/pipeline/placement_feasibility.rs:106` | The particle volume counting toward the solid phase. |
| `FeasibilityContext` | Pipeline — Packing | `src/pipeline/placement_feasibility.rs:112` | Everything a feasibility check reads. |
| `Candidate` | Pipeline — Packing | `src/pipeline/placement_feasibility.rs:132` | A proposed placement with its cheap quantities precomputed. |
| `Accepted` | Pipeline — Packing | `src/pipeline/placement_feasibility.rs:146` | What a passing check worked out along the way. |
| `check_placement` | Pipeline — Packing | `src/pipeline/placement_feasibility.rs:171` | Runs every feasibility rule in order, returning the one that stopped it. |
| `PAIR_PARALLEL_MIN` | Pipeline — Packing | `src/pipeline/placement_feasibility.rs:15` | Pairs needing an exact distance at which placement evaluates those distances in parallel; usize::MAX (serial) by default after end-to-end measurement. |
| `pair_needs_exact_test` | Pipeline — Packing | `src/pipeline/placement_feasibility.rs` | Centre-sphere and box tests deciding whether a neighbour needs the exact pair tests. |
| `first_pair_rejection` | Pipeline — Packing | `src/pipeline/placement_feasibility.rs` | Serial overlap/enclosure scan, then ordered parallel distances before the first failure; returns exactly the serial reason. |
| `solid_pair_rejection` | Pipeline — Packing | `src/pipeline/placement_feasibility.rs` | Overlap then enclosure test for one candidate-neighbour pair. |
| `retained_depth` | Pipeline — Packing | `src/pipeline/placement_feasibility.rs:382` | How far a straddling particle still reaches inside the domain. |
| `ToolRecord` | Pipeline — Packing | `src/pipeline/placement_outputs.rs:15` | The build identity as it appears in a record or report. |
| `StopReason` | Pipeline — Packing | `src/pipeline/placement_outputs.rs:53` | The fixed four-word vocabulary a run may stop with. |
| `ParticleRecord` | Pipeline — Packing | `src/pipeline/placement_outputs.rs:133` | One placed particle's entry in the record file. |
| `RecordFile` | Pipeline — Packing | `src/pipeline/placement_outputs.rs:152` | The per-particle record file's top-level shape. |
| `conventions` | Pipeline — Packing | `src/pipeline/placement_outputs.rs:173` | States every convention a reader needs to reconstruct a particle. |
| `ReportFile` | Pipeline — Packing | `src/pipeline/placement_outputs.rs:246` | The run report file's top-level shape. |
| `SizeClassRow` | Pipeline — Packing | `src/pipeline/placement_outputs.rs:226` | One class's target, drawn, placed, shortfall and top-up counts. |
| `write_json` | Pipeline — Packing | `src/pipeline/placement_outputs.rs:380` | Writes a JSON value to disk, creating parent directories. |
| `describe_output` | Pipeline — Packing | `src/pipeline/placement_outputs.rs:400` | Describes a written output file for the report's manifest. |
| `write_size_distribution_csv` | Pipeline — Packing | `src/pipeline/placement_outputs.rs:424` | Writes the per-size-class comparison CSV. |
| `inverse_normal_cdf` | Pipeline — Packing | `src/pipeline/placement_sizes.rs:21` | Wichura AS241 standard-normal quantile, error below 1e-15. |
| `poly` (placement_sizes.rs) | Pipeline — Packing | `src/pipeline/placement_sizes.rs:133` | Horner evaluation of highest-degree-first coefficients. |
| `normal_cdf` (placement_sizes.rs) | Pipeline — Packing | `src/pipeline/placement_sizes.rs:142` | Standard normal CDF via the complementary error function. |
| `erfc` | Pipeline — Packing | `src/pipeline/placement_sizes.rs:153` | Complementary error function, used to turn truncation bounds into probabilities. |
| `SizeDraw` | Pipeline — Packing | `src/pipeline/placement_sizes.rs:175` | One drawn diameter with its reporting class and draw order. |
| `SizeClass` | Pipeline — Packing | `src/pipeline/placement_sizes.rs:185` | A diameter band and the share of particles it should hold. |
| `SizeSource` | Pipeline — Packing | `src/pipeline/placement_sizes.rs:192` | A prepared target number distribution: truncated lognormal or histogram. |
| `SizeSource::prepare` | Pipeline — Packing | `src/pipeline/placement_sizes.rs:218` | Prepares a source, loading the histogram CSV and precomputing truncation. |
| `SizeSource::sample` | Pipeline — Packing | `src/pipeline/placement_sizes.rs:272` | Draws one diameter, consuming exactly one u64. |
| `SizeSource::support` | Pipeline — Packing | `src/pipeline/placement_sizes.rs:309` | The smallest and largest diameter the source can produce. |
| `SizeSource::mass_between` | Pipeline — Packing | `src/pipeline/placement_sizes.rs:381` | The share of the target distribution between two diameters. |
| `build_classes` | Pipeline — Packing | `src/pipeline/placement_sizes.rs:329` | Builds the reporting classes target and actual are compared over. |
| `build_equal_width` | Pipeline — Packing | `src/pipeline/placement_sizes.rs:355` | Splits a source's support into equal-width classes with their shares. |
| `class_for_diameter` | Pipeline — Packing | `src/pipeline/placement_sizes.rs:430` | Finds a diameter's reporting class; the top edge is inclusive. |
| `SizePlan` | Pipeline — Packing | `src/pipeline/placement_sizes.rs:449` | The sizes a run intends to place, drawn before any placement. |
| `plan_size_multiset` | Pipeline — Packing | `src/pipeline/placement_sizes.rs:473` | Draws the whole multiset, stopping at whichever count lands closer to the target. |
| `order_for_placement` | Pipeline — Packing | `src/pipeline/placement_sizes.rs:534` | Orders a drawn multiset largest-first, or back into draw order. |
| `ShapeShell` | Pipeline — Packing | `src/pipeline/placement_library.rs:13` | One closed shell: measured, centred, and digested. |
| `ShapeSource` | Pipeline — Packing | `src/pipeline/placement_library.rs:41` | A source file the library was built from, with its digest and shell counts. |
| `RejectedShell` | Pipeline — Packing | `src/pipeline/placement_library.rs:53` | A shell read but not kept, and why. |
| `ShapeLibrary` | Pipeline — Packing | `src/pipeline/placement_library.rs:61` | Every shape a run may draw from, plus what was read and not kept. |
| `load_shape_library` | Pipeline — Packing | `src/pipeline/placement_library.rs:95` | Loads, splits, measures and filters the shape files. Shells of a file with at least 32 (`LIBRARY_PARALLEL_MIN_SHELLS`) are prepared in parallel into an indexed buffer walked in shell order, so order, rejections and the first reported defect are those of a serial scan (§79). |
| `filter_reason` | Pipeline — Packing | `src/pipeline/placement_library.rs:234` | Says which library filter a shell failed, if any. |
| `shell_geometry_sha256` | Pipeline — Packing | `src/pipeline/placement_library.rs:274` | Digests a shell's geometry so a re-ordered file is detectable. |
| `TARGET_BIN_PROBES` | Pipeline — Packing | `src/pipeline/pack.rs:27` | Max consecutive placement failures tolerated for a chosen bin before it is excluded from this round's re-selection. |
| `PackPipeline` | Pipeline — Packing | `src/pipeline/pack.rs:95` | Pipeline struct wrapping a `PackingConfig`; implements `Pipeline`. |
| `CandidateProposal` | Pipeline — Packing | `src/pipeline/pack.rs:34` | One drawn candidate mesh plus its optional precomputed `MeshMetrics`. |
| `validate_sphericity_target` | Pipeline — Packing | `src/pipeline/pack.rs:110` | Validates `target_mean_sphericity`/`mean_sphericity_tolerance` config before packing starts. |
| `check_geometry_filters` | Pipeline — Packing | `src/pipeline/pack.rs:141` | Applies configured `min_volume`, `max_aspect_ratio`, `max_sharpness_ratio` filters to a candidate mesh. |
| `PackPipeline::run` | Pipeline — Packing | `src/pipeline/pack.rs:124` | Core sequential random packing loop with optional target-diameter-distribution and mean-sphericity steering. |
| `DiameterBin` | Pipeline — Packing | `src/pipeline/pack_targets.rs:8` | Half-open (closed at the final bin) diameter interval with a target frequency. |
| `DiameterBin::midpoint` | Pipeline — Packing | `src/pipeline/pack_targets.rs:16` | Arithmetic midpoint of the interval. |
| `TargetDistribution` | Pipeline — Packing | `src/pipeline/pack_targets.rs:22` | Parsed, normalized target diameter distribution (ordered, non-overlapping bins). |
| `DistributionState` | Pipeline — Packing | `src/pipeline/pack_targets.rs:27` | Mutable per-run counters tracked against a `TargetDistribution`. |
| `BinChoiceKind` | Pipeline — Packing | `src/pipeline/pack_targets.rs:40` | `Natural` / `Scaled` / `Fallback` classification of a bin choice. |
| `BinChoice` | Pipeline — Packing | `src/pipeline/pack_targets.rs:47` | Chosen bin index plus its `BinChoiceKind`. |
| `SphericityState` | Pipeline — Packing | `src/pipeline/pack_targets.rs:61` | Running sum/count of accepted sphericity values. |
| `TargetDistribution::state` | Pipeline — Packing | `src/pipeline/pack_targets.rs:68` | Builds a zeroed `DistributionState` sized to the bins. |
| `TargetDistribution::bin_for_diameter` | Pipeline — Packing | `src/pipeline/pack_targets.rs:81` | Maps a diameter to its containing bin index. |
| `TargetDistribution::choose_bin` | Pipeline — Packing | `src/pipeline/pack_targets.rs:98` | Core allocation heuristic: picks the bin with greatest debt, falling back to minimum-TVD choice. |
| `TargetDistribution::record_attempt` | Pipeline — Packing | `src/pipeline/pack_targets.rs:165` | Increments the attempt counter for a bin. |
| `TargetDistribution::record_success` | Pipeline — Packing | `src/pipeline/pack_targets.rs:176` | Commits a successful placement's bin, kind, and scale factor. |
| `TargetDistribution::summarize` | Pipeline — Packing | `src/pipeline/pack_targets.rs:212` | Computes max absolute error, total variation distance, and best-achievable rounding error. |
| `SphericityState::mean` | Pipeline — Packing | `src/pipeline/pack_targets.rs:259` | Accepted arithmetic mean sphericity. |
| `SphericityState::projected_error` | Pipeline — Packing | `src/pipeline/pack_targets.rs:272` | Distance of the mean-if-accepted from the target band; used to rank/steer candidates. |
| `SphericityState::record_success` | Pipeline — Packing | `src/pipeline/pack_targets.rs:298` | Commits a candidate's sphericity into the running mean. |
| `load_target_distribution_csv` | Pipeline — Packing | `src/pipeline/pack_targets.rs:309` | Parses and validates a `bin[,right],frequency` CSV into a `TargetDistribution`. |
| `write_distribution_comparison_csv` | Pipeline — Packing | `src/pipeline/pack_targets.rs:495` | Writes the `<output_stem>_diameter_distribution.csv` target-vs-actual report. |
| `parse_csv_f64` | Pipeline — Packing | `src/pipeline/pack_targets.rs:564` | Parses a required finite CSV float cell with row/column error context. |
| `parse_optional_csv_f64` | Pipeline — Packing | `src/pipeline/pack_targets.rs:587` | Parses an optional CSV float cell where a blank means "absent". |
| `default_gpu_min_pixels` | Config | `src/config/acceleration.rs` | Serde default for render GPU pixel threshold. |
| `RenderParams` | Config | `src/config/render.rs` | Render paths, camera, resolution, CPU, and acceleration settings. |
| `RenderConfig` | Config | `src/config/render.rs` | Top-level render YAML wrapper. |
| `default_output_path` (render) | Config | `src/config/render.rs` | Defaults output to `data/output/rendered.png`. |
| `default_projection` | Config | `src/config/render.rs` | Defaults projection to orthographic. |
| `default_fov_degrees` | Config | `src/config/render.rs` | Defaults perspective FOV to 45 degrees. |
| `default_fit_padding` | Config | `src/config/render.rs` | Defaults framing padding to 0.05. |
| `default_resolution` | Config | `src/config/render.rs` | Defaults one image dimension to 1024. |
| `RenderedImage` | Core & Compute | `src/types.rs` | Top-row-first RGBA8 image container. |
| `RenderedImage::new` | Core & Compute | `src/types.rs` | Constructs an image from RGBA bytes. |
| `RenderedImage::filled` | Core & Compute | `src/types.rs` | Allocates a solid-color image. |
| `select_backend_for_workload` | Core & Compute | `src/compute/policy.rs` | Unit-aware CPU/GPU/Auto selector. |
| `select_gpu_backend` | Core & Compute | `src/compute/policy.rs` | Shared GPU probe/memory guard. |
| `RenderProjection` | Geometry — Core | `src/geometry/render.rs` | Orthographic/perspective projection enum. |
| `RenderCameraSpec` | Geometry — Core | `src/geometry/render.rs` | Unvalidated camera/framing inputs. |
| `RenderCamera` | Geometry — Core | `src/geometry/render.rs` | Validated camera basis/projection data. |
| `RenderSettings` | Geometry — Core | `src/geometry/render.rs` | Shared appearance settings. |
| `RenderSettings::default` | Geometry — Core | `src/geometry/render.rs` | White background, steel-blue surface, 0.25 ambient. |
| `parse_render_vec3` | Geometry — Core | `src/geometry/render.rs` | Validates a finite three-element vector. |
| `parse_render_projection` | Geometry — Core | `src/geometry/render.rs` | Parses orthographic/perspective aliases. |
| `normalize_or_err` | Geometry — Core | `src/geometry/render.rs` | Normalizes a nonzero config vector. |
| `build_camera_basis` | Geometry — Core | `src/geometry/render.rs` | Builds orthonormal forward/right/up axes. |
| `build_render_camera` | Geometry — Core | `src/geometry/render.rs` | Validates and auto-frames the camera. |
| `bbox_corners` (render) | Geometry — Core | `src/geometry/render.rs` | Returns the eight bbox corners. |
| `RenderCamera::ray_for_pixel` | Geometry — Core | `src/geometry/render.rs` | Generates a pixel-center world ray. |
| `RenderCamera::view_proj_matrix` | Geometry — Core | `src/geometry/render.rs` | Builds the wgpu-compatible camera matrix. |
| `mat4_mul` (render) | Geometry — Core | `src/geometry/render.rs` | Multiplies row-major 4x4 matrices. |
| `shade_intensity` | Geometry — Core | `src/geometry/render.rs` | Computes ambient/diffuse headlight intensity. |
| `shade_channel` | Geometry — Core | `src/geometry/render.rs` | Applies intensity to one u8 channel. |
| `render_mesh_cpu` | Geometry — Core | `src/geometry/render.rs` | Parallel QBVH ray-cast renderer. |
| `sha256_bytes` | I/O | `src/io/hash.rs:13` | SHA-256 of a byte slice as lowercase hex. |
| `sha256_file` | I/O | `src/io/hash.rs:25` | Streams a file through SHA-256, returning the hex digest and byte count. |
| `hex_digest` | I/O | `src/io/hash.rs:42` | Renders a digest as lowercase hexadecimal. |
| `save_image` | I/O | `src/io/image.rs` | Validates and writes RGBA8 PNG. |
| `RenderVertex` | GPU | `src/gpu/render.rs` | Packed position/flat-normal vertex. |
| `RenderUniforms` | GPU | `src/gpu/render.rs` | Camera/appearance uniform layout. |
| `GpuRenderPipeline` | GPU | `src/gpu/render.rs` | Offscreen render state. |
| `build_render_vertices` | GPU | `src/gpu/render.rs` | Expands mesh faces for rasterization. |
| `to_wgsl_mat4` | GPU | `src/gpu/render.rs` | Converts matrix layout/precision for WGSL. |
| `GpuRenderPipeline::new` | GPU | `src/gpu/render.rs` | Creates the offscreen render pipeline. |
| `GpuRenderPipeline::render` | GPU | `src/gpu/render.rs` | Rasterizes and reads back RGBA8. |
| `RenderPipeline` | Pipeline — Core | `src/pipeline/render.rs` | Holds `RenderConfig`. |
| `RenderPipeline::run` | Pipeline — Core | `src/pipeline/render.rs` | Executes STL-to-PNG rendering with fallback. |
| `ArrayData` | Mesh Tooling | `src/io/vtu.rs:14` | Typed VTU DataArray storage (U8/I32/I64/U32/U64/F32/F64) with widening accessors. |
| `DataArray` | Mesh Tooling | `src/io/vtu.rs:118` | Named VTU array with component count; `scalar` convenience constructor. |
| `VtuDoc` | Mesh Tooling | `src/io/vtu.rs:138` | In-memory contract VTU (shared points, mixed cells, point/cell/field data). |
| `VtuDoc::validate` | Mesh Tooling | `src/io/vtu.rs:181` | Structural validation of offsets, indices, per-type node counts, array lengths. |
| `VtuEncoding` | Mesh Tooling | `src/io/vtu.rs:255` | VTU output encoding: Ascii or AppendedRaw (LittleEndian, UInt64 headers). |
| `save_vtu` | Mesh Tooling | `src/io/vtu.rs:273` | Write a VtuDoc as VTK XML UnstructuredGrid (ascii or appended raw). |
| `load_vtu` | Mesh Tooling | `src/io/vtu.rs:479` | Read the contract VTU subset; rejects compressed/base64 files explicitly. |
| `SetKind` | Mesh Tooling | `src/meshgen/render_scene.rs:8` | Render-set membership; Face outranks Volume in coincident-hit dedup. |
| `RenderScene` | Mesh Tooling | `src/meshgen/render_scene.rs:43` | Extraction result: triangles, segments, markers, framing bbox. |
| `SceneFilter` | Mesh Tooling | `src/meshgen/render_scene.rs:52` | AND-composed cell filters (kind/component/region/partition/regime/background/range/bbox/clip). |
| `ColorMode` | Mesh Tooling | `src/meshgen/render_scene.rs:67` | Uniform, categorical (integer arrays), or scalar-viridis (float arrays) coloring. |
| `point_array_cell_value` | Mesh Tooling | `src/meshgen/render_scene.rs:273` | Reduce a point array to one value per cell (mean of non-sentinel point values) for coloring. |
| `SceneSpec` | Mesh Tooling | `src/meshgen/render_scene.rs:77` | Full extraction specification (filters, colors, opacities, overlays, highlights). |
| `categorical_color` | Mesh Tooling | `src/meshgen/render_scene.rs:132` | 12-color categorical palette lookup; sentinel values map to grey. |
| `scalar_color` | Mesh Tooling | `src/meshgen/render_scene.rs:141` | Compact viridis ramp over a clamped scalar range. |
| `build_scene` | Mesh Tooling | `src/meshgen/render_scene.rs:358` | VtuDoc + SceneSpec -> RenderScene (boundary-face extraction, tagged faces, curves, wireframe). |
| `SceneRenderSettings` | Mesh Tooling | `src/geometry/scene_render.rs:12` | Scene render appearance; RGBA background with real alpha support. |
| `render_scene_cpu` | Mesh Tooling | `src/geometry/scene_render.rs:95` | CPU all-hits renderer with front-to-back transparency compositing, line overlays, markers. |
| `named_view` | Mesh Tooling | `src/geometry/scene_render.rs:287` | Resolve a named view preset to (view_direction, up_vector). |
| `ViewSpec` | Mesh Tooling | `src/config/mesh_render.rs:7` | YAML view: named preset or custom camera block (untagged enum). |
| `FilterSpec` | Mesh Tooling | `src/config/mesh_render.rs:22` | YAML kind-tagged filter mapped onto SceneFilter. |
| `MeshRenderParams` | Mesh Tooling | `src/config/mesh_render.rs:38` | mesh_render: YAML block (input, views, image, coloring, filters, overlays). |
| `MeshRenderConfig` | Mesh Tooling | `src/config/mesh_render.rs:92` | Top-level YAML document for the mesh-render subcommand. |
| `MeshRenderPipeline::run` | Mesh Tooling | `src/pipeline/mesh_render.rs:18` | mesh-render subcommand: load VTU, extract scene, render one PNG per view. |
| `orient2d_3d` | Mesh Tooling | `src/meshgen/predicates.rs:28` | Exact-sign triangle orientation in the best-conditioned 2D projection; S0 degenerate check. |
| `ProjectionAxis` / `best_projection_axis` | Mesh Tooling | `src/meshgen/predicates.rs:54/61` | Axis dropped by, and selector for, the best-conditioned planar projection. |
| `project_to_2d` / `orient2d_axis` / value and DD helpers | Mesh Tooling | `src/meshgen/predicates.rs:74/83/92/178` | Project by a selected axis and evaluate exact, f64-permanent, and DD `orient2d` values. |
| `two_sum` / `two_prod` | Mesh Tooling | `src/meshgen/predicates.rs:111/118` | Frozen error-free f64 sum/product primitives used by DD arithmetic. |
| `DoubleDouble` and arithmetic methods | Mesh Tooling | `src/meshgen/predicates.rs:125-172` | Add/sub/mul-only DD value; exact promotion, negation, collapse, and zero test. |
| `DeterminantRatio` and ordering methods | Mesh Tooling | `src/meshgen/predicates.rs:192-224` | Canonical DD numerator/denominator ratio used for source-edge ordering. |
| `PrecisionTier` / `EdgeTriPoint` / `CoplanarSegmentPoint` / `ConstructionOutcome` | Mesh Tooling | `src/meshgen/predicates.rs:229/236/244/254` | Construction precision provenance, C1/C3 results, and resolved/deferred result enum. |
| `orient3d_value_permanent` | Mesh Tooling | `src/meshgen/predicates.rs:266` | Shewchuk-order f64 determinant plus matching permanent. |
| `orient3d_filtered` | Mesh Tooling | `src/meshgen/predicates.rs:286` | Certified static-filter sign with exact robust fallback. |
| `orient3d_dd_value` | Mesh Tooling | `src/meshgen/predicates.rs:296` | DD orient3d determinant in the project sign convention. |
| `construct_edge_triangle_intersection` | Mesh Tooling | `src/meshgen/predicates.rs:320` | Frozen C1 determinant-ratio construction with f64/DD escalation and DD-floor deferral. |
| `construct_coplanar_segment_intersection` | Mesh Tooling | `src/meshgen/predicates.rs:384` | Frozen C3 affine construction; checks and retains DD ordering ratios on both defining edges. |
| `construct_three_triangle_intersection` | Mesh Tooling | `src/meshgen/predicates.rs:492` | Frozen local-frame C2 Cramer construction, fully rebuilt in DD on escalation. |
| `orient3d` | Mesh Tooling | `src/meshgen/predicates.rs:698` | Exact-sign tet orientation det[b-a,c-a,d-a]; sole negation of robust's opposite convention. |
| `tet_signed_volume` / `orient3d_sign_test` | Mesh Tooling | `src/meshgen/predicates.rs:708/713` | Signed tet volume and the unit-tet convention fixture. |
| `TetQuality` / `tet_quality` | Mesh Tooling | `src/meshgen/predicates.rs:729/748` | [V4] volume, aspect/radius ratio, dihedral, scaled-Jacobian and altitude metrics. |
| `node_key` | Mesh Tooling | `src/meshgen/predicates.rs:867` | Quantized integer node key; equal keys mean one mesh-scale node. |
| `RepairActionType` | Mesh Tooling | `src/meshgen/surface.rs:18` | Kind of S0 repair action (weld, degenerate drop, duplicate merge, pinhole, orientation, hole fill). |
| `RepairAction` / `RepairLog` | Mesh Tooling | `src/meshgen/surface.rs:43/55` | Structured repair record and ordered [V12] echo log. |
| `ConditionedSurface` / `ConditionStats` | Mesh Tooling | `src/meshgen/surface.rs:88/116` | S0 geometry with persistent source ids, provisional components, repair log, and counts. |
| `SurfaceComponent` | Mesh Tooling | `src/meshgen/surface.rs:99` | Shared schema-v1 component metadata row (X, Y, kind, closed). |
| `condition_surface` | Mesh Tooling | `src/meshgen/surface.rs:135` | S0 weld/dedupe/repair path; preserves cross-input coincident identities and repair ownership. |
| `condition_surface_to_doc` / `condition_surface_to_doc_with_components` | Mesh Tooling | `src/meshgen/surface.rs:301/307` | Build a complete schema-v1 s00 document with inferred or supplied component metadata. |
| `default_surface_components` / `source_component_is_closed` | Mesh Tooling | `src/meshgen/surface.rs:315/332` | Infer deterministic metadata and test exact combinatorial closure. |
| `surface_stage_to_doc` | Mesh Tooling | `src/meshgen/surface.rs:351` | Shared all-required-array schema-v1 builder for s00-s03 face/curve documents. |
| `FeatureEdgeKind` / `FeatureCurve` / `FeatureSet` | Mesh Tooling | `src/meshgen/features.rs:19/27/40` | Component-local S1 edge classes, chained feature polylines, junctions, and corners. |
| `detect_features` | Mesh Tooling | `src/meshgen/features.rs:54` | Deterministically detect and chain component-local sharp/rim/non-manifold edges. |
| `features_to_doc` / `features_to_doc_with_components` | Mesh Tooling | `src/meshgen/features.rs:290/296` | Build complete schema-v1 s01 documents with inferred or supplied component metadata. |
| `TriId` / `EdgeId::new` / `IsectProv` / `SegKey::new` | Mesh Tooling | `src/meshgen/arrange.rs:25/29-33/44/52-61` | Stable triangle id and canonical EdgeTri/EdgeEdge/TriTriTri/segment provenance keys. |
| `CoincidenceCase` and policy methods / `CoincidenceEntity` / `CoincidenceEvent` | Mesh Tooling | `src/meshgen/arrange.rs:73/88/97/105/112` | Frozen C1-C10 classification with exact reject/warn semantics and sorted participants. |
| `DegradedReason` / `DegradedNeighborhood` / `ArrangedPointFeature` | Mesh Tooling | `src/meshgen/arrange.rs:120/130/140` | Typed durable fallback records and welded C5/C6 point features. |
| `ArrangeComponent` / `ArrangeOptions` and builders | Mesh Tooling | `src/meshgen/arrange.rs:69/149/159/175` | Shared component metadata plus G2-1..G2-3 domain, epsilon, and coincidence-policy options. |
| `RegistryVertex` / `RegistrySegment` / `IntersectionRegistry` | Mesh Tooling | `src/meshgen/arrange.rs:183/195/204` | Symbolic-first registry entities; committed vertices retain every compatible provenance alias. |
| `ArrangedCurveKind` / `ArrangedCurve` | Mesh Tooling | `src/meshgen/arrange.rs:211/219` | Sharp/rim/intersection curves with component incidence and cyclic child-face order. |
| `ArrangedFace` / `ArrangementStats` / `ArrangedSurface` | Mesh Tooling | `src/meshgen/arrange.rs:228/240/256` | Atomic multi-source/tag faces, contacts, events, warnings, degraded routes, and diagnostics. |
| `arrange_surface` | Mesh Tooling | `src/meshgen/arrange.rs:426` | Deterministic CPU G2-1..G2-3 registry/CDT/overlay/policy/fallback/validation path. |
| `arranged_surface_to_doc` | Mesh Tooling | `src/meshgen/arrange.rs:761` | Diagnostic schema-v1 encoding with set-valued tags and FaceTagOrientation; not live s02. |
| `triangulate_parent` | Mesh Tooling | `src/meshgen/arrange.rs:3385` | Restricted pre-registered Spade CDT with propagated insertion/constraint errors and tiling checks. |
| `clip_arranged_to_box` | Mesh Tooling | `src/meshgen/arrange.rs:4634` | G2-5b box clip: Sutherland-Hodgman against 6 domain planes; solids capped (box tag), sheets open, curves clipped. |
| `ComponentClassification` / `ClosureDefect` / `RebuiltTopology` | Mesh Tooling | `src/meshgen/topo.rs:13/21/31` | G2-4 post-clip topology rebuild result types: component classification, defect report, and full rebuild output. |
| `rebuild_topology` | Mesh Tooling | `src/meshgen/topo.rs:43` | Re-derive components, closure status, and GWN fallback from the clipped arranged surface. |
| `generalized_winding_number` / `gwn_margin_band` | Mesh Tooling | `src/meshgen/topo.rs:233/255` | GWN at a query point (pairwise reduction, S accumulation) and the f32 margin band certificate. |
| `Severity` | Mesh Tooling | `src/meshgen/verify.rs:21` | Verifier finding severity (Info < Warn < Fail). |
| `CheckStatus` | Mesh Tooling | `src/meshgen/verify.rs:40` | Per-section outcome PASS/WARN/FAIL/SKIPPED. |
| `VerifyItem` | Mesh Tooling | `src/meshgen/verify.rs:63` | One finding with stable code, message, point/cell ids and coordinates. |
| `VerifySection` | Mesh Tooling | `src/meshgen/verify.rs:88` | One catalog entry: status, metrics, capped item list. |
| `VerifyGates` | Mesh Tooling | `src/meshgen/verify.rs:145` | Configurable, scale-invariant thresholds for the catalog. |
| `VerifyReport` | Mesh Tooling | `src/meshgen/verify.rs:179` | Full verification result; passed, exit_code, fired_codes, section. |
| `verify` | Mesh Tooling | `src/meshgen/verify.rs:430` | Run the check catalog over a contract or external VTU. |
| `report_to_json` | Mesh Tooling | `src/meshgen/verify.rs:1510` | Serialize the frozen JSON report (hand-rolled, no JSON dependency). |
| `report_to_log` | Mesh Tooling | `src/meshgen/verify.rs:1619` | Sectioned human-readable report with a per-check status line. |
| `annotate` | Mesh Tooling | `src/meshgen/verify.rs:1681` | Copy of the mesh carrying quality arrays and the verify_flags bitmask. |
| `VerifyGateParams` | Mesh Tooling | `src/config/mesh_verify.rs:8` | YAML gate overrides for the verification catalog. |
| `MeshVerifyParams` | Mesh Tooling | `src/config/mesh_verify.rs:78` | mesh_verify: YAML block (input, report/json/annotate, gates). |
| `MeshVerifySurface` | Mesh Tooling | `src/config/mesh_verify.rs` | One [V5] input surface: a bare path, or `{stl, priority}` when the mesh was built with an explicit rank. |
| `MeshVerifySurface::resolved_priority` | Mesh Tooling | `src/config/mesh_verify.rs` | Effective priority of a [V5] surface: the explicit value, else 0 — it must mirror the `meshgen.inputs` priority the mesh was built with. |
| `MeshVerifyConfig` | Mesh Tooling | `src/config/mesh_verify.rs:100` | Top-level YAML document for the mesh-verify subcommand. |
| `gates_from_config` | Mesh Tooling | `src/pipeline/mesh_verify.rs:21` | Overlay YAML overrides onto the contract default gates. |
| `verify_file` | Mesh Tooling | `src/pipeline/mesh_verify.rs:53` | Load, validate, verify, and write log/JSON/annotated VTU. |
| `MeshVerifyPipeline::run` | Mesh Tooling | `src/pipeline/mesh_verify.rs:12` | mesh-verify subcommand; nonzero exit when a gate fails. |
| `InputKind` | Mesh Tooling | `src/config/meshgen.rs:10` | Per-input surface role override: auto / solid / sheet. |
| `RepairLevel` | Mesh Tooling | `src/config/meshgen.rs:20` | S0 repair aggressiveness: strict / conservative / permissive. |
| `CoincidencePolicy` | Mesh Tooling | `src/config/meshgen.rs:30` | G2-2 coincidence policy: merge / reject / warn. |
| `FemProfile` | Mesh Tooling | `src/config/meshgen.rs:40` | Target solver profile for sheet/thin handling: implicit / explicit / none. |
| `DeterminismMode` | Mesh Tooling | `src/config/meshgen.rs:50` | Run reproducibility contract: strict (bitwise) / fast. |
| `UnmappedPolicy` | Mesh Tooling | `src/config/meshgen.rs:59` | INP export behaviour for unmapped regions: error / elset-only. |
| `SnapshotMode` | Mesh Tooling | `src/config/meshgen.rs:68` | Contract snapshot emission level: none / key / all. |
| `MeshGenInput` | Mesh Tooling | `src/config/meshgen.rs:84` | One STL input: stl path, optional priority, kind. |
| `MeshGenDomain` | Mesh Tooling | `src/config/meshgen.rs:94` | Axis-aligned generation domain (min/max, 3-component, min < max). |
| `MeshGenSizing` | Mesh Tooling | `src/config/meshgen.rs:107` | Sizing-field limits as fractions of the box diagonal. |
| `MeshGenGaps` | Mesh Tooling | `src/config/meshgen.rs:128` | Gap-field thickness factors and confidence floor. |
| `MeshGenEnvelope` | Mesh Tooling | `src/config/meshgen.rs:176` | Numerical envelope thickness as a fraction of the box diagonal. |
| `MeshGenRepair` | Mesh Tooling | `src/config/meshgen.rs:183` | S0 repair configuration (level). |
| `MeshGenMaterials` | Mesh Tooling | `src/config/meshgen.rs:194` | Material assignments for INP export; by_component preserves duplicate keys. |
| `MeshGenOutput` | Mesh Tooling | `src/config/meshgen.rs:211` | Output destinations: required vtu, optional abaqus/report. |
| `MeshGenParams` | Mesh Tooling | `src/config/meshgen.rs:225` | The meshgen: YAML block; call validate() after loading. |
| `MeshGenConfig` | Mesh Tooling | `src/config/meshgen.rs:257` | Top-level YAML wrapper (meshgen:). |
| `MeshGenInput::resolved_priority` | Mesh Tooling | `src/config/meshgen.rs:265` | Effective priority (explicit override or the file index). |
| `MeshGenParams::validate` | Mesh Tooling | `src/config/meshgen.rs:279` | Enforce the PLAN §6.3 parse-time rejects; Ok or InvalidConfig. |
| `deserialize_component_map` | Mesh Tooling | `src/config/meshgen.rs:509` | Deserialize by_component as a pair list preserving duplicate keys. |
| `MeshGenPipeline::run` | Mesh Tooling | `src/pipeline/meshgen.rs:158` | Normalize before S0, run S0/S1/G2-1..G2-3, withhold partial s02, return NotAvailable for G2-4/G2-5 and later stages. |
| `SampleKind` / `PairClass` | Mesh Generation | `src/meshgen/gapfield.rs:48/57` | S3 sample provenance and the frozen pair classes (intra / inter / solid-sheet / sheet-sheet / surface-box). |
| `GapPairing` / `GapSample` / `GapSample::passes_battery` | Mesh Generation | `src/meshgen/gapfield.rs:68/81/100` | One S3 correspondence, one sample (side, direction, t_raw/t/t_exact, battery flags), and the all-applicable-checks predicate. |
| `GapGroup` / `GapFieldStats` / `GapField` | Mesh Generation | `src/meshgen/gapfield.rs:110/122/137` | Provisional (component, side, opposite patch) group with confidence and t_r, S3 counters, and the whole separation field. |
| `GapFieldOptions` | Mesh Generation | `src/meshgen/gapfield.rs:152` | S3 inputs: domain, epsilon, bootstrap h, gap factors, confidence floor, virtual walls, densification, smoothing. |
| `FLAG_MUTUAL` / `FLAG_OPPOSITE_PATCH` / `FLAG_CONTINUITY` / `FLAG_NO_CROSSING` / `FLAG_ORIENTATION` / `FLAGS_ALL` | Mesh Generation | `src/meshgen/gapfield.rs:28-38` | The five pairing-battery bits and their union. |
| `compute_gap_field` | Mesh Generation | `src/meshgen/gapfield.rs:345` | Run S3 (rays + closest-pair sweep + battery + confidence) over a clipped, topology-rebuilt arranged surface. |
| `gapfield_to_doc` | Mesh Generation | `src/meshgen/gapfield.rs:1655` | Build the s03_gapfield document: arranged surface plus the separation_t point field (-1 = no pairing). |
| `Regime` / `SkipReason` / `MidSurfaceDefect` | Mesh Generation | `src/meshgen/gapfield.rs:166/177/190` | The three thin-feature regimes, the [THIN-SKIP] taxonomy (`Speck` is an area test, `Undersampled` a sample-count one), and the mid-surface validation defects. |
| `MidSurface` / `MidSurface::is_valid` / `ThinRegion` | Mesh Generation | `src/meshgen/gapfield.rs:204/216/227` | The midpoint patch with its source nodes and defects, and one segmented thin region. |
| `validate_mid_surface` | Mesh Generation | `src/meshgen/gapfield.rs:2436` | Run the reference thin-feature design §3.4's checks over a mid-surface candidate and record every defect. |
| `CouplingOptions` / `LockReason` / `CouplingReport` / `CouplingReport::locked_for` | Mesh Generation | `src/meshgen/sizing.rs:35/66/80/94` | S3<->S4 coupling inputs, lock reasons, run report, and per-reason lock query. |
| `regime_for` | Mesh Generation | `src/meshgen/sizing.rs:113` | Classify one region against the current thresholds with the 0.9/1.1 hysteresis dead band. |
| `couple_gap_and_sizing` | Mesh Generation | `src/meshgen/sizing.rs:159` | Run the S3<->S4 fixed point; errors on the post-loop G-8 ordering assertion. |
| `SizingCriterion` / `SizingSource` | Mesh Generation | `src/meshgen/sizing.rs:335/351` | Which §10.6 criterion produced a sizing constraint, and the constraint itself (point + largest permitted element). |
| `SizingOptions` / `SizingOptions::beta` / `SizingOptions::lfs_floor` | Mesh Generation | `src/meshgen/sizing.rs:363/410/429` | Sizing-field inputs; the Lipschitz constant `grading - 1`; the separation below which a gap belongs to the thin machinery, not the field. |
| `curvature_sources` | Mesh Generation | `src/meshgen/sizing.rs:504` | Chord-error curvature sources over the smooth interior edges of the conditioned input surface. |
| `feature_sources` | Mesh Generation | `src/meshgen/sizing.rs:605` | Feature-curve turn sources plus one per S1 corner/junction (shortest incident segment). |
| `collect_geometry_sources` | Mesh Generation | `src/meshgen/sizing.rs:677` | Curvature + feature sources in one canonically ordered list. |
| `gap_sources` | Mesh Generation | `src/meshgen/sizing.rs:712` | Regime-aware local-feature-size sources: `t / gap_cells` for every S3 sample that stays volumetric. |
| `SizingLookup` / `SizingLookup::build` / `eval` / `eval_box` | Mesh Generation | `src/meshgen/sizing.rs:969/987/1067/1083` | The graded field `min_s (h_s + beta*dist)`, Lipschitz by construction; point evaluation and the exact minimum over a box. |
| `SizingLeaf` / `SizingStats` / `SizingField` | Mesh Generation | `src/meshgen/sizing.rs:1156/1164/1182` | One octree leaf, the build's budgets/extents report, and the background octree itself. |
| `SizingField::locate` / `SizingField::sample` | Mesh Generation | `src/meshgen/sizing.rs:1220/1248` | Locate the leaf containing a point (binary search per level), and read its size. |
| `build_sizing_field` | Mesh Generation | `src/meshgen/sizing.rs:1265` | Refine the background octree, one parallel pass per level, until every leaf resolves the field inside it. |
| `SizingConstraint` / `SizingConstraint::new` / `evaluate` / `binding_region` | Mesh Generation | `src/meshgen/sizing.rs:1407/1421/1468/1498` | `C(R)` for the coupling driver, and which region and term set it. |
| `sizing_to_doc` | Mesh Generation | `src/meshgen/sizing.rs:1540` | Encode a sizing field as the `s04_sizing` voxel preview VTU with the `sizing_h` point array. |
| `FREUDENTHAL` / `CellTemplate` | Mesh Generation | `src/meshgen/lattice.rs:54/493` | The frozen 6-tet Kuhn table, and which template a leaf took. |
| `balance_octree` / `balance_violation` | Mesh Generation | `src/meshgen/lattice.rs:156/286` | Refine an octree to strong (face+edge+vertex) 2:1 balance; check the property directly. |
| `Lattice` / `LatticeStats` / `LatticeOptions` | Mesh Generation | `src/meshgen/lattice.rs:520/502/531` | The tetrahedralized background lattice, its build report, and the tet budget. |
| `build_lattice` / `build_lattice_with_splits` | Mesh Generation | `src/meshgen/lattice.rs:611/616` | Tetrahedralize a balanced octree with the Freudenthal and centroid-fan templates. |
| `lattice_to_doc` | Mesh Generation | `src/meshgen/lattice.rs:833` | Encode the lattice as the `s05_lattice` snapshot VTU (tets, not voxels - see the amendment note). |
| `Side` / `Provenance` / `OwnershipRecord` | Mesh Generation | `src/meshgen/classify.rs:60/68/82` | A tet's side of a component (absent = outside, `Ambiguous` = the cut decides), where the entry came from, and the sparse record itself. |
| `resolve` | Mesh Generation | `src/meshgen/classify.rs:171` | The frozen label rule: the inside set reduced to its minimum-priority members; `{0}` when empty. |
| `RAY_DIRECTIONS` | Mesh Generation | `src/meshgen/classify.rs:42` | The frozen re-shoot sequence for a ray that passes exactly through an edge or a vertex (ARB-9). |
| `Classification` / `ClassifyStats` / `ClassifyOptions` | Mesh Generation | `src/meshgen/classify.rs:140/112/457` | Per-vertex ownership, seeded records, region keys, the active-patch mask, and how each decision was reached. |
| `classify_lattice` | Mesh Generation | `src/meshgen/classify.rs:493` | S6: exact parity classification per lattice vertex x solid component, record seeding, and the active-patch filter. |
| `classified_to_doc` | Mesh Generation | `src/meshgen/classify.rs:810` | Encode the classified lattice as the `s06_classified` snapshot VTU. |
| `SNAP_MOTION_CAP` / `SNAP_RECHECK_LOW` / `SNAP_RECHECK_HIGH` | Mesh Generation | `src/meshgen/snap.rs:43/48/50` | The 30 % motion cap (ARB-11) and the 97.5/2.5 % re-check band that promotes a near-endpoint crossing instead of cutting it. |
| `WEIGHT_CORNER` / `WEIGHT_CURVE` / `WEIGHT_SURFACE` | Mesh Generation | `src/meshgen/snap.rs:58/60/62` | `1e7`/`1e4`/`1e0` - the frozen snap-target priority written as numbers. |
| `ALTERNATING_PROJECTION_PASSES` | Mesh Generation | `src/meshgen/snap.rs:54` | The fixed pass count of the degraded-arrangement curve target (ARB-2). |
| `TargetKind` | Mesh Generation | `src/meshgen/snap.rs:68` | What a node is bound to, in the contract's `constraint_kind` encoding: free / surface / polyline / corner / box face. |
| `EdgeCrossing` | Mesh Generation | `src/meshgen/snap.rs:90` | One exact edge-patch crossing: edge, face, component, parameter, and the constructed point - where S8 will cut. |
| `Snapped` / `SnapStats` / `SnapOptions` | Mesh Generation | `src/meshgen/snap.rs:138/105/161` | S7's moved nodes, per-node constraints, crossings and on-cut set; its diagnostics; and its domain + weld tolerance. |
| `snap_lattice` | Mesh Generation | `src/meshgen/snap.rs` | S7: capture corners and feature curves, promote near-endpoint crossings, re-check, and emit the final crossing list. |
| `unique_edges` | Mesh Generation | `src/meshgen/snap.rs` | The lattice's unique edge set as ascending node pairs. |
| `move_preserves_orientation` | Mesh Generation | `src/meshgen/snap.rs` | The exact ARB-10 test: whether moving one node keeps every incident tet positively oriented. |
| `snapped_to_doc` | Mesh Generation | `src/meshgen/snap.rs` | Encode the snapped lattice as the `s07_snapped` snapshot VTU. |
| `CUT_VOLUME_TOLERANCE` / `CUT_MIN_DIHEDRAL_DEG` | Mesh Generation | `src/meshgen/cut.rs:33/36` | The guarded dry-run's 1 % volume tolerance and the §4.4 runtime 8-degree dihedral floor. |
| `NodeSide` / `Escalation` | Mesh Generation | `src/meshgen/cut.rs:43/54` | Where a parent node sits relative to the patch, and why a cell could not take §6's path. |
| `InterfaceFace` | Mesh Generation | `src/meshgen/cut.rs:70` | One tagged cut triangle with its `(inside, outside)` element pair - the reserved export contract. |
| `CutMesh` / `CutStats` / `CutOptions` | Mesh Generation | `src/meshgen/cut.rs` | S8's nodes, tets, records, interface and escalation list; its diagnostics; its tolerances. |
| `snk_split_quad` / `snk_diagonal_is_02` | Mesh Generation | `src/meshgen/cut.rs` | Rule SNK (§4.1): a quad's diagonal is the one incident to its smallest-`NodeKey` vertex. |
| `prism_tets` / `prism_tets_with_diagonals` | Mesh Generation | `src/meshgen/cut.rs` | The frozen six-pattern prism table (§4.3); `None` only for the two cyclic sets Theorem T2 makes unreachable. |
| `FaceCutState` / `face_split` | Mesh Generation | `src/meshgen/cut.rs` | The §5.2 kirigami face-split table including `split_R`; `None` for a dangling-cut state. |
| `CellCut` / `cut_tet` | Mesh Generation | `src/meshgen/cut.rs` | The §6 single-patch tet case table - the pieces, their sides, and the interface triangles. |
| `orient_positively` / `guarded_dry_run` | Mesh Generation | `src/meshgen/cut.rs` | The canonical orientation fix, and §6's guarded dry-run (ARB-15). |
| `cut_lattice` / `cut_to_doc` | Mesh Generation | `src/meshgen/cut.rs` | S8: cut every crossed cell, derive the interface index, escalate the rest; encode `s08_cut`. |
| `FaceMesh` / `face_mesh` / `loop_fan` / `face_centroid` | Mesh Generation | `src/meshgen/junction.rs` | How an escalated cell's face is triangulated - the frozen table where it applies, a face-centroid fan where it does not. |
| `FannedCell` / `fan_cell` / `cell_centroid` / `TET_FACES` | Mesh Generation | `src/meshgen/junction.rs` | The conforming centroid fan that re-meshes an escalated cell (G6-0's adopted fallback). |
| `Stage` | Mesh Tooling | `src/meshgen/snapshot.rs:18` | The frozen stage enumeration (0..=11); also the snapshot index. |
| `Stage::from_path` | Mesh Tooling | `src/meshgen/snapshot.rs:74` | Parse the stage from a snapshot filename's sNN token. |
| `should_emit` | Mesh Tooling | `src/meshgen/snapshot.rs:99` | Whether a stage is emitted under none/key/all. |
| `snapshot_path` | Mesh Tooling | `src/meshgen/snapshot.rs:118` | <stem>.debug/<stem>_sNN_<name>.vtu path (Quality carries _r<N>). |
| `SnapshotMeta` | Mesh Tooling | `src/meshgen/snapshot.rs:132` | Bundled stamping inputs for stamp_metadata/emit_snapshot. |
| `stamp_metadata` | Mesh Tooling | `src/meshgen/snapshot.rs:166` | Stamp the full §2.4 metadata block onto a snapshot document. |
| `emit_snapshot` | Mesh Tooling | `src/meshgen/snapshot.rs:276` | Stamp metadata, then write the pair R4 defines: the delivered tets-only volume under the plain name and the mixed-cell contract document beside it. Returns the delivered path. |
| `warn_if_large` | Mesh Tooling | `src/meshgen/snapshot.rs:246` | Size WARN: snapshots=all + >5 M tets estimate. |
| `VerifyOptions` | Mesh Tooling | `src/meshgen/verify.rs:410` | Out-of-document verifier inputs (expected_stage for the [V12] cross-check). |
| `verify_with_options` | Mesh Tooling | `src/meshgen/verify.rs:744` | Verify with stage context; s00-s03 skip volume-only [V7]/[V8]/[V13]. |
| `BoundaryFace` | Mesh Tooling | `src/meshgen/verify.rs:3531` | One material-boundary face as [V13] measures it: area, local edge length, mean/max distance and signed offset. |
| `FidelityAcc` | Mesh Tooling | `src/meshgen/verify.rs:3548` | [V13]'s area-weighted per-component accumulator. |
| `absorb` | Mesh Tooling | `src/meshgen/verify.rs:3561` | Fold one boundary face into a [V13] accumulator. |
| `check_v13` | Mesh Tooling | `src/meshgen/verify.rs:3603` | [V13] interface fidelity: the material boundary read off the volume, measured against the input surface (the plan's P3). |
| `GpuClipPlane` | Mesh Tooling | `src/gpu/scene_render.rs:45` | Optional half-space clip for the GPU scene preview (smooth cut). |
| `GpuSceneOptions` | Mesh Tooling | `src/gpu/scene_render.rs:52` | GPU-only toggles: clip plane, overlay segments, markers. |
| `GpuScenePipeline` | Mesh Tooling | `src/gpu/scene_render.rs:74` | Offscreen GPU scene preview: coloured TriangleList + LineList overlay with clip-plane discard. |
| `GpuScenePipeline::render` | Mesh Tooling | `src/gpu/scene_render.rs:333` | Render one scene from one camera as an opaque preview. |
| `GpuScenePipeline::render_views` | Mesh Tooling | `src/gpu/scene_render.rs:361` | Batch views: one geometry upload reused across every camera. |

**Total: 422 documented rows** (functions, methods, structs, enums, constants, and grouped closely-related APIs) across the reference tree.

## Notes on counts

- Test-only functions are intentionally excluded; this index covers production code.
- Grouped rows keep tightly coupled APIs such as DD arithmetic and arrangement record types together.
- Same-named helpers in different modules are independent definitions, not duplicates.

## Reference documents

| Document | Covers |
|---|---|
| [core-and-compute.md](core-and-compute.md) | entry points, core types, compute policy |
| [config.md](config.md) | YAML config structs and deserialization helpers |
| [geometry-core.md](geometry-core.md) | core geometry, spatial grid, STL rendering |
| [geometry-volume-collision.md](geometry-volume-collision.md) | volume, collision, forging |
| [geometry-analysis.md](geometry-analysis.md) | metrics and S2 |
| [gpu.md](gpu.md) | feature-gated GPU paths |
| [io.md](io.md) | STL/TIFF/RAW I/O |
| [mesh-render-and-vtu.md](mesh-render-and-vtu.md) | contract VTU and volume-mesh rendering |
| [mesh-verify.md](mesh-verify.md) | verifier catalog and reports |
| [meshgen.md](meshgen.md) | mesh config plus S0/S1/G2-1..G2-3 |
| Pipeline reference pages | pipeline implementations by topic |

## See also

- [Algorithm docs](../algorithms/) — conceptual explanations of S2 correlation, simulated annealing, FFD forging, packing target distribution, PCA volume alignment, spatial-grid collision, and mesh clipping/volume fraction.
- [Example docs](../examples/) — per-pipeline walkthroughs with real captured output.
- [AGENTS.md](../../../AGENTS.md) — the `AI-FUNC-SUMMARY` comment convention these docs are built from, and the requirement to keep both in sync.
| `BandPair` | Mesh Generation | `src/meshgen/thin.rs:71` | One matched wall-vertex pair, carrying its rim node once collapsed (SPEC §8.1 Invariant B1). |
| `BandTemplate` | Mesh Generation | `src/meshgen/thin.rs:102` | Which row of the frozen §8.2 band table a cell took, plus its `regime` cell-array code. |
| `BandFailure` | Mesh Generation | `src/meshgen/thin.rs:138` | Why a band cell could not be meshed by its table row. |
| `BandCellMesh` | Mesh Generation | `src/meshgen/thin.rs:155` | One meshed band cell: tets, template, ladder rung, quality and volume error. |
| `snk_cell_diagonals` | Mesh Generation | `src/meshgen/thin.rs` | Rule SNK's diagonal choice on each of a band cell's three pair-edge quads. |
| `band_cell_boundary` | Mesh Generation | `src/meshgen/thin.rs` | The closed boundary triangulation of a band cell - the construction every template is derived from. |
| `enclosed_volume` | Mesh Generation | `src/meshgen/thin.rs` | Volume enclosed by a closed, consistently oriented triangulation (the band dry-run's target). |
| `band_cell_table` | Mesh Generation | `src/meshgen/thin.rs` | The frozen SPEC §8.2 band table, indexed by the number of collapsed pairs. |
| `mesh_band_cell` | Mesh Generation | `src/meshgen/thin.rs` | Mesh one band cell by the table under given diagonals, with orientation, dihedral and volume checks. |
| `steiner_band_cell` | Mesh Generation | `src/meshgen/thin.rs` | The §4.4 ladder's last rung: the cell's own boundary coned to a Steiner apex. |
| `band_cell_centroid` | Mesh Generation | `src/meshgen/thin.rs` | The Steiner point the band fallback cones to - the cell's centroid. |
| `mesh_band_layer` | Mesh Generation | `src/meshgen/thin.rs` | Mesh a whole band layer: the table, then the pairwise-negotiated flip, then Steiner, then regional demotion. |
| `predict_band_quality` | Mesh Generation | `src/meshgen/thin.rs` | Predict a band's element quality before meshing, from the SPEC §8.2 nominal cell. |
| `band_ladder` | Mesh Generation | `src/meshgen/thin.rs` | The PLAN §10.11 five-rung FEM-aware ladder for one thin region. |
| `ThinOptions` | Mesh Generation | `src/meshgen/thin.rs` | S8b's tunables: the dihedral and AR gates, the FEM profile, the altitude floor and the demotion share. |
| `MeshGenThin` | Mesh Tooling | `src/config/meshgen.rs` | The `meshgen.thin` config block: band gates, altitude floor, demotion share, volumetric-fallback policy. |
| `Slab` | Mesh Generation | `src/meshgen/thin.rs` | Which slab of a band cell a boundary triangle belongs to: near side, gap, far side. |
| `BandDecline` | Mesh Generation | `src/meshgen/thin.rs` | Why a cell is not a sandwich the doubly-cut rule covers; counted and reported per run. |
| `BandCellPlan` | Mesh Generation | `src/meshgen/thin.rs` | A sandwiched cell split: the three closed slabs plus the gap slab's three matched pairs. |
| `band_face_split` | Mesh Generation | `src/meshgen/thin.rs` | The doubly-cut face-split rule: a face carrying two cut nodes per crossed edge, into corner / strip / remainder. |
| `close_open_surface` | Mesh Generation | `src/meshgen/thin.rs` | Close an open oriented triangulation by triangulating the cycle its unmatched directed edges form. |
| `split_band_cell` | Mesh Generation | `src/meshgen/thin.rs` | Split a sandwiched cell into three closed slabs and report the gap slab's matched pairs. |
| `ThinContext` / `ThinRegime` | Mesh Generation | `src/meshgen/thin.rs` | The S3->S8b bridge: thin region per arranged face, effective regime, pair-class code, `t_sheet`, and whether `collapse_sheets` is on. |
| `thin_context` | Mesh Generation | `src/meshgen/gapfield.rs` | Reduce S3's thin regions to per-arranged-face lookups, mapping **both** walls of each region. |
| `pair_class_code` | Mesh Generation | `src/meshgen/gapfield.rs` | The `ThinRegionPairClass` code (SPEC contracts §2.3) of a `PairClass`. |
| `FACE_TAG_INTERFACE` / `FACE_TAG_SHEET` / `FACE_TAG_BOX_CAP` | Mesh Generation | `src/meshgen/cut.rs` | The `FaceTagKind` values S8 writes; the cut had been hard-coding `0`. |
| `face_is_single_patch` | Mesh Generation | `src/meshgen/cut.rs` | Whether the components cutting a face describe one surface (identical per-edge cut maps), so §5.2's table applies. |
| `collapsed_sheet_rim` | Mesh Generation | `src/meshgen/cut.rs` | The rim curve of a collapsed sheet - boundary edges whose endpoints both lie where the collapsed region ends. |
| `CURVE_KIND_RIM` | Mesh Generation | `src/meshgen/cut.rs` | The `CurveKind` value of a rim (SPEC contracts §2.3). |
| `nodes_on_rim` | Mesh Generation | `src/meshgen/cut.rs` | Mesh nodes within tolerance of an arranged rim curve - where an open sheet is allowed to end. |
| `expected_volume` | Mesh Generation | `src/meshgen/verify.rs` | The volume a component is entitled to once higher-priority bodies take their share, by stratified sampling. |
| `TriIndex::contains` | Mesh Generation | `src/meshgen/verify.rs` | Whether a point is inside a triangle soup, by generalized winding number - ray parity is undefined on a self-intersecting surface. |
| `connected_shells` | Mesh Generation | `src/meshgen/verify.rs` | Partition a triangle soup into shells that share vertex positions, so each part of a union can be bounded on its own. |
| `sampled_volume` | Mesh Generation | `src/meshgen/verify.rs` | Union-correct volume estimate: sample each shell's own bounds and charge each sample to the first shell containing it. |
| `ChildTets` | Mesh Generation | `src/meshgen/cut.rs` | The children one cell's §6 table row produces; eight is the largest row (case D). |
| `volume_only` | I/O | `src/io/vtu.rs:785` | The **delivered** tets-only document derived from a mixed-cell one: per-cell arrays sliced, `GlobalPointId` into the shared numbering, `Counts` restated so the derived file does not misdescribe itself. |
| `ArrayData::select_tuples` | I/O | `src/io/vtu.rs` | A new array holding only the tuples whose index passes a keep mask. |
| `contract_path` | Mesh Generation | `src/meshgen/snapshot.rs:333` | The auxiliary mixed-cell document's path beside a delivered volume: `<stem>_contract.vtu`. |
| `polygon_soup_centroid` | Mesh Generation | `src/meshgen/cut.rs` | Centroid of a closed triangle soup - the apex a slab's fan cones to. |
| `KEY_ORDER_REFINEMENT` | Mesh Generation | `src/meshgen/cut.rs` | How much finer than the weld grid S8's ordering key is (SPEC §1.2 Rule K-O). |
| `REGIME_NORMAL` | Mesh Generation | `src/meshgen/cut.rs` | The `regime` cell-array code of an ordinary element. |

| `PointClassifier` | `src/meshgen/classify.rs:487` | The per-solid geometry S6 classifies against, shared with S8 so a piece can be sampled at a point the lattice has no vertex at (SPEC §7.5). |
| `classify_lattice_with` | `src/meshgen/classify.rs:680` | `classify_lattice` against a caller-built `PointClassifier`, so the projection grids are built once. |
| `seed_record` | `src/meshgen/cut.rs:1452` | Settle an escalated piece's ownership by an interior sample instead of inheriting the parent's unresolved record. |
| `curve_sources` | `src/meshgen/sizing.rs:699` | Refine along every locked curve so a cell straddles at most one; the proximity criterion the two chord rules cannot express. |
| `curve_coverage` | `src/meshgen/snap.rs` | How many locked curve segments a chain of mesh edges actually covers; the measurement behind `[SNAP-CURVE]`. |
| `split_soup_by_surface` | `src/meshgen/junction.rs` | Partition an escalated cell's boundary soup by one surface and report **every** cap it leaves (SPEC §7.6); a triangle whose corners are all on the surface is classified by the caller's centroid oracle. |
| `open_boundary_loops` | `src/meshgen/junction.rs` | Chain a soup's once-used edges into one cycle per hole, or refuse a pinched or branching one. Undirected, because the soup has no agreed winding. |
| `split_soup_components` | `src/meshgen/junction.rs` | Split a triangle soup into its edge-connected pieces, ordered by lowest node - two walls leave the outside in two disjoint parts. |
| `curve_pierce_points` | `src/meshgen/cut.rs` | Gate G6-0: per lattice face, the point where a locked curve pierces it, keyed by the face's own corners. |
| `curve_mesh_edges` | `src/meshgen/cut.rs` | Per locked curve, the mesh edges lying along it - both endpoints and the midpoint on the curve; one cell per edge. |
| `nodes_on_curve` | `src/meshgen/cut.rs` | Gate G6-0: the mesh nodes that already lie on a locked curve - the case a strictly-interior piercing test reports nothing for. |
| `fan_from_walk_node` | `src/meshgen/cut.rs` | Triangulate a face's boundary walk as a fan from one of the walk's own nodes; None when any triangle is degenerate. |
| `segment_pierces_triangle` | `src/meshgen/cut.rs` | Where a segment crosses a triangle's interior, strictly - boundary touches and in-plane segments excluded. |
| `curve_segments` | `src/pipeline/meshgen.rs` | Gate G6-0's input: every locked curve of the arrangement as polyline segments. |
| `locked_curves` | `src/pipeline/meshgen.rs` | The same curves kept whole - kind, component set, radial patch count and polyline - for the VTU curve table and `[V9]`. |
| `soup_volume` | `src/meshgen/cut.rs` | The volume a closed polygon soup encloses, exactly for any shape: orient it by a breadth-first walk, then sum signed. |
| `fan_is_sound` | `src/meshgen/cut.rs` | Whether every triangle of a soup makes a non-degenerate tet with the soup's own centroid - the exact question `orient_positively`'s drop asks. |
| `cell_fan_is_conforming` | `src/meshgen/cut.rs` | Whether the tets §7.6's pieces would fan to are conforming among themselves - `[V3]`'s two tests asked of one cell before it commits. |
| `fan_is_simple` | `src/meshgen/cut.rs` | Whether a polygon's fan covers it once: every triangle non-degenerate and all wound the same way. |
| `fan_cap` | `src/meshgen/cut.rs` | Fan a cap polygon from whichever of its own vertices triangulates it cleanly; None when no vertex does. |
| `fan_swallows_vertex` | `src/meshgen/cut.rs` | Whether any fan edge runs through a polygon vertex that is not one of that triangle's own. |
| `split_escalated_cell` | `src/meshgen/cut.rs` | Apply §7.6 per crossing component, guarded by the hole topology and a fan-volume check; falls back to the total fan. |
| `crossed_face` | `src/meshgen/cut.rs` | Describe a face patches cross - one chord per two cut nodes, two nested chords where one component leaves four, and the meeting point where two chords interleave. |
| `ACTIVE_FACE_PROBE` | `src/meshgen/classify.rs` | How far off a face to probe when asking whether it is buried inside its own self-intersecting component. |
| `crossed_face_mesh` | `src/meshgen/junction.rs` | Triangulate such a face with both chords as edges: four sectors when they cross, three polygons when they do not. |
| `chord_meeting_point` | `src/meshgen/junction.rs` | Where two chords of one face meet - the point the S2 intersection curve pierces it. |
| `fan_polygon` | `src/meshgen/junction.rs` | Fan one convex sub-polygon of a face from its lowest-key vertex. |
| `fan_volume` | `src/meshgen/cut.rs` | The volume a closed polygon soup encloses, summed unsigned over an interior cone point. |
| `declare_contact_components` | `src/meshgen/cut.rs` | Declare every mesh face lying on a coincident arranged patch for ALL the components that patch belongs to. |
| `point_on_triangle` | `src/meshgen/cut.rs` | Whether a point lies within `eps` of a triangle (plane distance plus barycentric containment). |
| `contact_patches` | `src/pipeline/meshgen.rs` | S2's multi-tagged arranged faces as triangles paired with their component set - the only route the arrangement's coincidence takes into S8. |
| `DEFAULT_MAX_WIREFRAME_EDGES` | `src/meshgen/render_scene.rs` | Default ceiling on emitted wireframe segments; past it the frame is stride-thinned, never cut short. |

| `PreparedMeshQuery` | Geometry Analysis | `src/geometry/mesh_query.rs:20` | Immutable cached CPU parity query. |
| `PreparedMeshQuery::new` | Geometry Analysis | `src/geometry/mesh_query.rs:29` | Prepare bbox and triangle BVH. |
| `PreparedMeshQuery::bbox` | Geometry Analysis | `src/geometry/mesh_query.rs:68` | Return cached whole-mesh bbox. |
| `PreparedMeshQuery::contains_point` | Geometry Analysis | `src/geometry/mesh_query.rs:73` | Query parity with reusable hit scratch. |
| `MeshQueryScratch` | Geometry Analysis | `src/geometry/mesh_query.rs:7` | Reusable hits and triangle-test counter. |
| `build_nodes` | Geometry Analysis | `src/geometry/mesh_query.rs:144` | Build median BVH with preorder escape links. |
| `ray_reaches_box` | Geometry Analysis | `src/geometry/mesh_query.rs:195` | Conservative positive-ray slab test. |
| `VoxelS2` | Geometry Analysis | `src/geometry/s2.rs:810` | Owned reusable occupancy grid. |
| `VoxelS2::new` | Geometry Analysis | `src/geometry/s2.rs:818` | Prepare CPU occupancy once. |
| `VoxelS2::calculate` | Geometry Analysis | `src/geometry/s2.rs:825` | Compute exact or voxel MC on shared grid. |
| `calculate_s2_mesh_mc_seeded` | Geometry Analysis | `src/geometry/s2.rs:735` | Reproducible sample-block mesh MC. |
| `try_calculate_s2_gpu_exact` | Geometry Analysis | `src/geometry/s2.rs:996` | Fallible GPU exact with checked dimensions. |

| `SpatialGrid::remove` | Geometry Core | `src/geometry/spatial.rs:59` | Remove all item cell references. |
| `SpatialGrid::update` | Geometry Core | `src/geometry/spatial.rs:72` | Replace one item membership. |
| `SpatialQueryScratch` | Geometry Core | `src/geometry/spatial.rs:5` | Retained neighbors and membership storage. |
| `SpatialGrid::query_into` | Geometry Core | `src/geometry/spatial.rs:136` | Fill reusable query scratch. |

| `PackCollider` | Pipeline Packing | `src/pipeline/pack.rs:59` | Cached collider bbox and shape. |
| `PackCollider::new` | Pipeline Packing | `src/pipeline/pack.rs:66` | Prepare collision shape once. |
| `PackCollider::blocks` | Pipeline Packing | `src/pipeline/pack.rs:74` | Cached overlap or clearance predicate; updates PackQueryStats counters. |
| `bbox_may_block` | Pipeline Packing | `src/pipeline/pack.rs:103` | The exact predicate's own bbox rejection, on optional boxes. |
| `periodic_image_shifts` | Pipeline Packing | `src/pipeline/pack.rs:114` | Periodic shifts and shifted bboxes in `generate_periodic_ghosts` order. |
| `PackImage` | Pipeline Packing | `src/pipeline/pack.rs:148` | Accepted particle or `(particle_id, shift)` image with a lazily built collider. |
| `PackScene` | Pipeline Packing | `src/pipeline/pack.rs:155` | Image store, incremental grid, bbox-less list and ghost-build counter. |
| `PackScene::new` | Pipeline Packing | `src/pipeline/pack.rs:164` | Empty store with the domain/8 grid. |
| `PackScene::build_ghost` | Pipeline Packing | `src/pipeline/pack.rs:175` | Translate one image and prepare it (counted). |
| `PackScene::collider` | Pipeline Packing | `src/pipeline/pack.rs:183` | Thread-safe lazy image collider. |
| `PackScene::reachable` | Pipeline Packing | `src/pipeline/pack.rs:192` | Images whose bbox can block a query at the gap; counts queries in PackQueryStats. |
| `PackScene::blocks_any` | Pipeline Packing | `src/pipeline/pack.rs:219` | Serial/parallel any() over reachable images. |
| `PackScene::candidate_images` | Pipeline Packing | `src/pipeline/pack.rs:244` | Full legacy feasibility with lazy candidate and accepted images. |
| `PackScene::insert` | Pipeline Packing | `src/pipeline/pack.rs:276` | Record an accepted particle and its image descriptors. |
| `PackScene::image_stats` | Pipeline Packing | `src/pipeline/pack.rs:300` | Stored images, instantiated ghosts, ghost builds. |
| `PackPipeline::run_in_pool` | Pipeline Packing | `src/pipeline/pack.rs:442` | Packing work under configured pool. |

| `MeasurePipeline::run_in_pool` | Pipeline Core | `src/pipeline/measure.rs:83` | Method-specific measurement in configured pool. |

| `configured_mode` | Core & Compute | `src/compute/policy.rs:142` | Resolve strict environment override. |
| `resolve_execution` | Core & Compute | `src/compute/policy.rs:162` | Resolve method support, workload budget and fallback. |

| `GridPlan` | GPU | `src/gpu/runtime.rs:60` | Checked two-dimensional grid dispatch. |
| `grid_plan` | GPU | `src/gpu/runtime.rs:67` | Validate product, buffer and dispatch limits. |
| `GpuVoxelPipeline::voxelize_limited` | GPU | `src/gpu/voxel.rs:179` | Fallible voxel execution with bounded dispatch. |

| `particle_at_prepared` | Pipeline Placement | `src/pipeline/placement_labels.rs:326` | First particle in ordered cached candidates. |
| `LABEL_SLAB_VOXELS` | Pipeline Placement | `src/pipeline/placement_labels.rs:46` | Target voxels per label slab (4,194,304). |
| `label_dims` | Pipeline Placement | `src/pipeline/placement_labels.rs:111` | Label grid dimensions with overflow check. |
| `LabelQuery` | Pipeline Placement | `src/pipeline/placement_labels.rs:124` | Prepared particle queries, bbox grid and void shared by all slabs. |
| `LabelQuery::new` | Pipeline Placement | `src/pipeline/placement_labels.rs:136` | Prepare the per-run query context once. |
| `LabelQuery::centre` | Pipeline Placement | `src/pipeline/placement_labels.rs:170` | Voxel-centre world position. |
| `LabelQuery::fill_slab` | Pipeline Placement | `src/pipeline/placement_labels.rs:184` | Classify one z-slab into phase/id buffers. |
| `write_label_stacks` | Pipeline Placement | `src/pipeline/placement_labels.rs:257` | Stream both label TIFF stacks slab by slab. |

| `map_vertices` | Geometry Core | `src/geometry/mesh_ops.rs:162` | Serial or parallel independent vertex mapping. |

| `forge_owned` | Geometry Volume/Collision | `src/geometry/forging.rs:66` | Ownership-consuming FFD and ROI transform. |

| `load_stl_from_reader` | I/O | `src/io/stl.rs:192` | Forward-reader STL: streamed ASCII lines and bounded binary records. |
| `AsciiStlBuilder` | I/O | `src/io/stl.rs:47` | Incremental ASCII STL state: vertices, faces, pending vertices, dedup map. |
| `AsciiStlBuilder::push_line` | I/O | `src/io/stl.rs:56` | Consume one raw line with the legacy lossy/trim/vertex rules. |
| `parse_ascii_stream_or_binary` | I/O | `src/io/stl.rs:78` | Line-streamed ASCII STL with binary fallback on the retained bytes. |
| `TiffPageEncoder` | I/O | `src/io/volume.rs:552` | Incremental multi-page TIFF encoder over a borrowed seekable writer. |
| `TiffPageEncoder::new` | I/O | `src/io/volume.rs:562` | Write the TIFF header and fix page size/type. |
| `TiffPageEncoder::write_slices` | I/O | `src/io/volume.rs:590` | Append whole z-slices as consecutive pages. |
| `load_stl_hashed` | I/O | `src/io/stl.rs:193` | Single-pass STL parsing and raw digest. |
| `parse_binary_reader` | I/O | `src/io/stl.rs:109` | Read binary triangle records with incremental deduplication. |
| `read_stl_record` | I/O | `src/io/stl.rs:140` | Read complete record or report truncation. |
| `HashingReader` | I/O | `src/io/hash.rs:50` | Incremental digest over delivered bytes. |
| `HashingReader::new` | I/O | `src/io/hash.rs:58` | Wrap forward reader for hashing. |
| `HashingReader::finish` | I/O | `src/io/hash.rs:67` | Return digest and consumed byte count. |

| `stl_paths` | I/O | `src/io/stl.rs:218` | List STL paths in existing directory order. |

| `merge_prepared_particles` | Pipeline Optimize | `src/pipeline/optimize.rs:61` | Merge geometry and assign stable particle vertex ranges. |

| `FftWorkspace` | Geometry Analysis | `src/geometry/s2.rs:334` | Reusable FFT plans and complex arrays. |
| `FftWorkspace::new` | Geometry Analysis | `src/geometry/s2.rs:348` | Construct dimension-specific FFT workspace. |
| `FftWorkspace::array_bytes` | Geometry Analysis | `src/geometry/s2.rs:363` | Report retained complex-array capacities. |
| `with_fft_correlation` | Geometry Analysis | `src/geometry/s2.rs:489` | Evaluate occupancy FFT with bounded cache retention. |

| `FFT_RETAIN_BYTES` | Geometry Analysis | `src/geometry/s2.rs:332` | Maximum retained FFT array bytes per calling thread. |
| `smooth_fft_length` | Geometry Analysis | `src/geometry/s2.rs` | Smallest 2,3,5-smooth length >= a minimum. |
| `padded_fft_dims` | Geometry Analysis | `src/geometry/s2.rs` | Per-axis smooth padding >= 2N-1. |
| `ExactCpuMethod` | Geometry Analysis | `src/geometry/s2.rs` | CPU exact kernel selector (Fft/Direct). |
| `ExactCpuPlan` | Geometry Analysis | `src/geometry/s2.rs` | Modeled cost, working sets and chosen CPU exact kernel. |
| `ExactCpuPlan::selected_bytes` | Geometry Analysis | `src/geometry/s2.rs` | Working set of the selected kernel. |
| `ExactCpuPlan::fits_budget` | Geometry Analysis | `src/geometry/s2.rs` | Whether the selected kernel fits the budget. |
| `ExactCpuPlan::describe` | Geometry Analysis | `src/geometry/s2.rs` | One-line observable plan description. |
| `exact_shell_work` | Geometry Analysis | `src/geometry/s2.rs` | In-domain offsets, exact direct pair work, largest shell. |
| `fft_working_set_bytes` | Geometry Analysis | `src/geometry/s2.rs` | Checked FFT peak-byte estimate. |
| `direct_working_set_bytes` | Geometry Analysis | `src/geometry/s2.rs` | Checked direct peak-byte estimate. |
| `plan_exact_cpu` | Geometry Analysis | `src/geometry/s2.rs` | Cost-model/budget choice between FFT and direct. |
| `cached_exact_plan` | Geometry Analysis | `src/geometry/s2.rs` | Reuse and log the exact plan once per key. |
| `offset_in_domain` | Geometry Analysis | `src/geometry/s2.rs` | Whether a shift leaves a valid voxel pair. |
| `direct_pair_counts` | Geometry Analysis | `src/geometry/s2.rs` | Integer (hits, valid) for one shift via z runs. |
| `finish_exact_curve` | Geometry Analysis | `src/geometry/s2.rs` | Assemble, interpolate and pin S2(0). |
| `VoxelS2::calculate_exact_with` | Geometry Analysis | `src/geometry/s2.rs` | Exact S2 with a forced CPU kernel. |
| `DEFAULT_CPU_EXACT_BUDGET_BYTES` | Geometry Analysis | `src/geometry/s2.rs` | Default CPU exact working-set budget (768 MiB). |
| `NS_PER_FFT_UNIT` | Geometry Analysis | `src/geometry/s2.rs` | Calibrated FFT cost per P*log2(P) unit (ns). |
| `FFT_PARALLEL_EFFICIENCY` | Geometry Analysis | `src/geometry/s2.rs` | Modeled FFT parallel efficiency. |
| `NS_PER_DIRECT_PAIR` | Geometry Analysis | `src/geometry/s2.rs` | Calibrated direct cost per visited pair (ns). |
| `DIRECT_PARALLEL_EFFICIENCY` | Geometry Analysis | `src/geometry/s2.rs` | Modeled direct parallel efficiency. |

| `GpuShellS2Pipeline::resize_batch_buffers` | `src/gpu/s2_shell.rs:193` | Shell batch buffer capacity management; occupancy retained. |

| `GpuShellS2Pipeline::release_batch_capacity` | `src/gpu/s2_shell.rs:230` | Shell batch buffer capacity management; occupancy retained. |

| `MeshRenderPipeline::with_worker_pool` | `src/pipeline/mesh_render.rs:193` | Execute scene preparation, rendering and fallback within the worker budget. |

| `MeshRenderPipeline::run_in_pool` | `src/pipeline/mesh_render.rs:218` | Execute scene preparation, rendering and fallback within the worker budget. |

| `SceneRenderMemory::plan` | `src/compute/render_memory.rs:20` | Checked scene preview workset, budget and buffer planning without allocation. |

| `SceneRenderMemory::check_budget` | `src/compute/render_memory.rs:77` | Checked scene preview workset, budget and buffer planning without allocation. |

| `SceneRenderMemory::check_buffers` | `src/compute/render_memory.rs:93` | Checked scene preview workset, budget and buffer planning without allocation. |

| `mc_evaluation_peak` | `src/compute/mc_memory.rs:4` | Check logical MC peak including retained capacity and pending uploads. |
| `mc_evaluation_peak_batched` | `src/compute/mc_memory.rs:4` | `src/compute/mc_memory.rs` | `mc_evaluation_peak` with the radius batch (radii per dispatch) as a parameter. |
| `mc_largest_batch` | `src/compute/mc_memory.rs:4` | `src/compute/mc_memory.rs` | Largest radius batch (1..=128) whose MC peak fits the MiB limit; errors only when one radius per dispatch does not fit. |

| `check_mc_budget` | `src/compute/mc_memory.rs:44` | Check logical MC peak including retained capacity and pending uploads. |

| `GpuS2Pipeline::check_evaluation_budget` | `src/gpu/s2.rs:275` | Check logical MC peak including retained capacity and pending uploads. |
| `GpuS2Pipeline::set_memory_limit_mb` | `src/gpu/s2.rs:275` | `src/gpu/s2.rs` | Store the logical budget that uncertain-list regrowth must respect. |
| `mc_regrowth_peak` | `src/gpu/s2.rs:275` | `src/compute/mc_memory.rs` | Retained peak plus a regrown uncertain list and its staging. |
| `GpuVoxelPipeline::set_regrowth_headroom` | `src/gpu/s2.rs:275` | `src/gpu/voxel.rs` | Bytes a voxel uncertain-list regrowth may add beyond the planned list. |

| `exchange_best_snapshot` | `src/pipeline/optimize.rs:52` | Exchange immutable best Arc snapshots; release retired payload outside the lock. |

| `cpu_render_tile_pixels` | `src/geometry/render.rs:381` | Bounded CPU pixel-task scheduling with an explicit row reference. |

| `render_mesh_cpu_with_tiles` | `src/geometry/render.rs:390` | Bounded CPU pixel-task scheduling with an explicit row reference. |

| `GpuS2Pipeline::new_with_shader` | `src/gpu/s2.rs:154` | GPU MC partial-count execution and fixed-seed reference validation. |

| `GpuS2Pipeline::calculate_s2_gpu_counts` | `src/gpu/s2.rs:420` | GPU MC partial-count execution and fixed-seed reference validation. |

| Function | Source | Contract |
|---|---|---|
| `GpuShellS2Pipeline::new_with_shader` | `src/gpu/s2_shell.rs:55` | Private constructor taking shader source; returns initialized resources or a GPU error. Production uses the analytic shader; tests can use the frozen enumerated-count fixture. |

| Function | Source | Contract |
|---|---|---|
| `GpuShellS2Pipeline::ensure_reduction` | `src/gpu/s2_shell.rs` | Lazily fetch the cached tile reducer and grow its final buffers under the caller error scope; returns compile errors. |

| Function | Source | Contract |
|---|---|---|
| `offset_has_overlap` | `src/gpu/s2_shell.rs:57` | Check all unsigned displacement magnitudes against grid dimensions without signed overflow. |

| Function | Source | Contract |
|---|---|---|
| `GpuShellS2Pipeline::with_device` | `src/gpu/s2_shell.rs` | Build production shell resources on a held `Arc<SharedGpuDevice>`; no new device. |
| `GpuShellS2Pipeline::build_on_device` | `src/gpu/s2_shell.rs:96` | Compile shell resources on supplied handles with balanced GPU error scopes. |
| `GpuShellS2Pipeline::compute_s2_shell_resident` | `src/gpu/s2_shell.rs:385` | Read a same-device occupancy buffer directly; caller serializes producer and consumer. |
| `GpuShellS2Pipeline::compute_shell_input` | `src/gpu/s2_shell.rs:410` | Shared execution for host-uploaded or resident occupancy with identical offset semantics. |
| `GpuVoxelPipeline::shared_device` | `src/gpu/voxel.rs` | Share the process device handle for sequential stages; no device creation. |
| `GpuVoxelPipeline::occupancy_buffer` | `src/gpu/voxel.rs:54` | Clone completed occupancy storage handle; producer must not overwrite while consumed. |

| Function | Source | Contract |
|---|---|---|
| `GpuVoxelPipeline::voxelize_count` | `src/gpu/voxel.rs:199` | Voxelize and return only the occupied-cell count; retain the device field. |
| `GpuVoxelPipeline::ensure_counter` | `src/gpu/voxel.rs` | Lazily fetch the cached integer counter and allocate the four-byte output under caller error scope; returns compile errors. |
| `GpuVoxelPipeline::voxelize_impl` | `src/gpu/voxel.rs:267` | Checked common voxel execution with full-grid or count-only readback. |

| Function | Source | Contract |
|---|---|---|
| `GpuShellS2Pipeline::compute_s2_shell_resident_stream` | `src/gpu/s2_shell.rs:410` | Consume ordered offsets lazily in bounded batches on a resident grid; preserve per-offset ratios. |

| Function | Source | Contract |
|---|---|---|
| `shell_offset_iter` | `src/geometry/s2.rs:213` | Lazy ordered shell enumeration with constant cursor storage; GPU exact streaming, both CPU exact kernels, and tests. |

| Symbol | Source | Contract |
|---|---|---|
| `ExactMemoryPlan` | `src/compute/exact_memory.rs:5` | Fresh resident exact logical GPU peak and budget-selected partial batch. |
| `ExactMemoryPlan::new` | `src/compute/exact_memory.rs:12` | Checked resource arithmetic and batch selection; may still require check_budget for infeasible minima. |
| `ExactMemoryPlan::check_budget` | `src/compute/exact_memory.rs:52` | Enforce configured MiB cap before initialization. |

| Function | Source | Contract |
|---|---|---|
| `IslandVolumes::new` | `src/pipeline/optimize_volume.rs:12` | Build ordered connected-component contributions for a population. |
| `IslandVolumes::contributions` | `src/pipeline/optimize_volume.rs:20` | Clip connected components using the existing geometric volume definition. |
| `IslandVolumes::replace` | `src/pipeline/optimize_volume.rs:33` | Update one particle and return prior entries for rollback. |
| `IslandVolumes::restore` | `src/pipeline/optimize_volume.rs:41` | Restore entries after rejection. |
| `IslandVolumes::fraction` | `src/pipeline/optimize_volume.rs:46` | Sum cached scalars in merged component order, then clamp. |
| `OptimizeS2::voxel_coverage` | `src/pipeline/optimize_execution.rs` | Build island-local incremental coverage for voxel methods at the resolved pitch; None for mesh MC. |
| `OptimizeS2::evaluate_voxel_grid` | `src/pipeline/optimize_execution.rs` | Evaluate the fixed voxel S2 definition on a maintained grid; error for mesh MC. |
| `INCREMENTAL_VOXEL_OCCUPANCY` | `src/pipeline/optimize_execution.rs` | Enables incremental voxel occupancy in SA (on; exact by construction). |
| `island_s2` | `src/pipeline/optimize.rs` | Evaluate an SA stage from coverage when present, else merged mesh with optional cached VF. |
| `COVERAGE_REFRESH_INTERVAL` | `src/pipeline/optimize.rs` | Candidate evaluations between full coverage rebuilds (64). |
| `OptimizeS2::evaluate_with_vf` | `src/pipeline/optimize_execution.rs:242` | Evaluate mesh MC with optional validated VF; reject geometric cache for voxel methods. |
| `calculate_s2_mesh_mc_seeded_with_vf` | `src/geometry/s2.rs:765` | Preserve fixed-seed mesh MC samples while using caller-provided VF. |

| Function | Source | Contract |
|---|---|---|
| `consume_frames` | `src/pipeline/mesh_render.rs:25` | Ordered bounded PNG writer; joins before fallback, preserves output errors; sequential for one worker/view. |
| `render_and_write_overlapped` | `src/pipeline/mesh_render.rs:73` | Ordered CPU render/PNG overlap via rayon::join; at most one frame writing; write error stops further views. |
| `collect_opaque_hits` | `src/geometry/scene_render.rs:96` | Nearest-distance bounded coincidence group preserving Face priority and overlay depth. |
| `consume_file_batches` | `src/io/volume.rs:47` | Decode at most two files in the current pool; consume/validate in source order and stop subsequent batches on errors. |
| `write_tiff_pages` | `src/io/volume.rs:552` | Borrowed sequential TIFF encoding with explicit final flush and propagated output errors. |
| `for_each_boundary_value` | `src/pipeline/crop.rs:313` | Visit boundary voxels once in z-major order for specialized background counters. |

| `StageTimer` | Pipeline Core | `src/pipeline/timing.rs:3` | Per-pipeline total and stage clocks. |
| `StageTimer::start` | Pipeline Core | `src/pipeline/timing.rs:11` | Start both clocks. |
| `StageTimer::restart` | Pipeline Core | `src/pipeline/timing.rs:17` | Reset stage clock silently. |
| `StageTimer::stage` | Pipeline Core | `src/pipeline/timing.rs:22` | Print stage seconds and restart. |
| `StageTimer::report` | Pipeline Core | `src/pipeline/timing.rs:30` | Print an accumulated stage duration. |
| `StageTimer::total` | Pipeline Core | `src/pipeline/timing.rs:35` | Print seconds since start. |
| `StageTimer::report_resources` | Pipeline Core | `src/pipeline/timing.rs:42` | Print workers and peak RSS. |
| `format_stage_line` | Pipeline Core | `src/pipeline/timing.rs:49` | Shared `[Timing]` stage line format. |
| `report_workers` | Pipeline Core | `src/pipeline/timing.rs:54` | Print current Rayon worker count. |
| `report_peak_rss` | Pipeline Core | `src/pipeline/timing.rs:59` | Print VmHWM bytes or unavailable. |
| `peak_rss_bytes` | Pipeline Core | `src/pipeline/timing.rs:67` | Read VmHWM, None when unavailable. |
| `parse_vm_hwm` | Pipeline Core | `src/pipeline/timing.rs:73` | Parse a VmHWM kB line. |
| `GridStats` | Geometry Core | `src/geometry/spatial.rs:11` | Bucket occupancy summary. |
| `GridStats::summary_line` | Geometry Core | `src/geometry/spatial.rs:22` | Format a `[GridStats]` line. |
| `SpatialGrid::stats` | Geometry Core | `src/geometry/spatial.rs:181` | One-pass occupancy statistics. |
| `map_vertices_centroid` | Geometry Core | `src/geometry/mesh_ops.rs:265` | Vertex map with bit-identical post-map centroid. |
| `split_mesh_into_granules_reference` | Geometry Core | `src/geometry/mesh_ops.rs:136` | Test-only former granule split oracle. |
| `PackQueryStats` | Pipeline Packing | `src/pipeline/pack.rs:32` | Relaxed atomic collision counters. |
| `PackQueryStats::summary_line` | Pipeline Packing | `src/pipeline/pack.rs:44` | Format pack query counters. |
| `run_cli` | CLI | `src/main.rs` | Parse CLI arguments and run the selected pipeline; `main` releases shared GPU devices afterwards. |
| `sample_counts` (s2_monte_carlo.wgsl) | GPU | `src/gpu/shaders/s2_monte_carlo.wgsl` | Evaluate one global logical sample id for a batch-local radius slot. |
