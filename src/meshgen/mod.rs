pub mod arrange;
pub mod cdt;
pub mod classify;
pub mod cut;
pub mod features;
pub mod gapfield;
pub mod junction;
pub mod lattice;
pub mod predicates;
pub mod render_scene;
pub mod sizing;
pub mod snap;
pub mod snapshot;
pub mod surface;
pub mod thin;
pub mod topo;
pub mod verify;

pub use arrange::{
    arrange_surface, arranged_surface_to_doc, clip_arranged_to_box, triangulate_parent,
    ArrangeComponent, ArrangeOptions, ArrangedCurve, ArrangedCurveKind, ArrangedFace,
    ArrangedPointFeature, ArrangedSurface, ArrangementStats, CoincidenceCase, CoincidenceEntity,
    CoincidenceEvent, DegradedNeighborhood, DegradedReason, EdgeId, IntersectionRegistry,
    IsectProv, RegistrySegment, RegistryVertex, SegKey, TriId,
};
pub use classify::{
    classified_to_doc, classify_lattice, classify_lattice_with, resolve, Classification,
    ClassifyOptions, ClassifyStats, PointClassifier,
    OwnershipRecord, Provenance, Side, RAY_DIRECTIONS,
};
pub use cut::{
    cut_lattice, cut_tet, cut_to_doc, face_split, guarded_dry_run, orient_positively,
    polygon_soup_centroid, prism_tets,
    prism_tets_with_diagonals, snk_diagonal_is_02, snk_split_quad, CellCut, CutMesh, CutOptions,
    CutStats, Escalation, FaceCutState, InterfaceFace, NodeKey, NodeSide, CUT_MIN_DIHEDRAL_DEG,
    CUT_VOLUME_TOLERANCE,
};
pub use junction::{
    cell_centroid, face_centroid, face_mesh, fan_cell, loop_fan, FaceMesh, FannedCell, TET_FACES,
};
pub use features::{detect_features, FeatureCurve, FeatureEdgeKind, FeatureSet};
pub use lattice::{
    balance_octree, balance_violation, build_lattice, build_lattice_with_splits, lattice_to_doc,
    CellTemplate, Lattice, LatticeOptions, LatticeStats, FREUDENTHAL, LATTICE_MAX_TETS,
};
pub use gapfield::{
    compute_gap_field, gapfield_to_doc, validate_mid_surface, GapField, GapFieldOptions, GapFieldStats, GapGroup,
    GapPairing, GapSample, MidSurface, MidSurfaceDefect, PairClass, Regime, SampleKind, SkipReason,
    ThinRegion, BOX_COMPONENT, FLAGS_ALL, FLAG_CONTINUITY, FLAG_MUTUAL, FLAG_NO_CROSSING,
    FLAG_OPPOSITE_PATCH, FLAG_ORIENTATION,
};
pub use predicates::{node_key, orient2d_3d, orient3d, tet_quality, tet_signed_volume, TetQuality};
pub use render_scene::{
    build_scene, ColorMode, RenderScene, SceneFilter, SceneMarker, SceneSegment, SceneSpec,
    SceneTri, SetKind,
};
pub use sizing::{
    build_sizing_field, collect_geometry_sources, couple_gap_and_sizing, curvature_sources,
    feature_sources, gap_sources, regime_for, sizing_to_doc, CouplingOptions, CouplingReport,
    LockReason, SizingConstraint, SizingCriterion, SizingField, SizingLeaf, SizingLookup,
    SizingOptions, SizingSource, SizingStats, LFS_COVER_TOLERANCE, SIZING_MAX_LEAVES,
    SIZING_MAX_LEVEL,
};
pub use snap::{
    move_preserves_orientation, snap_lattice, snapped_to_doc, unique_edges, EdgeCrossing, SnapOptions,
    SnapStats, Snapped, TargetKind, ALTERNATING_PROJECTION_PASSES, SNAP_MOTION_CAP,
    SNAP_RECHECK_HIGH, SNAP_RECHECK_LOW, WEIGHT_CORNER, WEIGHT_CURVE, WEIGHT_SURFACE,
};
pub use snapshot::{
    emit_snapshot, should_emit, snapshot_dir, snapshot_path, stamp_metadata, warn_if_large,
    SnapshotMeta, Stage,
};
pub use surface::{
    condition_surface, condition_surface_to_doc, source_component_is_closed, ConditionedSurface,
    RepairAction, RepairActionType, RepairLog, SurfaceComponent,
};
pub use thin::{
    band_face_split, close_open_surface, split_band_cell, BandCellPlan, BandDecline,
    BandFaceParts, BandFaceSplit, BandSlabs, Slab,
    band_cell_boundary, band_cell_centroid, band_cell_table, band_ladder, enclosed_volume,
    mesh_band_cell, mesh_band_layer, predict_band_quality, snk_cell_diagonals, steiner_band_cell,
    BandCell, BandCellMesh, BandFacets, BandFailure, BandLayer, BandLayerMesh, BandPair,
    BandPrediction, BandTemplate, CellDiagonals, FemProfile, LadderDecision, LadderOutcome,
    ThinOptions, ThinStats, BAND_EDGES, BAND_EXPLICIT_ALTITUDE_RATIO, BAND_MAX_AR,
    BAND_MIN_DIHEDRAL_DEG, BAND_REGIONAL_FAILURE_SHARE, BAND_VOLUME_TOLERANCE,
};
pub use topo::{
    generalized_winding_number, gwn_margin_band, rebuild_topology, ClosureDefect,
    ComponentClassification, RebuiltTopology,
};
pub use verify::{
    annotate, report_to_json, report_to_log, verify, verify_with_options, CheckStatus, Severity,
    VerifyGates, VerifyItem, VerifyOptions, VerifyReport, VerifySection,
};
