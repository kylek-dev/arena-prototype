"""Eyes, lash lines and brows (rigid to head)."""
import math
import bpy
from mathutils import Vector as V, Matrix
import bf_lib as L

EYE_X, EYE_Z = 0.0325, 1.607
EYE_R = (0.0205, 0.0125, 0.0172)
EYE_YAW = 13.0
EYE_SINK = 0.0042

SCLERA = L.lin((0.96, 0.94, 0.90))
IRIS_OUT = L.lin((0.06, 0.26, 0.24))
IRIS_IN = L.lin((0.20, 0.66, 0.50))
IRIS_LOW = L.lin((0.55, 0.85, 0.55))
PUPIL = L.lin((0.05, 0.04, 0.06))
SHINE = L.lin((1.0, 1.0, 1.0))
LASH = L.lin((0.13, 0.07, 0.06))
BROW = L.lin((0.24, 0.13, 0.08))


def surface_y(body, x, z):
    ok, loc, nor, idx = body.ray_cast(V((x, -0.4, z)), V((0, 1, 0)))
    return loc.y if ok else None


def eye(body, side):
    x = EYE_X * side
    ys = surface_y(body, x, EYE_Z)
    c = V((x, ys + EYE_SINK, EYE_Z))
    M = Matrix.Rotation(math.radians(EYE_YAW * side), 3, 'Z')
    v, f, uv = L.ellipsoid((0, 0, 0), EYE_R, seg=36, rings=24)
    verts, cols = [], []
    for p in v:
        lx, ly, lz = p.x / EYE_R[0], p.y / EYE_R[1], p.z / EYE_R[2]
        col = SCLERA
        if ly < 0:
            dx, dz = lx, lz + 0.06
            r = math.hypot(dx, dz)
            if r < 0.66:
                t = r / 0.66
                col = L.mix(IRIS_IN, IRIS_OUT, L.smooth01((t - 0.45) / 0.55))
                col = L.mix(col, IRIS_LOW, 0.45 * L.smooth01((-dz - 0.15) / 0.4) * (1 - t))
                if r < 0.30:
                    col = PUPIL
            # shine (same world side for both eyes: light from upper-left of screen)
            if math.hypot(lx - 0.24, lz - 0.30) < 0.14:
                col = SHINE
            if math.hypot(lx + 0.16, lz + 0.22) < 0.07:
                col = L.mix(col, SHINE, 0.8)
        verts.append(c + M @ p)
        cols.append(col)
    return verts, f, cols, c, M


def arc_on_surface(body, pts2d, lift):
    out = []
    for x, z in pts2d:
        ok, loc, nor, idx = body.ray_cast(V((x, -0.4, z)), V((0, 1, 0)))
        out.append(loc + nor * lift)
    return out


def lash(body, side, eye_c):
    w, h = EYE_R[0] * 0.93, EYE_R[2] * 0.80
    pts = []
    n = 9
    for i in range(n + 1):
        t = math.radians(200 - 210 * i / n)  # inner (nasal) corner -> outer corner
        lx = -math.cos(t) * w  # positive = outward
        pts.append((eye_c.x + side * lx, eye_c.z + math.sin(t) * h + 0.0015))
    pts.append((eye_c.x + side * (w + 0.0065), eye_c.z + 0.0035))  # outer flick
    P = arc_on_surface(body, pts, 0.0012)
    P[-1] = P[-1] + V((0, 0.003, 0))
    rads = []
    for i in range(len(P)):
        t = i / (len(P) - 1)
        rads.append((0.0012 + 0.0016 * math.sin(math.pi * min(1, t * 1.25)), 0.0012))
    rads[-1] = (0.0006, 0.0006)
    return L.sweep(P, rads, ref=(0, 0, 1), ring=8, step=0.004)


def brow(body, side):
    pts = [(0.010, 1.6335), (0.022, 1.6400), (0.036, 1.6430), (0.050, 1.6405), (0.063, 1.6330)]
    pts = [(x * side, z) for x, z in pts]
    P = arc_on_surface(body, pts, 0.0018)
    rads = [(0.0038, 0.0018), (0.0040, 0.0019), (0.0034, 0.0017), (0.0024, 0.0014), (0.0010, 0.0009)]
    return L.sweep(P, rads, ref=(0, 0, 1), ring=8, step=0.005)


MOUTH = L.lin((0.45, 0.20, 0.22))


def mouth(body):
    pts = []
    for i in range(9):
        x = -0.0165 + 0.033 * i / 8
        z = 1.5475 + 0.0022 * (x / 0.0165) ** 2 - 0.0006 * (1 - (x / 0.0165) ** 2)
        pts.append((x, z))
    P = arc_on_surface(body, pts, 0.0006)
    rads = [(0.0006 + 0.0007 * math.sin(math.pi * i / 8), 0.0007) for i in range(9)]
    return L.sweep(P, rads, ref=(0, 0, 1), ring=8, step=0.004)


def build(body_name="BF_BodyRM"):
    body = bpy.data.objects[body_name]
    col = bpy.data.collections["BaseFemale"]
    parts, colors = [], []
    for side in (1, -1):
        v, f, c, ec, M = eye(body, side)
        parts.append((v, f, None))
        colors.append(c)
        lv, lf, _ = lash(body, side, ec)
        parts.append((lv, lf, None))
        colors.append([LASH] * len(lv))
        bv, bf, _ = brow(body, side)
        parts.append((bv, bf, None))
        colors.append([BROW] * len(bv))
    mv, mf, _ = mouth(body)
    parts.append((mv, mf, None))
    colors.append([MOUTH] * len(mv))
    ob = L.build_object("BF_Face", parts, col)
    flat = [c for cs in colors for c in cs]
    attr = ob.data.color_attributes.new("Col", 'FLOAT_COLOR', 'POINT')
    for i, c in enumerate(flat):
        attr.data[i].color = c
    L.shade_smooth(ob)
    return ob
