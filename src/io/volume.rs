use crate::error::{Result, RustMsptError};
use rayon::prelude::*;
use std::fs;
use std::io::{BufReader, BufWriter, Seek, Write};
use std::path::{Path, PathBuf};
use tiff::decoder::{Decoder, DecodingResult};
use tiff::encoder::{colortype, TiffEncoder};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VolumeNumericType {
    U8,
    U16,
    U32,
    I8,
    I16,
    I32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ByteOrder {
    LittleEndian,
    BigEndian,
}

/// A voxel sample type a volume can be stored in. The six file types store in their own width;
/// `i64` is the wide type code that needs arbitrary values (placement labels, tests) uses.
pub trait Voxel: Copy + PartialEq + Send + Sync + std::fmt::Debug + 'static {
    // AI-FUNC-SUMMARY: Widen the sample to i64 exactly; returns i64; side effects: None.
    fn to_i64(self) -> i64;
    // AI-FUNC-SUMMARY: Narrow an i64 to this type; returns None when it does not fit; side effects: None.
    fn from_i64(value: i64) -> Option<Self>;
}

macro_rules! impl_voxel {
    ($($t:ty),*) => {$(
        impl Voxel for $t {
            fn to_i64(self) -> i64 { self as i64 }
            fn from_i64(value: i64) -> Option<Self> { <$t>::try_from(value).ok() }
        }
    )*};
}
impl_voxel!(u8, i8, u16, i16, u32, i32, i64);

/// A z-major voxel volume (`idx = z * width * height + y * width + x`) stored in `T`.
/// `numeric_type` is the file type the values belong to; for a typed volume it matches `T`.
#[derive(Debug, Clone)]
pub struct Volume3D<T = i64> {
    pub width: usize,
    pub height: usize,
    pub depth: usize,
    pub data: Vec<T>,
    pub numeric_type: VolumeNumericType,
}

impl<T: Voxel> Volume3D<T> {
    // AI-FUNC-SUMMARY: Widen every sample to i64 (one allocation of 8 bytes per voxel); returns Volume3D<i64>; side effects: consumes self.
    pub fn into_i64(self) -> Volume3D<i64> {
        Volume3D {
            width: self.width,
            height: self.height,
            depth: self.depth,
            data: self.data.into_iter().map(Voxel::to_i64).collect(),
            numeric_type: self.numeric_type,
        }
    }
}

/// A loaded volume in the width of its file type, so a 16-bit CT costs 2 bytes per voxel rather than 8.
#[derive(Debug, Clone)]
pub enum AnyVolume {
    U8(Volume3D<u8>),
    I8(Volume3D<i8>),
    U16(Volume3D<u16>),
    I16(Volume3D<i16>),
    U32(Volume3D<u32>),
    I32(Volume3D<i32>),
}

impl AnyVolume {
    // AI-FUNC-SUMMARY: Widen whatever type was loaded to a Volume3D<i64>; returns it; side effects: consumes self.
    pub fn into_i64(self) -> Volume3D<i64> {
        match self {
            AnyVolume::U8(v) => v.into_i64(),
            AnyVolume::I8(v) => v.into_i64(),
            AnyVolume::U16(v) => v.into_i64(),
            AnyVolume::I16(v) => v.into_i64(),
            AnyVolume::U32(v) => v.into_i64(),
            AnyVolume::I32(v) => v.into_i64(),
        }
    }

    // AI-FUNC-SUMMARY: The file numeric type of the loaded volume; returns VolumeNumericType; side effects: None.
    pub fn numeric_type(&self) -> VolumeNumericType {
        match self {
            AnyVolume::U8(_) => VolumeNumericType::U8,
            AnyVolume::I8(_) => VolumeNumericType::I8,
            AnyVolume::U16(_) => VolumeNumericType::U16,
            AnyVolume::I16(_) => VolumeNumericType::I16,
            AnyVolume::U32(_) => VolumeNumericType::U32,
            AnyVolume::I32(_) => VolumeNumericType::I32,
        }
    }
}

/// Samples of one file type accumulated while loading; appending another type is an error.
enum VoxelVec {
    U8(Vec<u8>),
    I8(Vec<i8>),
    U16(Vec<u16>),
    I16(Vec<i16>),
    U32(Vec<u32>),
    I32(Vec<i32>),
}

impl VoxelVec {
    // AI-FUNC-SUMMARY: Append another buffer of the same sample type, reserving `reserve` more samples first when the buffer is empty; returns false on a type mismatch; side effects: moves the other buffer's samples in.
    fn append(&mut self, other: VoxelVec) -> bool {
        match (self, other) {
            (VoxelVec::U8(a), VoxelVec::U8(mut b)) => a.append(&mut b),
            (VoxelVec::I8(a), VoxelVec::I8(mut b)) => a.append(&mut b),
            (VoxelVec::U16(a), VoxelVec::U16(mut b)) => a.append(&mut b),
            (VoxelVec::I16(a), VoxelVec::I16(mut b)) => a.append(&mut b),
            (VoxelVec::U32(a), VoxelVec::U32(mut b)) => a.append(&mut b),
            (VoxelVec::I32(a), VoxelVec::I32(mut b)) => a.append(&mut b),
            _ => return false,
        }
        true
    }

    // AI-FUNC-SUMMARY: Reserve exactly `additional` more samples, reporting allocation failure; returns Result; side effects: allocates.
    fn try_reserve_exact(&mut self, additional: usize) -> std::result::Result<(), std::collections::TryReserveError> {
        match self {
            VoxelVec::U8(v) => v.try_reserve_exact(additional),
            VoxelVec::I8(v) => v.try_reserve_exact(additional),
            VoxelVec::U16(v) => v.try_reserve_exact(additional),
            VoxelVec::I16(v) => v.try_reserve_exact(additional),
            VoxelVec::U32(v) => v.try_reserve_exact(additional),
            VoxelVec::I32(v) => v.try_reserve_exact(additional),
        }
    }

    // AI-FUNC-SUMMARY: The file numeric type of these samples; returns VolumeNumericType; side effects: None.
    fn numeric_type(&self) -> VolumeNumericType {
        match self {
            VoxelVec::U8(_) => VolumeNumericType::U8,
            VoxelVec::I8(_) => VolumeNumericType::I8,
            VoxelVec::U16(_) => VolumeNumericType::U16,
            VoxelVec::I16(_) => VolumeNumericType::I16,
            VoxelVec::U32(_) => VolumeNumericType::U32,
            VoxelVec::I32(_) => VolumeNumericType::I32,
        }
    }

    // AI-FUNC-SUMMARY: Wrap the samples as a typed volume of the given shape; returns AnyVolume; side effects: consumes self.
    fn into_volume(self, width: usize, height: usize, depth: usize) -> AnyVolume {
        let numeric_type = self.numeric_type();
        macro_rules! wrap {
            ($variant:ident, $data:expr) => {
                AnyVolume::$variant(Volume3D { width, height, depth, data: $data, numeric_type })
            };
        }
        match self {
            VoxelVec::U8(v) => wrap!(U8, v),
            VoxelVec::I8(v) => wrap!(I8, v),
            VoxelVec::U16(v) => wrap!(U16, v),
            VoxelVec::I16(v) => wrap!(I16, v),
            VoxelVec::U32(v) => wrap!(U32, v),
            VoxelVec::I32(v) => wrap!(I32, v),
        }
    }
}

impl AnyVolume {
    // AI-FUNC-SUMMARY: Split a typed volume back into its shape and samples for concatenation; returns (width, height, depth, VoxelVec); side effects: consumes self.
    fn into_parts(self) -> (usize, usize, usize, VoxelVec) {
        macro_rules! parts {
            ($v:expr, $variant:ident) => {
                ($v.width, $v.height, $v.depth, VoxelVec::$variant($v.data))
            };
        }
        match self {
            AnyVolume::U8(v) => parts!(v, U8),
            AnyVolume::I8(v) => parts!(v, I8),
            AnyVolume::U16(v) => parts!(v, U16),
            AnyVolume::I16(v) => parts!(v, I16),
            AnyVolume::U32(v) => parts!(v, U32),
            AnyVolume::I32(v) => parts!(v, I32),
        }
    }
}

#[derive(Debug, Clone)]
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

// AI-FUNC-SUMMARY: Decode at most two independent files within the current Rayon pool and consume results in filename order; preserve first ordered decode/validation error and drop all unconsumed buffers before returning. One-worker pools remain sequential; a multi-page decoder is never shared.
fn consume_file_batches<T: Send>(
    files: &[PathBuf],
    batch_limit: usize,
    load: impl Fn(&Path) -> Result<T> + Sync,
    mut consume: impl FnMut(&Path, T) -> Result<()>,
) -> Result<()> {
    let batch_size = rayon::current_num_threads().min(batch_limit.clamp(1, 2));
    for batch in files.chunks(batch_size) {
        let decoded: Vec<Result<T>> = if batch.len() == 1 {
            batch.iter().map(|path| load(path)).collect()
        } else {
            batch.par_iter().map(|path| load(path)).collect()
        };
        for (path, result) in batch.iter().zip(decoded) {
            consume(path, result?)?;
        }
    }
    Ok(())
}

// AI-FUNC-SUMMARY:
// Purpose: Collect regular files in a folder sorted by file name, optionally filtering by extension.
// Inputs: folder path and optional allowed extensions.
// Returns: Sorted Vec of file paths.
// Side effects: Reads directory listing from disk.
fn collect_sorted_files(folder: &Path, extensions: Option<&[&str]>) -> Result<Vec<PathBuf>> {
    let mut files: Vec<PathBuf> = fs::read_dir(folder)?
        .filter_map(|entry| entry.ok().map(|e| e.path()))
        .filter(|path| path.is_file())
        .filter(|path| {
            if let Some(allowed) = extensions {
                return path
                    .extension()
                    .map(|ext| {
                        let lower = ext.to_string_lossy().to_ascii_lowercase();
                        allowed.iter().any(|v| *v == lower)
                    })
                    .unwrap_or(false);
            }
            true
        })
        .collect();

    files.sort_by(|a, b| a.file_name().cmp(&b.file_name()));
    Ok(files)
}

// AI-FUNC-SUMMARY:
// Purpose: Resolve inclusive slice range from start/end indices, treating -1 as "from beginning" or "to end".
// Inputs: total count and [start, end] request as isize.
// Returns: Validated inclusive (start, end) range.
// Side effects: None.
// Notes: Returns InvalidConfig for empty folder or invalid/overlapping range.
fn resolve_slice_range(total: usize, start: isize, end: isize) -> Result<(usize, usize)> {
    if total == 0 {
        return Err(RustMsptError::InvalidConfig(
            "Input folder has no files".to_string(),
        ));
    }

    let s = if start < 0 { 0 } else { start as usize };
    let e = if end < 0 {
        total - 1
    } else {
        end as usize
    };

    if s >= total || e >= total || s > e {
        return Err(RustMsptError::InvalidConfig(format!(
            "Invalid slice range [{start}, {end}] for {total} files"
        )));
    }

    Ok((s, e))
}

// AI-FUNC-SUMMARY:
// Purpose: Decode one raw image slice from bytes into samples of its own type according to bit depth (8/16/32), signedness, and byte order.
// Inputs: raw bytes and numeric interpretation parameters.
// Returns: Decoded samples as a VoxelVec of the matching type.
// Side effects: None.
// Notes: Returns InvalidConfig for unsupported bit depths or misaligned byte lengths.
fn decode_raw_slice(bytes: &[u8], bits: u8, signed: bool, byte_order: ByteOrder) -> Result<VoxelVec> {
    let aligned = |width: usize| -> Result<()> {
        if bytes.len().is_multiple_of(width) {
            Ok(())
        } else {
            Err(RustMsptError::InvalidConfig(format!(
                "RAW byte length is not aligned to {}-bit samples",
                width * 8
            )))
        }
    };
    macro_rules! words {
        ($t:ty, $n:expr) => {{
            aligned($n)?;
            bytes
                .chunks_exact($n)
                .map(|chunk| {
                    let word: [u8; $n] = chunk.try_into().expect("chunks_exact yields whole samples");
                    match byte_order {
                        ByteOrder::LittleEndian => <$t>::from_le_bytes(word),
                        ByteOrder::BigEndian => <$t>::from_be_bytes(word),
                    }
                })
                .collect()
        }};
    }
    Ok(match (bits, signed) {
        (8, false) => VoxelVec::U8(bytes.to_vec()),
        (8, true) => VoxelVec::I8(bytes.iter().map(|&v| v as i8).collect()),
        (16, false) => VoxelVec::U16(words!(u16, 2)),
        (16, true) => VoxelVec::I16(words!(i16, 2)),
        (32, false) => VoxelVec::U32(words!(u32, 4)),
        (32, true) => VoxelVec::I32(words!(i32, 4)),
        _ => {
            return Err(RustMsptError::InvalidConfig(
                "RAW bits must be 8/16/32".to_string(),
            ));
        }
    })
}

// AI-FUNC-SUMMARY:
// Purpose: Load a 3D volume from a folder of raw binary slice files.
// Inputs: RawFolderSpec with folder path, dimensions, bit depth, sign, byte order, and slice range.
// Returns: Volume3D<i64> with decoded data (widened from `load_raw_folder_typed`).
// Side effects: Reads all slice files from disk.
// Notes: Costs 8 bytes per voxel; code that can work in the file's own type should call `load_raw_folder_typed`.
pub fn load_raw_folder(spec: &RawFolderSpec) -> Result<Volume3D> {
    load_raw_folder_typed(spec).map(AnyVolume::into_i64)
}

// AI-FUNC-SUMMARY:
// Purpose: Load a 3D volume from a folder of raw binary slice files, keeping each sample in its file type.
// Inputs: RawFolderSpec with folder path, dimensions, bit depth, sign, byte order, and slice range.
// Returns: AnyVolume in the width of the RAW sample type.
// Side effects: Reads all slice files from disk.
// Notes: RAW files below 512 KiB remain serial; otherwise at most two files decode in the current pool; ordered validation/assembly preserves range and numeric type; checked output sizing and one fallible reservation after the first valid slice avoid repeated assembly growth. Returns InvalidConfig for size mismatches.
pub fn load_raw_folder_typed(spec: &RawFolderSpec) -> Result<AnyVolume> {
    if spec.width == 0 || spec.height == 0 {
        return Err(RustMsptError::InvalidConfig(
            "RAW width/height must be > 0".to_string(),
        ));
    }
    let files = collect_sorted_files(&spec.folder, None)?;
    let (start, end) = resolve_slice_range(files.len(), spec.slice_start, spec.slice_end)?;

    let bytes_per_pixel = match spec.bits {
        8 => 1usize,
        16 => 2usize,
        32 => 4usize,
        _ => {
            return Err(RustMsptError::InvalidConfig(
                "RAW bits must be 8/16/32".to_string(),
            ))
        }
    };
    let pixels = spec.width.checked_mul(spec.height)
        .ok_or_else(|| RustMsptError::InvalidConfig("RAW slice dimensions overflow".into()))?;
    let expected_len = pixels.checked_mul(bytes_per_pixel)
        .ok_or_else(|| RustMsptError::InvalidConfig("RAW slice byte count overflow".into()))?;
    let depth = end - start + 1;
    let output_len = pixels.checked_mul(depth)
        .ok_or_else(|| RustMsptError::InvalidConfig("RAW volume dimensions overflow".into()))?;

    let mut data: Option<VoxelVec> = None;
    consume_file_batches(&files[start..=end], if expected_len >= 512 * 1024 { 2 } else { 1 }, |path| {
        let bytes = fs::read(path)?;
        if bytes.len() != expected_len {
            return Err(RustMsptError::InvalidConfig(format!(
                "RAW slice size mismatch for {}: expected {}, got {}",
                path.display(), expected_len, bytes.len()
            )));
        }
        decode_raw_slice(&bytes, spec.bits, spec.signed, spec.byte_order)
    }, |_, slice| {
        match data.as_mut() {
            None => {
                // Reserve only after a valid first slice, preserving malformed-file errors.
                let mut first = slice;
                first.try_reserve_exact(output_len - pixels).map_err(|error| {
                    RustMsptError::InvalidConfig(format!("RAW volume allocation failed: {error}"))
                })?;
                data = Some(first);
            }
            Some(buffer) => {
                if !buffer.append(slice) {
                    return Err(RustMsptError::InvalidConfig("RAW sample type changed between slices".into()));
                }
            }
        }
        Ok(())
    })?;

    let data = data.ok_or_else(|| RustMsptError::InvalidConfig("RAW folder has no slices in range".into()))?;
    Ok(data.into_volume(spec.width, spec.height, depth))
}

// AI-FUNC-SUMMARY: Keep a TIFF DecodingResult's samples in their own type (no widening copy); returns VoxelVec; side effects: None.
fn tiff_decoding_to_voxels(decoded: DecodingResult) -> Result<VoxelVec> {
    match decoded {
        DecodingResult::U8(v) => Ok(VoxelVec::U8(v)),
        DecodingResult::U16(v) => Ok(VoxelVec::U16(v)),
        DecodingResult::U32(v) => Ok(VoxelVec::U32(v)),
        DecodingResult::I8(v) => Ok(VoxelVec::I8(v)),
        DecodingResult::I16(v) => Ok(VoxelVec::I16(v)),
        DecodingResult::I32(v) => Ok(VoxelVec::I32(v)),
        _ => Err(RustMsptError::InvalidConfig(
            "Unsupported TIFF sample type; supported: U8/U16/U32/I8/I16/I32".to_string(),
        )),
    }
}

// AI-FUNC-SUMMARY:
// Purpose: Load a multi-page TIFF file into a Volume3D with inclusive page range.
// Inputs: TIFF file path and [start, end] page range (-1 for begin/end).
// Returns: Decoded AnyVolume in the pages' own sample type.
// Side effects: Reads file from disk twice (once to count pages, once to decode).
// Notes: Validates consistent dimensions and numeric type across pages.
fn load_tiff_file_with_range(path: &Path, slice_start: isize, slice_end: isize) -> Result<AnyVolume> {
    let file = fs::File::open(path)?;
    let mut decoder = Decoder::new(BufReader::new(file))?;

    let mut pages = 1usize;
    while decoder.more_images() {
        decoder.next_image()?;
        pages += 1;
    }

    let file = fs::File::open(path)?;
    let mut decoder = Decoder::new(BufReader::new(file))?;
    let (start, end) = resolve_slice_range(pages, slice_start, slice_end)?;

    for _ in 0..start {
        decoder.next_image()?;
    }

    let mut width = 0usize;
    let mut height = 0usize;
    let mut depth = 0usize;
    let mut data: Option<VoxelVec> = None;

    for page_idx in start..=end {
        let (w, h) = decoder.dimensions()?;
        let w = w as usize;
        let h = h as usize;
        if width == 0 {
            width = w;
            height = h;
        } else if width != w || height != h {
            return Err(RustMsptError::InvalidConfig(format!(
                "TIFF page shape mismatch in {}",
                path.display()
            )));
        }

        let values = tiff_decoding_to_voxels(decoder.read_image()?)?;
        match data.as_mut() {
            None => data = Some(values),
            Some(buffer) => {
                if !buffer.append(values) {
                    return Err(RustMsptError::InvalidConfig(format!(
                        "TIFF page type mismatch in {}",
                        path.display()
                    )));
                }
            }
        }
        depth += 1;

        if page_idx < end {
            decoder.next_image()?;
        }
    }

    let data = data.unwrap_or(VoxelVec::U8(Vec::new()));
    Ok(data.into_volume(width, height, depth))
}

// AI-FUNC-SUMMARY: Load a TIFF file (all pages) into an AnyVolume; returns AnyVolume; side effects: Reads from disk.
fn load_tiff_file(path: &Path) -> Result<AnyVolume> {
    load_tiff_file_with_range(path, -1, -1)
}

// AI-FUNC-SUMMARY: Check whether a path has a TIFF file extension (.tif or .tiff); returns bool; side effects: None.
fn is_tiff_path(path: &Path) -> bool {
    path.extension()
        .map(|ext| {
            let lower = ext.to_string_lossy().to_ascii_lowercase();
            lower == "tif" || lower == "tiff"
        })
        .unwrap_or(false)
}

// AI-FUNC-SUMMARY: Load a TIFF volume from file or folder (all pages/slices); returns Volume3D; side effects: Reads from disk.
pub fn load_tiff_or_folder(path: &Path) -> Result<Volume3D> {
    load_tiff_or_folder_with_range(path, -1, -1)
}

// AI-FUNC-SUMMARY:
// Purpose: Load a TIFF volume from file or folder with inclusive slice range.
// Inputs: input path and [start,end] range where -1 means begin/end.
// Returns: Decoded Volume3D<i64> (widened from `load_tiff_or_folder_typed_with_range`).
// Side effects: Reads files from disk.
// Notes: Costs 8 bytes per voxel; code that can work in the file's own type should call the typed loader.
pub fn load_tiff_or_folder_with_range(path: &Path, slice_start: isize, slice_end: isize) -> Result<Volume3D> {
    load_tiff_or_folder_typed_with_range(path, slice_start, slice_end).map(AnyVolume::into_i64)
}

// AI-FUNC-SUMMARY:
// Purpose: Load a TIFF volume from file or folder with inclusive slice range, keeping samples in their file type.
// Inputs: input path and [start,end] range where -1 means begin/end.
// Returns: Decoded AnyVolume.
// Side effects: Reads files from disk.
// Notes: Files advance pages serially; folders decode at most two independent complete files in the current pool and assemble in filename order. The decoder's own typed buffers are moved in, never widened.
pub fn load_tiff_or_folder_typed_with_range(path: &Path, slice_start: isize, slice_end: isize) -> Result<AnyVolume> {
    if path.is_file() {
        return load_tiff_file_with_range(path, slice_start, slice_end);
    }

    if !path.is_dir() {
        return Err(RustMsptError::InvalidConfig(format!(
            "Input path is neither file nor folder: {}",
            path.display()
        )));
    }

    let files = collect_sorted_files(path, Some(&["tif", "tiff"]))?;
    if files.is_empty() {
        return Err(RustMsptError::InvalidConfig(format!(
            "No TIFF files found in {}",
            path.display()
        )));
    }
    let (start, end) = resolve_slice_range(files.len(), slice_start, slice_end)?;

    let mut width = 0usize;
    let mut height = 0usize;
    let mut depth = 0usize;
    let mut data: Option<VoxelVec> = None;

    consume_file_batches(&files[start..=end], 2, load_tiff_file, |file, vol| {
        let (w, h, d, values) = vol.into_parts();
        if width == 0 {
            width = w;
            height = h;
        } else if width != w || height != h {
            return Err(RustMsptError::InvalidConfig(format!(
                "TIFF shape mismatch at {}",
                file.display()
            )));
        }
        match data.as_mut() {
            None => data = Some(values),
            Some(buffer) => {
                if !buffer.append(values) {
                    return Err(RustMsptError::InvalidConfig(format!(
                        "TIFF numeric type mismatch at {}",
                        file.display()
                    )));
                }
            }
        }
        depth += d;
        Ok(())
    })?;

    let data = data.unwrap_or(VoxelVec::U8(Vec::new()));
    Ok(data.into_volume(width, height, depth))
}

// AI-FUNC-SUMMARY:
// Purpose: Write one z-slice of volume data into a TIFF encoder page.
// Inputs: encoder, dimensions, numeric type, and slice data buffer.
// Returns: Ok(()) on success.
// Side effects: Writes one TIFF page to the encoder stream.
// Notes: Returns InvalidConfig if values overflow the target numeric type.
fn write_tiff_slice<W: Write + Seek, T: Voxel>(
    encoder: &mut TiffEncoder<W>,
    width: u32,
    height: u32,
    ty: VolumeNumericType,
    slice: &[T],
) -> Result<()> {
    match ty {
        VolumeNumericType::U8 => {
            let mut v = Vec::with_capacity(slice.len());
            for &x in slice {
                let y = u8::try_from(x.to_i64()).map_err(|_| {
                    RustMsptError::InvalidConfig("Value out of range for u8 TIFF output".to_string())
                })?;
                v.push(y);
            }
            encoder.new_image::<colortype::Gray8>(width, height)?.write_data(&v)?;
        }
        VolumeNumericType::U16 => {
            let mut v = Vec::with_capacity(slice.len());
            for &x in slice {
                let y = u16::try_from(x.to_i64()).map_err(|_| {
                    RustMsptError::InvalidConfig("Value out of range for u16 TIFF output".to_string())
                })?;
                v.push(y);
            }
            encoder.new_image::<colortype::Gray16>(width, height)?.write_data(&v)?;
        }
        VolumeNumericType::U32 => {
            let mut v = Vec::with_capacity(slice.len());
            for &x in slice {
                let y = u32::try_from(x.to_i64()).map_err(|_| {
                    RustMsptError::InvalidConfig("Value out of range for u32 TIFF output".to_string())
                })?;
                v.push(y);
            }
            encoder.new_image::<colortype::Gray32>(width, height)?.write_data(&v)?;
        }
        VolumeNumericType::I8 => {
            let mut v = Vec::with_capacity(slice.len());
            for &x in slice {
                let y = i8::try_from(x.to_i64()).map_err(|_| {
                    RustMsptError::InvalidConfig("Value out of range for i8 TIFF output".to_string())
                })?;
                v.push(y);
            }
            encoder.new_image::<colortype::GrayI8>(width, height)?.write_data(&v)?;
        }
        VolumeNumericType::I16 => {
            let mut v = Vec::with_capacity(slice.len());
            for &x in slice {
                let y = i16::try_from(x.to_i64()).map_err(|_| {
                    RustMsptError::InvalidConfig("Value out of range for i16 TIFF output".to_string())
                })?;
                v.push(y);
            }
            encoder.new_image::<colortype::GrayI16>(width, height)?.write_data(&v)?;
        }
        VolumeNumericType::I32 => {
            let mut v = Vec::with_capacity(slice.len());
            for &x in slice {
                let y = i32::try_from(x.to_i64()).map_err(|_| {
                    RustMsptError::InvalidConfig("Value out of range for i32 TIFF output".to_string())
                })?;
                v.push(y);
            }
            encoder.new_image::<colortype::GrayI32>(width, height)?.write_data(&v)?;
        }
    }
    Ok(())
}

/// Incremental multi-page TIFF encoder over a borrowed seekable writer.
pub struct TiffPageEncoder<'a, W: Write + Seek> {
    encoder: TiffEncoder<&'a mut W>,
    width: u32,
    height: u32,
    numeric_type: VolumeNumericType,
    slice_len: usize,
}

impl<'a, W: Write + Seek> TiffPageEncoder<'a, W> {
    // AI-FUNC-SUMMARY: Start a multi-page TIFF stream (writes the TIFF header) for width x height pages of one numeric type; returns the encoder or InvalidConfig for empty/over-u32 dimensions; side effects: writes the header to the writer.
    pub fn new(
        writer: &'a mut W,
        width: usize,
        height: usize,
        numeric_type: VolumeNumericType,
    ) -> Result<Self> {
        if width == 0 || height == 0 {
            return Err(RustMsptError::InvalidConfig(
                "Cannot write empty volume".to_string(),
            ));
        }
        let w = u32::try_from(width)
            .map_err(|_| RustMsptError::InvalidConfig("TIFF width exceeds u32".into()))?;
        let h = u32::try_from(height)
            .map_err(|_| RustMsptError::InvalidConfig("TIFF height exceeds u32".into()))?;
        let slice_len = width
            .checked_mul(height)
            .ok_or_else(|| RustMsptError::InvalidConfig("TIFF volume dimensions overflow".into()))?;
        Ok(Self {
            encoder: TiffEncoder::new(writer)?,
            width: w,
            height: h,
            numeric_type,
            slice_len,
        })
    }

    // AI-FUNC-SUMMARY: Append whole z-slices (z-major, width*height values each) as consecutive pages; returns InvalidConfig if data is not a whole number of slices or a value overflows the numeric type; side effects: writes pages to the stream.
    pub fn write_slices<T: Voxel>(&mut self, data: &[T]) -> Result<()> {
        if !data.len().is_multiple_of(self.slice_len) {
            return Err(RustMsptError::InvalidConfig(format!(
                "TIFF page data length {} is not a multiple of the slice size {}",
                data.len(),
                self.slice_len
            )));
        }
        for slice in data.chunks(self.slice_len) {
            write_tiff_slice(&mut self.encoder, self.width, self.height, self.numeric_type, slice)?;
        }
        Ok(())
    }
}

// AI-FUNC-SUMMARY: Encode ordered pages through a borrowed seekable writer and explicitly flush after dropping the TIFF encoder; propagate final buffered-write failures instead of relying on BufWriter::drop.
fn write_tiff_pages<W: Write + Seek, T: Voxel>(
    writer: &mut W, width: u32, height: u32, ty: VolumeNumericType,
    data: &[T], slice_len: usize,
) -> Result<()> {
    {
        let mut encoder = TiffPageEncoder {
            encoder: TiffEncoder::new(&mut *writer)?,
            width,
            height,
            numeric_type: ty,
            slice_len,
        };
        encoder.write_slices(data)?;
    }
    writer.flush()?;
    Ok(())
}

// AI-FUNC-SUMMARY:
// Purpose: Save a Volume3D as a multi-page TIFF file or as a folder of per-slice TIFF files.
// Inputs: volume, output path, optional file prefix for folder mode, optional file extension.
// Returns: Ok(()) on success.
// Side effects: Creates parent directories; writes TIFF file(s) to disk.
// Notes: Folder output uses at most two writers in the current pool and returns the first slice-ordered error after joining a batch; current-batch partial files may remain, later batches are not started. Multi-page output stays serial. Both modes explicitly flush.
pub fn save_tiff_or_folder_with_ext<T: Voxel>(
    volume: &Volume3D<T>,
    output: &Path,
    file_prefix: Option<&str>,
    file_extension: Option<&str>,
) -> Result<()> {
    if volume.width == 0 || volume.height == 0 || volume.depth == 0 {
        return Err(RustMsptError::InvalidConfig(
            "Cannot write empty volume".to_string(),
        ));
    }

    let expected = volume.width.checked_mul(volume.height).and_then(|n| n.checked_mul(volume.depth))
        .ok_or_else(|| RustMsptError::InvalidConfig("TIFF volume dimensions overflow".into()))?;
    if volume.data.len() != expected {
        return Err(RustMsptError::InvalidConfig(format!(
            "Volume data length mismatch: expected {}, got {}",
            expected,
            volume.data.len()
        )));
    }

    let width = u32::try_from(volume.width).map_err(|_| RustMsptError::InvalidConfig("TIFF width exceeds u32".into()))?;
    let height = u32::try_from(volume.height).map_err(|_| RustMsptError::InvalidConfig("TIFF height exceeds u32".into()))?;
    let slice_len = volume.width * volume.height;

    if is_tiff_path(output) {
        if let Some(parent) = output.parent() {
            fs::create_dir_all(parent)?;
        }
        let file = fs::File::create(output)?;
        let mut writer = BufWriter::new(file);
        return write_tiff_pages(&mut writer, width, height, volume.numeric_type, &volume.data, slice_len);
    }

    fs::create_dir_all(output)?;
    let prefix = file_prefix.unwrap_or("slice");
    let extension = file_extension.unwrap_or("tiff").to_ascii_lowercase();
    if extension != "tif" && extension != "tiff" {
        return Err(RustMsptError::InvalidConfig(
            "TIFF folder output extension must be tif or tiff".to_string(),
        ));
    }
    let batch_size = rayon::current_num_threads().min(2).min(volume.depth);
    for first in (0..volume.depth).step_by(batch_size) {
        let count = batch_size.min(volume.depth - first);
        let data = &volume.data[first * slice_len..(first + count) * slice_len];
        let write = |(local, slice): (usize, &[T])| -> Result<()> {
            let path = output.join(format!("{}_{:04}.{}", prefix, first + local, extension));
            let file = fs::File::create(path)?;
            let mut writer = BufWriter::new(file);
            write_tiff_pages(&mut writer, width, height, volume.numeric_type, slice, slice_len)
        };
        let results: Vec<Result<()>> = if count == 1 {
            data.chunks(slice_len).enumerate().map(write).collect()
        } else {
            data.par_chunks(slice_len).enumerate().map(write).collect()
        };
        for result in results { result?; }
    }

    Ok(())
}

// AI-FUNC-SUMMARY: Save a Volume3D to TIFF file or folder sequence with default .tiff extension; returns Ok(()); side effects: Creates directories and writes TIFF files to disk.
pub fn save_tiff_or_folder<T: Voxel>(volume: &Volume3D<T>, output: &Path, file_prefix: Option<&str>) -> Result<()> {
    save_tiff_or_folder_with_ext(volume, output, file_prefix, Some("tiff"))
}

#[cfg(test)]
mod file_batch_tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    // AI-FUNC-SUMMARY: Inject a final flush failure after a valid TIFF has been encoded and require the error to escape; compare encoded bytes to a successful reference writer.
    #[test]
    fn final_tiff_flush_failure_is_reported() {
        struct FailFlush(std::io::Cursor<Vec<u8>>);
        impl Write for FailFlush {
            // AI-FUNC-SUMMARY: Forward test writes into the in-memory TIFF stream.
            fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> { self.0.write(bytes) }
            // AI-FUNC-SUMMARY: Inject a deterministic final-output failure for error propagation coverage.
            fn flush(&mut self) -> std::io::Result<()> { Err(std::io::Error::other("injected final flush failure")) }
        }
        impl Seek for FailFlush {
            // AI-FUNC-SUMMARY: Preserve TIFF directory-offset seeking in the fault-injection writer.
            fn seek(&mut self, pos: std::io::SeekFrom) -> std::io::Result<u64> { self.0.seek(pos) }
        }
        let data=[1,2,3,4,5,6,7,8];
        let mut expected=std::io::Cursor::new(Vec::new());
        write_tiff_pages(&mut expected,2,2,VolumeNumericType::U8,&data,4).unwrap();
        let mut failing=FailFlush(std::io::Cursor::new(Vec::new()));
        let error=write_tiff_pages(&mut failing,2,2,VolumeNumericType::U8,&data,4).unwrap_err();
        assert!(error.to_string().contains("injected final flush failure"));
        assert_eq!(failing.0.into_inner(),expected.into_inner());
    }

    // AI-FUNC-SUMMARY: Check retained decoded buffers stay within the two-file bound, callbacks remain ordered, and a consumer error prevents loading subsequent batches.
    #[test]
    fn decoded_file_buffers_are_bounded_and_errors_stop_batches() {
        struct Buffer<'a>(&'a AtomicUsize);
        impl Drop for Buffer<'_> {
            // AI-FUNC-SUMMARY: Release the test-only retained-buffer counter when a decoded buffer is consumed or discarded.
            fn drop(&mut self) { self.0.fetch_sub(1, Ordering::SeqCst); }
        }
        let files: Vec<_> = (0..9).map(|i|PathBuf::from(i.to_string())).collect();
        for workers in [1,2,8] {
            let pool=rayon::ThreadPoolBuilder::new().num_threads(workers).build().unwrap();
            for fail in [false,true] {
                let live=AtomicUsize::new(0);let peak=AtomicUsize::new(0);let loaded=AtomicUsize::new(0);
                let mut seen=Vec::new();
                let result=pool.install(||consume_file_batches(&files, 2, |_| {
                    loaded.fetch_add(1,Ordering::SeqCst);
                    peak.fetch_max(live.fetch_add(1,Ordering::SeqCst)+1,Ordering::SeqCst);
                    Ok(Buffer(&live))
                }, |path,_buffer| {
                    seen.push(path.to_path_buf());
                    if fail {Err(RustMsptError::InvalidConfig("stop".into()))}else{Ok(())}
                }));
                assert_eq!(live.load(Ordering::SeqCst),0);
                assert!(peak.load(Ordering::SeqCst)<=workers.min(2));
                if fail {
                    assert!(result.is_err());assert_eq!(seen,&files[..1]);
                    assert_eq!(loaded.load(Ordering::SeqCst),workers.min(2));
                } else {result.unwrap();assert_eq!(seen,files);}
            }
        }
    }
}
