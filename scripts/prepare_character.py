#!/usr/bin/env python3
"""Prepare the player's local character export; never modify source GLBs."""

import json
import os
import shutil
from pathlib import Path
import struct
import sys

sys.dont_write_bytecode = True
import character_kits
import quaternius_character


DIRECTORY = Path('web/public/assets')
# Quaternius is enabled by default. 0 forces procedural; 1 also permits the
# historical local-only BaseFemale/BaseMale exports if Quaternius is absent.
ENV = 'ARENA_RIGGED_CHARACTER'
# The first installed source is the player model. IDs match character::Profile.
CHARACTERS = (
    ('base-female', Path('assets/BaseFemale.glb'), 'BaseFemale'),
    ('base-male', Path('assets/BaseMale.glb'), 'BaseMale'),
)
# Matches character_animation::Clip. Order is explicit, not Blender export order.
CLIPS = ('Idle', 'Move_Forward', 'Move_Backward', 'Strafe_Left',
         'Strafe_Right', 'Jump', 'Attack', 'Cast')


def character_glb(source, name='BaseMale'):
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
    roots = [i for i, node in enumerate(nodes) if node.get('name') == f'{name}_Rig']
    if len(roots) != 1:
        raise ValueError(f'Expected exactly one {name}_Rig')
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
        raise ValueError(f'Expected exactly the eight named {name} clips')
    for animation in animations:
        channels = animation['channels']
        if not channels or any(c['target'].get('node') not in reachable for c in channels):
            raise ValueError('Animation target outside character hierarchy')
    # The simulation already supplies jump height. Retain pelvis/limb posing,
    # but do not apply the authored root jump a second time.
    jump = named['Jump']
    jump['channels'] = [c for c in jump['channels']
                        if not (c['target']['node'] == root_bones[0] and
                                c['target']['path'] == 'translation')]
    nodes[root_bones[0]]['translation'] = [0, 0, 0]
    document['animations'] = [named[name] for name in CLIPS]
    # Keep node/skin/accessor indices and all binary mesh/animation data stable.
    # Only the character hierarchy is reachable from the sole runtime scene.
    document['scenes'] = [{'name': f'{name} | Character only', 'nodes': roots}]
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


def model_path(identifier):
    return f'characters/{identifier}.glb'


def prepare(characters=CHARACTERS, directory=DIRECTORY, enabled=None):
    setting = os.environ.get(ENV)
    if enabled is None:
        enabled = setting != '0'
    (directory / 'characters').mkdir(parents=True, exist_ok=True)
    quaternius = (enabled and characters is CHARACTERS
                  and quaternius_character.MODEL.is_file()
                  and quaternius_character.LIBRARY.is_file())
    # Do not silently publish the legacy, license-unconfirmed character sources.
    legacy = enabled and (characters is not CHARACTERS or setting == '1')
    chosen = next((c for c in characters if c[1].is_file()), None) if legacy else None
    if quaternius:
        chosen = (character_kits.IDENTIFIER, None, 'Quaternius customizable')
    for identifier, _, _ in characters:
        if chosen is None or identifier != chosen[0]:
            (directory / model_path(identifier)).unlink(missing_ok=True)
    # The single dressed female was replaced by customizable kits.
    (directory / model_path(quaternius_character.IDENTIFIER)).unlink(missing_ok=True)
    kits = []
    if quaternius:
        kits = character_kits.prepare(directory)
        if not kits:
            raise ValueError('Quaternius sources are installed but no kit could be built')
    else:
        shutil.rmtree(directory / character_kits.DIRECTORY, ignore_errors=True)
    if chosen and not quaternius:
        identifier, source, name = chosen
        (directory / model_path(identifier)).write_bytes(character_glb(source.read_bytes(), name))
    manifest = {'available': chosen is not None, 'model': chosen and chosen[0]}
    if kits:
        manifest['kits'] = kits
    (directory / 'character.json').write_text(json.dumps(manifest) + '\n')
    print('Character asset:', f'{len(kits)} Quaternius kits ({", ".join(k["id"] for k in kits)})'
          if quaternius else f'{chosen[2]} rig and 8 clips' if chosen
          else 'procedural character (no character source installed)' if enabled
          else f'procedural character (rigged models disabled; unset {ENV})')


if __name__ == '__main__':
    prepare()
