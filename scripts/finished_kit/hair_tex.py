"""Textures for a hair or beard piece: the generated paint moved onto the game mesh's UVs by closest surface
point, then made grey for tinting. Texels near the piece's edge (sunk under the skin by hair_cut.py) take the
paint of the hair surface further in, so the generated skin under the hair's edge does not come along.
The grey keeps the painted light and dark of the locks (luminance ratios, with the highlights rolled into a soft
shoulder so they do not clip) and averages ATLAS, so each hair colour's tint lands on its swatch.
python hair_tex.py LOW.npz DENSE.npz RAW_BASECOLOR.png NORMAL.png OUT_PREFIX [--edge MM]
-> OUT_PREFIX.png (grey base colour), OUT_PREFIX_n.png (normal, filled outside the islands)"""
import sys
import numpy as np
from PIL import Image
from scipy import ndimage
from scipy.spatial import cKDTree
import igl
from kitlib import *

a = sys.argv[1:]; low_p, dense_p, raw_tex, nrm_p, out = a[:5]
EDGE = (float(a[a.index('--edge') + 1]) if '--edge' in a else 3.0) / 1e3
ATLAS = np.array([0.78, 0.78, 0.76])                                         # hairAtlas in web/src/domain/appearance.ts
lo, de = np.load(low_p), np.load(dense_p)
tex = np.asarray(Image.open(raw_tex).convert('RGB'), np.float64) / 255
R = Image.open(nrm_p).size[0]
tid, bary = raster(lo['UV'], R); m = tid >= 0
P = np.einsum('nk,nkj->nj', bary[m].astype(np.float64), lo['V'][lo['F'][tid[m]]])
DV, DF, CUV = de['V'], de['F'], de['CUV']
# dense vertices clear of the edge, each with its paint
ev = np.where(de['edge'])[0]; dist_edge = cKDTree(DV[ev]).query(DV)[0]
vuv = np.zeros((len(DV), 2)); vuv[DF.ravel()] = CUV.reshape(-1, 2); vcol = bilinear(tex, vuv)
good = dist_edge > EDGE; tree_good = cKDTree(DV[good])
_, fi, C = igl.point_mesh_squared_distance(P, DV, DF)
L = igl.barycentric_coordinates_tri(C, DV[DF[fi, 0]], DV[DF[fi, 1]], DV[DF[fi, 2]])
bad = ~np.isfinite(L).all(1); L[bad] = 1 / 3                                     # zero-area triangles: take the centre
col = bilinear(tex, np.einsum('nk,nkj->nj', L, CUV[fi]))
near_edge = (L * dist_edge[DF[fi]]).sum(1) < EDGE
k_d, k_i = tree_good.query(C[near_edge], k=6); w = 1 / np.maximum(k_d, 1e-5)
col[near_edge] = (vcol[good][k_i] * w[..., None]).sum(1) / w.sum(1, keepdims=True)
print('texels %d, %.1f%% of the texture; %.1f%% near the edge take the paint further in' % (m.sum(), m.mean() * 100, near_edge.mean() * 100))
lum = col @ np.array([0.2126, 0.7152, 0.0722])
mean = lum.astype(np.float64).mean()
KNEE, TOP = 0.86, 0.98                                                       # highlights roll off into a soft shoulder
def shoulder(g):
    g = g.copy(); o = g > KNEE; g[o] = KNEE + (TOP - KNEE) * np.tanh((g[o] - KNEE) / (TOP - KNEE)); return g
gain = ATLAS[0]
for it in range(60):                                                         # luminance ratios kept; gain set so the average is ATLAS
    g = shoulder(gain * lum / mean); gain *= ATLAS[0] / g.astype(np.float64).mean()
print('paint luminance mean %.3f p0.5 %.3f p99.5 %.3f; gain %.3f, %.1f%% of texels in the shoulder'
      % (mean, np.quantile(lum, 0.005), np.quantile(lum, 0.995), gain / mean, (gain * lum / mean > KNEE).mean() * 100))
grey = np.clip(g[:, None] * (ATLAS / ATLAS[0])[None, :], 0, 1)
img = np.zeros((R, R, 3)); img[m] = grey
idx = ndimage.distance_transform_edt(~m, return_distances=False, return_indices=True); img = img[idx[0], idx[1]]   # fill outside the islands
Image.fromarray((img * 255 + 0.5).astype(np.uint8)).save(out + '.png', optimize=True)
inside = np.asarray(Image.open(out + '.png'), np.float64)[m] / 255
print('grey average over the islands', inside.mean(0).round(3), 'p0.5 %.3f p99.5 %.3f' % tuple(np.quantile(inside[:, 0], [0.005, 0.995])))
n = np.asarray(Image.open(nrm_p).convert('RGB')); n = n[idx[0], idx[1]]
Image.fromarray(n).save(out + '_n.png', optimize=True)
print('wrote', out + '.png', out + '_n.png')
