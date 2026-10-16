use crate::error::{Result, RustMsptError};
use std::fs;
use std::io::Write;
use std::path::Path;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Dtype {
    F8,
    F4,
    I8,
    I4,
    U4,
    U8,
}

impl Dtype {
    // AI-FUNC-SUMMARY: Map a numpy little-endian descr string to a supported dtype; returns the dtype or an InvalidConfig error naming the descr.
    fn parse(descr: &str) -> Result<Self> {
        match descr {
            "<f8" => Ok(Dtype::F8),
            "<f4" => Ok(Dtype::F4),
            "<i8" => Ok(Dtype::I8),
            "<i4" => Ok(Dtype::I4),
            "<u4" => Ok(Dtype::U4),
            "<u8" => Ok(Dtype::U8),
            other => Err(RustMsptError::InvalidConfig(format!(
                "npy dtype {other} is not supported (little-endian f8, f4, i8, i4, u4, u8)"
            ))),
        }
    }

    // AI-FUNC-SUMMARY: Byte width of one element of this dtype; returns 4 or 8; side effects: none.
    fn width(self) -> usize {
        match self {
            Dtype::F8 | Dtype::I8 | Dtype::U8 => 8,
            Dtype::F4 | Dtype::I4 | Dtype::U4 => 4,
        }
    }
}

// AI-FUNC-SUMMARY: One numpy array read from a .npy file: its shape and its raw little-endian payload with the dtype it was stored in; carries no I/O.
pub struct NpyArray {
    pub shape: Vec<usize>,
    dtype: Dtype,
    data: Vec<u8>,
}

impl NpyArray {
    // AI-FUNC-SUMMARY: Number of elements implied by the shape; returns the product (1 for a scalar); side effects: none.
    pub fn len(&self) -> usize {
        self.shape.iter().product()
    }

    // AI-FUNC-SUMMARY: Whether the array holds no elements; returns bool; side effects: none.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    // AI-FUNC-SUMMARY: The elements as f64, widening f4 exactly; returns the values or an error for an integer dtype.
    pub fn to_f64(&self) -> Result<Vec<f64>> {
        match self.dtype {
            Dtype::F8 => Ok(self
                .data
                .as_chunks::<8>()
                .0
                .iter()
                .map(|c| f64::from_le_bytes(*c))
                .collect()),
            Dtype::F4 => Ok(self
                .data
                .as_chunks::<4>()
                .0
                .iter()
                .map(|c| f64::from(f32::from_le_bytes(*c)))
                .collect()),
            _ => Err(RustMsptError::InvalidConfig(
                "expected a floating-point npy array".into(),
            )),
        }
    }

    // AI-FUNC-SUMMARY: The elements as u32 indices; returns them or an error for a float dtype, a negative value or one above u32::MAX.
    pub fn to_index(&self) -> Result<Vec<u32>> {
        let bad = || {
            RustMsptError::InvalidConfig("npy index array holds a negative or oversized value".into())
        };
        match self.dtype {
            Dtype::I8 => self
                .data
                .as_chunks::<8>()
                .0
                .iter()
                .map(|c| u32::try_from(i64::from_le_bytes(*c)).map_err(|_| bad()))
                .collect(),
            Dtype::U8 => self
                .data
                .as_chunks::<8>()
                .0
                .iter()
                .map(|c| u32::try_from(u64::from_le_bytes(*c)).map_err(|_| bad()))
                .collect(),
            Dtype::I4 => self
                .data
                .as_chunks::<4>()
                .0
                .iter()
                .map(|c| u32::try_from(i32::from_le_bytes(*c)).map_err(|_| bad()))
                .collect(),
            Dtype::U4 => Ok(self
                .data
                .as_chunks::<4>()
                .0
                .iter()
                .map(|c| u32::from_le_bytes(*c))
                .collect()),
            _ => Err(RustMsptError::InvalidConfig(
                "expected an integer npy array".into(),
            )),
        }
    }
}

// AI-FUNC-SUMMARY:
// Purpose: Read a version 1/2/3 .npy file holding a C-ordered little-endian numeric array.
// Inputs: path to the file.
// Returns: NpyArray with shape, dtype and payload, or an error for a bad magic, Fortran order, an unsupported dtype or a truncated payload.
// Side effects: Reads the whole file.
pub fn read_npy(path: &Path) -> Result<NpyArray> {
    let bytes = fs::read(path)?;
    let invalid = |why: &str| RustMsptError::InvalidConfig(format!("{}: {why}", path.display()));
    if bytes.len() < 10 || &bytes[..6] != b"\x93NUMPY" {
        return Err(invalid("not a .npy file"));
    }
    let (header_len, start) = match bytes[6] {
        1 => (u16::from_le_bytes([bytes[8], bytes[9]]) as usize, 10),
        2 | 3 => {
            if bytes.len() < 12 {
                return Err(invalid("truncated header"));
            }
            (u32::from_le_bytes(bytes[8..12].try_into().unwrap()) as usize, 12)
        }
        _ => return Err(invalid("unsupported .npy version")),
    };
    let end = start + header_len;
    if bytes.len() < end {
        return Err(invalid("truncated header"));
    }
    let header = std::str::from_utf8(&bytes[start..end]).map_err(|_| invalid("header is not text"))?;
    let descr = header_value(header, "descr").ok_or_else(|| invalid("no descr"))?;
    let descr = descr.trim().trim_matches(|c| c == '\'' || c == '"');
    let dtype = Dtype::parse(descr)?;
    let fortran = header_value(header, "fortran_order").ok_or_else(|| invalid("no fortran_order"))?;
    if fortran.trim().starts_with("True") {
        return Err(invalid("Fortran-ordered arrays are not supported"));
    }
    let shape_text = header_value(header, "shape").ok_or_else(|| invalid("no shape"))?;
    let shape_text = shape_text.trim();
    let inner = shape_text
        .strip_prefix('(')
        .and_then(|s| s.split(')').next())
        .ok_or_else(|| invalid("shape is not a tuple"))?;
    let shape = inner
        .split(',')
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(|s| s.parse::<usize>().map_err(|_| invalid("shape entry is not an integer")))
        .collect::<Result<Vec<_>>>()?;
    let count: usize = shape.iter().product();
    let need = count * dtype.width();
    if bytes.len() - end < need {
        return Err(invalid("payload shorter than its shape"));
    }
    Ok(NpyArray {
        shape,
        dtype,
        data: bytes[end..end + need].to_vec(),
    })
}

// AI-FUNC-SUMMARY: Text of one key's value in a numpy header dict, up to the next top-level comma; returns Some(text) or None when the key is absent.
fn header_value<'a>(header: &'a str, key: &str) -> Option<&'a str> {
    let at = header.find(&format!("'{key}'"))?;
    let rest = &header[at + key.len() + 2..];
    let rest = &rest[rest.find(':')? + 1..];
    if rest.trim_start().starts_with('(') {
        let open = rest.find('(')?;
        let close = rest.find(')')?;
        return Some(&rest[open..=close]);
    }
    Some(rest.split(',').next().unwrap_or(rest))
}

// AI-FUNC-SUMMARY: Write a version 1.0 .npy header for a C-ordered array of the given descr and shape, padded to 64 bytes; returns the header bytes.
fn npy_header(descr: &str, shape: &[usize]) -> Vec<u8> {
    let dims = match shape.len() {
        1 => format!("({},)", shape[0]),
        _ => format!(
            "({})",
            shape.iter().map(|d| d.to_string()).collect::<Vec<_>>().join(", ")
        ),
    };
    let mut dict = format!("{{'descr': '{descr}', 'fortran_order': False, 'shape': {dims}, }}");
    let total = 10 + dict.len() + 1;
    dict.push_str(&" ".repeat((64 - total % 64) % 64));
    dict.push('\n');
    let mut out = b"\x93NUMPY\x01\x00".to_vec();
    out.extend_from_slice(&(dict.len() as u16).to_le_bytes());
    out.extend_from_slice(dict.as_bytes());
    out
}

// AI-FUNC-SUMMARY: Write an f64 array as a C-ordered little-endian .npy file; returns Ok or an I/O / shape-mismatch error; side effects: creates or replaces the file.
pub fn write_npy_f64(path: &Path, shape: &[usize], values: &[f64]) -> Result<()> {
    if shape.iter().product::<usize>() != values.len() {
        return Err(RustMsptError::InvalidConfig("npy shape does not match data".into()));
    }
    let mut file = std::io::BufWriter::new(fs::File::create(path)?);
    file.write_all(&npy_header("<f8", shape))?;
    for v in values {
        file.write_all(&v.to_le_bytes())?;
    }
    file.flush()?;
    Ok(())
}

// AI-FUNC-SUMMARY: Write an i64 array as a C-ordered little-endian .npy file; returns Ok or an I/O / shape-mismatch error; side effects: creates or replaces the file.
pub fn write_npy_i64(path: &Path, shape: &[usize], values: &[i64]) -> Result<()> {
    if shape.iter().product::<usize>() != values.len() {
        return Err(RustMsptError::InvalidConfig("npy shape does not match data".into()));
    }
    let mut file = std::io::BufWriter::new(fs::File::create(path)?);
    file.write_all(&npy_header("<i8", shape))?;
    for v in values {
        file.write_all(&v.to_le_bytes())?;
    }
    file.flush()?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    // AI-FUNC-SUMMARY: Verify f64 and i64 arrays round-trip through write_npy_* and read_npy with their shapes and values.
    #[test]
    fn arrays_round_trip() {
        let dir = tempfile::tempdir().unwrap();
        let f = dir.path().join("f.npy");
        write_npy_f64(&f, &[2, 3], &[0.5, -1.0, 2.0, f64::NAN, 1e-300, 7.0]).unwrap();
        let back = read_npy(&f).unwrap();
        assert_eq!(back.shape, vec![2, 3]);
        let v = back.to_f64().unwrap();
        assert_eq!(v[0], 0.5);
        assert!(v[3].is_nan());
        let i = dir.path().join("i.npy");
        write_npy_i64(&i, &[4], &[0, 3, 7, 5_000_000_000]).unwrap();
        let back = read_npy(&i).unwrap();
        assert_eq!(back.shape, vec![4]);
        assert!(back.to_index().is_err());
        write_npy_i64(&i, &[1, 4], &[0, 3, 7, 9]).unwrap();
        assert_eq!(read_npy(&i).unwrap().to_index().unwrap(), vec![0, 3, 7, 9]);
    }
}
