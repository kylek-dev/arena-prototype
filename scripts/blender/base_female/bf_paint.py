"""Hand-painted skin via vertex colours (light painted in, warm shadows)."""
import math
import bpy
from mathutils import Vector as V
import bf_lib as L
import bf_rig as R

SKIN = L.lin((0.97, 0.77, 0.63))
SKIN_SH = L.lin((0.80, 0.50, 0.44))
SKIN_HI = L.lin((1.0, 0.89, 0.79))
BLUSH = L.lin((0.96, 0.58, 0.55))
LIP = L.lin((0.86, 0.47, 0.47))
MOUTH = L.lin((0.48, 0.22, 0.24))


def g(p, c, r):
    return math.exp(-((p - V(c)).length / r) ** 2)


def skin(p, n):
    light = n.z * 0.55 - n.y * 0.2 + 0.05
    c = L.mix(SKIN, SKIN_HI, max(0.0, light) * 0.55)
    c = L.mix(c, SKIN_SH, max(0.0, -light) * 0.55)
    # cooler/darker toward the feet, warmer extremities
    c = L.mix(c, SKIN_SH, 0.25 * L.smooth01((0.6 - p.z) / 0.5))
    if p.z > 1.45:
        for s in (1, -1):
            c = L.mix(c, BLUSH, 0.45 * g(p, (0.044 * s, -0.058, 1.572), 0.017))
            c = L.mix(c, BLUSH, 0.30 * g(p, (0.077 * s, 0.02, 1.60), 0.02))   # ears
        c = L.mix(c, BLUSH, 0.25 * g(p, (0, -0.079, 1.582), 0.008))         # nose tip
        # lips
        if p.y < -0.05:
            w = 0.017 * (1 - ((p.z - 1.547) / 0.011) ** 2) if abs(p.z - 1.547) < 0.011 else 0
            if abs(p.x) < w:
                c = L.mix(c, LIP, 0.75 * (1 - (abs(p.x) / max(w, 1e-4)) ** 3))
    return c


def hand_skin(p, n):
    c = skin(p, n)
    side = 1 if p.x > 0 else -1
    W = V((R.W.x * side, R.W.y, R.W.z))
    D = V((R.D.x * side, R.D.y, R.D.z))
    t = (p - W).dot(D)
    return L.mix(c, BLUSH, 0.35 * L.smooth01((t - 0.11) / 0.06))


def paint(ob, fn):
    me = ob.data
    if "Col" in me.color_attributes:
        me.color_attributes.remove(me.color_attributes["Col"])
    a = me.color_attributes.new("Col", 'FLOAT_COLOR', 'POINT')
    me.update()
    for v in me.vertices:
        a.data[v.index].color = fn(v.co, v.normal)
