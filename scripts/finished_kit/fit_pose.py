"""Stage 1: pose and proportion the MakeHuman scaffold into the Meshy sculpt's A-pose.
Torso and head: global similarity fit (ICP). Limbs: joint-driven (hinge plane for arms), with elbow,
wrist, knee and ankle centres measured on the sculpt itself.  -> work/pose.npz"""
import json
import numpy as np
import igl
from scipy.spatial import cKDTree
from kitlib import *
from mh import MH

mh = MH(); W = mh.weights(); bn = mh.bone_names; B = {n: i for i, n in enumerate(bn)}
tg = np.load(KIT + 'work/target.npz'); TV, TF = tg['V'], tg['F']; TN = vertex_normals(TV, TF)
tree = cKDTree(TV)
MJ = json.load(open(KIT + 'work/meshy_joints.json'))
def mj(name):                       # Meshy joint, symmetrised
    p = np.array(MJ[name]); o = name.replace('_l', '_r') if name.endswith('_l') else name
    q = np.array(MJ[o]) * np.array([-1, 1, 1]) if o != name else p * np.array([-1, 1, 1])
    r = (p + q) / 2
    if o == name: r[0] = 0
    return r

def unit(v): return v / np.linalg.norm(v)
def frame(d, n):                    # orthonormal frame from a direction and a plane normal
    d = unit(d); n = unit(n - d * (n @ d)); return np.stack([d, n, np.cross(d, n)], 1)
def swing(a, b):                    # minimal rotation taking direction a to b
    a, b = unit(a), unit(b); v = np.cross(a, b); c = a @ b
    if np.linalg.norm(v) < 1e-9: return np.eye(3)
    K = np.array([[0, -v[2], v[1]], [v[2], 0, -v[0]], [-v[1], v[0], 0]]); return np.eye(3) + K + K @ K / (1 + c)
def axis_rot(axis, ang):
    a = unit(axis); K = np.array([[0, -a[2], a[1]], [a[2], 0, -a[0]], [-a[1], a[0], 0]])
    return np.eye(3) + np.sin(ang) * K + (1 - np.cos(ang)) * K @ K

# ---- A. global similarity: scale + translation (no rotation, x fixed) fitted on torso, head and legs
core = [B[n] for n in ('pelvis', 'spine_01', 'spine_02', 'spine_03', 'neck_01', 'head', 'thigh_l', 'thigh_r', 'calf_l', 'calf_r')]
core_v = np.where(mh.body & (W[:, core].sum(1) > 0.9))[0]
s = 1.81 / mh.V[mh.body, 2].max(); t = np.zeros(3)
Qt = quads_to_tris(mh.Qbody)
for it in range(30):
    P = mh.V[core_v] * s + t
    _, j = tree.query(P); C = TV[j]
    N = vertex_normals(mh.V * s + t, Qt)[core_v]
    ok = ((TN[j] * N).sum(1) > 0.5) & (np.linalg.norm(C - P, axis=1) < 0.06)
    X = mh.V[core_v][ok]; Y = C[ok]; xm, ym = X.mean(0), Y.mean(0)
    s = ((X - xm) * (Y - ym)).sum() / ((X - xm) ** 2).sum(); t = ym - s * xm; t[0] = 0
print('global scale %.4f translation %s  (body height %.3f m)' % (s, t.round(4), mh.V[mh.body, 2].max() * s + t[2]))
G = lambda p: np.asarray(p) * s + t
V0 = G(mh.V); H0 = {n: G(mh.head[n]) for n in bn}; T0 = {n: G(mh.tail[n]) for n in bn}

# ---- B. limb joint centres measured on the sculpt: centroid of a thin cross-section
def section_centre(J, d, side, radius=0.11, half=0.012):
    d = unit(d); idx = np.array(tree.query_ball_point(J, radius))
    P = TV[idx]; P = P[(np.abs((P - J) @ d) < half) & (P[:, 0] * side > 0.02)]
    return P.mean(0) if len(P) > 20 else J
def src_section(J, d, vmask, half=0.012):
    d = unit(d); P = V0[vmask]; P = P[np.abs((P - J) @ d) < half]
    return P.mean(0) if len(P) > 8 else J

T = {n: (np.eye(3) * 1.0, np.zeros(3)) for n in bn}        # per-bone affine: p -> A p + b (on V0)
def setT(name, A, src_pivot, dst_pivot): T[name] = (A, dst_pivot - A @ src_pivot)
def descend(name, A, src_pivot, dst_pivot):
    setT(name, A, src_pivot, dst_pivot)
    for c in bn:
        if mh.parent[c] == name: descend(c, A, src_pivot, dst_pivot)

report = {}
for side, sx in (('l', 1), ('r', -1)):
    mir = np.array([sx, 1, 1.0])
    # arm: shoulder stays where the fitted torso puts it; elbow and wrist come from the sculpt
    S = H0['upperarm_' + side]; E0 = H0['lowerarm_' + side]; W0 = H0['hand_' + side]; K0 = H0['middle_01_' + side]
    arm_mask = mh.body & (W[:, [B[b] for b in bn if b.endswith('_' + side) and not b.startswith(('thigh', 'calf', 'foot', 'ball', 'clavicle'))]].sum(1) > 0.5)
    E0c = src_section(E0, E0 - S, arm_mask); W0c = src_section(W0, W0 - E0, arm_mask)
    Et = mj('lowerarm_l') * mir; Wt = mj('hand_l') * mir; St = mj('upperarm_l') * mir
    for _ in range(3):
        Et = section_centre(Et, Et - St, sx); Wt = section_centre(Wt, Wt - Et, sx)
    ns = np.cross(E0c - S, W0c - E0c); nt = np.cross(Et - S, Wt - Et)
    Ru = frame(Et - S, nt) @ frame(E0c - S, ns).T
    su = np.linalg.norm(Et - S) / np.linalg.norm(E0c - S); du = unit(E0c - S)
    Au = Ru @ (np.eye(3) + (su - 1) * np.outer(du, du))
    setT('upperarm_' + side, Au, S, S)
    Rl = frame(Wt - Et, nt) @ frame(W0c - E0c, ns).T
    sl = np.linalg.norm(Wt - Et) / np.linalg.norm(W0c - E0c); dl = unit(W0c - E0c)
    Al = Rl @ (np.eye(3) + (sl - 1) * np.outer(dl, dl))
    setT('lowerarm_' + side, Al, E0c, Et)
    # hand: follows the forearm frame for now (twist and aim are refined in step C)
    descend('hand_' + side, Rl, W0c, Wt)
    # leg: hip stays; knee and ankle from the sculpt; swing only
    Hh = H0['thigh_' + side]; Kn0 = H0['calf_' + side]; An0 = H0['foot_' + side]; Ba0 = H0['ball_' + side]
    leg_mask = mh.body & (W[:, [B[b + '_' + side] for b in ('thigh', 'calf', 'foot', 'ball')]].sum(1) > 0.5)
    Kn0c = src_section(Kn0, Kn0 - Hh, leg_mask); An0c = src_section(An0, An0 - Kn0, leg_mask)
    Knt = mj('calf_l') * mir; Ant = mj('foot_l') * mir
    for _ in range(3):
        Knt = section_centre(Knt, Knt - Hh, sx, radius=0.13); Ant = section_centre(Ant, Ant - Knt, sx, radius=0.09)
    Rt_ = swing(Kn0c - Hh, Knt - Hh); st = np.linalg.norm(Knt - Hh) / np.linalg.norm(Kn0c - Hh); dt = unit(Kn0c - Hh)
    setT('thigh_' + side, Rt_ @ (np.eye(3) + (st - 1) * np.outer(dt, dt)), Hh, Hh)
    Rc = swing(Rt_ @ (An0c - Kn0c), Ant - Knt) @ Rt_; sc = np.linalg.norm(Ant - Knt) / np.linalg.norm(An0c - Kn0c); dc = unit(An0c - Kn0c)
    setT('calf_' + side, Rc @ (np.eye(3) + (sc - 1) * np.outer(dc, dc)), Kn0c, Knt)
    descend('foot_' + side, np.eye(3), An0c, Ant)          # feet stay flat and forward
    if side == 'l':
        WRIST_T, ELBOW_T = Wt.copy(), Et.copy()
        report.update(elbow=Et.round(4).tolist(), wrist=Wt.round(4).tolist(), knee=Knt.round(4).tolist(), ankle=Ant.round(4).tolist(),
                      upperarm_scale=round(su, 3), forearm_scale=round(sl, 3), thigh_scale=round(st, 3), calf_scale=round(sc, 3))

# ---- C. hand: anatomical frame on the scaffold (wrist, knuckles) against the sculpt hand's principal axes
def hand_frame(main, normal):       # columns: main (wrist->fingers), radial (towards thumb), palm normal
    m = unit(main); n = unit(normal - m * (normal @ m)); return np.stack([m, np.cross(n, m), n], 1)
Wr0 = H0['hand_l']; m_s = H0['middle_01_l'] - Wr0; r_s = H0['index_01_l'] - H0['pinky_01_l']
n_s = np.cross(m_s, r_s); n_s = n_s if n_s[2] < 0 else -n_s                 # scaffold rest pose: palms face down
Fs = hand_frame(m_s, n_s); assert Fs[:, 1] @ unit(r_s) > 0.8, 'scaffold hand frame handedness'
Wt = WRIST_T; fore = unit(WRIST_T - ELBOW_T)
tp = TV[(TV[:, 0] > 0.2) & ((TV - Wt) @ fore > 0.012) & (np.linalg.norm(TV - Wt, axis=1) < 0.27)]
c = tp.mean(0); _, sv, vt = np.linalg.svd(tp - c, full_matrices=False)
m_t = unit(c - Wt); n_t = vt[2] if vt[2][0] < 0 else -vt[2]                 # sculpt: palms face the body
Ft = hand_frame(m_t, n_t)
print('sculpt hand: main %s palm normal %s thumb side %s (expect forward, -Y)' % (m_t.round(2), n_t.round(2), Ft[:, 1].round(2)))
Rh = Ft @ Fs.T
hand_bones = [b for b in bn if b.endswith('_l') and b.split('_')[0] in ('hand', 'index', 'middle', 'ring', 'pinky', 'thumb')]
hv = np.where(mh.body & (W[:, [B[b] for b in hand_bones]].sum(1) > 0.6))[0]
len_s = ((V0[hv] - Wr0) @ Fs[:, 0]).max(); len_t = ((tp - Wt) @ Ft[:, 0]).max(); hs = len_t / len_s
print('hand length: sculpt %.1f mm, scaffold %.1f mm, scale %.3f' % (len_t * 1e3, len_s * 1e3, hs))
# forearm twist: the part of the hand rotation about the forearm axis is spread from elbow to wrist
Al_, bl_ = T['lowerarm_l']; Rl_ = Rh @ np.linalg.inv(frame(WRIST_T - ELBOW_T, np.cross(ELBOW_T - H0['upperarm_l'], WRIST_T - ELBOW_T)) @ frame(H0['hand_l'] - H0['lowerarm_l'], np.cross(H0['lowerarm_l'] - H0['upperarm_l'], H0['hand_l'] - H0['lowerarm_l'])).T)
from scipy.spatial.transform import Rotation
q = Rotation.from_matrix(Rl_).as_quat(); TWIST = 2 * np.arctan2(q[:3] @ fore, q[3]); TWIST = (TWIST + np.pi) % (2 * np.pi) - np.pi
print('forearm twist to distribute: %.1f deg' % np.degrees(TWIST))
for side, sx in (('l', 1), ('r', -1)):
    Mx = np.diag([sx, 1, 1.0]); A2 = Mx @ (Rh * hs) @ Mx
    for b in [h[:-2] + '_' + side for h in hand_bones]:
        T[b] = (A2, WRIST_T * np.array([sx, 1, 1.0]) - A2 @ (Wr0 * np.array([sx, 1, 1.0])))

# ---- C2. fingers: find each finger of the sculpt hand (they separate into connected pieces beyond the
# webbing) and aim the scaffold's finger chains at them, keeping the scaffold's finger lengths.
import scipy.sparse as sp
from scipy.sparse.csgraph import connected_components
hand_idx = np.where((TV[:, 0] > 0.2) & ((TV - Wt) @ fore > 0.0) & (np.linalg.norm(TV - Wt, axis=1) < 0.27))[0]
hloc = -np.ones(len(TV), int); hloc[hand_idx] = np.arange(len(hand_idx)); HP = TV[hand_idx]
hf = TF[(hloc[TF] >= 0).all(1)]; he = hloc[np.concatenate([hf[:, [0, 1]], hf[:, [1, 2]], hf[:, [2, 0]]])]
S_ = (HP - Wt) @ Ft[:, 0]; R_ = (HP - Wt) @ Ft[:, 1]
def pieces(mask, min_n=60):
    idx = np.where(mask)[0]; l = -np.ones(len(HP), int); l[idx] = np.arange(len(idx)); e = l[he]; e = e[(e >= 0).all(1)]
    nc, lab = connected_components(sp.coo_matrix((np.ones(len(e)), (e[:, 0], e[:, 1])), shape=(len(idx), len(idx))), directed=False)
    return [idx[lab == c] for c in range(nc) if (lab == c).sum() >= min_n]
# thumb first: scanning from the fingers towards the wrist, it is the piece that stays separate from the palm
thumb = None
for c_ in np.arange(0.16, 0.02, -0.004):
    ps = pieces(S_ > c_, 40); big = max(ps, key=len) if ps else []
    side = [p for p in ps if p is not big and S_[p].max() < S_.max() - 0.03]            # ends well short of the fingertips
    if side: thumb = max(side, key=len)
    elif thumb is not None and len(ps) == 1: break
assert thumb is not None, 'could not separate the thumb on the sculpt hand'
not_thumb = np.ones(len(HP), bool); not_thumb[thumb] = False
for s_cut in np.arange(0.105, 0.175, 0.004):
    fingers = pieces((S_ > s_cut) & not_thumb)
    if len(fingers) >= 3: break
assert len(fingers) in (3, 4), 'could not separate the fingers on the sculpt hand (%d pieces)' % len(fingers)
while len(fingers) < 4:                                         # two fingers that touch: split the widest piece across the hand
    big = max(range(len(fingers)), key=lambda i: len(fingers[i])); c = fingers.pop(big); r = R_[c]; lo_, hi_ = r.min(), r.max()
    for _ in range(20):
        mid = (lo_ + hi_) / 2; lo_, hi_ = r[r < mid].mean(), r[r >= mid].mean()
    fingers += [c[r < (lo_ + hi_) / 2], c[r >= (lo_ + hi_) / 2]]
fingers.sort(key=lambda c: -R_[c].mean())                       # most radial (thumb side) first: index, middle, ring, pinky
def digit(c):
    P = HP[c]; cen = P.mean(0); _, _, vt = np.linalg.svd(P - cen, full_matrices=False); d = vt[0] if vt[0] @ Ft[:, 0] > 0 else -vt[0]
    t = (P - cen) @ d; tipc = P[t > t.max() - 0.004].mean(0); return d, tipc + d * 0.001
tgt_digits = dict(zip(('index', 'middle', 'ring', 'pinky'), map(digit, fingers))); tgt_digits['thumb'] = digit(thumb)
print('sculpt fingers found at cut %.0f mm; pieces %s, thumb %d verts' % (s_cut * 1e3, [len(c) for c in fingers], len(thumb)))
A_h, b_h = T['hand_l']
for name, (d_t, tip_t) in tgt_digits.items():
    B0 = A_h @ H0[name + '_01_l'] + b_h; T0_ = A_h @ T0[name + '_03_l'] + b_h; L = np.linalg.norm(T0_ - B0)
    Rf = swing(T0_ - B0, d_t); Bn = tip_t - d_t * L                                     # new knuckle position
    print('  %-6s tip moves %.1f mm, knuckle %.1f mm, turn %.1f deg' % (name, np.linalg.norm(tip_t - T0_) * 1e3, np.linalg.norm(Bn - B0) * 1e3, np.degrees(np.arccos(np.clip(unit(T0_ - B0) @ d_t, -1, 1)))))
    for side, sx in (('l', 1), ('r', -1)):
        Mx = np.diag([sx, 1, 1.0]); A2 = Mx @ (Rf @ A_h) @ Mx; t2 = Mx @ (Rf @ (b_h - B0) + Bn)
        for k in ('_01_', '_02_', '_03_'): T[name + k + side] = (A2, t2)

# ---- D. feet: uniform scale to the sculpt's foot length, sole on the ground, heel and centre matched
fv = np.where(mh.body & (W[:, [B['foot_l'], B['ball_l']]].sum(1) > 0.6))[0]
tf = TV[(TV[:, 0] > 0.02) & (TV[:, 2] < 0.10)]
fs = (tf[:, 1].max() - tf[:, 1].min()) / (V0[fv, 1].max() - V0[fv, 1].min())
src_anchor = np.array([V0[fv, 0].mean(), V0[fv, 1].max(), V0[fv, 2].min()]); dst_anchor = np.array([tf[:, 0].mean(), tf[:, 1].max(), 0.0])
print('foot scale %.3f  sculpt foot length %.1f mm' % (fs, (tf[:, 1].max() - tf[:, 1].min()) * 1e3))
for side, sx in (('l', 1), ('r', -1)):
    mir = np.array([sx, 1, 1.0])
    for b in ('foot_' + side, 'ball_' + side): T[b] = (np.eye(3) * fs, dst_anchor * mir - fs * src_anchor * mir)

# ---- E. head: scale + translation ICP on the skull and face (no rotation, centred)
hd = np.where(mh.body & (W[:, B['head']] > 0.8))[0]; sh, th = 1.0, np.zeros(3)
for it in range(60):
    P = V0[hd] * sh + th; _, j = tree.query(P); C = TV[j]
    Nn = vertex_normals(V0 * sh + th, Qt)[hd]
    dist = np.linalg.norm(C - P, axis=1); ok = ((TN[j] * Nn).sum(1) > 0.6) & (dist < max(0.012, np.percentile(dist, 80)))
    X = V0[hd][ok]; Y = C[ok]; xm, ym = X.mean(0), Y.mean(0)
    sh = ((X - xm) * (Y - ym)).sum() / ((X - xm) ** 2).sum(); th = ym - sh * xm; th[0] = 0
print('head: scale %.3f translation %s  residual median %.2f mm' % (sh, th.round(4), np.median(dist[ok]) * 1e3))
T['head'] = (np.eye(3) * sh, th)

def lbs(V):
    out = np.zeros_like(V)
    for n in bn:
        A, b = T[n]; P = V @ A.T + b
        if n.startswith('lowerarm_'):                      # progressive pronation along the forearm
            sx = 1 if n.endswith('_l') else -1; mir = np.array([sx, 1, 1.0])
            E, ax = ELBOW_T * mir, fore * mir; L = np.linalg.norm(WRIST_T - ELBOW_T)
            tt = np.clip((P - E) @ ax / L, 0, 1) * TWIST * sx
            rel = P - E; par = np.outer(rel @ ax, ax); perp = rel - par
            P = E + par + perp * np.cos(tt)[:, None] + np.cross(ax, perp) * np.sin(tt)[:, None]
        out += W[:, [B[n]]] * P
    return out
V2 = lbs(V0)
# keep the result exactly symmetric
V2 = (V2 + V2[mh.mirror] * np.array([-1, 1, 1.0])) / 2
d2, _, _ = igl.point_mesh_squared_distance(V2[mh.body], TV, TF); d = np.sqrt(d2) * 1e3
print('body->sculpt distance mm: median %.1f  p90 %.1f  max %.1f' % (np.median(d), np.percentile(d, 90), d.max()))
print(json.dumps(report))
np.savez_compressed(KIT + 'work/pose.npz', V=V2, Q=mh.Qbody)
