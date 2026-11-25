"""Neutralise the baked normal map where the fitted mesh deliberately departs from the sculpt (ears, eyelids,
sealed pockets), and keep form only: a light blur removes sub-millimetre noise.   -> work/bake_normal.png"""
import numpy as np, cv2
from PIL import Image
from kitlib import KIT
n = np.asarray(Image.open(KIT + 'work/bake_normal_raw.png').convert('RGB'), np.float32) / 255
d = np.load(KIT + 'work/bake_dist.npy'); d = cv2.resize(d, (n.shape[1], n.shape[0]), interpolation=cv2.INTER_LINEAR)
keep = 1 - np.clip((d - 0.0015) / 0.0015, 0, 1)
# the rebuilt eyelids and the ears (with their blend rings) are our own forms: no sculpt normals there
import collections
from mh import MH
mh = MH(); body = np.where(mh.body)[0]; loc = -np.ones(len(mh.V), int); loc[body] = np.arange(len(body)); mirror = loc[mh.mirror[body]]
geo = np.load(KIT + 'work/bake_geo.npz'); tid, bary, T = geo['tid'], geo['bary'], geo['T']
own = np.zeros(len(body)); loops = np.load(KIT + 'work/eye_loops_l.npy')
for k, ring in enumerate(loops):
    w = np.clip(k / 2, 0, 1); own[ring] = np.maximum(own[ring], w); own[mirror[ring]] = np.maximum(own[mirror[ring]], w)
nbr = collections.defaultdict(set)
for t in T:
    for a_, b_ in ((0, 1), (1, 2), (2, 0)): nbr[t[a_]].add(t[b_]); nbr[t[b_]].add(t[a_])
front = set(np.where(mh.mask('ears')[body])[0].tolist()); own[list(front)] = 1; seen = set(front)
for w in (1.0, 1.0, 0.75, 0.5, 0.25):
    nxt = {u for v in front for u in nbr[v] if u not in seen}
    for u in nxt: own[u] = max(own[u], w)
    seen |= nxt; front = nxt
m = tid >= 0; om = np.zeros(tid.shape, np.float32); om[m] = np.einsum('nk,nk->n', bary[m], own[T[tid[m]]])
om = cv2.resize(om, (n.shape[1], n.shape[0]), interpolation=cv2.INTER_LINEAR); om = cv2.dilate(om, np.ones((5, 5), np.uint8))
keep = keep * (1 - np.clip(om, 0, 1)); keep = cv2.GaussianBlur(keep, (0, 0), 3)
v = n * 2 - 1; v = cv2.GaussianBlur(v, (0, 0), 0.8)
v = v * keep[..., None] + np.array([0, 0, 1.0], np.float32) * (1 - keep[..., None]); v /= np.linalg.norm(v, axis=2, keepdims=True)
Image.fromarray(((v * 0.5 + 0.5) * 255 + 0.5).astype(np.uint8)).save(KIT + 'work/bake_normal.png')
print('normal map: %.1f%% of texels neutralised' % ((keep < 0.5).mean() * 100))
