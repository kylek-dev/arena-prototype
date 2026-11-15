"""Build raw (pre-remesh) body and hand volumes."""
import math
import bpy
from mathutils import Vector as V, Matrix
import bf_lib as L
import bf_rig as R


def ell_frame(center, radii, M, seg=20, rings=12):
    v, f, uv = L.ellipsoid((0, 0, 0), radii, (0, 0, 0), seg, rings)
    return [V(center) + M @ p for p in v], f, uv


HEAD_C = V((0, 0.014, 1.606))
HEAD_R = (0.079, 0.094, 0.106)


def _bump(u, center, width):
    c = V(center).normalized()
    d = (u - c).length
    return math.exp(-(d / width) ** 2)


def head_mesh(seg=72, rings=48):
    v, f, uv = L.ellipsoid((0, 0, 0), (1, 1, 1), seg=seg, rings=rings)
    out = []
    rx, ry, rz = HEAD_R
    for u in v:
        u = V(u)
        a, b, h = u.x, u.y, u.z
        x, y, z = a * rx, b * ry, h * rz
        front = L.smooth01(-b / 0.8)
        low = L.smooth01(-h / 0.95)
        # jaw taper toward a soft, narrow chin
        x *= 1.0 - 0.40 * low * (0.35 + 0.65 * front)
        # flatter face plane
        if b < 0:
            y *= 1.0 - 0.14 * front * (1 - 0.5 * low)
        # tuck the back/underside of the skull into the neck
        if b > 0 and h < 0:
            y *= 1.0 - 0.30 * low * L.smooth01(b / 0.8)
            z *= 1.0 - 0.25 * L.smooth01(b / 0.6) * low
        # crown slightly fuller toward the back
        if h > 0:
            y += 0.010 * L.smooth01(h) * (1 if b > 0 else 0.2)
        p = V((x, y, z))
        n = p.normalized()
        # sculpted planes (offsets along the radial direction)
        off = 0.0
        for sx in (1, -1):
            off += -0.0075 * _bump(u, (0.36 * sx, -0.88, 0.06), 0.24)   # eye socket
            off += 0.0040 * _bump(u, (0.40 * sx, -0.80, 0.27), 0.18)    # brow
            off += 0.0045 * _bump(u, (0.58 * sx, -0.70, -0.22), 0.30)   # cheekbone
        off += 0.0030 * _bump(u, (0, -0.94, -0.50), 0.16)              # upper lip
        off += 0.0020 * _bump(u, (0, -0.90, -0.64), 0.14)              # lower lip
        off += -0.0018 * _bump(u, (0, -0.86, -0.71), 0.10)             # under-lip groove
        p += n * off
        out.append(HEAD_C + p)
    return out, f, uv


def body_parts():
    parts = []
    torso = [
        ((0, 0.016, 0.79), (0.120, 0.080)),
        ((0, 0.012, 0.85), (0.172, 0.104)),
        ((0, 0.012, 0.92), (0.182, 0.108)),
        ((0, 0.012, 1.00), (0.160, 0.094)),
        ((0, 0.012, 1.07), (0.128, 0.084)),
        ((0, 0.010, 1.14), (0.134, 0.087)),
        ((0, 0.006, 1.22), (0.150, 0.094)),
        ((0, 0.004, 1.30), (0.162, 0.099)),
        ((0, 0.008, 1.37), (0.170, 0.092)),
        ((0, 0.012, 1.42), (0.140, 0.074)),
    ]
    parts.append(L.sweep([p for p, _ in torso], [r for _, r in torso], ref=(1, 0, 0), ring=32, step=0.015))
    neck = [((0, 0.014, 1.39), (0.046, 0.045)), ((0, 0.018, 1.47), (0.041, 0.042)), ((0, 0.016, 1.54), (0.040, 0.040))]
    parts.append(L.sweep([p for p, _ in neck], [r for _, r in neck], ref=(1, 0, 0), ring=20, step=0.01))
    parts.append(head_mesh())
    nose = [((0, -0.060, 1.618), (0.0062, 0.006)), ((0, -0.070, 1.598), (0.0072, 0.0072)),
            ((0, -0.078, 1.583), (0.0095, 0.0092))]
    parts.append(L.sweep([p for p, _ in nose], [r for _, r in nose], ref=(1, 0, 0), ring=16, step=0.004))
    parts.append(L.ellipsoid((0, -0.073, 1.580), (0.0135, 0.0075, 0.0075), seg=16, rings=10))
    left = []
    left.append(L.ellipsoid((0.076, 0.020, 1.600), (0.010, 0.018, 0.026), rot=(0, 0, -14), seg=16, rings=12))  # ear
    # torso extras
    left.append(L.ellipsoid((0.074, 0.036, 0.885), (0.078, 0.060, 0.082), seg=20, rings=12))  # glute
    left.append(L.ellipsoid((0.058, -0.060, 1.262), (0.058, 0.048, 0.053), rot=(-12, 0, 8), seg=20, rings=12))  # bust
    left.append(L.ellipsoid((0.180, 0.008, 1.384), (0.057, 0.059, 0.061), seg=20, rings=12))  # deltoid
    left.append(L.ellipsoid((0.090, 0.022, 1.418), (0.092, 0.054, 0.036), rot=(0, 14, 0), seg=20, rings=12))  # trap
    # arm
    d1 = (R.E - R.S).normalized()
    arm = [
        (R.S - d1 * 0.02, (0.053, 0.053)),
        (R.S + d1 * 0.09, (0.050, 0.052)),
        (R.S + d1 * 0.19, (0.046, 0.048)),
        (R.E, (0.040, 0.039)),
        (R.E + (R.W - R.E) * 0.25, (0.044, 0.041)),
        (R.E + (R.W - R.E) * 0.62, (0.037, 0.032)),
        (R.W, (0.029, 0.023)),
    ]
    left.append(L.sweep([p for p, _ in arm], [r for _, r in arm], ref=(0, 1, 0), ring=20, step=0.015))
    # leg
    H, K, A = R.HIP, R.KNEE, R.ANKLE
    leg = [
        (V((0.066, 0.006, 0.97)), (0.070, 0.070)),
        (H + V((-0.006, 0, -0.02)), (0.090, 0.092)),
        (H + (K - H) * 0.30, (0.090, 0.092)),
        (H + (K - H) * 0.66, (0.076, 0.077)),
        (K + V((0, 0, 0.03)), (0.060, 0.062)),
        (K + (A - K) * 0.25 + V((0, 0.012, 0)), (0.062, 0.066)),
        (K + (A - K) * 0.50 + V((0, 0.006, 0)), (0.053, 0.055)),
        (K + (A - K) * 0.80, (0.040, 0.042)),
        (A, (0.036, 0.040)),
    ]
    left.append(L.sweep([p for p, _ in leg], [r for _, r in leg], ref=(1, 0, 0), ring=24, step=0.02))
    foot = [
        ((0.101, 0.064, 0.045), (0.036, 0.042)),
        ((0.102, 0.020, 0.052), (0.042, 0.048)),
        ((0.104, -0.050, 0.040), (0.048, 0.038)),
        ((0.106, -0.118, 0.028), (0.052, 0.027)),
        ((0.106, -0.182, 0.024), (0.045, 0.020)),
    ]
    left.append(L.sweep([p for p, _ in foot], [r for _, r in foot], ref=(1, 0, 0), ring=20, step=0.012))
    parts += left + [L.mirror_part(p) for p in left]
    return parts


def hand_parts():
    D, N, F, W, K = R.D, R.N, R.F, R.W, R.K
    left = []
    palm = [(W - D * 0.012, (0.024, 0.017)), (W + D * 0.04, (0.040, 0.019)), (K - D * 0.002, (0.044, 0.016))]
    left.append(L.sweep([p for p, _ in palm], [r for _, r in palm], ref=tuple(F), ring=20, step=0.008))
    for name, lat, rec, spr, lens, rad in R.FINGERS:
        pts = R.finger_chain(lat, rec, spr, lens)
        d0 = (pts[1] - pts[0]).normalized()
        path = [pts[0] - d0 * 0.012] + pts
        rads = [(rad * 1.05, rad * 0.9), (rad, rad * 0.9), (rad * 0.92, rad * 0.86), (rad * 0.82, rad * 0.76)]
        left.append(L.sweep(path, rads, ref=tuple(F), ring=14, step=0.006))
    tp = R.thumb_chain()
    d0 = (tp[1] - tp[0]).normalized()
    path = [tp[0] - d0 * 0.01] + tp
    rads = [(0.013, 0.012), (0.0125, 0.0115), (0.0112, 0.0102), (0.0098, 0.009), (0.0086, 0.0078)]
    left.append(L.sweep(path, rads, ref=tuple(N), ring=14, step=0.006))
    M = Matrix((D, F, N)).transposed()
    left.append(ell_frame(W + D * 0.036 + F * 0.018 + N * 0.006, (0.030, 0.018, 0.014), M))  # thenar pad
    return left + [L.mirror_part(p) for p in left]


def build():
    col = bpy.data.collections.get("BaseFemale")
    if not col:
        col = bpy.data.collections.new("BaseFemale")
        bpy.context.scene.collection.children.link(col)
    body = L.build_object("BF_BodyRaw", body_parts(), col)
    hands = L.build_object("BF_HandsRaw", hand_parts(), col)
    return body, hands
