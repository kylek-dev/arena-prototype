"""Build the kit GLB for the game from the fitted body, its textures and the kit skeleton, plus the kit's
manifest entry.   python build_kit.py [OUT_DIR]   ->  OUT_DIR/meshy-human-male.glb, OUT_DIR/kit.json"""
import sys, os, json, collections
import numpy as np
from scipy.spatial import cKDTree
from glbwriter import GLB, C_WG, to_gltf, quat, quat_to_mat
from kitlib import *
from mh import MH
import skeleton, layout

KIT_ID = 'meshy-human-male'
out_dir = sys.argv[1] if len(sys.argv) > 1 else KIT + 'out'; os.makedirs(out_dir, exist_ok=True)
mh = MH(); body = np.where(mh.body)[0]
L = layout.compute(mh); V, Q, uvq, region, RN = L['V'], L['Q'], L['uvq'], L['region'], L['names']
ep = np.load(KIT + 'work/eye_parts.npz')
sk = skeleton.build(mh, np.load(KIT + 'work/pose.npz')['V']); names = sk['names']; J = {n: i for i, n in enumerate(names)}
TEX = KIT + 'tex/'

# ---------------------------------------------------------------- skin weights (scaffold's game rig, by bone name)
Wm = mh.weights()[body]; W65 = np.zeros((len(V), len(names)))
for k, b in enumerate(mh.bone_names): W65[:, J[{'Root': 'root', 'head': 'Head'}.get(b, b)]] += Wm[:, k]
# The trunk joints sit where the clips expect them (skeleton.py), not where the scaffold's weights assume,
# so the pelvis-and-spine share of every vertex is redistributed by height. The hand-over at each joint
# runs from the joint below to the joint above (about 24 cm): Attack bends one spine joint by 32 degrees,
# and with a short hand-over the chest folded into a hard crease over the abdomen.
chain = ['pelvis', 'spine_01', 'spine_02', 'spine_03', 'neck_01']; cj = [J[n] for n in chain[:4]]
mass = W65[:, cj].sum(1); zj = [sk['pos'][n][2] for n in chain]
def ramp(lo, hi):
    t = np.clip((V[:, 2] - lo) / (hi - lo), 0, 1); return (t + t * t * (3 - 2 * t)) / 2      # nearly even bending, soft ends
above = [ramp(zj[k - 1], zj[k + 1]) for k in (1, 2, 3)]        # how far a vertex is past spine_01, spine_02, spine_03
share = [1 - above[0], above[0] * (1 - above[1]), above[0] * above[1] * (1 - above[2]), above[0] * above[1] * above[2]]
for j, sh in zip(cj, share): W65[:, j] = mass * sh
idx = np.argsort(-W65, axis=1)[:, :4]; bw = np.take_along_axis(W65, idx, 1); bw[bw < 1e-4] = 0; bw /= bw.sum(1, keepdims=True); idx[bw == 0] = 0
bj, bw = idx.astype(np.uint16), bw.astype(np.float32)
rigid = lambda n, joint: (np.tile(np.array([J[joint], 0, 0, 0], np.uint16), (n, 1)), np.tile(np.array([1, 0, 0, 0], np.float32), (n, 1)))

# ---------------------------------------------------------------- body normals (smooth across region and UV seams)
T_all = np.concatenate([Q[:, [0, 1, 2]], Q[:, [0, 2, 3]]])
fn = np.cross(V[T_all[:, 1]] - V[T_all[:, 0]], V[T_all[:, 2]] - V[T_all[:, 0]]); N = np.zeros_like(V)
for k in range(3): np.add.at(N, T_all[:, k], fn)
N /= np.maximum(np.linalg.norm(N, axis=1, keepdims=True), 1e-20)

glb = GLB('pitborn human-male-kit build_kit.py')
tx = lambda f, n: glb.texture(TEX + f, n)
m_body = glb.material('Skin', tx('skin_body.png', 'Skin_BaseColor'), tx('skin_body_n.png', 'Skin_Normal'))
m_head = glb.material('Skin_Head', tx('skin_head.png', 'Skin_Head_BaseColor'), tx('skin_head_n.png', 'Skin_Head_Normal'))
m_trunks = glb.material('Trunks', tx('trunks.png', 'Trunks_BaseColor'), tx('trunks_n.png', 'Trunks_Normal'))
m_white = glb.material('Eye_White', tx('eye_white.png', 'Eye_White_BaseColor')); m_iris = glb.material('Eye_Iris', tx('eye_iris.png', 'Eye_Iris_BaseColor'))

fm = np.load(KIT + 'work/face_morphs.npz'); FACE_NAMES = [str(x) for x in fm['names']]; FACE_D = fm['deltas']

def compact(faces_mask, uv_fn, material, P=V, Nrm=N, morph=False):
    """One primitive from a set of body quads: only the vertices it uses, split at UV seams."""
    f = np.where(faces_mask)[0]; cv = Q[f].reshape(-1); cu = uv_fn(uvq[f].reshape(-1, 2))
    key = np.concatenate([cv[:, None], np.round(cu * 1e5).astype(np.int64)], 1)
    _, first, inv = np.unique(key, axis=0, return_index=True, return_inverse=True); inv = inv.reshape(-1, 4)
    sv = cv[first]; uv = cu[first]; uv = np.stack([uv[:, 0], 1 - uv[:, 1]], 1)
    tris = np.concatenate([inv[:, [0, 1, 2]], inv[:, [0, 2, 3]]])
    return glb.primitive(P[sv], Nrm[sv], uv, bj[sv], bw[sv], tris, material, targets=[d[sv] for d in FACE_D] if morph else None), len(tris)

meshes = []; tri_count = collections.OrderedDict()
ident = lambda uv: uv; head_uv = lambda uv: layout.remap(uv, L['crop_head']); trunks_uv = lambda uv: layout.remap(uv, L['crop_trunks'])
for ri, r in enumerate(RN):
    if r in ('Hips', 'Thighs'): continue                      # permanently under the trunks piece; not shipped
    prims = []; n = 0
    for mask, fn_, mat in ((L['use_head_tex'], head_uv, m_head), (~L['use_head_tex'], ident, m_body)):
        m = (region == ri) & mask
        if m.any(): p, k = compact(m, fn_, mat, morph=(r == 'Head')); prims.append(p); n += k
    if r == 'Head':
        # a dark ribbon just behind the closed lips, so no pixel between the two lip edges can show through
        fe = L['fit']; lo = fe['mouth_lower']; back = np.array([0, 0.002, 0.0]); h = np.array([0, 0, 0.0015])
        P = np.concatenate([V[lo] + back + h, V[lo] + back - h]); nl = len(lo)
        tri = np.array([t for i in range(nl - 1) for t in ((i, i + 1, nl + i + 1), (i, nl + i + 1, nl + i))])
        fnr = np.cross(P[tri[0, 1]] - P[tri[0, 0]], P[tri[0, 2]] - P[tri[0, 0]])
        if fnr[1] > 0: tri = tri[:, ::-1]
        f0 = np.where((region == ri) & L['use_head_tex'] & (Q == lo[nl // 2]).any(1))[0][0]; c0 = list(Q[f0]).index(lo[nl // 2])
        uv0 = head_uv(uvq[f0][c0][None])[0]; uvs = np.tile([uv0[0], 1 - uv0[1]], (2 * nl, 1))
        idx2 = np.concatenate([lo, lo])
        prims.append(glb.primitive(P, np.tile([0, -1.0, 0], (2 * nl, 1)), uvs, bj[idx2], bw[idx2], tri, m_head, targets=[d[idx2] for d in FACE_D])); n += len(tri)
    meshes.append(('Body_' + r, prims)); tri_count['Body_' + r] = n

# ---------------------------------------------------------------- trunks: the covered skin pushed out 2.5 mm, edges turned in to the skin
tm = L['trunks']; tf = np.where(tm)[0]; tv = np.unique(Q[tf]); Vt = V.copy(); Vt[tv] += N[tv] * 0.0025
prim_t, n_t = compact(tm, trunks_uv, m_trunks, P=Vt)
edge_use = collections.Counter()
for f in Q[tf]:
    for k in range(4): edge_use[(f[k], f[(k + 1) % 4])] += 1
rim = [(a, b) for (a, b), c in edge_use.items() if (b, a) not in edge_use]         # open edges, in face winding order
uv_of = {}
for f, uvs in zip(Q[tf], uvq[tf]):
    for k in range(4): uv_of[(f[k], f[(k + 1) % 4])] = (uvs[k], uvs[(k + 1) % 4])
rp, rn, ruv, rj, rw, rt = [], [], [], [], [], []
for a, b in rim:
    ua, ub = uv_of[(a, b)]; b0 = len(rp); nrm = np.cross(Vt[a] - Vt[b], V[b] - Vt[b]); nrm /= np.linalg.norm(nrm)
    for v, u, p in ((b, ub, Vt[b]), (a, ua, Vt[a]), (a, ua, V[a]), (b, ub, V[b])):
        rp.append(p); rn.append(nrm); u2 = trunks_uv(np.array([u]))[0]; ruv.append((u2[0], 1 - u2[1])); rj.append(bj[v]); rw.append(bw[v])
    rt += [(b0, b0 + 1, b0 + 2), (b0, b0 + 2, b0 + 3)]
prim_r = glb.primitive(np.array(rp), np.array(rn), np.array(ruv), np.array(rj), np.array(rw), np.array(rt), m_trunks)
meshes.append(('Trunks', [prim_t, prim_r])); tri_count['Trunks'] = n_t + len(rt)

# ---------------------------------------------------------------- eyebrows: the forehead's own quads 0.7 mm above the skin, both sides
bd = np.load(KIT + 'work/brows.npz'); bq = bd['quads']; X0, X1, Z0, Z1 = bd['window']
loc_ = -np.ones(len(mh.V), int); loc_[body] = np.arange(len(body)); mir_v = loc_[mh.mirror[body]]
ids = np.unique(bq); lid = -np.ones(len(V), int); lid[ids] = np.arange(len(ids)); nb = len(ids)
all_ids = np.concatenate([ids, mir_v[ids]])
bP = V[all_ids] + N[all_ids] * 0.0007
side_x = np.concatenate([V[ids][:, 0], -V[mir_v[ids]][:, 0]])                      # mirror the right brow into the left brow's texture
bUV = np.stack([(side_x - X0) / (X1 - X0), 1 - (V[all_ids][:, 2] - Z0) / (Z1 - Z0)], 1)
qa = lid[bq]; bT = np.concatenate([qa[:, [0, 1, 2]], qa[:, [0, 2, 3]], (qa + nb)[:, [0, 2, 1]], (qa + nb)[:, [0, 3, 2]]])
BROWS = [str(x) for x in bd['names']]
for shape in BROWS:
    mat = glb.material('Brows_' + shape, tx(f'brows_{shape.lower()}.png', f'Brows_{shape}_BaseColor'), alpha='BLEND')
    meshes.append(('Brows_' + shape, [glb.primitive(bP, N[all_ids], bUV, bj[all_ids], bw[all_ids], bT, mat, targets=[d[all_ids] for d in FACE_D])]))
    tri_count['Brows_' + shape] = len(bT)

# ---------------------------------------------------------------- hair and beards: generated pieces, rigid to the head
# Each is cut from a generated bust and reduced to a game mesh by hair_fit.py, hair_cut.py, hair_low.py and
# hair_tex.py (work/<stem>.npz, tex/<stem>.png, tex/<stem>_n.png). They follow the face presets: each vertex
# takes the targets of the nearest head vertex. Near the head's skin a vertex also takes that skin's weights, so
# the nape and the jaw, which partly follow the neck, do not slide out from under the hair when the head turns;
# from SKIN_NEAR to SKIN_FAR out it blends to rigid on Head. Only the head's skin counts: hair hanging by the
# neck (a ponytail) stays rigid on Head, or it would bend with the neck into the back.
HAIR = [('SweptBack', 'Swept back', 'hair_swept'), ('CloseCrop', 'Close crop', 'hair_crop'),   # (node suffix, label, file stem)
        ('TiedBack', 'Tied back', 'hair_tied')]
BEARDS = [('Full', 'Full beard', 'beard_full'), ('Handlebar', 'Handlebar', 'beard_goatee')]
head_v = np.unique(Q[region == RN.index('Head')]); head_tree = cKDTree(V[head_v])
skin_v = head_v; skin_tree = head_tree
SKIN_NEAR, SKIN_FAR = 0.008, 0.020
def follow_skin(P):
    dist, k = skin_tree.query(P); near = skin_v[k]; t = np.clip((SKIN_FAR - dist) / (SKIN_FAR - SKIN_NEAR), 0, 1)
    Wd = np.zeros((len(P), len(names))); np.add.at(Wd, (np.arange(len(P))[:, None].repeat(4, 1), bj[near].astype(int)), bw[near])
    Wd = Wd * t[:, None]; Wd[:, J['Head']] += 1 - t
    i4 = np.argsort(-Wd, axis=1)[:, :4]; w4 = np.take_along_axis(Wd, i4, 1); w4[w4 < 1e-4] = 0; w4 /= w4.sum(1, keepdims=True); i4[w4 == 0] = 0
    return i4.astype(np.uint16), w4.astype(np.float32)
def generated_piece(node, stem, tint_material):
    d = np.load(KIT + f'work/{stem}.npz'); PV, PF, PUV = d['V'], d['F'], d['UV']                # UVs per corner, rows down
    fnn = np.cross(PV[PF[:, 1]] - PV[PF[:, 0]], PV[PF[:, 2]] - PV[PF[:, 0]]); Np = np.zeros_like(PV)
    for k in range(3): np.add.at(Np, PF[:, k], fnn)
    Np /= np.maximum(np.linalg.norm(Np, axis=1, keepdims=True), 1e-20)                     # smooth across UV seams
    cv = PF.reshape(-1); cu = PUV.reshape(-1, 2)
    key = np.concatenate([cv[:, None], np.round(cu * 1e5).astype(np.int64)], 1)
    _, first, inv = np.unique(key, axis=0, return_index=True, return_inverse=True); sv = cv[first]
    near = head_v[head_tree.query(PV[sv])[1]]
    mat = glb.material(tint_material, tx(f'{stem}.png', f'{tint_material}_BaseColor'), tx(f'{stem}_n.png', f'{tint_material}_Normal'))
    meshes.append((node, [glb.primitive(PV[sv], Np[sv], cu[first], *follow_skin(PV[sv]), inv.reshape(-1, 3), mat, targets=[dd[near] for dd in FACE_D])]))
    tri_count[node] = len(PF)
for style, _, stem in HAIR: generated_piece('Hair_' + style, stem, 'Hair_' + style)
for style, _, stem in BEARDS: generated_piece('Beard_' + style, stem, 'Beard_' + style)
# stubble: the face's own quads 0.7 mm above the skin, alpha-blended, shaped like the full beard (stubble.py)
st = np.load(KIT + 'work/stubble.npz'); s_ids = st['ids']; s_loc = -np.ones(len(V), int); s_loc[s_ids] = np.arange(len(s_ids))
sq = s_loc[st['quads']]; sT = np.concatenate([sq[:, [0, 1, 2]], sq[:, [0, 2, 3]]])
m_stubble = glb.material('Beard_Stubble', tx('stubble.png', 'Beard_Stubble_BaseColor'), alpha='BLEND')
meshes.append(('Beard_Stubble', [glb.primitive(V[s_ids] + N[s_ids] * 0.0007, N[s_ids], st['uv'], bj[s_ids], bw[s_ids], sT, m_stubble,
                                               targets=[dd[s_ids] for dd in FACE_D])]))
tri_count['Beard_Stubble'] = len(sT)
BEARDS.append(('Stubble', 'Stubble', None))

# ---------------------------------------------------------------- eyes (rigid to the head): eye white, iris, catchlights
def tri_mesh(P, faces, facing=np.array([0, -1.0, 0])):
    """Triangles and normals of a small piece; the winding is set so it faces `facing` (the front)."""
    tri = []
    for f in faces: tri += [(f[0], f[i], f[i + 1]) for i in range(1, len(f) - 1)]
    tri = np.array(tri); f_n = np.cross(P[tri[:, 1]] - P[tri[:, 0]], P[tri[:, 2]] - P[tri[:, 0]])
    if f_n.sum(0) @ facing < 0: tri = tri[:, ::-1]; f_n = -f_n
    n = np.zeros_like(P)
    for k in range(3): np.add.at(n, tri[:, k], f_n)
    return tri, n / np.maximum(np.linalg.norm(n, axis=1, keepdims=True), 1e-20)
eye_prims = []; n_eye = 0
for sx in (1, -1):
    S = np.array([sx, 1, 1.0]); flip = (lambda f: f[::-1]) if sx < 0 else (lambda f: f)
    P = ep['cap_V'] * S; uv = ep['cap_UV'].copy(); uv[:, 0] *= 0.75; uv[:, 1] = 1 - uv[:, 1]
    tri, n = tri_mesh(P, [flip(list(f)) for f in ep['cap_T']] + [flip(list(f)) for f in ep['cap_Q']])
    eye_prims.append(glb.primitive(P, n, uv, *rigid(len(P), 'Head'), tri, m_white)); n_eye += len(tri)
    P = ep['iris_V'] * S; uv = ep['iris_UV'].copy(); uv[:, 1] = 1 - uv[:, 1]
    if sx < 0: uv[:, 0] = 1 - uv[:, 0]
    tri, n = tri_mesh(P, [flip(list(f)) for f in ep['iris_tris']] + [flip(list(f)) for f in ep['iris_quads']])
    eye_prims.append(glb.primitive(P, n, uv, *rigid(len(P), 'Head'), tri, m_iris)); n_eye += len(tri)
    c = ep['iris_centre'] * np.array([sx, 1.0]); r = float(ep['iris_radius']); y0 = ep['iris_V'][:, 1].min() - 0.0002; gv, gf = [], []
    for ox, oz, rr in ((-0.22, 0.30, 0.17), (0.25, 0.22, 0.11)):                   # catchlights, on the same side in both eyes
        b0 = len(gv); gv.append((c[0] + ox * r, y0, c[1] + oz * r))
        gv += [(c[0] + ox * r + rr * r * np.cos(a) * 0.8, y0, c[1] + oz * r + rr * r * np.sin(a)) for a in 2 * np.pi * np.arange(10) / 10]
        gf += [(b0, b0 + 1 + (s + 1) % 10, b0 + 1 + s) for s in range(10)]
    gv = np.array(gv); tri, n = tri_mesh(gv, gf)
    eye_prims.append(glb.primitive(gv, n, np.tile([0.875, 0.5], (len(gv), 1)), *rigid(len(gv), 'Head'), tri, m_white)); n_eye += len(tri)
meshes.append(('Eyes', eye_prims)); tri_count['Eyes'] = n_eye

# ---------------------------------------------------------------- skeleton: the kit's joints in the kit's order; rest pose = bind pose (A-pose)
Wg = {}
for n in names:
    M = np.eye(4); M[:3, :3] = C_WG @ sk['rot'][n]; M[:3, 3] = C_WG @ sk['pos'][n]; Wg[n] = M
joint_node = {}
for n in names:
    p = sk['parent'][n]; Lm = Wg[n] if p is None else np.linalg.inv(Wg[p]) @ Wg[n]
    joint_node[n] = glb.node(name=n, translation=Lm[:3, 3].round(7).tolist(), rotation=quat(Lm[:3, :3]).round(7).tolist())
children = collections.defaultdict(list)
for n in names:
    if sk['parent'][n]: children[sk['parent'][n]].append(joint_node[n])

# ---------------------------------------------------------------- attachment sockets: plain named nodes under a joint (not skin joints)
P_ = sk['pos']; unit = lambda v: v / np.linalg.norm(v); up = np.array([0, 0, 1.0])
surf = lambda sel: V[np.unique(Q)][sel(V[np.unique(Q)])]
world_axes = np.eye(3)                                         # work-space orientation whose glTF form is: +X left, +Y up, +Z front
def surface_point(direction, near, radius):                    # farthest body point along `direction` within `radius` of `near`
    Pb = V[np.unique(Q)]; Pb = Pb[np.linalg.norm(Pb - near, axis=1) < radius]; return Pb[np.argmax(Pb @ direction)]
sockets = collections.OrderedDict()
sockets['socket_head'] = ('Head', np.array([0, P_['Head'][1], V[:, 2].max()]), world_axes)
back = surface_point(np.array([0, 1.0, 0]), np.array([0, 0.1, 1.38]), 0.12); chest = surface_point(np.array([0, -1.0, 0]), np.array([0, -0.1, 1.36]), 0.12)
sockets['socket_back'] = ('spine_03', np.array([0, back[1], 1.38]), world_axes); sockets['socket_chest'] = ('spine_03', np.array([0, chest[1], 1.36]), world_axes)
belt = surface_point(np.array([0, -1.0, 0]), np.array([0, -0.1, 1.09]), 0.12); sockets['socket_belt'] = ('pelvis', np.array([0, belt[1], 1.09]), world_axes)
for side, sx in (('l', 1), ('r', -1)):
    S = np.array([sx, 1, 1.0]); j = lambda n: P_[n + '_' + side]
    top = surface_point(up, j('upperarm') + np.array([0, 0, 0.05]), 0.09); sockets['socket_shoulder_' + side] = ('clavicle_' + side, np.array([j('upperarm')[0], j('upperarm')[1], top[2]]), world_axes)
    hip = surface_point(S * np.array([1.0, 0, 0]), np.array([sx * 0.15, 0.03, 1.02]), 0.12); sockets['socket_hip_' + side] = ('pelvis', np.array([hip[0], 0.03, 1.02]), world_axes)
    sockets['socket_back_' + side] = ('spine_03', np.array([sx * 0.12, back[1], 1.36]), world_axes)
    # grip: in the palm, between the knuckles and the wrist. Axes: +Y along the gripped shaft towards the thumb side,
    # +Z the way the knuckles point (blade edge), +X out of the back of the hand.
    main = unit(j('middle_01') - j('hand')); radial = unit(j('index_01') - j('pinky_01')); radial = unit(radial - main * (radial @ main))
    palm = np.cross(main, radial); palm = palm if palm[0] * sx < 0 else -palm          # palms face the body in the bind pose
    grip = j('hand') + main * 0.075 + palm * 0.028
    Rg = np.stack([np.cross(radial, main), radial, main], 1)                       # columns: X, Y (shaft), Z (knuckles)
    sockets['socket_weapon_' + side] = ('hand_' + side, grip, Rg)
fore = unit(P_['hand_l'] - P_['lowerarm_l']); mid = (P_['hand_l'] + P_['lowerarm_l']) / 2
main = unit(P_['middle_01_l'] - P_['hand_l']); radial = unit(P_['index_01_l'] - P_['pinky_01_l']); dorsal = -np.cross(main, unit(radial - main * (radial @ main)))
dorsal = dorsal if dorsal[0] > 0 else -dorsal; dorsal = unit(dorsal - fore * (dorsal @ fore))
strap = surface_point(dorsal, mid, 0.07)
sockets['socket_shield_l'] = ('lowerarm_l', strap, np.stack([np.cross(-fore, dorsal), -fore, dorsal], 1))       # +Y towards the elbow, +Z out of the forearm
socket_info = {}
for name, (joint, pos, R) in sockets.items():
    M = np.eye(4); M[:3, :3] = np.eye(3) if R is world_axes else C_WG @ R; M[:3, 3] = C_WG @ pos       # R's columns are the socket axes in work space
    Lm = np.linalg.inv(Wg[joint]) @ M
    children[joint].append(glb.node(name=name, translation=Lm[:3, 3].round(6).tolist(), rotation=quat(Lm[:3, :3]).round(6).tolist()))
    assert abs(np.linalg.det(M[:3, :3]) - 1) < 1e-6, name
    socket_info[name] = dict(joint=joint, position=(C_WG @ pos).round(4).tolist())
for n in names:
    if children[n]: glb.j['nodes'][joint_node[n]]['children'] = children[n]
ibm = np.stack([np.linalg.inv(Wg[n]).T.reshape(-1) for n in names])
glb.j['skins'].append(dict(name='Armature', joints=[joint_node[n] for n in names], inverseBindMatrices=glb.acc(ibm, 'MAT4')))
mesh_nodes = []
for name, prims in meshes:
    me = dict(name=name, primitives=prims)
    if 'targets' in prims[0]: me['weights'] = [0.0] * len(FACE_NAMES); me['extras'] = dict(targetNames=FACE_NAMES)
    glb.j['meshes'].append(me); mesh_nodes.append(glb.node(name=name, mesh=len(glb.j['meshes']) - 1, skin=0))
arm = glb.node(name='Armature', children=[joint_node[n] for n in names if sk['parent'][n] is None] + mesh_nodes)
glb.j['scenes'][0]['nodes'] = [arm]

# ---------------------------------------------------------------- clips: the current kit's (hand-edited) clips, pelvis height retargeted
# Fingers. (1) A clip that leaves every finger at the kit's flat rest pose has no finger animation (Meshy's
# rig has no fingers, so its run is such a clip): it takes the hands of the Idle clip, so the hands do not
# spring flat open. (2) The clips' finger keys are relative to a hand joint aligned like the kit's; this
# hand joint is bound like the previous kit's instead, so the first joint of each finger is re-based.
og, obuf = sk['old_glb']; oname = {i: n.get('name') for i, n in enumerate(og['nodes'])}; onode = {v: k for k, v in oname.items()}
hip_old = np.mean([og['nodes'][onode[t]]['translation'] for t in ('thigh_l', 'thigh_r')], axis=0)
hip_new = np.mean([glb.j['nodes'][joint_node[t]]['translation'] for t in ('thigh_l', 'thigh_r')], axis=0)
scale = np.mean([sk['pos'][t][2] for t in ('thigh_l', 'thigh_r')]) / np.mean([sk['old_pos'][t][2] for t in ('thigh_l', 'thigh_r')])
is_finger = lambda n: n.split('_')[0] in skeleton.FINGERS
kit_rest = lambda n: np.array(og['nodes'][onode[n]].get('rotation', [0, 0, 0, 1]), float)
turn = lambda q, r: np.degrees(2 * np.arccos(np.clip(np.abs(q @ r), 0, 1)))        # angle between rotations
chans = lambda an: {(oname[c['target']['node']], c['target']['path']): an['samplers'][c['sampler']] for c in an['channels']}
idle = chans(next(a for a in og['animations'] if a['name'] == 'Idle'))
hands = {n: accessor(og, obuf, idle[(n, 'rotation')]['output']).astype(np.float64) for n in names if is_finger(n)}
assert max(turn(q, q[0]).max() for q in hands.values()) < 1.0, 'the Idle hand pose is expected to be constant'
rebase = {n: sk['rot'][sk['parent'][n]].T @ sk['aligned'][sk['parent'][n]] for n in names if is_finger(n) and sk['parent'][n] in sk['aligned']}
filled = []
for an in og['animations']:
    ch = chans(an)
    samplers, channels, times = [], [], {}
    no_fingers = max(turn(accessor(og, obuf, s['output']).astype(np.float64), kit_rest(jn)).max() for (jn, path), s in ch.items() if is_finger(jn) and path == 'rotation') < 1.0
    if no_fingers: filled.append(an['name'])
    pel_q = accessor(og, obuf, ch[('pelvis', 'rotation')]['output']).astype(np.float64)
    for (jn, path), s in ch.items():
        if jn not in joint_node: continue
        tin = accessor(og, obuf, s['input']).reshape(-1).astype(np.float32); val = accessor(og, obuf, s['output']).astype(np.float64)
        if path == 'translation':
            assert jn == 'pelvis' and len(val) == len(pel_q), 'only the pelvis translation is keyed, at the pelvis rotation key times'
            val = np.array([scale * (p + quat_to_mat(q) @ hip_old) - quat_to_mat(q) @ hip_new for p, q in zip(val, pel_q)])
        if path == 'rotation' and is_finger(jn):
            if no_fingers: val = np.tile(hands[jn][0], (len(val), 1))
            if jn in rebase:
                val = np.array([quat(rebase[jn] @ quat_to_mat(q)) for q in val])
                for i in range(1, len(val)):
                    if val[i] @ val[i - 1] < 0: val[i] = -val[i]
        key = tin.tobytes()
        if key not in times: times[key] = glb.acc(tin[:, None], 'SCALAR', minmax=True)
        samplers.append(dict(input=times[key], output=glb.acc(val, 'VEC4' if path == 'rotation' else 'VEC3'), interpolation=s.get('interpolation', 'LINEAR')))
        channels.append(dict(sampler=len(samplers) - 1, target=dict(node=joint_node[jn], path=path)))
    glb.j['animations'].append(dict(name=an['name'], samplers=samplers, channels=channels))

print('clips without finger animation, given the Idle hands:', ', '.join(filled) or 'none')
glb_path = os.path.join(out_dir, KIT_ID + '.glb'); size = glb.write(glb_path)
visible = sum(v for k, v in tri_count.items() if not k.startswith(('Brows_', 'Hair_', 'Beard_'))) + tri_count['Brows_' + BROWS[0]]   # bald, one brow set
manifest = collections.OrderedDict(
    id=KIT_ID, ancestry='human', body='male', path=f'characters/kits/{KIT_ID}.glb', sole=0.0, top=round(float(V[:, 2].max()), 4),
    license='LicenseRef-Meshy-Pro-private',
    regions=[n for n, _ in meshes if n.startswith('Body_') or n == 'Trunks'],
    groups=collections.OrderedDict([
        # the first option of a group is its default; the face group changes shape (morph weights), not nodes
        ('face', [dict(id='chiseled', label='Chiseled', nodes=[], weights=[0.0] * len(FACE_NAMES))] +
                 [dict(id=n.lower(), label=n, nodes=[], weights=[1.0 if j == i else 0.0 for j in range(len(FACE_NAMES))]) for i, n in enumerate(FACE_NAMES)]),
        ('brows', [dict(id='Brows_' + b, label=b, nodes=['Brows_' + b]) for b in BROWS] + [dict(id='Brows_None', label='None', nodes=[])]),
        # a look that names no option of this kit (such as another kit's hairstyle) falls back to "none"
        ('hair', [dict(id='none', label='None', nodes=[])] + [dict(id='Hair_' + h, label=lab, nodes=['Hair_' + h]) for h, lab, _ in HAIR]),
        ('beard', [dict(id='none', label='None', nodes=[])] + [dict(id='Beard_' + b, label=lab, nodes=['Beard_' + b]) for b, lab, _ in BEARDS]),
    ]),
    materials=collections.OrderedDict([('Skin', dict(tint='skin')), ('Skin_Head', dict(tint='skin')), ('Eye_Iris', dict(tint='eyes'))] +
                                      [('Brows_' + b, dict(tint='hair')) for b in BROWS] + [('Hair_' + h, dict(tint='hair')) for h, _, _ in HAIR] +
                                      [('Beard_' + b, dict(tint='facialHair')) for b, _, _ in BEARDS]),
    morph=None, morphNodes=['Body_Head'] + ['Brows_' + b for b in BROWS] + ['Hair_' + h for h, _, _ in HAIR] + ['Beard_' + b for b, _, _ in BEARDS], eyes='brown', sockets=socket_info,
    slots=dict(armor=['head', 'shoulders', 'back', 'chest', 'shirt', 'tabard', 'bracers', 'gloves', 'belt', 'pants', 'boots'], weapons=['mainHand', 'offHand', 'twoHand', 'ranged']),
    triangles=tri_count)
json.dump(manifest, open(os.path.join(out_dir, 'kit.json'), 'w'), indent=1)
print('wrote %s: %.2f MB' % (glb_path, size / 1e6)); print('triangles:', dict(tri_count), 'total', visible)
print('sockets:', ', '.join(f'{k}->{v["joint"]}' for k, v in socket_info.items()))
