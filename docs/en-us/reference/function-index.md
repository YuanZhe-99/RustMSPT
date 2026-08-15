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
| `MeshRenderConfig` | Mesh Tooling | `src/config/mesh_render.rs:88` | Top-level YAML document for the mesh-render subcommand. |
| `MeshRenderPipeline::run` | Mesh Tooling | `src/pipeline/mesh_render.rs:16` | mesh-render subcommand: load VTU, extract scene, render one PNG per view. |
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
| `MeshVerifyParams` | Mesh Tooling | `src/config/mesh_verify.rs:36` | mesh_verify: YAML block (input, report/json/annotate, gates). |
| `MeshVerifySurface` | Mesh Tooling | `src/config/mesh_verify.rs` | One [V5] input surface: a bare path, or `{stl, priority}` when the mesh was built with an explicit rank. |
| `MeshVerifySurface::resolved_priority` | Mesh Tooling | `src/config/mesh_verify.rs` | Effective priority of a [V5] surface: the explicit value, else 0 — it must mirror the `meshgen.inputs` priority the mesh was built with. |
| `MeshVerifyConfig` | Mesh Tooling | `src/config/mesh_verify.rs:50` | Top-level YAML document for the mesh-verify subcommand. |
| `gates_from_config` | Mesh Tooling | `src/pipeline/mesh_verify.rs:18` | Overlay YAML overrides onto the contract default gates. |
| `verify_file` | Mesh Tooling | `src/pipeline/mesh_verify.rs:42` | Load, validate, verify, and write log/JSON/annotated VTU. |
| `MeshVerifyPipeline::run` | Mesh Tooling | `src/pipeline/mesh_verify.rs:9` | mesh-verify subcommand; nonzero exit when a gate fails. |
| `InputKind` | Mesh Tooling | `src/config/meshgen.rs:10` | Per-input surface role override: auto / solid / sheet. |
| `RepairLevel` | Mesh Tooling | `src/config/meshgen.rs:20` | S0 repair aggressiveness: strict / conservative / permissive. |
| `CoincidencePolicy` | Mesh Tooling | `src/config/meshgen.rs:30` | G2-2 coincidence policy: merge / reject / warn. |
| `FemProfile` | Mesh Tooling | `src/config/meshgen.rs:40` | Target solver profile for sheet/thin handling: implicit / explicit / none. |
| `DeterminismMode` | Mesh Tooling | `src/config/meshgen.rs:50` | Run reproducibility contract: strict (bitwise) / fast. |
| `UnmappedPolicy` | Mesh Tooling | `src/config/meshgen.rs:59` | INP export behaviour for unmapped regions: error / elset-only. |
| `SnapshotMode` | Mesh Tooling | `src/config/meshgen.rs:68` | Contract snapshot emission level: none / key / all. |
| `MeshGenInput` | Mesh Tooling | `src/config/meshgen.rs:80` | One STL input: stl path, optional priority, kind. |
| `MeshGenDomain` | Mesh Tooling | `src/config/meshgen.rs:90` | Axis-aligned generation domain (min/max, 3-component, min < max). |
| `MeshGenSizing` | Mesh Tooling | `src/config/meshgen.rs:99` | Sizing-field limits as fractions of the box diagonal. |
| `MeshGenGaps` | Mesh Tooling | `src/config/meshgen.rs:114` | Gap-field thickness factors and confidence floor. |
| `MeshGenEnvelope` | Mesh Tooling | `src/config/meshgen.rs:125` | Numerical envelope thickness as a fraction of the box diagonal. |
| `MeshGenRepair` | Mesh Tooling | `src/config/meshgen.rs:132` | S0 repair configuration (level). |
| `MeshGenMaterials` | Mesh Tooling | `src/config/meshgen.rs:143` | Material assignments for INP export; by_component preserves duplicate keys. |
| `MeshGenOutput` | Mesh Tooling | `src/config/meshgen.rs:154` | Output destinations: required vtu, optional abaqus/report. |
| `MeshGenParams` | Mesh Tooling | `src/config/meshgen.rs:168` | The meshgen: YAML block; call validate() after loading. |
| `MeshGenConfig` | Mesh Tooling | `src/config/meshgen.rs:198` | Top-level YAML wrapper (meshgen:). |
| `MeshGenInput::resolved_priority` | Mesh Tooling | `src/config/meshgen.rs:204` | Effective priority (explicit override or the file index). |
| `MeshGenParams::validate` | Mesh Tooling | `src/config/meshgen.rs:218` | Enforce the PLAN §6.3 parse-time rejects; Ok or InvalidConfig. |
| `deserialize_component_map` | Mesh Tooling | `src/config/meshgen.rs:352` | Deserialize by_component as a pair list preserving duplicate keys. |
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
| `GpuScenePipeline` | Mesh Tooling | `src/gpu/scene_render.rs:70` | Offscreen GPU scene preview: coloured TriangleList + LineList overlay with clip-plane discard. |
| `GpuScenePipeline::render` | Mesh Tooling | `src/gpu/scene_render.rs:310` | Render one scene from one camera as an opaque preview. |
| `GpuScenePipeline::render_views` | Mesh Tooling | `src/gpu/scene_render.rs:330` | Batch views: one geometry upload reused across every camera. |

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
