# I/O 参考文档

> **待翻译：** `save_image` 的详细契约见[英文 I/O 参考](../../en-us/reference/io.md#imagers)。

本页记录 `src/io/mod.rs`（`io` 包的模块枢纽）、`src/io/hash.rs`（内容哈希）、`src/io/stl.rs`（ASCII/二进制 STL 网格的读写）以及 `src/io/volume.rs`（TIFF 与 RAW 体数据的读写）。

## 索引

| 函数 | 位置 | 摘要 |
|---|---|---|
| `sha256_bytes` | `src/io/hash.rs:13` | 字节切片的 SHA-256，返回小写十六进制。 |
| `sha256_file` | `src/io/hash.rs:25` | 流式计算文件的 SHA-256，返回十六进制摘要与字节数。 |
| `hex_digest` | `src/io/hash.rs:42` | 将摘要渲染为小写十六进制。 |
| `parse_ascii_vertex` | `src/io/stl.rs:9` | 将一行 ASCII STL 的 `vertex x y z` 记录解析为 `Vec3`。 |
| `quantize_key` | `src/io/stl.rs:21` | 将顶点量化为固定精度的整数键，用于容差去重。 |
| `WeldMap` | `src/io/stl.rs:10` | 顶点焊接表类型（量化键到首个索引），使用非 SipHash 的快速哈希；从不迭代。 |
| `WeldHasher` | `src/io/stl.rs:15` | 用于量化顶点键的乘法-异或哈希，带 64 位终结混合。 |
| `dedup_vertex` | `src/io/stl.rs:65` | 通过量化键查找，对照现有列表对顶点去重。 |
| `AsciiStlBuilder` | `src/io/stl.rs:47` | ASCII STL 增量状态：顶点、面、待组面顶点、去重表。 |
| `AsciiStlBuilder::push_line` | `src/io/stl.rs:56` | 按原 lossy/trim/vertex 规则处理一行原始字节。 |
| `parse_ascii_stream_or_binary` | `src/io/stl.rs:78` | 逐行流式解析 ASCII STL，未得到三角形时对保留字节回退 binary。 |
| `parse_f32_le` | `src/io/stl.rs:93` | 解析小端序 `f32` 字节并向上转换为 `f64`。 |
| `parse_binary_stl` | `src/io/stl.rs:104` | 将二进制 STL 字节解析为顶点已去重的 `Mesh`。 |
| `looks_ascii_stl` | `src/io/stl.rs:151` | 启发式检测字节内容是否为 ASCII STL。 |
| `load_stl` | `src/io/stl.rs:170` | 加载 STL 文件，自动检测 ASCII/二进制格式。 |
| `load_folder_stls` | `src/io/stl.rs:206` | 加载文件夹中所有 STL 文件。 |
| `load_stl_or_merge_folder` | `src/io/stl.rs:233` | 加载单个 STL 文件，或将目录中所有 STL 合并为一个网格。 |
| `save_stl` | `src/io/stl.rs:262` | 将网格保存为二进制 STL 文件。 |
| `collect_sorted_files` | `src/io/volume.rs:50` | 收集文件夹中的常规文件，按名称排序，可选按扩展名过滤。 |
| `resolve_slice_range` | `src/io/volume.rs:78` | 根据起止索引解析出闭区间切片范围，将 `-1` 视为"从头开始"/"到末尾"。 |
| `decode_raw_slice` | `src/io/volume.rs:107` | 根据位深、符号性和字节序，将一个原始图像切片解码为 `i64` 值。 |
| `load_raw_folder` | `src/io/volume.rs:227` | 从一个原始二进制切片文件文件夹中加载 `Volume3D`。 |
| `tiff_decoding_to_i64` | `src/io/volume.rs:266` | 将 TIFF 的 `DecodingResult` 转换为 `Vec<i64>` 缓冲区及其数值类型。 |
| `load_tiff_file_with_range` | `src/io/volume.rs:286` | 在一个闭区间页码范围内，将多页 TIFF 文件加载为 `Volume3D`。 |
| `load_tiff_file` | `src/io/volume.rs:354` | 将一个 TIFF 文件（所有页）加载为 `Volume3D`。 |
| `is_tiff_path` | `src/io/volume.rs:359` | 检查路径是否具有 `.tif`/`.tiff` 扩展名。 |
| `load_tiff_or_folder` | `src/io/volume.rs:369` | 从文件或文件夹加载 TIFF 体数据（所有页/切片）。 |
| `load_tiff_or_folder_with_range` | `src/io/volume.rs:379` | 在一个闭区间切片范围内，从文件或文件夹加载 TIFF 体数据。 |
| `write_tiff_slice` | `src/io/volume.rs:446` | 将体数据的一个 z 切片写入 TIFF 编码器的一页。 |
| `TiffPageEncoder` | `src/io/volume.rs:552` | 基于借用可 seek writer 的增量多页 TIFF 编码器。 |
| `TiffPageEncoder::new` | `src/io/volume.rs:562` | 写入 TIFF 头并固定页尺寸与类型。 |
| `TiffPageEncoder::write_slices` | `src/io/volume.rs:590` | 以连续页追加完整 z 切片。 |
| `save_tiff_or_folder_with_ext` | `src/io/volume.rs:524` | 将 `Volume3D` 保存为多页 TIFF 文件或按切片逐一保存的 TIFF 文件夹，可配置扩展名。 |
| `save_tiff_or_folder` | `src/io/volume.rs:586` | 使用默认的 `.tiff` 扩展名，将 `Volume3D` 保存为 TIFF 文件或切片文件序列。 |
| `load_stl_from_reader` | `src/io/stl.rs:192` | Forward-reader STL: streamed ASCII lines and bounded binary records. |
| `load_stl_hashed` | `src/io/stl.rs:193` | Single-pass STL parsing and raw digest. |
| `parse_binary_reader` | `src/io/stl.rs:157` | Read binary triangle records with incremental deduplication. |
| `read_stl_record` | `src/io/stl.rs:140` | Read complete record or report truncation. |
| `HashingReader` | `src/io/hash.rs:50` | Incremental digest over delivered bytes. |
| `HashingReader::new` | `src/io/hash.rs:58` | Wrap forward reader for hashing. |
| `HashingReader::finish` | `src/io/hash.rs:67` | Return digest and consumed byte count. |
| `stl_paths` | `src/io/stl.rs:218` | List STL paths in existing directory order. |

## 模块职责：`io/mod.rs`

`src/io/mod.rs` 声明了 `stl` 和 `volume` 两个子模块，并在 `io::` 路径下重新导出它们的公共项（来自 `stl` 的 `load_stl`、`load_folder_stls`、`load_stl_or_merge_folder`、`save_stl`；来自 `volume` 的 `load_raw_folder`、`load_tiff_or_folder`、`load_tiff_or_folder_with_range`、`save_tiff_or_folder`、`save_tiff_or_folder_with_ext`、`ByteOrder`、`RawFolderSpec`、`Volume3D`、`VolumeNumericType`）。该文件本身不包含任何函数。

## hash.rs

#### sha256_bytes

- **签名：** `pub fn sha256_bytes(bytes: &[u8]) -> String`
- **源码：** `src/io/hash.rs:13`
- **用途：** 计算内存中字节切片的 SHA-256 摘要。
- **参数：**
  - `bytes` — 待哈希的内容。
- **返回：** 64 个小写十六进制字符的摘要。
- **副作用：** 无。
- **说明：** 用于放置记录与运行报告中的内容身份。这是密码学摘要，与本 crate 中另一处 "hash"（`pipeline/meshgen.rs` 的 `config_hash`，一个仅用于变更检测的非密码学 `u64`）无关。

#### sha256_file

- **签名：** `pub fn sha256_file(path: &Path) -> Result<(String, u64)>`
- **源码：** `src/io/hash.rs:25`
- **用途：** 在不将整个文件读入内存的前提下计算其内容哈希。
- **参数：**
  - `path` — 待读取的文件。
- **返回：** `(小写十六进制摘要, 已哈希的字节数)`。
- **副作用：** 打开并读取该文件。
- **说明：** 通过 64 KiB 缓冲区流式读取，因此文件大小受限于磁盘而非内存。文件无法打开或读取时返回 `RustMsptError::Io` —— 文件缺失是错误，绝不会返回"空内容的摘要"。返回的长度即报告中每个输入与输出所记录的 `bytes`。

#### hex_digest

- **签名：** `fn hex_digest(digest: &[u8]) -> String`
- **源码：** `src/io/hash.rs:42`
- **用途：** 将原始摘要格式化为小写十六进制。
- **参数：**
  - `digest` — 原始摘要字节。
- **返回：** 长度为 `2 * digest.len()` 的小写十六进制字符串。
- **副作用：** 无。
- **说明：** 私有函数；两个公开入口共用它，因此二者的格式不可能出现分歧。

## stl.rs

本文件实现了 STL（立体光刻）网格的 I/O，读取时自动检测 ASCII/二进制格式，写入时仅输出二进制格式。一套共享的顶点去重方案（将每个坐标量化到 1e-6 网格，再以得到的 `(i64, i64, i64)` 三元组作为 `HashMap` 的键）将 ASCII 和二进制解析器中每个三角形冗余生成的重合顶点，合并为一份带索引的顶点列表——这与几何代码中其他地方用于网格裁剪的量化键去重模式相同。

### 公共函数

#### load_stl

- **签名：** `pub fn load_stl(path: &Path) -> Result<Mesh>`
- **源码位置：** `src/io/stl.rs:170`
- **用途：** 加载 STL 文件，自动检测其为 ASCII 还是二进制格式。
- **参数：**
  - `path` — `.stl` 文件的路径。
- **返回值：** 解析得到的 `Mesh`。
- **副作用：** 读取文件；binary 按记录有界读取，ASCII 逐行流式解析。
- **说明：** 使用 `looks_ascii_stl` 嗅探格式。若嗅探结果为 ASCII，则流式调用 `parse_ascii_stream_or_binary`；若该解析失败（例如在看似合法的头部之后出现格式错误的内容），会静默回退到 `parse_binary_stl`，而不是向上传播 ASCII 解析错误。二进制解析在其他情况下则是终止路径。

#### load_folder_stls

- **签名：** `pub fn load_folder_stls(folder: &Path) -> Result<Vec<(PathBuf, Mesh)>>`
- **源码位置：** `src/io/stl.rs:206`
- **用途：** 加载文件夹内（不递归）直接包含的每一个 `.stl` 文件（扩展名匹配不区分大小写）。
- **参数：**
  - `folder` — 要扫描的目录（不递归）。
- **返回值：** `(文件路径, 解析后的网格)` 对组成的 `Vec`，顺序为目录遍历顺序（未排序）。
- **副作用：** 读取目录列表，并从磁盘读取每一个匹配的文件。
- **说明：** 非 `.stl` 条目和子目录会被静默跳过。由于结果未排序，遍历顺序依赖于操作系统，这与 `volume.rs` 中的 `collect_sorted_files` 不同。

#### load_stl_or_merge_folder

- **签名：** `pub fn load_stl_or_merge_folder(path: &Path) -> Result<Mesh>`
- **源码位置：** `src/io/stl.rs:233`
- **用途：** 从单个 STL 文件加载一个网格，或将目录中所有 STL 文件合并为一个网格。
- **参数：**
  - `path` — 文件路径（单个 STL）或目录路径（待合并的 STL 文件夹）。
- **返回值：** 加载（或合并）后的 `Mesh`。合并时，顶点数组会被拼接，且每个后续网格的面索引都会按当前累计的顶点数进行偏移。
- **副作用：** 从磁盘读取一个或多个文件。
- **说明：** 若 `path` 是不包含任何 STL 文件的目录，则返回 `RustMsptError::InvalidConfig`。合并过程中不进行跨文件的顶点去重——仅在每个文件自身的解析内部去重。

#### save_stl

- **签名：** `pub fn save_stl(path: &Path, mesh: &Mesh, solid_name: &str) -> Result<()>`
- **源码位置：** `src/io/stl.rs:262`
- **用途：** 将网格以二进制 STL 文件形式写入磁盘。
- **参数：**
  - `path` — 目标文件路径。
  - `mesh` — 待序列化的网格。
  - `solid_name` — 写入 80 字节二进制头部的名称（若超过 80 字节则截断；UTF-8/ASCII 字节按原样复制）。
- **返回值：** 成功时返回 `Ok(())`。
- **副作用：** 如有需要会创建父目录（`fs::create_dir_all`）；在 `path` 处创建/覆盖文件。
- **说明：** 若网格为空，则返回 `RustMsptError::InvalidMesh`。三角形法线始终写为零向量——不进行法线重新计算。顶点坐标从 `f64` 向下转换为 `f32` 以适配 STL 二进制格式，因此往返转换不保留精度。每个三角形的属性字节计数尾部始终写为 `0u16`。

### 私有辅助函数

#### parse_ascii_vertex

- **签名：** `fn parse_ascii_vertex(line: &str) -> Option<Vec3>`
- **源码位置：** `src/io/stl.rs:9`
- **用途：** 将 ASCII STL 文本中已修剪的单行解析为一条 `vertex x y z` 记录。
- **参数：**
  - `line` — STL 文本的一行。
- **返回值：** 若该行恰好包含 4 个以空白分隔的标记，第一个等于 `"vertex"`，其余三个可解析为 `f64`，则返回 `Some(Vec3)`；否则返回 `None`。
- **副作用：** 无。

#### quantize_key

- **签名：** `fn quantize_key(v: Vec3) -> (i64, i64, i64)`
- **源码位置：** `src/io/stl.rs:21`
- **用途：** 为一个顶点生成可哈希的、经过容差量化的键，使近乎相同的浮点坐标映射到同一个键。
- **参数：**
  - `v` — 待量化的顶点。
- **返回值：** `(x, y, z)`——每个坐标乘以 `1_000_000.0`、四舍五入并转换为 `i64`，即量化到 1e-6 单位的网格上。
- **副作用：** 无。
- **说明：** `1e6` 缩放因子是一个固定常量；任意轴上真实位置相差小于约 `5e-7` 单位的顶点会被视为相同。

#### dedup_vertex

- **签名：** `fn dedup_vertex(vertices: &mut Vec<Vec3>, map: &mut WeldMap, v: Vec3) -> usize`
- **源码位置：** `src/io/stl.rs:65`
- **用途：** 返回与 `v` 的量化键匹配的现有顶点的索引；若无匹配，则将 `v` 作为新顶点追加并返回其新索引。
- **参数：**
  - `vertices` — 运行中的顶点列表，在缓存未命中时被追加。
  - `map` — 量化键 → 索引的缓存，在缓存未命中时更新。
  - `v` — 待查找或插入的顶点。
- **返回值：** `v` 在 `vertices` 中的索引（已存在或新插入）。
- **副作用：** 原地修改 `vertices` 和 `map`。
- **说明：** `map` 为 `WeldMap`：使用本地 `WeldHasher`（乘法-异或加 64 位终结混合）替代 SipHash 的 `HashMap`。该表只做查找与插入、从不迭代，因此哈希函数不会改变顶点获得的索引；在 34 MB 二进制 STL 上载入阶段由 0.127 s 降至约 0.06 s，下游输出逐字节一致（PLAN.Performance.md §73）。二进制读取还按文件头的三角形数预分配顶点、面与哈希表（上限 2^24，避免损坏的文件头过量预留）。

#### AsciiStlBuilder / push_line

- **签名：** `struct AsciiStlBuilder`；`fn push_line(&mut self, raw: &[u8])`
- **源码：** `src/io/stl.rs:47`
- **用途：** 取代原整段文本的 `parse_ascii_stl`：对一行原始字节（末尾 `\n` 可有可无）做 lossy 解码、trim 并用 `parse_ascii_vertex` 匹配；每满三个顶点生成一个去重后的 `Triangle`。
- **副作用：** 修改 builder。
- **说明：** `0x0A` 不会出现在 UTF-8 多字节序列内，因此按 `\n` 切行后逐行 lossy 解码与整段解码等价；`trim` 去掉 CR，CRLF 与 LF 的结果与 `str::lines` 相同。

#### parse_ascii_stream_or_binary

- **签名：** `fn parse_ascii_stream_or_binary(reader: impl BufRead, path: &Path) -> Result<Mesh>`
- **源码：** `src/io/stl.rs:78`
- **用途：** 逐行解析被嗅探为 ASCII 的 STL；若没有得到任何三角形，则把同一字节按 binary 解析（原有回退）。
- **返回：** ASCII 网格，或 binary 回退的结果/错误。
- **副作用：** 读到 EOF。
- **说明：** 原始字节只保留到第一个 ASCII 三角形完成为止，此后 ASCII 结果已确定；被误判为 ASCII 的 binary 文件仍会完整保留，与之前相同。`tests/stl_stream_tests.rs::ascii_stream_matches_whole_text_oracle` 将顶点/面逐位与原整段解析比较（CRLF、制表符、科学计数、缺少 `endsolid`、无结尾换行、Unicode 空白、非法 UTF-8、单独 CR、`VERTEX`、不完整 facet、200 KB 长行、短读、摘要及 binary 回退）。

#### parse_f32_le

- **签名：** `fn parse_f32_le(bytes: &[u8]) -> f64`
- **源码位置：** `src/io/stl.rs:93`
- **用途：** 将 4 个字节按小端序读取为 IEEE-754 `f32`，并扩宽为 `f64`。
- **参数：**
  - `bytes` — 一个 4 字节切片（直接索引 `[0..4]`；若输入更短则会 panic）。
- **返回值：** 解码得到的值，类型为 `f64`。
- **副作用：** 无。

#### parse_binary_stl

- **签名：** `fn parse_binary_stl(bytes: &[u8], path: &Path) -> Result<Mesh>`
- **源码位置：** `src/io/stl.rs:104`
- **用途：** 解析二进制 STL 格式（80 字节头部、4 字节三角形计数，随后每个三角形 50 字节）为已去重的 `Mesh`。
- **参数：**
  - `bytes` — 完整的文件内容。
  - `path` — 源路径，仅用于错误信息。
- **返回值：** 解析得到的 `Mesh`。
- **副作用：** 无（对给定字节切片的纯解析）。
- **说明：** 若文件小于 84 字节的最小头部要求，或声明的三角形计数意味着文件大小应大于实际大小（`84 + tri_count * 50`，使用 `saturating_mul` 计算以避免损坏计数导致溢出），则返回 `RustMsptError::InvalidMesh`。每个 50 字节的三角形记录为：12 字节法线（跳过/忽略）、3×12 字节顶点、2 字节属性计数（跳过）。文件中的法线完全不会被读入网格。

#### looks_ascii_stl

- **签名：** `fn looks_ascii_stl(bytes: &[u8]) -> bool`
- **源码位置：** `src/io/stl.rs:151`
- **用途：** 启发式地嗅探一段字节缓冲区是 ASCII STL 还是二进制 STL。
- **参数：**
  - `bytes` — 文件内容（或其前缀）。
- **返回值：** 若前 5 个字节是 `"solid"`，且对前 512 字节做小写扫描后同时包含 `"facet"` 和 `"vertex"`，则返回 `true`。
- **副作用：** 无。
- **说明：** 这仅是一种启发式方法——一个二进制 STL 若其 80 字节头部恰好以 `"solid"` 开头（这是已知的 STL 格式歧义），可能通过第一项检查，但前 512 字节中的 `"facet"`/`"vertex"` 子串测试通常能消除歧义。`load_stl` 通过在 ASCII 解析失败时回退到二进制解析，进一步防范误判。

## volume.rs

本文件实现了两种独立的三维图像格式，作为堆积管线中孔隙/体积几何的输入/输出：原始扁平二进制切片堆栈（`RawFolderSpec` / `load_raw_folder`）和 TIFF（单个多页文件或单页文件的文件夹）。两者最终都汇聚到统一的 `Volume3D` 表示。

### 类型

#### VolumeNumericType

- **签名：** `pub enum VolumeNumericType { U8, U16, U32, I8, I16, I32 }`
- **源码位置：** `src/io/volume.rs:8-16`
- **用途：** 标记已加载体数据的原始逐体素位深与符号性，与经扩宽后的内存存储类型无关。
- **派生：** `Debug, Clone, Copy, PartialEq, Eq`。
- **说明：** 既用于加载时解释/校验 RAW 字节解码和 TIFF 样本类型，也用于在 `write_tiff_slice` 中写出 TIFF 输出时选择正确的窄化转换（使用带范围检查的 `try_from`）。

#### ByteOrder

- **签名：** `pub enum ByteOrder { LittleEndian, BigEndian }`
- **源码位置：** `src/io/volume.rs:18-22`
- **用途：** 选择用于解码 RAW 体数据文件中多字节样本的字节序。
- **派生：** `Debug, Clone, Copy, PartialEq, Eq`。
- **说明：** 仅由 `decode_raw_slice`（进而 `load_raw_folder`）使用；TIFF 文件内部自带字节序标记，不受此类型影响。

#### Volume3D

- **签名：**
  ```rust
  pub struct Volume3D {
      pub width: usize,
      pub height: usize,
      pub depth: usize,
      pub data: Vec<i64>,
      pub numeric_type: VolumeNumericType,
  }
  ```
- **源码位置：** `src/io/volume.rs:24-31`
- **用途：** 三维标量体数据在内存中的统一表示，与来源格式（RAW 或 TIFF）无关。

| 字段 | 类型 | 含义 |
|---|---|---|
| `width` | `usize` | 沿 X 方向的体素数（每个切片的列数）。 |
| `height` | `usize` | 沿 Y 方向的体素数（每个切片的行数）。 |
| `depth` | `usize` | 沿 Z 方向的切片数。 |
| `data` | `Vec<i64>` | 扁平体素缓冲区，长度为 `width * height * depth`。加载时每种源数值类型（`u8`/`u16`/`u32`/`i8`/`i16`/`i32`）都会被扩宽为 `i64`，以便单一缓冲区类型能统一表示任意支持的位深；`numeric_type` 记录了原始位深，以便保存时能正确地窄化回去。 |
| `numeric_type` | `VolumeNumericType` | 原始（扩宽前）的样本类型，用于在写回时对数值进行范围检查和窄化。 |

> **重要：** `Volume3D::data` 采用 **Z 主序索引**：`idx = z * width * height + y * width + x`。每个 z 切片是一个连续的 `width * height` 块，各切片依次排列。这是每个读取器（`load_raw_folder`、`load_tiff_file_with_range`、`load_tiff_or_folder_with_range`）生成的布局，也是每个写入器（`save_tiff_or_folder_with_ext`）在按 `z` 将 `data` 切分为 `width * height` 大小的块时所依赖的布局。任何手动索引 `Volume3D::data` 的代码——包括 GPU/WGSL 计算着色器——都必须遵循这种 Z 主序布局，而非 X 主序布局，否则体素位置会被无声地打乱。

#### RawFolderSpec

- **签名：**
  ```rust
  pub struct RawFolderSpec {
      pub folder: PathBuf,
      pub width: usize,
      pub height: usize,
      pub bits: u8,
      pub signed: bool,
      pub byte_order: ByteOrder,
      pub slice_start: isize,
      pub slice_end: isize,
  }
  ```
- **源码位置：** `src/io/volume.rs:33-43`
- **用途：** 完整指定如何将一个存放扁平原始二进制切片文件的文件夹解释为 `Volume3D`。

| 字段 | 类型 | 含义 |
|---|---|---|
| `folder` | `PathBuf` | 存放原始切片文件的目录（文件夹中所有文件都会被纳入考虑，不按扩展名过滤）。 |
| `width` | `usize` | 每行体素数；必须 `> 0`。 |
| `height` | `usize` | 每个切片的行数；必须 `> 0`。 |
| `bits` | `u8` | 样本位深；必须是 `8`、`16` 或 `32`。 |
| `signed` | `bool` | 样本是否为有符号整数。 |
| `byte_order` | `ByteOrder` | 多字节样本（16/32 位）使用的字节序。 |
| `slice_start` | `isize` | 待加载的第一个切片索引（含）；`-1` 表示"从第一个文件开始"。 |
| `slice_end` | `isize` | 待加载的最后一个切片索引（含）；`-1` 表示"直到最后一个文件"。 |

### 公共函数

#### load_raw_folder

- **签名：** `pub fn load_raw_folder(spec: &RawFolderSpec) -> Result<Volume3D>`
- **源码位置：** `src/io/volume.rs:227`
- **用途：** 将文件夹中（在请求的切片范围内）的每个文件作为固定大小的原始二进制切片读取并解码，加载为 `Volume3D`。
- **参数：**
  - `spec` — 文件夹路径、每切片尺寸、样本格式和切片范围。
- **返回值：** 一个 `Volume3D`，其 `depth` 等于实际加载的切片数（`end - start + 1`），其 `numeric_type` 由 `spec.bits`/`spec.signed` 推导得出。
- **副作用：** 列出目录，并将每个所选文件读入内存。
- **说明：** 若 `width` 或 `height` 为 `0`，若 `bits` 不是 `8`/`16`/`32` 之一，或若任一所选文件的字节长度与 `width * height * bytes_per_pixel` 不完全相等，则返回 `RustMsptError::InvalidConfig`。文件在应用切片范围之前先按文件名排序（通过 `collect_sorted_files`），因此顺序取决于文件名排序顺序，而非文件修改时间或嵌入的切片编号。

#### load_tiff_or_folder

- **签名：** `pub fn load_tiff_or_folder(path: &Path) -> Result<Volume3D>`
- **源码位置：** `src/io/volume.rs:369`
- **用途：** 加载整个 TIFF 体数据（所有页，可来自单个多页文件或文件夹中的所有切片），不加范围限制。
- **参数：**
  - `path` — TIFF 文件路径或 TIFF 文件所在目录。
- **返回值：** 完整的 `Volume3D`。
- **副作用：** 从磁盘读取（参见 `load_tiff_or_folder_with_range`）。
- **说明：** 薄封装：`load_tiff_or_folder_with_range(path, -1, -1)`。

#### load_tiff_or_folder_with_range

- **签名：** `pub fn load_tiff_or_folder_with_range(path: &Path, slice_start: isize, slice_end: isize) -> Result<Volume3D>`
- **源码位置：** `src/io/volume.rs:379`
- **用途：** 从单个多页 TIFF 文件或存放单/多页 TIFF 文件的文件夹中加载 TIFF 体数据，限定在一个闭区间的切片/页范围内。
- **参数：**
  - `path` — 文件路径或目录路径。
  - `slice_start`、`slice_end` — 闭区间的边界；任一端为 `-1` 表示"从头开始"/"直到末尾"。
- **返回值：** 解码得到的 `Volume3D`，其 `depth` 等于实际加载的页数/文件数。
- **副作用：** 从磁盘读取一个或多个文件。
- **说明：** 若 `path` 是文件，则直接委托给 `load_tiff_file_with_range`，此时范围指的是*该文件内的页*。若 `path` 是目录，则收集 `.tif`/`.tiff` 文件（通过 `collect_sorted_files` 按名称排序），针对*文件数量*解析范围，并通过 `load_tiff_file` 完整加载每个所选文件（即在文件夹模式下，不会对每个文件的页范围做二次选择——每个所选文件的每一页都会被包含）。若 `path` 既不是文件也不是目录，若目录中没有匹配文件，或若所选文件之间的宽/高/数值类型不一致，则返回 `RustMsptError::InvalidConfig`。

#### save_tiff_or_folder_with_ext

- **签名：** `pub fn save_tiff_or_folder_with_ext(volume: &Volume3D, output: &Path, file_prefix: Option<&str>, file_extension: Option<&str>) -> Result<()>`
- **源码位置：** `src/io/volume.rs:524`
- **用途：** 将 `Volume3D` 保存为单个多页 TIFF 文件，或保存为逐切片单独编号的 TIFF 文件夹，输出扩展名可配置。
- **参数：**
  - `volume` — 待写入的体数据。
  - `output` — 目标路径；若其扩展名为 `.tif`/`.tiff`（依据 `is_tiff_path`），则在该处写入单个多页文件；否则将 `output` 视为（并创建为）文件夹。
  - `file_prefix` — 仅文件夹模式：每个切片文件的文件名前缀（默认 `"slice"`）；切片命名为 `{prefix}_{z:04}.{extension}`。
  - `file_extension` — 仅文件夹模式：使用的文件扩展名，必须是 `tif` 或 `tiff`（默认 `"tiff"`，比较时不区分大小写）。
- **返回值：** 成功时返回 `Ok(())`。
- **副作用：** 按需创建父目录/输出目录；在磁盘上创建/覆盖一个或多个文件。
- **说明：** 若 `width`/`height`/`depth` 中任一为 `0`，若 `volume.data.len() != width * height * depth`，或（文件夹模式下）若 `file_extension` 解析结果不是 `tif`/`tiff`，则返回 `RustMsptError::InvalidConfig`。每个 z 切片从 `volume.data` 中按 `data[z*slice_len .. (z+1)*slice_len]` 提取——即依赖于 `Volume3D` 文档中所述的 Z 主序布局。编码期间会对逐体素的值进行范围检查，并窄化为体数据的 `numeric_type`（参见 `write_tiff_slice`）；不适配的值会产生 `RustMsptError::InvalidConfig`。

#### save_tiff_or_folder

- **签名：** `pub fn save_tiff_or_folder(volume: &Volume3D, output: &Path, file_prefix: Option<&str>) -> Result<()>`
- **源码位置：** `src/io/volume.rs:586`
- **用途：** 使用默认的 `"tiff"` 扩展名，将 `Volume3D` 保存为 TIFF 文件或切片文件序列。
- **参数：**
  - `volume` — 待写入的体数据。
  - `output` — 目标文件或文件夹路径。
  - `file_prefix` — 文件夹模式下的文件名前缀（参见 `save_tiff_or_folder_with_ext`）。
- **返回值：** 成功时返回 `Ok(())`。
- **副作用：** 与 `save_tiff_or_folder_with_ext` 相同。
- **说明：** 薄封装：`save_tiff_or_folder_with_ext(volume, output, file_prefix, Some("tiff"))`。

### 私有辅助函数

#### collect_sorted_files

- **签名：** `fn collect_sorted_files(folder: &Path, extensions: Option<&[&str]>) -> Result<Vec<PathBuf>>`
- **源码位置：** `src/io/volume.rs:50`
- **用途：** 列出文件夹内直接包含的常规文件，可选按一组小写扩展名过滤，按文件名排序。
- **参数：**
  - `folder` — 要扫描的目录（不递归）。
  - `extensions` — 若为 `Some`，则只保留扩展名（小写化后）匹配给定字符串之一的文件；若为 `None`，则保留所有常规文件。
- **返回值：** 按 `file_name()` 排序的 `Vec<PathBuf>`。
- **副作用：** 从磁盘读取目录列表。个别条目的目录读取错误会通过 `filter_map(|entry| entry.ok()...)` 被静默丢弃；只有顶层的 `fs::read_dir` 错误会向上传播。

#### resolve_slice_range

- **签名：** `fn resolve_slice_range(total: usize, start: isize, end: isize) -> Result<(usize, usize)>`
- **源码位置：** `src/io/volume.rs:78`
- **用途：** 将可能带有哨兵值的 `(start, end)` 切片/页边界，转换为经过校验的、具体的闭区间 `(usize, usize)` 范围。
- **参数：**
  - `total` — 可用条目（文件或页）的总数。
  - `start` — 请求的起始索引（含）；`-1`（或任何负数）表示 `0`。
  - `end` — 请求的结束索引（含）；`-1`（或任何负数）表示 `total - 1`。
- **返回值：** `(s, e)` —— 解析得到的闭区间边界，均为 `usize`。
- **副作用：** 无。
- **说明：** 若 `total == 0`（"没有文件"），或若解析出的边界越界或倒置（`s >= total || e >= total || s > e`），则返回 `RustMsptError::InvalidConfig`。除 `-1` 之外的任何负数 `start`/`end` 都被等同处理为 `-1`（钳制到 `0`/`total - 1`），而不会被拒绝。

#### decode_raw_slice

- **签名：** `fn decode_raw_slice(bytes: &[u8], bits: u8, signed: bool, byte_order: ByteOrder) -> Result<Vec<i64>>`
- **源码位置：** `src/io/volume.rs:107`
- **用途：** 根据位深、符号性和字节序，将一段扁平字节缓冲区解码为逐体素的 `i64` 值。
- **参数：**
  - `bytes` — 一个切片的原始样本字节。
  - `bits` — 样本宽度：`8`、`16` 或 `32`。
  - `signed` — 是否将样本解释为有符号整数。
  - `byte_order` — 16/32 位样本使用的字节序（对 8 位样本忽略）。
- **返回值：** 一个 `Vec<i64>`，每个解码样本对应一项，均从其原生宽度（`u8`/`i8`/`u16`/`i16`/`u32`/`i32`）扩宽为 `i64`。
- **副作用：** 无。
- **说明：** 若 `bytes.len()` 不是样本字节宽度的整数倍（16 位为 2，32 位为 4；8 位无对齐限制），或若 `bits` 不是 `8`/`16`/`32`，则返回 `RustMsptError::InvalidConfig`。

#### tiff_decoding_to_i64

- **签名：** `fn tiff_decoding_to_i64(decoded: DecodingResult) -> Result<(Vec<i64>, VolumeNumericType)>`
- **源码位置：** `src/io/volume.rs:266`
- **用途：** 将 `tiff` crate 的 `DecodingResult` 枚举（某一页解码后的像素缓冲区）转换为本 crate 自身的 `(Vec<i64>, VolumeNumericType)` 表示。
- **参数：**
  - `decoded` — 来自 `tiff::decoder::Decoder::read_image` 的解码图像数据。
- **返回值：** 由扩宽后的 `i64` 值与匹配的 `VolumeNumericType` 标签组成的元组。
- **副作用：** 无。
- **说明：** 处理 `DecodingResult::{U8,U16,U32,I8,I16,I32}`；任何其他变体（例如 `F32`/`F64` 浮点 TIFF，或 64 位整数变体）会返回带有 "Unsupported TIFF sample type" 消息的 `RustMsptError::InvalidConfig`——不支持浮点 TIFF 体数据。

#### load_tiff_file_with_range

- **签名：** `fn load_tiff_file_with_range(path: &Path, slice_start: isize, slice_end: isize) -> Result<Volume3D>`
- **源码位置：** `src/io/volume.rs:286`
- **用途：** 从单个多页 TIFF 文件中加载一个闭区间的页范围，得到 `Volume3D`。
- **参数：**
  - `path` — `.tif`/`.tiff` 文件的路径。
  - `slice_start`、`slice_end` — 闭区间的页索引边界；`-1` 表示"从头开始"/"直到末尾"。
- **返回值：** 覆盖所请求页范围的 `Volume3D`，其 `depth` 等于实际加载的页数。
- **副作用：** **两次**打开并读取该文件：第一遍在一个循环中调用 `decoder.next_image()`，仅用于统计总页数（`more_images()`/`next_image()`）；第二遍（对一个新的 `File::open` 使用新的 `Decoder`）实际定位到 `start` 并解码 `start..=end` 范围内的页。
- **说明：** 若任一页的尺寸与第一个加载页不同，或若任一页解码后的数值类型与第一页不同，则返回 `RustMsptError::InvalidConfig`。成功时，`numeric_type` 来自该循环，是 `Some`；由于 `start..=end` 始终非空（由 `resolve_slice_range` 保证），`unwrap_or(VolumeNumericType::U8)` 的兜底分支在实践中不可达。

#### load_tiff_file

- **签名：** `fn load_tiff_file(path: &Path) -> Result<Volume3D>`
- **源码位置：** `src/io/volume.rs:354`
- **用途：** 将单个 TIFF 文件的每一页加载为 `Volume3D`。
- **参数：**
  - `path` — `.tif`/`.tiff` 文件的路径。
- **返回值：** 该文件对应的完整 `Volume3D`。
- **副作用：** 从磁盘读取该文件（两次——参见 `load_tiff_file_with_range`）。
- **说明：** 薄封装：`load_tiff_file_with_range(path, -1, -1)`。

#### is_tiff_path

- **签名：** `fn is_tiff_path(path: &Path) -> bool` — `src/io/volume.rs:359`。检查是否具有 `.tif`/`.tiff` 扩展名（不区分大小写）。无副作用。

#### write_tiff_slice

- **签名：** `fn write_tiff_slice<W: Write + Seek>(encoder: &mut TiffEncoder<W>, width: u32, height: u32, ty: VolumeNumericType, slice: &[i64]) -> Result<()>`
- **源码位置：** `src/io/volume.rs:446`
- **用途：** 将一个切片的 `i64` 体素值窄化回其原生位宽，并通过 TIFF 编码器将其写为一个灰度页。
- **参数：**
  - `encoder` — 待追加一页的、已打开的多页 TIFF 编码器。
  - `width`、`height` — 页的像素尺寸。
  - `ty` — 要窄化到的目标数值类型（`U8`/`U16`/`U32`/`I8`/`I16`/`I32`），各自映射到对应的 `tiff::encoder::colortype`（`Gray8`、`Gray16`、`Gray32`、`GrayI8`、`GrayI16`、`GrayI32`）。
  - `slice` — 该页长度为 `width * height` 的 `i64` 值。
- **返回值：** 成功时返回 `Ok(())`。
- **副作用：** 向 `encoder` 底层的文件流写入一个新的图像/页。
- **说明：** 使用 `TryFrom`（`u8::try_from`、`i32::try_from` 等）将每个 `i64` 值转换为更窄的目标类型；任何超出目标类型范围的值都会返回 `RustMsptError::InvalidConfig`（"Value out of range for ... TIFF output"），而不是静默地饱和或截断。

## 交叉说明

- **顶点去重与体素扩宽是两种互不相关、但都为数值稳健性而存在的机制：** `stl.rs` 的 `quantize_key`/`dedup_vertex` 使用固定的 1e6 量化尺度合并近乎重复的浮点顶点位置；`volume.rs` 的扩宽为 `i64`（`decode_raw_slice`、`tiff_decoding_to_i64`）则是为了让 `Volume3D` 能以一个统一类型的缓冲区容纳六种整数样本类型中的任意一种，且不发生有损转换，同时由 `VolumeNumericType` 记录原始类型，以便保存时能精确窄化回去。
- **两遍式 TIFF 范围加载：** `load_tiff_file_with_range` 在进行真正的（可能受范围限制的）解码之前，会完整地重新打开并重新解码文件以统计页数，这是因为底层的 `tiff` crate 的 `Decoder` 仅提供前向迭代（`more_images`/`next_image`），没有随机访问式的页数查询接口。
- 两个文件中的所有 `AI-FUNC-SUMMARY` 注释均已对照其所标注的代码进行核对，未发现有陈旧到需要添加 `Doc note` 提示的情况。

Binary STL output now uses a 64 KiB BufWriter and explicitly flushes before success, propagating late I/O failures. Header, zero normals, f32 coordinates and attribute bytes are unchanged. This reduces tiny write system calls without buffering the full file.

### Shared STL stream and digest (PERF-18)

`load_stl_from_reader(reader, path)` accepts a forward-only reader. It sniffs at most 512 bytes, chains that prefix back for binary parsing, and preserves ASCII-first/fallback behavior. `parse_binary_reader` reads one 50-byte triangle record at a time, deduplicates in first-encounter order, grows geometry storage as records arrive, and drains permitted trailing data. Truncated header/records return InvalidMesh; other read errors propagate. A corrupt count does not cause a count-sized initial allocation. ASCII is streamed line by line (see below).

`load_stl_hashed(path)` returns `(Mesh, sha256, bytes)` from one file pass through `HashingReader`, including ignored binary trailers in the digest/count. Placement shape loading uses this entry, preserving input list and first-face shell order. `HashingReader::finish` describes consumed bytes only; successful STL parsing drains its input before finishing. Independent `sha256_file` remains bounded and unchanged.

`stl_paths` retains directory iteration order. Folder STL loading uses batches of at most two independent readers under the current Rayon pool; results/errors are consumed in path order. Folder merging consumes each completed batch into the output, so it retains at most two unmerged input meshes rather than the entire folder plus the output. The API returning all individual meshes still retains those results by contract.

### 文件夹有界解码（2026-09-23）

`consume_file_batches` 使用当前 Rayon 池最多并行加载两个 RAW/TIFF 文件（单 worker 池一次一个），随后按原文件名顺序验证并合并。逐文件 Result 保持有序，因此较早文件的 shape/type 错误优先于较晚的解码错误；失败后不启动后续批次。每个 TIFF reader 仍串行推进页，文件夹范围仍选择完整文件。上限是两个已解码文件而非字节预算：单个多页文件和汇总 Volume3D 仍常驻内存。I/O 不创建线程池，单文件 TIFF 加载仍顺序执行，输出遵循下方有界 writer 契约。线程/类型/顺序/存活缓冲验证和性能限制见 PLAN.Performance.md §59。

小于 512 KiB 的 RAW 文件即使多 worker 也串行解码；该阈值依据 §59 的小文件退化与较大切片对照。只有一个文件的批次始终直接解码。

### TIFF 文件夹有界写出与显式刷新（2026-09-23）

`save_tiff_or_folder_with_ext` 借用切片数据、不复制整个 volume，按原 z 索引分配文件名，在当前 Rayon 池最多启用两个 encoder/writer。每批全部完成后按切片顺序检查错误，返回最早切片的错误，不启动后续批次。失败批中的另一个文件可能已经创建或覆盖；保留部分文件，不删除已有用户输出。单 worker 或单切片批直接执行，单个多页 TIFF 仍顺序编码。`write_tiff_pages<W: Write + Seek>` 借用 writer，按顺序写页，释放 encoder 后显式 flush，两种输出模式都传播刷新错误。这仅确认缓冲写出，不等于 fsync 持久化。尺寸乘积及 u32 范围检查在创建输出前拒绝溢出。测试/基准与限制见 PLAN.Performance.md §60。

### RAW 汇总缓冲预留（2026-09-23）

RAW 平面大小、文件字节数和选定输出体素数均使用 checked arithmetic。首个选定切片成功解码后，以 `try_reserve_exact` 一次预留最终体素数，后续按序追加不再触发几何增长。首片格式错误仍先于预留返回，分配失败显式传播。双文件解码上限和 512 KiB 并行阈值保持不变。Vec 容量请求不是进程 RSS 上限，单片临时缓冲仍与最终输出共存。见 PLAN.Performance.md §63。

### ASCII STL 流式解析（PERF-18，2026-09-25）

ASCII STL 现在从 reader 逐行解析（`parse_ascii_stream_or_binary`），第一个三角形完成后不再缓存文件文本；512 字节嗅探、ASCII 优先、对同一字节的 binary 回退，以及 `load_stl_hashed` 的单遍摘要（仍读到 EOF）均不变。Release 测量（忽略测试 `ascii_stream_memory_benchmark`；200,000 facet、35.5 MB CRLF 文件、600,000 顶点、5 轮）：原整段解析峰值存活堆 114.2 MB（参考实现按精确大小读文件，原 `read_to_end` 的增长只会更高），流式为 78.8 MB，即节省约一个文件大小；耗时中位 0.346 s 对 0.371 s，慢约 7 %，来自逐行复制，处于共享机器噪声范围内。剩余峰值为网格与去重表。

`TiffPageEncoder`（2026-09-25）将 `write_tiff_pages` 的逐页循环公开，调用方可向同一个多页 TIFF 追加若干切片块；`write_tiff_pages` 也改用它，两者输出字节相同。调用方在释放编码器后自行 flush writer。
