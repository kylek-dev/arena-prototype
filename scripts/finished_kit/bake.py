"""Transfer Meshy's paint onto the fitted mesh's UVs by closest surface point (no ray misses).
Around the right eye the paint is taken from the mirrored left eye, so both real eye openings match
their surroundings; the lid's inner edge gets the lash-line and lower-lid tones.
python bake.py RES  ->  work/bake_dense.png, work/bake_low.png, work/bake_cloth.png, work/bake_geo.npz"""
import sys
import numpy as np
from PIL import Image
from scipy import ndimage
import igl
from kitlib import *
from mh import MH

R = int(sys.argv[1]) if len(sys.argv) > 1 else 2048
mh = MH(); fit = np.load(KIT + 'work/fit_eyes.npz'); V, Q, keep = fit['V'], fit['Q'], fit['face_keep']
is_body = mh.body[mh.Q].all(1); UVq = mh.loop_uv.reshape(-1, 4, 2)[is_body][keep]      # per-corner UVs of the kept body quads
Q = Q[keep]
body = np.where(mh.body)[0]; loc = -np.ones(len(mh.V), int); loc[body] = np.arange(len(body)); mirror = loc[mh.mirror[body]]
loops = np.load(KIT + 'work/eye_loops_l.npy')
mw = np.zeros(len(V)); 
for k in range(4, 11): mw[mirror[loops[k]]] = np.clip((k - 3) / 4, 0, 1)                 # right-eye rings: blend to mirrored paint
mw[mirror[fit['lid_strip'].ravel()]] = 1
T = np.concatenate([Q[:, [0, 1, 2]], Q[:, [0, 2, 3]]]); TUV = np.concatenate([UVq[:, [0, 1, 2]], UVq[:, [0, 2, 3]]])
tuv = TUV.copy(); tuv[..., 1] = 1 - tuv[..., 1]                                         # image rows run down

tid, bary = raster(tuv, R); m = tid >= 0
print('texels covered: %.1f%%' % (m.mean() * 100))
P = np.einsum('nk,nkj->nj', bary[m], V[T[tid[m]]])
MW = np.einsum('nk,nk->n', bary[m], mw[T[tid[m]]]); Pm = P * np.array([-1, 1, 1.0])
# lid inner edge (margin -> eye surface): which texels, and whether upper or lower lid
strip = np.zeros(len(V), int); mg, inn = fit['lid_strip']; n_ = len(mg)
is_up = np.zeros(n_, bool); is_up[fit['upper']] = True; is_up[fit['lower'][[0, -1]]] = False
for arr in (mg, inn):
    strip[arr] = np.where(is_up, 2, 1); strip[mirror[arr]] = np.where(is_up, 2, 1)
fs = strip[T]; strip_face = (fs > 0).all(1); strip_up = (fs == 2).sum(1) >= 2
STRIP = strip_face[tid[m]]; STRIP_UP = strip_up[tid[m]]

def fill(img, mask):
    idx = ndimage.distance_transform_edt(~mask, return_distances=False, return_indices=True)
    return img[idx[0], idx[1]]

def transfer(SV, SF, SUV, tex, extra=None):
    d2, fi, C = igl.point_mesh_squared_distance(P, SV, SF)
    a, b, c = SV[SF[fi, 0]], SV[SF[fi, 1]], SV[SF[fi, 2]]
    L = igl.barycentric_coordinates_tri(C, a, b, c)
    uv = np.einsum('nk,nkj->nj', L, SUV[fi])
    c = bilinear(tex, uv)
    if MW.max() > 0:                                     # mirrored sampling around the right eye
        sel = MW > 0; _, fj, Cj = igl.point_mesh_squared_distance(Pm[sel], SV, SF)
        Lj = igl.barycentric_coordinates_tri(Cj, SV[SF[fj, 0]], SV[SF[fj, 1]], SV[SF[fj, 2]])
        cm = bilinear(tex, np.einsum('nk,nkj->nj', Lj, SUV[fj])); c[sel] = c[sel] * (1 - MW[sel, None]) + cm * MW[sel, None]
    c[STRIP & STRIP_UP] = (0.30, 0.17, 0.12); c[STRIP & ~STRIP_UP] = (0.70, 0.44, 0.33)
    out = np.zeros((R, R, 3), np.float32); out[m] = c
    res = [fill(out, m)]
    if extra is not None:
        e = np.zeros((R, R), np.float32); e[m] = np.einsum('nk,nk->n', L, extra[SF[fi]]); res.append(e)
    return res + [np.sqrt(d2)]

save = lambda name, img: Image.fromarray((np.clip(img, 0, 1) * 255 + 0.5).astype(np.uint8)).save(KIT + 'work/' + name)
tg = np.load(KIT + 'work/target_clean.npz')
tex = np.asarray(Image.open(KIT + 'src/dense_basecolor.png').convert('RGB'), np.float32) / 255
col, cloth, dist = transfer(tg['V_raw'], tg['F'], tg['corner_uv'].astype(np.float64), tex, tg['cloth'].astype(np.float64))
save('bake_dense.png', col); save('bake_cloth.png', np.repeat(cloth[..., None], 3, 2))
_, band, _ = transfer(tg['V_raw'], tg['F'], tg['corner_uv'].astype(np.float64), tex, tg['band'].astype(np.float64))
pos = np.zeros((R, R, 3), np.float32); pos[m] = P
np.savez_compressed(KIT + 'work/bake_aux.npz', pos=fill(pos, m).astype(np.float32), band=band.astype(np.float16), cloth=cloth.astype(np.float16), mask=m)
dc = np.sqrt(igl.point_mesh_squared_distance(P, tg['V'], tg['F'])[0]); dm = np.zeros((R, R), np.float32); dm[m] = dc
np.save(KIT + 'work/bake_dist.npy', fill(dm, m))                                        # fit -> cleaned sculpt distance per texel (masks the normal bake)
print('dense: surface distance mm p50 %.2f p99 %.2f' % (np.median(dist) * 1e3, np.percentile(dist, 99) * 1e3))
g, buf, LP, LUV, LF = load_mesh(LOW_GLB)
tex = np.asarray(Image.open(KIT + 'src/low_basecolor.png').convert('RGB'), np.float32) / 255
col, dist = transfer(low_to_work(LP), LF, LUV[LF], tex)
save('bake_low.png', col)
print('low: surface distance mm p50 %.2f p99 %.2f' % (np.median(dist) * 1e3, np.percentile(dist, 99) * 1e3))
np.savez_compressed(KIT + 'work/bake_geo.npz', tid=tid, bary=bary, T=T, TUV=TUV)
