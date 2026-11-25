"""Joint data: the scaffold rig's joints on a given shape, and the game kit's rest skeleton (work space)."""
import json
import numpy as np
from kitlib import *
from mh import MH


def scaffold_joints(mh, Vall):
    """Head and tail of each scaffold bone: centres of the base mesh's joint helper cubes."""
    rig = json.load(open(KIT + 'src/rig.game_engine.json'))
    def end(d):
        if d['strategy'] == 'CUBE': return Vall[mh.mask(d['cube_name'])].mean(0)
        return Vall[d['vertex_indices']].mean(0)
    return {b: end(d['head']) for b, d in rig.items()}, {b: end(d['tail']) for b, d in rig.items()}


def kit_skeleton(path):
    """Names (skin order), parents, rest world positions and rotations (work-space axes) of the kit skeleton."""
    g, buf = read_glb(path); W = world_matrices(g); skin = g['skins'][0]; par = parents(g)
    C = np.array([[1, 0, 0], [0, 0, -1], [0, 1, 0.0]])                 # glTF (Y up, +Z front) -> work (Z up, -Y front)
    names = [g['nodes'][j]['name'] for j in skin['joints']]
    parent = {g['nodes'][j]['name']: (g['nodes'][par[j]]['name'] if par.get(j) in skin['joints'] else None) for j in skin['joints']}
    pos = {g['nodes'][j]['name']: C @ W[j][:3, 3] for j in skin['joints']}
    rot = {g['nodes'][j]['name']: C @ W[j][:3, :3] for j in skin['joints']}   # world rest orientation, columns = joint axes in work space
    return g, buf, names, parent, pos, rot


def bind_pose(path):
    """Bind pose from a GLB's inverse bind matrices: joint names, world positions and rotations (work space)."""
    g, buf = read_glb(path); skin = g['skins'][0]
    ibm = accessor(g, buf, skin['inverseBindMatrices']).reshape(-1, 4, 4).transpose(0, 2, 1).astype(np.float64)
    C = np.array([[1, 0, 0], [0, 0, -1], [0, 1, 0.0]])
    pos, rot = {}, {}
    for j, M in zip(skin['joints'], ibm):
        B = np.linalg.inv(M); n = g['nodes'][j]['name']; pos[n] = C @ B[:3, 3]; rot[n] = C @ B[:3, :3]
    return pos, rot
