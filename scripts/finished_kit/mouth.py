"""Stage 3b: a closed mouth. The scaffold's lips enclose a mouth pocket and meet along their own wavy
contact line, which showed in game as a second line through the painted mouth. Here the pocket is removed
and the two lip edges are closed onto one line that follows the mouth line Meshy painted.
work/fit_eyes.npz (updated in place: V, face_keep, mouth_upper, mouth_lower)"""
import collections
import numpy as np
from PIL import Image
import igl
from kitlib import *
from mh import MH
from eyelib import eye_loops

mh = MH(); body = np.where(mh.body)[0]; loc = -np.ones(len(mh.V), int); loc[body] = np.arange(len(body)); mirror = loc[mh.mirror[body]]
f = dict(np.load(KIT + 'work/fit_eyes.npz')); V, Q, keep = f['V'].copy(), f['Q'], f['face_keep'].copy()
Vb = mh.V[body]; lips = mh.mask('lips')[body]; c_b = Vb[lips].mean(0); Qk = Q[keep]

# --- lip rings: the chain that starts at the lip outline and narrows to the contact line
loops = [L for L in eye_loops(Vb, Qk, c_b, reach=0.045) if len(L) == 38]
nbr = collections.defaultdict(set)
for q in Qk:
    for k in range(4): nbr[q[k]].add(q[(k + 1) % 4]); nbr[q[(k + 1) % 4]].add(q[k])
sets = [set(L) for L in loops]
adj = {i: [j for j in range(len(loops)) if j != i and any(nbr[v] & sets[j] for v in loops[i])] for i in range(len(loops))}
chains = []
for s in [i for i, a in adj.items() if len(a) == 1]:
    if any(s in c for c in chains): continue
    order = [s]
    while True:
        nx = [j for j in adj[order[-1]] if j not in order]
        if not nx: break
        order.append(nx[0])
    chains.append(order)
zspan = lambda i: np.ptp(Vb[loops[i]][:, 2]); front = lambda c: Vb[loops[c[0]]][:, 1].mean()
lip_chain = min(chains, key=lambda c: min(front(c), front(c[::-1])))            # the chain that reaches furthest forward
if zspan(lip_chain[0]) < zspan(lip_chain[-1]): lip_chain = lip_chain[::-1]      # outline first, contact line last
A = [list(loops[lip_chain[0]])]
for i in lip_chain[1:]:
    s = set(loops[i]); A.append([next(iter(nbr[v] & s)) for v in A[-1]])
A = np.array(A); CONTACT = len(A) - 1; print('lip rings: %d (outline to contact), %d vertices each' % (len(A), A.shape[1]))

# --- the pocket behind the contact ring goes
contact = set(A[CONTACT].tolist()); other = [c for c in chains if c is not lip_chain and set(c) != set(lip_chain)]
seed = set(v for c in other for i in c for v in loops[i]) - set(A.ravel().tolist())
pocket = set(seed); grow = list(seed)
while grow:
    v = grow.pop()
    for u in nbr[v]:
        if u not in pocket and u not in contact and u not in set(A.ravel().tolist()): pocket.add(u); grow.append(u)
pm = np.zeros(len(V), bool); pm[list(pocket)] = True
gone = pm[Q].any(1) & keep; keep &= ~pm[Q].any(1)
print('mouth pocket removed: %d vertices, %d quads' % (pm.sum(), gone.sum()))

# --- Meshy's painted mouth line, read off the sculpt's own texture
tg = np.load(KIT + 'work/target.npz'); TV, TF = tg['V'], tg['F']; cuv = tg['corner_uv'].astype(np.float64)
tc = TV[TF].mean(1); local = np.where((np.abs(tc[:, 0]) < 0.05) & (tc[:, 2] > 1.61) & (tc[:, 2] < 1.69) & (tc[:, 1] < -0.04))[0]; LF = TF[local]
tex = np.asarray(Image.open(KIT + 'src/dense_basecolor.png').convert('RGB'), np.float32) / 255
_A, _B, _C = TV[LF[:, 0]], TV[LF[:, 1]], TV[LF[:, 2]]
_den = (_B[:, 2] - _C[:, 2]) * (_A[:, 0] - _C[:, 0]) + (_C[:, 0] - _B[:, 0]) * (_A[:, 2] - _C[:, 2])
def surf_y(x, z):
    with np.errstate(divide='ignore', invalid='ignore'):
        l0 = ((_B[:, 2] - _C[:, 2]) * (x - _C[:, 0]) + (_C[:, 0] - _B[:, 0]) * (z - _C[:, 2])) / _den
        l1 = ((_C[:, 2] - _A[:, 2]) * (x - _C[:, 0]) + (_A[:, 0] - _C[:, 0]) * (z - _C[:, 2])) / _den
    l2 = 1 - l0 - l1; ins = (l0 >= -1e-9) & (l1 >= -1e-9) & (l2 >= -1e-9)
    return (l0[ins] * _A[ins, 1] + l1[ins] * _B[ins, 1] + l2[ins] * _C[ins, 1]).min()
ring0 = V[A[0]]; zc = V[A[CONTACT]][:, 2].mean(); xm = np.abs(V[A[CONTACT]][:, 0]).max()
xs = np.linspace(-xm, xm, 81); zs = np.arange(zc - 0.006, zc + 0.006, 0.0002)
gx, gz = np.meshgrid(xs, zs); P = np.stack([gx.ravel(), np.array([surf_y(x, z) for x, z in zip(gx.ravel(), gz.ravel())]), gz.ravel()], 1)
_, fi, C = igl.point_mesh_squared_distance(P, TV, LF)
Lb = igl.barycentric_coordinates_tri(C, TV[LF[fi, 0]], TV[LF[fi, 1]], TV[LF[fi, 2]])
lum = (bilinear(tex, np.einsum('nk,nkj->nj', Lb, cuv[local[fi]])) @ np.array([0.2126, 0.7152, 0.0722])).reshape(gz.shape)
import cv2
lum = cv2.GaussianBlur(lum.astype(np.float32), (0, 0), 2)
line = zs[np.argmin(lum, axis=0)]
line = (line + line[::-1]) / 2                                                  # the mesh is symmetric
coef = np.polyfit(xs, line, 6); z_line = lambda x: np.polyval(coef, x)
print('painted mouth line: z %.1f mm at the centre, %.1f mm at the corners; the scaffold lips met at %.1f mm' % (z_line(0) * 1e3, z_line(xm) * 1e3, zc * 1e3))

# --- close the contact ring onto that line. The scaffold's upper lip edge has more vertices than the lower
# one, so the lower edge defines the line and every upper vertex is put exactly on it (on a lower vertex
# or on the straight segment between two): the two edges then coincide with no gap.
m = A[CONTACT]; n = len(m); j_a, j_b = int(np.argmax(Vb[m, 0])), int(np.argmin(Vb[m, 0]))
fwd = [(j_a + i) % n for i in range((j_b - j_a) % n + 1)]; bwd = [(j_a - i) % n for i in range((j_a - j_b) % n + 1)]
upper, lower = (fwd, bwd) if Vb[m[fwd], 2].mean() > Vb[m[bwd], 2].mean() else (bwd, fwd)
lo, up = m[lower], m[upper]                                                    # both run corner (+x) to corner (-x), ends shared
sym_x = lambda ids: (V[ids, 0] - V[mirror[ids], 0]) / 2                         # exactly mirrored x positions
on_line = lambda x: np.array([x, surf_y(x, z_line(x)) + 0.0006, z_line(x)])    # a slight crease
xl = sym_x(lo); xl[0], xl[-1] = xm, -xm
assert np.all(np.diff(xl) < 0), 'lower lip edge does not run monotonically across the mouth'
Pl = np.array([on_line(x) for x in xl]); Pl = (Pl + Pl[::-1] * np.array([-1, 1, 1.0])) / 2
V[lo] = Pl
xu = np.clip(sym_x(up), -xm, xm); xu[0], xu[-1] = xm, -xm
snap = {int(np.argmin(np.abs(xu[1:-1] - x))) + 1: i for i, x in enumerate(xl) if 0 < i < len(xl) - 1}   # one upper vertex per lower vertex
seg = np.zeros(len(up), int); tt = np.zeros(len(up))
for j, x in enumerate(xu):
    if j in snap: seg[j], tt[j] = snap[j], 0.0
    else:
        i = int(np.clip(np.searchsorted(-xl, -x, side='right') - 1, 0, len(xl) - 2)); seg[j] = i; tt[j] = np.clip((xl[i] - x) / (xl[i] - xl[i + 1]), 0, 1)
seg[-1], tt[-1] = len(xl) - 2, 1.0
order = np.argsort(seg + tt, kind='stable'); assert np.all(order == np.arange(len(up))), 'upper lip vertices would cross on the line'
V[up] = Pl[seg] * (1 - tt[:, None]) + Pl[np.minimum(seg + 1, len(xl) - 1)] * tt[:, None]
HOLD = max(CONTACT - 4, 1)                                                      # spread the inner lip rings evenly, on the sculpt
for k in range(HOLD + 1, CONTACT):
    s_ = (k - HOLD) / (CONTACT - HOLD)
    V[A[k]] = igl.point_mesh_squared_distance(V[A[HOLD]] * (1 - s_) + V[A[CONTACT]] * s_, TV, LF)[2]
for it in range(15):
    for k in range(HOLD, CONTACT):
        idx = A[k]; avg = (V[np.roll(idx, 1)] + V[np.roll(idx, -1)] + V[A[k - 1]] + V[A[k + 1]]) / 4
        V[idx] = igl.point_mesh_squared_distance(V[idx] * 0.5 + avg * 0.5, TV, LF)[2]
moved = np.unique(A[HOLD:CONTACT].ravel()); V[moved] = (V[moved] + V[mirror[moved]] * np.array([-1, 1, 1.0])) / 2
f.update(V=V, face_keep=keep, mouth_lower=lo, mouth_upper=up, mouth_seg=seg, mouth_t=tt)
np.savez_compressed(KIT + 'work/fit_eyes.npz', **f)
print('lips closed: %d lower and %d upper edge vertices on one line' % (len(lo), len(up)))
