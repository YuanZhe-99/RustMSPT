use crate::error::{Result, RustMsptError};
use crate::io::vtu::{VtuDoc, VTK_POLY_LINE, VTK_TETRA, VTK_TRIANGLE};
use crate::types::{BoundingBox, Vec3};
use std::collections::HashMap;

// AI-FUNC-SUMMARY: Render-set membership of an extracted triangle; Face outranks Volume when coincident hits are deduplicated; side effects: none.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum SetKind {
    Volume,
    Face,
}

// AI-FUNC-SUMMARY: One world-space triangle ready for rendering (color, opacity, set); side effects: none.
#[derive(Clone, Debug)]
pub struct SceneTri {
    pub a: Vec3,
    pub b: Vec3,
    pub c: Vec3,
    pub color: [u8; 3],
    pub alpha: f64,
    pub set: SetKind,
}

// AI-FUNC-SUMMARY: One world-space line segment (feature curve or wireframe edge) with a color; side effects: none.
#[derive(Clone, Debug)]
pub struct SceneSegment {
    pub a: Vec3,
    pub b: Vec3,
    pub color: [u8; 3],
}

// AI-FUNC-SUMMARY: One highlighted world-space point marker; side effects: none.
#[derive(Clone, Debug)]
pub struct SceneMarker {
    pub p: Vec3,
    pub color: [u8; 3],
}

// AI-FUNC-SUMMARY:
// Purpose: Renderer-ready extraction of a contract VTU: shaded triangles, overlay segments, markers, and the framing bbox.
// Notes: `bbox` is the FULL document bbox (not the filtered subset) so camera framing stays stable across filter changes.
#[derive(Clone, Debug, Default)]
pub struct RenderScene {
    pub tris: Vec<SceneTri>,
    pub segments: Vec<SceneSegment>,
    pub markers: Vec<SceneMarker>,
    pub bbox: Option<BoundingBox>,
}

// AI-FUNC-SUMMARY: AND-composed cell filters per PLAN §9.4; attribute filters skip cells where the attribute is inapplicable (sentinel −1); side effects: none.
#[derive(Clone, Debug)]
pub enum SceneFilter {
    CellKind(Vec<u8>),
    Component(Vec<i64>),
    RegionKey(Vec<i64>),
    Partition(Vec<i64>),
    Regime(Vec<i64>),
    BackgroundOnly,
    ExcludeBackground,
    ArrayRange { array: String, min: f64, max: f64 },
    BBox(BoundingBox),
    ClipPlane { origin: Vec3, normal: Vec3 },
}

// AI-FUNC-SUMMARY: How extracted triangles are colored: fixed color, categorical palette over an integer cell array, or a sequential colormap over a scalar cell array; side effects: none.
#[derive(Clone, Debug)]
pub enum ColorMode {
    Uniform([u8; 3]),
    Categorical { array: String },
    Scalar { array: String, min: Option<f64>, max: Option<f64> },
}

// AI-FUNC-SUMMARY:
// Purpose: Full specification of one extraction pass (filters, coloring, opacities, overlay toggles, highlights).
// Notes: `opacity_overrides` maps region_key -> alpha for the volume set; missing keys use `volume_opacity`.
#[derive(Clone, Debug)]
pub struct SceneSpec {
    pub filters: Vec<SceneFilter>,
    pub color_mode: ColorMode,
    pub volume_opacity: f64,
    pub face_opacity: f64,
    pub opacity_overrides: HashMap<i64, f64>,
    pub show_faces: bool,
    pub show_curves: bool,
    pub wireframe: bool,
    pub max_wireframe_edges: usize,
    pub highlight_points: Vec<Vec3>,
}

impl Default for SceneSpec {
    fn default() -> Self {
        SceneSpec {
            filters: Vec::new(),
            color_mode: ColorMode::Categorical { array: "region_key".to_string() },
            volume_opacity: 1.0,
            face_opacity: 1.0,
            opacity_overrides: HashMap::new(),
            show_faces: true,
            show_curves: true,
            wireframe: false,
            max_wireframe_edges: 200_000,
            highlight_points: Vec::new(),
        }
    }
}

const CATEGORICAL_PALETTE: [[u8; 3]; 12] = [
    [77, 121, 168],
    [242, 142, 44],
    [225, 87, 89],
    [118, 183, 178],
    [89, 161, 79],
    [237, 201, 73],
    [176, 122, 161],
    [255, 157, 167],
    [156, 117, 95],
    [186, 176, 172],
    [78, 121, 110],
    [212, 166, 106],
];

const SENTINEL_COLOR: [u8; 3] = [128, 128, 128];

const CURVE_KIND_COLORS: [[u8; 3]; 4] = [
    [255, 140, 0],  // sharp
    [220, 20, 60],  // rim
    [199, 21, 133], // intersection
    [105, 105, 105],// box
];

// AI-FUNC-SUMMARY: Map a categorical integer to a palette color (negative/sentinel -> grey); returns [u8;3]; side effects: none.
pub fn categorical_color(value: i64) -> [u8; 3] {
    if value < 0 {
        SENTINEL_COLOR
    } else {
        CATEGORICAL_PALETTE[(value as usize) % CATEGORICAL_PALETTE.len()]
    }
}

// AI-FUNC-SUMMARY: Map a scalar in [min,max] onto a compact viridis approximation; returns [u8;3]; side effects: none.
pub fn scalar_color(value: f64, min: f64, max: f64) -> [u8; 3] {
    const STOPS: [[f64; 3]; 6] = [
        [68.0, 1.0, 84.0],
        [59.0, 82.0, 139.0],
        [33.0, 145.0, 140.0],
        [94.0, 201.0, 98.0],
        [253.0, 231.0, 37.0],
        [253.0, 231.0, 37.0],
    ];
    let t = if max > min { ((value - min) / (max - min)).clamp(0.0, 1.0) } else { 0.0 };
    let x = t * (STOPS.len() - 2) as f64;
    let i = (x as usize).min(STOPS.len() - 2);
    let f = x - i as f64;
    let mut c = [0u8; 3];
    for k in 0..3 {
        c[k] = (STOPS[i][k] + (STOPS[i + 1][k] - STOPS[i][k]) * f).round() as u8;
    }
    c
}

struct DocIndex<'a> {
    doc: &'a VtuDoc,
    region_key: Option<&'a crate::io::vtu::DataArray>,
    face_tag_key: Option<&'a crate::io::vtu::DataArray>,
    curve_id: Option<&'a crate::io::vtu::DataArray>,
    region_sets: Option<(Vec<i64>, Vec<i64>)>, // offsets, components
    face_tag_sets: Option<(Vec<i64>, Vec<i64>)>,
    curve_kinds: Option<Vec<i64>>,
}

impl<'a> DocIndex<'a> {
    fn new(doc: &'a VtuDoc) -> Self {
        let table = |off: &str, comp: &str| -> Option<(Vec<i64>, Vec<i64>)> {
            let o = doc.field_array(off)?;
            let c = doc.field_array(comp)?;
            Some((
                (0..o.data.len()).map(|i| o.data.get_i64(i)).collect(),
                (0..c.data.len()).map(|i| c.data.get_i64(i)).collect(),
            ))
        };
        DocIndex {
            doc,
            region_key: doc.cell_array("region_key"),
            face_tag_key: doc.cell_array("face_tag_key"),
            curve_id: doc.cell_array("curve_id"),
            region_sets: table("RegionSetOffsets", "RegionSetComponents"),
            face_tag_sets: table("FaceTagOffsets", "FaceTagComponents"),
            curve_kinds: doc
                .field_array("CurveKind")
                .map(|a| (0..a.data.len()).map(|i| a.data.get_i64(i)).collect()),
        }
    }

    fn set_members(table: &Option<(Vec<i64>, Vec<i64>)>, key: i64) -> Option<&[i64]> {
        let (offsets, comps) = table.as_ref()?;
        if key < 0 || key as usize >= offsets.len() {
            return None;
        }
        let start = if key == 0 { 0 } else { offsets[key as usize - 1] as usize };
        let end = offsets[key as usize] as usize;
        comps.get(start..end)
    }

    fn cell_centroid(&self, i: usize) -> Vec3 {
        let nodes = self.doc.cell(i);
        let mut c = Vec3::new(0.0, 0.0, 0.0);
        for &n in nodes {
            c = c.add(self.doc.points[n as usize]);
        }
        c.scale(1.0 / nodes.len() as f64)
    }

    fn is_background_tet(&self, i: usize) -> Option<bool> {
        let key = self.region_key?.data.get_i64(i);
        let members = Self::set_members(&self.region_sets, key)?;
        Some(members == [0])
    }

    fn cell_components(&self, i: usize) -> Option<Vec<i64>> {
        match self.doc.types[i] {
            VTK_TETRA => {
                let key = self.region_key?.data.get_i64(i);
                Self::set_members(&self.region_sets, key).map(|m| m.to_vec())
            }
            VTK_TRIANGLE => {
                let key = self.face_tag_key?.data.get_i64(i);
                Self::set_members(&self.face_tag_sets, key).map(|m| m.to_vec())
            }
            VTK_POLY_LINE => {
                let id = self.curve_id?.data.get_i64(i);
                let o = self.doc.field_array("CurveCompOffsets")?;
                let c = self.doc.field_array("CurveCompComponents")?;
                if id < 0 || id as usize >= o.data.len() {
                    return None;
                }
                let start = if id == 0 { 0 } else { o.data.get_i64(id as usize - 1) as usize };
                let end = o.data.get_i64(id as usize) as usize;
                Some((start..end).map(|k| c.data.get_i64(k)).collect())
            }
            _ => None,
        }
    }
}

fn require_array<'a>(
    doc: &'a VtuDoc,
    name: &str,
    producer: &str,
) -> Result<&'a crate::io::vtu::DataArray> {
    doc.cell_array(name).ok_or_else(|| {
        RustMsptError::InvalidConfig(format!(
            "filter/color needs cell array '{name}', which is absent from this VTU (produced by {producer})"
        ))
    })
}

fn cell_passes(idx: &DocIndex, filter: &SceneFilter, i: usize) -> Result<bool> {
    let kind = idx.doc.types[i];
    Ok(match filter {
        SceneFilter::CellKind(kinds) => {
            let semantic = match idx.doc.cell_array("cell_kind") {
                Some(arr) => arr.data.get_i64(i) as u8,
                None => match kind {
                    VTK_TETRA => 0,
                    VTK_TRIANGLE => 1,
                    VTK_POLY_LINE => 2,
                    _ => 3,
                },
            };
            kinds.contains(&semantic)
        }
        SceneFilter::Component(xs) => match idx.cell_components(i) {
            Some(members) => members.iter().any(|x| xs.contains(x)),
            None => true,
        },
        SceneFilter::RegionKey(keys) => {
            if kind != VTK_TETRA {
                true
            } else {
                let arr = require_array(idx.doc, "region_key", "mesh generation")?;
                keys.contains(&arr.data.get_i64(i))
            }
        }
        SceneFilter::Partition(ps) => {
            if kind != VTK_TETRA {
                true
            } else {
                let arr = require_array(idx.doc, "partition_id", "mesh generation")?;
                ps.contains(&arr.data.get_i64(i))
            }
        }
        SceneFilter::Regime(rs) => {
            if kind != VTK_TETRA {
                true
            } else {
                let arr = require_array(idx.doc, "regime", "mesh generation")?;
                rs.contains(&arr.data.get_i64(i))
            }
        }
        SceneFilter::BackgroundOnly => {
            if kind != VTK_TETRA {
                true
            } else {
                idx.is_background_tet(i).ok_or_else(|| {
                    RustMsptError::InvalidConfig(
                        "background filter needs region_key + RegionSet tables".to_string(),
                    )
                })?
            }
        }
        SceneFilter::ExcludeBackground => {
            if kind != VTK_TETRA {
                true
            } else {
                !idx.is_background_tet(i).ok_or_else(|| {
                    RustMsptError::InvalidConfig(
                        "background filter needs region_key + RegionSet tables".to_string(),
                    )
                })?
            }
        }
        SceneFilter::ArrayRange { array, min, max } => {
            if kind != VTK_TETRA {
                true
            } else {
                let arr = require_array(idx.doc, array, "mesh generation or mesh-verify --annotate")?;
                let v = arr.data.get_f64(i);
                v >= *min && v <= *max
            }
        }
        SceneFilter::BBox(bb) => bb.contains_point(idx.cell_centroid(i)),
        SceneFilter::ClipPlane { origin, normal } => {
            idx.cell_centroid(i).sub(*origin).dot(*normal) <= 0.0
        }
    })
}

fn tet_color(idx: &DocIndex, mode: &ColorMode, i: usize, scalar_range: (f64, f64)) -> Result<[u8; 3]> {
    Ok(match mode {
        ColorMode::Uniform(c) => *c,
        ColorMode::Categorical { array } => {
            let arr = require_array(idx.doc, array, "mesh generation")?;
            categorical_color(arr.data.get_i64(i))
        }
        ColorMode::Scalar { array, .. } => {
            let arr = require_array(idx.doc, array, "mesh generation or mesh-verify --annotate")?;
            scalar_color(arr.data.get_f64(i), scalar_range.0, scalar_range.1)
        }
    })
}

// AI-FUNC-SUMMARY:
// Purpose: Extract a RenderScene from a contract VTU: apply the AND-composed filter chain, pull boundary faces of the selected tet subset (crinkle-clip semantics), add tagged-face cells, curve segments, optional wireframe, and markers.
// Inputs: doc (contract VTU), spec (filters, coloring, opacities, toggles).
// Returns: RenderScene, or InvalidConfig naming the missing array when a filter/color mode needs one the file lacks.
// Side effects: None.
// Notes: Boundary faces are faces referenced by exactly one SELECTED tet, so bbox/clip filters expose interior faces automatically. The scene bbox is the full document bbox for stable framing.
pub fn build_scene(doc: &VtuDoc, spec: &SceneSpec) -> Result<RenderScene> {
    doc.validate()?;
    let idx = DocIndex::new(doc);
    let n = doc.num_cells();

    let mut selected = vec![true; n];
    for f in &spec.filters {
        for (i, sel) in selected.iter_mut().enumerate() {
            if *sel {
                *sel = cell_passes(&idx, f, i)?;
            }
        }
    }

    let scalar_range = if let ColorMode::Scalar { array, min, max } = &spec.color_mode {
        let arr = require_array(doc, array, "mesh generation or mesh-verify --annotate")?;
        let mut lo = f64::INFINITY;
        let mut hi = f64::NEG_INFINITY;
        for i in 0..n {
            if doc.types[i] == VTK_TETRA {
                let v = arr.data.get_f64(i);
                lo = lo.min(v);
                hi = hi.max(v);
            }
        }
        (min.unwrap_or(lo), max.unwrap_or(hi))
    } else {
        (0.0, 1.0)
    };

    let mut scene = RenderScene::default();

    let mut face_count: HashMap<[i64; 3], (u32, usize)> = HashMap::new();
    for (i, sel) in selected.iter().enumerate() {
        if doc.types[i] == VTK_TETRA && *sel {
            let c = doc.cell(i);
            const TET_FACES: [[usize; 3]; 4] = [[0, 1, 2], [0, 1, 3], [0, 2, 3], [1, 2, 3]];
            for f in TET_FACES {
                let mut key = [c[f[0]], c[f[1]], c[f[2]]];
                key.sort_unstable();
                let e = face_count.entry(key).or_insert((0, i));
                e.0 += 1;
            }
        }
    }

    let mut wire_edges: HashMap<[i64; 2], ()> = HashMap::new();
    for (key, (count, owner)) in &face_count {
        if *count == 1 {
            let color = tet_color(&idx, &spec.color_mode, *owner, scalar_range)?;
            let alpha = if let Some(rk) = idx.region_key {
                let k = rk.data.get_i64(*owner);
                *spec.opacity_overrides.get(&k).unwrap_or(&spec.volume_opacity)
            } else {
                spec.volume_opacity
            };
            scene.tris.push(SceneTri {
                a: doc.points[key[0] as usize],
                b: doc.points[key[1] as usize],
                c: doc.points[key[2] as usize],
                color,
                alpha,
                set: SetKind::Volume,
            });
            if spec.wireframe && wire_edges.len() < spec.max_wireframe_edges {
                for (u, v) in [(0, 1), (1, 2), (0, 2)] {
                    let mut ek = [key[u], key[v]];
                    ek.sort_unstable();
                    wire_edges.entry(ek).or_insert(());
                }
            }
        }
    }

    if spec.show_faces {
        for (i, sel) in selected.iter().enumerate() {
            if doc.types[i] == VTK_TRIANGLE && *sel {
                let c = doc.cell(i);
                let color = match (&spec.color_mode, idx.face_tag_key) {
                    (ColorMode::Uniform(u), _) => *u,
                    (_, Some(ft)) => categorical_color(ft.data.get_i64(i)),
                    (_, None) => SENTINEL_COLOR,
                };
                scene.tris.push(SceneTri {
                    a: doc.points[c[0] as usize],
                    b: doc.points[c[1] as usize],
                    c: doc.points[c[2] as usize],
                    color,
                    alpha: spec.face_opacity,
                    set: SetKind::Face,
                });
            }
        }
    }

    if spec.show_curves {
        for (i, sel) in selected.iter().enumerate() {
            if doc.types[i] == VTK_POLY_LINE && *sel {
                let c = doc.cell(i);
                let color = match (idx.curve_id, &idx.curve_kinds) {
                    (Some(cid), Some(kinds)) => {
                        let id = cid.data.get_i64(i);
                        if id >= 0 && (id as usize) < kinds.len() {
                            CURVE_KIND_COLORS[(kinds[id as usize].max(0) as usize)
                                % CURVE_KIND_COLORS.len()]
                        } else {
                            CURVE_KIND_COLORS[3]
                        }
                    }
                    _ => CURVE_KIND_COLORS[3],
                };
                for w in c.windows(2) {
                    scene.segments.push(SceneSegment {
                        a: doc.points[w[0] as usize],
                        b: doc.points[w[1] as usize],
                        color,
                    });
                }
            }
        }
    }

    for ek in wire_edges.keys() {
        scene.segments.push(SceneSegment {
            a: doc.points[ek[0] as usize],
            b: doc.points[ek[1] as usize],
            color: [40, 40, 40],
        });
    }

    for p in &spec.highlight_points {
        scene.markers.push(SceneMarker { p: *p, color: [255, 0, 0] });
    }

    if !doc.points.is_empty() {
        let mut min = doc.points[0];
        let mut max = doc.points[0];
        for p in &doc.points {
            min = Vec3::new(min.x.min(p.x), min.y.min(p.y), min.z.min(p.z));
            max = Vec3::new(max.x.max(p.x), max.y.max(p.y), max.z.max(p.z));
        }
        scene.bbox = Some(BoundingBox { min, max });
    }

    Ok(scene)
}
