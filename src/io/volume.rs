use crate::error::{Result, RustMsptError};
use std::fs;
use std::io::{BufReader, BufWriter};
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

#[derive(Debug, Clone)]
pub struct Volume3D {
    pub width: usize,
    pub height: usize,
    pub depth: usize,
    pub data: Vec<i64>,
    pub numeric_type: VolumeNumericType,
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
// Purpose: Decode one raw image slice from bytes according to bit depth (8/16/32), signedness, and byte order.
// Inputs: raw bytes and numeric interpretation parameters.
// Returns: Decoded scalar values as Vec<i64>.
// Side effects: None.
// Notes: Returns InvalidConfig for unsupported bit depths or misaligned byte lengths.
fn decode_raw_slice(bytes: &[u8], bits: u8, signed: bool, byte_order: ByteOrder) -> Result<Vec<i64>> {
    let mut out = Vec::new();
    match (bits, signed) {
        (8, false) => {
            out.reserve(bytes.len());
            for &v in bytes {
                out.push(v as i64);
            }
        }
        (8, true) => {
            out.reserve(bytes.len());
            for &v in bytes {
                out.push((v as i8) as i64);
            }
        }
        (16, false) => {
            if bytes.len() % 2 != 0 {
                return Err(RustMsptError::InvalidConfig(
                    "RAW byte length is not aligned to 16-bit samples".to_string(),
                ));
            }
            out.reserve(bytes.len() / 2);
            for chunk in bytes.chunks_exact(2) {
                let v = match byte_order {
                    ByteOrder::LittleEndian => u16::from_le_bytes([chunk[0], chunk[1]]),
                    ByteOrder::BigEndian => u16::from_be_bytes([chunk[0], chunk[1]]),
                };
                out.push(v as i64);
            }
        }
        (16, true) => {
            if bytes.len() % 2 != 0 {
                return Err(RustMsptError::InvalidConfig(
                    "RAW byte length is not aligned to 16-bit samples".to_string(),
                ));
            }
            out.reserve(bytes.len() / 2);
            for chunk in bytes.chunks_exact(2) {
                let v = match byte_order {
                    ByteOrder::LittleEndian => i16::from_le_bytes([chunk[0], chunk[1]]),
                    ByteOrder::BigEndian => i16::from_be_bytes([chunk[0], chunk[1]]),
                };
                out.push(v as i64);
            }
        }
        (32, false) => {
            if bytes.len() % 4 != 0 {
                return Err(RustMsptError::InvalidConfig(
                    "RAW byte length is not aligned to 32-bit samples".to_string(),
                ));
            }
            out.reserve(bytes.len() / 4);
            for chunk in bytes.chunks_exact(4) {
                let v = match byte_order {
                    ByteOrder::LittleEndian => {
                        u32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]])
                    }
                    ByteOrder::BigEndian => {
                        u32::from_be_bytes([chunk[0], chunk[1], chunk[2], chunk[3]])
                    }
                };
                out.push(v as i64);
            }
        }
        (32, true) => {
            if bytes.len() % 4 != 0 {
                return Err(RustMsptError::InvalidConfig(
                    "RAW byte length is not aligned to 32-bit samples".to_string(),
                ));
            }
            out.reserve(bytes.len() / 4);
            for chunk in bytes.chunks_exact(4) {
                let v = match byte_order {
                    ByteOrder::LittleEndian => {
                        i32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]])
                    }
                    ByteOrder::BigEndian => {
                        i32::from_be_bytes([chunk[0], chunk[1], chunk[2], chunk[3]])
                    }
                };
                out.push(v as i64);
            }
        }
        _ => {
            return Err(RustMsptError::InvalidConfig(
                "RAW bits must be 8/16/32".to_string(),
            ));
        }
    }
    Ok(out)
}

// AI-FUNC-SUMMARY:
// Purpose: Load a 3D volume from a folder of raw binary slice files.
// Inputs: RawFolderSpec with folder path, dimensions, bit depth, sign, byte order, and slice range.
// Returns: Volume3D with decoded data.
// Side effects: Reads all slice files from disk.
// Notes: Validates per-slice byte size against expected dimensions. Returns InvalidConfig for size mismatches.
pub fn load_raw_folder(spec: &RawFolderSpec) -> Result<Volume3D> {
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
    let expected_len = spec.width * spec.height * bytes_per_pixel;

    let mut data: Vec<i64> = Vec::new();
    for path in &files[start..=end] {
        let bytes = fs::read(path)?;
        if bytes.len() != expected_len {
            return Err(RustMsptError::InvalidConfig(format!(
                "RAW slice size mismatch for {}: expected {}, got {}",
                path.display(),
                expected_len,
                bytes.len()
            )));
        }
        let mut slice = decode_raw_slice(&bytes, spec.bits, spec.signed, spec.byte_order)?;
        data.append(&mut slice);
    }

    let depth = end - start + 1;
    let numeric_type = match (spec.bits, spec.signed) {
        (8, false) => VolumeNumericType::U8,
        (8, true) => VolumeNumericType::I8,
        (16, false) => VolumeNumericType::U16,
        (16, true) => VolumeNumericType::I16,
        (32, false) => VolumeNumericType::U32,
        (32, true) => VolumeNumericType::I32,
        _ => {
            return Err(RustMsptError::InvalidConfig(
                "RAW bits must be 8/16/32".to_string(),
            ))
        }
    };

    Ok(Volume3D {
        width: spec.width,
        height: spec.height,
        depth,
        data,
        numeric_type,
    })
}

// AI-FUNC-SUMMARY: Convert a TIFF DecodingResult into a Vec<i64> buffer and its numeric type; returns (data, type); side effects: None.
fn tiff_decoding_to_i64(decoded: DecodingResult) -> Result<(Vec<i64>, VolumeNumericType)> {
    match decoded {
        DecodingResult::U8(v) => Ok((v.into_iter().map(|x| x as i64).collect(), VolumeNumericType::U8)),
        DecodingResult::U16(v) => Ok((v.into_iter().map(|x| x as i64).collect(), VolumeNumericType::U16)),
        DecodingResult::U32(v) => Ok((v.into_iter().map(|x| x as i64).collect(), VolumeNumericType::U32)),
        DecodingResult::I8(v) => Ok((v.into_iter().map(|x| x as i64).collect(), VolumeNumericType::I8)),
        DecodingResult::I16(v) => Ok((v.into_iter().map(|x| x as i64).collect(), VolumeNumericType::I16)),
        DecodingResult::I32(v) => Ok((v.into_iter().map(|x| x as i64).collect(), VolumeNumericType::I32)),
        _ => Err(RustMsptError::InvalidConfig(
            "Unsupported TIFF sample type; supported: U8/U16/U32/I8/I16/I32".to_string(),
        )),
    }
}

// AI-FUNC-SUMMARY:
// Purpose: Load a multi-page TIFF file into a Volume3D with inclusive page range.
// Inputs: TIFF file path and [start, end] page range (-1 for begin/end).
// Returns: Decoded Volume3D.
// Side effects: Reads file from disk twice (once to count pages, once to decode).
// Notes: Validates consistent dimensions and numeric type across pages.
fn load_tiff_file_with_range(path: &Path, slice_start: isize, slice_end: isize) -> Result<Volume3D> {
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
    let mut data: Vec<i64> = Vec::new();
    let mut numeric_type: Option<VolumeNumericType> = None;

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

        let decoded = decoder.read_image()?;
        let (mut values, ty) = tiff_decoding_to_i64(decoded)?;
        if let Some(existing) = numeric_type {
            if existing != ty {
                return Err(RustMsptError::InvalidConfig(format!(
                    "TIFF page type mismatch in {}",
                    path.display()
                )));
            }
        } else {
            numeric_type = Some(ty);
        }
        data.append(&mut values);
        depth += 1;

        if page_idx < end {
            decoder.next_image()?;
        }
    }

    Ok(Volume3D {
        width,
        height,
        depth,
        data,
        numeric_type: numeric_type.unwrap_or(VolumeNumericType::U8),
    })
}

// AI-FUNC-SUMMARY: Load a TIFF file (all pages) into a Volume3D; returns Volume3D; side effects: Reads from disk.
fn load_tiff_file(path: &Path) -> Result<Volume3D> {
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
// Returns: Decoded Volume3D.
// Side effects: Reads files from disk.
// Notes: For files, loads multi-page TIFF with page range. For folders, loads TIFF sequence with slice range.
pub fn load_tiff_or_folder_with_range(path: &Path, slice_start: isize, slice_end: isize) -> Result<Volume3D> {
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
    let mut data = Vec::new();
    let mut numeric_type: Option<VolumeNumericType> = None;

    for file in &files[start..=end] {
        let vol = load_tiff_file(&file)?;
        if width == 0 {
            width = vol.width;
            height = vol.height;
            numeric_type = Some(vol.numeric_type);
        } else {
            if width != vol.width || height != vol.height {
                return Err(RustMsptError::InvalidConfig(format!(
                    "TIFF shape mismatch at {}",
                    file.display()
                )));
            }
            if numeric_type != Some(vol.numeric_type) {
                return Err(RustMsptError::InvalidConfig(format!(
                    "TIFF numeric type mismatch at {}",
                    file.display()
                )));
            }
        }

        depth += vol.depth;
        data.extend(vol.data);
    }

    Ok(Volume3D {
        width,
        height,
        depth,
        data,
        numeric_type: numeric_type.unwrap_or(VolumeNumericType::U8),
    })
}

// AI-FUNC-SUMMARY:
// Purpose: Write one z-slice of volume data into a TIFF encoder page.
// Inputs: encoder, dimensions, numeric type, and slice data buffer.
// Returns: Ok(()) on success.
// Side effects: Writes one TIFF page to the encoder stream.
// Notes: Returns InvalidConfig if values overflow the target numeric type.
fn write_tiff_slice(
    encoder: &mut TiffEncoder<BufWriter<fs::File>>,
    width: u32,
    height: u32,
    ty: VolumeNumericType,
    slice: &[i64],
) -> Result<()> {
    match ty {
        VolumeNumericType::U8 => {
            let mut v = Vec::with_capacity(slice.len());
            for &x in slice {
                let y = u8::try_from(x).map_err(|_| {
                    RustMsptError::InvalidConfig("Value out of range for u8 TIFF output".to_string())
                })?;
                v.push(y);
            }
            encoder.new_image::<colortype::Gray8>(width, height)?.write_data(&v)?;
        }
        VolumeNumericType::U16 => {
            let mut v = Vec::with_capacity(slice.len());
            for &x in slice {
                let y = u16::try_from(x).map_err(|_| {
                    RustMsptError::InvalidConfig("Value out of range for u16 TIFF output".to_string())
                })?;
                v.push(y);
            }
            encoder.new_image::<colortype::Gray16>(width, height)?.write_data(&v)?;
        }
        VolumeNumericType::U32 => {
            let mut v = Vec::with_capacity(slice.len());
            for &x in slice {
                let y = u32::try_from(x).map_err(|_| {
                    RustMsptError::InvalidConfig("Value out of range for u32 TIFF output".to_string())
                })?;
                v.push(y);
            }
            encoder.new_image::<colortype::Gray32>(width, height)?.write_data(&v)?;
        }
        VolumeNumericType::I8 => {
            let mut v = Vec::with_capacity(slice.len());
            for &x in slice {
                let y = i8::try_from(x).map_err(|_| {
                    RustMsptError::InvalidConfig("Value out of range for i8 TIFF output".to_string())
                })?;
                v.push(y);
            }
            encoder.new_image::<colortype::GrayI8>(width, height)?.write_data(&v)?;
        }
        VolumeNumericType::I16 => {
            let mut v = Vec::with_capacity(slice.len());
            for &x in slice {
                let y = i16::try_from(x).map_err(|_| {
                    RustMsptError::InvalidConfig("Value out of range for i16 TIFF output".to_string())
                })?;
                v.push(y);
            }
            encoder.new_image::<colortype::GrayI16>(width, height)?.write_data(&v)?;
        }
        VolumeNumericType::I32 => {
            let mut v = Vec::with_capacity(slice.len());
            for &x in slice {
                let y = i32::try_from(x).map_err(|_| {
                    RustMsptError::InvalidConfig("Value out of range for i32 TIFF output".to_string())
                })?;
                v.push(y);
            }
            encoder.new_image::<colortype::GrayI32>(width, height)?.write_data(&v)?;
        }
    }
    Ok(())
}

// AI-FUNC-SUMMARY:
// Purpose: Save a Volume3D as a multi-page TIFF file or as a folder of per-slice TIFF files.
// Inputs: volume, output path, optional file prefix for folder mode, optional file extension.
// Returns: Ok(()) on success.
// Side effects: Creates parent directories; writes TIFF file(s) to disk.
// Notes: Uses .tiff extension by default. Detects file vs folder mode by output extension. Returns error for empty volume or data length mismatch.
pub fn save_tiff_or_folder_with_ext(
    volume: &Volume3D,
    output: &Path,
    file_prefix: Option<&str>,
    file_extension: Option<&str>,
) -> Result<()> {
    if volume.width == 0 || volume.height == 0 || volume.depth == 0 {
        return Err(RustMsptError::InvalidConfig(
            "Cannot write empty volume".to_string(),
        ));
    }

    let expected = volume.width * volume.height * volume.depth;
    if volume.data.len() != expected {
        return Err(RustMsptError::InvalidConfig(format!(
            "Volume data length mismatch: expected {}, got {}",
            expected,
            volume.data.len()
        )));
    }

    let width = volume.width as u32;
    let height = volume.height as u32;
    let slice_len = volume.width * volume.height;

    if is_tiff_path(output) {
        if let Some(parent) = output.parent() {
            fs::create_dir_all(parent)?;
        }
        let file = fs::File::create(output)?;
        let writer = BufWriter::new(file);
        let mut encoder = TiffEncoder::new(writer)?;
        for z in 0..volume.depth {
            let start = z * slice_len;
            let end = start + slice_len;
            write_tiff_slice(&mut encoder, width, height, volume.numeric_type, &volume.data[start..end])?;
        }
        return Ok(());
    }

    fs::create_dir_all(output)?;
    let prefix = file_prefix.unwrap_or("slice");
    let extension = file_extension.unwrap_or("tiff").to_ascii_lowercase();
    if extension != "tif" && extension != "tiff" {
        return Err(RustMsptError::InvalidConfig(
            "TIFF folder output extension must be tif or tiff".to_string(),
        ));
    }
    for z in 0..volume.depth {
        let path = output.join(format!("{}_{:04}.{}", prefix, z, extension));
        let file = fs::File::create(path)?;
        let writer = BufWriter::new(file);
        let mut encoder = TiffEncoder::new(writer)?;
        let start = z * slice_len;
        let end = start + slice_len;
        write_tiff_slice(&mut encoder, width, height, volume.numeric_type, &volume.data[start..end])?;
    }

    Ok(())
}

// AI-FUNC-SUMMARY: Save a Volume3D to TIFF file or folder sequence with default .tiff extension; returns Ok(()); side effects: Creates directories and writes TIFF files to disk.
pub fn save_tiff_or_folder(volume: &Volume3D, output: &Path, file_prefix: Option<&str>) -> Result<()> {
    save_tiff_or_folder_with_ext(volume, output, file_prefix, Some("tiff"))
}
