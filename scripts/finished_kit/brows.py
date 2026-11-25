"""Eyebrow pieces: a thin layer of the forehead's own quads just above the skin, with tintable brow textures.
'Natural' is the brow painted on Meshy's full-detail model, 'Bold' the heavier brow of its retopologized
model, 'Fine' a slimmer version of Natural. Textures are near-neutral grey with alpha, tinted by hair colour.
-> work/brows.npz, tex/brows_*.png"""
import numpy as np, cv2
from PIL import Image
from kitlib import *
from mh import MH
import layout

X0, X1, Z0, Z1 = 0.004, 0.070, 1.706, 1.752          # left brow window on the face (work space, metres)
TW, TH = 512, 384
mh = MH(); L = layout.compute(mh); V, Q, uvq, region, names = L['V'], L['Q'], L['uvq'], L['region'], L['names']
fc = V[Q].mean(1)
zone = (region == names.index('Head')) & (fc[:, 0] > X0 - 0.004) & (fc[:, 0] < X1 + 0.004) & (fc[:, 2] > Z0 - 0.004) & (fc[:, 2] < Z1 + 0.004) & (fc[:, 1] < -0.03)
loops = np.load(KIT + 'work/eye_loops_l.npy'); lid = set(loops[8:].ravel().tolist())
zone &= ~np.array([any(v in lid for v in q) for q in Q])                  # keep clear of the lid margin
fq = Q[zone]; fuv = uvq[zone]
planar = lambda P: np.stack([(P[..., 0] - X0) / (X1 - X0), (P[..., 2] - Z0) / (Z1 - Z0)], -1)
print('brow layer: %d quads per side' % len(fq))

# rasterise the zone's triangles in the planar brow space -> master UV per brow texel
T = np.concatenate([fq[:, [0, 1, 2]], fq[:, [0, 2, 3]]]); TUVm = np.concatenate([fuv[:, [0, 1, 2]], fuv[:, [0, 2, 3]]])
pl = planar(V[T]); px = np.stack([pl[..., 0] * TW, (1 - pl[..., 1]) * TH], -1)
muv = np.full((TH, TW, 2), np.nan, np.float32)
for t in range(len(T)):
    x, y = px[t, :, 0], px[t, :, 1]; x0, x1 = max(int(np.floor(x.min())), 0), min(int(np.ceil(x.max())), TW - 1); y0, y1 = max(int(np.floor(y.min())), 0), min(int(np.ceil(y.max())), TH - 1)
    d = (y[1] - y[2]) * (x[0] - x[2]) + (x[2] - x[1]) * (y[0] - y[2])
    if x0 > x1 or y0 > y1 or abs(d) < 1e-12: continue
    gx, gy = np.meshgrid(np.arange(x0, x1 + 1) + 0.5, np.arange(y0, y1 + 1) + 0.5)
    l0 = ((y[1] - y[2]) * (gx - x[2]) + (x[2] - x[1]) * (gy - y[2])) / d; l1 = ((y[2] - y[0]) * (gx - x[2]) + (x[0] - x[2]) * (gy - y[2])) / d; l2 = 1 - l0 - l1
    ins = (l0 >= -1e-3) & (l1 >= -1e-3) & (l2 >= -1e-3)
    muv[y0:y1 + 1, x0:x1 + 1][ins] = (l0[..., None] * TUVm[t, 0] + l1[..., None] * TUVm[t, 1] + l2[..., None] * TUVm[t, 2])[ins]
ok = ~np.isnan(muv[..., 0])
def sample(path):
    img = np.asarray(Image.open(path).convert('RGB'), np.float32) / 255; R = img.shape[0]
    mx = (np.nan_to_num(muv[..., 0]) * R - 0.5).astype(np.float32); my = ((1 - np.nan_to_num(muv[..., 1])) * R - 0.5).astype(np.float32)
    return cv2.remap(img, mx, my, cv2.INTER_LINEAR)
lum = lambda c: c @ np.array([0.2126, 0.7152, 0.0722], np.float32)
skin = lum(sample(KIT + 'work/tex_skin.png'))
def brow_alpha(path):
    p = sample(path); d = skin - lum(p); warm = p[..., 0] - p[..., 2] > 0.08         # darker than the skin there, and brow-coloured
    a = np.clip((d - 0.05) / 0.16, 0, 1) * ok * warm
    core = cv2.morphologyEx((a > 0.85).astype(np.uint8), cv2.MORPH_OPEN, np.ones((5, 5), np.uint8))   # solidly dark: the brow, not lid shading
    lab_n, lab, stats, _ = cv2.connectedComponentsWithStats(core)
    if lab_n > 1:                                                                   # keep the brow itself (largest solid patch) and its soft edge
        big = 1 + np.argmax(stats[1:, cv2.CC_STAT_AREA]); keep = cv2.dilate((lab == big).astype(np.uint8), cv2.getStructuringElement(cv2.MORPH_ELLIPSE, (11, 11))) > 0; a = a * keep
    return cv2.GaussianBlur(a, (0, 0), 0.8), p
shapes = {}
a_nat, p_nat = brow_alpha(KIT + 'work/bake_dense.png'); shapes['Natural'] = a_nat
a_bold, _ = brow_alpha(KIT + 'work/bake_low.png'); shapes['Bold'] = a_bold
fine = cv2.erode(a_nat, cv2.getStructuringElement(cv2.MORPH_ELLIPSE, (9, 7))); shapes['Fine'] = cv2.GaussianBlur(fine, (0, 0), 1.2)
for name, a in shapes.items():
    # near-neutral grey (the game multiplies by the hair colour), a little lighter towards the feathered edge
    g = 0.70 + 0.14 * (1 - a) + 0.04 * np.sin(np.linspace(0, 40, TW))[None, :] * a
    rgba = np.dstack([g, g, g * 0.985, a]); Image.fromarray((np.clip(rgba, 0, 1) * 255 + 0.5).astype(np.uint8), 'RGBA').save(KIT + f'tex/brows_{name.lower()}.png', optimize=True)
    ys, xs = np.where(a > 0.5); print('%-8s coverage %.1f%%, spans %.1f x %.1f mm' % (name, (a > 0.5).mean() * 100, (xs.max() - xs.min()) / TW * (X1 - X0) * 1e3, (ys.max() - ys.min()) / TH * (Z1 - Z0) * 1e3))
# keep only the quads a brow actually covers (any shape), plus one ring of margin
union = cv2.dilate(np.max(list(shapes.values()), axis=0), np.ones((9, 9), np.uint8))
qpx = np.stack([planar(V[fq])[..., 0] * TW, (1 - planar(V[fq])[..., 1]) * TH], -1)
used = np.array([union[max(int(q[:, 1].min()), 0):min(int(q[:, 1].max()) + 2, TH), max(int(q[:, 0].min()), 0):min(int(q[:, 0].max()) + 2, TW)].max(initial=0) > 0.02 for q in qpx])
vs = set(fq[used].ravel().tolist()); used |= np.array([any(v in vs for v in q) for q in fq])
print('brow layer trimmed to %d quads per side' % used.sum())
np.savez_compressed(KIT + 'work/brows.npz', quads=fq[used], names=np.array(list(shapes)), window=np.array([X0, X1, Z0, Z1]))
prev = np.concatenate([np.dstack([shapes[n]] * 3) * 0.9 + 0.05 for n in shapes], 1); Image.fromarray((prev * 255).astype(np.uint8)).save(KIT + 'qa/brows_alpha.png')
