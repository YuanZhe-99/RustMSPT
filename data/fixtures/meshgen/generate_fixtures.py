#!/usr/bin/env -S uv run --script
# /// script
# requires-python = ">=3.11"
# dependencies = []
# ///
"""G0-3 fixture generator: hand-writes the contract VTU fixtures named in
SPEC_meshgen_contracts.md §6 directly from the frozen schema.

These are authored *to the schema*, not produced by the mesher (which does not
exist yet) -- that is the point: they validate the reader and the verifier
before there is anything to verify.

Emitted ASCII layout matches `save_vtu(.., VtuEncoding::Ascii)` exactly so the
files round-trip through the real reader byte-stably.
"""
import os, sys

OUT = sys.argv[1] if len(sys.argv) > 1 else "data/fixtures/meshgen"

SCHEMA_VERSION = 1
GENERATOR_VERSION = (0, 1, 0)

# --------------------------------------------------------------- emission

def fmt(v):
    if isinstance(v, float):
        r = repr(v)
        return r[:-2] if r.endswith(".0") else r
    return str(v)


def arr(name, vtk_type, values, components=1, tuples=None):
    return dict(name=name, type=vtk_type, values=list(values),
                components=components, tuples=tuples)


def emit_array(a, indent):
    comp = f' NumberOfComponents="{a["components"]}"' if a["components"] != 1 else ""
    tup = f' NumberOfTuples="{a["tuples"]}"' if a["tuples"] is not None else ""
    body = " ".join(fmt(v) for v in a["values"])
    return (f'{indent}<DataArray type="{a["type"]}" Name="{a["name"]}"{comp}{tup}'
            f' format="ascii">{body}</DataArray>\n')


def write_vtu(path, points, cells, field, point_data, cell_data):
    """cells: list of (vtk_type, [node ids])"""
    conn, offs, typs = [], [], []
    for t, nodes in cells:
        conn.extend(nodes)
        offs.append(len(conn))
        typs.append(t)
    out = ['<VTKFile type="UnstructuredGrid" version="1.0" byte_order="LittleEndian"'
           ' header_type="UInt64">\n', "  <UnstructuredGrid>\n"]
    if field:
        # VTK requires NumberOfTuples on FieldData arrays; save_vtu always emits
        # it there, so the fixtures carry it too and stay byte-canonical.
        out.append("    <FieldData>\n")
        for a in field:
            a = dict(a, tuples=len(a["values"]) // a["components"])
            out.append(emit_array(a, "      "))
        out.append("    </FieldData>\n")
    out.append(f'    <Piece NumberOfPoints="{len(points)}" NumberOfCells="{len(cells)}">\n')
    out.append("      <Points>\n")
    flat = [c for p in points for c in p]
    out.append(emit_array(arr("Points", "Float64", flat, components=3), "        "))
    out.append("      </Points>\n")
    out.append("      <Cells>\n")
    out.append(emit_array(arr("connectivity", "Int64", conn), "        "))
    out.append(emit_array(arr("offsets", "Int64", offs), "        "))
    out.append(emit_array(arr("types", "UInt8", typs), "        "))
    out.append("      </Cells>\n")
    out.append("      <PointData>\n")
    for a in point_data:
        out.append(emit_array(a, "        "))
    out.append("      </PointData>\n")
    out.append("      <CellData>\n")
    for a in cell_data:
        out.append(emit_array(a, "        "))
    out.append("      </CellData>\n")
    out.append("    </Piece>\n  </UnstructuredGrid>\n</VTKFile>\n")
    os.makedirs(os.path.dirname(path), exist_ok=True)
    with open(path, "w") as f:
        f.write("".join(out))
    return path


TETRA, TRI, POLYLINE = 10, 5, 4

def oriented(points, n):
    """Return the 4-node list with positive orientation (SPEC_meshgen_geometry §1.1),
    so a fixture never reads as inverted unless that is the defect it carries."""
    a, b, c, d = (points[i] for i in n)
    u = [b[i] - a[i] for i in range(3)]
    v = [c[i] - a[i] for i in range(3)]
    w = [d[i] - a[i] for i in range(3)]
    det = (u[0] * (v[1] * w[2] - v[2] * w[1])
           - u[1] * (v[0] * w[2] - v[2] * w[0])
           + u[2] * (v[0] * w[1] - v[1] * w[0]))
    return list(n) if det > 0 else [n[0], n[2], n[1], n[3]]


def metadata(stage_index, counts, dmin=(0.0, 0.0, 0.0), dmax=(1.0, 1.0, 1.0),
             config_hash=0, determinism=0):
    return [
        arr("SchemaVersion", "Int32", [SCHEMA_VERSION], tuples=1),
        arr("StageIndex", "Int32", [stage_index], tuples=1),
        arr("GeneratorVersion", "Int32", list(GENERATOR_VERSION), tuples=3),
        arr("ConfigHash", "UInt64", [config_hash], tuples=1),
        arr("DomainMin", "Float64", list(dmin), components=3, tuples=1),
        arr("DomainMax", "Float64", list(dmax), components=3, tuples=1),
        arr("Counts", "Int64", list(counts), tuples=len(counts)),
        arr("DeterminismMode", "UInt8", [determinism], tuples=1),
    ]


def sets_table(prefix, sets, extra=None):
    """VTK-style END offsets, matching VtuDoc::cell() and render_scene::set_members"""
    offsets, comps, acc = [], [], 0
    for s in sets:
        acc += len(s)
        offsets.append(acc)
        comps.extend(s)
    out = [arr(prefix + "Offsets", "Int64", offsets),
           arr(prefix + "Components", "Int32", comps)]
    if extra:
        out.extend(extra)
    return out


def full_cell_data(n, kind, region, partition, regime, face_tag, curve):
    return [
        arr("cell_kind", "UInt8", kind),
        arr("region_key", "Int32", region),
        arr("partition_id", "Int32", partition),
        arr("regime", "UInt8", regime),
        arr("face_tag_key", "Int32", face_tag),
        arr("curve_id", "Int32", curve),
    ]


def full_point_data(n_id_key, constraint_kind, constraint_ref):
    return [
        arr("n_id_key", "Int32", n_id_key),
        arr("constraint_kind", "UInt8", constraint_kind),
        arr("constraint_ref", "Int32", constraint_ref),
    ]


# ------------------------------------------------ F0: the reference fixture
# One unit cube, Freudenthal 6-tet decomposition (SPEC_meshgen_geometry §2.2).
CUBE = [(0., 0., 0.), (1., 0., 0.), (0., 1., 0.), (1., 1., 0.),
        (0., 0., 1.), (1., 0., 1.), (0., 1., 1.), (1., 1., 1.)]
KUHN = [(0, 1, 3, 7), (0, 1, 7, 5), (0, 2, 7, 3), (0, 2, 6, 7), (0, 4, 5, 7), (0, 4, 7, 6)]
# region assignment: K0,K1,K2 -> region set 0 = {0} (background); K3,K4,K5 -> set 1 = {1}
REGION_OF_TET = [0, 0, 0, 1, 1, 1]


def shared_interface_faces():
    """faces shared by one region-0 tet and one region-1 tet"""
    seen = {}
    for i, t in enumerate(KUHN):
        for k in range(4):
            f = tuple(sorted(t[j] for j in range(4) if j != k))
            seen.setdefault(f, []).append(i)
    out = []
    for f, owners in sorted(seen.items()):
        if len(owners) == 2 and REGION_OF_TET[owners[0]] != REGION_OF_TET[owners[1]]:
            lo = owners[0] if REGION_OF_TET[owners[0]] == 1 else owners[1]
            hi = owners[1] if lo == owners[0] else owners[0]
            out.append((f, lo, hi))
    return out


def build_good(path, **over):
    faces = shared_interface_faces()
    cells = [(TETRA, list(t)) for t in KUHN]
    cells += [(TRI, list(f)) for f, _, _ in faces]
    # A box-edge feature curve of component 1, along nodes that region 1 actually reaches
    # ({0,2,4,5,6,7} - K3/K4/K5). It used to run 0-1-3, where the mesh carries no component-1
    # material at all, so the fixture declared a curve of a body that is not there; [V9]'s
    # first clause reads exactly that and the declaration has to be true for the reference
    # fixture to mean anything.
    cells += [(POLYLINE, [0, 2, 6])]
    n_tet, n_face = len(KUHN), len(faces)

    kind = [0]*n_tet + [1]*n_face + [2]
    region = [REGION_OF_TET[i] for i in range(n_tet)] + [-1]*n_face + [-1]
    partition = [0]*n_tet + [-1]*n_face + [-1]
    regime = [0]*n_tet + [255]*n_face + [255]
    face_tag = [-1]*n_tet + [0]*n_face + [-1]
    curve = [-1]*n_tet + [-1]*n_face + [0]

    # node id sets: nodes of the {1} region carry {0,1}, others {0}
    in_one = set()
    for i, t in enumerate(KUHN):
        if REGION_OF_TET[i] == 1:
            in_one.update(t)
    n_id_key = [1 if v in in_one else 0 for v in range(8)]
    constraint_kind = [0]*8
    constraint_ref = [-1]*8
    for f, _, _ in faces:                                   # interface nodes
        for v in f:
            constraint_kind[v] = 1
            constraint_ref[v] = 1
    for v in (0, 2, 6):                                     # curve nodes
        constraint_kind[v] = 2
        constraint_ref[v] = 0

    field = sets_table("RegionSet", [[0], [1]],
                       extra=[arr("RegionSetPriority", "UInt32", [4294967295, 1])])
    field += sets_table("NIdSet", [[0], [0, 1]])
    field += sets_table("FaceTag", [[1]],
                        extra=[arr("FaceTagKind", "UInt8", [0]),
                               arr("FaceTagSideElems", "Int32",
                                   [e for _, lo, hi in faces for e in (lo, hi)],
                                   components=2)])
    field += [arr("ComponentX", "Int32", [1]), arr("ComponentY", "UInt32", [1]),
              arr("ComponentKind", "UInt8", [0]), arr("ComponentClosed", "UInt8", [1])]
    field += ([arr("CurveKind", "UInt8", [3])] + sets_table("CurveComp", [[1]])
              + [arr("CurveRadialPatches", "Int32", [2])])
    field += metadata(11, [8, n_tet, n_face, 1])

    cd = full_cell_data(len(cells), kind, region, partition, regime, face_tag, curve)
    pd = full_point_data(n_id_key, constraint_kind, constraint_ref)
    points = list(CUBE)

    mutate = over.get("mutate")
    if mutate:
        mutate(points, cells, cd, pd, field)
    # Counts must describe the mesh as emitted, so a fixture's only reported defect
    # is the one it was built to carry (and never a stale-metadata artifact).
    counts = next(a for a in field if a["name"] == "Counts")
    counts["values"] = [len(points),
                        sum(1 for t, _ in cells if t == TETRA),
                        sum(1 for t, _ in cells if t == TRI),
                        sum(1 for t, _ in cells if t == POLYLINE)]
    return write_vtu(path, points, cells, field, pd, cd)


def cd_get(cd, name):
    return next(a for a in cd if a["name"] == name)["values"]


# ------------------------------------------------------ corrupted fixtures

def m_inverted(points, cells, cd, pd, field):
    t, nodes = cells[0]
    cells[0] = (t, [nodes[0], nodes[2], nodes[1], nodes[3]])       # sign flip


def m_duplicate_node(points, cells, cd, pd, field):
    points.append(points[7])                                       # exact copy of v7
    dup = len(points) - 1
    t, nodes = cells[4]
    cells[4] = (t, [dup if n == 7 else n for n in nodes])
    for a in pd:
        a["values"].append(a["values"][7])


def m_hanging_node(points, cells, cd, pd, field):
    """split one tet in two using a new node on the midpoint of edge v0-v7;
    the neighbouring tets still see the unsplit edge -> T-junction"""
    points.append(tuple((points[0][i] + points[7][i]) / 2 for i in range(3)))
    m = len(points) - 1
    t, nodes = cells[0]                                            # (0,1,3,7)
    cells[0] = (t, oriented(points, [0, 1, 3, m]))
    cells.insert(1, (TETRA, oriented(points, [m, 1, 3, 7])))
    for a in cd:
        a["values"].insert(1, a["values"][0])
    for a in pd:
        a["values"].append(a["values"][0])


def m_triple_face(points, cells, cd, pd, field):
    """a third tet glued onto an interior face already shared by two"""
    points.append((0.4, 0.5, 0.3))
    apex = len(points) - 1
    cells.insert(6, (TETRA, oriented(points, [0, 1, 7, apex])))                      # face (0,1,7) is shared
    for a in cd:
        a["values"].insert(6, a["values"][0])
    for a in pd:
        a["values"].append(a["values"][0])


def m_boundary_leak(points, cells, cd, pd, field):
    """delete one tet: the faces it covered become untagged boundary faces
    that do not lie on any domain plane"""
    del cells[5]
    for a in cd:
        del a["values"][5]


def m_unwelded_sheet(points, cells, cd, pd, field):
    """duplicate one node of a tagged face and point the face at the copy, so the
    face no longer shares all three of its nodes with the tets on either side"""
    for i, (t, nodes) in enumerate(cells):
        if t == TRI:
            target = nodes[0]
            points.append(points[target])
            dup = len(points) - 1
            cells[i] = (t, [dup if n == target else n for n in nodes])
            for a in pd:
                a["values"].append(a["values"][target])
            break
    next(a for a in field if a["name"] == "FaceTagKind")["values"] = [1]   # sheet


def seal_exposed(points, cells, cd, dmin, dmax, tol=1e-12):
    """Tag every boundary face that is not on a domain plane, so a fixture reports
    only the defect it was built to carry (and never an incidental [V3] leak)."""
    from collections import defaultdict
    owners = defaultdict(list)
    for i, (t, n) in enumerate(cells):
        if t == TETRA:
            for f in ((1, 2, 3), (0, 3, 2), (0, 1, 3), (0, 2, 1)):
                owners[tuple(sorted((n[f[0]], n[f[1]], n[f[2]])))].append(i)
    tagged = {tuple(sorted(n)) for t, n in cells if t == TRI}
    for key in sorted(owners):
        if len(owners[key]) != 1 or key in tagged:
            continue
        pts = [points[k] for k in key]
        if any(all(abs(q[ax] - v) <= tol for q in pts)
               for ax in range(3) for v in (dmin[ax], dmax[ax])):
            continue
        cells.append((TRI, list(key)))
        for a in cd:
            a["values"].append({"cell_kind": 1, "face_tag_key": 0}.get(a["name"], -1)
                               if a["name"] != "regime" else 255)


def build_stacked_band(path):
    """Two stacked element layers across one thin gap, where a band must be exactly
    one layer thick. Purpose-built (not a cube mutation): the defect only exists in
    a real band, and it is detectable by the [V7] one-layer check that lands with G7-1.
    Both prisms are split by the SPEC_meshgen_geometry §4.3 pattern for the diagonal
    set the smallest-node-key rule selects, so the slab itself is conforming."""
    t_gap = 0.1
    tri = [(0.0, 0.0), (1.0, 0.0), (0.0, 1.0)]
    points = [(x, y, z) for z in (0.0, t_gap / 2, t_gap) for (x, y) in tri]
    a, m, b = [0, 1, 2], [3, 4, 5], [6, 7, 8]
    cells = []
    for lo, hi in ((a, m), (m, b)):
        # SNK diagonals a0-b1, a1-b2, a0-b2  ->  (a0,a1,a2,b2), (a0,a1,b1,b2), (a0,b0,b1,b2)
        for t in ([lo[0], lo[1], lo[2], hi[2]],
                  [lo[0], lo[1], hi[1], hi[2]],
                  [lo[0], hi[0], hi[1], hi[2]]):
            cells.append((TETRA, oriented(points, t)))
    n_tet = len(cells)
    cells.append((TRI, list(a)))                      # the two band walls
    cells.append((TRI, list(b)))

    kind = [0] * n_tet + [1, 1]
    region = [0] * n_tet + [-1, -1]
    partition = [0] * n_tet + [-1, -1]
    regime = [1] * n_tet + [255, 255]                 # 1 = band
    face_tag = [-1] * n_tet + [0, 0]
    curve = [-1] * len(cells)
    cd = full_cell_data(len(cells), kind, region, partition, regime, face_tag, curve)
    pd = full_point_data([0] * 9, [1] * 9, [-1] * 9)

    dmin, dmax = (0.0, 0.0, 0.0), (1.0, 1.0, t_gap)
    seal_exposed(points, cells, cd, dmin, dmax)

    field = sets_table("RegionSet", [[0]], extra=[arr("RegionSetPriority", "UInt32", [4294967295])])
    field += sets_table("NIdSet", [[0]])
    field += sets_table("FaceTag", [[1]],
                        extra=[arr("FaceTagKind", "UInt8", [0]),
                               arr("FaceTagSideElems", "Int32",
                                   [e for _ in range(sum(1 for t, _ in cells if t == TRI))
                                    for e in (0, -1)], components=2)])
    field += [arr("ComponentX", "Int32", [1]), arr("ComponentY", "UInt32", [1]),
              arr("ComponentKind", "UInt8", [0]), arr("ComponentClosed", "UInt8", [1])]
    field += [arr("CurveKind", "UInt8", [3])] + sets_table("CurveComp", [[1]])
    field += metadata(9, [len(points), n_tet, sum(1 for t, _ in cells if t == TRI), 0],
                      dmin=dmin, dmax=dmax)
    counts = next(a for a in field if a["name"] == "Counts")
    counts["values"] = [len(points), n_tet, sum(1 for t, _ in cells if t == TRI), 0]
    return write_vtu(path, points, cells, field, pd, cd)


def m_partition_id(points, cells, cd, pd, field):
    """a second, disconnected component, but both are labelled partition 0"""
    base = len(points)
    points.extend([(3., 3., 3.), (4., 3., 3.), (3., 4., 3.), (3., 3., 4.)])
    cells.append((TETRA, oriented(points, [base, base + 1, base + 2, base + 3])))
    for a in cd:
        a["values"].append(a["values"][0])
    cd_get(cd, "partition_id")[-1] = 0                             # should be 1
    for a in pd:
        a["values"].extend([a["values"][0]] * 4)


def m_region_key(points, cells, cd, pd, field):
    cd_get(cd, "region_key")[2] = 7                                # table has 2 entries


def m_pinhole_sheet(points, cells, cd, pd, field):
    """a sheet with a hole: drop one of its triangles and declare the tag a sheet"""
    for i in range(len(cells) - 1, -1, -1):
        if cells[i][0] == TRI:
            del cells[i]
            for a in cd:
                del a["values"][i]
            break
    next(a for a in field if a["name"] == "FaceTagKind")["values"] = [1]


def m_curve_node_id(points, cells, cd, pd, field):
    """[V9] a node on a declared curve whose N_ID does not contain the curve's component."""
    n_id = next(a for a in pd if a["name"] == "n_id_key")
    n_id["values"][6] = 0                    # {0} instead of {0,1}


def m_radial_patches(points, cells, cd, pd, field):
    """[V9] fewer material sectors around a junction edge than the curve table declares."""
    # Declare the curve a two-body junction with four radial patches. The mesh shows two
    # sectors around edge (0,2), which is the mismatch. The nodes' N_ID is widened to {0,1,2}
    # first so the first clause stays quiet and the fixture carries one defect, not two.
    for a in field:
        if a["name"] == "CurveCompOffsets":
            a["values"] = [2]
        elif a["name"] == "CurveCompComponents":
            a["values"] = [1, 2]
        elif a["name"] == "CurveRadialPatches":
            a["values"] = [4]
        elif a["name"] == "NIdSetOffsets":
            a["values"] = [1, 3, 6]
        elif a["name"] == "NIdSetComponents":
            a["values"] = [0, 0, 1, 0, 1, 2]
    n_id = next(a for a in pd if a["name"] == "n_id_key")
    for v in (0, 2, 6):
        n_id["values"][v] = 2


def m_open_junction_fan(points, cells, cd, pd, field):
    """[V9] a junction edge whose tet fan does not close."""
    # Move the declared curve onto the cube's main diagonal 0-7, which all six Kuhn tets
    # share and whose fan is closed and strictly interior - every face around it carries two
    # tets, and none of them is on the domain plane. Then drop one of the six. The fan opens
    # by the two faces that tet alone carried, which is what the clause reads.
    for i, (t, n) in enumerate(cells):
        if t == POLYLINE:
            cells[i] = (POLYLINE, [0, 7])
            break
    for v in range(8):
        pd_kind = next(a for a in pd if a["name"] == "constraint_kind")
        pd_ref = next(a for a in pd if a["name"] == "constraint_ref")
        if v in (0, 7):
            pd_kind["values"][v] = 2
            pd_ref["values"][v] = 0
    # N_ID must already contain component 1 at both ends, so the first clause stays quiet.
    n_id = next(a for a in pd if a["name"] == "n_id_key")
    n_id["values"][0] = 1
    n_id["values"][7] = 1
    drop = next(i for i, (t, n) in enumerate(cells)
                if t == TETRA and 0 in n and 7 in n)
    del cells[drop]
    for a in cd:
        del a["values"][drop]


FIXTURES = [
    ("good_cube", None, "reference; every check passes"),
    ("bad_inverted_tet", m_inverted, "[V1] negative tet volume"),
    ("bad_duplicate_node", m_duplicate_node, "[V2] duplicate coincident nodes"),
    ("bad_hanging_node", m_hanging_node, "[V3] T-junction / hanging node"),
    ("bad_triple_face", m_triple_face, "[V3] interior face with 3 adjacent tets"),
    ("bad_boundary_leak", m_boundary_leak, "[V3] untagged boundary face off the domain planes"),
    ("bad_unwelded_sheet", m_unwelded_sheet, "[V7] sheet face not welded to both sides"),
    ("bad_stacked_band", "builder", "[V7] band region with two element layers"),
    ("bad_partition_id", m_partition_id, "[V8] stored partition_id != recomputed"),
    ("bad_region_key", m_region_key, "[V6] region_key outside the region-set table"),
    ("bad_pinhole_sheet", m_pinhole_sheet, "[V8] sheet with an interior hole"),
    ("bad_curve_node_id", m_curve_node_id, "[V9] curve node N_ID missing the curve's component"),
    ("bad_radial_patches", m_radial_patches, "[V9] fewer material sectors than radial patches"),
    ("bad_open_junction_fan", m_open_junction_fan, "[V9] junction edge whose tet fan does not close"),
]

if __name__ == "__main__":
    for name, mut, desc in FIXTURES:
        path = os.path.join(OUT, name + ".vtu")
        p = build_stacked_band(path) if mut == "builder" else build_good(path, mutate=mut)
        print("  wrote %-28s  %s" % (os.path.basename(p), desc))
    print("%d fixtures written to %s" % (len(FIXTURES), OUT))
