"""Measure the painted left eye on Meshy's texture (front orthographic render at a known scale):
opening outline, corners and iris disc, in work-space millimetres.  -> work/eye_outline.json + QA image"""
import json
import numpy as np
import cv2
from kitlib import KIT

MM_PX = 0.07044 * (1.81 / 1.70); CX, CZ = 800, 350; Z0 = 1.589 * (1.81 / 1.70)       # render scale and centre (see ref_renders.py "eyes")
img = cv2.imread(KIT + 'qa/ref/eyes_dense.png'); h, w = img.shape[:2]
rgb = img[..., ::-1].astype(np.float32) / 255; mx, mn = rgb.max(2), rgb.min(2); sat = (mx - mn) / np.maximum(mx, 1e-6); lum = rgb @ np.array([0.2126, 0.7152, 0.0722])
rb = rgb[..., 0] - rgb[..., 2]
eye = ((rb < 0.385) & (lum > 0.5)) | (rb < 0.22)                                      # warm-cream eye white, or neutral dark iris / pupil (the warm dark lash line is excluded)
eye[:, :w // 2] = False; eye[int(h * 0.45):] = False                                 # character's left eye is on the image's right; skip the nostrils
eye = cv2.morphologyEx(eye.astype(np.uint8), cv2.MORPH_CLOSE, cv2.getStructuringElement(cv2.MORPH_ELLIPSE, (15, 15)))
eye = cv2.morphologyEx(eye, cv2.MORPH_OPEN, cv2.getStructuringElement(cv2.MORPH_ELLIPSE, (7, 7)))
nlab, lab, stats, cent = cv2.connectedComponentsWithStats(eye); k = 1 + np.argmax(stats[1:, cv2.CC_STAT_AREA]); blob = lab == k
xs = np.where(blob.any(0))[0]; top = np.array([np.where(blob[:, x])[0].min() for x in xs]); bot = np.array([np.where(blob[:, x])[0].max() for x in xs])
dark = (rb < 0.22) & (lum < 0.45) & blob
dark = cv2.morphologyEx(dark.astype(np.uint8), cv2.MORPH_OPEN, cv2.getStructuringElement(cv2.MORPH_ELLIPSE, (25, 25)))   # drop the lash line, keep the iris disc
ys_, xs_ = np.where(dark > 0); icx, icy = xs_.mean(), ys_.mean(); ir = (xs_.max() - xs_.min()) / 2
to_mm = lambda px, py: ((px - CX) * MM_PX, Z0 * 1e3 + (CZ - py) * MM_PX)
# The lid margins are read off the render by eye (control points in crop pixels, x from the image centre);
# automatic thresholding cannot separate the cream eye white from lid skin reliably at the corners.
from scipy.interpolate import PchipInterpolator
UP = [(240, 108), (300, 72), (350, 50), (400, 43), (450, 52), (500, 75), (540, 92), (560, 98)]
LO = [(240, 108), (270, 125), (320, 145), (380, 160), (430, 158), (480, 145), (530, 118), (560, 98)]
xs = np.arange(240, 561) + w // 2
up = PchipInterpolator([p[0] + w // 2 for p in UP], [p[1] for p in UP])(xs); lo = PchipInterpolator([p[0] + w // 2 for p in LO], [p[1] for p in LO])(xs)
out = dict(mm_per_px=MM_PX,
           upper=[to_mm(x, y) for x, y in zip(xs, up)], lower=[to_mm(x, y) for x, y in zip(xs, lo)],
           inner=to_mm(xs[0], up[0]), outer=to_mm(xs[-1], up[-1]), iris_centre=to_mm(icx, icy), iris_radius=ir * MM_PX)
print('eye opening: width %.1f mm, height %.1f mm' % ((xs[-1] - xs[0]) * MM_PX, (lo - up).max() * MM_PX))
print('inner corner (x,z mm)', np.round(out['inner'], 2), 'outer', np.round(out['outer'], 2))
print('iris centre', np.round(out['iris_centre'], 2), 'radius %.2f mm' % out['iris_radius'])
json.dump(out, open(KIT + 'work/eye_outline.json', 'w'))
dbg = img.copy()
for x, a, b in zip(xs, up, lo): dbg[int(round(a)), x] = (0, 255, 0); dbg[int(round(b)), x] = (0, 255, 0)
cv2.circle(dbg, (int(icx), int(icy)), int(ir), (255, 255, 0), 1)
cv2.imwrite(KIT + 'qa/eye_outline.png', dbg[:int(h * 0.4), w // 2:])
