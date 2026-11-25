"""Stage 2: non-rigid ICP (Amberg et al. 2007, per-vertex affine with edge stiffness) of the posed
scaffold's body onto the sculpt. Symmetric by construction.  work/pose.npz -> work/fit.npz"""
import sys, time
import numpy as np
import scipy.sparse as sp
from scipy.sparse.linalg import splu
from scipy.spatial import cKDTree
import igl
from kitlib import *
from mh import MH

mh = MH(); body = np.where(mh.body)[0]; n = len(body)
loc = -np.ones(len(mh.V), int); loc[body] = np.arange(n)
Q = loc[mh.Qbody]; Ft = quads_to_tris(Q)
V = np.load(KIT + 'work/pose.npz')['V']; Vb = V[body]
mirror = loc[mh.mirror[body]]; MIR = np.array([-1, 1, 1.0])
tg = np.load(KIT + 'work/target_clean.npz'); TV, TF = tg['V'], tg['F']
TFN = np.cross(TV[TF[:, 1]] - TV[TF[:, 0]], TV[TF[:, 2]] - TV[TF[:, 0]]); TFN /= np.linalg.norm(TFN, axis=1, keepdims=True)

# --- ears keep the scaffold's (artist-made) forms: they take no part in the surface fit and are placed
# afterwards as one scaled rigid piece on the sculpt's ear.
import collections
_nb = collections.defaultdict(set)
for q in Q:
    for k in range(4): _nb[q[k]].add(q[(k + 1) % 4]); _nb[q[(k + 1) % 4]].add(q[k])
ear = mh.mask('ears')[body] & (Vb[:, 0] > 0); ear_i = np.where(ear)[0]

# --- vertices that must not chase the surface: anything facing other scaffold geometry at close range
# (mouth interior, eye sockets behind the eyeballs, nostrils, deep ear folds). Head only.
Wt = mh.weights(); head_w = Wt[:, mh.bone_names.index('head')]
N0 = vertex_normals(Vb, Ft)
occ_src = np.where(mh.body | mh.mask('helper-l-eye') | mh.mask('helper-r-eye') | mh.mask('helper-upper-teeth') | mh.mask('helper-lower-teeth') | mh.mask('helper-tongue'))[0]
tree_all = cKDTree(V[occ_src]); interior = np.zeros(n, bool)
for i in np.where(head_w[body] > 0.5)[0]:
    p, nn = Vb[i], N0[i]
    js = tree_all.query_ball_point(p + nn * 0.012, 0.014)
    if not js: continue
    r = V[occ_src[js]] - p; along = r @ nn; lat = np.linalg.norm(r - np.outer(along, nn), axis=1)
    interior[i] = np.any((along > 0.0025) & (lat < 0.0045))
interior |= interior[mirror]
no_data = interior.copy(); no_data[ear_i] = True; no_data[mirror[ear_i]] = True
print('interior (no data term) verts:', interior.sum())

# --- system pieces
E = np.unique(np.sort(np.concatenate([Q[:, [0, 1]], Q[:, [1, 2]], Q[:, [2, 3]], Q[:, [3, 0]]]), axis=1), axis=0)
ne = len(E); gamma = 1.0
rows = np.repeat(np.arange(ne * 4), 2); k4 = np.tile(np.arange(4), ne)
cols = np.stack([E[:, 0].repeat(4) * 4 + k4, E[:, 1].repeat(4) * 4 + k4], 1).reshape(-1)
vals = np.tile(np.array([1, 1, 1, gamma]), ne).repeat(2) * np.tile([-1.0, 1.0], ne * 4)
MG = sp.csr_matrix((vals, (rows, cols)), shape=(ne * 4, n * 4))
Dm = sp.csr_matrix((np.concatenate([Vb, np.ones((n, 1))], 1).reshape(-1), (np.arange(n).repeat(4), np.arange(n * 4))), shape=(n, n * 4))
MGtMG = (MG.T @ MG).tocsc()

def correspond(P, max_dist, cos_min):
    N = vertex_normals(P, Ft)
    d2, fi, C = igl.point_mesh_squared_distance(P, TV, TF)
    w = ((TFN[fi] * N).sum(1) > cos_min) & (np.sqrt(d2) < max_dist) & ~no_data
    # symmetric targets and weights
    Cs = (C + C[mirror] * MIR) / 2; ws = w & w[mirror]
    return Cs, ws.astype(float), np.sqrt(d2)

X = np.tile(np.concatenate([np.eye(3), np.zeros((1, 3))]), (n, 1))      # identity per vertex (4n x 3)
P = Vb.copy(); t0 = time.time()
schedule = [(60, 0.06, 0.4), (30, 0.05, 0.5), (15, 0.045, 0.6), (8, 0.04, 0.65), (4, 0.035, 0.7), (2, 0.03, 0.7), (1.2, 0.025, 0.7), (0.8, 0.02, 0.7), (0.5, 0.015, 0.7), (0.3, 0.012, 0.7)]
if len(sys.argv) > 1: schedule = schedule[:int(sys.argv[1])]
for alpha, max_dist, cos_min in schedule:
    for inner in range(4):
        U, w, dist = correspond(P, max_dist, cos_min)
        Wd = sp.diags(w)
        A = (alpha ** 2 * MGtMG + Dm.T @ (Wd @ Wd) @ Dm).tocsc()
        rhs = Dm.T @ ((w ** 2)[:, None] * U)
        Xn = splu(A).solve(rhs)
        Pn = Dm @ Xn; step = np.linalg.norm(Pn - P, axis=1).max(); X, P = Xn, Pn
        if step < 2e-4: break
    print('alpha %5.1f  used %5d/%d  median %.2f mm  p95 %.2f mm  max step %.2f mm  (%.0fs)' % (
        alpha, int(w.sum()), n, np.median(dist[w > 0]) * 1e3, np.percentile(dist[w > 0], 95) * 1e3, step * 1e3, time.time() - t0))
P = (P + P[mirror] * MIR) / 2

# --- ear placement: similarity fit (pairs in both directions) of the ear onto the part of the sculpt that
# stands clear of the fitted skull; feathered over four rings, and the feather zone is put back on the skin.
def umeyama(X, Y):
    xm, ym = X.mean(0), Y.mean(0); Xc, Yc = X - xm, Y - ym; U, S, Vt = np.linalg.svd(Yc.T @ Xc / len(X))
    d = np.sign(np.linalg.det(U @ Vt)); R = U @ np.diag([1, 1, d]) @ Vt; sc = (S * [1, 1, d]).sum() / (Xc ** 2).sum() * len(X)
    return sc, R, ym - sc * R @ xm
box = TV[(TV[:, 0] > 0.055) & (TV[:, 2] > 1.65) & (TV[:, 2] < 1.74) & (TV[:, 1] > -0.02) & (TV[:, 1] < 0.05)]
skull = Ft[~ear[Ft].any(1) & (P[Ft][:, :, 0].min(1) > 0.03) & (P[Ft][:, :, 2].min(1) > 1.6)]
dsk = np.sqrt(igl.point_mesh_squared_distance(box, P, skull)[0]); TE = box[dsk > 0.003]
E0 = P[ear_i].copy(); sE, RE, tE = 1.0, np.eye(3), np.zeros(3); te_tree = cKDTree(TE)
for it in range(60):
    cur = sE * E0 @ RE.T + tE; _, j = te_tree.query(cur); _, k = cKDTree(cur).query(TE)
    sE, RE, tE = umeyama(np.concatenate([E0, E0[k]]), np.concatenate([TE[j], TE]))
print('ear: %d sculpt points, scale %.2f, rotation %.1f deg' % (len(TE), sE, np.degrees(np.arccos(np.clip((np.trace(RE) - 1) / 2, -1, 1)))))
we = ear.astype(float); front = set(ear_i.tolist()); feather = []
for ring_w in (0.8, 0.6, 0.4, 0.2):
    nxt = {u for v in front for u in _nb[v] if we[u] == 0}
    for u in nxt: we[u] = ring_w
    feather += list(nxt); front = nxt
sel = np.where(we > 0)[0]; sel = sel[P[sel, 0] > 0]; p = P[sel]
P[sel] = p + we[sel, None] * (sE * p @ RE.T + tE - p)
fz = np.array([v for v in feather if P[v, 0] > 0]); P[fz] = igl.point_mesh_squared_distance(P[fz], TV, TF)[2]
P[mirror[sel]] = P[sel] * MIR
# helpers (eyeballs, teeth, joint cubes ...) follow the affine of their nearest body vertex
out = V.copy(); out[body] = P
helpers = np.where(~mh.body)[0]; _, j = cKDTree(Vb).query(V[helpers])
Xr = X.reshape(n, 4, 3); out[helpers] = np.einsum('ij,ijk->ik', np.concatenate([V[helpers], np.ones((len(helpers), 1))], 1), Xr[j])
d2, _, _ = igl.point_mesh_squared_distance(P, TV, TF); d = np.sqrt(d2) * 1e3
print('final body->sculpt mm: median %.2f p90 %.2f p99 %.2f max %.1f' % (np.median(d), np.percentile(d, 90), np.percentile(d, 99), d.max()))
np.savez_compressed(KIT + 'work/fit.npz', V=out[body], Q=Q, Vall=out, interior=interior, dist=d)
