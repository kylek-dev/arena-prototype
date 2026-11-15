"""Landmarks and 45-bone skeleton for BaseFemale (left side +X, faces -Y, Z up)."""
import math
from mathutils import Vector as V, Matrix

ARM_DROP = 42.0  # A-pose: degrees below horizontal

S = V((0.172, 0.012, 1.405))           # shoulder joint
_d1 = V((math.cos(math.radians(ARM_DROP)), 0.0, -math.sin(math.radians(ARM_DROP))))
E = S + _d1 * 0.28 + V((0, 0.012, 0))  # elbow (slightly back)
_d2 = (V((_d1.x, -0.075, _d1.z))).normalized()
W = E + _d2 * 0.245                    # wrist
D = _d2                                # hand axis
N = (-(D.cross(V((0, 1, 0))))).normalized()   # palm normal (faces body/down)
F = D.cross(N).normalized()            # thumb direction (forward)
if F.y > 0:
    F = -F
K = W + D * 0.092                      # middle knuckle

HIP = V((0.088, 0.002, 0.905))
KNEE = V((0.097, -0.012, 0.49))
ANKLE = V((0.102, 0.018, 0.088))
BALL = V((0.106, -0.118, 0.026))
TOE = V((0.106, -0.195, 0.024))


def rot_about(v, axis, deg):
    return Matrix.Rotation(math.radians(deg), 3, axis) @ v


# finger definitions: name, lateral offset (along F), knuckle recess (along D),
# spread deg (toward F), segment lengths, radius
FINGERS = [
    ("index", 0.0325, 0.006, 5.0, (0.044, 0.040), 0.0094),
    ("middle", 0.0110, 0.000, 0.0, (0.048, 0.043), 0.0097),
    ("ring", -0.0110, 0.005, -5.0, (0.045, 0.040), 0.0091),
    ("pinky", -0.0310, 0.016, -11.0, (0.036, 0.033), 0.0081),
]
CURL = 6.0  # rest curl per joint, degrees toward palm


def finger_chain(lat, recess, spread, lens):
    base = K + F * lat - D * recess
    d = (D * math.cos(math.radians(spread)) + F * math.sin(math.radians(spread))).normalized()
    axis = d.cross(N).normalized()     # rotating +deg about this curls toward palm
    pts = [base]
    cur = d
    for L in lens:
        cur = rot_about(cur, axis, CURL)
        pts.append(pts[-1] + cur * L)
    return pts


def thumb_chain():
    base = W + D * 0.018 + F * 0.022 + N * 0.012
    d = (D * 0.62 + F * 0.62 + N * 0.42).normalized()
    pts = [base]
    lens = (0.044, 0.034, 0.029)
    cur = d
    for i, L in enumerate(lens):
        pts.append(pts[-1] + cur * L)
        cur = (cur + D * 0.18).normalized()
    return pts


def bones_left():
    """name -> (head, tail, parent, roll_ref)"""
    b = {}
    fwd = V((0, -1, 0))
    b["clavicle.L"] = (V((0.022, -0.012, 1.392)), S + V((-0.005, -0.002, 0.0)), "spine_03", V((0, 0, 1)))
    b["upper_arm.L"] = (S, E, "clavicle.L", V((0, 1, 0)))
    b["forearm.L"] = (E, W, "upper_arm.L", V((0, 1, 0)))
    b["hand.L"] = (W, K, "forearm.L", -N)
    for name, lat, rec, spr, lens, _ in FINGERS:
        pts = finger_chain(lat, rec, spr, lens)
        b[f"{name}_01.L"] = (pts[0], pts[1], "hand.L", -N)
        b[f"{name}_02.L"] = (pts[1], pts[2], f"{name}_01.L", -N)
    tp = thumb_chain()
    for i in range(3):
        b[f"thumb_0{i + 1}.L"] = (tp[i], tp[i + 1], "hand.L" if i == 0 else f"thumb_0{i}.L", -N)
    b["thigh.L"] = (HIP, KNEE, "pelvis", fwd)
    b["shin.L"] = (KNEE, ANKLE, "thigh.L", fwd)
    b["foot.L"] = (ANKLE, BALL, "shin.L", V((0, 0, 1)))
    b["toe.L"] = (BALL, TOE, "foot.L", V((0, 0, 1)))
    return b


def all_bones():
    fwd = V((0, -1, 0))
    b = {
        "root": (V((0, 0, 0)), V((0, -0.25, 0)), None, V((0, 0, 1))),
        "pelvis": (V((0, 0.004, 0.935)), V((0, 0.004, 1.035)), "root", fwd),
        "spine_01": (V((0, 0.004, 1.035)), V((0, 0.0, 1.145)), "pelvis", fwd),
        "spine_02": (V((0, 0.0, 1.145)), V((0, 0.0, 1.27)), "spine_01", fwd),
        "spine_03": (V((0, 0.0, 1.27)), V((0, 0.006, 1.405)), "spine_02", fwd),
        "neck": (V((0, 0.008, 1.425)), V((0, 0.014, 1.515)), "spine_03", fwd),
        "head": (V((0, 0.014, 1.515)), V((0, 0.014, 1.72)), "neck", fwd),
    }
    left = bones_left()
    b.update(left)
    for n, (h, t, p, r) in left.items():
        m = lambda v: V((-v.x, v.y, v.z))
        pr = p.replace(".L", ".R") if p else p
        b[n.replace(".L", ".R")] = (m(h), m(t), pr, m(r))
    return b
