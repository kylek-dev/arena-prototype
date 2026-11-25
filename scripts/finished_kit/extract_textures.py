"""Extract the embedded textures of the two raw Meshy GLBs into src/ (read-only on the downloads)."""
import json, os, struct, sys, io
from PIL import Image
ART = os.environ.get('PITBORN_ART', os.path.expanduser('~/pitborn-art'))
D = ART + '/character-sheets/human/male/'
OUT = ART + '/human-male-kit/src/'
for name, tag in (('Meshy_AI_male_human.glb', 'low'), ('Meshy_AI_male_human_detailed.glb', 'dense')):
    f = open(D + name, 'rb'); f.read(12); clen, _ = struct.unpack('<II', f.read(8)); g = json.loads(f.read(clen)); f.read(8); bin0 = f.tell()
    m = g['materials'][0]; pbr = m['pbrMetallicRoughness']
    role = {pbr['baseColorTexture']['index']: 'basecolor', m['normalTexture']['index']: 'normal', pbr['metallicRoughnessTexture']['index']: 'orm'}
    for ti, t in enumerate(g['textures']):
        bv = g['bufferViews'][g['images'][t['source']]['bufferView']]; f.seek(bin0 + bv.get('byteOffset', 0)); b = f.read(bv['byteLength'])
        im = Image.open(io.BytesIO(b)); p = f'{OUT}{tag}_{role[ti]}.png'; im.save(p); print(p, im.size, im.mode)
        if role[ti] == 'basecolor':
            im.convert('RGB').resize((1024, 1024), Image.LANCZOS).save(f'{OUT}{tag}_basecolor_preview.png')
