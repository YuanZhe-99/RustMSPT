# 函数索引

> **待翻译：** 新增 render 配置、相机、CPU/GPU 渲染、PNG I/O、`RenderedImage` 和 `RenderPipeline` 的完整索引见[英文函数索引](../../en-us/reference/function-index.md)。

`src/` 中每个已记录的函数、结构体、枚举和常量的主索引，编译自每份[参考文档](.)顶部的 `## Index` 表格。每一行都链接到该条目的完整说明。

| 条目 | 模块 | 源码位置 | 概述 |
|---|---|---|---|
| `load_yaml` | Config | `src/config/mod.rs:47` | 读取一个文件并将其作为 YAML 反序列化为类型化的配置结构体。 |
| `parse_box_dimensions` | Config | `src/config/mod.rs:58` | 将一个 3 或 6 元素的尺寸切片转换为 `BoundingBox`。 |
| `parse_usize_like` | Config | `src/config/deserialize.rs:4` | 从字符串解析 `usize`，去除下划线分隔符。 |
| `deserialize_usize_flexible` | Config | `src/config/deserialize.rs:17` | Serde `deserialize_with` 辅助函数：接受 YAML 数字或数字字符串作为 `usize`。 |
| `deserialize_option_usize_flexible` | Config | `src/config/deserialize.rs:39` | Serde `deserialize_with` 辅助函数：接受 YAML 数字/字符串/null 作为 `Option<usize>`。 |
| `parse_i32_like` | Config | `src/config/deserialize.rs:61` | 从字符串解析 `i32`，去除下划线分隔符。 |
| `deserialize_option_i32_flexible` | Config | `src/config/deserialize.rs:73` | Serde `deserialize_with` 辅助函数：接受 YAML 数字/字符串/null 作为 `Option<i32>`。 |
| `default_backend` | Config | `src/config/acceleration.rs:21` | `backend` 的 serde 默认值：`"wgpu"`。 |
| `default_true` | Config | `src/config/acceleration.rs:25` | `cpu_fallback` 的 serde 默认值：`true`。 |
| `default_gpu_min_voxels` | Config | `src/config/acceleration.rs:29` | `gpu_min_voxels` 的 serde 默认值：`250_000`。 |
| `default_gpu_precision` | Config | `src/config/acceleration.rs:33` | `gpu_precision` 的 serde 默认值：`"f32"`。 |
| `AccelerationConfig::default` | Config | `src/config/acceleration.rs:38` | 与 serde 默认值相匹配的 Rust 层 `Default` 实现。 |
| `RustMsptError` | Core & Compute | `src/error.rs:4` | 覆盖 I/O、YAML、TIFF、配置、网格和 GPU 失败情形的全局错误枚举。 |
| `Result` | Core & Compute | `src/error.rs:27` | 在整个 crate 中使用的类型别名 `Result<T> = std::result::Result<T, RustMsptError>`。 |
| `Cli` | Core & Compute | `src/main.rs:19` | 顶层 clap CLI 结构体，包装一个 `Commands` 子命令。 |
| `Commands` | Core & Compute | `src/main.rs:25` | 7 个 CLI 子命令的枚举（Forge/Measure/Optimize/Pack/Scale/Crop/SplitFilter）。 |
| `default_config_path` | Core & Compute | `src/main.rs:85` | 构建 `data/input/` 下的默认配置路径。 |
| `pick_config_path` | Core & Compute | `src/main.rs:90` | 选择用户提供的配置路径，或回退到默认路径。 |
| `main`（main.rs） | Core & Compute | `src/main.rs:100` | CLI 入口点：解析参数、加载配置、应用覆盖项、运行所选流水线。 |
| `main`（precision_test.rs） | Core & Compute | `src/bin/precision_test.rs:5` | 独立的诊断二进制程序，比较 CPU 精确法、CPU 蒙特卡洛法与 GPU 蒙特卡洛法三种方法计算 S2 的精度/性能。 |
| `Vec3` | Core & Compute | `src/types.rs:2` | 具有 `f64` 分量和基本向量代数方法的三维向量。 |
| `Vec3::new` | Core & Compute | `src/types.rs:10` | 用 x/y/z 分量构造一个向量。 |
| `Vec3::add` | Core & Compute | `src/types.rs:15` | 向量加法。 |
| `Vec3::sub` | Core & Compute | `src/types.rs:20` | 向量减法。 |
| `Vec3::scale` | Core & Compute | `src/types.rs:25` | 标量乘法。 |
| `Vec3::dot` | Core & Compute | `src/types.rs:30` | 点积。 |
| `Vec3::cross` | Core & Compute | `src/types.rs:35` | 叉积。 |
| `BoundingBox` | Core & Compute | `src/types.rs:45` | 由 `min`/`max` 角点定义的轴对齐包围盒。 |
| `BoundingBox::from_size` | Core & Compute | `src/types.rs:52` | 从原点到给定尺寸构建一个包围盒。 |
| `BoundingBox::size` | Core & Compute | `src/types.rs:60` | 返回包围盒的边长。 |
| `BoundingBox::volume` | Core & Compute | `src/types.rs:65` | 返回包围盒的（非负）体积。 |
| `BoundingBox::contains_point` | Core & Compute | `src/types.rs:71` | 测试一个点是否位于包围盒内部或边界上。 |
| `Triangle` | Core & Compute | `src/types.rs:82` | 引用网格顶点数组的索引三元组 `(a, b, c)`。 |
| `Mesh` | Core & Compute | `src/types.rs:89` | 顶点/面容器：`vertices: Vec<Vec3>`、`faces: Vec<Triangle>`。 |
| `Mesh::empty` | Core & Compute | `src/types.rs:96` | 构造一个空网格。 |
| `Mesh::is_empty` | Core & Compute | `src/types.rs:104` | 若网格没有顶点或没有面，则为 true。 |
| `AccelerationMode` | Core & Compute | `src/compute/backend.rs:5` | 请求的计算模式枚举：`Auto`（默认）、`Cpu`、`Gpu`。 |
| `AccelerationMode::fmt`（Display） | Core & Compute | `src/compute/backend.rs:12` | 将模式格式化为 `"auto"`/`"cpu"`/`"gpu"`。 |
| `BackendCaps` | Core & Compute | `src/compute/backend.rs:23` | 所选后端上报的能力（名称、GPU 支持情况、缓冲区大小限制）。 |
| `ComputeBackend` | Core & Compute | `src/compute/backend.rs:31` | 实际选定的具体后端枚举：`Cpu`，或 `Gpu { .. }`（特性门控）。 |
| `ComputeBackend::name` | Core & Compute | `src/compute/backend.rs:43` | 人类可读的后端名称。 |
| `ComputeBackend::is_gpu` | Core & Compute | `src/compute/backend.rs:52` | 该后端是否为 GPU 后端。 |
| `ComputeBackend::caps` | Core & Compute | `src/compute/backend.rs:64` | 返回该后端的 `BackendCaps` 摘要。 |
| `ComputeBackend::fmt`（Display） | Core & Compute | `src/compute/backend.rs:87` | 格式化为 `"cpu"` 或 `"gpu/wgpu/{adapter_name}"`。 |
| `FallbackReason` | Core & Compute | `src/compute/policy.rs:4` | 记录为何无法满足所请求的后端，以及实际改用了什么。 |
| `BackendSelection` | Core & Compute | `src/compute/policy.rs:10` | 后端选择的结果：选定的 `ComputeBackend` 加上可选的 `FallbackReason`。 |
| `select_backend` | Core & Compute | `src/compute/policy.rs:21` | 计算密集型流水线使用的中央 CPU/GPU/Auto 调度策略。 |
| `MeshMetrics` | Geometry — Analysis | `src/geometry/metrics.rs:8` | 保存体积、表面积、等体积直径和球形度的结构体。 |
| `mesh_is_closed` | Geometry — Analysis | `src/geometry/metrics.rs:20` | 验证网格是否为流形、朝向一致、体积非零的壳体（或壳体集合）。 |
| `mesh_metrics` | Geometry — Analysis | `src/geometry/metrics.rs:105` | 为一个封闭网格计算体积、表面积、等体积直径和球形度。 |
| `scale_mesh_to_equivalent_diameter` | Geometry — Analysis | `src/geometry/metrics.rs:138` | 就地缩放网格，使其等体积直径匹配目标值。 |
| `RAY_DIR_GPU` | Geometry — Analysis | `src/geometry/s2.rs:10` | 固定的非轴对齐单位射线方向常量，与 GPU 光线投射内核共享。 |
| `index_3d_to_flat` | Geometry — Analysis | `src/geometry/s2.rs:13` | 将三维体素索引转换为扁平数组索引（y/z 主序跨步）。 |
| `ray_intersects_triangle` | Geometry — Analysis | `src/geometry/s2.rs:23` | Möller–Trumbore 光线-三角形相交测试。 |
| `point_inside_mesh` | Geometry — Analysis | `src/geometry/s2.rs:60` | 光线投射的点-网格包含测试（奇数命中规则）。 |
| `build_bbox_occupancy` | Geometry — Analysis | `src/geometry/s2.rs:109` | 将网格并行体素化为一个布尔占用网格。 |
| `shell_offsets_for_distance` | Geometry — Analysis | `src/geometry/s2.rs:181` | 枚举位于一个球壳环带内的整数体素偏移量。 |
| `fill_missing_s2_with_smooth_interpolation` | Geometry — Analysis | `src/geometry/s2.rs:214` | 通过线性或三次样条插值填补不受支持的 S2 半径值。 |
| `fft_index_3d` | Geometry — Analysis | `src/geometry/s2.rs:325` | 将三维 FFT 网格索引转换为扁平索引（与 `index_3d_to_flat` 逻辑相同）。 |
| `fft_3d_in_place` | Geometry — Analysis | `src/geometry/s2.rs:335` | 对复数缓冲区就地执行可分离的三维 FFT/IFFT。 |
| `autocorrelation_counts_fft` | Geometry — Analysis | `src/geometry/s2.rs:409` | 通过 FFT 卷积计算占用自相关计数。 |
| `calculate_s2_exact_direct` | Geometry — Analysis | `src/geometry/s2.rs:441` | 按球壳偏移逐对直接枚举计算精确 S2（不使用 FFT）。 |
| `calculate_s2_exact_fft` | Geometry — Analysis | `src/geometry/s2.rs:531` | 使用基于 FFT 的自相关计算精确 S2。 |
| `calculate_s2_monte_carlo_mesh` | Geometry — Analysis | `src/geometry/s2.rs:610` | 直接在网格上采样进行蒙特卡洛 S2 估计（不体素化）。 |
| `calculate_s2` | Geometry — Analysis | `src/geometry/s2.rs:685` | 顶层 S2 调度器；路由到精确法（FFT 或直接法）或体素化蒙特卡洛法。 |
| `approximate_s2` | Geometry — Analysis | `src/geometry/s2.rs:801` | 采用默认体素间距进行蒙特卡洛 S2 估计的便捷封装。 |
| `l2_norm` | Geometry — Analysis | `src/geometry/s2.rs:806` | 两个 S2 向量在其共同长度前缀上的欧几里得距离。 |
| `calculate_s2_with_gpu` | Geometry — Analysis | `src/geometry/s2.rs:825` | 针对蒙特卡洛/"both" 方法的 GPU 加速 S2，带 CPU 回退。*（特性 `gpu`）* |
| `calculate_s2_gpu_exact` | Geometry — Analysis | `src/geometry/s2.rs:855` | GPU 加速的精确 S2（GPU 体素化 + GPU 球壳配对计数）。*（特性 `gpu`）* |
| `mesh_bbox` | Geometry — Core | `src/geometry/bbox.rs:8` | 网格的轴对齐包围盒。 |
| `bbox_overlaps` | Geometry — Core | `src/geometry/bbox.rs:28` | 两个包围盒之间的严格重叠测试。 |
| `bbox_distance` | Geometry — Core | `src/geometry/bbox.rs:38` | 两个包围盒之间的最小欧几里得距离。 |
| `check_boundary_constraints_mode` | Geometry — Core | `src/geometry/bbox.rs:72` | 根据堆积边界模式规则校验网格的放置位置。 |
| `mesh_centroid` | Geometry — Core | `src/geometry/mesh_ops.rs:5` | 网格顶点的算术质心。 |
| `vec_norm` | Geometry — Core | `src/geometry/mesh_ops.rs:19` | 向量的欧几里得长度。 |
| `merge_meshes` | Geometry — Core | `src/geometry/mesh_ops.rs:24` | 将多个网格合并为一个，并重新映射面索引。 |
| `split_mesh_into_granules` | Geometry — Core | `src/geometry/mesh_ops.rs:44` | 将网格拆分为连通分量（在共享顶点上做 BFS）。 |
| `translate_mesh` | Geometry — Core | `src/geometry/mesh_ops.rs:117` | 就地按增量向量平移所有网格顶点。 |
| `move_mesh_to_target_center` | Geometry — Core | `src/geometry/mesh_ops.rs:124` | 移动网格使其质心与目标位置一致。 |
| `wrap_mesh_centroid_to_box` | Geometry — Core | `src/geometry/mesh_ops.rs:135` | 在周期边界条件下，将网格质心环绕映射到包围盒内。 |
| `scale_mesh` | Geometry — Core | `src/geometry/mesh_ops.rs:156` | 就地围绕原点均匀缩放网格顶点。 |
| `mesh_surface_area` | Geometry — Core | `src/geometry/mesh_ops.rs:163` | 网格总表面积（各三角形面积之和）。 |
| `rotate_mesh_around_center` | Geometry — Core | `src/geometry/mesh_ops.rs:182` | 使用罗德里格斯旋转公式绕网格质心旋转网格。 |
| `box_mesh` | Geometry — Core | `src/geometry/mesh_ops.rs:203` | 从 `BoundingBox` 构建一个三角化的立方体网格。 |
| `SpatialGrid::new` | Geometry — Core | `src/geometry/spatial.rs:14` | 用给定单元大小在一个包围盒上构造一个空的均匀网格。 |
| `SpatialGrid::insert` | Geometry — Core | `src/geometry/spatial.rs:31` | 将某项的索引插入其包围盒重叠的每个单元格中。 |
| `SpatialGrid::build` | Geometry — Core | `src/geometry/spatial.rs:49` | 从一批 (index, bbox) 对构造并填充一个网格。 |
| `SpatialGrid::query_neighbors` | Geometry — Core | `src/geometry/spatial.rs:58` | 查找与查询包围盒重叠的候选邻居索引。 |
| `SpatialGrid::query_neighbors_with_margin` | Geometry — Core | `src/geometry/spatial.rs:68` | 与 `query_neighbors` 相同，但按给定余量扩展。 |
| `SpatialGrid::point_to_cell_clamped` | Geometry — Core | `src/geometry/spatial.rs:93` | 将一个点映射到网格单元坐标，并夹紧到网格边界内。 |
| `SpatialGrid::point_to_cell` | Geometry — Core | `src/geometry/spatial.rs:99` | 将一个点映射到网格单元坐标，不做夹紧处理。 |
| `estimate_cell_size` | Geometry — Core | `src/geometry/spatial.rs:108` | 根据一组包围盒启发式地选取 `SpatialGrid` 的单元大小。 |
| `mesh_volume` | Geometry — Volume & Collision | `src/geometry/volume.rs:6` | 通过散度定理计算封闭网格的绝对体积。 |
| `mesh_signed_volume` | Geometry — Volume & Collision | `src/geometry/volume.rs:18` | 封闭网格的带符号体积（符号反映面片缠绕方向）。 |
| `orient_components_to_positive_volume` | Geometry — Volume & Collision | `src/geometry/volume.rs:35` | 翻转任何带符号体积为负的连通分量的缠绕方向。 |
| `clip_plane_signed_distance` | Geometry — Volume & Collision | `src/geometry/volume.rs:55` | 一个点到平面的带符号距离。 |
| `clip_segment_plane_intersection` | Geometry — Volume & Collision | `src/geometry/volume.rs:60` | 一条线段与平面相交处的插值交点。 |
| `clip_polygon_with_plane` | Geometry — Volume & Collision | `src/geometry/volume.rs:75` | 用 Sutherland-Hodgman 算法将一个凸多边形按半平面裁剪。 |
| `quantize_point_key` | Geometry — Volume & Collision | `src/geometry/volume.rs:125` | 将一个点四舍五入为固定精度的整数键，用于去重/哈希。 |
| `collect_triangle_plane_segment` | Geometry — Volume & Collision | `src/geometry/volume.rs:139` | 提取一个三角形穿越裁剪平面处的线段。 |
| `plane_basis` | Geometry — Volume & Collision | `src/geometry/volume.rs:175` | 在垂直于某法向量的平面内构建一个正交归一的 (u, v) 基。 |
| `triangulate_cap_from_segments` | Geometry — Volume & Collision | `src/geometry/volume.rs:204` | 从穿越平面的边线段三角化出一个平面封顶（环查找 + 扇形三角化）。 |
| `clip_mesh_by_plane_with_cap` | Geometry — Volume & Collision | `src/geometry/volume.rs:345` | 用一个平面裁剪网格，并对产生的开口进行封顶。 |
| `clip_mesh_by_bbox` | Geometry — Volume & Collision | `src/geometry/volume.rs:383` | 通过六次连续的平面裁剪，将网格裁剪到一个轴对齐包围盒内。 |
| `particle_volume_in_bbox` | Geometry — Volume & Collision | `src/geometry/volume.rs:404` | 网格裁剪到一个包围盒后的体积。 |
| `volume_fraction_in_bbox` | Geometry — Volume & Collision | `src/geometry/volume.rs:410` | 单个网格在一个包围盒内的体积分数。 |
| `volume_fraction_of_meshes_in_bbox` | Geometry — Volume & Collision | `src/geometry/volume.rs:420` | 多个网格在一个包围盒内的总体积分数（并行计算）。 |
| `to_parry_trimesh` | Geometry — Volume & Collision | `src/geometry/collision.rs:14` | 将一个 `Mesh` 转换为 parry3d 的 `TriMesh`。 |
| `mesh_collision_exact_prepared` | Geometry — Volume & Collision | `src/geometry/collision.rs:42` | 在给定预先构建的包围盒/形状的情况下，进行经包围盒过滤的精确碰撞测试。 |
| `mesh_distance_exact_prepared` | Geometry — Volume & Collision | `src/geometry/collision.rs:80` | 在给定预先构建的包围盒/形状的情况下，进行经包围盒过滤的精确距离查询。 |
| `mesh_collision_exact` | Geometry — Volume & Collision | `src/geometry/collision.rs:125` | 便捷封装：构建包围盒/形状后测试碰撞。 |
| `mesh_distance_exact` | Geometry — Volume & Collision | `src/geometry/collision.rs:134` | 便捷封装：构建包围盒/形状后计算距离。 |
| `generate_periodic_ghosts` | Geometry — Volume & Collision | `src/geometry/collision.rs:148` | 为周期边界碰撞生成一个网格经平移的镜像副本。 |
| `simulate_forging_ffd` | Geometry — Volume & Collision | `src/geometry/forging.rs:10` | 带侧向鼓凸的简单 Z 轴自由变形压缩。 |
| `simulate_forging_ffd_with_tracking` | Geometry — Volume & Collision | `src/geometry/forging.rs:43` | 带孔隙致密化和感兴趣区域包围盒跟踪、轴向可配置的自由变形锻造。 |
| `GpuContext` | GPU | `src/gpu/context.rs:3` | GPU 初始化成功后持有适配器名称与缓冲区大小能力信息。 |
| `GpuContext::caps` | GPU | `src/gpu/context.rs:11` | 返回描述该 GPU 上下文的 `BackendCaps`。 |
| `GpuInitError` | GPU | `src/gpu/context.rs:22` | 包装 GPU 初始化失败信息的错误类型。 |
| `GpuInitError`（`Display` 实现） | GPU | `src/gpu/context.rs:24` | 格式化错误信息。 |
| `try_init_gpu` | GPU | `src/gpu/context.rs:38` | 探测一个 wgpu 适配器/设备并返回一个 `GpuContext`；供 `compute::policy::select_backend` 使用。 |
| `GpuS2Pipeline` | GPU | `src/gpu/s2.rs:10` | 用于蒙特卡洛 S2 两点相关函数的 GPU 流水线状态。 |
| `build_triangle_buffer`（s2.rs） | GPU | `src/gpu/s2.rs:27` | 为 S2 蒙特卡洛流水线构建归一化的 `f32` 三角形位置缓冲区。 |
| `pack_params` | GPU | `src/gpu/s2.rs:48` | 将蒙特卡洛 S2 着色器参数打包为与 WGSL `Params` 布局匹配的字节缓冲区。 |
| `GpuS2Pipeline::new` | GPU | `src/gpu/s2.rs:95` | 初始化 wgpu 设备和蒙特卡洛 S2 计算流水线。 |
| `GpuS2Pipeline::update_mesh` | GPU | `src/gpu/s2.rs:266` | 为新网格重新上传三角形数据，而无需重建流水线。 |
| `GpuS2Pipeline::ensure_output_capacity` | GPU | `src/gpu/s2.rs:287` | 若调用次数超过当前容量，则扩容输出缓冲区。 |
| `GpuS2Pipeline::calculate_s2_gpu` | GPU | `src/gpu/s2.rs:311` | 为所有半径分派蒙特卡洛 S2 内核并回读结果。 |
| `OffsetEntry` | GPU | `src/gpu/s2_shell.rs:6` | 与 WGSL 布局相匹配的打包 `(radius_idx, dx, dy, dz)` 球壳偏移记录。 |
| `GpuShellS2Pipeline` | GPU | `src/gpu/s2_shell.rs:13` | 用于精确球壳配对 S2 计算的 GPU 流水线状态。 |
| `build_offset_buffer` | GPU | `src/gpu/s2_shell.rs:30` | 将 `(radius_idx, [dx,dy,dz])` 元组转换为 `OffsetEntry` 记录。 |
| `GpuShellS2Pipeline::new` | GPU | `src/gpu/s2_shell.rs:48` | 初始化 wgpu 设备和球壳 S2 计算流水线。 |
| `GpuShellS2Pipeline::compute_s2_shell` | GPU | `src/gpu/s2_shell.rs:135` | 在一个占用网格上分派精确的球壳配对计数并回读 S2(r)。 |
| `GpuVoxelPipeline` | GPU | `src/gpu/voxel.rs:5` | 用于网格体素化的 GPU 流水线状态。 |
| `build_triangle_buffer`（voxel.rs） | GPU | `src/gpu/voxel.rs:17` | 为体素化流水线构建归一化的 `f32` 三角形位置缓冲区（与 `s2.rs` 中的是独立副本）。 |
| `GpuVoxelPipeline::new` | GPU | `src/gpu/voxel.rs:37` | 初始化 wgpu 设备和体素化计算流水线。 |
| `GpuVoxelPipeline::voxelize` | GPU | `src/gpu/voxel.rs:116` | 分派光线投射体素化并回读占用网格。 |
| `GpuVolumeTransformPipeline` | GPU | `src/gpu/volume_transform.rs:5` | 用于体数据旋转裁剪的 GPU 流水线状态。 |
| `GpuVolumeTransformPipeline::new` | GPU | `src/gpu/volume_transform.rs:23` | 初始化 wgpu 设备和体数据变换计算流水线。 |
| `GpuVolumeTransformPipeline::rotate_and_crop` | GPU | `src/gpu/volume_transform.rs:145` | 分派旋转/裁剪/重采样内核并回读变换后的体数据。 |
| `parse_ascii_vertex` | I/O | `src/io/stl.rs:9` | 将一行 ASCII STL 的 `vertex x y z` 解析为一个 `Vec3`。 |
| `quantize_key` | I/O | `src/io/stl.rs:21` | 将一个顶点量化为固定精度的整数键，用于容差去重。 |
| `dedup_vertex` | I/O | `src/io/stl.rs:31` | 通过量化键查找，对某个顶点与已有列表进行去重比对。 |
| `parse_ascii_stl` | I/O | `src/io/stl.rs:48` | 将 ASCII STL 文本解析为一个顶点已去重的 `Mesh`。 |
| `parse_f32_le` | I/O | `src/io/stl.rs:93` | 解析小端序的 `f32` 字节并向上转换为 `f64`。 |
| `parse_binary_stl` | I/O | `src/io/stl.rs:104` | 将二进制 STL 字节解析为一个顶点已去重的 `Mesh`。 |
| `looks_ascii_stl` | I/O | `src/io/stl.rs:166` | 启发式地检测字节内容是否为 ASCII STL。 |
| `load_stl` | I/O | `src/io/stl.rs:185` | 加载一个 STL 文件，自动检测 ASCII/二进制格式。 |
| `load_folder_stls` | I/O | `src/io/stl.rs:203` | 加载一个文件夹中的所有 STL 文件。 |
| `load_stl_or_merge_folder` | I/O | `src/io/stl.rs:226` | 加载单个 STL 文件，或将目录中所有 STL 合并为一个网格。 |
| `save_stl` | I/O | `src/io/stl.rs:258` | 将一个网格保存为二进制 STL 文件。 |
| `collect_sorted_files` | I/O | `src/io/volume.rs:50` | 收集文件夹中的常规文件，按名称排序，可选按扩展名过滤。 |
| `resolve_slice_range` | I/O | `src/io/volume.rs:78` | 从起止索引解析出一个闭区间切片范围，将 `-1` 视为“从起始处”/“到结尾处”。 |
| `decode_raw_slice` | I/O | `src/io/volume.rs:107` | 根据位深度、符号和字节序，将一个原始图像切片解码为 `i64` 值。 |
| `load_raw_folder` | I/O | `src/io/volume.rs:205` | 从一个原始二进制切片文件夹加载 `Volume3D`。 |
| `tiff_decoding_to_i64` | I/O | `src/io/volume.rs:266` | 将一个 TIFF `DecodingResult` 转换为 `Vec<i64>` 缓冲区及其数值类型。 |
| `load_tiff_file_with_range` | I/O | `src/io/volume.rs:286` | 在一个闭区间页范围内，将多页 TIFF 文件加载为 `Volume3D`。 |
| `load_tiff_file` | I/O | `src/io/volume.rs:354` | 将一个 TIFF 文件（所有页）加载为 `Volume3D`。 |
| `is_tiff_path` | I/O | `src/io/volume.rs:359` | 检查一个路径是否具有 `.tif`/`.tiff` 扩展名。 |
| `load_tiff_or_folder` | I/O | `src/io/volume.rs:369` | 从一个文件或文件夹（所有页/切片）加载 TIFF 体数据。 |
| `load_tiff_or_folder_with_range` | I/O | `src/io/volume.rs:379` | 在一个闭区间切片范围内，从文件或文件夹加载 TIFF 体数据。 |
| `write_tiff_slice` | I/O | `src/io/volume.rs:446` | 将体数据的一个 z 切片写入 TIFF 编码器的一页中。 |
| `save_tiff_or_folder_with_ext` | I/O | `src/io/volume.rs:524` | 将 `Volume3D` 保存为一个多页 TIFF 文件或一个逐切片 TIFF 文件夹，并可配置扩展名。 |
| `save_tiff_or_folder` | I/O | `src/io/volume.rs:586` | 使用默认的 `.tiff` 扩展名，将 `Volume3D` 保存为 TIFF 文件或切片序列文件夹。 |
| `Pipeline::run`（trait） | Pipeline — Core | `src/pipeline/mod.rs:17` | 每个流水线结构体实现的 trait 方法，用于端到端执行。 |
| `create_progress_bar` | Pipeline — Core | `src/pipeline/mod.rs:25` | 用给定的模板和填充字符构建一个感知 tty 的 indicatif 进度条。 |
| `RotationMode`（枚举） | Pipeline — Core | `src/pipeline/rotation.rs:6` | 表示不旋转、固定轴或随机轴。 |
| `parse_rotation_mode` | Pipeline — Core | `src/pipeline/rotation.rs:18` | 将 `none/x/y/z/vector/any` 配置字符串解析为一个 `RotationMode`。 |
| `sample_rotation_axis` | Pipeline — Core | `src/pipeline/rotation.rs:57` | 为给定的 `RotationMode` 抽取一个具体的旋转轴向量。 |
| `ScalePipeline`（结构体） | Pipeline — Core | `src/pipeline/scale.rs:8` | 持有缩放流水线所需的 `ScaleConfig`。 |
| `ScalePipeline::run` | Pipeline — Core | `src/pipeline/scale.rs:19` | 加载一个 STL，应用单位换算/系数缩放，可选修正朝向，保存输出。 |
| `ForgePipeline`（结构体） | Pipeline — Core | `src/pipeline/forge.rs:13` | 持有自由变形锻造流水线所需的 `ForgingConfig`。 |
| `ForgePipeline::parse_roi_bbox` | Pipeline — Core | `src/pipeline/forge.rs:19` | 从配置中解析一个可选的 6 元素感兴趣区域包围盒。 |
| `ForgePipeline::parse_compression_axis` | Pipeline — Core | `src/pipeline/forge.rs:38` | 将压缩轴字符串（`x`/`y`/`z`）解析为一个索引和标签。 |
| `ForgePipeline::run` | Pipeline — Core | `src/pipeline/forge.rs:58` | 运行基于自由变形的压缩/锻造，跟踪感兴趣区域，写出锻造后的 STL 和一份文本报告。 |
| `MeasurePipeline`（结构体） | Pipeline — Core | `src/pipeline/measure.rs:16` | 持有 S2/体积分数测量流水线所需的 `MeasurementConfig`。 |
| `MeasurePipeline::parse_optional_bbox` | Pipeline — Core | `src/pipeline/measure.rs:22` | 从配置中解析一个可选的包围盒（3 元素尺寸或 6 元素 min/max）。 |
| `MeasurePipeline::l2_error` | Pipeline — Core | `src/pipeline/measure.rs:31` | 计算两个 S2 值向量在其公共前缀长度上的 L2 距离。 |
| `MeasurePipeline::run` | Pipeline — Core | `src/pipeline/measure.rs:53` | 加载一个 STL，计算体积分数和 S2 相关函数（精确法/蒙特卡洛法/both，CPU 或 GPU），写出一份报告。 |
| `CropPipeline`（结构体） | Pipeline — Crop & Split-Filter | `src/pipeline/crop.rs:14` | 持有裁剪流水线所需的 `CropConfig`。 |
| `InterpolationMode`（枚举） | Pipeline — Crop & Split-Filter | `src/pipeline/crop.rs:19` | 旋转+裁剪过程中使用的最近邻 vs. 三线性重采样模式。 |
| `parse_byte_order` | Pipeline — Crop & Split-Filter | `src/pipeline/crop.rs:25` | 将 `little`/`big`（或 `le`/`be`）解析为一个 `ByteOrder`。 |
| `parse_interpolation_mode` | Pipeline — Crop & Split-Filter | `src/pipeline/crop.rs:36` | 将 `nearest`/`trilinear` 解析为一个 `InterpolationMode`，默认使用三线性插值。 |
| `load_input_volume` | Pipeline — Crop & Split-Filter | `src/pipeline/crop.rs:52` | 按配置从原始文件夹或 TIFF/TIFF 文件夹加载输入 CT 体数据。 |
| `voxel_index` | Pipeline — Crop & Split-Filter | `src/pipeline/crop.rs:86` | 计算 `(x, y, z)` 体素坐标对应的扁平数据索引。 |
| `sample_voxel_or_background` | Pipeline — Crop & Split-Filter | `src/pipeline/crop.rs:91` | 在整数坐标处读取一个体素，若越界则返回背景值。 |
| `sample_nearest` | Pipeline — Crop & Split-Filter | `src/pipeline/crop.rs:106` | 在分数源坐标处进行最近邻采样。 |
| `sample_trilinear` | Pipeline — Crop & Split-Filter | `src/pipeline/crop.rs:114` | 在分数源坐标处进行三线性插值采样。 |
| `stabilize_bound` | Pipeline — Crop & Split-Filter | `src/pipeline/crop.rs:147` | 将接近整数的浮点数在给定误差范围内锁定为其确切整数值。 |
| `float_bounds_to_inclusive_i64` | Pipeline — Crop & Split-Filter | `src/pipeline/crop.rs:157` | 将浮点数的 min/max 边界转换为一个闭区间整数 `[start, end]` 范围。 |
| `boundary_non_bg_ratio` | Pipeline — Crop & Split-Filter | `src/pipeline/crop.rs:170` | 给定厚度的边界壳层内非背景体素所占的比例。 |
| `infer_trim_pixels` | Pipeline — Crop & Split-Filter | `src/pipeline/crop.rs:211` | 根据边界伪影强度启发式地推断 0/1/2 像素的边缘裁剪量。 |
| `resolve_trim_pixels` | Pipeline — Crop & Split-Filter | `src/pipeline/crop.rs:229` | 从配置中解析出有效的边缘裁剪像素数，支持 `-1` 表示自动。 |
| `trim_volume_border` | Pipeline — Crop & Split-Filter | `src/pipeline/crop.rs:252` | 从体数据的 XY 面裁去固定数量的边界体素。 |
| `detect_background_mode` | Pipeline — Crop & Split-Filter | `src/pipeline/crop.rs:296` | 将体数据边界上的众数体素值检测为背景值。 |
| `estimate_pca_bbox` | Pipeline — Crop & Split-Filter | `src/pipeline/crop.rs:330` | 计算 PCA 旋转、质心以及旋转坐标系下的前景包围盒。 |
| `rotate_and_crop` | Pipeline — Crop & Split-Filter | `src/pipeline/crop.rs:432` | CPU 上基于 rayon 并行的体数据旋转裁剪，输出为轴对齐结果。 |
| `rotate_and_crop_gpu` | Pipeline — Crop & Split-Filter | `src/pipeline/crop.rs:494` | 通过 `GpuVolumeTransformPipeline` 实现的 GPU 加速旋转裁剪（特性 `gpu`）。 |
| `CropPipeline::run` | Pipeline — Crop & Split-Filter | `src/pipeline/crop.rs:559` | 编排加载 → 背景检测 → PCA 包围盒 → 旋转+裁剪（GPU 或 CPU） → 边缘裁剪 → 保存 TIFF 的整个流程。 |
| `SplitFilterPipeline`（结构体） | Pipeline — Crop & Split-Filter | `src/pipeline/split_filter.rs:13` | 持有拆分过滤流水线所需的 `SplitFilterConfig`。 |
| `VolumeStats`（结构体） | Pipeline — Crop & Split-Filter | `src/pipeline/split_filter.rs:18` | 保留颗粒体积的最小/最大/均值/中位数摘要。 |
| `volume_stats_for_kept` | Pipeline — Crop & Split-Filter | `src/pipeline/split_filter.rs:26` | 对 `keep` 标志为 true 的颗粒计算 `VolumeStats`。 |
| `count_kept` | Pipeline — Crop & Split-Filter | `src/pipeline/split_filter.rs:54` | 统计一个 `keep` 布尔切片中 `true` 项的数量。 |
| `report_step` | Pipeline — Crop & Split-Filter | `src/pipeline/split_filter.rs:59` | 为某个过滤步骤追加一行前/后/移除数量的摘要。 |
| `append_volume_histogram` | Pipeline — Crop & Split-Filter | `src/pipeline/split_filter.rs:71` | 追加一份体积值的单一文本直方图。 |
| `append_volume_histogram_comparison` | Pipeline — Crop & Split-Filter | `src/pipeline/split_filter.rs:119` | 追加一份体积值前/后并排对比的文本直方图。 |
| `normal_cdf` | Pipeline — Crop & Split-Filter | `src/pipeline/split_filter.rs:202` | 标准正态分布 CDF，通过 `erf_approx` 计算。 |
| `erf_approx` | Pipeline — Crop & Split-Filter | `src/pipeline/split_filter.rs:208` | Abramowitz & Stegun 7.1.26 误差函数近似公式。 |
| `apply_lognormal_rebalance` | Pipeline — Crop & Split-Filter | `src/pipeline/split_filter.rs:225` | 相对于拟合的对数正态分布，从过度代表的对数体积分箱中剔除多余颗粒。 |
| `SplitFilterPipeline::run` | Pipeline — Crop & Split-Filter | `src/pipeline/split_filter.rs:300` | 编排拆分 → 长宽比/锐度/体积过滤 → 保存 STL → 生成报告的整个流程。 |
| `OptimizePipeline` | Pipeline — Optimize | `src/pipeline/optimize.rs:27` | 包装已解析的 `OptimizationConfig` 的流水线入口结构体。 |
| `ParticlePrepared` | Pipeline — Optimize | `src/pipeline/optimize.rs:32` | 每个颗粒的缓存：网格 + 预计算的包围盒 + parry3d 碰撞形状。 |
| `IslandResult` | Pipeline — Optimize | `src/pipeline/optimize.rs:39` | 一次模拟退火岛屿运行的结果：最优颗粒/损失/S2 以及各阶段耗时。 |
| `GlobalBest` | Pipeline — Optimize | `src/pipeline/optimize.rs:47` | 由 `Arc<Mutex<GlobalBest>>` 保护的跨岛屿共享最优解。 |
| `prepare_particle` | Pipeline — Optimize | `src/pipeline/optimize.rs:53` | 从一个原始网格构建一个 `ParticlePrepared`（包围盒 + parry3d 形状）。 |
| `format_s2_series` | Pipeline — Optimize | `src/pipeline/optimize.rs:60` | 将一个 S2 向量格式化为固定精度、以空格分隔的字符串。 |
| `push_history_s2` | Pipeline — Optimize | `src/pipeline/optimize.rs:69` | 向本次运行的历史日志追加一行带标签的 S2 快照。 |
| `prune_progress_message` | Pipeline — Optimize | `src/pipeline/optimize.rs:74` | 为剪枝阶段构建进度条消息字符串。 |
| `selective_prune_to_target_vf` | Pipeline — Optimize | `src/pipeline/optimize.rs:86` | 退火前阶段：迭代地移除颗粒以逼近目标体积分数，同时最小化 S2 损失的增加。 |
| `run_sa_island` | Pipeline — Optimize | `src/pipeline/optimize.rs:285` | 单个岛屿的核心模拟退火循环；是代码库中最重要的单个函数。 |
| `OptimizePipeline::run` | Pipeline — Optimize | `src/pipeline/optimize.rs:797` | 顶层 `Pipeline::run` 编排：加载、目标计算、剪枝、单/多岛屿模拟退火、保存。 |
| `TARGET_BIN_PROBES` | Pipeline — Packing | `src/pipeline/pack.rs:27` | 某个选定分箱在被排除出本轮重新选择之前，可容忍的最大连续放置失败次数。 |
| `PackPipeline` | Pipeline — Packing | `src/pipeline/pack.rs:29` | 包装一个 `PackingConfig` 的流水线结构体；实现了 `Pipeline`。 |
| `CandidateProposal` | Pipeline — Packing | `src/pipeline/pack.rs:34` | 一个抽取出的候选网格及其可选的预计算 `MeshMetrics`。 |
| `validate_sphericity_target` | Pipeline — Packing | `src/pipeline/pack.rs:44` | 在堆积开始前校验 `target_mean_sphericity`/`mean_sphericity_tolerance` 配置。 |
| `check_geometry_filters` | Pipeline — Packing | `src/pipeline/pack.rs:75` | 对候选网格应用已配置的 `min_volume`、`max_aspect_ratio`、`max_sharpness_ratio` 过滤条件。 |
| `PackPipeline::run` | Pipeline — Packing | `src/pipeline/pack.rs:124` | 核心的顺序随机堆积循环，可选带有目标粒径分布和平均球形度导向控制。 |
| `DiameterBin` | Pipeline — Packing | `src/pipeline/pack_targets.rs:8` | 带目标频次的半开（末端分箱为闭）粒径区间。 |
| `DiameterBin::midpoint` | Pipeline — Packing | `src/pipeline/pack_targets.rs:16` | 区间的算术中点。 |
| `TargetDistribution` | Pipeline — Packing | `src/pipeline/pack_targets.rs:22` | 已解析、归一化的目标粒径分布（有序、不重叠的分箱）。 |
| `DistributionState` | Pipeline — Packing | `src/pipeline/pack_targets.rs:27` | 针对某个 `TargetDistribution` 跟踪的可变的单次运行计数器。 |
| `BinChoiceKind` | Pipeline — Packing | `src/pipeline/pack_targets.rs:40` | 分箱选择的 `Natural`（自然）/ `Scaled`（缩放）/ `Fallback`（回退）分类。 |
| `BinChoice` | Pipeline — Packing | `src/pipeline/pack_targets.rs:47` | 所选的分箱索引及其 `BinChoiceKind`。 |
| `SphericityState` | Pipeline — Packing | `src/pipeline/pack_targets.rs:61` | 已接受球形度值的累计和/计数。 |
| `TargetDistribution::state` | Pipeline — Packing | `src/pipeline/pack_targets.rs:68` | 构建一个与分箱数量匹配的、清零的 `DistributionState`。 |
| `TargetDistribution::bin_for_diameter` | Pipeline — Packing | `src/pipeline/pack_targets.rs:81` | 将一个直径映射到其所属的分箱索引。 |
| `TargetDistribution::choose_bin` | Pipeline — Packing | `src/pipeline/pack_targets.rs:98` | 核心分配启发式算法：选取欠账最大的分箱，回退时选取最小全变差距离的分箱。 |
| `TargetDistribution::record_attempt` | Pipeline — Packing | `src/pipeline/pack_targets.rs:165` | 为某个分箱递增尝试计数器。 |
| `TargetDistribution::record_success` | Pipeline — Packing | `src/pipeline/pack_targets.rs:176` | 提交一次成功放置所对应的分箱、种类和缩放系数。 |
| `TargetDistribution::summarize` | Pipeline — Packing | `src/pipeline/pack_targets.rs:212` | 计算最大绝对误差、全变差距离，以及可达到的最佳舍入误差。 |
| `SphericityState::mean` | Pipeline — Packing | `src/pipeline/pack_targets.rs:259` | 已接受值的算术平均球形度。 |
| `SphericityState::projected_error` | Pipeline — Packing | `src/pipeline/pack_targets.rs:272` | 若接受该候选后，均值与目标区间之间的预计距离；用于对候选进行排序/导向。 |
| `SphericityState::record_success` | Pipeline — Packing | `src/pipeline/pack_targets.rs:298` | 将某个候选的球形度提交到运行中的均值统计中。 |
| `load_target_distribution_csv` | Pipeline — Packing | `src/pipeline/pack_targets.rs:309` | 解析并校验一个 `bin[,right],frequency` 格式的 CSV，得到一个 `TargetDistribution`。 |
| `write_distribution_comparison_csv` | Pipeline — Packing | `src/pipeline/pack_targets.rs:495` | 写出 `<output_stem>_diameter_distribution.csv` 目标-实际对比报告。 |
| `parse_csv_f64` | Pipeline — Packing | `src/pipeline/pack_targets.rs:564` | 解析一个必需的有限 CSV 浮点数单元格，附带行/列错误上下文。 |
| `parse_optional_csv_f64` | Pipeline — Packing | `src/pipeline/pack_targets.rs:587` | 解析一个可选的 CSV 浮点数单元格，空白表示“缺失”。 |

**总计：246 个已记录条目**（函数、方法、结构体、枚举和常量），分布在 11 份参考文档中，覆盖全部 45 个 `src/*.rs` 文件。

## 关于数量的说明

- 对 `src/` 中 `fn ` 声明的原始 `grep` 搜索找到 151 个函数。本索引列出了 246 个条目，是因为它还包含了已记录的结构体、枚举和常量（例如 `MeshMetrics`、`RotationMode`、`RAY_DIR_GPU`）及其关联方法——而不仅仅是函数。
- `#[cfg(test)] mod tests` 代码块内的仅测试用函数（例如 `src/compute/mod.rs` 中的 6 个单元测试）被有意排除在外——本索引仅覆盖生产代码。
- 两个 `main` 函数（`src/main.rs` 和 `src/bin/precision_test.rs`）分别列出，因为它们属于不同的二进制目标（`rustmspt` 和 `precision_test`）。
- 出现了两个 `build_triangle_buffer` 函数（`src/gpu/s2.rs` 和 `src/gpu/voxel.rs`）——这是不同模块中两个独立定义、同名的私有辅助函数，并非重复项。

## 参考文档

| 文档 | 覆盖范围 |
|---|---|
| [core-and-compute.md](core-and-compute.md) | `main.rs`、`lib.rs`、`error.rs`、`types.rs`、`bin/precision_test.rs`、`compute/*` |
| [config.md](config.md) | `config/*` —— YAML 配置结构体与反序列化辅助函数 |
| [geometry-core.md](geometry-core.md) | `geometry/{mod,bbox,mesh_ops,spatial}.rs` |
| [geometry-volume-collision.md](geometry-volume-collision.md) | `geometry/{volume,collision,forging}.rs` |
| [geometry-analysis.md](geometry-analysis.md) | `geometry/{metrics,s2}.rs` |
| [gpu.md](gpu.md) | `gpu/*`（特性门控） |
| [io.md](io.md) | `io/{stl,volume}.rs` |
| [pipeline-core.md](pipeline-core.md) | `pipeline/{mod,rotation,scale,forge,measure}.rs` |
| [pipeline-crop-and-splitfilter.md](pipeline-crop-and-splitfilter.md) | `pipeline/{crop,split_filter}.rs` |
| [pipeline-packing.md](pipeline-packing.md) | `pipeline/{pack,pack_targets}.rs` |
| [pipeline-optimize.md](pipeline-optimize.md) | `pipeline/optimize.rs` |

## 另请参阅

- [算法文档](../algorithms/) —— S2 相关函数、模拟退火、自由变形锻造、堆积目标分布、PCA 体数据对齐、空间网格碰撞以及网格裁剪/体积分数的概念性说明。
- [示例文档](../examples/) —— 附带真实捕获输出的各流水线走查。
- [AGENTS.md](../../../AGENTS.md) —— 这些文档所依据的 `AI-FUNC-SUMMARY` 注释约定，以及保持两者同步的要求。
