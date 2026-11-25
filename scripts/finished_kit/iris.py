"""Tintable iris texture (near-neutral grey; the game multiplies it by the chosen eye colour) and the eye
white sheet (Meshy's painted eye white on the left three quarters, a plain white swatch for catchlights on
the right).  -> work/tex_eye_iris.png, work/tex_eye_white.png"""
import numpy as np
from PIL import Image
from kitlib import KIT

N = 256; y, x = np.mgrid[0:N, 0:N]; u = (x + 0.5) / N * 2 - 1; v = 1 - (y + 0.5) / N * 2
r = np.hypot(u, v); th = np.arctan2(v, u); rng = np.random.default_rng(7)
def ring_noise(freqs, amp):                              # smooth angular streaks ("fibres"), painterly not photographic
    out = np.zeros_like(r)
    for f, a in zip(freqs, amp):
        ph = rng.uniform(0, 2 * np.pi, 3); out += a * np.sin(f * th + ph[0] + 1.5 * np.sin(3 * r + ph[1]))
    return out
sm = lambda a, b, t: np.clip((t - a) / (b - a), 0, 1) ** 2 * (3 - 2 * np.clip((t - a) / (b - a), 0, 1))
val = 0.66 + 0.10 * sm(0.34, 0.62, r) - 0.10 * sm(0.62, 0.98, r)          # lighter mid ring, darker towards the rim
val += 0.045 * ring_noise((9, 17, 31), (1.0, 0.7, 0.4)) * sm(0.34, 0.5, r)
val *= 1 - 0.55 * sm(0.80, 1.0, r)                                        # limbal ring
val *= 1 - 0.30 * sm(0.15, 0.95, v)                                       # soft shadow under the upper lid
pupil = 1 - sm(0.30, 0.37, r); val = val * (1 - pupil) + 0.03 * pupil
img = np.repeat(np.clip(val, 0, 1)[..., None], 3, 2)
Image.fromarray((img * 255 + 0.5).astype(np.uint8)).save(KIT + 'work/tex_eye_iris.png')
white = np.asarray(Image.open(KIT + 'work/eye_white.png').convert('RGB'))                # 256 x 128 from eyes.py
sheet = np.full((128, 256, 3), 255, np.uint8); sheet[:, :192] = np.asarray(Image.fromarray(white).resize((192, 128), Image.LANCZOS))
Image.fromarray(sheet).save(KIT + 'work/tex_eye_white.png')
print('iris mean grey %.3f' % img[r < 1].mean())
