"""Remove the tank top's sculpted hems (neckline, armholes) from the fit target: smooth a narrow band
around the cloth/skin boundary of the upper body, leaving everything else untouched.
work/target.npz -> work/target_clean.npz (+ cleaned cloth mask and band)"""
import numpy as np
import scipy.sparse as sp
from scipy.sparse.csgraph import connected_components
from scipy.spatial import cKDTree
from kitlib import *

tg = np.load(KIT + 'work/target.npz'); V, F, cloth = tg['V'].copy(), tg['F'], tg['cloth'].copy()
n = len(V)
E = np.concatenate([F[:, [0, 1]], F[:, [1, 2]], F[:, [2, 0]]]); A = sp.coo_matrix((np.ones(len(E)), (E[:, 0], E[:, 1])), shape=(n, n)).tocsr(); A = ((A + A.T) > 0).astype(float)
# clean the paint-derived mask: clothing only exists between mid-thigh and the collar, in large patches
cloth &= (V[:, 2] > 0.6) & (V[:, 2] < 1.56)
for _ in range(2):                                     # close pinholes, drop specks
    cloth = (A @ cloth.astype(float)) / np.maximum(A.sum(1).A1, 1) > 0.5
sub = A[cloth][:, cloth]; nc, lab = connected_components(sub, directed=False); cnt = np.bincount(lab)
keep = np.zeros(n, bool); keep[np.where(cloth)[0][cnt[lab] > 800]] = True; cloth = keep
print('cloth verts', cloth.sum(), 'components kept', (cnt > 800).sum(), 'z range', V[cloth, 2].min().round(3), V[cloth, 2].max().round(3))
nb_cloth = A @ cloth.astype(float); deg = A.sum(1).A1
boundary = ((cloth & (nb_cloth < deg)) | (~cloth & (nb_cloth > 0)))
upper = boundary & (V[:, 2] > 1.20)                    # neckline and armholes; the waist hem is hidden by the trunks
print('upper hem boundary verts', upper.sum(), ' z', V[upper, 2].min().round(3), V[upper, 2].max().round(3))
d, _ = cKDTree(V[upper]).query(V); band = d < 0.016
print('band verts', band.sum())
# membrane smoothing inside the band, fixed outside
L = sp.diags(1 / np.maximum(deg, 1)) @ A
P = V.copy(); w = np.clip((0.016 - d) / 0.006, 0, 1)[:, None]      # full strength in the core, feathered at the rim
for it in range(120):
    P = P + w * 0.6 * (L @ P - P)
P = (P + (P * np.array([-1, 1, 1.0]))[cKDTree(V).query(V * np.array([-1, 1, 1.0]))[1]] * np.array([-1, 1, 1.0])) / 2 if False else P
mv = np.linalg.norm(P - V, axis=1) * 1e3
print('moved: max %.2f mm, mean in band %.2f mm' % (mv.max(), mv[band].mean()))
out = dict(tg); out['V'] = P; out['cloth'] = cloth; out['band'] = band; out['V_raw'] = V
np.savez_compressed(KIT + 'work/target_clean.npz', **out)
