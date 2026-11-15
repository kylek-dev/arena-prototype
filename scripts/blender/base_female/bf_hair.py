"""Chunky sculpted hair: cap, swept bangs, face-framing locks, high ponytail."""
import math
import bpy
from mathutils import Vector as V
import bf_lib as L
import bf_body as B

DARK = L.lin((0.085, 0.045, 0.032))
BASE = L.lin((0.21, 0.11, 0.065))
HIGH = L.lin((0.46, 0.27, 0.15))
BAND = L.lin((0.86, 0.64, 0.24))
BAND_D = L.lin((0.55, 0.36, 0.12))

HAIRLINE = [(0.0, 0.50), (0.5, 0.46), (0.9, 0.32), (1.25, 0.22), (1.75, 0.25), (2.1, -0.10),
            (2.5, -0.48), (math.pi, -0.55)]


def hline(phi):
    phi = abs(phi)
    for (p0, h0), (p1, h1) in zip(HAIRLINE, HAIRLINE[1:]):
        if phi <= p1:
            t = (phi - p0) / (p1 - p0)
            return h0 + (h1 - h0) * L.smooth01(t)
    return HAIRLINE[-1][1]


def dir_of(phi, h):
    """unit 'head direction' from azimuth (0=front, +=character left) and height (-1..1)."""
    c = math.sqrt(max(0.0, 1 - h * h))
    return V((math.sin(phi) * c, -math.cos(phi) * c, h))


def surf(body, u):
    """Point on the actual head surface in head-direction u (+ its normal)."""
    rx, ry, rz = B.HEAD_R
    o = B.HEAD_C + V((u.x * rx, u.y * ry, u.z * rz)) * 2.5
    d = (B.HEAD_C - o).normalized()
    ok, loc, nor, idx = body.ray_cast(o, d)
    if not ok:
        return B.HEAD_C + V((u.x * rx, u.y * ry, u.z * rz)), u
    return loc, nor


def hair_color(stripe, along, light):
    c = L.mix(DARK, BASE, 0.35 + 0.65 * stripe)
    c = L.mix(c, HIGH, max(0.0, light) * (0.35 + 0.65 * stripe))
    c = L.mix(c, DARK, 0.4 * max(0.0, -light))
    return c


TIE_U = dir_of(math.pi, 0.62)


def cap(body):
    hv, hf, _ = B.head_mesh(seg=48, rings=34)
    U, _, _ = L.ellipsoid((0, 0, 0), (1, 1, 1), seg=48, rings=34)
    keep_v = []
    pos, cols = [], []
    ref = V((0, -1, 0.3)).normalized()
    ax = TIE_U
    e1 = (ref - ax * ref.dot(ax)).normalized()
    e2 = ax.cross(e1)
    for u in U:
        u = V(u)
        phi = math.atan2(u.x, -u.y)
        inside = u.z > hline(phi)
        keep_v.append(inside)
        loc, nor = surf(body, u)
        lift = 0.0045 + 0.007 * L.smooth01(u.z) + 0.006 * math.exp(-((u - dir_of(0, 0.75)).length / 0.35) ** 2)
        pos.append(loc + nor * lift)
        q = u - ax * u.dot(ax)
        ang = math.atan2(q.dot(e2), q.dot(e1))
        stripe = 0.5 + 0.5 * math.cos(ang * 9)
        light = nor.z * 0.8 - 0.1
        cols.append(hair_color(stripe, 0, light))
    faces = [f for f in hf if all(keep_v[i] for i in f)]
    used = sorted({i for f in faces for i in f})
    remap = {o: n for n, o in enumerate(used)}
    verts = [pos[i] for i in used]
    faces = [tuple(remap[i] for i in f) for f in faces]
    return verts, faces, [cols[i] for i in used]


def lock(body, keys, base_w, thick, lift0, lift1, stripes=3, tip=0.0025):
    """keys: list of (phi, h) along the head surface; width tapers to a point."""
    P, refs, rads = [], [], []
    n = len(keys)
    for i, (phi, h) in enumerate(keys):
        t = i / (n - 1)
        loc, nor = surf(body, dir_of(phi, h))
        lift = lift0 + (lift1 - lift0) * t * t
        P.append(loc + nor * lift)
        refs.append(nor)
        w = base_w * (1.0 - 0.15 * t) * (1 - L.smooth01((t - 0.55) / 0.45)) + tip
        th = thick * (1 - 0.6 * L.smooth01((t - 0.5) / 0.5)) + tip * 0.5
        rads.append((th, w))
    v, f, uv = L.sweep(P, rads, ref=refs, ring=10, step=0.010, cap_scale=(0.6, 1.0))
    cols = []
    for p, (u, a) in zip(v, uv):
        stripe = 0.5 + 0.5 * math.cos(a * 2 * math.pi * stripes)
        light = 0.75 * (1 - u) - 0.1
        cols.append(hair_color(stripe, u, light))
    return v, f, cols


def tube(points, rads, ref=(1, 0, 0), stripes=4, band=None):
    v, f, uv = L.sweep(points, rads, ref=ref, ring=14, step=0.016, cap_scale=(0.5, 1.0))
    cols = []
    for p, (u, a) in zip(v, uv):
        if band:
            cols.append(L.mix(BAND_D, BAND, 0.5 + 0.5 * math.cos(a * 2 * math.pi)))
            continue
        stripe = 0.5 + 0.5 * math.cos(a * 2 * math.pi * stripes)
        light = 0.85 * (1 - u) ** 2 - 0.25 * u
        cols.append(hair_color(stripe, u, light))
    return v, f, cols


def ponytail(body):
    T, nT = surf(body, TIE_U)
    up = V((0, 0.55, 0.85)).normalized()
    parts = []
    base = T + nT * 0.004
    # tie band
    parts.append(tube([base - up * 0.006, base + up * 0.020], [(0.026, 0.024), (0.025, 0.023)], band=True))
    root = base + up * 0.012
    main = [root, root + V((0, 0.040, 0.028)), root + V((0, 0.088, 0.000)), root + V((0, 0.108, -0.075)),
            root + V((0, 0.100, -0.170)), root + V((0, 0.086, -0.265)), root + V((0, 0.094, -0.350)),
            root + V((0, 0.115, -0.420))]
    rmain = [(0.026, 0.023), (0.040, 0.032), (0.048, 0.039), (0.047, 0.040), (0.042, 0.036),
             (0.032, 0.028), (0.018, 0.016), (0.003, 0.003)]
    parts.append(tube(main, rmain, stripes=5))
    for sx in (1, -1):
        side = [root + V((0.004 * sx, 0.03, 0.02)), root + V((0.018 * sx, 0.082, -0.01)),
                root + V((0.030 * sx, 0.098, -0.090)), root + V((0.036 * sx, 0.090, -0.180)),
                root + V((0.040 * sx, 0.096, -0.255)), root + V((0.058 * sx, 0.118, -0.315))]
        rs = [(0.020, 0.018), (0.028, 0.024), (0.028, 0.024), (0.023, 0.020), (0.012, 0.011), (0.0025, 0.0025)]
        parts.append(tube(side, rs, stripes=3))
    # crown lock that sweeps over the tie toward the tail
    return parts


def build(body_name="BF_BodyRM"):
    body = bpy.data.objects[body_name]
    col = bpy.data.collections["BaseFemale"]
    cv, cf, cc = cap(body)
    co = L.build_object("BF_HairCapTmp", [(cv, cf, None)], col)
    a = co.data.color_attributes.new("Col", 'FLOAT_COLOR', 'POINT')
    for i, c in enumerate(cc):
        a.data[i].color = c
    L.apply_mod(co, 'SOLIDIFY', thickness=0.007, offset=0.0, use_even_offset=True, use_rim=True)
    me = co.data
    cv = [v.co.copy() for v in me.vertices]
    cf = [tuple(p.vertices) for p in me.polygons]
    cc = [tuple(me.color_attributes["Col"].data[i].color) for i in range(len(cv))]
    bpy.data.objects.remove(co, do_unlink=True)
    parts = [(cv, cf, cc)]
    # swept bangs (part on character's left, sweeping across to the right)
    parts.append(lock(body, [(0.38, 0.86), (0.12, 0.70), (-0.22, 0.55), (-0.55, 0.43), (-0.82, 0.30), (-0.98, 0.14)],
                      0.026, 0.008, 0.006, 0.013, stripes=3))
    parts.append(lock(body, [(0.46, 0.80), (0.30, 0.62), (0.10, 0.52), (-0.10, 0.44), (-0.26, 0.37)],
                      0.022, 0.007, 0.007, 0.011, stripes=2))
    parts.append(lock(body, [(0.30, 0.90), (-0.05, 0.80), (-0.40, 0.66), (-0.70, 0.52), (-0.95, 0.38)],
                      0.024, 0.008, 0.008, 0.010, stripes=3))
    parts.append(lock(body, [(0.55, 0.80), (0.66, 0.60), (0.80, 0.44), (0.92, 0.30)],
                      0.020, 0.007, 0.007, 0.010, stripes=2))
    # face-framing locks
    for s in (1, -1):
        parts.append(lock(body, [(1.05 * s, 0.42), (1.18 * s, 0.18), (1.24 * s, -0.12), (1.22 * s, -0.40),
                                 (1.15 * s, -0.62)], 0.017, 0.007, 0.006, 0.014, stripes=2))
    parts += ponytail(body)
    ob = L.build_object("BF_Hair", [(v, f, None) for v, f, c in parts], col)
    flat = [c for _, _, cs in parts for c in cs]
    attr = ob.data.color_attributes.new("Col", 'FLOAT_COLOR', 'POINT')
    for i, c in enumerate(flat):
        attr.data[i].color = c
    # chunky shell for the cap (solidify keeps colours)
    L.shade_smooth(ob)
    return ob
