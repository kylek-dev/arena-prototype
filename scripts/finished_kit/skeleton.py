"""The kit skeleton for the rebuilt body.

Joint names, order and hierarchy are the game kit's (65 joints, UE4 Mannequin names). The body is bound in
its own A-pose:
  - torso, head, clavicles, legs and feet keep the kit's rest orientations, exactly as the current
    `meshy-human-male` kit does, so the existing clips move them the same way;
  - upper and lower arms and the hands use that kit's bind orientations (arms re-aimed to the A-pose), so
    every clip moves the wrist exactly as it did on that kit;
  - every finger bone is aligned to the real finger of this mesh, and the finger joints are placed on the
    fitted fingers (the scaffold's joints carried over by a local fit of the skin around each joint);
  - the trunk joints (pelvis and the three spine joints, neck base) sit where that kit has them, because
    the clips' back arch was tuned on those pivots: lower pivots turn it into a thrust-out chest;
  - every other joint is the scaffold's anatomical joint centre, mirrored to be exactly symmetric.
The pelvis translation keys of the clips are retargeted so the hip height scales with this body's legs."""
import numpy as np
from rigdata import *

MIR = np.diag([-1.0, 1, 1])
FINGERS = ('index', 'middle', 'ring', 'pinky', 'thumb')
TRUNK = ('pelvis', 'spine_01', 'spine_02', 'spine_03', 'neck_01')


def unit(v): return v / np.linalg.norm(v)
def frame(d, lateral):
    d = unit(d); n = unit(lateral - d * (lateral @ d)); return np.stack([d, n, np.cross(d, n)], 1)


def carry(p, Vr, Vf, cand, k=40):
    """Where a point of the scaffold's rest shape lies in the fitted shape: a similarity fit of the rest
    vertices nearest to it (among `cand`) onto their fitted positions."""
    idx = cand[np.argsort(np.linalg.norm(Vr[cand] - p, axis=1))[:k]]; A, B = Vr[idx], Vf[idx]; ma, mb = A.mean(0), B.mean(0)
    U, S, Vt = np.linalg.svd((B - mb).T @ (A - ma)); D = np.diag([1, 1, np.sign(np.linalg.det(U @ Vt))])
    return mb + (S * np.diag(D)).sum() / ((A - ma) ** 2).sum() * (U @ D @ Vt) @ (p - ma)


def build(mh, Vpose, Vfit=None):
    head, tail = scaffold_joints(mh, Vpose)
    if Vfit is None: Vfit = np.load(KIT + 'work/fit.npz')['V']
    body = np.where(mh.body)[0]; Vrest = mh.V[body]; Wb = mh.weights()[body]; bone = {b: i for i, b in enumerate(mh.bone_names)}
    rest_head, rest_tail = scaffold_joints(mh, mh.V)
    g, buf, names, parent, kit_pos, kit_rot = kit_skeleton(OLD_KIT)       # node rest of the old kit == the kit's rest pose
    old_pos, old_rot = bind_pose(OLD_KIT)
    sc = lambda n: head[{'root': 'Root', 'Head': 'head'}.get(n, n)]
    pos, rot, aligned = {}, {}, {}
    # ---- positions (left side and centre from the scaffold; right side mirrored)
    for n in names:
        if n.endswith('_r'): continue
        if n == 'root': p = np.zeros(3)
        elif n.endswith('_04_leaf_l'): p = tail[n.replace('_04_leaf_l', '_03_l')]
        elif n == 'ball_leaf_l': p = tail['ball_l']
        elif n in TRUNK: p = old_pos[n].copy()
        else: p = sc(n).copy()
        if not n.endswith('_l'): p[0] = 0.0
        pos[n] = p
    for fg in FINGERS:                                                    # finger joints: on the fitted fingers
        for k in ('_01_l', '_02_l', '_03_l'):
            pos[fg + k] = carry(rest_head[fg + k], Vrest, Vfit, np.where(Wb[:, bone[fg + k]] > 0.05)[0])
        pos[fg + '_04_leaf_l'] = carry(rest_tail[fg + '_03_l'], Vrest, Vfit, np.where(Wb[:, bone[fg + '_03_l']] > 0.5)[0])
    # ---- orientations
    lat_o = pos['index_01_l'] - pos['pinky_01_l']; lat_k = kit_pos['index_01_l'] - kit_pos['pinky_01_l']
    for n in names:
        if n.endswith('_r'): continue
        base = n[:-2] if n.endswith('_l') else n
        if base in ('upperarm', 'lowerarm'): R = old_rot[n]
        elif base == 'hand':
            R = old_rot[n]
            # where the hand joint would point if it were aligned to this hand the way the kit's is to its own:
            # finger keys of clips are relative to that (see build_kit.py)
            aligned[n] = frame(pos['middle_01_l'] - pos[n], lat_o) @ frame(kit_pos['middle_01_l'] - kit_pos[n], lat_k).T @ kit_rot[n]
        elif base.split('_')[0] in FINGERS:
            fb = n.replace('_04_leaf_l', '_03_l')                         # leaf joints follow their last phalanx
            nxt = {'_01_l': '_02_l', '_02_l': '_03_l', '_03_l': '_04_leaf_l'}
            ch = fb[:-5] + nxt[fb[-5:]]
            R = frame(pos[ch] - pos[fb], lat_o) @ frame(kit_pos[ch] - kit_pos[fb], lat_k).T @ kit_rot[n]
        else: R = kit_rot[n]
        rot[n] = R
    for n in names:
        if n.endswith('_r'):
            l = n[:-2] + '_l'; pos[n] = MIR @ pos[l]; rot[n] = MIR @ rot[l] @ MIR
            if l in aligned: aligned[n] = MIR @ aligned[l] @ MIR
    # mirrored frames must match the kit's own right-side convention
    chk = max(np.abs(MIR @ kit_rot[n[:-2] + '_l'] @ MIR - kit_rot[n]).max() for n in names if n.endswith('_r'))
    assert chk < 2e-3, 'kit right-side joints are not mirror images of the left (%.4f)' % chk
    return dict(names=names, parent=parent, pos=pos, rot=rot, aligned=aligned, kit_pos=kit_pos, kit_rot=kit_rot, old_pos=old_pos, old_rot=old_rot, old_glb=(g, buf))
