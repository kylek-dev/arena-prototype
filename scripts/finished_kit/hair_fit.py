"""Fit a generated bust (head with hair or a beard) onto the kit head: a trimmed similarity ICP driven only by
kit skin that is skin on both models, so the hair or beard never pulls the fit.
Then the signed distance of every raw vertex outside the kit head, for cutting the hair piece out.
python hair_fit.py RAW.glb OUT.npz [--beard]   ->  raw mesh in work space (V, F, UV per corner), distance, fit residuals"""
import sys
import numpy as np
import igl
from kitlib import *
from mh import MH
import layout

raw_path, out = sys.argv[1], sys.argv[2]
mh = MH(); L = layout.compute(mh); V, Q, region, RN = L['V'], L['Q'], L['region'], L['names']
headq = Q[(region == RN.index('Head')) | (region == RN.index('Neck'))]
HT = quads_to_tris(headq); hv = np.unique(headq)
# kit points used for the fit: skin on both models. A hair bust has hair above the brows and behind the ears;
# a beard bust (--beard) is bald but has a beard on the jaw, chin, cheeks and upper lip.
P = V[hv]
if '--beard' in sys.argv:
    use = ((P[:, 2] > 1.685) | (P[:, 1] > 0.02)) & (P[:, 2] > 1.50)
else:
    use = (P[:, 2] < 1.745) & ~((P[:, 1] > 0.0) & (P[:, 2] > 1.625)) & (P[:, 2] > 1.50)
K = P[use]; nose_k = V[hv][np.argmin(np.where(np.abs(P[:, 0]) < 0.004, P[:, 1], 9))]

g, buf, RP, RUV, RF = load_mesh(raw_path)
R0 = gltf_to_blender(RP)
BEARD = '--beard' in sys.argv
if BEARD:                                                                 # a bald bust: start from the top of the scalp
    anchor_k = V[hv][np.argmax(P[:, 2])]; anchor_r = R0[np.argmax(R0[:, 2])]
else:                                                                     # hair on top: start from the nose tip
    up = R0[:, 2] > R0[:, 2].min() + 0.6 * np.ptp(R0[:, 2])               # head part of the bust
    mid = up & (np.abs(R0[:, 0]) < 0.02 * np.ptp(R0[:, 0]))
    anchor_k = nose_k; anchor_r = R0[np.where(mid)[0][np.argmin(R0[mid, 1])]]

def umeyama(A, B):                                                        # B ~ s R A + t
    ma, mb = A.mean(0), B.mean(0); a, b = A - ma, B - mb
    U, S, Vt = np.linalg.svd(b.T @ a / len(A)); D = np.eye(3); D[2, 2] = np.sign(np.linalg.det(U @ Vt))
    Rm = U @ D @ Vt; s = np.trace(np.diag(S) @ D) / (a ** 2).sum(1).mean(); return s, Rm, mb - s * Rm @ ma

def icp(s0, iters=40, keep=0.8):
    s, Rm = s0, np.eye(3); t = anchor_k - s * anchor_r
    for it in range(iters):
        Kr = ((K - t) @ Rm) / s                                            # kit points in raw space
        d2, fi, C = igl.point_mesh_squared_distance(Kr, R0, RF)
        Cw = s * C @ Rm.T + t; res = np.linalg.norm(Cw - K, axis=1)
        sel = res <= np.quantile(res, keep)
        s, Rm, t = umeyama(C[sel], K[sel])
    Cw = s * C @ Rm.T + t; res = np.linalg.norm(Cw - K, axis=1)
    return s, Rm, t, res

best = None
for s0 in np.geomspace(0.08, 0.5, 18):
    s, Rm, t, res = icp(s0, iters=12)
    score = np.quantile(res, 0.8)
    print('start %.3f -> scale %.4f  p80 %.2f mm' % (s0, s, score * 1e3))
    if best is None or score < best[0]: best = (score, s)
s, Rm, t, res = icp(best[1], iters=60)
ang = np.degrees(np.arccos(np.clip((np.trace(Rm) - 1) / 2, -1, 1)))
print('fit: scale %.5f, rotation %.2f deg, residual mm p50 %.2f p80 %.2f p95 %.2f' % (s, ang, *(np.quantile(res, [0.5, 0.8, 0.95]) * 1e3)))
W = s * R0 @ Rm.T + t
sd = igl.signed_distance(W, V, HT, sign_type=igl.SIGNED_DISTANCE_TYPE_PSEUDONORMAL)[0]
np.savez_compressed(out, V=W, F=RF, UV=RUV, sd=sd, scale=s, R=Rm, t=t, fit_points=K, fit_res=res)
print('wrote', out)
