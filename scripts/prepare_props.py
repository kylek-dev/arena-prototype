#!/usr/bin/env python3
"""Prepare the CC0 Quaternius Fantasy Props MegaKit for map placement.

Copies every glTF prop (JSON + BIN) into web/public/assets/props, capping
material metalness and darkening plain metal (see METALNESS, IRON), and shrinks the shared trim-sheet textures with the pinned `character-textures`
tool (colour 1024 px, normal/ORM 512 px). The props keep referencing the
shared PNGs by relative URI, so the browser decodes each trim sheet once no
matter how many props a map uses. Sources are never modified.

`assets/props/catalog.json` (tracked; the Rust map validator embeds it) lists
every prop id with a category and its rest bounds in metres. This script
rewrites it deterministically; `--check` fails if it is stale instead.
"""

import argparse
import json
import math
from pathlib import Path
import shutil
import subprocess
import sys

ROOT = Path(__file__).resolve().parent.parent
SOURCE = ROOT / 'assets/quaternius/props/Exports/glTF'
TARGET = ROOT / 'web/public/assets/props'
CATALOG = ROOT / 'assets/props/catalog.json'
COLOR_EDGE = 1024
DETAIL_EDGE = 512
# The metal trim sheet marks nearly every texel fully metallic. The pack's
# previews rely on reflections the arena has no environment map for, so full
# metal renders as flat grey/white with no colour. glTF multiplies this factor
# into the texture's metalness: metal keeps a sheen and shows its painted and
# vertex colours as diffuse light.
METALNESS = 0.3
# Plain (untinted) metal: the trim sheet is a light lilac grey that the pack's
# renders darken through dark reflections. Multiply it (linear) to a warm dark
# iron so anvils, crates' bands and chains read as iron. Vertex-tinted metal
# (coins, keys) keeps its colours.
IRON = [0.4, 0.42, 0.36, 1.0]
PLAIN_METAL = 'MI_Trim_Metal'

# First matching keyword wins; ids are matched case-insensitively.
CATEGORIES = (
    ('Small items', ('potion',)),  # before 'pot'
    ('Weapons & armour', ('axe', 'sword', 'shield', 'weaponstand', 'pickaxe', 'dummy',
                          'anvil', 'whetstone')),
    ('Banners & lights', ('banner', 'candle', 'chandelier', 'lantern', 'torch')),
    ('Furniture', ('bed', 'bench', 'bookcase', 'bookstand', 'cabinet', 'chair', 'nightstand',
                   'shelf', 'stool', 'table_large', 'workbench', 'stall', 'peg_rack')),
    ('Containers', ('barrel', 'bag', 'bucket', 'cage', 'cauldron', 'chest', 'crate', 'pot',
                    'pouch', 'vase')),
    ('Small items', ('',)),
)


def category(name):
    lower = name.lower()
    for label, words in CATEGORIES:
        if any(word in lower for word in words):
            return label
    raise AssertionError('the last category matches everything')


def label(name):
    return name.replace('_', ' ')


def multiply(a, b):
    return [[sum(a[r][k] * b[k][c] for k in range(4)) for c in range(4)] for r in range(4)]


def node_matrix(node):
    if 'matrix' in node:
        m = node['matrix']  # column-major
        return [[m[c * 4 + r] for c in range(4)] for r in range(4)]
    tx, ty, tz = node.get('translation', (0, 0, 0))
    x, y, z, w = node.get('rotation', (0, 0, 0, 1))
    sx, sy, sz = node.get('scale', (1, 1, 1))
    rotation = [
        [1 - 2 * (y * y + z * z), 2 * (x * y - z * w), 2 * (x * z + y * w)],
        [2 * (x * y + z * w), 1 - 2 * (x * x + z * z), 2 * (y * z - x * w)],
        [2 * (x * z - y * w), 2 * (y * z + x * w), 1 - 2 * (x * x + y * y)],
    ]
    scale = (sx, sy, sz)
    return [
        [rotation[r][0] * scale[0], rotation[r][1] * scale[1], rotation[r][2] * scale[2],
         (tx, ty, tz)[r]] for r in range(3)
    ] + [[0, 0, 0, 1]]


def bounds(document):
    """World-space bounds of the default scene from accessor min/max corners."""
    low = [math.inf] * 3
    high = [-math.inf] * 3
    identity = [[float(r == c) for c in range(4)] for r in range(4)]
    scene = document['scenes'][document.get('scene', 0)]

    def visit(index, parent):
        node = document['nodes'][index]
        world = multiply(parent, node_matrix(node))
        if 'mesh' in node:
            for primitive in document['meshes'][node['mesh']]['primitives']:
                accessor = document['accessors'][primitive['attributes']['POSITION']]
                lo, hi = accessor['min'], accessor['max']
                for corner in range(8):
                    p = [hi[a] if corner >> a & 1 else lo[a] for a in range(3)] + [1]
                    q = [sum(world[r][k] * p[k] for k in range(4)) for r in range(3)]
                    for a in range(3):
                        low[a] = min(low[a], q[a])
                        high[a] = max(high[a], q[a])
        for child in node.get('children', []):
            visit(child, world)

    for root in scene['nodes']:
        visit(root, identity)
    if not math.isfinite(low[0]):
        raise ValueError('prop has no geometry')
    return [round(v, 3) + 0.0 for v in low], [round(v, 3) + 0.0 for v in high]


def catalog(source=SOURCE):
    props = []
    for path in sorted(source.glob('*.gltf')):
        document = json.loads(path.read_text())
        low, high = bounds(document)
        props.append({'id': path.stem, 'label': label(path.stem), 'category': category(path.stem),
                      'min': low, 'max': high})
    if not props:
        raise SystemExit(f'No props found in {source}')
    return {'source': 'Quaternius Fantasy Props MegaKit (CC0 1.0)', 'props': props}


def catalog_text(source=SOURCE):
    data = catalog(source)
    lines = ['{', f'  "source": {json.dumps(data["source"])},', '  "props": [']
    lines += [f'    {json.dumps(p, ensure_ascii=False)}' + (',' if i + 1 < len(data['props']) else '')
              for i, p in enumerate(data['props'])]
    return '\n'.join(lines + ['  ]', '}']) + '\n'


def runtime_gltf(document):
    """The prop's glTF with metalness capped and plain metal darkened."""
    for material in document.get('materials', []):
        pbr = material.setdefault('pbrMetallicRoughness', {})
        pbr['metallicFactor'] = min(pbr.get('metallicFactor', 1.0), METALNESS)
        if material.get('name') == PLAIN_METAL and 'baseColorFactor' not in pbr:
            pbr['baseColorFactor'] = IRON
    return document


def texture_kind(name):
    if name.endswith('_Normal.png'):
        return 'normal', DETAIL_EDGE
    if name.endswith('_ORM.png'):
        return 'data', DETAIL_EDGE
    if name.endswith('_BaseColor.png'):
        return 'color', COLOR_EDGE
    return 'color', DETAIL_EDGE  # Page noise: a faint overlay


def shrink(source, target):
    kind, edge = texture_kind(source.name)
    result = subprocess.run(
        ['cargo', 'run', '--quiet', '--locked', '--release', '-p', 'arena-textures',
         '--bin', 'character-textures', '--', kind, str(edge)],
        input=source.read_bytes(), capture_output=True, check=False, cwd=ROOT)
    if result.returncode:
        raise SystemExit(f'{source.name}: ' + result.stderr.decode(errors='replace'))
    target.write_bytes(result.stdout)


def fresh(target, source):
    return target.is_file() and target.stat().st_mtime >= source.stat().st_mtime


def prepare():
    TARGET.mkdir(parents=True, exist_ok=True)
    images = set()
    for path in sorted(SOURCE.glob('*.gltf')):
        document = json.loads(path.read_text())
        for buffer in document.get('buffers', []):
            shutil.copyfile(SOURCE / buffer['uri'], TARGET / buffer['uri'])
        images.update(image['uri'] for image in document.get('images', []))
        (TARGET / path.name).write_text(json.dumps(runtime_gltf(document), indent=1) + '\n')
    for name in sorted(images):
        if not fresh(TARGET / name, SOURCE / name):
            shrink(SOURCE / name, TARGET / name)
    (TARGET / 'catalog.json').write_text(CATALOG.read_text())
    shutil.copyfile(SOURCE.parent.parent / 'License_Standard.txt', TARGET / 'LICENSE-CC0.txt')
    print(f'Props: {len(list(SOURCE.glob("*.gltf")))} models, {len(images)} shared textures '
          f'in {TARGET.relative_to(ROOT)}')


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--check', action='store_true', help='fail if the catalog is stale')
    args = parser.parse_args()
    if not SOURCE.is_dir():
        raise SystemExit(f'Missing {SOURCE}')
    text = catalog_text()
    if args.check:
        if CATALOG.read_text() != text:
            raise SystemExit(f'{CATALOG} is stale; run scripts/prepare_props.py')
        return
    CATALOG.parent.mkdir(parents=True, exist_ok=True)
    if not CATALOG.is_file() or CATALOG.read_text() != text:
        CATALOG.write_text(text)
    prepare()


if __name__ == '__main__':
    sys.exit(main())
