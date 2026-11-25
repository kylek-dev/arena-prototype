"""Stage 3: real eyes. Move the scaffold's eyelid margin onto the painted eye outline, give the lid a
thickness, remove the hidden socket pocket, and build the eye pieces (eye white cap, iris disc) behind
the lids. Symmetric.   work/fit.npz -> work/fit_eyes.npz, work/eye_parts.npz, work/eye_white.png"""
import json, collections
import numpy as np
import cv2
import igl
from PIL import Image
from kitlib import *
from mh import MH

RECESS = 0.0012        # eye surface sits this far behind the lid margin
LID_IN = 0.0005        # lid inner edge is pulled this far into the opening
mh = MH(); body = np.where(mh.body)[0]; loc = -np.ones(len(mh.V), int); loc[body] = np.arange(len(body)); mirror = loc[mh.mirror[body]]
f = np.load(KIT + 'work/fit.npz'); V = f['V'].copy(); Q = f['Q']
loops = np.load(KIT + 'work/eye_loops_l.npy')                  # (21, 28) outer skin -> socket
tg = np.load(KIT + 'work/target.npz'); TV, TF, TC = tg['V'], tg['F'], tg['color']
# only the front of the face around the left eye is ever queried: a small local mesh keeps the ray casts fast
_c = TV[TF].mean(1); TF = TF[(np.abs(_c[:, 0] - 0.03) < 0.045) & (np.abs(_c[:, 2] - 1.711) < 0.035) & (_c[:, 1] < -0.03)]
print('local eye-region triangles:', len(TF))
o = json.load(open(KIT + 'work/eye_outline.json'))
up = np.array(o['upper']) * 1e-3; lo = np.array(o['lower']) * 1e-3                    # (x, z) metres, inner -> outer corner
iris_c = np.array(o['iris_centre']) * 1e-3; iris_r = o['iris_radius'] * 1e-3

# Front surface of the sculpt along +Y, by projecting the local triangles onto the XZ plane.
# (libigl's ray caster drops hits for axis-aligned rays on this mesh, so this is done directly.)
_A, _B, _C = TV[TF[:, 0]], TV[TF[:, 1]], TV[TF[:, 2]]
_den = (_B[:, 2] - _C[:, 2]) * (_A[:, 0] - _C[:, 0]) + (_C[:, 0] - _B[:, 0]) * (_A[:, 2] - _C[:, 2])
def hit(x, z):
    with np.errstate(divide='ignore', invalid='ignore'):
        l0 = ((_B[:, 2] - _C[:, 2]) * (x - _C[:, 0]) + (_C[:, 0] - _B[:, 0]) * (z - _C[:, 2])) / _den
        l1 = ((_C[:, 2] - _A[:, 2]) * (x - _C[:, 0]) + (_A[:, 0] - _C[:, 0]) * (z - _C[:, 2])) / _den
    l2 = 1 - l0 - l1; ins = np.where((l0 >= -1e-9) & (l1 >= -1e-9) & (l2 >= -1e-9))[0]
    if not len(ins): raise RuntimeError('no surface at x=%.4f z=%.4f' % (x, z))
    y = l0[ins] * _A[ins, 1] + l1[ins] * _B[ins, 1] + l2[ins] * _C[ins, 1]; k = ins[np.argmin(y)]
    return y.min(), k, l1[k], l2[k]
surf_y = lambda x, z: hit(x, z)[0]
def paint(x, z):
    t, fi, u, v = hit(x, z); a, b, c = TF[fi]; return TC[a] * (1 - u - v) + TC[b] * u + TC[c] * v

# --- radial alignment of the loops: column j is one radial edge path from skin to socket
nbr = collections.defaultdict(set)
for q in Q:
    for k in range(4): nbr[q[k]].add(q[(k + 1) % 4]); nbr[q[(k + 1) % 4]].add(q[k])
A = [list(loops[0])]
for k in range(1, len(loops)):
    s = set(loops[k].tolist()); A.append([next(iter(nbr[v] & s)) for v in A[-1]])
A = np.array(A); MARGIN, INNER = 10, 11
Vb = mh.V[body]                                                # basis shape decides which vertex is which
m = A[MARGIN]; j_in, j_out = np.argmin(Vb[m, 0]), np.argmax(Vb[m, 0]); n = len(m)
fwd = [(j_in + i) % n for i in range((j_out - j_in) % n + 1)]; bwd = [(j_in - i) % n for i in range((j_in - j_out) % n + 1)]
upper, lower = (fwd, bwd) if Vb[m[fwd], 2].mean() > Vb[m[bwd], 2].mean() else (bwd, fwd)
def along(P):
    d = np.concatenate([[0], np.cumsum(np.linalg.norm(np.diff(P, axis=0), axis=1))]); return d / d[-1]
tgt = np.zeros((n, 2))
for chain, curve in ((upper, up), (lower, lo)):
    s = along(Vb[m[chain]][:, [0, 2]]); cs = along(curve)
    tgt[chain] = np.stack([np.interp(s, cs, curve[:, 0]), np.interp(s, cs, curve[:, 1])], 1)
# --- lid skin rings follow with a smooth falloff and stay on the sculpt surface
disp = tgt - V[m][:, [0, 2]]
print('lid margin moves up to %.1f mm' % (np.linalg.norm(disp, axis=1).max() * 1e3))
# Outer rings slide a little and are put back on the sculpt by nearest point (a straight front projection
# would jump onto the nose or temple where the face turns sideways); the margin itself is set exactly.
HOLD = 4                                                        # rings up to here only slide; the fit bunches the inner rings on the closed lid
for k in range(1, HOLD + 1):
    t = k / MARGIN; w = t * t * (3 - 2 * t); idx = A[k]
    moved_ = V[idx] + w * np.stack([disp[:, 0], np.zeros(n), disp[:, 1]], 1)
    V[idx] = igl.point_mesh_squared_distance(moved_, TV, TF)[2]
for j in range(n):
    V[A[MARGIN][j]] = (tgt[j, 0], surf_y(tgt[j, 0], tgt[j, 1]), tgt[j, 1])
for k in range(HOLD + 1, MARGIN):                               # spread the rest evenly between the last sliding ring and the margin
    s_ = (k - HOLD) / (MARGIN - HOLD); idx = A[k]
    V[idx] = igl.point_mesh_squared_distance(V[A[HOLD]] * (1 - s_) + V[A[MARGIN]] * s_, TV, TF)[2]
# relax the lid rings between the fixed outer skin and the margin so no quad folds, staying on the sculpt
for it in range(25):
    for k in range(2, MARGIN):
        idx = A[k]; avg = (V[np.roll(idx, 1)] + V[np.roll(idx, -1)] + V[A[k - 1]] + V[A[k + 1]]) / 4
        V[idx] = igl.point_mesh_squared_distance(V[idx] * 0.5 + avg * 0.5, TV, TF)[2]

# --- the eye surface: a smooth quadric through the sculpt inside the opening, set back behind the lids
poly = (np.concatenate([up, lo[::-1]]) * 1e5).astype(np.float32).reshape(-1, 1, 2)
gx, gz = np.meshgrid(np.linspace(up[0, 0], up[-1, 0], 40), np.linspace(lo[:, 1].min(), up[:, 1].max(), 20))
G = np.stack([gx.ravel(), gz.ravel()], 1); G = G[[cv2.pointPolygonTest(poly, (float(x * 1e5), float(z * 1e5)), False) > 0 for x, z in G]]
gy = np.array([surf_y(x, z) for x, z in G]); c0 = G.mean(0)
basis = lambda P: np.stack([np.ones(len(P)), P[:, 0] - c0[0], P[:, 1] - c0[1], (P[:, 0] - c0[0]) ** 2, (P[:, 0] - c0[0]) * (P[:, 1] - c0[1]), (P[:, 1] - c0[1]) ** 2], 1)
coef = np.linalg.lstsq(basis(G), gy, rcond=None)[0]
eye_y = lambda P: basis(np.atleast_2d(P)) @ coef + RECESS
print('eye surface fit residual: rms %.2f mm, max %.2f mm' % (np.sqrt(((basis(G) @ coef - gy) ** 2).mean()) * 1e3, np.abs(basis(G) @ coef - gy).max() * 1e3))
cen = np.array([np.concatenate([up, lo])[:, 0].mean(), np.concatenate([up, lo])[:, 1].mean()])
for j in range(n):
    p = tgt[j] + LID_IN * (cen - tgt[j]) / max(np.linalg.norm(cen - tgt[j]), 1e-9)
    V[A[INNER][j]] = (p[0], eye_y(p)[0], p[1])
# --- drop the socket pocket behind the lid's inner edge
pocket = set(A[INNER + 1:].ravel().tolist()); grow = list(pocket); inner_set = set(A[INNER].tolist())
while grow:
    v = grow.pop()
    for u in nbr[v]:
        if u not in pocket and u not in inner_set: pocket.add(u); grow.append(u)
pocket_l = np.array(sorted(pocket)); pocket_all = np.zeros(len(V), bool); pocket_all[pocket_l] = True; pocket_all[mirror[pocket_l]] = True
face_keep = ~pocket_all[Q].any(1)
print('socket pocket removed: %d verts, %d quads (both eyes)' % (pocket_all.sum(), (~face_keep).sum()))
# --- mirror the left eye region onto the right
moved = np.unique(A[:INNER + 1].ravel()); V[mirror[moved]] = V[moved] * np.array([-1, 1, 1.0])
# --- eye pieces (left; the build mirrors them)
pad = 0.002; x0, x1 = up[0, 0] - pad, up[-1, 0] + pad; z0, z1 = lo[:, 1].min() - pad, up[:, 1].max() + pad
# eye white: a fan from the centre out to the lid's inner edge (plus a hair under it), so it can never poke through the lids
Pin = np.array([[V[v, 0], V[v, 2]] for v in A[INNER]]); nrm = (Pin - cen) / np.linalg.norm(Pin - cen, axis=1, keepdims=True)
cp = [cen.copy()]; cap_rings = [0.4, 0.75, 1.0]
for t in cap_rings:
    for j in range(n): cp.append(cen + t * (Pin[j] - cen) + (0.0004 * nrm[j] if t == 1.0 else 0))
cp = np.array(cp); cap_V = np.stack([cp[:, 0], eye_y(cp), cp[:, 1]], 1); cap_UV = np.stack([(cp[:, 0] - x0) / (x1 - x0), (cp[:, 1] - z0) / (z1 - z0)], 1)
cap_T = np.array([[0, 1 + j, 1 + (j + 1) % n] for j in range(n)])
cap_Q = np.array([[1 + r * n + j, 1 + (r + 1) * n + j, 1 + (r + 1) * n + (j + 1) % n, 1 + r * n + (j + 1) % n] for r in range(len(cap_rings) - 1) for j in range(n)])
# make the fan face the viewer (-Y)
_a, _b, _c = cap_V[cap_T[0]]
if np.cross(_b - _a, _c - _a)[1] > 0: cap_T = cap_T[:, ::-1]; cap_Q = cap_Q[:, ::-1]
SEG = 24; rings = [0.35, 0.7, 1.0]; ip = [iris_c.copy()]; iuv = [np.array([0.5, 0.5])]
for rr in rings:
    for s in range(SEG):
        a = 2 * np.pi * s / SEG; ip.append(iris_c + iris_r * rr * np.array([np.cos(a), np.sin(a)])); iuv.append(0.5 + 0.5 * rr * np.array([np.cos(a), np.sin(a)]))
ip = np.array(ip); iris_V = np.stack([ip[:, 0], eye_y(ip) - 0.0003, ip[:, 1]], 1); iris_UV = np.array(iuv)     # the iris sits 0.3 mm in front of the eye white
tris = [[0, 1 + (s + 1) % SEG, 1 + s] for s in range(SEG)]
quads = [[1 + r * SEG + (s + 1) % SEG, 1 + (r + 1) * SEG + (s + 1) % SEG, 1 + (r + 1) * SEG + s, 1 + r * SEG + s] for r in range(len(rings) - 1) for s in range(SEG)]
# --- eye white texture: Meshy's own painted eye white, with the painted iris lifted out
RW, RH = 256, 128; img = np.zeros((RH, RW, 3), np.float32)
tx, tz = np.meshgrid(x0 + (np.arange(RW) + 0.5) / RW * (x1 - x0), z1 - (np.arange(RH) + 0.5) / RH * (z1 - z0))
tp = np.stack([tx.ravel(), np.zeros(RW * RH), tz.ravel()], 1); tp[:, 1] = eye_y(tp[:, [0, 2]]) - RECESS
_, fi, C = igl.point_mesh_squared_distance(tp, TV, TF)
L = igl.barycentric_coordinates_tri(C, TV[TF[fi, 0]], TV[TF[fi, 1]], TV[TF[fi, 2]])
img = np.einsum('nk,nkj->nj', L, TC[TF[fi]]).reshape(RH, RW, 3).astype(np.float32)
px = lambda x, z: ((x - x0) / (x1 - x0) * RW, (z1 - z) / (z1 - z0) * RH)
mask = np.zeros((RH, RW), np.uint8); cxp, czp = px(*iris_c); cv2.circle(mask, (int(round(cxp)), int(round(czp))), int(np.ceil(iris_r * 1.12 / (x1 - x0) * RW)), 255, -1)
u8 = (np.clip(img, 0, 1) * 255).astype(np.uint8); white = cv2.inpaint(u8[..., ::-1].copy(), mask, 9, cv2.INPAINT_TELEA)[..., ::-1]
Image.fromarray(u8).save(KIT + 'work/eye_paint_src.png'); Image.fromarray(white).save(KIT + 'work/eye_white.png')
np.savez_compressed(KIT + 'work/fit_eyes.npz', V=V, Q=Q, face_keep=face_keep, lid_strip=np.stack([A[MARGIN], A[INNER]]), upper=np.array(upper), lower=np.array(lower))
np.savez_compressed(KIT + 'work/eye_parts.npz', cap_V=cap_V, cap_Q=cap_Q, cap_T=cap_T, cap_UV=cap_UV, iris_V=iris_V, iris_UV=iris_UV, iris_tris=np.array(tris), iris_quads=np.array(quads), iris_centre=iris_c, iris_radius=iris_r)
print('eye white %d tris + %d quads, iris %d tris + %d quads per eye' % (len(cap_T), len(cap_Q), len(tris), len(quads)))
