"""Final texture set from the 4096 masters (spec in docs/ART-PRODUCTION.md): body 2048 colour + 1024 normal,
head 1024 + 1024 on its own material, trunks 1024 + 1024 (Meshy's cloth paint).  -> tex/*.png"""
import os
import numpy as np, cv2
from PIL import Image
from kitlib import KIT
import layout

L = layout.compute(); os.makedirs(KIT + 'tex', exist_ok=True)
load = lambda n: np.asarray(Image.open(KIT + 'work/' + n).convert('RGB'), np.float32) / 255
skin, cloth, nrm = load('tex_skin.png'), load('bake_dense.png'), load('bake_normal.png'); R = skin.shape[0]
# The game tints skin by (palette swatch / skinAtlas). skin_paint.py grades the paint onto skinAtlas, so the
# "Natural" tone (tint 1) shows it as is and every other tone lands on its swatch.
cov = np.load(KIT + 'work/bake_aux.npz')['mask']
print('skin average %s (skinAtlas is [0.86 0.62 0.48])' % skin[cov].mean(0, dtype=np.float64).round(3))
def save(img, name, normal=False):
    if normal:
        v = img * 2 - 1; v /= np.maximum(np.linalg.norm(v, axis=2, keepdims=True), 1e-6); img = v * 0.5 + 0.5
    Image.fromarray((np.clip(img, 0, 1) * 255 + 0.5).astype(np.uint8)).save(KIT + 'tex/' + name, optimize=True)
    print('%-22s %s  %.2f MB' % (name, img.shape[:2], os.path.getsize(KIT + 'tex/' + name) / 1e6))
def crop(img, rect, size):
    x0, x1 = rect[0] * R, rect[2] * R; y0, y1 = (1 - rect[3]) * R, (1 - rect[1]) * R       # image rows run down
    M = np.array([[(x1 - x0) / size, 0, x0], [0, (y1 - y0) / size, y0]], np.float32)
    return cv2.warpAffine(img, M, (size, size), flags=cv2.INTER_AREA | cv2.WARP_INVERSE_MAP, borderMode=cv2.BORDER_REPLICATE)
rs = lambda img, size: cv2.resize(img, (size, size), interpolation=cv2.INTER_AREA)
save(rs(skin, 2048), 'skin_body.png'); save(rs(nrm, 1024), 'skin_body_n.png', True)
save(crop(skin, L['crop_head'], 1024), 'skin_head.png'); save(crop(nrm, L['crop_head'], 1024), 'skin_head_n.png', True)
# The trunks are cloth everywhere, with a plain waistband and leg hems on the piece's own straight edges.
# Meshy's shorts paint is kept 5.5 cm in from each edge; nearer than that (the old shirt hem, Meshy's own
# wandering leg hems, skin) the nearest shorts paint is carried over and softened, then the bands are drawn.
from scipy import ndimage
aux = np.load(KIT + 'work/bake_aux.npz'); pos = aux['pos'].astype(np.float32); PX = R / 2048
rg = np.load(KIT + 'work/regions.npz'); Vf = L['V']
def above(loop):                                              # height of every texel above a leg opening's plane
    P_ = Vf[rg[loop]]; c = P_.mean(0); n = np.linalg.svd(P_ - c)[2][2]; n = n * np.sign(n[2]); return (pos - c.astype(np.float32)) @ n.astype(np.float32)
edge = np.minimum(Vf[rg['loop_waist'], 2].mean() - pos[..., 2], np.where(pos[..., 0] > 0, above('loop_trunks_hem_L'), above('loop_trunks_hem_R')))   # metres in from the nearest edge
step = lambda x, a, b: np.clip((x - a) / (b - a), 0, 1) ** 2 * (3 - 2 * np.clip((x - a) / (b - a), 0, 1))
shorts = (aux['cloth'] > 0.9) & cov & (edge > 0.035)
idx = ndimage.distance_transform_edt(~shorts, return_distances=False, return_indices=True)
near = cloth[idx[0], idx[1]]; soft = cv2.GaussianBlur(near, (0, 0), 10 * PX)
keep = (step(edge, 0.035, 0.055) * shorts)[..., None]
trunks_paint = soft * (1 - keep) + near * keep
BAND = np.where(edge < 0, 0, np.where(Vf[rg['loop_waist'], 2].mean() - pos[..., 2] < 0.06, 0.024, 0.018))          # waistband 24 mm, leg hems 18 mm
inband = 1 - step(edge, BAND - 0.0012, BAND + 0.0012); seam = np.exp(-((edge - BAND) / 0.0014) ** 2)
trunks_paint = trunks_paint * (1 + 0.10 * inband - 0.38 * seam)[..., None]
save(crop(trunks_paint, L['crop_trunks'], 1024), 'trunks.png'); save(crop(nrm, L['crop_trunks'], 1024), 'trunks_n.png', True)
for n in ('tex_eye_white.png', 'tex_eye_iris.png'):
    Image.open(KIT + 'work/' + n).save(KIT + 'tex/' + n.replace('tex_', ''), optimize=True)
print('head texture covers %d faces (regions: %s)' % (L['use_head_tex'].sum(), sorted({L['names'][r] for r in L['region'][L['use_head_tex']]})))
print('head crop %.0f px of the master, trunks crop %.0f px' % ((L['crop_head'][2] - L['crop_head'][0]) * R, (L['crop_trunks'][2] - L['crop_trunks'][0]) * R))
