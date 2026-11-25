"""Face presets as morph targets on the head, composed from MakeHuman's CC0 face shape targets (artist-made
deltas on the scaffold topology). The deltas are scaled to this head, and faded to zero at the neck line,
around the eyes (the eye pieces are rigid) and on the scalp (so hairstyles fit every face).
-> work/face_morphs.npz"""
import gzip, collections
import numpy as np
from kitlib import *
from mh import MH
import layout

PRESETS = collections.OrderedDict([          # MakeHuman target -> weight ("lr-" applies the l- and r- targets together)
    ('Broad', {'head-square': 0.7, 'chin-width-incr': 0.8, 'chin-bones-incr': 0.6, 'lr-cheek-bones-incr': 0.5, 'nose-width2-incr': 0.5, 'nose-width3-incr': 0.4, 'mouth-scale-horiz-incr': 0.3}),
    ('Lean', {'head-oval': 0.6, 'lr-cheek-volume-decr': 0.8, 'lr-cheek-bones-incr': 0.4, 'chin-width-decr': 0.5, 'chin-height-incr': 0.4, 'nose-scale-horiz-decr': 0.4, 'nose-hump-incr': 0.3}),
    ('Rugged', {'head-rectangular': 0.6, 'chin-prominent-incr': 0.7, 'chin-bones-incr': 0.5, 'chin-cleft-incr': 0.7, 'nose-hump-incr': 0.7, 'nose-volume-incr': 0.5, 'nose-width1-incr': 0.4, 'forehead-trans-forward': 0.4}),
])
mh = MH(); body = np.where(mh.body)[0]; loc = -np.ones(len(mh.V), int); loc[body] = np.arange(len(body)); mirror = loc[mh.mirror[body]]
L = layout.compute(mh); V, Q, region, names = L['V'], L['Q'], L['region'], L['names']

def target(name):
    d = np.zeros((len(mh.V), 3))
    for line in gzip.open(KIT + f'src/targets/{name}.target.gz').read().decode().splitlines():
        p = line.split()
        if len(p) == 4: d[int(p[0])] = (float(p[1]), float(p[2]), float(p[3]))
    return np.stack([d[:, 0], -d[:, 2], d[:, 1]], 1)[body] * 0.1          # MakeHuman (Y up, Z front, decimetres) -> work space metres
HEAD_SCALE = 1.0085 * 0.906                                               # global and head scales of the fit (fit_pose.py)

# --- where the face may move
nbr = collections.defaultdict(set)
for q in Q:
    for k in range(4): nbr[q[k]].add(q[(k + 1) % 4]); nbr[q[(k + 1) % 4]].add(q[k])
head_v = np.zeros(len(V), bool); head_v[np.unique(Q[region == names.index('Head')])] = True
other_v = np.zeros(len(V), bool); other_v[np.unique(Q[region != names.index('Head')])] = True
def rings_from(seed, n):                                                  # 0 on the seed, rising to 1 over n rings
    w = np.ones(len(V)); front = set(np.where(seed)[0].tolist()); w[list(front)] = 0; seen = set(front)
    for k in range(1, n):
        nxt = {u for v in front for u in nbr[v] if u not in seen}
        for u in nxt: w[u] = k / n
        seen |= nxt; front = nxt
    return w
loops = np.load(KIT + 'work/eye_loops_l.npy'); eye = np.zeros(len(V), bool)
eye[loops[4:12].ravel()] = True; eye[mirror[loops[4:12].ravel()]] = True         # lid rings near the opening stay put
scalp = mh.mask('scalp')[body]
free = head_v * rings_from(other_v, 4) * rings_from(eye, 4) * rings_from(scalp, 3)
print('head vertices %d, free to move (weight > 0.5): %d' % (head_v.sum(), (free > 0.5).sum()))

_f = L['fit']; m_lo, m_up, m_seg, m_t = _f['mouth_lower'], _f['mouth_upper'], _f['mouth_seg'], _f['mouth_t']
out = []
for name, mix in PRESETS.items():
    d = np.zeros((len(V), 3))
    for t, w in mix.items():
        for tn in ([t.replace('lr-', 'l-'), t.replace('lr-', 'r-')] if t.startswith('lr-') else [t]): d += w * target(tn)
    d = d * HEAD_SCALE * free[:, None]
    d = (d + d[mirror] * np.array([-1, 1, 1.0])) / 2                      # exactly symmetric
    d[m_up] = d[m_lo][m_seg] * (1 - m_t[:, None]) + d[m_lo][np.minimum(m_seg + 1, len(m_lo) - 1)] * m_t[:, None]   # the closed lips move as one line
    out.append(d); m = np.linalg.norm(d, axis=1)
    print('%-7s max move %.1f mm, mean over the face %.2f mm' % (name, m.max() * 1e3, m[free > 0.5].mean() * 1e3))
np.savez_compressed(KIT + 'work/face_morphs.npz', names=np.array(list(PRESETS)), deltas=np.array(out), free=free)
