# I/O Reference

This page documents `src/io/mod.rs`, content hashing in `hash.rs`, PNG output in `image.rs`, STL I/O in `stl.rs`, and TIFF/RAW volume I/O in `volume.rs`.

## Index

| Function | Location | Summary |
|---|---|---|
| `sha256_bytes` | `src/io/hash.rs:13` | SHA-256 of a byte slice as lowercase hex. |
| `sha256_file` | `src/io/hash.rs:25` | Streams a file through SHA-256, returning the hex digest and the byte count. |
| `hex_digest` | `src/io/hash.rs:42` | Renders a digest as lowercase hexadecimal. |
| `save_image` | `src/io/image.rs:11` | Validates and writes a top-row-first RGBA8 PNG. |
| `parse_ascii_vertex` | `src/io/stl.rs:9` | Parses one ASCII STL `vertex x y z` line into a `Vec3`. |
| `quantize_key` | `src/io/stl.rs:21` | Quantizes a vertex to a fixed-precision integer key for tolerant deduplication. |
| `dedup_vertex` | `src/io/stl.rs:31` | Deduplicates a vertex against an existing list via quantized key lookup. |
| `parse_ascii_stl` | `src/io/stl.rs:48` | Parses ASCII STL text into a `Mesh` with deduplicated vertices. |
| `parse_f32_le` | `src/io/stl.rs:93` | Parses little-endian `f32` bytes and upcasts to `f64`. |
| `parse_binary_stl` | `src/io/stl.rs:104` | Parses binary STL bytes into a `Mesh` with deduplicated vertices. |
| `looks_ascii_stl` | `src/io/stl.rs:166` | Heuristically detects whether bytes represent ASCII STL. |
| `load_stl` | `src/io/stl.rs:185` | Loads an STL file with automatic ASCII/binary detection. |
| `load_folder_stls` | `src/io/stl.rs:203` | Loads all STL files in a folder. |
| `load_stl_or_merge_folder` | `src/io/stl.rs:226` | Loads a single STL file, or merges all STLs in a directory into one mesh. |
| `save_stl` | `src/io/stl.rs:258` | Saves a mesh as a binary STL file. |
| `collect_sorted_files` | `src/io/volume.rs:50` | Collects regular files in a folder, sorted by name, optionally filtered by extension. |
| `resolve_slice_range` | `src/io/volume.rs:78` | Resolves an inclusive slice range from start/end indices, treating `-1` as "from beginning"/"to end". |
| `decode_raw_slice` | `src/io/volume.rs:107` | Decodes one raw image slice into `i64` values per bit depth, sign, and byte order. |
| `load_raw_folder` | `src/io/volume.rs:205` | Loads a `Volume3D` from a folder of raw binary slice files. |
| `tiff_decoding_to_i64` | `src/io/volume.rs:266` | Converts a TIFF `DecodingResult` into a `Vec<i64>` buffer plus its numeric type. |
| `load_tiff_file_with_range` | `src/io/volume.rs:286` | Loads a multi-page TIFF file into a `Volume3D` over an inclusive page range. |
| `load_tiff_file` | `src/io/volume.rs:354` | Loads a TIFF file (all pages) into a `Volume3D`. |
| `is_tiff_path` | `src/io/volume.rs:359` | Checks whether a path has a `.tif`/`.tiff` extension. |
| `load_tiff_or_folder` | `src/io/volume.rs:369` | Loads a TIFF volume from a file or folder (all pages/slices). |
| `load_tiff_or_folder_with_range` | `src/io/volume.rs:379` | Loads a TIFF volume from a file or folder over an inclusive slice range. |
| `write_tiff_slice` | `src/io/volume.rs:446` | Writes one z-slice of volume data into a TIFF encoder page. |
| `save_tiff_or_folder_with_ext` | `src/io/volume.rs:524` | Saves a `Volume3D` as a multi-page TIFF file or a folder of per-slice TIFF files, with configurable extension. |
| `save_tiff_or_folder` | `src/io/volume.rs:586` | Saves a `Volume3D` to TIFF file or folder sequence with the default `.tiff` extension. |

## Module role: `io/mod.rs`

`src/io/mod.rs` declares the `stl` and `volume` submodules and re-exports their public items at the `io::` path (`load_stl`, `load_folder_stls`, `load_stl_or_merge_folder`, `save_stl` from `stl`; `load_raw_folder`, `load_tiff_or_folder`, `load_tiff_or_folder_with_range`, `save_tiff_or_folder`, `save_tiff_or_folder_with_ext`, `ByteOrder`, `RawFolderSpec`, `Volume3D`, `VolumeNumericType` from `volume`). It contains no functions of its own.

## hash.rs

#### sha256_bytes

- **Signature:** `pub fn sha256_bytes(bytes: &[u8]) -> String`
- **Source:** `src/io/hash.rs:13`
- **Purpose:** Computes the SHA-256 digest of an in-memory byte slice.
- **Parameters:**
  - `bytes` — the content to hash.
- **Returns:** The digest as 64 lowercase hexadecimal characters.
- **Side effects:** None.
- **Notes:** Content identity for the placement record and run report. This is a cryptographic digest, unrelated to the crate's other "hash" -- `pipeline/meshgen.rs`'s `config_hash`, which is a non-cryptographic `u64` used only for change detection.

#### sha256_file

- **Signature:** `pub fn sha256_file(path: &Path) -> Result<(String, u64)>`
- **Source:** `src/io/hash.rs:25`
- **Purpose:** Hashes a file's contents without loading the whole file into memory.
- **Parameters:**
  - `path` — the file to read.
- **Returns:** `(lowercase hex digest, number of bytes hashed)`.
- **Side effects:** Opens and reads the file.
- **Notes:** Reads through a 64 KiB buffer, so file size is bounded by disk, not by memory. Returns `RustMsptError::Io` when the file cannot be opened or read -- a missing file is an error, never the digest of nothing. The returned length is what the report records as `bytes` for each input and output.

#### hex_digest

- **Signature:** `fn hex_digest(digest: &[u8]) -> String`
- **Source:** `src/io/hash.rs:42`
- **Purpose:** Formats a raw digest as lowercase hexadecimal.
- **Parameters:**
  - `digest` — the raw digest bytes.
- **Returns:** A `2 * digest.len()`-character lowercase hex string.
- **Side effects:** None.
- **Notes:** Private; shared by both public entry points so the two can never disagree on formatting.

## image.rs

#### save_image

- **Signature:** `pub fn save_image(path: &Path, image: &RenderedImage) -> Result<()>`
- **Purpose:** Validate positive dimensions, exact `width * height * 4` RGBA8 length, and a `.png` extension, then create parent directories and encode the image.
- **Returns:** `Ok(())`, or config/I/O/image errors. Overwrites the destination.

## stl.rs

This file implements STL (stereolithography) mesh I/O with automatic ASCII/binary format detection on read, and binary-only output on write. A shared vertex-deduplication scheme (quantize each coordinate to a 1e-6 grid, then key a `HashMap` on the resulting `(i64, i64, i64)` triple) collapses coincident vertices emitted redundantly per-triangle in both the ASCII and binary parsers into a single indexed vertex list — this is the same quantized-key deduplication pattern used for mesh clipping elsewhere in the geometry code.

### Public functions

#### load_stl

- **Signature:** `pub fn load_stl(path: &Path) -> Result<Mesh>`
- **Source:** `src/io/stl.rs:185`
- **Purpose:** Loads an STL file, auto-detecting ASCII vs. binary format.
- **Parameters:**
  - `path` — path to the `.stl` file.
- **Returns:** The parsed `Mesh`.
- **Side effects:** Reads the whole file into memory.
- **Notes:** Uses `looks_ascii_stl` to sniff the format. If the sniff says ASCII, it attempts `parse_ascii_stl`; if that parse fails (e.g. malformed content after a valid-looking header), it silently falls back to `parse_binary_stl` rather than propagating the ASCII error. Binary parsing is otherwise the terminal path.

#### load_folder_stls

- **Signature:** `pub fn load_folder_stls(folder: &Path) -> Result<Vec<(PathBuf, Mesh)>>`
- **Source:** `src/io/stl.rs:203`
- **Purpose:** Loads every `.stl` file (case-insensitive extension match) directly inside a folder.
- **Parameters:**
  - `folder` — directory to scan (non-recursive).
- **Returns:** A `Vec` of `(file path, parsed mesh)` pairs, in directory-iteration order (not sorted).
- **Side effects:** Reads the directory listing and every matching file from disk.
- **Notes:** Non-`.stl` entries and subdirectories are silently skipped. Iteration order is OS-dependent since results are not sorted, unlike `collect_sorted_files` in `volume.rs`.

#### load_stl_or_merge_folder

- **Signature:** `pub fn load_stl_or_merge_folder(path: &Path) -> Result<Mesh>`
- **Source:** `src/io/stl.rs:226`
- **Purpose:** Loads a single mesh from either one STL file, or by merging every STL file in a directory into one combined mesh.
- **Parameters:**
  - `path` — a file path (single STL) or directory path (folder of STLs to merge).
- **Returns:** The loaded (or merged) `Mesh`. When merging, vertex arrays are concatenated and each subsequent mesh's face indices are offset by the running vertex count so far.
- **Side effects:** Reads one or more files from disk.
- **Notes:** Returns `RustMsptError::InvalidConfig` if `path` is a directory containing no STL files. No cross-file vertex deduplication is performed during the merge — only within each individual file's own parse.

#### save_stl

- **Signature:** `pub fn save_stl(path: &Path, mesh: &Mesh, solid_name: &str) -> Result<()>`
- **Source:** `src/io/stl.rs:258`
- **Purpose:** Writes a mesh to disk as a binary STL file.
- **Parameters:**
  - `path` — destination file path.
  - `mesh` — the mesh to serialize.
  - `solid_name` — name written into the 80-byte binary header (truncated to 80 bytes if longer; UTF-8/ASCII bytes copied as-is).
- **Returns:** `Ok(())` on success.
- **Side effects:** Creates parent directories (`fs::create_dir_all`) if needed; creates/overwrites the file at `path`.
- **Notes:** Returns `RustMsptError::InvalidMesh` if the mesh is empty. Triangle normals are always written as zero vectors — no normal recomputation is performed. Vertex coordinates are downcast from `f64` to `f32` for the STL binary format, so round-trip precision is not preserved. The attribute byte count trailer per triangle is always written as `0u16`.

### Private helpers

#### parse_ascii_vertex

- **Signature:** `fn parse_ascii_vertex(line: &str) -> Option<Vec3>`
- **Source:** `src/io/stl.rs:9`
- **Purpose:** Parses a single trimmed line of ASCII STL text as a `vertex x y z` record.
- **Parameters:**
  - `line` — one line of STL text.
- **Returns:** `Some(Vec3)` if the line has exactly 4 whitespace-separated tokens with the first equal to `"vertex"` and the remaining three parsing as `f64`; otherwise `None`.
- **Side effects:** None.

#### quantize_key

- **Signature:** `fn quantize_key(v: Vec3) -> (i64, i64, i64)`
- **Source:** `src/io/stl.rs:21`
- **Purpose:** Produces a hashable, tolerance-quantized key for a vertex so near-identical floating point coordinates map to the same key.
- **Parameters:**
  - `v` — the vertex to quantize.
- **Returns:** `(x, y, z)` each coordinate multiplied by `1_000_000.0`, rounded, and cast to `i64` — i.e. quantized to a grid of 1e-6 units.
- **Side effects:** None.
- **Notes:** The `1e6` scale factor is a fixed constant; vertices whose true positions differ by less than roughly `5e-7` units on any axis are treated as identical.

#### dedup_vertex

- **Signature:** `fn dedup_vertex(vertices: &mut Vec<Vec3>, map: &mut HashMap<(i64, i64, i64), usize>, v: Vec3) -> usize`
- **Source:** `src/io/stl.rs:31`
- **Purpose:** Returns the index of an existing vertex matching `v`'s quantized key, or appends `v` as a new vertex and returns its new index.
- **Parameters:**
  - `vertices` — running vertex list, appended to on cache miss.
  - `map` — quantized-key → index cache, updated on cache miss.
  - `v` — the vertex to look up or insert.
- **Returns:** The index of `v` in `vertices` (pre-existing or freshly inserted).
- **Side effects:** Mutates `vertices` and `map` in place.

#### parse_ascii_stl

- **Signature:** `fn parse_ascii_stl(content: &str, path: &Path) -> Result<Mesh>`
- **Source:** `src/io/stl.rs:48`
- **Purpose:** Parses a full ASCII STL document into a deduplicated `Mesh`.
- **Parameters:**
  - `content` — the full STL text.
  - `path` — source path, used only for error messages.
- **Returns:** The parsed `Mesh` (deduplicated vertices, one `Triangle` per facet).
- **Side effects:** None (pure parse over the given string).
- **Notes:** Requires the (trimmed) content to start with `"solid"`, else returns `RustMsptError::InvalidMesh`. Parsing is line-oriented and ignores all lines that don't match the `vertex x y z` pattern (i.e. `facet normal`, `outer loop`, `endloop`, `endfacet`, `endsolid` lines are effectively skipped rather than validated) — every group of three consecutive `vertex` lines is taken as one triangle regardless of surrounding keywords. Returns `RustMsptError::InvalidMesh` if no vertices or faces were parsed.

#### parse_f32_le

- **Signature:** `fn parse_f32_le(bytes: &[u8]) -> f64`
- **Source:** `src/io/stl.rs:93`
- **Purpose:** Reads 4 bytes as a little-endian IEEE-754 `f32` and widens to `f64`.
- **Parameters:**
  - `bytes` — a 4-byte slice (indexes `[0..4]` directly; panics on shorter input).
- **Returns:** The decoded value as `f64`.
- **Side effects:** None.

#### parse_binary_stl

- **Signature:** `fn parse_binary_stl(bytes: &[u8], path: &Path) -> Result<Mesh>`
- **Source:** `src/io/stl.rs:104`
- **Purpose:** Parses the binary STL format (80-byte header, 4-byte triangle count, then 50 bytes per triangle) into a deduplicated `Mesh`.
- **Parameters:**
  - `bytes` — the full file contents.
  - `path` — source path, used only for error messages.
- **Returns:** The parsed `Mesh`.
- **Side effects:** None (pure parse over the given byte slice).
- **Notes:** Returns `RustMsptError::InvalidMesh` if the file is smaller than the 84-byte minimum header, or if the declared triangle count implies a file size larger than what's actually present (`84 + tri_count * 50`, computed with `saturating_mul` to avoid overflow on corrupt counts). Each 50-byte triangle record is: 12 bytes normal (skipped/ignored), 3×12 bytes vertices, 2 bytes attribute count (skipped). Normals in the file are not read into the mesh at all.

#### looks_ascii_stl

- **Signature:** `fn looks_ascii_stl(bytes: &[u8]) -> bool`
- **Source:** `src/io/stl.rs:166`
- **Purpose:** Heuristically sniffs whether a byte buffer is ASCII STL rather than binary STL.
- **Parameters:**
  - `bytes` — the file contents (or a prefix of them).
- **Returns:** `true` if the first 5 bytes are `"solid"` and a lowercase scan of the first 512 bytes contains both `"facet"` and `"vertex"`.
- **Side effects:** None.
- **Notes:** This is only a heuristic — a binary STL whose 80-byte header happens to start with `"solid"` (a known STL-format ambiguity) could pass the first check, but the `"facet"`/`"vertex"` substring test in the first 512 bytes usually disambiguates. `load_stl` further guards against a false positive by falling back to binary parsing if ASCII parsing fails.

## volume.rs

This file implements two independent 3D image formats used as inputs/outputs for the packing pipeline's pore/volume geometry: raw flat binary slice stacks (`RawFolderSpec` / `load_raw_folder`) and TIFF (single multi-page file or a folder of single-page files). Both converge on the common `Volume3D` representation.

### Types

#### VolumeNumericType

- **Signature:** `pub enum VolumeNumericType { U8, U16, U32, I8, I16, I32 }`
- **Source:** `src/io/volume.rs:8-16`
- **Purpose:** Tags the original per-voxel bit depth and signedness of a loaded volume, independent of the widened in-memory storage type.
- **Derives:** `Debug, Clone, Copy, PartialEq, Eq`.
- **Notes:** Used both to interpret/validate RAW byte decoding and TIFF sample types on load, and to pick the correct narrowing conversion (with range-checked `try_from`) when writing TIFF output in `write_tiff_slice`.

#### ByteOrder

- **Signature:** `pub enum ByteOrder { LittleEndian, BigEndian }`
- **Source:** `src/io/volume.rs:18-22`
- **Purpose:** Selects the byte order used to decode multi-byte samples in RAW volume files.
- **Derives:** `Debug, Clone, Copy, PartialEq, Eq`.
- **Notes:** Only consumed by `decode_raw_slice` (and thus `load_raw_folder`); TIFF files carry their own byte-order marker internally and are unaffected by this type.

#### Volume3D

- **Signature:**
  ```rust
  pub struct Volume3D {
      pub width: usize,
      pub height: usize,
      pub depth: usize,
      pub data: Vec<i64>,
      pub numeric_type: VolumeNumericType,
  }
  ```
- **Source:** `src/io/volume.rs:24-31`
- **Purpose:** The unified in-memory representation of a 3D scalar volume, regardless of source format (RAW or TIFF).

| Field | Type | Meaning |
|---|---|---|
| `width` | `usize` | Number of voxels along X (columns per slice). |
| `height` | `usize` | Number of voxels along Y (rows per slice). |
| `depth` | `usize` | Number of slices along Z. |
| `data` | `Vec<i64>` | Flat voxel buffer, length `width * height * depth`. Every source numeric type (`u8`/`u16`/`u32`/`i8`/`i16`/`i32`) is widened to `i64` on load so a single buffer type can represent any supported bit depth uniformly; `numeric_type` records what the original bit depth was so it can be narrowed back correctly on save. |
| `numeric_type` | `VolumeNumericType` | The original (pre-widening) sample type, used to range-check and narrow values when writing back out. |

> **Important:** `Volume3D::data` uses **z-major indexing**: `idx = z * width * height + y * width + x`. Each z-slice is a contiguous `width * height` block, and slices are laid out one after another. This is the layout every reader (`load_raw_folder`, `load_tiff_file_with_range`, `load_tiff_or_folder_with_range`) produces and every writer (`save_tiff_or_folder_with_ext`) consumes by slicing `data` into `width * height`-sized chunks per `z`. Code that indexes into `Volume3D::data` manually — including GPU/WGSL compute shaders — must match this z-major layout, not an x-major one, or voxel positions will be silently scrambled.

#### RawFolderSpec

- **Signature:**
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
- **Source:** `src/io/volume.rs:33-43`
- **Purpose:** Fully specifies how to interpret a folder of flat raw binary slice files as a `Volume3D`.

| Field | Type | Meaning |
|---|---|---|
| `folder` | `PathBuf` | Directory containing the raw slice files (all files in the folder are considered, unfiltered by extension). |
| `width` | `usize` | Voxels per row; must be `> 0`. |
| `height` | `usize` | Rows per slice; must be `> 0`. |
| `bits` | `u8` | Sample bit depth; must be `8`, `16`, or `32`. |
| `signed` | `bool` | Whether samples are signed integers. |
| `byte_order` | `ByteOrder` | Byte order for multi-byte samples (16/32-bit). |
| `slice_start` | `isize` | First slice index to load, inclusive; `-1` means "from the first file". |
| `slice_end` | `isize` | Last slice index to load, inclusive; `-1` means "through the last file". |

### Public functions

#### load_raw_folder

- **Signature:** `pub fn load_raw_folder(spec: &RawFolderSpec) -> Result<Volume3D>`
- **Source:** `src/io/volume.rs:205`
- **Purpose:** Loads a `Volume3D` by reading and decoding every file in a folder (within the requested slice range) as a fixed-size raw binary slice.
- **Parameters:**
  - `spec` — folder path, per-slice dimensions, sample format, and slice range.
- **Returns:** A `Volume3D` whose `depth` equals the number of slices actually loaded (`end - start + 1`) and whose `numeric_type` is derived from `spec.bits`/`spec.signed`.
- **Side effects:** Lists the directory and reads every selected file into memory.
- **Notes:** Returns `RustMsptError::InvalidConfig` if `width` or `height` is `0`, if `bits` is not one of `8`/`16`/`32`, or if any selected file's byte length doesn't exactly equal `width * height * bytes_per_pixel`. Files are sorted by file name (via `collect_sorted_files`) before the slice range is applied, so ordering depends on filename sort order, not file modification time or embedded slice numbers.

#### load_tiff_or_folder

- **Signature:** `pub fn load_tiff_or_folder(path: &Path) -> Result<Volume3D>`
- **Source:** `src/io/volume.rs:369`
- **Purpose:** Loads an entire TIFF volume (all pages, from a single multi-page file or all slices in a folder) with no range restriction.
- **Parameters:**
  - `path` — a TIFF file path or a directory of TIFF files.
- **Returns:** The full `Volume3D`.
- **Side effects:** Reads from disk (see `load_tiff_or_folder_with_range`).
- **Notes:** Thin wrapper: `load_tiff_or_folder_with_range(path, -1, -1)`.

#### load_tiff_or_folder_with_range

- **Signature:** `pub fn load_tiff_or_folder_with_range(path: &Path, slice_start: isize, slice_end: isize) -> Result<Volume3D>`
- **Source:** `src/io/volume.rs:379`
- **Purpose:** Loads a TIFF volume from either a single multi-page TIFF file or a folder of single/multi-page TIFF files, restricted to an inclusive slice/page range.
- **Parameters:**
  - `path` — a file path or directory path.
  - `slice_start`, `slice_end` — inclusive range bounds; `-1` on either end means "from the beginning" / "through the end".
- **Returns:** The decoded `Volume3D`, with `depth` equal to the number of pages/files actually loaded.
- **Side effects:** Reads one or more files from disk.
- **Notes:** If `path` is a file, delegates directly to `load_tiff_file_with_range` and the range addresses *pages within that file*. If `path` is a directory, it collects `.tif`/`.tiff` files (sorted by name via `collect_sorted_files`), resolves the range against *file count*, and loads each selected file in full via `load_tiff_file` (i.e. once in folder mode, per-file page ranges are not sub-selected — every page of every selected file is included). Returns `RustMsptError::InvalidConfig` if `path` is neither a file nor a directory, if the directory has no matching files, or if width/height/numeric-type are inconsistent across the selected files.

#### save_tiff_or_folder_with_ext

- **Signature:** `pub fn save_tiff_or_folder_with_ext(volume: &Volume3D, output: &Path, file_prefix: Option<&str>, file_extension: Option<&str>) -> Result<()>`
- **Source:** `src/io/volume.rs:524`
- **Purpose:** Saves a `Volume3D` either as one multi-page TIFF file or as a folder of individually numbered per-slice TIFF files, with a configurable output extension.
- **Parameters:**
  - `volume` — the volume to write.
  - `output` — destination path; if it has a `.tif`/`.tiff` extension (per `is_tiff_path`), a single multi-page file is written there; otherwise `output` is treated as (and created as) a folder.
  - `file_prefix` — folder-mode only: filename prefix for each slice file (default `"slice"`); slices are named `{prefix}_{z:04}.{extension}`.
  - `file_extension` — folder-mode only: file extension to use, must be `tif` or `tiff` (default `"tiff"`, case-insensitive comparison).
- **Returns:** `Ok(())` on success.
- **Side effects:** Creates parent/output directories as needed; creates/overwrites one or more files on disk.
- **Notes:** Returns `RustMsptError::InvalidConfig` if any of `width`/`height`/`depth` is `0`, if `volume.data.len() != width * height * depth`, or (folder mode) if `file_extension` resolves to anything other than `tif`/`tiff`. Each z-slice is extracted from `volume.data` as `data[z*slice_len .. (z+1)*slice_len]` — i.e. it relies on the z-major layout documented on `Volume3D`. Per-voxel values are range-checked and narrowed to the volume's `numeric_type` during encoding (see `write_tiff_slice`); values that don't fit produce `RustMsptError::InvalidConfig`.

#### save_tiff_or_folder

- **Signature:** `pub fn save_tiff_or_folder(volume: &Volume3D, output: &Path, file_prefix: Option<&str>) -> Result<()>`
- **Source:** `src/io/volume.rs:586`
- **Purpose:** Saves a `Volume3D` to TIFF file or folder sequence using the default `"tiff"` extension.
- **Parameters:**
  - `volume` — the volume to write.
  - `output` — destination file or folder path.
  - `file_prefix` — folder-mode filename prefix (see `save_tiff_or_folder_with_ext`).
- **Returns:** `Ok(())` on success.
- **Side effects:** Same as `save_tiff_or_folder_with_ext`.
- **Notes:** Thin wrapper: `save_tiff_or_folder_with_ext(volume, output, file_prefix, Some("tiff"))`.

### Private helpers

#### collect_sorted_files

- **Signature:** `fn collect_sorted_files(folder: &Path, extensions: Option<&[&str]>) -> Result<Vec<PathBuf>>`
- **Source:** `src/io/volume.rs:50`
- **Purpose:** Lists regular files directly inside a folder, optionally filtered to a set of lowercase extensions, sorted by file name.
- **Parameters:**
  - `folder` — directory to scan (non-recursive).
  - `extensions` — if `Some`, only files whose extension (lowercased) matches one of the given strings are kept; if `None`, all regular files are kept.
- **Returns:** A `Vec<PathBuf>` sorted by `file_name()`.
- **Side effects:** Reads the directory listing from disk. Directory-read errors on individual entries are silently discarded via `filter_map(|entry| entry.ok()...)`; only the top-level `fs::read_dir` error propagates.

#### resolve_slice_range

- **Signature:** `fn resolve_slice_range(total: usize, start: isize, end: isize) -> Result<(usize, usize)>`
- **Source:** `src/io/volume.rs:78`
- **Purpose:** Converts possibly-sentinel `(start, end)` slice/page bounds into a validated, concrete inclusive `(usize, usize)` range.
- **Parameters:**
  - `total` — total number of available items (files or pages).
  - `start` — requested first index, inclusive; `-1` (or any negative) means `0`.
  - `end` — requested last index, inclusive; `-1` (or any negative) means `total - 1`.
- **Returns:** `(s, e)` — the resolved inclusive bounds, both `usize`.
- **Side effects:** None.
- **Notes:** Returns `RustMsptError::InvalidConfig` if `total == 0` ("no files"), or if the resolved bounds are out of range or inverted (`s >= total || e >= total || s > e`). Any negative `start`/`end` other than `-1` is treated identically to `-1` (clamped to `0`/`total - 1`), not rejected.

#### decode_raw_slice

- **Signature:** `fn decode_raw_slice(bytes: &[u8], bits: u8, signed: bool, byte_order: ByteOrder) -> Result<Vec<i64>>`
- **Source:** `src/io/volume.rs:107`
- **Purpose:** Decodes a flat byte buffer into per-voxel `i64` values according to bit depth, signedness, and byte order.
- **Parameters:**
  - `bytes` — raw sample bytes for one slice.
  - `bits` — sample width: `8`, `16`, or `32`.
  - `signed` — whether to interpret samples as signed integers.
  - `byte_order` — byte order for 16/32-bit samples (ignored for 8-bit).
- **Returns:** A `Vec<i64>` with one entry per decoded sample, each widened from its native width (`u8`/`i8`/`u16`/`i16`/`u32`/`i32`) to `i64`.
- **Side effects:** None.
- **Notes:** Returns `RustMsptError::InvalidConfig` if `bytes.len()` isn't a multiple of the sample byte width (2 for 16-bit, 4 for 32-bit; 8-bit has no alignment constraint), or if `bits` isn't `8`/`16`/`32`.

#### tiff_decoding_to_i64

- **Signature:** `fn tiff_decoding_to_i64(decoded: DecodingResult) -> Result<(Vec<i64>, VolumeNumericType)>`
- **Source:** `src/io/volume.rs:266`
- **Purpose:** Converts the `tiff` crate's `DecodingResult` enum (one page's decoded pixel buffer) into the crate's own `(Vec<i64>, VolumeNumericType)` representation.
- **Parameters:**
  - `decoded` — the decoded image data from `tiff::decoder::Decoder::read_image`.
- **Returns:** A tuple of the widened `i64` values and the matching `VolumeNumericType` tag.
- **Side effects:** None.
- **Notes:** Handles `DecodingResult::{U8,U16,U32,I8,I16,I32}`; any other variant (e.g. `F32`/`F64` float TIFFs, or 64-bit integer variants) returns `RustMsptError::InvalidConfig` with an "Unsupported TIFF sample type" message — floating-point TIFF volumes are not supported.

#### load_tiff_file_with_range

- **Signature:** `fn load_tiff_file_with_range(path: &Path, slice_start: isize, slice_end: isize) -> Result<Volume3D>`
- **Source:** `src/io/volume.rs:286`
- **Purpose:** Loads an inclusive page range from a single multi-page TIFF file into a `Volume3D`.
- **Parameters:**
  - `path` — path to the `.tif`/`.tiff` file.
  - `slice_start`, `slice_end` — inclusive page-index bounds; `-1` means "from beginning"/"through end".
- **Returns:** A `Volume3D` covering the requested pages, with `depth` equal to the number of pages actually loaded.
- **Side effects:** Opens and reads the file from disk **twice**: once in a first pass that calls `decoder.next_image()` in a loop purely to count total pages (`more_images()`/`next_image()`), and a second time (a fresh `Decoder` over a fresh `File::open`) to actually seek to `start` and decode pages `start..=end`.
- **Notes:** Returns `RustMsptError::InvalidConfig` if any page's dimensions differ from the first loaded page's, or if any page's decoded numeric type differs from the first page's. On success, `numeric_type` is `Some` from the loop; the `unwrap_or(VolumeNumericType::U8)` fallback is unreachable in practice since `start..=end` is always non-empty (guaranteed by `resolve_slice_range`).

#### load_tiff_file

- **Signature:** `fn load_tiff_file(path: &Path) -> Result<Volume3D>`
- **Source:** `src/io/volume.rs:354`
- **Purpose:** Loads every page of a single TIFF file into a `Volume3D`.
- **Parameters:**
  - `path` — path to the `.tif`/`.tiff` file.
- **Returns:** The full `Volume3D` for that file.
- **Side effects:** Reads the file from disk (twice — see `load_tiff_file_with_range`).
- **Notes:** Thin wrapper: `load_tiff_file_with_range(path, -1, -1)`.

#### is_tiff_path

- **Signature:** `fn is_tiff_path(path: &Path) -> bool` — `src/io/volume.rs:359`. Checks for a `.tif`/`.tiff` extension (case-insensitive). No side effects.

#### write_tiff_slice

- **Signature:** `fn write_tiff_slice(encoder: &mut TiffEncoder<BufWriter<fs::File>>, width: u32, height: u32, ty: VolumeNumericType, slice: &[i64]) -> Result<()>`
- **Source:** `src/io/volume.rs:446`
- **Purpose:** Narrows one slice's `i64` voxel values back to their native bit width and writes them as one grayscale page via the TIFF encoder.
- **Parameters:**
  - `encoder` — the open multi-page TIFF encoder to append a page to.
  - `width`, `height` — page dimensions in pixels.
  - `ty` — the target numeric type to narrow to (`U8`/`U16`/`U32`/`I8`/`I16`/`I32`), each mapped to a corresponding `tiff::encoder::colortype` (`Gray8`, `Gray16`, `Gray32`, `GrayI8`, `GrayI16`, `GrayI32`).
  - `slice` — the `width * height`-length `i64` values for this page.
- **Returns:** `Ok(())` on success.
- **Side effects:** Writes one new image/page to the `encoder`'s underlying file stream.
- **Notes:** Uses `TryFrom` (`u8::try_from`, `i32::try_from`, etc.) to convert each `i64` value to the narrower target type; any value outside the target type's range returns `RustMsptError::InvalidConfig` ("Value out of range for ... TIFF output") rather than saturating or truncating silently.

## Cross-cutting notes

- **Vertex dedup vs. voxel widening are unrelated mechanisms that both exist for numerical robustness:** `stl.rs`'s `quantize_key`/`dedup_vertex` collapse near-duplicate floating-point vertex positions using a fixed 1e6 quantization scale; `volume.rs`'s widen-to-`i64` (`decode_raw_slice`, `tiff_decoding_to_i64`) instead exists so `Volume3D` can hold any of six integer sample types in one uniformly-typed buffer without lossy conversion, with `VolumeNumericType` tracking the original type for exact narrowing back out on save.
- **Two-pass TIFF range loading:** `load_tiff_file_with_range` fully re-opens and re-decodes the file's page count before doing the real (potentially range-restricted) decode pass, because the underlying `tiff` crate's `Decoder` only exposes forward iteration (`more_images`/`next_image`) with no random-access page count query.
- All `AI-FUNC-SUMMARY` comments in both files were checked against the code they annotate; none were found to be stale enough to warrant a `Doc note` callout.
