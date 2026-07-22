use crate::error::{Result, RustMsptError};
use crate::types::Vec3;
use std::io::Write;
use std::path::Path;

pub const VTK_POLY_LINE: u8 = 4;
pub const VTK_TRIANGLE: u8 = 5;
pub const VTK_TETRA: u8 = 10;
pub const VTK_VOXEL: u8 = 11;
pub const VTK_HEXAHEDRON: u8 = 12;

// AI-FUNC-SUMMARY: Typed storage for one VTU DataArray; variants mirror the VTK scalar types the contract subset supports; side effects: none.
#[derive(Clone, Debug, PartialEq)]
pub enum ArrayData {
    U8(Vec<u8>),
    I32(Vec<i32>),
    I64(Vec<i64>),
    U32(Vec<u32>),
    U64(Vec<u64>),
    F32(Vec<f32>),
    F64(Vec<f64>),
}

impl ArrayData {
    // AI-FUNC-SUMMARY: Number of stored scalar values; returns usize; side effects: none.
    pub fn len(&self) -> usize {
        match self {
            ArrayData::U8(v) => v.len(),
            ArrayData::I32(v) => v.len(),
            ArrayData::I64(v) => v.len(),
            ArrayData::U32(v) => v.len(),
            ArrayData::U64(v) => v.len(),
            ArrayData::F32(v) => v.len(),
            ArrayData::F64(v) => v.len(),
        }
    }

    // AI-FUNC-SUMMARY: True when the array holds no values; returns bool; side effects: none.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    // AI-FUNC-SUMMARY: VTK type attribute string for this variant; returns &'static str; side effects: none.
    pub fn vtk_type(&self) -> &'static str {
        match self {
            ArrayData::U8(_) => "UInt8",
            ArrayData::I32(_) => "Int32",
            ArrayData::I64(_) => "Int64",
            ArrayData::U32(_) => "UInt32",
            ArrayData::U64(_) => "UInt64",
            ArrayData::F32(_) => "Float32",
            ArrayData::F64(_) => "Float64",
        }
    }

    // AI-FUNC-SUMMARY: Read one value widened to f64 (lossless for all supported types up to f64 semantics); returns f64; side effects: none.
    pub fn get_f64(&self, i: usize) -> f64 {
        match self {
            ArrayData::U8(v) => v[i] as f64,
            ArrayData::I32(v) => v[i] as f64,
            ArrayData::I64(v) => v[i] as f64,
            ArrayData::U32(v) => v[i] as f64,
            ArrayData::U64(v) => v[i] as f64,
            ArrayData::F32(v) => v[i] as f64,
            ArrayData::F64(v) => v[i],
        }
    }

    // AI-FUNC-SUMMARY: Read one value narrowed to i64 (floats truncate); returns i64; side effects: none.
    pub fn get_i64(&self, i: usize) -> i64 {
        match self {
            ArrayData::U8(v) => v[i] as i64,
            ArrayData::I32(v) => v[i] as i64,
            ArrayData::I64(v) => v[i],
            ArrayData::U32(v) => v[i] as i64,
            ArrayData::U64(v) => v[i] as i64,
            ArrayData::F32(v) => v[i] as i64,
            ArrayData::F64(v) => v[i] as i64,
        }
    }

    fn write_ascii(&self, out: &mut Vec<u8>) {
        match self {
            ArrayData::U8(v) => write_ascii_items(out, v),
            ArrayData::I32(v) => write_ascii_items(out, v),
            ArrayData::I64(v) => write_ascii_items(out, v),
            ArrayData::U32(v) => write_ascii_items(out, v),
            ArrayData::U64(v) => write_ascii_items(out, v),
            ArrayData::F32(v) => write_ascii_items(out, v),
            ArrayData::F64(v) => write_ascii_items(out, v),
        }
    }

    fn to_le_bytes(&self) -> Vec<u8> {
        match self {
            ArrayData::U8(v) => v.clone(),
            ArrayData::I32(v) => v.iter().flat_map(|x| x.to_le_bytes()).collect(),
            ArrayData::I64(v) => v.iter().flat_map(|x| x.to_le_bytes()).collect(),
            ArrayData::U32(v) => v.iter().flat_map(|x| x.to_le_bytes()).collect(),
            ArrayData::U64(v) => v.iter().flat_map(|x| x.to_le_bytes()).collect(),
            ArrayData::F32(v) => v.iter().flat_map(|x| x.to_le_bytes()).collect(),
            ArrayData::F64(v) => v.iter().flat_map(|x| x.to_le_bytes()).collect(),
        }
    }
}

fn write_ascii_items<T: std::fmt::Display>(out: &mut Vec<u8>, items: &[T]) {
    for (i, x) in items.iter().enumerate() {
        if i > 0 {
            out.push(b' ');
        }
        let _ = write!(out, "{x}");
    }
}

// AI-FUNC-SUMMARY: One named VTU data array with a component count; side effects: none.
#[derive(Clone, Debug, PartialEq)]
pub struct DataArray {
    pub name: String,
    pub components: usize,
    pub data: ArrayData,
}

impl DataArray {
    // AI-FUNC-SUMMARY: Convenience constructor with components = 1; returns DataArray; side effects: none.
    pub fn scalar(name: &str, data: ArrayData) -> Self {
        DataArray { name: name.to_string(), components: 1, data }
    }
}

// AI-FUNC-SUMMARY:
// Purpose: In-memory representation of the PLAN §7 contract VTU (one UnstructuredGrid piece; mixed cells share one point array).
// Inputs: built field-by-field by producers (mesh generation, snapshots, tests).
// Returns: N/A (data type).
// Side effects: None.
// Notes: `offsets` are VTK end-offsets into `connectivity`; `types` holds VTK cell type codes (VTK_TETRA etc.).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct VtuDoc {
    pub points: Vec<Vec3>,
    pub connectivity: Vec<i64>,
    pub offsets: Vec<i64>,
    pub types: Vec<u8>,
    pub point_data: Vec<DataArray>,
    pub cell_data: Vec<DataArray>,
    pub field_data: Vec<DataArray>,
}

impl VtuDoc {
    // AI-FUNC-SUMMARY: Number of cells (length of `types`); returns usize; side effects: none.
    pub fn num_cells(&self) -> usize {
        self.types.len()
    }

    // AI-FUNC-SUMMARY: Connectivity slice of cell i using VTK end-offsets; returns &[i64]; side effects: none.
    pub fn cell(&self, i: usize) -> &[i64] {
        let start = if i == 0 { 0 } else { self.offsets[i - 1] as usize };
        let end = self.offsets[i] as usize;
        &self.connectivity[start..end]
    }

    // AI-FUNC-SUMMARY: Look up a cell-data array by name; returns Option<&DataArray>; side effects: none.
    pub fn cell_array(&self, name: &str) -> Option<&DataArray> {
        self.cell_data.iter().find(|a| a.name == name)
    }

    // AI-FUNC-SUMMARY: Look up a point-data array by name; returns Option<&DataArray>; side effects: none.
    pub fn point_array(&self, name: &str) -> Option<&DataArray> {
        self.point_data.iter().find(|a| a.name == name)
    }

    // AI-FUNC-SUMMARY: Look up a field-data array by name; returns Option<&DataArray>; side effects: none.
    pub fn field_array(&self, name: &str) -> Option<&DataArray> {
        self.field_data.iter().find(|a| a.name == name)
    }

    // AI-FUNC-SUMMARY:
    // Purpose: Structural validation of the document before writing or after reading.
    // Returns: Ok(()) or InvalidMesh describing the first violation.
    // Side effects: None.
    // Notes: Checks offsets monotone and consistent with connectivity length, node indices in range, per-cell node counts matching the cell type, and data-array lengths against cell/point counts.
    pub fn validate(&self) -> Result<()> {
        if self.offsets.len() != self.types.len() {
            return Err(RustMsptError::InvalidMesh(format!(
                "vtu: offsets len {} != types len {}",
                self.offsets.len(),
                self.types.len()
            )));
        }
        let mut prev = 0i64;
        for (i, &off) in self.offsets.iter().enumerate() {
            if off < prev {
                return Err(RustMsptError::InvalidMesh(format!(
                    "vtu: offsets not monotone at cell {i}"
                )));
            }
            let n = (off - prev) as usize;
            let expected = match self.types[i] {
                VTK_TETRA => Some(4),
                VTK_TRIANGLE => Some(3),
                VTK_VOXEL | VTK_HEXAHEDRON => Some(8),
                VTK_POLY_LINE => None,
                _ => None,
            };
            if let Some(e) = expected {
                if n != e {
                    return Err(RustMsptError::InvalidMesh(format!(
                        "vtu: cell {i} type {} has {n} nodes, expected {e}",
                        self.types[i]
                    )));
                }
            }
            prev = off;
        }
        if prev as usize != self.connectivity.len() {
            return Err(RustMsptError::InvalidMesh(format!(
                "vtu: last offset {} != connectivity len {}",
                prev,
                self.connectivity.len()
            )));
        }
        let np = self.points.len() as i64;
        if let Some(&bad) = self.connectivity.iter().find(|&&c| c < 0 || c >= np) {
            return Err(RustMsptError::InvalidMesh(format!(
                "vtu: connectivity index {bad} out of range (points: {np})"
            )));
        }
        for a in &self.cell_data {
            if a.data.len() != self.num_cells() * a.components {
                return Err(RustMsptError::InvalidMesh(format!(
                    "vtu: cell array '{}' len {} != cells {} * components {}",
                    a.name,
                    a.data.len(),
                    self.num_cells(),
                    a.components
                )));
            }
        }
        for a in &self.point_data {
            if a.data.len() != self.points.len() * a.components {
                return Err(RustMsptError::InvalidMesh(format!(
                    "vtu: point array '{}' len {} != points {} * components {}",
                    a.name,
                    a.data.len(),
                    self.points.len(),
                    a.components
                )));
            }
        }
        Ok(())
    }
}

// AI-FUNC-SUMMARY: Output encoding for save_vtu (whitespace ascii or raw appended binary); side effects: none.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum VtuEncoding {
    Ascii,
    AppendedRaw,
}

struct ArrayRef<'a> {
    name: &'a str,
    components: usize,
    data: ArrayData,
    tuples: Option<usize>,
}

// AI-FUNC-SUMMARY:
// Purpose: Write a VtuDoc as a VTK XML UnstructuredGrid (.vtu) file in the PLAN §7 contract layout.
// Inputs: path (parent dirs created), doc (validated first), encoding (ascii or appended raw, LittleEndian, header UInt64).
// Returns: Ok(()) or an I/O / validation error.
// Side effects: Creates/overwrites the file at `path`.
// Notes: Ascii floats use Rust Display (shortest round-trip form), so ascii round-trips are exact.
pub fn save_vtu(path: &Path, doc: &VtuDoc, encoding: VtuEncoding) -> Result<()> {
    doc.validate()?;
    if let Some(parent) = path.parent() {
        if !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(parent)?;
        }
    }

    let mut flat_points = Vec::with_capacity(doc.points.len() * 3);
    for p in &doc.points {
        flat_points.extend_from_slice(&[p.x, p.y, p.z]);
    }

    // Arrays are collected in EMISSION order (FieldData, Points, Cells, PointData,
    // CellData) because appended-payload offsets are assigned by a running index
    // during emission; any order mismatch corrupts every offset after the first
    // out-of-order array.
    let mut arrays: Vec<ArrayRef> = Vec::new();
    for a in &doc.field_data {
        arrays.push(ArrayRef {
            name: &a.name,
            components: a.components,
            data: a.data.clone(),
            tuples: Some(a.data.len() / a.components.max(1)),
        });
    }
    arrays.push(ArrayRef {
        name: "Points",
        components: 3,
        data: ArrayData::F64(flat_points),
        tuples: None,
    });
    arrays.push(ArrayRef {
        name: "connectivity",
        components: 1,
        data: ArrayData::I64(doc.connectivity.clone()),
        tuples: None,
    });
    arrays.push(ArrayRef {
        name: "offsets",
        components: 1,
        data: ArrayData::I64(doc.offsets.clone()),
        tuples: None,
    });
    arrays.push(ArrayRef {
        name: "types",
        components: 1,
        data: ArrayData::U8(doc.types.clone()),
        tuples: None,
    });
    for a in &doc.point_data {
        arrays.push(ArrayRef { name: &a.name, components: a.components, data: a.data.clone(), tuples: None });
    }
    for a in &doc.cell_data {
        arrays.push(ArrayRef { name: &a.name, components: a.components, data: a.data.clone(), tuples: None });
    }

    let mut offsets_bytes: Vec<usize> = Vec::with_capacity(arrays.len());
    let mut appended: Vec<u8> = Vec::new();
    if encoding == VtuEncoding::AppendedRaw {
        for a in &arrays {
            offsets_bytes.push(appended.len());
            let bytes = a.data.to_le_bytes();
            appended.extend_from_slice(&(bytes.len() as u64).to_le_bytes());
            appended.extend_from_slice(&bytes);
        }
    }

    let mut out: Vec<u8> = Vec::new();
    let _ = writeln!(
        out,
        "<VTKFile type=\"UnstructuredGrid\" version=\"1.0\" byte_order=\"LittleEndian\" header_type=\"UInt64\">"
    );
    let _ = writeln!(out, "  <UnstructuredGrid>");

    let mut idx = 0usize;
    let mut emit = |out: &mut Vec<u8>, a: &ArrayRef, indent: &str| {
        let comp = if a.components != 1 {
            format!(" NumberOfComponents=\"{}\"", a.components)
        } else {
            String::new()
        };
        let tuples = match a.tuples {
            Some(t) => format!(" NumberOfTuples=\"{t}\""),
            None => String::new(),
        };
        match encoding {
            VtuEncoding::Ascii => {
                let _ = write!(
                    out,
                    "{indent}<DataArray type=\"{}\" Name=\"{}\"{comp}{tuples} format=\"ascii\">",
                    a.data.vtk_type(),
                    a.name
                );
                a.data.write_ascii(out);
                let _ = writeln!(out, "</DataArray>");
            }
            VtuEncoding::AppendedRaw => {
                let _ = writeln!(
                    out,
                    "{indent}<DataArray type=\"{}\" Name=\"{}\"{comp}{tuples} format=\"appended\" offset=\"{}\"/>",
                    a.data.vtk_type(),
                    a.name,
                    offsets_bytes[idx]
                );
            }
        }
        idx += 1;
    };

    let n_field = doc.field_data.len();
    let n_point = doc.point_data.len();
    let n_cell = doc.cell_data.len();
    let (field_a, rest) = arrays.split_at(n_field);
    let (points_a, rest) = rest.split_at(1);
    let (cells_a, rest) = rest.split_at(3);
    let (point_a, cell_a) = rest.split_at(n_point);
    debug_assert_eq!(cell_a.len(), n_cell);

    if !field_a.is_empty() {
        let _ = writeln!(out, "    <FieldData>");
        for a in field_a {
            emit(&mut out, a, "      ");
        }
        let _ = writeln!(out, "    </FieldData>");
    }
    let _ = writeln!(
        out,
        "    <Piece NumberOfPoints=\"{}\" NumberOfCells=\"{}\">",
        doc.points.len(),
        doc.num_cells()
    );
    let _ = writeln!(out, "      <Points>");
    for a in points_a {
        emit(&mut out, a, "        ");
    }
    let _ = writeln!(out, "      </Points>");
    let _ = writeln!(out, "      <Cells>");
    for a in cells_a {
        emit(&mut out, a, "        ");
    }
    let _ = writeln!(out, "      </Cells>");
    let _ = writeln!(out, "      <PointData>");
    for a in point_a {
        emit(&mut out, a, "        ");
    }
    let _ = writeln!(out, "      </PointData>");
    let _ = writeln!(out, "      <CellData>");
    for a in cell_a {
        emit(&mut out, a, "        ");
    }
    let _ = writeln!(out, "      </CellData>");
    let _ = writeln!(out, "    </Piece>");
    let _ = writeln!(out, "  </UnstructuredGrid>");
    if encoding == VtuEncoding::AppendedRaw {
        let _ = write!(out, "  <AppendedData encoding=\"raw\">\n_");
        out.extend_from_slice(&appended);
        let _ = write!(out, "\n  </AppendedData>\n");
    }
    let _ = writeln!(out, "</VTKFile>");

    std::fs::write(path, out)?;
    Ok(())
}

#[derive(Clone, Debug)]
struct RawArray {
    name: String,
    vtk_type: String,
    components: usize,
    section: Section,
    payload: Payload,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Section {
    Points,
    Cells,
    PointData,
    CellData,
    FieldData,
}

#[derive(Clone, Debug)]
enum Payload {
    Ascii(String),
    Appended(usize),
}

fn invalid(msg: impl Into<String>) -> RustMsptError {
    RustMsptError::InvalidMesh(format!("vtu: {}", msg.into()))
}

fn attr(tag: &str, key: &str) -> Option<String> {
    let pat = format!("{key}=\"");
    let start = tag.find(&pat)? + pat.len();
    let end = tag[start..].find('"')? + start;
    Some(tag[start..end].to_string())
}

// AI-FUNC-SUMMARY:
// Purpose: Read a .vtu file written in the contract subset (single piece, ascii or raw-appended, LittleEndian, uncompressed).
// Inputs: path of the file.
// Returns: VtuDoc (validated) or an error naming the unsupported construct.
// Side effects: None beyond reading the file.
// Notes: Accepts Float32/Float64 points, Int32/Int64/UInt* connectivity and offsets, UInt8/Int* types, header_type UInt32 or UInt64. Rejects compressed and base64 ("binary" format) files explicitly. Unknown sections/arrays outside the subset are preserved as point/cell/field arrays when decodable.
pub fn load_vtu(path: &Path) -> Result<VtuDoc> {
    let bytes = std::fs::read(path)?;

    let (header_str, raw_block): (&str, &[u8]) = match find_subslice(&bytes, b"<AppendedData") {
        Some(pos) => {
            let head = std::str::from_utf8(&bytes[..pos])
                .map_err(|_| invalid("non-UTF8 header"))?;
            let after = &bytes[pos..];
            let underscore = find_subslice(after, b"_")
                .ok_or_else(|| invalid("AppendedData without '_' marker"))?;
            let raw_start = pos + underscore + 1;
            let close = find_subslice(&bytes[raw_start..], b"</AppendedData>")
                .ok_or_else(|| invalid("unterminated AppendedData"))?;
            (head, &bytes[raw_start..raw_start + close])
        }
        None => {
            let head = std::str::from_utf8(&bytes).map_err(|_| invalid("non-UTF8 file"))?;
            (head, &[][..])
        }
    };

    let vtk_tag_end = header_str
        .find("<VTKFile")
        .and_then(|s| header_str[s..].find('>').map(|e| (s, s + e + 1)))
        .ok_or_else(|| invalid("missing <VTKFile> tag"))?;
    let vtk_tag = &header_str[vtk_tag_end.0..vtk_tag_end.1];
    if let Some(bo) = attr(vtk_tag, "byte_order") {
        if bo != "LittleEndian" {
            return Err(invalid(format!("unsupported byte_order {bo}")));
        }
    }
    if attr(vtk_tag, "compressor").is_some() {
        return Err(invalid("compressed VTU files are not supported"));
    }
    let header64 = match attr(vtk_tag, "header_type").as_deref() {
        Some("UInt64") => true,
        Some("UInt32") | None => false,
        Some(other) => return Err(invalid(format!("unsupported header_type {other}"))),
    };

    let arrays = collect_arrays(header_str)?;

    let mut doc = VtuDoc::default();
    let mut got_points = false;
    let mut conn: Option<ArrayData> = None;
    let mut offs: Option<ArrayData> = None;
    let mut typs: Option<ArrayData> = None;

    for ra in arrays {
        let data = decode_payload(&ra, raw_block, header64)?;
        match (ra.section, ra.name.as_str()) {
            (Section::Points, _) => {
                if ra.components != 3 {
                    return Err(invalid("Points array must have 3 components"));
                }
                let n = data.len() / 3;
                doc.points = (0..n)
                    .map(|i| Vec3 {
                        x: data.get_f64(3 * i),
                        y: data.get_f64(3 * i + 1),
                        z: data.get_f64(3 * i + 2),
                    })
                    .collect();
                got_points = true;
            }
            (Section::Cells, "connectivity") => conn = Some(data),
            (Section::Cells, "offsets") => offs = Some(data),
            (Section::Cells, "types") => typs = Some(data),
            (Section::Cells, other) => {
                return Err(invalid(format!("unexpected Cells array '{other}'")));
            }
            (Section::PointData, _) => doc.point_data.push(DataArray {
                name: ra.name,
                components: ra.components,
                data,
            }),
            (Section::CellData, _) => doc.cell_data.push(DataArray {
                name: ra.name,
                components: ra.components,
                data,
            }),
            (Section::FieldData, _) => doc.field_data.push(DataArray {
                name: ra.name,
                components: ra.components,
                data,
            }),
        }
    }

    if !got_points {
        return Err(invalid("missing Points array"));
    }
    let conn = conn.ok_or_else(|| invalid("missing connectivity"))?;
    let offs = offs.ok_or_else(|| invalid("missing offsets"))?;
    let typs = typs.ok_or_else(|| invalid("missing types"))?;
    doc.connectivity = (0..conn.len()).map(|i| conn.get_i64(i)).collect();
    doc.offsets = (0..offs.len()).map(|i| offs.get_i64(i)).collect();
    doc.types = (0..typs.len()).map(|i| typs.get_i64(i) as u8).collect();

    doc.validate()?;
    Ok(doc)
}

fn find_subslice(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack
        .windows(needle.len())
        .position(|w| w == needle)
}

fn section_range(text: &str, open: &str, close: &str) -> Option<(usize, usize)> {
    let s = text.find(open)?;
    let body = s + text[s..].find('>')? + 1;
    let e = text[body..].find(close)? + body;
    Some((body, e))
}

fn collect_arrays(header: &str) -> Result<Vec<RawArray>> {
    let mut out = Vec::new();
    let sections = [
        (Section::FieldData, "<FieldData", "</FieldData>"),
        (Section::Points, "<Points", "</Points>"),
        (Section::Cells, "<Cells", "</Cells>"),
        (Section::PointData, "<PointData", "</PointData>"),
        (Section::CellData, "<CellData", "</CellData>"),
    ];
    for (section, open, close) in sections {
        let Some((s, e)) = section_range(header, open, close) else {
            continue;
        };
        let body = &header[s..e];
        let mut pos = 0usize;
        while let Some(rel) = body[pos..].find("<DataArray") {
            let tag_start = pos + rel;
            let tag_end = body[tag_start..]
                .find('>')
                .ok_or_else(|| invalid("unterminated DataArray tag"))?
                + tag_start;
            let tag = &body[tag_start..=tag_end];
            let self_closing = tag.trim_end_matches('>').ends_with('/');
            let name = attr(tag, "Name").unwrap_or_default();
            let vtk_type = attr(tag, "type").ok_or_else(|| invalid("DataArray without type"))?;
            let components = attr(tag, "NumberOfComponents")
                .and_then(|c| c.parse::<usize>().ok())
                .unwrap_or(1);
            let format = attr(tag, "format").unwrap_or_else(|| "ascii".to_string());
            let payload = match format.as_str() {
                "ascii" => {
                    if self_closing {
                        Payload::Ascii(String::new())
                    } else {
                        let content_end = body[tag_end + 1..]
                            .find("</DataArray>")
                            .ok_or_else(|| invalid("unterminated ascii DataArray"))?
                            + tag_end
                            + 1;
                        let content = body[tag_end + 1..content_end].to_string();
                        pos = content_end;
                        out.push(RawArray {
                            name,
                            vtk_type,
                            components,
                            section,
                            payload: Payload::Ascii(content),
                        });
                        continue;
                    }
                }
                "appended" => {
                    let off = attr(tag, "offset")
                        .and_then(|o| o.parse::<usize>().ok())
                        .ok_or_else(|| invalid("appended DataArray without offset"))?;
                    Payload::Appended(off)
                }
                other => {
                    return Err(invalid(format!(
                        "unsupported DataArray format '{other}' (base64/compressed not supported)"
                    )));
                }
            };
            pos = tag_end + 1;
            out.push(RawArray { name, vtk_type, components, section, payload });
        }
    }
    Ok(out)
}

fn decode_payload(ra: &RawArray, raw: &[u8], header64: bool) -> Result<ArrayData> {
    match &ra.payload {
        Payload::Ascii(text) => decode_ascii(&ra.vtk_type, text),
        Payload::Appended(offset) => {
            let (len, data_start) = if header64 {
                if raw.len() < offset + 8 {
                    return Err(invalid("appended offset out of range"));
                }
                let mut b = [0u8; 8];
                b.copy_from_slice(&raw[*offset..offset + 8]);
                (u64::from_le_bytes(b) as usize, offset + 8)
            } else {
                if raw.len() < offset + 4 {
                    return Err(invalid("appended offset out of range"));
                }
                let mut b = [0u8; 4];
                b.copy_from_slice(&raw[*offset..offset + 4]);
                (u32::from_le_bytes(b) as usize, offset + 4)
            };
            if raw.len() < data_start + len {
                return Err(invalid("appended block truncated"));
            }
            decode_binary(&ra.vtk_type, &raw[data_start..data_start + len])
        }
    }
}

fn decode_ascii(vtk_type: &str, text: &str) -> Result<ArrayData> {
    fn parse<T: std::str::FromStr>(text: &str) -> Result<Vec<T>> {
        text.split_whitespace()
            .map(|t| {
                t.parse::<T>()
                    .map_err(|_| invalid(format!("bad ascii value '{t}'")))
            })
            .collect()
    }
    Ok(match vtk_type {
        "UInt8" => ArrayData::U8(parse(text)?),
        "Int32" => ArrayData::I32(parse(text)?),
        "Int64" => ArrayData::I64(parse(text)?),
        "UInt32" => ArrayData::U32(parse(text)?),
        "UInt64" => ArrayData::U64(parse(text)?),
        "Float32" => ArrayData::F32(parse(text)?),
        "Float64" => ArrayData::F64(parse(text)?),
        other => return Err(invalid(format!("unsupported DataArray type {other}"))),
    })
}

fn decode_binary(vtk_type: &str, bytes: &[u8]) -> Result<ArrayData> {
    fn chunks<const N: usize, T>(bytes: &[u8], f: impl Fn([u8; N]) -> T) -> Result<Vec<T>> {
        if !bytes.len().is_multiple_of(N) {
            return Err(invalid("appended block size not a multiple of item size"));
        }
        Ok(bytes
            .chunks_exact(N)
            .map(|c| {
                let mut b = [0u8; N];
                b.copy_from_slice(c);
                f(b)
            })
            .collect())
    }
    Ok(match vtk_type {
        "UInt8" => ArrayData::U8(bytes.to_vec()),
        "Int32" => ArrayData::I32(chunks(bytes, i32::from_le_bytes)?),
        "Int64" => ArrayData::I64(chunks(bytes, i64::from_le_bytes)?),
        "UInt32" => ArrayData::U32(chunks(bytes, u32::from_le_bytes)?),
        "UInt64" => ArrayData::U64(chunks(bytes, u64::from_le_bytes)?),
        "Float32" => ArrayData::F32(chunks(bytes, f32::from_le_bytes)?),
        "Float64" => ArrayData::F64(chunks(bytes, f64::from_le_bytes)?),
        other => return Err(invalid(format!("unsupported DataArray type {other}"))),
    })
}
