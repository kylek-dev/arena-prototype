"""Load the exported MakeHuman scaffold (work/mh_base.npz)."""
import json
import numpy as np
from kitlib import KIT


class MH:
    def __init__(self, path=KIT + 'work/mh_base.npz'):
        d = np.load(path)
        self.V = d['V'].astype(np.float64)
        n = len(self.V)
        self.Q = d['loop_vert'].reshape(-1, 4).astype(np.int64)        # all quads
        self.loop_uv = d['loop_uv'].astype(np.float64)
        self.names = [str(x) for x in d['group_names']]
        self.gi, self.gv, self.gw = d['g_group'], d['g_vert'], d['g_weight']
        self.bones = json.loads(str(d['bones']))
        self.bone_names = [b['name'] for b in self.bones]
        self.parent = {b['name']: b['parent'] for b in self.bones}
        self.head = {b['name']: np.array(b['head']) for b in self.bones}
        self.tail = {b['name']: np.array(b['tail']) for b in self.bones}
        W = np.zeros((n, len(self.bones)))
        for k, b in enumerate(self.bone_names):
            if b in self.names:
                m = self.gi == self.names.index(b); W[self.gv[m], k] = self.gw[m]
        self.W_raw = W
        self.body = self.mask('body')
        # every quad is either all body or all helper
        self.Qbody = self.Q[self.body[self.Q].all(1)]
        # mirror map (x -> -x) on the basis shape
        key = lambda P: np.round(P * 2e4).astype(np.int64)
        idx = {tuple(k): i for i, k in enumerate(key(self.V))}
        mk = key(self.V * np.array([-1, 1, 1.0]))
        self.mirror = np.array([idx.get(tuple(k), -1) for k in mk])

    def mask(self, group):
        m = np.zeros(len(self.V), bool); m[self.gv[self.gi == self.names.index(group)]] = True; return m

    def weights(self):
        """Bone weights for every vertex; helpers without weights copy the nearest weighted body vertex."""
        W = self.W_raw.copy(); tot = W.sum(1)
        bad = np.where(tot < 1e-6)[0]
        if len(bad):
            from scipy.spatial import cKDTree
            good = np.where((tot > 1e-6) & self.body)[0]
            _, j = cKDTree(self.V[good]).query(self.V[bad]); W[bad] = W[good[j]]
        return W / W.sum(1, keepdims=True)
