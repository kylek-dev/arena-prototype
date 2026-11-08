#!/usr/bin/env python3
"""Prepare the local BaseMale export; never modify the user's source GLB."""

import json
from pathlib import Path
import struct


SOURCE = Path('assets/BaseMale.glb')
DIRECTORY = Path('web/public/assets')
MODEL = 'characters/base-male.glb'
# Matches character_animation::Clip. Order is explicit, not Blender export order.
CLIPS = ('Idle', 'Move_Forward', 'Move_Backward', 'Strafe_Left',
         'Strafe_Right', 'Jump', 'Attack', 'Cast')


def character_glb(source):
    if len(source) < 20:
        raise ValueError('Expected a complete GLB 2.0 export')
    magic, version, size = struct.unpack_from('<III', source)
    if magic != 0x46546C67 or version != 2 or size != len(source):
        raise ValueError('Expected a complete GLB 2.0 export')
    json_size, kind = struct.unpack_from('<II', source, 12)
    if kind != 0x4E4F534A or 20 + json_size > len(source):
        raise ValueError('Expected a JSON first chunk')
    document = json.loads(source[20:20 + json_size])
    nodes = document['nodes']
    roots = [i for i, node in enumerate(nodes) if node.get('name') == 'BaseMale_Rig']
    if len(roots) != 1:
        raise ValueError('Expected exactly one BaseMale_Rig')
    reachable = set()

    def visit(index):
        if index in reachable:
            raise ValueError('Character hierarchy must be a tree')
        node = nodes[index]
        if ('camera' in node or
                'KHR_lights_punctual' in node.get('extensions', {}) or
                any(word in node.get('name', '').lower()
                    for word in ('studio', 'backdrop', 'cube'))):
            raise ValueError('Character hierarchy contains presentation objects')
        reachable.add(index)
        for child in node.get('children', []):
            visit(child)

    visit(roots[0])
    root_bones = [i for i in reachable if nodes[i].get('name') == 'root']
    if len(root_bones) != 1:
        raise ValueError('Expected exactly one root bone')
    skin_nodes = [nodes[i] for i in reachable if 'mesh' in nodes[i]]
    if not skin_nodes or any('skin' not in node for node in skin_nodes):
        raise ValueError('Expected skinned character meshes')
    for node in skin_nodes:
        skin = document['skins'][node['skin']]
        if not set(skin['joints']) <= reachable:
            raise ValueError('Skin joint outside character hierarchy')
    animations = document.get('animations', [])
    named = {a.get('name'): a for a in animations}
    if len(named) != len(animations) or set(named) != set(CLIPS):
        raise ValueError('Expected exactly the eight named BaseMale clips')
    for animation in animations:
        channels = animation['channels']
        if not channels or any(c['target'].get('node') not in reachable for c in channels):
            raise ValueError('Animation target outside character hierarchy')
    # The simulation already supplies jump height. Retain pelvis/limb posing,
    # but do not apply the authored 0.52 m root jump a second time.
    jump = named['Jump']
    jump['channels'] = [c for c in jump['channels']
                        if not (c['target']['node'] == root_bones[0] and
                                c['target']['path'] == 'translation')]
    nodes[root_bones[0]]['translation'] = [0, 0, 0]
    document['animations'] = [named[name] for name in CLIPS]
    # Keep node/skin/accessor indices and all binary mesh/animation data stable.
    # Only the character hierarchy is reachable from the sole runtime scene.
    document['scenes'] = [{'name': 'BaseMale | Character only', 'nodes': roots}]
    document['scene'] = 0
    # Runtime extras are unnecessary and include local authoring paths/server state.
    def remove_extras(value):
        if isinstance(value, dict):
            value.pop('extras', None)
            for child in value.values():
                remove_extras(child)
        elif isinstance(value, list):
            for child in value:
                remove_extras(child)
    remove_extras(document)
    payload = json.dumps(document, separators=(',', ':')).encode()
    payload += b' ' * (-len(payload) % 4)
    tail = source[20 + json_size:]
    return (struct.pack('<III', magic, version, 20 + len(payload) + len(tail)) +
            struct.pack('<II', len(payload), kind) + payload + tail)


def prepare(source=SOURCE, directory=DIRECTORY):
    target = directory / MODEL
    target.parent.mkdir(parents=True, exist_ok=True)
    available = source.is_file()
    if available:
        target.write_bytes(character_glb(source.read_bytes()))
    else:
        target.unlink(missing_ok=True)
    (directory / 'character.json').write_text(json.dumps({'available': available}) + '\n')
    print('Character asset:', 'BaseMale rig and eight clips' if available
          else 'procedural fallback (BaseMale not installed)')


if __name__ == '__main__':
    prepare()
