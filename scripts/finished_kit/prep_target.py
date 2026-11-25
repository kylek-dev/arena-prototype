"""Prepare the fit target from the raw Meshy files: the dense sculpt in work space, welded, with a
per-vertex cloth mask read from its own paint, plus Meshy's joint positions.  -> work/target.npz"""
import json
import numpy as np
from PIL import Image
import igl
from kitlib import *

g, buf, P, UV, F = load_mesh(DENSE_GLB)
V = dense_to_work(P)
Vw, Fw, inv, ok = weld(V, F)
print('dense verts', len(P), '-> welded', len(Vw), 'tris', len(Fw), 'bounds', Vw.min(0).round(4), Vw.max(0).round(4))
# symmetry check: distance from mirrored verts to the surface
M = Vw * np.array([-1, 1, 1.0])
d2, _, _ = igl.point_mesh_squared_distance(M, Vw, Fw)
d = np.sqrt(d2); print('mirror distance mm: mean %.3f p95 %.3f max %.3f' % (d.mean() * 1e3, np.percentile(d, 95) * 1e3, d.max() * 1e3))
# cloth mask from the dense model's own paint (dark, unsaturated cloth vs skin)
img = np.asarray(Image.open(KIT + 'src/dense_basecolor.png').convert('RGB'), np.float32) / 255
col = bilinear(img, UV)
lum = col @ np.array([0.2126, 0.7152, 0.0722]); sat = col.max(1) - col.min(1)
cloth_split = (lum < 0.42) & (sat < 0.16)
acc = np.zeros(len(Vw)); cnt = np.zeros(len(Vw)); np.add.at(acc, inv, cloth_split.astype(float)); np.add.at(cnt, inv, 1)
cloth = acc / np.maximum(cnt, 1) > 0.5
colw = np.zeros((len(Vw), 3)); np.add.at(colw, inv, col); colw /= np.maximum(cnt, 1)[:, None]
print('cloth verts', cloth.sum(), 'of', len(Vw), ' z range of cloth', Vw[cloth, 2].min().round(3), Vw[cloth, 2].max().round(3))
# Meshy joints (rest pose) from the rigged low export
gl, bl = read_glb(LOW_GLB)
W = world_matrices(gl); skin = gl['skins'][0]
joints = {gl['nodes'][j]['name']: low_to_work(W[j][:3, 3]).round(5).tolist() for j in skin['joints']}
for k in ('pelvis', 'spine_03', 'neck_01', 'head', 'clavicle_l', 'upperarm_l', 'lowerarm_l', 'hand_l', 'LeftHand_End', 'thigh_l', 'calf_l', 'foot_l', 'ball_l'):
    print(' ', k, joints[k])
json.dump(joints, open(KIT + 'work/meshy_joints.json', 'w'), indent=1)
np.savez_compressed(KIT + 'work/target.npz', V=Vw, F=Fw, cloth=cloth, color=colw.astype(np.float32),
                    corner_uv=UV[F[ok]].astype(np.float32), split_index=F[ok], inv=inv)
