"""Stubble: a thin layer of the face's own quads just above the skin (like the brows) with a grey, alpha-blended
texture, tinted by the facial hair colour. Its shape is the full beard's footprint on the kit skin (where the
generated beard sits, from hair_cut.py), with a soft edge and a fine grain, so stubble and beard agree.
python stubble.py BEARD_DENSE.npz   ->  work/stubble.npz, tex/stubble.png"""
import sys
import numpy as np, cv2, igl
from PIL import Image
from scipy.spatial import cKDTree
from kitlib import *
from mh import MH
import layout

FEATHER, ALPHA, GREY, R = 0.004, 0.55, np.array([0.78, 0.78, 0.76]), 512      # GREY averages hairAtlas
mh = MH(); L = layout.compute(mh); V, Q, region, RN = L['V'], L['Q'], L['region'], L['names']
face = (region == RN.index('Head')) | (region == RN.index('Neck'))
HT = quads_to_tris(Q[face])
bd = np.load(sys.argv[1]); BV = bd['V'][~bd['edge']]
foot = igl.signed_distance(BV, V, HT, sign_type=igl.SIGNED_DISTANCE_TYPE_PSEUDONORMAL)[2]   # beard's footprint on the skin
tree = cKDTree(foot)
# the quads the footprint (with its soft edge) touches
fq = np.where(face)[0]; dq = tree.query(V[Q[fq]].reshape(-1, 3))[0].reshape(-1, 4)
quads = Q[fq[(dq < FEATHER * 1.5).any(1)]]; ids = np.unique(quads)
# spherical projection around the jaw: the under-chin, which faces down, still gets texels
C0 = np.array([0.0, -0.01, 1.675])
def sph(P):
    d = P - C0; return np.stack([np.arctan2(d[:, 0], -d[:, 1]), np.arctan2(d[:, 2], np.hypot(d[:, 0], d[:, 1]))], 1)
S = sph(V[ids]); lo, hi = S.min(0), S.max(0); pad = 0.03 * (hi - lo)
uv_of = lambda P: (sph(P) - lo + pad) / (hi - lo + 2 * pad) * np.array([1, -1]) + np.array([0, 1])   # rows down
UV = np.zeros((len(V), 2)); UV[ids] = uv_of(V[ids])
T = quads_to_tris(quads); tid, bary = raster(UV[T], R); m = tid >= 0
P = np.einsum('nk,nkj->nj', bary[m].astype(np.float64), V[T[tid[m]]])
d = tree.query(P)[0]
# inside the footprint (closing the small gaps between its points), then a soft edge about FEATHER wide
inside = np.zeros((R, R), np.float32); inside[m] = d < 0.002
area = 0.5 * np.linalg.norm(np.cross(V[T[:, 1]] - V[T[:, 0]], V[T[:, 2]] - V[T[:, 0]]), axis=1).sum()
px = np.sqrt(area / m.sum())                                                # metres per texel, on average
sig = FEATHER / 2 / px
soft = cv2.GaussianBlur(inside, (0, 0), sig)
cov = np.clip((soft[m] - 0.2) / 0.6, 0, 1); cov = cov * cov * (3 - 2 * cov)
rng = np.random.default_rng(7); grain = cv2.GaussianBlur(rng.random((R, R)).astype(np.float32), (0, 0), 0.8)
grain = (grain - grain.min()) / (grain.max() - grain.min())
a = np.zeros((R, R), np.float32); a[m] = cov * ALPHA * (0.6 + 0.8 * grain[m]).clip(0, 1.4) / 1.0
a = np.clip(a, 0, 1)
g = np.ones((R, R, 3)) * GREY; g[m] *= 0.92 + 0.16 * grain[m][:, None]                       # a little lighter and darker, inside the layer
img = np.dstack([np.clip(g, 0, 1), a])
Image.fromarray((img * 255 + 0.5).astype(np.uint8), 'RGBA').save(KIT + 'tex/stubble.png', optimize=True)
np.savez_compressed(KIT + 'work/stubble.npz', quads=quads, uv=UV[ids], ids=ids)
print('stubble: %d quads, footprint from %d beard points; alpha mean over the layer %.3f, max %.2f; grey average %s'
      % (len(quads), len(foot), a[m].mean(), a.max(), np.round(g[m].mean(0), 3)))
