"""Blank skin from Meshy's own paint, on our UVs:
 - the tank top's painted chest/ab/back shading is re-coloured with the skin tones found on the body
   (a luminance ramp measured from the arms, neck and legs), and joined to the real skin smoothly across
   the old neckline and armholes;
 - the painted eyebrows are lifted out (kept as a separate tintable brow texture) and the skin under them
   is filled from the surrounding forehead.
work/bake_dense.png -> work/tex_skin.png, work/tex_brow_meshy.png, QA masks"""
import numpy as np, cv2
import scipy.sparse as sp
from scipy.sparse.linalg import spsolve
from PIL import Image
import igl
from kitlib import *

Z_WAIST, Z_HEM = 1.092, 0.690      # the old shirt hem / trunks waist, and the old shorts hem on the thigh
NATURAL = np.array([0xdb, 0x9e, 0x7a], np.float32) / 255      # palette "skin-natural" = skinAtlas in web/src/domain/appearance.ts
col = np.asarray(Image.open(KIT + 'work/bake_dense.png').convert('RGB'), np.float32) / 255; R = col.shape[0]; PX = R / 2048   # filter sizes are tuned at 2048
aux = np.load(KIT + 'work/bake_aux.npz'); pos = aux['pos']; cloth = aux['cloth'].astype(np.float32); band = aux['band'].astype(np.float32); cov = aux['mask']
geo = np.load(KIT + 'work/bake_geo.npz'); tid, bary, T = geo['tid'], geo['bary'], geo['T']
fit = np.load(KIT + 'work/fit_eyes.npz'); V = fit['V']
lum = lambda c: c @ np.array([0.2126, 0.7152, 0.0722], np.float32)
ell = lambda px: cv2.getStructuringElement(cv2.MORPH_ELLIPSE, (int(px * PX) | 1, int(px * PX) | 1))
from scipy import ndimage
def fill(img, mask):                                                     # nearest valid texel everywhere (keeps filters clean at UV island borders)
    idx = ndimage.distance_transform_edt(~mask, return_distances=False, return_indices=True); return img[idx[0], idx[1]]
def inpaint(img, mask, radius):
    # Texels outside the UV islands hold nothing, and a blot on an island's edge would be refilled from
    # them. So everything that is not good paint first takes its nearest good texel, and only then is the
    # masked area smoothed in from its surroundings.
    bad = mask > 0; base = fill(img, cov & ~bad)
    u8 = (np.clip(base, 0, 1) * 255).astype(np.uint8)
    return np.where(bad[..., None], cv2.inpaint(u8[..., ::-1].copy(), bad.astype(np.uint8), int(radius * PX), cv2.INPAINT_TELEA)[..., ::-1].astype(np.float32) / 255, img)
x_, y_, z = pos[..., 0], pos[..., 1], pos[..., 2]
hands = (np.abs(x_) > 0.36) & (z < 1.08)
tank = cloth > 0.5                 # all painted clothing becomes skin; the trunks are a separate piece with the cloth paint
skin = (cloth < 0.05) & (band < 0.05) & (z < 1.52) & (z > 0.12) & ((z > 1.14) | (z < 0.64))   # body skin away from hems, face, soles
L = lum(col)
# every old garment edge gets a blend zone: neckline and armholes (from the sculpt), waist and leg hems (by height)
waist = np.clip(1 - np.abs(z - Z_WAIST) / 0.045, 0, 1) * (np.abs(x_) < 0.27); leghem = np.clip(1 - np.abs(z - Z_HEM) / 0.04, 0, 1) * (np.abs(x_) < 0.30)
band = np.maximum(band, np.maximum(waist, leghem).astype(np.float32) * cov)

# 1. skin colour as a function of brightness, measured on the real skin
Ls = L[skin & cov]; Cs = col[skin & cov]
edges = np.quantile(Ls, np.linspace(0.005, 0.995, 49)); mid = (edges[:-1] + edges[1:]) / 2
ramp = np.array([np.median(Cs[(Ls >= a) & (Ls < b)], axis=0) for a, b in zip(edges[:-1], edges[1:])])
for k in range(3): ramp[:, k] = np.maximum.accumulate(ramp[:, k])       # brighter is never darker in any channel
to_skin = lambda l: np.stack([np.interp(l, mid, ramp[:, k]) for k in range(3)], -1)

# 2. the cloth's painted shading: drop weave and thin wrinkle strokes, keep the muscle forms
Lc = fill(L, tank & cov)
Lc = cv2.morphologyEx(Lc, cv2.MORPH_CLOSE, ell(4)); Lc = cv2.bilateralFilter(Lc, int(9 * PX) | 1, 0.06, 4 * PX)
core = tank & (band < 0.05)
# (64-bit sums: a float32 mean over ten million texels drifts by several percent)
mc, sc_ = Lc[core].mean(dtype=np.float64), Lc[core].std(dtype=np.float64); Lsm, Lss = Ls.mean(dtype=np.float64), Ls.std(dtype=np.float64)
gain = 1.15 * Lss / sc_; Ln = np.clip(Lsm + 0.01 + (Lc - mc) * gain, np.quantile(Ls, 0.03), np.quantile(Ls, 0.985)).astype(np.float32)
out = np.where(tank[..., None], to_skin(Ln), col)
print('cloth shading sd %.3f -> skin sd %.3f (gain %.2f)' % (sc_, Ln[core].std(dtype=np.float64), gain))

# 3. paint residue first, so no blend below can pick it up: cloth-coloured specks, thin dark hem strokes and
#    anything far darker than skin on the body (not hands, feet or face)
near_hem = cv2.dilate(((cloth > 0.02) & (cloth < 0.98) | (band > 0.02)).astype(np.uint8), ell(31)) > 0
def residue(img):
    Lo = lum(img).astype(np.float32); body_ = cov & (z > 0.2) & (z < 1.60) & ~hands
    speck = body_ & (Lo < 0.50) & (img.max(2) - img.min(2) < 0.25)
    stroke = body_ & near_hem & (cv2.morphologyEx(Lo, cv2.MORPH_CLOSE, ell(11)) - Lo > 0.035)
    blot = body_ & near_hem & (cv2.morphologyEx(Lo, cv2.MORPH_CLOSE, ell(45)) - Lo > 0.08)
    dark = body_ & (Lo < 0.45)
    bad = cv2.dilate((speck | stroke | blot | dark).astype(np.uint8), ell(7))
    return inpaint(img, bad, 5), int(bad.sum())
out, n1 = residue(out)

# 4. join across the old garment edges: per-vertex harmonic blend between the skin either side
m = cov
def per_vertex(img):                                                     # average of the texels around each vertex
    acc = np.zeros((len(V), img.shape[-1] if img.ndim == 3 else 1)); cnt = np.zeros(len(V))
    ii = tid[m]; w = bary[m]; val = img[m].reshape(len(ii), -1)
    for k in range(3):
        sel = w[:, k] > 0.6; np.add.at(acc, T[ii[sel], k], val[sel]); np.add.at(cnt, T[ii[sel], k], 1)
    return acc / np.maximum(cnt, 1)[:, None], cnt > 0
vc, has = per_vertex(out); vband, _ = per_vertex(band)
vb = (vband[:, 0] > 0.25) & has
E = np.unique(np.sort(np.concatenate([T[:, [0, 1]], T[:, [1, 2]], T[:, [2, 0]]]), 1), axis=0)
n = len(V); A = sp.coo_matrix((np.ones(len(E)), (E[:, 0], E[:, 1])), shape=(n, n)); A = (A + A.T).tocsr(); Lap = sp.diags(A.sum(1).A1) - A
b_idx = np.where(vb)[0]; f_idx = np.where(~vb)[0]
x = vc.copy(); x[b_idx] = spsolve(Lap[b_idx][:, b_idx].tocsc(), -Lap[b_idx][:, f_idx] @ vc[f_idx])
smooth = np.zeros_like(out); smooth[m] = np.einsum('nk,nkj->nj', bary[m], x[T[tid[m]]])
wb = cv2.GaussianBlur(np.clip(band * 1.6, 0, 1), (0, 0), 3 * PX)[..., None]
out = out * (1 - wb) + smooth * wb
out, n2 = residue(out)
print('garment edges: %d vertices re-blended; residue texels cleaned: %d, then %d' % (len(b_idx), n1, n2))

# 5. nipples the tank top had covered
from mh import MH
mh_ = MH(); body_i = np.where(mh_.body)[0]; nip = mh_.mask('nipple')[body_i]
for sx in (1, -1):
    c = V[nip & (V[:, 0] * sx > 0)].mean(0); d = np.linalg.norm(pos - c, axis=2)
    a = np.clip(1 - d / 0.011, 0, 1)[..., None] ** 1.5 * cov[..., None]                 # soft 11 mm disc
    out = out * (1 - 0.55 * a) + to_skin(np.full(out.shape[:2], mid[2])) * np.array([1.0, 0.86, 0.84], np.float32) * (0.55 * a)
out = fill(out, cov)

# 6. eyebrows: lift the painted brows out and fill the skin beneath
brow_zone = (z > 1.712) & (z < 1.748) & (np.abs(x_) > 0.006) & (np.abs(x_) < 0.064) & (y_ < -0.045) & cov
brow = brow_zone & (L < 0.52) & (col[..., 0] - col[..., 2] < 0.40)
brow = cv2.morphologyEx(brow.astype(np.uint8), cv2.MORPH_OPEN, ell(2)); grow = cv2.dilate(brow, ell(13))
soft = cv2.GaussianBlur(grow.astype(np.float32), (0, 0), 2 * PX)[..., None]
out = out * (1 - soft) + inpaint(out, grow, 7) * soft
print('eyebrow texels lifted: %d' % brow.sum())

# 7. grade to the palette. Meshy's skin is a saturated salmon with a nearly clipped red channel; the game
#    tints skin by (swatch / "skin-natural"), so that paint made every tone redder than its swatch and dark
#    tones harsh. Hue and chroma are moved onto "skin-natural" in Lab (lightness and all shading are kept),
#    so "Natural" shows the palette's own natural skin and every other tone lands on its swatch.
lab = cv2.cvtColor(np.ascontiguousarray(out, np.float32), cv2.COLOR_RGB2Lab); ref = cv2.cvtColor(NATURAL.reshape(1, 1, 3), cv2.COLOR_RGB2Lab)[0, 0]
sk = cov
mean = lab[sk].mean(0, dtype=np.float64); k = np.hypot(ref[1], ref[2]) / np.hypot(mean[1], mean[2])
lab[..., 0] = np.clip(lab[..., 0] + (ref[0] - mean[0]), 0, 100)
lab[..., 1] = ref[1] + k * (lab[..., 1] - mean[1]); lab[..., 2] = ref[2] + k * (lab[..., 2] - mean[2])
graded = np.clip(cv2.cvtColor(lab, cv2.COLOR_Lab2RGB), 0, 1)
print('skin average %s -> %s (palette natural %s); chroma x%.2f' % (out[sk].mean(0, dtype=np.float64).round(3), graded[sk].mean(0, dtype=np.float64).round(3), NATURAL.round(3), k))
out = fill(graded, cov)
Image.fromarray((np.clip(out, 0, 1) * 255 + 0.5).astype(np.uint8)).save(KIT + 'work/tex_skin.png')
Image.fromarray((grow * 255).astype(np.uint8)).save(KIT + 'qa/mask_brow.png'); Image.fromarray((tank * 255).astype(np.uint8)).save(KIT + 'qa/mask_tank.png')
