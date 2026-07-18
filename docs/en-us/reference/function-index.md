# Function Index

Master index of every documented function, struct, enum, and constant across `src/`, compiled from the `## Index` table at the top of each [reference](.) document. Each row links to the item's full entry.

| Item | Module | Source | Summary |
|---|---|---|---|
| `load_yaml` | Config | `src/config/mod.rs:47` | Reads a file and deserializes it as YAML into a typed config struct. |
| `parse_box_dimensions` | Config | `src/config/mod.rs:58` | Converts a 3- or 6-element dimensions slice into a `BoundingBox`. |
| `parse_usize_like` | Config | `src/config/deserialize.rs:4` | Parses a `usize` from a string, stripping underscore separators. |
| `deserialize_usize_flexible` | Config | `src/config/deserialize.rs:17` | Serde `deserialize_with` helper: accepts a YAML number or numeric string as `usize`. |
| `deserialize_option_usize_flexible` | Config | `src/config/deserialize.rs:39` | Serde `deserialize_with` helper: accepts a YAML number/string/null as `Option<usize>`. |
| `parse_i32_like` | Config | `src/config/deserialize.rs:61` | Parses an `i32` from a string, stripping underscore separators. |
| `deserialize_option_i32_flexible` | Config | `src/config/deserialize.rs:73` | Serde `deserialize_with` helper: accepts a YAML number/string/null as `Option<i32>`. |
| `default_backend` | Config | `src/config/acceleration.rs:21` | Serde default for `backend`: `"wgpu"`. |
| `default_true` | Config | `src/config/acceleration.rs:25` | Serde default for `cpu_fallback`: `true`. |
| `default_gpu_min_voxels` | Config | `src/config/acceleration.rs:29` | Serde default for `gpu_min_voxels`: `250_000`. |
| `default_gpu_precision` | Config | `src/config/acceleration.rs:33` | Serde default for `gpu_precision`: `"f32"`. |
| `AccelerationConfig::default` | Config | `src/config/acceleration.rs:38` | Rust-level `Default` impl matching the serde defaults. |
| `RustMsptError` | Core & Compute | `src/error.rs:4` | Crate-wide error enum including image encoding failures. |
| `Result` | Core & Compute | `src/error.rs:27` | Type alias `Result<T> = std::result::Result<T, RustMsptError>` used throughout the crate. |
| `Cli` | Core & Compute | `src/main.rs:19` | Top-level clap CLI struct wrapping a `Commands` subcommand. |
| `Commands` | Core & Compute | `src/main.rs:25` | Enum of the 8 CLI subcommands, including Render. |
| `default_config_path` | Core & Compute | `src/main.rs:85` | Builds the default config path under `data/input/`. |
| `pick_config_path` | Core & Compute | `src/main.rs:90` | Chooses a user-supplied config path or falls back to the default. |
| `main` (main.rs) | Core & Compute | `src/main.rs:100` | CLI entry point: parses args, loads config, applies overrides, runs the selected pipeline. |
| `main` (precision_test.rs) | Core & Compute | `src/bin/precision_test.rs:5` | Standalone diagnostic binary comparing S2 computation precision/performance across CPU exact, CPU Monte Carlo, and GPU Monte Carlo methods. |
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
| `Triangle` | Core & Compute | `src/types.rs:82` | Index triple `(a, b, c)` referencing a mesh's vertex array. |
| `Mesh` | Core & Compute | `src/types.rs:89` | Vertex/face container: `vertices: Vec<Vec3>`, `faces: Vec<Triangle>`. |
| `Mesh::empty` | Core & Compute | `src/types.rs:96` | Constructs an empty mesh. |
| `Mesh::is_empty` | Core & Compute | `src/types.rs:104` | True if the mesh has no vertices or no faces. |
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
| `select_backend` | Core & Compute | `src/compute/policy.rs:21` | Central CPU/GPU/Auto dispatch policy used by compute-heavy pipelines. |
| `MeshMetrics` | Geometry — Analysis | `src/geometry/metrics.rs:8` | Struct holding volume, surface area, equivalent diameter, and sphericity. |
| `mesh_is_closed` | Geometry — Analysis | `src/geometry/metrics.rs:20` | Validates that a mesh is a manifold, consistently-oriented, nonzero-volume shell (or set of shells). |
| `mesh_metrics` | Geometry — Analysis | `src/geometry/metrics.rs:105` | Computes volume, surface area, equivalent diameter, and sphericity for a closed mesh. |
| `scale_mesh_to_equivalent_diameter` | Geometry — Analysis | `src/geometry/metrics.rs:138` | Rescales a mesh in place so its equivalent-volume diameter matches a target. |
| `RAY_DIR_GPU` | Geometry — Analysis | `src/geometry/s2.rs:10` | Fixed non-axis-aligned unit ray direction constant, shared with the GPU ray-casting kernels. |
| `index_3d_to_flat` | Geometry — Analysis | `src/geometry/s2.rs:13` | Converts a 3D voxel index to a flat array index (y/z-major strides). |
| `ray_intersects_triangle` | Geometry — Analysis | `src/geometry/s2.rs:23` | Möller–Trumbore ray-triangle intersection test. |
| `point_inside_mesh` | Geometry — Analysis | `src/geometry/s2.rs:60` | Ray-casting point-in-mesh containment test (odd-hit rule). |
| `build_bbox_occupancy` | Geometry — Analysis | `src/geometry/s2.rs:109` | Parallel voxelization of a mesh into a boolean occupancy grid. |
| `shell_offsets_for_distance` | Geometry — Analysis | `src/geometry/s2.rs:181` | Enumerates integer voxel offsets lying within a spherical shell annulus. |
| `fill_missing_s2_with_smooth_interpolation` | Geometry — Analysis | `src/geometry/s2.rs:214` | Fills unsupported S2 radii via linear or cubic-spline interpolation. |
| `fft_index_3d` | Geometry — Analysis | `src/geometry/s2.rs:325` | Converts a 3D FFT-grid index to a flat index (identical logic to `index_3d_to_flat`). |
| `fft_3d_in_place` | Geometry — Analysis | `src/geometry/s2.rs:335` | Separable 3D FFT/IFFT performed in place on a complex buffer. |
| `autocorrelation_counts_fft` | Geometry — Analysis | `src/geometry/s2.rs:409` | Computes occupancy autocorrelation counts via FFT convolution. |
| `calculate_s2_exact_direct` | Geometry — Analysis | `src/geometry/s2.rs:441` | Exact S2 by direct pair enumeration per shell offset (no FFT). |
| `calculate_s2_exact_fft` | Geometry — Analysis | `src/geometry/s2.rs:531` | Exact S2 using FFT-based autocorrelation. |
| `calculate_s2_monte_carlo_mesh` | Geometry — Analysis | `src/geometry/s2.rs:610` | Monte Carlo S2 estimation sampling directly on the mesh (no voxelization). |
| `calculate_s2` | Geometry — Analysis | `src/geometry/s2.rs:685` | Top-level S2 dispatcher; routes to exact (FFT or direct) or voxelized Monte Carlo. |
| `approximate_s2` | Geometry — Analysis | `src/geometry/s2.rs:801` | Convenience wrapper for Monte Carlo S2 estimation with a default voxel pitch. |
| `l2_norm` | Geometry — Analysis | `src/geometry/s2.rs:806` | Euclidean distance between two S2 vectors over their common-length prefix. |
| `calculate_s2_with_gpu` | Geometry — Analysis | `src/geometry/s2.rs:825` | GPU-accelerated S2 for Monte Carlo/"both" methods, with CPU fallback. *(feature `gpu`)* |
| `calculate_s2_gpu_exact` | Geometry — Analysis | `src/geometry/s2.rs:855` | GPU-accelerated exact S2 (GPU voxelization + GPU shell pair counting). *(feature `gpu`)* |
| `mesh_bbox` | Geometry — Core | `src/geometry/bbox.rs:8` | Axis-aligned bounding box of a mesh. |
| `bbox_overlaps` | Geometry — Core | `src/geometry/bbox.rs:28` | Strict overlap test between two bounding boxes. |
| `bbox_distance` | Geometry — Core | `src/geometry/bbox.rs:38` | Minimum Euclidean distance between two bounding boxes. |
| `check_boundary_constraints_mode` | Geometry — Core | `src/geometry/bbox.rs:72` | Validates a mesh's placement against packing boundary mode rules. |
| `mesh_centroid` | Geometry — Core | `src/geometry/mesh_ops.rs:5` | Arithmetic centroid of mesh vertices. |
| `vec_norm` | Geometry — Core | `src/geometry/mesh_ops.rs:19` | Euclidean length of a vector. |
| `merge_meshes` | Geometry — Core | `src/geometry/mesh_ops.rs:24` | Combines multiple meshes into one, remapping face indices. |
| `split_mesh_into_granules` | Geometry — Core | `src/geometry/mesh_ops.rs:44` | Splits a mesh into connected components (BFS over shared vertices). |
| `translate_mesh` | Geometry — Core | `src/geometry/mesh_ops.rs:117` | Translates all mesh vertices by a delta vector, in place. |
| `move_mesh_to_target_center` | Geometry — Core | `src/geometry/mesh_ops.rs:124` | Moves a mesh so its centroid matches a target position. |
| `wrap_mesh_centroid_to_box` | Geometry — Core | `src/geometry/mesh_ops.rs:135` | Wraps a mesh's centroid into a box under periodic boundary conditions. |
| `scale_mesh` | Geometry — Core | `src/geometry/mesh_ops.rs:156` | Uniformly scales mesh vertices about the origin, in place. |
| `mesh_surface_area` | Geometry — Core | `src/geometry/mesh_ops.rs:163` | Total surface area of a mesh (sum of triangle areas). |
| `rotate_mesh_around_center` | Geometry — Core | `src/geometry/mesh_ops.rs:182` | Rotates a mesh about its centroid using Rodrigues' rotation formula. |
| `box_mesh` | Geometry — Core | `src/geometry/mesh_ops.rs:203` | Builds a triangulated box mesh from a `BoundingBox`. |
| `SpatialGrid::new` | Geometry — Core | `src/geometry/spatial.rs:14` | Constructs an empty uniform grid over a box with given cell size. |
| `SpatialGrid::insert` | Geometry — Core | `src/geometry/spatial.rs:31` | Inserts an item index into every cell its bbox overlaps. |
| `SpatialGrid::build` | Geometry — Core | `src/geometry/spatial.rs:49` | Constructs and populates a grid from a batch of (index, bbox) pairs. |
| `SpatialGrid::query_neighbors` | Geometry — Core | `src/geometry/spatial.rs:58` | Finds candidate neighbor indices overlapping a query bbox. |
| `SpatialGrid::query_neighbors_with_margin` | Geometry — Core | `src/geometry/spatial.rs:68` | Same as `query_neighbors`, expanded by a margin distance. |
| `SpatialGrid::point_to_cell_clamped` | Geometry — Core | `src/geometry/spatial.rs:93` | Maps a point to grid cell coordinates, clamped to grid bounds. |
| `SpatialGrid::point_to_cell` | Geometry — Core | `src/geometry/spatial.rs:99` | Maps a point to grid cell coordinates, unclamped. |
| `estimate_cell_size` | Geometry — Core | `src/geometry/spatial.rs:108` | Heuristically picks a `SpatialGrid` cell size from a set of bboxes. |
| `mesh_volume` | Geometry — Volume & Collision | `src/geometry/volume.rs:6` | Absolute volume of a closed mesh via the divergence theorem. |
| `mesh_signed_volume` | Geometry — Volume & Collision | `src/geometry/volume.rs:18` | Signed volume of a closed mesh (sign reflects face winding). |
| `orient_components_to_positive_volume` | Geometry — Volume & Collision | `src/geometry/volume.rs:35` | Flips winding of any connected component with negative signed volume. |
| `clip_plane_signed_distance` | Geometry — Volume & Collision | `src/geometry/volume.rs:55` | Signed distance of a point from a plane. |
| `clip_segment_plane_intersection` | Geometry — Volume & Collision | `src/geometry/volume.rs:60` | Interpolated intersection point of a segment with a plane. |
| `clip_polygon_with_plane` | Geometry — Volume & Collision | `src/geometry/volume.rs:75` | Sutherland-Hodgman clip of a convex polygon against a half-plane. |
| `quantize_point_key` | Geometry — Volume & Collision | `src/geometry/volume.rs:125` | Rounds a point to a fixed-precision integer key for dedup/hashing. |
| `collect_triangle_plane_segment` | Geometry — Volume & Collision | `src/geometry/volume.rs:139` | Extracts the segment where a triangle crosses a clip plane. |
| `plane_basis` | Geometry — Volume & Collision | `src/geometry/volume.rs:175` | Builds an orthonormal (u, v) basis in the plane perpendicular to a normal. |
| `triangulate_cap_from_segments` | Geometry — Volume & Collision | `src/geometry/volume.rs:204` | Triangulates a planar cap from cross-plane edge segments (ring-finding + fan). |
| `clip_mesh_by_plane_with_cap` | Geometry — Volume & Collision | `src/geometry/volume.rs:345` | Clips a mesh against one plane and caps the resulting opening. |
| `clip_mesh_by_bbox` | Geometry — Volume & Collision | `src/geometry/volume.rs:383` | Clips a mesh to an axis-aligned box via six successive plane clips. |
| `particle_volume_in_bbox` | Geometry — Volume & Collision | `src/geometry/volume.rs:404` | Volume of a mesh after clipping it to a bounding box. |
| `volume_fraction_in_bbox` | Geometry — Volume & Collision | `src/geometry/volume.rs:410` | Volume fraction of a single mesh within a bounding box. |
| `volume_fraction_of_meshes_in_bbox` | Geometry — Volume & Collision | `src/geometry/volume.rs:420` | Total volume fraction of multiple meshes within a bounding box (parallel). |
| `to_parry_trimesh` | Geometry — Volume & Collision | `src/geometry/collision.rs:14` | Converts a `Mesh` into a parry3d `TriMesh`. |
| `mesh_collision_exact_prepared` | Geometry — Volume & Collision | `src/geometry/collision.rs:42` | Bbox-filtered exact collision test given pre-built bboxes/shapes. |
| `mesh_distance_exact_prepared` | Geometry — Volume & Collision | `src/geometry/collision.rs:80` | Bbox-filtered exact distance query given pre-built bboxes/shapes. |
| `mesh_collision_exact` | Geometry — Volume & Collision | `src/geometry/collision.rs:125` | Convenience wrapper: builds bbox/shape then tests collision. |
| `mesh_distance_exact` | Geometry — Volume & Collision | `src/geometry/collision.rs:134` | Convenience wrapper: builds bbox/shape then computes distance. |
| `generate_periodic_ghosts` | Geometry — Volume & Collision | `src/geometry/collision.rs:148` | Generates translated ghost copies of a mesh for periodic boundary collision. |
| `simulate_forging_ffd` | Geometry — Volume & Collision | `src/geometry/forging.rs:10` | Simple Z-axis FFD compression with lateral bulge. |
| `simulate_forging_ffd_with_tracking` | Geometry — Volume & Collision | `src/geometry/forging.rs:43` | Axis-configurable FFD forging with void densification and ROI bbox tracking. |
| `GpuContext` | GPU | `src/gpu/context.rs:3` | Holds adapter name and buffer-size capabilities after successful GPU init. |
| `GpuContext::caps` | GPU | `src/gpu/context.rs:11` | Returns `BackendCaps` describing this GPU context. |
| `GpuInitError` | GPU | `src/gpu/context.rs:22` | Error type wrapping a GPU initialization failure message. |
| `GpuInitError` (`Display` impl) | GPU | `src/gpu/context.rs:24` | Formats the error message. |
| `try_init_gpu` | GPU | `src/gpu/context.rs:38` | Probes for a wgpu adapter/device and returns a `GpuContext`; used by `compute::policy::select_backend`. |
| `GpuS2Pipeline` | GPU | `src/gpu/s2.rs:10` | GPU pipeline state for Monte Carlo S2 two-point correlation. |
| `build_triangle_buffer` (s2.rs) | GPU | `src/gpu/s2.rs:27` | Builds a normalized `f32` triangle position buffer for the S2 Monte Carlo pipeline. |
| `pack_params` | GPU | `src/gpu/s2.rs:48` | Packs Monte Carlo S2 shader parameters into a byte buffer matching the WGSL `Params` layout. |
| `GpuS2Pipeline::new` | GPU | `src/gpu/s2.rs:95` | Initializes the wgpu device and Monte Carlo S2 compute pipeline. |
| `GpuS2Pipeline::update_mesh` | GPU | `src/gpu/s2.rs:266` | Re-uploads triangle data for a new mesh without recreating the pipeline. |
| `GpuS2Pipeline::ensure_output_capacity` | GPU | `src/gpu/s2.rs:287` | Grows the output buffers if the invocation count exceeds current capacity. |
| `GpuS2Pipeline::calculate_s2_gpu` | GPU | `src/gpu/s2.rs:311` | Dispatches the Monte Carlo S2 kernel for all radii and reads back results. |
| `OffsetEntry` | GPU | `src/gpu/s2_shell.rs:6` | Packed `(radius_idx, dx, dy, dz)` shell-offset record matching the WGSL layout. |
| `GpuShellS2Pipeline` | GPU | `src/gpu/s2_shell.rs:13` | GPU pipeline state for exact shell-pair S2 computation. |
| `build_offset_buffer` | GPU | `src/gpu/s2_shell.rs:30` | Converts `(radius_idx, [dx,dy,dz])` tuples into `OffsetEntry` records. |
| `GpuShellS2Pipeline::new` | GPU | `src/gpu/s2_shell.rs:48` | Initializes the wgpu device and shell S2 compute pipeline. |
| `GpuShellS2Pipeline::compute_s2_shell` | GPU | `src/gpu/s2_shell.rs:135` | Dispatches exact shell-pair counting over an occupancy grid and reads back S2(r). |
| `GpuVoxelPipeline` | GPU | `src/gpu/voxel.rs:5` | GPU pipeline state for mesh voxelization. |
| `build_triangle_buffer` (voxel.rs) | GPU | `src/gpu/voxel.rs:17` | Builds a normalized `f32` triangle position buffer for the voxelization pipeline (separate copy from `s2.rs`). |
| `GpuVoxelPipeline::new` | GPU | `src/gpu/voxel.rs:37` | Initializes the wgpu device and voxelization compute pipeline. |
| `GpuVoxelPipeline::voxelize` | GPU | `src/gpu/voxel.rs:116` | Dispatches ray-casting voxelization and reads back the occupancy grid. |
| `GpuVolumeTransformPipeline` | GPU | `src/gpu/volume_transform.rs:5` | GPU pipeline state for volume rotate-and-crop. |
| `GpuVolumeTransformPipeline::new` | GPU | `src/gpu/volume_transform.rs:23` | Initializes the wgpu device and volume-transform compute pipeline. |
| `GpuVolumeTransformPipeline::rotate_and_crop` | GPU | `src/gpu/volume_transform.rs:145` | Dispatches the rotate/crop/resample kernel and reads back the transformed volume. |
| `parse_ascii_vertex` | I/O | `src/io/stl.rs:9` | Parses one ASCII STL `vertex x y z` line into a `Vec3`. |
| `quantize_key` | I/O | `src/io/stl.rs:21` | Quantizes a vertex to a fixed-precision integer key for tolerant deduplication. |
| `dedup_vertex` | I/O | `src/io/stl.rs:31` | Deduplicates a vertex against an existing list via quantized key lookup. |
| `parse_ascii_stl` | I/O | `src/io/stl.rs:48` | Parses ASCII STL text into a `Mesh` with deduplicated vertices. |
| `parse_f32_le` | I/O | `src/io/stl.rs:93` | Parses little-endian `f32` bytes and upcasts to `f64`. |
| `parse_binary_stl` | I/O | `src/io/stl.rs:104` | Parses binary STL bytes into a `Mesh` with deduplicated vertices. |
| `looks_ascii_stl` | I/O | `src/io/stl.rs:166` | Heuristically detects whether bytes represent ASCII STL. |
| `load_stl` | I/O | `src/io/stl.rs:185` | Loads an STL file with automatic ASCII/binary detection. |
| `load_folder_stls` | I/O | `src/io/stl.rs:203` | Loads all STL files in a folder. |
| `load_stl_or_merge_folder` | I/O | `src/io/stl.rs:226` | Loads a single STL file, or merges all STLs in a directory into one mesh. |
| `save_stl` | I/O | `src/io/stl.rs:258` | Saves a mesh as a binary STL file. |
| `collect_sorted_files` | I/O | `src/io/volume.rs:50` | Collects regular files in a folder, sorted by name, optionally filtered by extension. |
| `resolve_slice_range` | I/O | `src/io/volume.rs:78` | Resolves an inclusive slice range from start/end indices, treating `-1` as "from beginning"/"to end". |
| `decode_raw_slice` | I/O | `src/io/volume.rs:107` | Decodes one raw image slice into `i64` values per bit depth, sign, and byte order. |
| `load_raw_folder` | I/O | `src/io/volume.rs:205` | Loads a `Volume3D` from a folder of raw binary slice files. |
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
| `create_progress_bar` | Pipeline — Core | `src/pipeline/mod.rs:25` | Builds a tty-aware indicatif progress bar with a given template and fill characters. |
| `RotationMode` (enum) | Pipeline — Core | `src/pipeline/rotation.rs:6` | Represents no rotation, a fixed axis, or a random axis. |
| `parse_rotation_mode` | Pipeline — Core | `src/pipeline/rotation.rs:18` | Parses `none/x/y/z/vector/any` config strings into a `RotationMode`. |
| `sample_rotation_axis` | Pipeline — Core | `src/pipeline/rotation.rs:57` | Draws a concrete rotation axis vector for a given `RotationMode`. |
| `ScalePipeline` (struct) | Pipeline — Core | `src/pipeline/scale.rs:8` | Holds `ScaleConfig` for the scaling pipeline. |
| `ScalePipeline::run` | Pipeline — Core | `src/pipeline/scale.rs:19` | Loads an STL, applies unit-conversion/factor scaling, optionally fixes orientation, saves output. |
| `ForgePipeline` (struct) | Pipeline — Core | `src/pipeline/forge.rs:13` | Holds `ForgingConfig` for the FFD forging pipeline. |
| `ForgePipeline::parse_roi_bbox` | Pipeline — Core | `src/pipeline/forge.rs:19` | Parses an optional 6-element ROI bounding box from config. |
| `ForgePipeline::parse_compression_axis` | Pipeline — Core | `src/pipeline/forge.rs:38` | Parses the compression axis string (`x`/`y`/`z`) into an index and label. |
| `ForgePipeline::run` | Pipeline — Core | `src/pipeline/forge.rs:58` | Runs FFD-based compression/forging, tracks ROI, writes forged STL and a text report. |
| `MeasurePipeline` (struct) | Pipeline — Core | `src/pipeline/measure.rs:16` | Holds `MeasurementConfig` for the S2/volume-fraction measurement pipeline. |
| `MeasurePipeline::parse_optional_bbox` | Pipeline — Core | `src/pipeline/measure.rs:22` | Parses an optional bounding box (3-element size or 6-element min/max) from config. |
| `MeasurePipeline::l2_error` | Pipeline — Core | `src/pipeline/measure.rs:31` | Computes the L2 distance between two S2 value vectors over their common prefix length. |
| `MeasurePipeline::run` | Pipeline — Core | `src/pipeline/measure.rs:53` | Loads an STL, computes volume fraction and S2 correlation (exact/MC/both, CPU or GPU), writes a report. |
| `CropPipeline` (struct) | Pipeline — Crop & Split-Filter | `src/pipeline/crop.rs:14` | Holds `CropConfig` for the crop pipeline. |
| `InterpolationMode` (enum) | Pipeline — Crop & Split-Filter | `src/pipeline/crop.rs:19` | Nearest vs. trilinear resampling mode used during rotate+crop. |
| `parse_byte_order` | Pipeline — Crop & Split-Filter | `src/pipeline/crop.rs:25` | Parses `little`/`big` (or `le`/`be`) into a `ByteOrder`. |
| `parse_interpolation_mode` | Pipeline — Crop & Split-Filter | `src/pipeline/crop.rs:36` | Parses `nearest`/`trilinear` into an `InterpolationMode`, defaulting to trilinear. |
| `load_input_volume` | Pipeline — Crop & Split-Filter | `src/pipeline/crop.rs:52` | Loads the input CT volume from a raw folder or TIFF/TIFF-folder per config. |
| `voxel_index` | Pipeline — Crop & Split-Filter | `src/pipeline/crop.rs:86` | Computes the flat data index for `(x, y, z)` voxel coordinates. |
| `sample_voxel_or_background` | Pipeline — Crop & Split-Filter | `src/pipeline/crop.rs:91` | Reads a voxel at integer coordinates, returning the background value if out of bounds. |
| `sample_nearest` | Pipeline — Crop & Split-Filter | `src/pipeline/crop.rs:106` | Nearest-neighbor sample at fractional source coordinates. |
| `sample_trilinear` | Pipeline — Crop & Split-Filter | `src/pipeline/crop.rs:114` | Trilinear-interpolated sample at fractional source coordinates. |
| `stabilize_bound` | Pipeline — Crop & Split-Filter | `src/pipeline/crop.rs:147` | Snaps a near-integer float to its exact integer within an epsilon. |
| `float_bounds_to_inclusive_i64` | Pipeline — Crop & Split-Filter | `src/pipeline/crop.rs:157` | Converts float min/max bounds to an inclusive integer `[start, end]` range. |
| `boundary_non_bg_ratio` | Pipeline — Crop & Split-Filter | `src/pipeline/crop.rs:170` | Fraction of non-background voxels within a boundary shell of given thickness. |
| `infer_trim_pixels` | Pipeline — Crop & Split-Filter | `src/pipeline/crop.rs:211` | Heuristically infers 0/1/2 pixels of edge trim from boundary artifact intensity. |
| `resolve_trim_pixels` | Pipeline — Crop & Split-Filter | `src/pipeline/crop.rs:229` | Resolves the effective edge-trim pixel count from config, supporting `-1` for auto. |
| `trim_volume_border` | Pipeline — Crop & Split-Filter | `src/pipeline/crop.rs:252` | Trims a fixed number of border voxels from the XY faces of a volume. |
| `detect_background_mode` | Pipeline — Crop & Split-Filter | `src/pipeline/crop.rs:296` | Detects the background value as the modal voxel value on the volume boundary. |
| `estimate_pca_bbox` | Pipeline — Crop & Split-Filter | `src/pipeline/crop.rs:330` | Computes PCA rotation, centroid, and rotated-frame foreground bounding box. |
| `rotate_and_crop` | Pipeline — Crop & Split-Filter | `src/pipeline/crop.rs:432` | CPU, rayon-parallel rotate-and-crop of the volume into an axis-aligned output. |
| `rotate_and_crop_gpu` | Pipeline — Crop & Split-Filter | `src/pipeline/crop.rs:494` | GPU-accelerated rotate-and-crop via `GpuVolumeTransformPipeline` (feature `gpu`). |
| `CropPipeline::run` | Pipeline — Crop & Split-Filter | `src/pipeline/crop.rs:559` | Orchestrates load → background detect → PCA bbox → rotate+crop (GPU or CPU) → edge trim → save TIFF. |
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
| `OptimizePipeline` | Pipeline — Optimize | `src/pipeline/optimize.rs:27` | Pipeline entry-point struct wrapping the parsed `OptimizationConfig`. |
| `ParticlePrepared` | Pipeline — Optimize | `src/pipeline/optimize.rs:32` | Per-particle cache of mesh + precomputed bbox + parry3d collision shape. |
| `IslandResult` | Pipeline — Optimize | `src/pipeline/optimize.rs:39` | Outcome of one SA island run: best particles/loss/S2 plus profiling durations. |
| `GlobalBest` | Pipeline — Optimize | `src/pipeline/optimize.rs:47` | Cross-island shared best solution, guarded by `Arc<Mutex<GlobalBest>>`. |
| `prepare_particle` | Pipeline — Optimize | `src/pipeline/optimize.rs:53` | Builds a `ParticlePrepared` (bbox + parry3d shape) from a raw mesh. |
| `format_s2_series` | Pipeline — Optimize | `src/pipeline/optimize.rs:60` | Formats an S2 vector as a fixed-precision, space-separated string. |
| `push_history_s2` | Pipeline — Optimize | `src/pipeline/optimize.rs:69` | Appends a labeled S2 snapshot line to the run's history log. |
| `prune_progress_message` | Pipeline — Optimize | `src/pipeline/optimize.rs:74` | Builds the progress-bar message string for the pruning stage. |
| `selective_prune_to_target_vf` | Pipeline — Optimize | `src/pipeline/optimize.rs:86` | Pre-annealing stage: iteratively removes particles to approach the target volume fraction while minimizing S2-loss increase. |
| `run_sa_island` | Pipeline — Optimize | `src/pipeline/optimize.rs:285` | The core simulated-annealing loop for one island; the single most important function in the codebase. |
| `OptimizePipeline::run` | Pipeline — Optimize | `src/pipeline/optimize.rs:797` | Top-level `Pipeline::run` orchestration: load, target computation, pruning, single/multi-island SA, save. |
| `TARGET_BIN_PROBES` | Pipeline — Packing | `src/pipeline/pack.rs:27` | Max consecutive placement failures tolerated for a chosen bin before it is excluded from this round's re-selection. |
| `PackPipeline` | Pipeline — Packing | `src/pipeline/pack.rs:29` | Pipeline struct wrapping a `PackingConfig`; implements `Pipeline`. |
| `CandidateProposal` | Pipeline — Packing | `src/pipeline/pack.rs:34` | One drawn candidate mesh plus its optional precomputed `MeshMetrics`. |
| `validate_sphericity_target` | Pipeline — Packing | `src/pipeline/pack.rs:44` | Validates `target_mean_sphericity`/`mean_sphericity_tolerance` config before packing starts. |
| `check_geometry_filters` | Pipeline — Packing | `src/pipeline/pack.rs:75` | Applies configured `min_volume`, `max_aspect_ratio`, `max_sharpness_ratio` filters to a candidate mesh. |
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
| `save_image` | I/O | `src/io/image.rs` | Validates and writes RGBA8 PNG. |
| `request_adapter_device` | GPU | `src/gpu/context.rs` | Shared filtered wgpu device request. |
| `RenderVertex` | GPU | `src/gpu/render.rs` | Packed position/flat-normal vertex. |
| `RenderUniforms` | GPU | `src/gpu/render.rs` | Camera/appearance uniform layout. |
| `GpuRenderPipeline` | GPU | `src/gpu/render.rs` | Offscreen render state. |
| `build_render_vertices` | GPU | `src/gpu/render.rs` | Expands mesh faces for rasterization. |
| `to_wgsl_mat4` | GPU | `src/gpu/render.rs` | Converts matrix layout/precision for WGSL. |
| `GpuRenderPipeline::new` | GPU | `src/gpu/render.rs` | Creates the offscreen render pipeline. |
| `GpuRenderPipeline::render` | GPU | `src/gpu/render.rs` | Rasterizes and reads back RGBA8. |
| `RenderPipeline` | Pipeline — Core | `src/pipeline/render.rs` | Holds `RenderConfig`. |
| `RenderPipeline::run` | Pipeline — Core | `src/pipeline/render.rs` | Executes STL-to-PNG rendering with fallback. |

**Total: 287 documented items** (functions, methods, structs, enums, and constants) across 11 reference documents, covering all 50 Rust source files under `src/`.

## Notes on counts

- The index includes functions plus documented structs, enums, constants, and associated methods, not functions alone.
- Test-only functions inside `#[cfg(test)] mod tests` blocks (e.g. the 6 unit tests in `src/compute/mod.rs`) are intentionally excluded — this index covers production code only.
- The two `main` functions (`src/main.rs` and `src/bin/precision_test.rs`) are listed separately since they belong to different binary targets (`rustmspt` and `precision_test`).
- Two `build_triangle_buffer` functions appear (`src/gpu/s2.rs` and `src/gpu/voxel.rs`) — these are separate, independently-defined private helpers with the same name in different modules, not duplicates.

## Reference documents

| Document | Covers |
|---|---|
| [core-and-compute.md](core-and-compute.md) | `main.rs`, `lib.rs`, `error.rs`, `types.rs`, `bin/precision_test.rs`, `compute/*` |
| [config.md](config.md) | `config/*` — YAML config structs and deserialization helpers |
| [geometry-core.md](geometry-core.md) | `geometry/{mod,bbox,mesh_ops,spatial,render}.rs` |
| [geometry-volume-collision.md](geometry-volume-collision.md) | `geometry/{volume,collision,forging}.rs` |
| [geometry-analysis.md](geometry-analysis.md) | `geometry/{metrics,s2}.rs` |
| [gpu.md](gpu.md) | `gpu/*` (feature-gated) |
| [io.md](io.md) | `io/{image,stl,volume}.rs` |
| [pipeline-core.md](pipeline-core.md) | `pipeline/{mod,rotation,scale,forge,measure,render}.rs` |
| [pipeline-crop-and-splitfilter.md](pipeline-crop-and-splitfilter.md) | `pipeline/{crop,split_filter}.rs` |
| [pipeline-packing.md](pipeline-packing.md) | `pipeline/{pack,pack_targets}.rs` |
| [pipeline-optimize.md](pipeline-optimize.md) | `pipeline/optimize.rs` |

## See also

- [Algorithm docs](../algorithms/) — conceptual explanations of S2 correlation, simulated annealing, FFD forging, packing target distribution, PCA volume alignment, spatial-grid collision, and mesh clipping/volume fraction.
- [Example docs](../examples/) — per-pipeline walkthroughs with real captured output.
- [AGENTS.md](../../../AGENTS.md) — the `AI-FUNC-SUMMARY` comment convention these docs are built from, and the requirement to keep both in sync.
