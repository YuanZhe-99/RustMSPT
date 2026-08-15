//! GA-4 snapshot framework: frozen stage names, emission gating, metadata
//! stamping, and the size WARN. The contract (naming, metadata, the `key` set)
//! is frozen in `SPEC_meshgen_contracts.md` §2.4/§3; this module is the producer
//! side. Stage drivers (G1-2 onward) call `emit_snapshot` at their boundaries;
//! `mesh-verify` parses the stage from a snapshot filename and cross-checks it
//! via [V12].

use crate::config::meshgen::SnapshotMode;
use crate::error::Result;
use crate::io::vtu::{
    save_vtu, ArrayData, DataArray, VtuDoc, VtuEncoding, VTK_POLY_LINE, VTK_TETRA, VTK_TRIANGLE,
};
use crate::types::Vec3;
use std::path::{Path, PathBuf};

// AI-FUNC-SUMMARY:
// Purpose: The frozen stage enumeration (SPEC_meshgen_contracts §2.4); also the snapshot index.
// Notes: The index and filename name are the contract; `Quality` is the only stage that
//   carries a round suffix (`s10_quality_r<N>`). Surface stages (Conditioned..Gapfield)
//   emit face/curve cells only; Sizing/Lattice previews write `cell_kind = 3` voxels.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Stage {
    Conditioned = 0,
    Features = 1,
    Arranged = 2,
    Gapfield = 3,
    Sizing = 4,
    Lattice = 5,
    Classified = 6,
    Snapped = 7,
    Cut = 8,
    Thin = 9,
    Quality = 10,
    Final = 11,
}

impl Stage {
    // AI-FUNC-SUMMARY: Numeric stage index (0..=11) matching `StageIndex` metadata; returns u8; side effects: none.
    pub fn index(self) -> u8 {
        self as u8
    }

    // AI-FUNC-SUMMARY: Human-readable filename stem (e.g. "arranged"); returns &'static str; side effects: none.
    pub fn name(self) -> &'static str {
        match self {
            Stage::Conditioned => "conditioned",
            Stage::Features => "features",
            Stage::Arranged => "arranged",
            Stage::Gapfield => "gapfield",
            Stage::Sizing => "sizing",
            Stage::Lattice => "lattice",
            Stage::Classified => "classified",
            Stage::Snapped => "snapped",
            Stage::Cut => "cut",
            Stage::Thin => "thin",
            Stage::Quality => "quality",
            Stage::Final => "final",
        }
    }

    // AI-FUNC-SUMMARY: Resolve a stage from its numeric index; returns Option<Stage>; side effects: none.
    pub fn from_index(i: u8) -> Option<Stage> {
        match i {
            0 => Some(Stage::Conditioned),
            1 => Some(Stage::Features),
            2 => Some(Stage::Arranged),
            3 => Some(Stage::Gapfield),
            4 => Some(Stage::Sizing),
            5 => Some(Stage::Lattice),
            6 => Some(Stage::Classified),
            7 => Some(Stage::Snapped),
            8 => Some(Stage::Cut),
            9 => Some(Stage::Thin),
            10 => Some(Stage::Quality),
            11 => Some(Stage::Final),
            _ => None,
        }
    }

    // AI-FUNC-SUMMARY: True for the `key` snapshot set {s02, s05, s08, s11}; returns bool; side effects: none.
    pub fn is_key(self) -> bool {
        matches!(
            self,
            Stage::Arranged | Stage::Lattice | Stage::Cut | Stage::Final
        )
    }

    // AI-FUNC-SUMMARY: True for surface stages (s00-s03) that emit face/curve cells only; returns bool; side effects: none.
    pub fn is_surface_stage(self) -> bool {
        matches!(
            self,
            Stage::Conditioned | Stage::Features | Stage::Arranged | Stage::Gapfield
        )
    }

    // AI-FUNC-SUMMARY:
    // Purpose: Parse the stage from a snapshot filename's `sNN` token.
    // Returns: Option<Stage> (None when the path is not a snapshot).
    // Side effects: None.
    // Notes: Used by `mesh-verify` to cross-check `StageIndex` vs filename ([V12]).
    //   Matches the first `sNN` (two digits) in the file stem.
    pub fn from_path(path: &Path) -> Option<Stage> {
        let stem = path.file_stem()?.to_str()?;
        let bytes = stem.as_bytes();
        let mut i = 0;
        while i + 2 < bytes.len() {
            if bytes[i] == b's' && bytes[i + 1].is_ascii_digit() && bytes[i + 2].is_ascii_digit() {
                let n = (bytes[i + 1] - b'0') * 10 + (bytes[i + 2] - b'0');
                if let Some(stage) = Stage::from_index(n) {
                    return Some(stage);
                }
            }
            i += 1;
        }
        None
    }
}

// AI-FUNC-SUMMARY: Whether a stage should be emitted under the given mode; returns bool; side effects: none.
pub fn should_emit(mode: SnapshotMode, stage: Stage) -> bool {
    match mode {
        SnapshotMode::None => false,
        SnapshotMode::Key => stage.is_key(),
        SnapshotMode::All => true,
    }
}

// AI-FUNC-SUMMARY:
// Purpose: The snapshot directory `<output_vtu_parent>/<stem>.debug/`.
// Returns: PathBuf to the debug directory.
// Side effects: None.
// Notes: Matches SPEC §3 (`<output_stem>.debug/`).
pub fn snapshot_dir(output_vtu: &Path) -> PathBuf {
    let stem = output_vtu.file_stem().unwrap_or_default();
    let mut dir = output_vtu
        .parent()
        .map(|p| p.to_path_buf())
        .unwrap_or_default();
    dir.push(format!("{}.debug", stem.to_string_lossy()));
    dir
}

// AI-FUNC-SUMMARY:
// Purpose: The full snapshot path `<dir>/<stem>_sNN_<name>.vtu` (Quality carries `_r<N>`).
// Returns: PathBuf.
// Side effects: None.
pub fn snapshot_path(output_vtu: &Path, stage: Stage, round: Option<u32>) -> PathBuf {
    let stem = output_vtu
        .file_stem()
        .unwrap_or_default()
        .to_string_lossy()
        .to_string();
    let name = match (stage, round) {
        (Stage::Quality, Some(r)) => format!("quality_r{r}"),
        (Stage::Quality, None) => "quality".to_string(),
        _ => stage.name().to_string(),
    };
    let fname = format!("{stem}_s{:02}_{}.vtu", stage.index(), name);
    snapshot_dir(output_vtu).join(fname)
}

// AI-FUNC-SUMMARY: Count cells by kind for the `Counts` metadata array; returns [points, tets, faces, curves]; side effects: none.
fn counts_of(doc: &VtuDoc) -> [i64; 4] {
    let (mut tets, mut faces, mut curves) = (0i64, 0i64, 0i64);
    for &t in &doc.types {
        match t {
            VTK_TETRA => tets += 1,
            VTK_TRIANGLE => faces += 1,
            VTK_POLY_LINE => curves += 1,
            _ => {}
        }
    }
    [doc.points.len() as i64, tets, faces, curves]
}

// AI-FUNC-SUMMARY: Replace (or append) one field-data array by name; side effects: mutates the doc's field_data.
fn set_field(doc: &mut VtuDoc, name: &str, components: usize, data: ArrayData) {
    if let Some(existing) = doc.field_data.iter_mut().find(|a| a.name == name) {
        existing.components = components;
        existing.data = data;
    } else {
        doc.field_data.push(DataArray {
            name: name.to_string(),
            components,
            data,
        });
    }
}

// AI-FUNC-SUMMARY:
// Purpose: Inputs that `stamp_metadata`/`emit_snapshot` need beyond the document itself.
// Notes: Bundled into a struct so `emit_snapshot` stays under clippy's argument limit
//   and stage drivers can build one `SnapshotMeta` and reuse it across rounds.
#[derive(Debug, Clone, Copy)]
pub struct SnapshotMeta {
    pub stage: Stage,
    pub round: Option<u32>,
    pub config_hash: u64,
    pub domain: (Vec3, Vec3),
    pub determinism_strict: bool,
    pub generator_version: [i32; 3],
}

impl SnapshotMeta {
    // AI-FUNC-SUMMARY: Convenience constructor for the common case (no IQD round, generator 0.1.0); returns SnapshotMeta; side effects: none.
    pub fn new(stage: Stage, config_hash: u64, domain: (Vec3, Vec3)) -> Self {
        SnapshotMeta {
            stage,
            round: None,
            config_hash,
            domain,
            determinism_strict: true,
            generator_version: [0, 1, 0],
        }
    }

    // AI-FUNC-SUMMARY: Builder-style setter for the determinism flag; returns Self; side effects: none.
    pub fn with_determinism(mut self, mode: crate::config::meshgen::DeterminismMode) -> Self {
        self.determinism_strict = matches!(mode, crate::config::meshgen::DeterminismMode::Strict);
        self
    }
}

// AI-FUNC-SUMMARY:
// Purpose: Stamp the full §2.4 metadata block onto a snapshot document.
// Inputs: the doc, and the snapshot metadata (stage, config hash, domain, determinism, generator version).
// Returns: Ok(()) (always succeeds; metadata is plain data).
// Side effects: Mutates `doc.field_data`, overwriting any prior metadata arrays.
// Notes: `Counts` is recomputed from the mesh so it always agrees with the cells;
//   the writer derives `NumberOfTuples` from `data.len()/components`, matching the
//   frozen fixture encoding.
pub fn stamp_metadata(doc: &mut VtuDoc, meta: &SnapshotMeta) {
    set_field(doc, "SchemaVersion", 1, ArrayData::I32(vec![1]));
    set_field(
        doc,
        "StageIndex",
        1,
        ArrayData::I32(vec![meta.stage.index() as i32]),
    );
    set_field(
        doc,
        "GeneratorVersion",
        1,
        ArrayData::I32(meta.generator_version.to_vec()),
    );
    set_field(doc, "ConfigHash", 1, ArrayData::U64(vec![meta.config_hash]));
    set_field(
        doc,
        "DomainMin",
        3,
        ArrayData::F64(vec![meta.domain.0.x, meta.domain.0.y, meta.domain.0.z]),
    );
    set_field(
        doc,
        "DomainMax",
        3,
        ArrayData::F64(vec![meta.domain.1.x, meta.domain.1.y, meta.domain.1.z]),
    );
    set_field(doc, "Counts", 1, ArrayData::I64(counts_of(doc).to_vec()));
    set_field(
        doc,
        "DeterminismMode",
        1,
        ArrayData::U8(vec![if meta.determinism_strict { 0 } else { 1 }]),
    );
}

// AI-FUNC-SUMMARY:
// Purpose: Write one snapshot as the pair requirement R4 defines - the delivered **tets-only
//   volume** under the plain name, and the mixed-cell contract document beside it as `_contract`.
// Inputs: the doc, the output VTU path (determines the debug dir + stem), the snapshot metadata,
//   and the VTU encoding.
// Returns: the path of the **delivered** file, which is what gets printed and opened.
// Side effects: Writes one or two `.vtu` files under `<stem>.debug/`.
// Notes: Ascii encoding is used when the output path ends in `.ascii.vtu`, otherwise appended-raw.
//
//   **Which file gets the plain name is the whole point (P-2.1).** The contract document is
//   mixed-cell by design - `VTK_TRIANGLE` cells carry the face-tag contract S9-S11 and the INP
//   export need, `VTK_POLY_LINE` cells carry the rim curves - and that makes it unreadable as
//   opened: ParaView's Feature Edges walks those triangle cells and draws a web over every
//   interface, which reads as a cracked mesh even though the volume underneath is watertight
//   (measured: free faces all on the domain box, none shared by more than two tets, no
//   non-manifold edge, on all nine acceptance cases). Handing someone that file and telling them
//   to open the companion answers a question they did not ask. So the deliverable is the volume,
//   region identity travels on it as a cell array, and the file that needs a filter says so in
//   its name.
//
//   The volume is *derived*, never authored - `volume_only` of the same document - so the two
//   cannot drift and their node numbering is identical, with `GlobalPointId` mapping back.
//
//   A document with no tets at all is a surface stage (s00-s03): there is no volume to deliver,
//   the document *is* the artefact, and naming it `_contract` would misdescribe it. The rule keys
//   off the document rather than the stage label, so a stage that later grows a volume needs no
//   change here.
pub fn emit_snapshot(
    doc: &mut VtuDoc,
    output_vtu: &Path,
    meta: &SnapshotMeta,
    encoding: VtuEncoding,
) -> Result<PathBuf> {
    stamp_metadata(doc, meta);
    let path = snapshot_path(output_vtu, meta.stage, meta.round);
    if let Some(dir) = path.parent() {
        if !dir.as_os_str().is_empty() {
            std::fs::create_dir_all(dir)?;
        }
    }
    let encoding = if path.to_string_lossy().ends_with(".ascii.vtu") {
        VtuEncoding::Ascii
    } else {
        encoding
    };
    doc.validate()?;
    let volume = crate::io::vtu::volume_only(doc);
    if volume.types.is_empty() {
        save_vtu(&path, doc, encoding)?;
        return Ok(path);
    }
    save_vtu(&contract_path(&path), doc, encoding)?;
    volume.validate()?;
    save_vtu(&path, &volume, encoding)?;
    Ok(path)
}

// AI-FUNC-SUMMARY:
// Purpose: The auxiliary mixed-cell document's path beside a delivered volume: `<stem>_contract.vtu`.
// Inputs: the delivered file's path.
// Returns: the auxiliary's path.
// Side effects: None.
// Notes: `Stage::from_path` scans the stem for `sNN`, so the suffix does not disturb `[V12]`'s
//   stage/filename cross-check - both files answer for the same stage, because they are the same
//   document.
pub fn contract_path(delivered: &Path) -> PathBuf {
    let stem = delivered
        .file_stem()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_default();
    delivered.with_file_name(format!("{stem}_contract.vtu"))
}

// AI-FUNC-SUMMARY:
// Purpose: The size-WARN gate (SPEC §3 / PLAN §14): warn before a run when
//   `snapshots: all` is combined with an estimate above 5 M tets.
// Inputs: the snapshot mode and a tet-count estimate.
// Returns: true when a WARN was printed.
// Side effects: Prints a WARN to stdout.
// Notes: ≈60 B/tet per volume snapshot; `snapshots: all` on a 20 M-tet run ≈ 1.2 GB.
pub fn warn_if_large(mode: SnapshotMode, n_tets_estimate: usize) -> bool {
    if matches!(mode, SnapshotMode::All) && n_tets_estimate > 5_000_000 {
        let gb = (n_tets_estimate as f64) * 60.0 / 1.073_741_824e9;
        println!(
            "[mesh] WARN: snapshots=all with ~{n_tets_estimate} tets; volume snapshots may total ~{gb:.1} GiB (60 B/tet). Consider snapshots=key."
        );
        true
    } else {
        false
    }
}
