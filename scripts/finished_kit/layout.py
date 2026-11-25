"""Shared face layout of the body: kept quads, their UVs, region per face, which faces use the head texture,
which faces the trunks piece covers, and the texture crops (in the master UV square) of the head and trunks."""
import numpy as np
from kitlib import *
from mh import MH


def compute(mh=None, master=4096, pad_px=10):
    mh = mh or MH(); fit = np.load(KIT + 'work/fit_eyes.npz'); rg = np.load(KIT + 'work/regions.npz')
    keep = fit['face_keep']; Q = fit['Q'][keep]; is_body = mh.body[mh.Q].all(1); uvq = mh.loop_uv.reshape(-1, 4, 2)[is_body][keep]
    names = [str(x) for x in rg['names']]; region = rg['region']
    # UV islands: faces connected through a shared (vertex, uv) corner
    key = np.concatenate([Q.reshape(-1, 1), np.round(uvq.reshape(-1, 2) * 1e5).astype(np.int64)], 1)
    _, inv = np.unique(key, axis=0, return_inverse=True); inv = inv.reshape(-1, 4)
    parent = np.arange(inv.max() + 1)
    def find(x):
        while parent[x] != x: parent[x] = parent[parent[x]]; x = parent[x]
        return x
    for f in inv:
        r = find(f[0])
        for c in f[1:]: parent[find(c)] = r
    island = np.array([find(f[0]) for f in inv])
    head_faces = region == names.index('Head')
    isl, cnt = np.unique(island[head_faces], return_counts=True); head_island = isl[np.argmax(cnt)]
    use_head_tex = island == head_island
    trunks = np.isin(region, [names.index('Hips'), names.index('Thighs')])
    def crop(mask):
        uv = uvq[mask].reshape(-1, 2); lo, hi = uv.min(0), uv.max(0); c = (lo + hi) / 2; h = (hi - lo).max() / 2 + pad_px / master
        return np.array([c[0] - h, c[1] - h, c[0] + h, c[1] + h])               # square (u0, v0, u1, v1), Blender v (up)
    return dict(V=fit['V'], Q=Q, uvq=uvq, region=region, names=names, corner=inv, use_head_tex=use_head_tex, trunks=trunks,
                crop_head=crop(use_head_tex), crop_trunks=crop(trunks), fit=fit, regions_file=rg)


def remap(uv, rect):
    """Master UV -> UV inside a square crop."""
    return (uv - rect[:2]) / (rect[2:] - rect[:2])
