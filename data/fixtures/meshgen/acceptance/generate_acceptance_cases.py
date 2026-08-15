import struct, math, os

def write_stl(name, tris):
    with open(name, 'wb') as f:
        f.write(b'\0'*80); f.write(struct.pack('<I', len(tris)))
        for a, b, c in tris:
            n = (0.0, 0.0, 0.0)
            f.write(struct.pack('<3f', *n))
            for p in (a, b, c): f.write(struct.pack('<3f', *p))
            f.write(struct.pack('<H', 0))

def box(lo, hi):
    x0,y0,z0 = lo; x1,y1,z1 = hi
    v = [(x0,y0,z0),(x1,y0,z0),(x1,y1,z0),(x0,y1,z0),(x0,y0,z1),(x1,y0,z1),(x1,y1,z1),(x0,y1,z1)]
    q = [(0,3,2,1),(4,5,6,7),(0,1,5,4),(1,2,6,5),(2,3,7,6),(3,0,4,7)]
    t = []
    for a,b,c,d in q:
        t.append((v[a],v[b],v[c])); t.append((v[a],v[c],v[d]))
    return t

def icosphere(center, r, subdiv=2):
    p = (1 + 5**0.5) / 2
    verts = [(-1,p,0),(1,p,0),(-1,-p,0),(1,-p,0),(0,-1,p),(0,1,p),
             (0,-1,-p),(0,1,-p),(p,0,-1),(p,0,1),(-p,0,-1),(-p,0,1)]
    faces = [(0,11,5),(0,5,1),(0,1,7),(0,7,10),(0,10,11),(1,5,9),(5,11,4),(11,10,2),
             (10,7,6),(7,1,8),(3,9,4),(3,4,2),(3,2,6),(3,6,8),(3,8,9),(4,9,5),
             (2,4,11),(6,2,10),(8,6,7),(9,8,1)]
    verts = [list(v) for v in verts]
    for _ in range(subdiv):
        mid = {}; new = []
        def m(i, j):
            k = (min(i,j), max(i,j))
            if k not in mid:
                a, b = verts[i], verts[j]
                verts.append([(a[t]+b[t])/2 for t in range(3)])
                mid[k] = len(verts)-1
            return mid[k]
        for a,b,c in faces:
            ab, bc, ca = m(a,b), m(b,c), m(c,a)
            new += [(a,ab,ca),(b,bc,ab),(c,ca,bc),(ab,bc,ca)]
        faces = new
    out = []
    for a,b,c in faces:
        tri = []
        for i in (a,b,c):
            v = verts[i]; L = math.sqrt(sum(t*t for t in v))
            tri.append(tuple(center[t] + r*v[t]/L for t in range(3)))
        out.append(tuple(tri))
    return out

def strut(p, q, w):
    """Axis-aligned rectangular strut from p to q with square cross-section w."""
    lo = [min(p[i], q[i]) for i in range(3)]
    hi = [max(p[i], q[i]) for i in range(3)]
    for i in range(3):
        if hi[i] - lo[i] < 1e-9:
            lo[i] -= w/2; hi[i] += w/2
    return box(tuple(lo), tuple(hi))

O = os.path.dirname(os.path.abspath(__file__))
J = lambda *a: os.path.join(O, *a)

# A-1 sphere
write_stl(J('a1_sphere.stl'), icosphere((0.5,0.5,0.5), 0.30, 3))
# A-2 cube
write_stl(J('a2_cube.stl'), box((0.2317,0.2317,0.2317),(0.7683,0.7683,0.7683)))
# A-3 sphere and cube, intersecting (two components)
write_stl(J('a3_sphere.stl'), icosphere((0.40,0.50,0.50), 0.235, 3))
write_stl(J('a3_cube.stl'),   box((0.50,0.3217,0.3217),(0.8217,0.6783,0.6783)))
# A-4 cube inside sphere
write_stl(J('a4_sphere.stl'), icosphere((0.5,0.5,0.5), 0.335, 3))
write_stl(J('a4_cube.stl'),   box((0.3817,0.3817,0.3817),(0.6183,0.6183,0.6183)))
# --- A-6 and A-7: the two thin regimes (2026-08-07) ---
# The original single A-6 (limb 0.0180) and A-7 (gap 0.0080) were RETIRED: their
# feature sat below lfs_floor = gap_cells * h_min, so nothing refined toward it and
# the ladder logged "declared thin but kept volumetric". They measured the resolution
# floor, not the thin path. A-6 reached 18.70% volume error against A-6b's 0.036% on
# the same geometry - the fixture was the defect, not the mesher.
# These four put the feature *inside* a named regime instead, sized against
# t_sheet = t_sheet_factor * h and t_layer = t_layer_factor * h so the regime is a
# property of the fixture rather than of the resolution it happens to be run at.
#
#   a: thickness <= t_sheet  -> the limb/gap collapses to an interface, nodes tagged
#   b: t_sheet < thickness <= t_layer -> one element through the thickness (a band)
CUBE6 = ((0.2017, 0.2017, 0.2017), (0.5817, 0.5817, 0.5817))
write_stl(J('a6a_cube.stl'), box(*CUBE6))
write_stl(J('a6b_cube.stl'), box(*CUBE6))
# Limb thickness: 0.0060 (sheet regime) and 0.0200 (band regime), both above the
# lfs_floor their configs set, unlike A-6's 0.0180 which is below it.
#
# The thicknesses are sized against the *converged* h, not h_max. t_sheet and t_layer
# are fractions of whatever h the S3<->S4 loop settles on, so any criterion that lowers
# h narrows the sheet window under the fixture - the curve criterion alone (curve_cells
# 2) halves it. Their configs run h_max_frac 0.04, which converges to h = 0.034641, so
# t_sheet = 0.006928 and t_layer = 0.034641 and both thicknesses land where intended.
write_stl(J('a6a_limb.stl'), box((0.5817, 0.2917, 0.2917), (0.8517, 0.4917, 0.2977)))
write_stl(J('a6b_limb.stl'), box((0.5817, 0.2917, 0.2917), (0.8517, 0.4917, 0.3117)))
# A-7's plates, with the gap set to the same two regimes. Lower plate is fixed;
# the upper plate's z_min is what moves.
A7_LO = ((0.2017, 0.2017, 0.3017), (0.7817, 0.7817, 0.4217))
write_stl(J('a7a_lower.stl'), box(*A7_LO))
write_stl(J('a7b_lower.stl'), box(*A7_LO))
write_stl(J('a7a_upper.stl'), box((0.2017, 0.2017, 0.4277), (0.7817, 0.7817, 0.5497)))
write_stl(J('a7b_upper.stl'), box((0.2017, 0.2017, 0.4417), (0.7817, 0.7817, 0.5637)))

# A-8 lattice: a 3x3 pillar grid with x- and y-runners on two OFFSET decks.
# The decks are offset deliberately. Putting both runner families in one plane makes
# their side faces coplanar over every crossing, and at 3x3 scale S2 rejects the
# arrangement with `[ARR-RESID] crossing or refused local CDT constraint` - recorded in
# PLAN 17.4 as a genuine A-8 finding. A single coplanar crossing pair, and a triple
# junction of three mutually perpendicular struts, both arrange fine; it is the
# combination at scale that fails.
w = 0.05
lo, hi = 0.2317, 0.7683
xs = [0.3217, 0.5, 0.6783]
lat = []
for x in xs:
    for y in xs:
        lat += box((x - w/2, y - w/2, lo), (x + w/2, y + w/2, hi))
for y in xs:
    lat += box((lo, y - w/2, 0.4017 - w/2), (hi, y + w/2, 0.4017 + w/2))
for x in xs:
    lat += box((x - w/2, lo, 0.5983 - w/2), (x + w/2, hi, 0.5983 + w/2))
write_stl(J('a8_lattice.stl'), lat)

print("generated", len([f for f in os.listdir(O) if f.endswith('.stl')]), "stl files")
