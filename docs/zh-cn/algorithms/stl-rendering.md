# STL Rendering

`render` 流水线无需窗口即可把 STL 网格渲染为 PNG。`focus_point` 是画面中心目标点，`view_direction` 从相机指向该点；可选 `up_vector` 用于确定画面旋转方向，平行时会自动选择备用轴。

正交投影将网格包围盒八个角投影到相机平面，按输出宽高比和 `fit_padding` 自动取景。透视投影使用垂直 FOV；未指定 `camera_distance` 时，根据包围球自动确定相机距离。

CPU 路径通过 parry3d `TriMesh` 的 QBVH 为每个像素查询最近交点，并由 rayon 按行并行。GPU 路径使用 `GpuRenderPipeline` 将三角形离屏光栅化到颜色和深度纹理，再通过 256 字节对齐的 staging buffer 读回。GPU 失败时回退 CPU。

两条路径共享相机与着色定义。CPU 为 f64 光线投射，GPU 为 f32 光栅化，因此测试允许边缘约 1 像素和通道约 1 LSB 的差异。参见[英文完整算法说明](../../en-us/algorithms/stl-rendering.md)。
