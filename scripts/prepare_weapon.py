#!/usr/bin/env python3
"""Prepare the original Blender export for the game without modifying its source."""

import json
from pathlib import Path
import struct


SOURCE = Path('assets/Verdant Eclipse.glb')
TARGET = Path('web/public/assets/weapons/verdant-eclipse.glb')
SCENE = 'VERDANT ECLIPSE | Legendary Warglaive'


def weapon_glb(source):
    magic, version, size = struct.unpack_from('<III', source)
    if magic != 0x46546C67 or version != 2 or size != len(source):
        raise ValueError('Expected a complete GLB 2.0 export')
    json_size, kind = struct.unpack_from('<II', source, 12)
    if kind != 0x4E4F534A:
        raise ValueError('Expected the first GLB chunk to contain JSON')
    document = json.loads(source[20:20 + json_size])
    scenes = [scene for scene in document['scenes'] if scene.get('name') == SCENE]
    if len(scenes) != 1:
        raise ValueError('Expected exactly one named Verdant Eclipse scene')
    nodes = document['nodes']
    # This export has flat mesh roots. Reject hierarchy changes rather than
    # accidentally copying a studio collection into the arena.
    roots = []
    for index in scenes[0]['nodes']:
        node = nodes[index]
        if node.get('name') == 'Backdrop' or 'camera' in node or 'KHR_lights_punctual' in node.get('extensions', {}):
            continue
        if 'children' in node or 'mesh' not in node:
            raise ValueError('Weapon export must contain flat mesh roots')
        roots.append(index)
    if not roots or not any(nodes[i].get('name') == 'Central handgrip' for i in roots):
        raise ValueError('Weapon export is missing its handgrip/geometry')
    # Keep mesh/accessor/buffer indices stable; unused mesh data is harmless.
    # Only the reachable weapon nodes enter the game scene.
    document['nodes'] = [nodes[i] for i in roots]
    document['scenes'] = [{'name': 'Verdant Eclipse | Weapon only', 'nodes': list(range(len(roots)))}]
    document['scene'] = 0
    payload = json.dumps(document, separators=(',', ':')).encode()
    payload += b' ' * (-len(payload) % 4)
    tail = source[20 + json_size:]
    return struct.pack('<III', magic, version, 20 + len(payload) + len(tail)) + struct.pack('<II', len(payload), kind) + payload + tail


def main():
    TARGET.parent.mkdir(parents=True, exist_ok=True)
    TARGET.write_bytes(weapon_glb(SOURCE.read_bytes()))
    print(f'Weapon asset: {TARGET} (weapon-only scene; source unchanged)')
    target = TARGET.with_name('aetherthorn.glb')
    target.write_bytes(staff_glb(Path('assets/Aetherthorn.glb').read_bytes()))
    print(f'Weapon asset: {target} (staff hierarchy only; source unchanged)')


def staff_glb(source):
    magic, version, size = struct.unpack_from('<III', source)
    if magic != 0x46546C67 or version != 2 or size != len(source):
        raise ValueError('Expected a complete GLB 2.0 export')
    json_size, kind = struct.unpack_from('<II', source, 12)
    if kind != 0x4E4F534A:
        raise ValueError('Expected a JSON first chunk')
    document = json.loads(source[20:20 + json_size])
    nodes = document['nodes']
    roots = [i for i, node in enumerate(nodes) if node.get('name') == 'Crystal Staff']
    if len(roots) != 1:
        raise ValueError('Expected exactly one Crystal Staff root')
    keep = []
    def visit(index):
        if index in keep:
            raise ValueError('Staff must be a tree')
        node = nodes[index]
        if 'camera' in node or 'KHR_lights_punctual' in node.get('extensions', {}):
            raise ValueError('Staff hierarchy must not contain studio lights/cameras')
        if any(word in node.get('name', '').lower() for word in ('plinth', 'backdrop', 'cube')):
            raise ValueError('Staff hierarchy contains presentation geometry')
        keep.append(index)
        for child in node.get('children', []):
            visit(child)
    visit(roots[0])
    names = {nodes[i].get('name') for i in keep}
    if not {'Primary six sided power crystal', 'Tapered lacquer shaft', 'Long wrapped handgrip'} <= names:
        raise ValueError('Staff is missing crystal, shaft or handgrip')
    remap = {old: new for new, old in enumerate(keep)}
    filtered = []
    for index in keep:
        node = dict(nodes[index])
        if 'children' in node:
            node['children'] = [remap[i] for i in node['children']]
        filtered.append(node)
    document['nodes'] = filtered
    document['scenes'] = [{'name': 'Aetherthorn | Weapon only', 'nodes': [0]}]
    document['scene'] = 0
    payload = json.dumps(document, separators=(',', ':')).encode()
    payload += b' ' * (-len(payload) % 4)
    tail = source[20 + json_size:]
    return struct.pack('<III', magic, version, 20 + len(payload) + len(tail)) + struct.pack('<II', len(payload), kind) + payload + tail


if __name__ == '__main__':
    main()
