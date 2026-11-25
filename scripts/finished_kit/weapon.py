"""A generated weapon in the kit's grip-socket frame: scaled to length, blade and grip thinned to game proportions
(a single-image generation guesses depth), and placed so the right hand's socket holds it.
Socket axes (ADR 0053): +Y along the gripped shaft towards the thumb side (towards the blade), +Z the way the
knuckles point (the cutting edge), +X = Y x Z.
python weapon.py RAW.glb OUT.npz [--length M] [--blade-depth F] [--grip-width F] [--hand M]
-> OUT.npz: dense V (socket frame, metres), F, CUV (raw texture UVs per face corner), edge (none: a closed prop)"""
import sys
import numpy as np
from kitlib import *

a = sys.argv[1:]; raw, out = a[0], a[1]
opt = lambda k, d: float(a[a.index(k) + 1]) if k in a else d
LENGTH, BLADE_DEPTH, GRIP_WIDTH, HAND = opt('--length', 1.35), opt('--blade-depth', 0.55), opt('--grip-width', 0.6), opt('--hand', 0.05)
g, buf, P, UV, F = load_mesh(raw)
V, Fw, inv, ok = weld(gltf_to_blender(P), F, 1e-7); CUV = UV[F[ok]]
V = V * (LENGTH / np.ptp(V[:, 2])); V[:, 2] -= V[:, 2].min()                # upright, point at z = 0
# the long axis profile: width across the blade (x) per height
zs = np.linspace(0, LENGTH, 136); width = np.array([np.ptp(V[(V[:, 2] >= z) & (V[:, 2] < z + 0.01), 0]) if ((V[:, 2] >= z) & (V[:, 2] < z + 0.01)).any() else 0 for z in zs])
guard = zs[np.argmax(width)]                                                # the crossguard is the widest part
blade_top = zs[(zs < guard) & (width < 0.6 * width.max())].max() + 0.01     # where the guard starts
above = (zs > guard) & (width < 0.6 * width.max())
grip_lo = zs[above].min()                                                    # where the grip leaves the guard
wide = above & (zs > grip_lo + 0.05) & (width > np.median(width[above & (zs < grip_lo + 0.15)]) * 1.15)
grip_hi = zs[wide].min() if wide.any() else zs[above].max()                 # the pommel starts where it widens again
print('blade 0..%.3f, guard %.3f, grip %.3f..%.3f, pommel to %.3f m' % (blade_top, guard, grip_lo, grip_hi, LENGTH))
axis = V[(V[:, 2] > grip_lo + 0.02) & (V[:, 2] < grip_hi - 0.02)][:, :2].mean(0)   # the grip's axis
ramp = lambda z, z0, z1: np.clip((z - z0) / (z1 - z0), 0, 1)
smooth = lambda t: t * t * (3 - 2 * t)
# blade: thinner across its flat (y), fading out into the guard
tb = 1 - smooth(ramp(V[:, 2], blade_top - 0.03, blade_top))
V[:, 1] = axis[1] + (V[:, 1] - axis[1]) * (1 - (1 - BLADE_DEPTH) * tb)
# grip: thinner around its axis, fading in from the guard and out into the pommel
tg = smooth(ramp(V[:, 2], grip_lo, grip_lo + 0.015)) * (1 - smooth(ramp(V[:, 2], grip_hi - 0.015, grip_hi)))
V[:, :2] = axis + (V[:, :2] - axis) * (1 - (1 - GRIP_WIDTH) * tg)[:, None]
grip_w = np.ptp(V[(V[:, 2] > grip_lo + 0.03) & (V[:, 2] < grip_hi - 0.03), 0]); blade_d = np.ptp(V[(V[:, 2] > 0.3) & (V[:, 2] < 0.6), 1])
print('after thinning: grip %.1f mm across, blade %.1f mm deep' % (grip_w * 1e3, blade_d * 1e3))
# socket frame: origin where the right hand holds (HAND above the guard), +Y to the tip, +Z along the edge (x)
origin = np.array([axis[0], axis[1], grip_lo + HAND])
p = V - origin; L = np.stack([-p[:, 1], -p[:, 2], p[:, 0]], 1)            # (X, Y, Z) = (-y, -z, x)
np.savez_compressed(out, V=L, F=Fw, CUV=CUV, edge=np.zeros(len(L), bool))
print('wrote %s: %d verts, %d tris; tip at Y %.3f, pommel at Y %.3f' % (out, len(L), len(Fw), L[:, 1].max(), L[:, 1].min()))
