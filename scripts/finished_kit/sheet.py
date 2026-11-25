"""python sheet.py OUT.png COLS HEIGHT img1 img2 ...  (contact sheet, each image scaled to HEIGHT)"""
import sys
from PIL import Image
out, cols, h = sys.argv[1], int(sys.argv[2]), int(sys.argv[3]); ims = []
for p in sys.argv[4:]:
    im = Image.open(p).convert('RGB'); ims.append(im.resize((round(im.width * h / im.height), h), Image.LANCZOS))
rows = [ims[i:i + cols] for i in range(0, len(ims), cols)]
W = max(sum(i.width for i in r) for r in rows); S = Image.new('RGB', (W, h * len(rows)), (40, 40, 44))
for y, r in enumerate(rows):
    x = 0
    for im in r: S.paste(im, (x, y * h)); x += im.width
S.save(out); print(out, S.size)
