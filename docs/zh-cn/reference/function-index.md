# 函数索引

> **待翻译：** 新增 render 配置、相机、CPU/GPU 渲染、PNG I/O、`RenderedImage` 和 `RenderPipeline` 的完整索引见[英文函数索引](../../en-us/reference/function-index.md)。

`src/` 中每个已记录的函数、结构体、枚举和常量的主索引，编译自每份[参考文档](.)顶部的 `## Index` 表格。每一行都链接到该条目的完整说明。

| 条目 | 模块 | 源码位置 | 概述 |
|---|---|---|---|
| `PlacementParams` | Config | `src/config/placement.rs:22` | YAML 中书写的 `placement:` 块，尚未校验。 |
| `PlacementParams::validate` | Config | `src/config/placement.rs:584` | 施加所有跨字段规则并解析所有路径，得到 `ResolvedPlacement`。 |
| `ResolvedPlacement` | Config | `src/config/placement.rs:386` | 已校验的放置块：路径已解析，且不再留有可选项。 |
| `PackDocument` | Config | `src/config/placement.rs:469` | `pack` 配置选中的引擎：`Placement` 或 `Legacy`。 |
| `load_pack_document` | Config | `src/config/placement.rs:498` | 两遍解析的探针，判定配置选中的打包引擎。 |
| `resolve_against` | Config | `src/config/placement.rs:531` | 把配置中书写的路径按词法拼接到配置所在目录上。 |
| `config_dir` | Config | `src/config/placement.rs:547` | 配置中相对路径所依据的目录；没有父目录时为 `.`。 |
| `require_positive` | Config | `src/config/placement.rs:555` | 按字段名拒绝非有限或非正的数值。 |
| `require_non_negative` | Config | `src/config/placement.rs:565` | 按字段名拒绝非有限或为负的数值。 |
| `missing` (placement.rs) | Config | `src/config/placement.rs:902` | 为某个 kind 必需却缺失的分布参数构造错误。 |
| `default_threads` | Config | `src/config/placement.rs:47` | 线程设置的默认值 `-1`，表示使用全部可用核心。 |
| `default_vf_tolerance` | Config | `src/config/placement.rs:279` | 目标体积分数的默认相对容差。 |
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
| `cli_path_as_config_relative` | Core & Compute | `src/main.rs:157` | 改写命令行路径，使配置相对解析仍保持其原意。 |
| `BoundingBox::intersects_domain` | Core & Compute | `src/types.rs:96` | 两个包围盒是否相接；相切也算。 |
| `BuildIdentity` | Core & Compute | `src/version.rs:18` | 该二进制的身份：版本、git 提交、工作树是否有改动、启用的特性、构建平台。 |
| `build_identity` | Core & Compute | `src/version.rs:36` | 返回编译期写入的构建身份；工具身份的唯一来源。 |
| `BuildIdentity::version_detail` | Core & Compute | `src/version.rs:64` | 不含程序名的单行身份，供 clap 的 `--version` 使用。 |
| `BuildIdentity::version_line` | Core & Compute | `src/version.rs:92` | 含程序名的单行身份。 |
| `identity_json` | Core & Compute | `src/version.rs:103` | 将身份序列化为格式化 JSON，未确定的值写为 `null`。 |
| `non_empty` (version.rs) | Core & Compute | `src/version.rs:4` | 将构建脚本传入的空字符串映射为 `None`。 |
| `build_identity_version_line` | Core & Compute | `src/main.rs:146` | 泄漏版本详情为 `&'static str` 供 clap 使用。 |
| `git_output` | Core & Compute | `build.rs:11` | 构建期运行 git 命令，任何失败均返回 `None`。 |
| `rerun_if_exists` | Core & Compute | `build.rs:25` | 仅对存在的路径输出 `rerun-if-changed`。 |
| `emit_rerun_triggers` | Core & Compute | `build.rs:37` | 输出所有会改变所记录身份的重跑触发条件。 |
| `enabled_features` | Core & Compute | `build.rs:62` | 从 `CARGO_FEATURE_*` 读取已启用的 cargo 特性并排序。 |
| `main` (build.rs) | Core & Compute | `build.rs:80` | 将构建身份写入编译期环境变量。 |
| `Cli` | Core & Compute | `src/main.rs:26` | 顶层 clap CLI 结构体，包装一个 `Commands` 子命令。 |
| `Commands` | Core & Compute | `src/main.rs:32` | 7 个 CLI 子命令的枚举（Forge/Measure/Optimize/Pack/Scale/Crop/SplitFilter）。 |
| `default_config_path` | Core & Compute | `src/main.rs:151` | 构建 `data/input/` 下的默认配置路径。 |
| `pick_config_path` | Core & Compute | `src/main.rs:156` | 选择用户提供的配置路径，或回退到默认路径。 |
| `main`（main.rs） | Core & Compute | `src/main.rs:100` | CLI 入口点：解析参数、加载配置、应用覆盖项、运行所选流水线。 |
| `main`（precision_test.rs） | Core & Compute | `src/bin/precision_test.rs:5` | 独立的诊断二进制程序，比较 CPU 精确法、CPU 蒙特卡洛法与 GPU 蒙特卡洛法三种方法计算 S2 的精度/性能。 |
| `BoundingBox::expanded` | Core & Compute | `src/types.rs:88` | 按同一裕量在六个面上放大或收缩包围盒。 |
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
| `mesh_closedness` | Geometry — Analysis | `src/geometry/metrics.rs:37` | 判定网格是否为朝向一致的闭合流形；若否，说明原因。 |
| `mesh_is_closed` | Geometry — Analysis | `src/geometry/metrics.rs:23` | `mesh_closedness` 的布尔包装。 |
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
| `UnitQuat` | Geometry — Core | `src/geometry/quaternion.rs:17` | 标量在前的单位四元数 `[w, x, y, z]`，规范化为 `w >= 0`。 |
| `UnitQuat::identity` | Geometry — Core | `src/geometry/quaternion.rs:26` | 单位旋转。 |
| `UnitQuat::new` | Geometry — Core | `src/geometry/quaternion.rs:42` | 对原始分量做归一化与符号规范化。 |
| `UnitQuat::to_wxyz` | Geometry — Core | `src/geometry/quaternion.rs:58` | 按记录发布的顺序返回四个分量。 |
| `UnitQuat::norm` | Geometry — Core | `src/geometry/quaternion.rs:63` | 分量的欧氏范数，用于校验是否为单位四元数。 |
| `UnitQuat::rotate_point` | Geometry — Core | `src/geometry/quaternion.rs:75` | 绕原点旋转一个点。 |
| `UnitQuat::to_matrix` | Geometry — Core | `src/geometry/quaternion.rs:89` | 由四元数导出的行主序 3x3 旋转矩阵。 |
| `sample_uniform_quaternion` | Geometry — Core | `src/geometry/quaternion.rs:123` | Shoemake 方法：恰好三个均匀数给出 Haar 均匀旋转。 |
| `transform_shell` | Geometry — Core | `src/geometry/quaternion.rs:148` | 缩放、绕质心旋转、再平移这一变换的唯一定义。 |
| `icosphere_mesh` | Geometry — Core | `src/geometry/mesh_ops.rs:248` | 细分二十面体得到的闭合、外向球面网格。 |
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
| `VoidVolumeMethod` | Geometry — Volume & Collision | `src/geometry/void_index.rs:18` | 给出孔隙域内体积的计算方法。 |
| `VoidIndex` | Geometry — Volume & Collision | `src/geometry/void_index.rs:32` | 冻结孔隙，为放置运行的各类查询建立索引。 |
| `VoidIndex::build` | Geometry — Volume & Collision | `src/geometry/void_index.rs:56` | 校验孔隙网格并建立索引；朝向不一致时拒绝。 |
| `VoidIndex::bbox` | Geometry — Volume & Collision | `src/geometry/void_index.rs:118` | 孔隙的包围盒。 |
| `VoidIndex::shells` | Geometry — Volume & Collision | `src/geometry/void_index.rs:123` | 孔隙包含多少个闭合壳。 |
| `VoidIndex::is_outward` | Geometry — Volume & Collision | `src/geometry/void_index.rs:128` | 孔隙各壳是否朝外缠绕。 |
| `VoidIndex::total_volume` | Geometry — Volume & Collision | `src/geometry/void_index.rs:133` | 符号一致的各壳求和得到的孔隙总体积。 |
| `VoidIndex::contains_point` | Geometry — Volume & Collision | `src/geometry/void_index.rs:150` | 在层次结构上做射线奇偶判定；对嵌套壳同样正确。 |
| `VoidIndex::near_box` | Geometry — Volume & Collision | `src/geometry/void_index.rs:166` | 包围盒预筛：为假即远离孔隙且未被其嵌套。 |
| `VoidIndex::intersects` | Geometry — Volume & Collision | `src/geometry/void_index.rs:186` | 颗粒表面是否与孔面相交。 |
| `VoidIndex::min_distance_to` | Geometry — Volume & Collision | `src/geometry/void_index.rs:202` | 颗粒到孔隙的最小面到面距离。 |
| `VoidIndex::surface_distance` | Geometry — Volume & Collision | `src/geometry/void_index.rs:219` | 点到孔面的无符号距离。 |
| `VoidIndex::any_vertex_inside` | Geometry — Volume & Collision | `src/geometry/void_index.rs:233` | 网格是否有顶点落在孔隙内部。 |
| `VoidIndex::any_void_vertex_inside` | Geometry — Volume & Collision | `src/geometry/void_index.rs:245` | 孔隙是否有顶点落在颗粒内部。 |
| `VoidIndex::volume_in_domain` | Geometry — Volume & Collision | `src/geometry/void_index.rs:264` | 孔隙在域内的体积，以及所用的计算方法。 |
| `VoidIndex::sample_surface_point` | Geometry — Volume & Collision | `src/geometry/void_index.rs:288` | 按面积加权在孔面上取点，并给出外法向。 |
| `VoidIndex::overlap_volume` | Geometry — Volume & Collision | `src/geometry/void_index.rs:330` | 以域锚定的体素计数给出颗粒落在孔隙内的体积。 |
| `point_inside_mesh_local` | Geometry — Volume & Collision | `src/geometry/void_index.rs:375` | 对无层次结构的小网格做射线奇偶判定。 |
| `DOMAIN_FACE_NAMES` | Geometry — Volume & Collision | `src/geometry/volume.rs:454` | 六个域面名称，按裁剪平面顺序排列。 |
| `mesh_volume_centroid` | Geometry — Volume & Collision | `src/geometry/volume.rs:467` | 闭合网格的体积质心（不是顶点均值）。 |
| `shell_signed_volumes` | Geometry — Volume & Collision | `src/geometry/volume.rs:498` | 逐壳有符号体积，用于暴露各壳的朝向。 |
| `plane_signed_distance` | Geometry — Volume & Collision | `src/geometry/volume.rs:506` | 点到平面的有符号距离。 |
| `clip_tagged_polygon` | Geometry — Volume & Collision | `src/geometry/volume.rs:520` | 带标记的 Sutherland-Hodgman 裁剪，标出裁剪新建的边。 |
| `mesh_volume_in_bbox_exact` | Geometry — Volume & Collision | `src/geometry/volume.rs:574` | 逐平面封盖，给出精确的域内体积与被切的面。 |
| `cut_face_names` | Geometry — Volume & Collision | `src/geometry/volume.rs:689` | 给出真正被裁剪切到的域面名称。 |
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
| `to_parry_trimesh` | Geometry — Volume & Collision | `src/geometry/collision.rs:29` | 将一个 `Mesh` 转换为 parry3d 的 `TriMesh`。 |
| `trimesh_contains_point` | Geometry — Volume & Collision | `src/geometry/collision.rs:61` | 借助形状的层次包围体，以射线奇偶判定点是否位于实体内部。 |
| `mesh_surfaces_intersect_prepared` | Geometry — Volume & Collision | `src/geometry/collision.rs:99` | 给定预先构建的包围盒/形状，精确判定两个网格表面是否相交。 |
| `mesh_solids_nested_prepared` | Geometry — Volume & Collision | `src/geometry/collision.rs:150` | 判定两个闭合实体中是否有一个整体位于另一个内部。 |
| `mesh_collision_exact_prepared` | Geometry — Volume & Collision | `src/geometry/collision.rs:196` | 判定两个网格实体是否重叠：表面相交，或一个包含另一个。 |
| `mesh_distance_exact_prepared` | Geometry — Volume & Collision | `src/geometry/collision.rs:214` | 在给定预先构建的包围盒/形状的情况下，进行经包围盒过滤的精确距离查询。 |
| `mesh_collision_exact` | Geometry — Volume & Collision | `src/geometry/collision.rs:259` | 便捷封装：构建包围盒/形状后测试碰撞。 |
| `mesh_distance_exact` | Geometry — Volume & Collision | `src/geometry/collision.rs:268` | 便捷封装：构建包围盒/形状后计算距离。 |
| `generate_periodic_ghosts` | Geometry — Volume & Collision | `src/geometry/collision.rs:282` | 为周期边界碰撞生成一个网格经平移的镜像副本。 |
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
| `sha256_bytes` | I/O | `src/io/hash.rs:13` | 字节切片的 SHA-256，返回小写十六进制。 |
| `sha256_file` | I/O | `src/io/hash.rs:25` | 流式计算文件的 SHA-256，返回十六进制摘要与字节数。 |
| `hex_digest` | I/O | `src/io/hash.rs:42` | 将摘要渲染为小写十六进制。 |
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
| `PlacementPipeline` | Pipeline — Core | `src/pipeline/placement.rs:38` | 持有已校验 `ResolvedPlacement` 的流水线结构体。 |
| `seeded_rng` | Pipeline — Core | `src/pipeline/rng.rs:13` | 构建带种子运行所使用的 ChaCha12 随机流。 |
| `u01` | Pipeline — Core | `src/pipeline/rng.rs:32` | 唯一的均匀分布原语：一次抽取，落在 `[0, 1)`。 |
| `uniform_range` | Pipeline — Core | `src/pipeline/rng.rs:45` | 在 `[lo, hi)` 上抽取一个均匀值；退化区间返回 `lo`。 |
| `uniform_index` | Pipeline — Core | `src/pipeline/rng.rs:62` | 从 `0..n` 中均匀抽取一个下标。 |
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
| `PHASE_MATRIX` | Pipeline — Packing | `src/pipeline/placement_labels.rs:12` | 标签场中相编码 0。 |
| `VoxelLabelsHeader` | Pipeline — Packing | `src/pipeline/placement_labels.rs:21` | 标签体数据的说明：间距、原点、排布与相表。 |
| `PhaseLabel` | Pipeline — Packing | `src/pipeline/placement_labels.rs:38` | 一个相编码及其名称。 |
| `write_voxel_labels` | Pipeline — Packing | `src/pipeline/placement_labels.rs:56` | 写出三相标签场与逐体素颗粒标识场。 |
| `particle_at` | Pipeline — Packing | `src/pipeline/placement_labels.rs:178` | 查找包含某点的已放置颗粒。 |
| `point_in_particle` | Pipeline — Packing | `src/pipeline/placement_labels.rs:189` | 对单个颗粒网格做射线奇偶包含判定。 |
| `VoidReport` | Pipeline — Packing | `src/pipeline/placement_outputs.rs:278` | 运行如何处理冻结孔隙，以及如何度量它。 |
| `build_void_report` | Pipeline — Packing | `src/pipeline/placement.rs:1156` | 为报告描述冻结孔隙，含其体积计算方法。 |
| `PlacementPipeline` | Pipeline — Packing | `src/pipeline/placement.rs:38` | 持有已校验 `ResolvedPlacement` 的流水线结构体。 |
| `PlacementOutcome` | Pipeline — Packing | `src/pipeline/placement.rs:61` | 一次完整运行的产出，供进程内调用方使用。 |
| `run_placement` | Pipeline — Packing | `src/pipeline/placement.rs:76` | 端到端运行引擎并写出全部输出文件。 |
| `resolve_threads` | Pipeline — Packing | `src/pipeline/placement.rs:253` | 把线程设置换算为至少为 1 的工作线程数。 |
| `EngineState` | Pipeline — Packing | `src/pipeline/placement.rs:265` | 放置循环累积的全部状态。 |
| `place_all` | Pipeline — Packing | `src/pipeline/placement.rs:347` | 按顺序尝试每个已规划尺寸，接受放得下的。 |
| `try_place_one` | Pipeline — Packing | `src/pipeline/placement.rs:386` | 在单颗粒尝试预算内尝试放置一个尺寸。 |
| `accept` | Pipeline — Packing | `src/pipeline/placement.rs:544` | 把已接受的候选提交进几何与记录。 |
| `run_top_up` | Pipeline — Packing | `src/pipeline/placement.rs:605` | 仅因裁剪而未达标时补抽新批次。 |
| `decide_stop` | Pipeline — Packing | `src/pipeline/placement.rs:788` | 判定运行以四种停止原因中的哪一种结束。 |
| `write_outputs` | Pipeline — Packing | `src/pipeline/placement.rs:788` | 写出几何、逐颗粒记录与尺寸 CSV。 |
| `entity_id` | Pipeline — Packing | `src/pipeline/placement.rs:933` | 已放置颗粒的稳定标识。 |
| `particle_record` | Pipeline — Packing | `src/pipeline/placement.rs:938` | 把一个已放置颗粒转为其记录条目。 |
| `size_class_rows` | Pipeline — Packing | `src/pipeline/placement.rs:986` | 构造逐分组的目标与实际对照行。 |
| `blank_report` | Pipeline — Packing | `src/pipeline/placement.rs:1012` | 放置开始之前的报告初始形态。 |
| `describe_input` | Pipeline — Packing | `src/pipeline/placement.rs:1191` | 为报告描述输入文件及其摘要。 |
| `finish_report` | Pipeline — Packing | `src/pipeline/placement.rs:1210` | 填入运行结束后已知的全部内容。 |
| `summary` (placement.rs) | Pipeline — Packing | `src/pipeline/placement.rs:1274` | 构造供人阅读的 stdout 摘要。 |
| `read_record` | Pipeline — Packing | `src/pipeline/placement.rs:1335` | 读回已写出的逐颗粒记录。 |
| `read_report` | Pipeline — Packing | `src/pipeline/placement.rs:1343` | 读回已写出的运行报告。 |
| `RejectReason` | Pipeline — Packing | `src/pipeline/placement_feasibility.rs:17` | 候选放置未被接受的原因；即报告中的键。 |
| `RejectReason::as_str` | Pipeline — Packing | `src/pipeline/placement_feasibility.rs:44` | 拒绝原因在报告中的稳定键名。 |
| `PlacedParticle` | Pipeline — Packing | `src/pipeline/placement_feasibility.rs:76` | 通过全部检查的颗粒，附带缓存的形状。 |
| `PlacedParticle::volume_in_domain_solid` | Pipeline — Packing | `src/pipeline/placement_feasibility.rs:106` | 计入固相的颗粒体积。 |
| `FeasibilityContext` | Pipeline — Packing | `src/pipeline/placement_feasibility.rs:112` | 可行性检查所读取的全部内容。 |
| `Candidate` | Pipeline — Packing | `src/pipeline/placement_feasibility.rs:132` | 候选放置，附带已预先算好的廉价量。 |
| `Accepted` | Pipeline — Packing | `src/pipeline/placement_feasibility.rs:146` | 通过检查过程中顺带算出的结果。 |
| `check_placement` | Pipeline — Packing | `src/pipeline/placement_feasibility.rs:171` | 按序运行全部可行性规则，返回拦下它的那一条。 |
| `retained_depth` | Pipeline — Packing | `src/pipeline/placement_feasibility.rs:382` | 跨界颗粒仍伸入域内的深度。 |
| `ToolRecord` | Pipeline — Packing | `src/pipeline/placement_outputs.rs:15` | 记录与报告中出现的构建身份。 |
| `StopReason` | Pipeline — Packing | `src/pipeline/placement_outputs.rs:53` | 运行可用的四词固定停止原因词表。 |
| `ParticleRecord` | Pipeline — Packing | `src/pipeline/placement_outputs.rs:133` | 记录文件中单个已放置颗粒的条目。 |
| `RecordFile` | Pipeline — Packing | `src/pipeline/placement_outputs.rs:152` | 逐颗粒记录文件的顶层结构。 |
| `conventions` | Pipeline — Packing | `src/pipeline/placement_outputs.rs:173` | 写明读者复原颗粒所需的全部约定。 |
| `ReportFile` | Pipeline — Packing | `src/pipeline/placement_outputs.rs:246` | 运行报告文件的顶层结构。 |
| `SizeClassRow` | Pipeline — Packing | `src/pipeline/placement_outputs.rs:226` | 单个分组的目标、抽取、放置、缺口与补抽计数。 |
| `write_json` | Pipeline — Packing | `src/pipeline/placement_outputs.rs:380` | 把 JSON 值写入磁盘并创建父目录。 |
| `describe_output` | Pipeline — Packing | `src/pipeline/placement_outputs.rs:400` | 为报告清单描述已写出的输出文件。 |
| `write_size_distribution_csv` | Pipeline — Packing | `src/pipeline/placement_outputs.rs:424` | 写出逐尺寸分组的对照 CSV。 |
| `inverse_normal_cdf` | Pipeline — Packing | `src/pipeline/placement_sizes.rs:21` | Wichura AS241 标准正态分位数，误差低于 1e-15。 |
| `poly` (placement_sizes.rs) | Pipeline — Packing | `src/pipeline/placement_sizes.rs:133` | 对最高次在前的系数做 Horner 求值。 |
| `normal_cdf` (placement_sizes.rs) | Pipeline — Packing | `src/pipeline/placement_sizes.rs:142` | 经由互补误差函数计算标准正态 CDF。 |
| `erfc` | Pipeline — Packing | `src/pipeline/placement_sizes.rs:153` | 互补误差函数，用于把截断边界换算成概率。 |
| `SizeDraw` | Pipeline — Packing | `src/pipeline/placement_sizes.rs:175` | 一次抽取的直径，附带其统计分组与抽取序号。 |
| `SizeClass` | Pipeline — Packing | `src/pipeline/placement_sizes.rs:185` | 一个直径区间及其应占的颗粒份额。 |
| `SizeSource` | Pipeline — Packing | `src/pipeline/placement_sizes.rs:192` | 已就绪的目标数量分布：截断对数正态或直方图。 |
| `SizeSource::prepare` | Pipeline — Packing | `src/pipeline/placement_sizes.rs:218` | 准备分布源：读取直方图 CSV 并预先算好截断概率。 |
| `SizeSource::sample` | Pipeline — Packing | `src/pipeline/placement_sizes.rs:272` | 抽取一个直径，恰好消耗一个 u64。 |
| `SizeSource::support` | Pipeline — Packing | `src/pipeline/placement_sizes.rs:309` | 该分布源能产生的最小与最大直径。 |
| `SizeSource::mass_between` | Pipeline — Packing | `src/pipeline/placement_sizes.rs:381` | 目标分布落在两个直径之间的份额。 |
| `build_classes` | Pipeline — Packing | `src/pipeline/placement_sizes.rs:329` | 构造目标与实际对照所用的统计分组。 |
| `build_equal_width` | Pipeline — Packing | `src/pipeline/placement_sizes.rs:355` | 把分布支撑集切成等宽分组并给出各自份额。 |
| `class_for_diameter` | Pipeline — Packing | `src/pipeline/placement_sizes.rs:430` | 查找直径所属分组；最上端边界为闭区间。 |
| `SizePlan` | Pipeline — Packing | `src/pipeline/placement_sizes.rs:449` | 运行打算放置的尺寸集合，在任何放置之前抽定。 |
| `plan_size_multiset` | Pipeline — Packing | `src/pipeline/placement_sizes.rs:473` | 抽取整个尺寸集合，停在离目标更近的那个数量上。 |
| `order_for_placement` | Pipeline — Packing | `src/pipeline/placement_sizes.rs:534` | 把尺寸集合按从大到小排序，或还原为抽取顺序。 |
| `ShapeShell` | Pipeline — Packing | `src/pipeline/placement_library.rs:13` | 一个闭合壳：已度量、已居中、已计算摘要。 |
| `ShapeSource` | Pipeline — Packing | `src/pipeline/placement_library.rs:41` | 构成形状库的源文件，附带摘要与壳数统计。 |
| `RejectedShell` | Pipeline — Packing | `src/pipeline/placement_library.rs:53` | 读入但未保留的壳，以及未保留的原因。 |
| `ShapeLibrary` | Pipeline — Packing | `src/pipeline/placement_library.rs:61` | 运行可抽取的全部形状，以及读入但未保留的部分。 |
| `load_shape_library` | Pipeline — Packing | `src/pipeline/placement_library.rs:85` | 加载、拆分、度量并过滤形状文件。 |
| `filter_reason` | Pipeline — Packing | `src/pipeline/placement_library.rs:236` | 指出某个壳未通过哪条形状库过滤规则。 |
| `shell_geometry_sha256` | Pipeline — Packing | `src/pipeline/placement_library.rs:276` | 对壳的几何计算摘要，使文件重排可被察觉。 |
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
| `orient2d_3d` | 网格工具 | `src/meshgen/predicates.rs:28` | 最佳条件 2D 投影下的符号精确三角形定向；用于 S0 退化检查。 |
| `ProjectionAxis` / `best_projection_axis` | 网格工具 | `src/meshgen/predicates.rs:54/61` | 最佳条件平面投影所丢弃的轴及其选择器。 |
| `project_to_2d` / `orient2d_axis` / 值与 DD 辅助函数 | 网格工具 | `src/meshgen/predicates.rs:74/83/92/178` | 按指定轴投影并计算精确、f64 permanent 与 DD `orient2d` 值。 |
| `two_sum` / `two_prod` | 网格工具 | `src/meshgen/predicates.rs:111/118` | DD 算术使用的冻结无误差 f64 和/积原语。 |
| `DoubleDouble` 及算术方法 | 网格工具 | `src/meshgen/predicates.rs:125-172` | 仅加/减/乘的 DD 值；精确提升、取负、折叠与零测试。 |
| `DeterminantRatio` 及排序方法 | 网格工具 | `src/meshgen/predicates.rs:192-224` | 用于源边排序的规范 DD 分子/分母比值。 |
| `PrecisionTier` / `EdgeTriPoint` / `CoplanarSegmentPoint` / `ConstructionOutcome` | 网格工具 | `src/meshgen/predicates.rs:229/236/244/254` | 构造精度溯源、C1/C3 结果与已解析/延迟结果枚举。 |
| `orient3d_value_permanent` | 网格工具 | `src/meshgen/predicates.rs:266` | Shewchuk 顺序 f64 行列式及匹配 permanent。 |
| `orient3d_filtered` | 网格工具 | `src/meshgen/predicates.rs:286` | 带精确 robust 回退的认证静态过滤符号。 |
| `orient3d_dd_value` | 网格工具 | `src/meshgen/predicates.rs:296` | 项目符号约定下的 DD orient3d 行列式。 |
| `construct_edge_triangle_intersection` | 网格工具 | `src/meshgen/predicates.rs:320` | 冻结 C1 行列式比值构造，含 f64/DD 升级与 DD 下限延迟。 |
| `construct_coplanar_segment_intersection` | 网格工具 | `src/meshgen/predicates.rs:384` | 冻结 C3 仿射构造；检查并保留两条定义边的 DD 排序比值。 |
| `construct_three_triangle_intersection` | 网格工具 | `src/meshgen/predicates.rs:492` | 冻结局部坐标 C2 Cramer 构造，升级时以 DD 完整重建。 |
| `orient3d` | 网格工具 | `src/meshgen/predicates.rs:698` | 符号精确四面体定向；全项目唯一对 robust 相反约定取负之处。 |
| `tet_signed_volume` / `orient3d_sign_test` | 网格工具 | `src/meshgen/predicates.rs:708/713` | 有符号四面体体积与单位四面体约定夹具。 |
| `TetQuality` / `tet_quality` | 网格工具 | `src/meshgen/predicates.rs:729/748` | [V4] 体积、纵横比/半径比、二面角、缩放雅可比与高度指标。 |
| `node_key` | 网格工具 | `src/meshgen/predicates.rs:867` | 量化整数节点键；键相同代表一个网格尺度节点。 |
| `RepairActionType` | 网格工具 | `src/meshgen/surface.rs:18` | S0 修复动作类型（焊接、退化丢弃、重复合并、针孔、定向、补洞）。 |
| `RepairAction` / `RepairLog` | 网格工具 | `src/meshgen/surface.rs:43/55` | 结构化修复记录与有序 [V12] 回显日志。 |
| `ConditionedSurface` / `ConditionStats` | 网格工具 | `src/meshgen/surface.rs:88/116` | 带持久源 ID、临时构件、修复日志与计数的 S0 几何。 |
| `SurfaceComponent` | 网格工具 | `src/meshgen/surface.rs:99` | 共享 schema-v1 构件元数据行（X、Y、类型、闭合）。 |
| `condition_surface` | 网格工具 | `src/meshgen/surface.rs:135` | S0 焊接/去重/修复；保留跨输入重合身份及修复归属。 |
| `condition_surface_to_doc` / `condition_surface_to_doc_with_components` | 网格工具 | `src/meshgen/surface.rs:301/307` | 以推断或给定构件元数据构建完整 schema-v1 s00 文档。 |
| `default_surface_components` / `source_component_is_closed` | 网格工具 | `src/meshgen/surface.rs:315/332` | 推断确定性元数据并测试精确组合闭合性。 |
| `surface_stage_to_doc` | 网格工具 | `src/meshgen/surface.rs:351` | s00-s03 面/曲线文档的共享全必备数组 schema-v1 构建器。 |
| `FeatureEdgeKind` / `FeatureCurve` / `FeatureSet` | 网格工具 | `src/meshgen/features.rs:19/27/40` | 按构件隔离的 S1 边类型、链接特征折线、交汇点与角点。 |
| `detect_features` | 网格工具 | `src/meshgen/features.rs:54` | 确定性按构件检测并链接锐边/边缘/非流形边。 |
| `features_to_doc` / `features_to_doc_with_components` | 网格工具 | `src/meshgen/features.rs:290/296` | 以推断或给定构件元数据构建完整 schema-v1 s01 文档。 |
| `TriId` / `EdgeId::new` / `IsectProv` / `SegKey::new` | 网格工具 | `src/meshgen/arrange.rs:25/29-33/44/52-61` | 稳定三角形 ID 与规范 EdgeTri/EdgeEdge/TriTriTri/线段溯源键。 |
| `CoincidenceCase` 及策略方法 / `CoincidenceEntity` / `CoincidenceEvent` | 网格工具 | `src/meshgen/arrange.rs:73/88/97/105/112` | 冻结 C1-C10 分类、精确拒绝/警告语义与排序参与者。 |
| `DegradedReason` / `DegradedNeighborhood` / `ArrangedPointFeature` | 网格工具 | `src/meshgen/arrange.rs:120/130/140` | 类型化持久回退记录与焊接 C5/C6 点特征。 |
| `ArrangeComponent` / `ArrangeOptions` 及构造方法 | 网格工具 | `src/meshgen/arrange.rs:69/149/159/175` | 共享构件元数据及 G2-1..G2-3 区域、epsilon、重合策略选项。 |
| `RegistryVertex` / `RegistrySegment` / `IntersectionRegistry` | 网格工具 | `src/meshgen/arrange.rs:183/195/204` | 符号优先注册实体；提交顶点保留全部兼容溯源别名。 |
| `ArrangedCurveKind` / `ArrangedCurve` | 网格工具 | `src/meshgen/arrange.rs:211/219` | 带构件关联和循环子面顺序的锐边/边缘/相交曲线。 |
| `ArrangedFace` / `ArrangementStats` / `ArrangedSurface` | 网格工具 | `src/meshgen/arrange.rs:228/240/256` | 原子多源/多标签面、接触、事件、警告、降级路由与诊断。 |
| `arrange_surface` | 网格工具 | `src/meshgen/arrange.rs:426` | 确定性 CPU G2-1..G2-3 注册/CDT/覆盖/策略/回退/校验路径。 |
| `arranged_surface_to_doc` | 网格工具 | `src/meshgen/arrange.rs:761` | 含集合标签与 FaceTagOrientation 的诊断 schema-v1 编码；尚非 live s02。 |
| `triangulate_parent` | 网格工具 | `src/meshgen/arrange.rs:3385` | 受限预注册 Spade CDT，传播插入/约束错误并校验铺满。 |
| `clip_arranged_to_box` | 网格工具 | `src/meshgen/arrange.rs:4634` | G2-5b 盒裁剪：对 6 个域平面进行 Sutherland-Hodgman；实体封顶（box 标签）、片体开放、曲线裁剪。 |
| `ComponentClassification` / `ClosureDefect` / `RebuiltTopology` | 网格工具 | `src/meshgen/topo.rs:13/21/31` | G2-4 裁剪后拓扑重建结果类型：构件分类、缺陷报告与完整重建输出。 |
| `rebuild_topology` | 网格工具 | `src/meshgen/topo.rs:43` | 从裁剪后排布面重新推导构件、闭合状态与 GWN 回退。 |
| `generalized_winding_number` / `gwn_margin_band` | 网格工具 | `src/meshgen/topo.rs:233/255` | 查询点处的 GWN（两两归约、S 累积）与 f32 边际带证书。 |
| `Severity` | 网格工具 | `src/meshgen/verify.rs:21` | 验证发现项严重级别（Info < Warn < Fail）。 |
| `CheckStatus` | 网格工具 | `src/meshgen/verify.rs:40` | 单节结论 PASS/WARN/FAIL/SKIPPED。 |
| `VerifyItem` | 网格工具 | `src/meshgen/verify.rs:63` | 单条发现：稳定代码、消息、点/单元编号与坐标。 |
| `VerifySection` | 网格工具 | `src/meshgen/verify.rs:88` | 单个目录条目：状态、指标、限长条目列表。 |
| `VerifyGates` | 网格工具 | `src/meshgen/verify.rs:145` | 目录的可配置、与尺度无关的门限。 |
| `VerifyReport` | 网格工具 | `src/meshgen/verify.rs:179` | 完整验证结果；passed、exit_code、fired_codes、section。 |
| `verify` | 网格工具 | `src/meshgen/verify.rs:430` | 对契约 VTU 或外部 VTU 运行检查目录。 |
| `report_to_json` | 网格工具 | `src/meshgen/verify.rs:1510` | 序列化冻结的 JSON 报告（手写，无 JSON 依赖）。 |
| `report_to_log` | 网格工具 | `src/meshgen/verify.rs:1619` | 分节人读报告，每项检查一行状态。 |
| `annotate` | 网格工具 | `src/meshgen/verify.rs:1681` | 附带质量数组与 verify_flags 位掩码的网格副本。 |
| `VerifyGateParams` | 网格工具 | `src/config/mesh_verify.rs:8` | 验证目录的 YAML 门限覆盖。 |
| `MeshVerifyParams` | 网格工具 | `src/config/mesh_verify.rs:36` | mesh_verify: YAML 块（输入、report/json/annotate、门限）。 |
| `MeshVerifySurface` | 网格工具 | `src/config/mesh_verify.rs` | 单个 [V5] 输入曲面：裸路径，或在网格以显式优先级生成时使用 `{stl, priority}`。 |
| `MeshVerifySurface::resolved_priority` | 网格工具 | `src/config/mesh_verify.rs` | [V5] 曲面的有效优先级：显式取值，否则为 0——必须与生成该网格时 `meshgen.inputs` 的优先级一致。 |
| `MeshVerifyConfig` | 网格工具 | `src/config/mesh_verify.rs:50` | mesh-verify 子命令的顶层 YAML 文档。 |
| `gates_from_config` | 网格工具 | `src/pipeline/mesh_verify.rs:18` | 将 YAML 覆盖项叠加到契约默认门限。 |
| `verify_file` | 网格工具 | `src/pipeline/mesh_verify.rs:42` | 加载、校验、验证并写出日志/JSON/带注解 VTU。 |
| `MeshVerifyPipeline::run` | 网格工具 | `src/pipeline/mesh_verify.rs:9` | mesh-verify 子命令；门限不通过时退出码非零。 |
| `InputKind` | 网格工具 | `src/config/meshgen.rs:10` | 单个输入的面角色覆盖：auto / solid / sheet。 |
| `RepairLevel` | 网格工具 | `src/config/meshgen.rs:20` | S0 修复激进度：strict / conservative / permissive。 |
| `CoincidencePolicy` | 网格工具 | `src/config/meshgen.rs:30` | G2-2 重合面策略：merge / reject / warn。 |
| `FemProfile` | 网格工具 | `src/config/meshgen.rs:40` | 控制薄层/薄片处理的目标求解器配置：implicit / explicit / none。 |
| `DeterminismMode` | 网格工具 | `src/config/meshgen.rs:50` | 运行可复现性契约：strict（按位）/ fast。 |
| `UnmappedPolicy` | 网格工具 | `src/config/meshgen.rs:59` | INP 导出对未映射区域的行为：error / elset-only。 |
| `SnapshotMode` | 网格工具 | `src/config/meshgen.rs:68` | 契约快照产出级别：none / key / all。 |
| `MeshGenInput` | 网格工具 | `src/config/meshgen.rs:80` | 单个 STL 输入：stl 路径、可选 priority、kind。 |
| `MeshGenDomain` | 网格工具 | `src/config/meshgen.rs:90` | 轴对齐生成区域（min/max，3 分量，min < max）。 |
| `MeshGenSizing` | 网格工具 | `src/config/meshgen.rs:99` | 作为包围盒对角线分数的尺寸场上下限。 |
| `MeshGenGaps` | 网格工具 | `src/config/meshgen.rs:114` | 间隙场厚度因子与置信度下限。 |
| `MeshGenEnvelope` | 网格工具 | `src/config/meshgen.rs:125` | 作为包围盒对角线分数的数值包络厚度。 |
| `MeshGenRepair` | 网格工具 | `src/config/meshgen.rs:132` | S0 修复配置（level）。 |
| `MeshGenMaterials` | 网格工具 | `src/config/meshgen.rs:143` | INP 导出的材料分配；by_component 保留重复键。 |
| `MeshGenOutput` | 网格工具 | `src/config/meshgen.rs:154` | 输出目的地：必填 vtu，可选 abaqus/report。 |
| `MeshGenParams` | 网格工具 | `src/config/meshgen.rs:168` | meshgen: YAML 块；加载后须调用 validate()。 |
| `MeshGenConfig` | 网格工具 | `src/config/meshgen.rs:198` | 顶层 YAML 包装（meshgen:）。 |
| `MeshGenInput::resolved_priority` | 网格工具 | `src/config/meshgen.rs:204` | 有效优先级（显式覆盖或文件索引）。 |
| `MeshGenParams::validate` | 网格工具 | `src/config/meshgen.rs:218` | 强制 PLAN §6.3 解析期拒绝；Ok 或 InvalidConfig。 |
| `deserialize_component_map` | 网格工具 | `src/config/meshgen.rs:352` | 将 by_component 反序列化为保留重复键的有序对列表。 |
| `MeshGenPipeline::run` | 网格工具 | `src/pipeline/meshgen.rs:158` | S0 前归一化，运行 S0/S1/G2-1..G2-3，扣留部分 s02，并对 G2-4/G2-5 及后续阶段返回 NotAvailable。 |
| `SampleKind` / `PairClass` | 网格生成 | `src/meshgen/gapfield.rs:48/57` | S3 采样来源与冻结的配对类别（intra / inter / solid-sheet / sheet-sheet / surface-box）。 |
| `GapPairing` / `GapSample` / `GapSample::passes_battery` | 网格生成 | `src/meshgen/gapfield.rs:68/81/100` | 单条 S3 对应关系、单个采样（侧、方向、t_raw/t/t_exact、校验位）与"全部适用检查通过"判据。 |
| `GapGroup` / `GapFieldStats` / `GapField` | 网格生成 | `src/meshgen/gapfield.rs:110/122/137` | 带置信度与 t_r 的临时（构件, 侧, 对侧面片）分组、S3 计数器与整体分离场。 |
| `GapFieldOptions` | 网格生成 | `src/meshgen/gapfield.rs:152` | S3 输入：区域、epsilon、引导 h、间隙因子、置信度下限、虚拟壁、加密、平滑。 |
| `FLAG_MUTUAL` / `FLAG_OPPOSITE_PATCH` / `FLAG_CONTINUITY` / `FLAG_NO_CROSSING` / `FLAG_ORIENTATION` / `FLAGS_ALL` | 网格生成 | `src/meshgen/gapfield.rs:28-38` | 五项配对校验位及其并集。 |
| `compute_gap_field` | 网格生成 | `src/meshgen/gapfield.rs:345` | 在裁剪且拓扑重建后的排布面上运行 S3（射线 + 最近点对扫掠 + 校验组 + 置信度）。 |
| `gapfield_to_doc` | 网格生成 | `src/meshgen/gapfield.rs:1655` | 构建 s03_gapfield 文档：排布面加 separation_t 点场（-1 表示无配对）。 |
| `point_array_cell_value` | 网格工具 | `src/meshgen/render_scene.rs:273` | 将点数组归约为每单元一个值（非哨兵点值的均值）以供着色。 |
| `Regime` / `SkipReason` / `MidSurfaceDefect` | 网格生成 | `src/meshgen/gapfield.rs:166/177/190` | 三种薄特征状态、[THIN-SKIP] 分类与中面校验缺陷。 |
| `MidSurface` / `MidSurface::is_valid` / `ThinRegion` | 网格生成 | `src/meshgen/gapfield.rs:204/216/227` | 带源节点与缺陷的中点面片，以及一个分割后的薄区域。 |
| `validate_mid_surface` | 网格生成 | `src/meshgen/gapfield.rs:2436` | 对候选中面执行 the reference thin-feature design §3.4 检查并记录全部缺陷。 |
| `CouplingOptions` / `LockReason` / `CouplingReport` / `CouplingReport::locked_for` | 网格生成 | `src/meshgen/sizing.rs:35/66/80/94` | S3<->S4 耦合输入、锁定原因、运行报告与按原因查询。 |
| `regime_for` | 网格生成 | `src/meshgen/sizing.rs:113` | 以 0.9/1.1 滞回死区分类单个区域。 |
| `couple_gap_and_sizing` | 网格生成 | `src/meshgen/sizing.rs:159` | 运行 S3<->S4 不动点；违反 G-8 排序断言时返回错误。 |
| `SizingCriterion` / `SizingSource` | 网格生成 | `src/meshgen/sizing.rs:335/351` | 产出该尺寸约束的 §10.6 准则，以及约束本身（位置 + 允许的最大单元）。 |
| `SizingOptions` / `SizingOptions::beta` / `SizingOptions::lfs_floor` | 网格生成 | `src/meshgen/sizing.rs:363/410/429` | 尺寸场输入；Lipschitz 常数 `grading - 1`；低于该分离量的间隙归薄特征机制而非尺寸场。 |
| `curvature_sources` | 网格生成 | `src/meshgen/sizing.rs:504` | 在条件化输入曲面的光滑内部边上产出弦差曲率源。 |
| `feature_sources` | 网格生成 | `src/meshgen/sizing.rs:605` | 特征曲线转折源，外加每个 S1 角点/交汇点一个（最短关联段）。 |
| `collect_geometry_sources` | 网格生成 | `src/meshgen/sizing.rs:677` | 曲率源与特征源合为一个规范有序列表。 |
| `gap_sources` | 网格生成 | `src/meshgen/sizing.rs:712` | 状态相关的局部特征尺寸源：对每个保持体网格的 S3 采样取 `t / gap_cells`。 |
| `SizingLookup` / `SizingLookup::build` / `eval` / `eval_box` | 网格生成 | `src/meshgen/sizing.rs:969/987/1067/1083` | 由构造即 Lipschitz 的梯度场 `min_s (h_s + beta*dist)`；点求值与盒上精确最小值。 |
| `SizingLeaf` / `SizingStats` / `SizingField` | 网格生成 | `src/meshgen/sizing.rs:1156/1164/1182` | 单个八叉树叶子、构建的预算/范围报告，以及背景八叉树本身。 |
| `SizingField::locate` / `SizingField::sample` | 网格生成 | `src/meshgen/sizing.rs:1220/1248` | 定位包含某点的叶子（每层一次二分查找）并读取其尺寸。 |
| `build_sizing_field` | 网格生成 | `src/meshgen/sizing.rs:1265` | 每层一趟并行细化背景八叉树，直至每个叶子都解析其内部的场。 |
| `SizingConstraint` / `SizingConstraint::new` / `evaluate` / `binding_region` | 网格生成 | `src/meshgen/sizing.rs:1407/1421/1468/1498` | 供耦合驱动使用的 `C(R)`，以及绑定它的区域与项。 |
| `sizing_to_doc` | 网格生成 | `src/meshgen/sizing.rs:1540` | 将尺寸场编码为携带 `sizing_h` 点数组的 `s04_sizing` 体素预览 VTU。 |
| `FREUDENTHAL` / `CellTemplate` | 网格生成 | `src/meshgen/lattice.rs:54/493` | 冻结的 6-tet Kuhn 表，以及叶子采用了哪类模板。 |
| `balance_octree` / `balance_violation` | 网格生成 | `src/meshgen/lattice.rs:156/286` | 将八叉树细化到强（面+边+顶点）2:1 平衡；并可直接检验该性质。 |
| `Lattice` / `LatticeStats` / `LatticeOptions` | 网格生成 | `src/meshgen/lattice.rs:520/502/531` | 四面体化的背景晶格、其构建报告与四面体预算。 |
| `build_lattice` / `build_lattice_with_splits` | 网格生成 | `src/meshgen/lattice.rs:611/616` | 以 Freudenthal 与形心扇形模板对平衡八叉树作四面体化。 |
| `lattice_to_doc` | 网格生成 | `src/meshgen/lattice.rs:833` | 将晶格编码为 `s05_lattice` 快照 VTU（四面体而非体素——见修订说明）。 |
| `Side` / `Provenance` / `OwnershipRecord` | 网格生成 | `src/meshgen/classify.rs:60/68/82` | 四面体相对构件的内外侧（缺省即外部，`Ambiguous` 表示由切割裁定）、条目来源，以及稀疏记录本身。 |
| `resolve` | 网格生成 | `src/meshgen/classify.rs:171` | 冻结的标签规则：内部集合取优先级最小者；集合为空时为 `{0}`。 |
| `RAY_DIRECTIONS` | 网格生成 | `src/meshgen/classify.rs:42` | 射线恰好穿过边或顶点时的冻结重发射序列（ARB-9）。 |
| `Classification` / `ClassifyStats` / `ClassifyOptions` | 网格生成 | `src/meshgen/classify.rs:140/112/457` | 逐顶点归属、播种记录、区域键、活跃面片掩码，以及各判定的达成方式。 |
| `classify_lattice` | 网格生成 | `src/meshgen/classify.rs:493` | S6：按晶格顶点 x 实体构件的精确奇偶分类、记录播种与活跃面片过滤。 |
| `classified_to_doc` | 网格生成 | `src/meshgen/classify.rs:810` | 将分类后的晶格编码为 `s06_classified` 快照 VTU。 |
| `SNAP_MOTION_CAP` / `SNAP_RECHECK_LOW` / `SNAP_RECHECK_HIGH` | 网格生成 | `src/meshgen/snap.rs:43/48/50` | 30% 位移上限（ARB-11），以及 97.5/2.5% 复核带——近端点交点改为提升端点而非切割。 |
| `WEIGHT_CORNER` / `WEIGHT_CURVE` / `WEIGHT_SURFACE` | 网格生成 | `src/meshgen/snap.rs:58/60/62` | `1e7`/`1e4`/`1e0`——冻结的吸附目标优先级的数值写法。 |
| `ALTERNATING_PROJECTION_PASSES` | 网格生成 | `src/meshgen/snap.rs:54` | 退化排布邻域曲线目标的固定交替投影趟数（ARB-2）。 |
| `TargetKind` | 网格生成 | `src/meshgen/snap.rs:68` | 节点被约束到何处，采用契约的 `constraint_kind` 编码：自由/曲面/折线/角点/盒面。 |
| `EdgeCrossing` | 网格生成 | `src/meshgen/snap.rs:90` | 一个精确的边—面片交点：边、面、构件、参数与构造点——S8 将在此切割。 |
| `Snapped` / `SnapStats` / `SnapOptions` | 网格生成 | `src/meshgen/snap.rs:138/105/161` | S7 移动后的节点、逐节点约束、交点与在切面上的节点集；其诊断；以及域盒与焊接容差。 |
| `snap_lattice` | 网格生成 | `src/meshgen/snap.rs` | S7：捕获角点与特征曲线、提升近端点交点、复核，并给出最终交点表。 |
| `unique_edges` | 网格生成 | `src/meshgen/snap.rs` | 晶格的去重边集合，每条以升序节点对表示。 |
| `move_preserves_orientation` | 网格生成 | `src/meshgen/snap.rs` | ARB-10 的精确判据：移动某节点后其所有相邻四面体是否仍为正定向。 |
| `snapped_to_doc` | 网格生成 | `src/meshgen/snap.rs` | 将吸附后的晶格编码为 `s07_snapped` 快照 VTU。 |
| `CUT_VOLUME_TOLERANCE` / `CUT_MIN_DIHEDRAL_DEG` | 网格生成 | `src/meshgen/cut.rs:33/36` | 受保护试运行的 1% 体积容差，以及 §4.4 运行期 8 度二面角下限。 |
| `NodeSide` / `Escalation` | 网格生成 | `src/meshgen/cut.rs:43/54` | 母节点相对面片的位置，以及单元无法走 §6 路径的原因。 |
| `InterfaceFace` | 网格生成 | `src/meshgen/cut.rs:70` | 一个带标签的切割三角形及其 `(内侧, 外侧)` 单元对——预留的导出契约。 |
| `CutMesh` / `CutStats` / `CutOptions` | 网格生成 | `src/meshgen/cut.rs` | S8 的节点、四面体、记录、界面与升级清单；其诊断；其容差。 |
| `snk_split_quad` / `snk_diagonal_is_02` | 网格生成 | `src/meshgen/cut.rs` | SNK 规则（§4.1）：四边形的对角线取过其最小 `NodeKey` 顶点的那条。 |
| `prism_tets` / `prism_tets_with_diagonals` | 网格生成 | `src/meshgen/cut.rs` | 冻结的六模式棱柱表（§4.3）；仅对定理 T2 判为不可达的两组循环对角线返回 `None`。 |
| `FaceCutState` / `face_split` | 网格生成 | `src/meshgen/cut.rs` | §5.2 剪纸式面剖分表（含 `split_R`）；悬空切割状态返回 `None`。 |
| `CellCut` / `cut_tet` | 网格生成 | `src/meshgen/cut.rs` | §6 单面片四面体情形表——碎片、其侧别与界面三角形。 |
| `orient_positively` / `guarded_dry_run` | 网格生成 | `src/meshgen/cut.rs` | 规范定向修正，以及 §6 的受保护试运行（ARB-15）。 |
| `cut_lattice` / `cut_to_doc` | 网格生成 | `src/meshgen/cut.rs` | S8：切割所有被穿越单元、派生界面索引、升级其余单元；并编码 `s08_cut`。 |
| `FaceMesh` / `face_mesh` / `loop_fan` / `face_centroid` | 网格生成 | `src/meshgen/junction.rs` | 升级单元的面如何三角化——冻结表适用处用表，否则用面质心扇形。 |
| `FannedCell` / `fan_cell` / `cell_centroid` / `TET_FACES` | 网格生成 | `src/meshgen/junction.rs` | 重新划分升级单元的协调质心扇形（G6-0 采纳的回退方案）。 |
| `Stage` | 网格工具 | `src/meshgen/snapshot.rs:18` | 冻结的阶段枚举（0..=11）；亦为快照索引。 |
| `Stage::from_path` | 网格工具 | `src/meshgen/snapshot.rs:74` | 从快照文件名的 sNN 标记解析阶段。 |
| `should_emit` | 网格工具 | `src/meshgen/snapshot.rs:99` | 在 none/key/all 下是否产出某阶段。 |
| `snapshot_path` | 网格工具 | `src/meshgen/snapshot.rs:118` | <stem>.debug/<stem>_sNN_<name>.vtu 路径（Quality 带 _r<N>）。 |
| `SnapshotMeta` | 网格工具 | `src/meshgen/snapshot.rs:132` | stamp_metadata/emit_snapshot 的打标输入集合。 |
| `stamp_metadata` | 网格工具 | `src/meshgen/snapshot.rs:166` | 将完整 §2.4 元数据块打标到快照文档。 |
| `emit_snapshot` | 网格工具 | `src/meshgen/snapshot.rs:276` | 打标元数据，然后写出 R4 定义的一对文件：以普通名交付的仅四面体体网格，以及紧邻其旁的混合单元契约文档。返回交付文件的路径。 |
| `warn_if_large` | 网格工具 | `src/meshgen/snapshot.rs:246` | 尺寸 WARN：snapshots=all + 估计 >5 M 四面体。 |
| `VerifyOptions` | 网格工具 | `src/meshgen/verify.rs:410` | 文档之外的验证器输入（expected_stage 用于 [V12] 交叉校验）。 |
| `verify_with_options` | 网格工具 | `src/meshgen/verify.rs:744` | 带阶段上下文的验证；s00-s03 跳过仅体网格 [V7]/[V8]/[V13]。 |
| `BoundaryFace` | 网格工具 | `src/meshgen/verify.rs:3531` | [V13] 眼中的一个材料边界面：面积、局部边长、距离与带符号偏移。 |
| `FidelityAcc` | 网格工具 | `src/meshgen/verify.rs:3548` | [V13] 按面积加权的逐分量累加器。 |
| `absorb` | 网格工具 | `src/meshgen/verify.rs:3561` | 将一个边界面折叠进 [V13] 累加器。 |
| `check_v13` | 网格工具 | `src/meshgen/verify.rs:3603` | [V13] 界面保真度：从体网格读出材料边界并与输入曲面比对（计划中的 P3）。 |
| `GpuClipPlane` | 网格工具 | `src/gpu/scene_render.rs:45` | GPU 场景预览的可选半空间裁剪（平滑切割）。 |
| `GpuSceneOptions` | 网格工具 | `src/gpu/scene_render.rs:52` | GPU 专用开关：裁剪平面、叠加线段、标记。 |
| `GpuScenePipeline` | 网格工具 | `src/gpu/scene_render.rs:70` | 离屏 GPU 场景预览：带颜色的 TriangleList + LineList 叠加，均支持裁剪平面丢弃。 |
| `GpuScenePipeline::render` | 网格工具 | `src/gpu/scene_render.rs:310` | 以不透明预览方式渲染单相机单场景。 |
| `GpuScenePipeline::render_views` | 网格工具 | `src/gpu/scene_render.rs:330` | 批量视图：几何数据仅上传一次，供所有相机复用。 |

**总计：358 个已记录行**（函数、方法、结构体、枚举、常量以及紧密相关 API 组合行）。

## 关于数量的说明

- 仅测试函数有意排除；本索引覆盖生产代码。
- 紧密耦合的 API（如 DD 算术与排布记录类型）合并在同一行。
- 不同模块中的同名辅助函数是独立定义，并非重复项。

## 参考文档

| 文档 | 覆盖范围 |
|---|---|
| [core-and-compute.md](core-and-compute.md) | 入口、核心类型、计算策略 |
| [config.md](config.md) | YAML 配置与反序列化辅助函数 |
| [geometry-core.md](geometry-core.md) | 核心几何、空间网格、STL 渲染 |
| [geometry-volume-collision.md](geometry-volume-collision.md) | 体积、碰撞、锻造 |
| [geometry-analysis.md](geometry-analysis.md) | 指标与 S2 |
| [gpu.md](gpu.md) | 特性门控 GPU 路径 |
| [io.md](io.md) | STL/TIFF/RAW I/O |
| [mesh-render-and-vtu.md](mesh-render-and-vtu.md) | 契约 VTU 与体网格渲染 |
| [mesh-verify.md](mesh-verify.md) | 验证目录与报告 |
| [meshgen.md](meshgen.md) | mesh 配置及 S0/S1/G2-1..G2-3 |
| 流水线参考页 | 按主题划分的流水线实现 |

## 另请参阅

| `BandPair` | 网格生成 | `src/meshgen/thin.rs:71` | 一对匹配的壁面顶点，塌缩后携带其边缘节点（SPEC §8.1 不变式 B1）。 |
| `BandTemplate` | 网格生成 | `src/meshgen/thin.rs:102` | 单元取用了冻结 §8.2 条带表的哪一行，以及其 `regime` 单元数组编码。 |
| `BandFailure` | 网格生成 | `src/meshgen/thin.rs:138` | 条带单元无法按其表行剖分的原因。 |
| `BandCellMesh` | 网格生成 | `src/meshgen/thin.rs:155` | 一个已剖分的条带单元：四面体、模板、阶梯级别、质量与体积误差。 |
| `snk_cell_diagonals` | 网格生成 | `src/meshgen/thin.rs` | Rule SNK 在条带单元三条对边四边形上的对角线选择。 |
| `band_cell_boundary` | 网格生成 | `src/meshgen/thin.rs` | 条带单元的闭合边界三角化——所有模板的推导来源。 |
| `enclosed_volume` | 网格生成 | `src/meshgen/thin.rs` | 一致定向闭合三角化所围的体积（条带试运行的比较目标）。 |
| `band_cell_table` | 网格生成 | `src/meshgen/thin.rs` | 冻结的 SPEC §8.2 条带表，以塌缩点对数为索引。 |
| `mesh_band_cell` | 网格生成 | `src/meshgen/thin.rs` | 在给定对角线下按表剖分一个条带单元，并做定向、二面角与体积校验。 |
| `steiner_band_cell` | 网格生成 | `src/meshgen/thin.rs` | §4.4 阶梯的末级：把单元自身边界锥化到 Steiner 顶点。 |
| `band_cell_centroid` | 网格生成 | `src/meshgen/thin.rs` | 条带回退所锥化的 Steiner 点——单元形心。 |
| `mesh_band_layer` | 网格生成 | `src/meshgen/thin.rs` | 剖分整个条带层：先查表，再成对协商翻转，再 Steiner，最后区域降级。 |
| `predict_band_quality` | 网格生成 | `src/meshgen/thin.rs` | 剖分之前，用 SPEC §8.2 标称单元预测条带的单元质量。 |
| `band_ladder` | 网格生成 | `src/meshgen/thin.rs` | 针对单个薄区域的 PLAN §10.11 五级 FEM 感知阶梯。 |
| `ThinOptions` | 网格生成 | `src/meshgen/thin.rs` | S8b 的可调项：二面角与长宽比门限、FEM 剖面、高度下限与降级占比。 |
| `MeshGenThin` | 网格工具 | `src/config/meshgen.rs` | `meshgen.thin` 配置块：条带门限、高度下限、降级占比与体元回退策略。 |

| `Slab` | 网格生成 | `src/meshgen/thin.rs` | 条带单元的边界三角形属于哪一层：近侧、间隙、远侧。 |
| `BandDecline` | 网格生成 | `src/meshgen/thin.rs` | 单元为何不属于双切规则覆盖的夹层情形；逐次运行计数并报告。 |
| `BandCellPlan` | 网格生成 | `src/meshgen/thin.rs` | 已切分的夹层单元：三个闭合层，以及间隙层的三对匹配点对。 |
| `band_face_split` | 网格生成 | `src/meshgen/thin.rs` | 双切面剖分规则：把每条被切边带两个切点的面剖分为角部／条带／剩余三部分。 |
| `close_open_surface` | 网格生成 | `src/meshgen/thin.rs` | 用未配对有向边所构成的环把开放的定向三角化封闭。 |
| `split_band_cell` | 网格生成 | `src/meshgen/thin.rs` | 把夹层单元切分为三个闭合层，并给出间隙层的匹配点对。 |
| `ThinContext` / `ThinRegime` | 网格生成 | `src/meshgen/thin.rs` | S3->S8b 的桥接：按排布面索引的薄区编号、生效薄区制式、配对类别编码、`t_sheet`，以及 `collapse_sheets` 是否开启。 |
| `thin_context` | 网格生成 | `src/meshgen/gapfield.rs` | 把 S3 的薄区归约为按排布面查表的形式，并映射每个薄区的**两侧**壁面。 |
| `pair_class_code` | 网格生成 | `src/meshgen/gapfield.rs` | 某个 `PairClass` 对应的 `ThinRegionPairClass` 编码（SPEC contracts §2.3）。 |
| `FACE_TAG_INTERFACE` / `FACE_TAG_SHEET` / `FACE_TAG_BOX_CAP` | 网格生成 | `src/meshgen/cut.rs` | S8 写出的 `FaceTagKind` 取值；此前切割阶段一直硬编码为 `0`。 |
| `face_is_single_patch` | 网格生成 | `src/meshgen/cut.rs` | 切割某个面的各组件是否描述同一张曲面（逐边切割节点映射完全相同），从而可套用 §5.2 的表。 |
| `collapsed_sheet_rim` | 网格生成 | `src/meshgen/cut.rs` | 塌缩薄片的边缘曲线——两端点均位于塌缩区域终止处的边界边。 |
| `CURVE_KIND_RIM` | 网格生成 | `src/meshgen/cut.rs` | 边缘对应的 `CurveKind` 取值（SPEC contracts §2.3）。 |
| `nodes_on_rim` | 网格生成 | `src/meshgen/cut.rs` | 位于排布边缘曲线容差范围内的网格节点——开放薄片允许终止的位置。 |
| `expected_volume` | 网格生成 | `src/meshgen/verify.rs` | 在更高优先级实体取走各自份额后，某组件应得的体积，采用分层采样估计。 |
| `TriIndex::contains` | 网格生成 | `src/meshgen/verify.rs` | 用广义绕数判定点是否位于三角形集合内部——射线奇偶性在自相交曲面上无定义。 |
| `connected_shells` | 网格生成 | `src/meshgen/verify.rs` | 按共享顶点位置把三角形集合划分为若干壳，使并集的每个组成部分可以各自定界。 |
| `sampled_volume` | 网格生成 | `src/meshgen/verify.rs` | 并集正确的体积估计：在每个壳自身的包围盒内采样，并把每个采样点计入第一个包含它的壳。 |
| `ChildTets` | 网格生成 | `src/meshgen/cut.rs` | 单个单元按 §6 表格行生成的子单元；最大的一行（情形 D）为 8 个。 |
| `volume_only` | 输入输出 | `src/io/vtu.rs:785` | 由混合单元文档派生出的**交付**用仅四面体文档：逐单元数组同步筛选，带回共享编号的 `GlobalPointId`，并重述 `Counts` 以免派生文件自我描述失真。 |
| `ArrayData::select_tuples` | 输入输出 | `src/io/vtu.rs` | 仅保留索引通过保留掩码的元组，生成新数组。 |
| `contract_path` | 网格生成 | `src/meshgen/snapshot.rs:333` | 交付体网格旁的辅助混合单元文档路径：`<stem>_contract.vtu`。 |
| `polygon_soup_centroid` | 网格生成 | `src/meshgen/cut.rs` | 闭合三角形集合的形心——各层扇形锥化所用的顶点。 |
| `KEY_ORDER_REFINEMENT` | 网格生成 | `src/meshgen/cut.rs` | S8 的排序键比焊接网格细多少（SPEC §1.2 Rule K-O）。 |
| `REGIME_NORMAL` | 网格生成 | `src/meshgen/cut.rs` | 普通单元的 `regime` 单元数组编码。 |

- [算法文档](../algorithms/) —— 概念性算法说明。
- [示例文档](../examples/) —— 各流水线走查。
- [AGENTS.md](../../../AGENTS.md) —— `AI-FUNC-SUMMARY` 与文档同步规则。

| `PointClassifier` | `src/meshgen/classify.rs:487` | S6 用于判定的逐实体分量几何，与 S8 共享，使得在晶格没有顶点的位置也能对碎片采样（SPEC §7.5）。 |
| `classify_lattice_with` | `src/meshgen/classify.rs:680` | 针对调用方已构建的 `PointClassifier` 运行 `classify_lattice`，投影网格只构建一次。 |
| `seed_record` | `src/meshgen/cut.rs:1452` | 以内部采样确定升级单元碎片的归属，而非继承父单元尚未裁决的记录。 |
| `curve_sources` | `src/meshgen/sizing.rs:699` | 沿每条锁定曲线加密，使单元至多跨越一条曲线；这是两条弦高准则无法表达的邻近性准则。 |
| `curve_coverage` | `src/meshgen/snap.rs` | 有多少锁定曲线线段真正被网格边链覆盖；`[SNAP-CURVE]` 背后的测量。 |
| `split_soup_by_surface` | `src/meshgen/junction.rs` | 按单张曲面划分升级单元的边界三角汤，并报告它留下的**每一个**封盖环（SPEC §7.6）；三个角点全部落在曲面上的三角形交由调用方的形心判别。 |
| `open_boundary_loops` | `src/meshgen/junction.rs` | 将三角汤中只被使用一次的边按孔洞串成环；对夹断或分叉的孔洞予以拒绝。按无向计数，因为三角汤没有一致的绕向。 |
| `split_soup_components` | `src/meshgen/junction.rs` | 将三角汤拆分为按边连通的各块，并以最小节点号排序——两道壁面会把外侧留成两块互不相连的实体。 |
| `curve_pierce_points` | `src/meshgen/cut.rs` | 门 G6-0：按晶格面给出锁定曲线刺穿该面的位置，以该面自身角点为键。 |
| `curve_mesh_edges` | `src/meshgen/cut.rs` | 按锁定曲线给出沿其分布的网格棱——两端点与中点都须落在曲线上；每条棱只发一个单元。 |
| `nodes_on_curve` | `src/meshgen/cut.rs` | 门 G6-0：已经落在锁定曲线上的网格节点——严格取内部的刺穿判定对这种情形不会报告任何结果。 |
| `fan_from_walk_node` | `src/meshgen/cut.rs` | 以边界走线自身的某个节点为扇心三角化该面；任一三角形退化时返回 None。 |
| `segment_pierces_triangle` | `src/meshgen/cut.rs` | 线段严格穿过三角形内部的位置——排除仅触及边界与共面的情形。 |
| `curve_segments` | `src/pipeline/meshgen.rs` | 门 G6-0 的输入：排布中全部锁定曲线的折线段。 |
| `locked_curves` | `src/pipeline/meshgen.rs` | 同一批曲线的完整形式——种类、分量集合、径向面片数与折线——供 VTU 曲线表与 `[V9]` 使用。 |
| `soup_volume` | `src/meshgen/cut.rs` | 闭合三角汤所围体积，对任意形状均精确：先以广度优先遍历定向，再作带符号求和。 |
| `fan_is_sound` | `src/meshgen/cut.rs` | 三角汤的每个三角形与其自身形心是否构成非退化四面体——即 `orient_positively` 丢弃时所问的精确问题。 |
| `cell_fan_is_conforming` | `src/meshgen/cut.rs` | §7.6 各碎片扇形化后得到的四面体彼此之间是否协调——即在单元提交之前先对它自己提出 `[V3]` 的两项检查。 |
| `fan_is_simple` | `src/meshgen/cut.rs` | 多边形的扇形化是否恰好覆盖它一次：每个三角形非退化且绕向一致。 |
| `fan_cap` | `src/meshgen/cut.rs` | 从盖多边形自身顶点中选一个能干净三角剖分的作扇形化；若无则返回 None。 |
| `fan_swallows_vertex` | `src/meshgen/cut.rs` | 扇形化的任一条边是否穿过了不属于该三角形的多边形顶点。 |
| `split_escalated_cell` | `src/meshgen/cut.rs` | 对每个穿越分量施加 §7.6，由孔洞拓扑与扇形体积检查守护；失败时回退到整体扇形。 |
| `crossed_face` | `src/meshgen/cut.rs` | 描述被面片穿过的面：每两个切割节点给出一条弦，同一分量留下四个节点时给出两条嵌套弦，两条弦互相穿插时给出相遇点。 |
| `ACTIVE_FACE_PROBE` | `src/meshgen/classify.rs` | 判断某个面是否埋在其自身自相交分量内部时，向两侧偏移的探测距离。 |
| `crossed_face_mesh` | `src/meshgen/junction.rs` | 以两条弦为边三角化该面：相交时分四个扇区，不相交时分三块多边形。 |
| `chord_meeting_point` | `src/meshgen/junction.rs` | 一个面上两条弦的相遇点——S2 交线穿刺该面之处。 |
| `fan_polygon` | `src/meshgen/junction.rs` | 从最小键顶点扇形化面的一个凸子多边形。 |
| `fan_volume` | `src/meshgen/cut.rs` | 闭合多边形三角汤所围的体积，以内部锥点上的无符号和计算。 |
| `declare_contact_components` | `src/meshgen/cut.rs` | 为落在重合排布面片上的每个网格面声明该面片所属的全部分量。 |
| `point_on_triangle` | `src/meshgen/cut.rs` | 判断一点是否落在三角形的 `eps` 之内（平面距离加重心坐标包含性）。 |
| `contact_patches` | `src/pipeline/meshgen.rs` | S2 的多标记排布面，作为三角形与其分量集合的配对——排布阶段的重合信息进入 S8 的唯一通道。 |
| `DEFAULT_MAX_WIREFRAME_EDGES` | `src/meshgen/render_scene.rs` | 输出线框线段数的默认上限；超出后按步长细化，绝不截断。 |
