"""Customizable Quaternius character kits. Standard library only.

One GLB per body (human/elf/orc x female/male) holds the skeleton, the shared
UAL1 clips and *every* compatible hairstyle, brow, beard and outfit piece as a
separately named node. The browser chooses which nodes are visible and which
tints/texture variants apply; nothing is re-exported per choice.

The base body is split by dominant bone into regions so outfit pieces can hide
exactly the skin they cover. Parts keep their own inverse bind matrices (a
second skin over the same joints) when their bind pose differs slightly from
the body's, so pieces authored for another proportion stay attached to the
bone they were rigged to instead of being rejected.
"""

from collections import Counter
from copy import deepcopy
from dataclasses import dataclass, field
import json
from pathlib import Path
import struct

import quaternius_character as q


ROOT = q.ROOT
BASE = ROOT / 'base/Base Characters/Godot - UE'
BASE_TEXTURES = ROOT / 'base/Base Characters/Textures'
HAIR = ROOT / 'base/Hairstyles/Rigged to Head Bone/glTF (Godot -Unreal)'
OUTFITS = ROOT / 'outfits/Exports/glTF (Godot-Unreal)/Outfits'
OUTFIT_TEXTURES = ROOT / 'outfits/Textures'
RACES = ROOT / 'races/Exports/GLB (Godot-Unreal)'
RACE_TEXTURES = ROOT / 'races/Textures'
DIRECTORY = 'characters/kits'
# character.json model id: the player is assembled from a kit at runtime.
IDENTIFIER = 'quaternius-kits'
# One scale for every kit, so bodies keep their authored relative heights.
# It matches the previous dressed-female normalization (1.9 m boot-to-buns).
SCALE = 1.9 / (1.7791516 + 0.0039672535)

HAIRSTYLES = (
    ('Hair_Buns', 'Buns'), ('Hair_Long', 'Long'), ('Hair_SimpleParted', 'Parted'),
    ('Hair_Buzzed', 'Buzzed'), ('Hair_BuzzedFemale', 'Buzzed (fitted)'),
)
BROWS = (('Eyebrows_Female', 'Fine'), ('Eyebrows_Regular', 'Full'))
BEARD = 'Hair_Beard'

# Source material -> kit material slot. Tint/texture overrides address slots.
MATERIALS = {
    'MI_Superhero_Female': 'Skin', 'MI_Superhero_Male': 'Skin',
    'MI_Regular_Female': 'Skin_Hands', 'MI_Regular_Male': 'Skin_Hands',
    'MI_Elf_Female': 'Skin', 'MI_Elf_Male': 'Skin',
    'MI_Orc_Female': 'Skin', 'MI_Orc_Male': 'Skin',
    'MI_Hair_1': 'Hair_1', 'MI_Hair_2': 'Hair_2', 'MI_FantasyHair': 'Hair_Fantasy',
    'MI_Eyes': 'Eyes', 'MI_Eyes_Green': 'Eyes', 'MI_Eyes_Brown': 'Eyes', 'MI_Eyes_Orc': 'Eyes',
    'MI_Ranger': 'Ranger', 'MI_Peasant': 'Peasant',
}

# Dominant-bone body regions. Calves split at a fixed height between the
# lowest trouser hem (0.40 m) and the highest boot top (0.56 m).
CALF_SPLIT = 0.45
REGION_BONES = {
    'Head': 'Head', 'neck_01': 'Head',
    'spine_01': 'Torso', 'spine_02': 'Torso', 'spine_03': 'Torso',
    'clavicle_l': 'Torso', 'clavicle_r': 'Torso',
    'root': 'Hips', 'pelvis': 'Hips',
    'thigh_l': 'Thighs', 'thigh_r': 'Thighs',
    'calf_l': 'Calves', 'calf_r': 'Calves',
    'foot_l': 'Feet', 'foot_r': 'Feet', 'ball_l': 'Feet', 'ball_r': 'Feet',
    'ball_leaf_l': 'Feet', 'ball_leaf_r': 'Feet',
}
REGIONS = ('Head', 'Torso', 'Hips', 'Arms', 'Thighs', 'Calves', 'Shins', 'Feet')

# Outfit pieces by neutral name: (slot, body regions the piece hides).
PIECES = {
    'Body': ('torso', ('Torso',)),
    'Belt_1': ('torso', ()), 'Belt_2': ('torso', ()),
    'Arms': ('arms', ('Arms',)), 'Bracers': ('arms', ()),
    'Legs': ('legs', ('Hips', 'Thighs', 'Calves')),
    'Feet': ('feet', ('Shins', 'Feet')),
    'Pauldrons': ('shoulders', ()),
    'Hood': ('head', ()),
}
OUTFIT_SLOTS = ('head', 'shoulders', 'torso', 'arms', 'legs', 'feet')
# Finished kits built offline (docs/ART-PRODUCTION.md, ADR 0053): one folder per kit holding
# <id>.glb and kit.json, its manifest entry. They ship as built, after the Quaternius kits.
PREBUILT = ROOT.parent / 'characters/kits'
OUTFIT_SETS = (('Ranger', 'Ranger'), ('Peasant', 'Peasant'))


def piece_name(source):
    """Female_Ranger_Acc_Pauldrons / Male_Ranger_Feet_Boots -> Ranger_Pauldrons / Ranger_Feet."""
    _, outfit, *rest = source.split('_')
    rest = '_'.join(rest).replace('.001', '')
    rest = {'Acc_Pauldron': 'Pauldrons', 'Acc_Pauldrons': 'Pauldrons',
            'Feet_Boots': 'Feet', 'Head_Hood': 'Hood', 'Arms_Bracer': 'Bracers',
            'Body_Belt_1': 'Belt_1', 'Body_Belt_2': 'Belt_2'}.get(rest, rest)
    if rest not in PIECES:
        raise ValueError(f'Unknown outfit piece: {source}')
    return f'{outfit}_{rest}'


@dataclass
class Kit:
    id: str
    ancestry: str
    body: str
    base: Path
    body_node: str
    eyes_node: str
    # Rest pose to adopt when the base's own differs from its outfits'.
    skeleton: Path | None = None
    drop: tuple = ()
    outfits: tuple = ()
    race_hair: str | None = None
    horns: str | None = None
    morph: bool = False
    skin_variants: tuple = ()
    # Optional QAL sources: built only where installed, never committed.
    optional: bool = False
    extra: dict = field(default_factory=dict)


def human(body):
    title = body.title()
    return Kit(
        id=f'human-{body}', ancestry='human', body=body,
        base=BASE / f'Superhero_{title}_FullBody.gltf',
        body_node={'female': 'Superhero_Female', 'male': 'SuperHero_Male'}[body],
        eyes_node='Eyes', drop=('Eyebrows',),
        # The male outfits, beard and brows share a bind pose that differs
        # from the male base by up to 2.4 cm in the limbs; adopt theirs.
        skeleton=OUTFITS / 'Male_Ranger.gltf' if body == 'male' else None,
        outfits=tuple(OUTFITS / f'{title}_{name}.gltf' for name, _ in OUTFIT_SETS),
        skin_variants=(
            ('dark', 'Dark', None),
            ('light', 'Light', BASE_TEXTURES / {
                'female': 'T_Superhero_Female_Light_BaseColor.png',
                'male': 'T_Superhero_Male_Ligh.png'}[body]),
        ),
    )


def race(name, body):
    title = body.title()
    stem = f'{name.title()}_{title}'
    return Kit(
        id=f'{name}-{body}', ancestry=name, body=body,
        base=RACES / f'{stem}.glb', body_node=f'{stem}_Body', eyes_node=f'{stem}_Eyes',
        race_hair=f'{stem}_Hair', horns='Elf_Male_Horns' if name == 'elf' else None,
        morph=True, optional=True,
        skin_variants=tuple(
            (str(i), f'Tone {i}', None if i == 1 else
             RACE_TEXTURES / f'Variations/T_{stem}_BaseColor_{i}.png') for i in (1, 2, 3)),
    )


KITS = (human('female'), human('male'),
        race('elf', 'female'), race('elf', 'male'),
        race('orc', 'female'), race('orc', 'male'))

# Eye colours: recoloured from the CC0 brown iris, plus the races' own eyes.
IRIS = BASE / 'T_Eye_Brown.png'
EYE_COLORS = (('hazel', 'Hazel', '8e7a3a'), ('amber', 'Amber', 'c08a2a'),
              ('green', 'Green', '4f8a4a'), ('blue', 'Blue', '3f7fc8'),
              ('grey', 'Grey', '8a949c'), ('violet', 'Violet', '7a5ab8'))
RACE_EYES = (('elf', 'Elven green', RACE_TEXTURES / 'T_Eye_Green.png'),
             ('orc', 'Orcish glow', RACE_TEXTURES / 'T_Eye_Orc.png'))
OUTFIT_VARIANTS = {
    'Ranger': (('1', 'Forest', None),
               ('3', 'Dusk', OUTFIT_TEXTURES / 'Ranger/T_Ranger_3_BaseColor.png')),
    'Peasant': (('1', 'Homespun', None),
                ('2', 'Dyed', OUTFIT_TEXTURES / 'Peasant/T_Peasant_2_BaseColor.png')),
}
FANTASY_HAIR = tuple(
    (str(i), f'Style {i}', None if i == 1 else
     RACE_TEXTURES / f'Variations/T_FantasyHair_BaseColor_{i}.png') for i in (1, 2, 3))


def available(kit):
    return kit.base.is_file() and all(p.is_file() for p in kit.outfits)


def joint_names(document):
    return [document['nodes'][i]['name'] for i in document['skins'][0]['joints']]


def check_skeleton(document, addon):
    """Same joint names, order and hierarchy. Rest poses may differ slightly."""
    if joint_names(document) != joint_names(addon):
        raise ValueError('Part joint names/order differ from character')

    def children(doc):
        joints = doc['skins'][0]['joints']
        return [[doc['nodes'][c]['name'] for c in doc['nodes'][i].get('children', [])
                 if c in joints] for i in joints]
    if children(document) != children(addon):
        raise ValueError('Part joint hierarchy differs from character')


def adopt_rest_pose(document, path):
    """Copy another export's joint rest transforms onto this skeleton."""
    source, _ = q.read_asset(path)
    check_skeleton(document, source)
    for a, b in zip(document['skins'][0]['joints'], source['skins'][0]['joints']):
        for key in ('translation', 'rotation', 'scale'):
            document['nodes'][a].pop(key, None)
            if key in source['nodes'][b]:
                document['nodes'][a][key] = list(source['nodes'][b][key])


def skin_for(document, buffers, matrices):
    """Reuse a skin whose inverse bind matrices match, else add one."""
    for index, skin in enumerate(document['skins']):
        existing = q.values(document, buffers, skin['inverseBindMatrices'])
        if all(abs(x - y) <= 1e-5 for a, b in zip(existing, matrices) for x, y in zip(a, b)):
            return index
    data = b''.join(struct.pack('<16f', *m) for m in matrices)
    buffers.append(data)
    document['bufferViews'].append({'buffer': len(buffers) - 1, 'byteLength': len(data)})
    document['accessors'].append({'bufferView': len(document['bufferViews']) - 1,
                                  'componentType': 5126, 'type': 'MAT4',
                                  'count': len(matrices)})
    document['skins'].append({'joints': list(document['skins'][0]['joints']),
                              'inverseBindMatrices': len(document['accessors']) - 1})
    return len(document['skins']) - 1


def attach(document, buffers, path, rename):
    """Merge a skinned part's meshes; `rename(node name)` -> kit name or None."""
    addon, extra = q.read_asset(path)
    check_skeleton(document, addon)
    skin = skin_for(document, buffers, q.values(
        addon, extra, addon['skins'][0]['inverseBindMatrices']))
    offsets = {key: len(document.get(key, [])) for key in
               ('bufferViews', 'accessors', 'images', 'samplers', 'textures',
                'materials', 'meshes')}
    buffer_offset = len(buffers)
    buffers.extend(extra)
    for original in addon.get('bufferViews', []):
        view = deepcopy(original)
        view['buffer'] += buffer_offset
        document['bufferViews'].append(view)
    for original in addon.get('accessors', []):
        accessor = deepcopy(original)
        if 'bufferView' in accessor:
            accessor['bufferView'] += offsets['bufferViews']
        for key in ('indices', 'values'):
            if 'sparse' in accessor:
                accessor['sparse'][key]['bufferView'] += offsets['bufferViews']
        document['accessors'].append(accessor)
    for original in addon.get('images', []):
        image = deepcopy(original)
        if 'uri' in image:
            image['uri'] = str((path.parent / image['uri']).resolve())
        else:
            image['bufferView'] += offsets['bufferViews']
        document.setdefault('images', []).append(image)
    document.setdefault('samplers', []).extend(deepcopy(addon.get('samplers', [])))
    for original in addon.get('textures', []):
        texture = deepcopy(original)
        texture['source'] += offsets['images']
        if 'sampler' in texture:
            texture['sampler'] += offsets['samplers']
        document.setdefault('textures', []).append(texture)
    for original in addon.get('materials', []):
        material = deepcopy(original)
        for _, info in q.texture_infos(material):
            info['index'] += offsets['textures']
        document.setdefault('materials', []).append(material)
    for original in addon['meshes']:
        mesh = deepcopy(original)
        for primitive in mesh['primitives']:
            primitive['attributes'] = {key: value + offsets['accessors']
                                       for key, value in primitive['attributes'].items()}
            primitive['indices'] += offsets['accessors']
            primitive['material'] += offsets['materials']
            for target in primitive.get('targets', []):
                for key in target:
                    target[key] += offsets['accessors']
        document['meshes'].append(mesh)
    root = document['scenes'][document.get('scene', 0)]['nodes'][0]
    added = []
    for original in addon['nodes']:
        if 'mesh' not in original:
            continue
        name = rename(original.get('name', ''))
        if name is None:
            continue
        if original.get('skin') != 0:
            raise ValueError('Expected skinned parts')
        node = {'name': name, 'mesh': original['mesh'] + offsets['meshes'], 'skin': skin}
        document['nodes'][root].setdefault('children', []).append(len(document['nodes']))
        document['nodes'].append(node)
        added.append(name)
    return added


def split_body(document, buffers, node_name, calf_split=CALF_SPLIT):
    """Replace the base body node with one node per covered region.

    Index-only split: every region shares the original vertex accessors, skin
    and morph targets, so seams and skinning are unchanged.
    """
    nodes = [n for n in document['nodes'] if n.get('name') == node_name and 'mesh' in n]
    if len(nodes) != 1:
        raise ValueError(f'Expected one {node_name} body node')
    body = nodes[0]
    mesh = document['meshes'][body['mesh']]
    if len(mesh['primitives']) != 1:
        raise ValueError('Expected a single-primitive body mesh')
    primitive = mesh['primitives'][0]
    names = joint_names(document)
    positions = q.mesh_values(document, buffers, primitive['attributes']['POSITION'])
    joints = q.mesh_values(document, buffers, primitive['attributes']['JOINTS_0'])
    weights = q.mesh_values(document, buffers, primitive['attributes']['WEIGHTS_0'])
    indices = [v[0] for v in q.mesh_values(document, buffers, primitive['indices'])]
    if len(indices) % 3:
        raise ValueError('Expected triangle-list body geometry')
    regions = {region: [] for region in REGIONS}
    for triangle in zip(indices[::3], indices[1::3], indices[2::3]):
        total = Counter()
        for vertex in triangle:
            for joint, weight in zip(joints[vertex], weights[vertex]):
                total[names[joint]] += weight
        region = REGION_BONES.get(total.most_common(1)[0][0], 'Arms')
        if region == 'Calves' and sum(positions[v][1] for v in triangle) / 3 <= calf_split:
            region = 'Shins'
        regions[region].extend(triangle)
    if not regions['Head']:
        raise ValueError('Body split found no head geometry')
    body_index = document['nodes'].index(body)
    parent = next(n for n in document['nodes'] if body_index in n.get('children', []))
    parent['children'].remove(body_index)
    # An unreachable, anonymous leftover: nothing can address the old body.
    body.pop('mesh')
    body.pop('skin', None)
    body.pop('name', None)
    created = []
    for region, selected in regions.items():
        if not selected:
            continue
        data = struct.pack('<' + 'I' * len(selected), *selected)
        buffers.append(data)
        document['bufferViews'].append({'buffer': len(buffers) - 1, 'byteLength': len(data)})
        document['accessors'].append({
            'bufferView': len(document['bufferViews']) - 1, 'componentType': 5125,
            'type': 'SCALAR', 'count': len(selected),
            'min': [min(selected)], 'max': [max(selected)]})
        part = deepcopy(mesh)
        part['name'] = f'Body_{region}'
        part['primitives'][0]['indices'] = len(document['accessors']) - 1
        document['meshes'].append(part)
        parent['children'].append(len(document['nodes']))
        document['nodes'].append({'name': f'Body_{region}', 'mesh': len(document['meshes']) - 1,
                                  'skin': 0})
        created.append(f'Body_{region}')
    return created


def drop_nodes(document, names):
    for node in document['nodes']:
        if node.get('name') in names and 'mesh' in node:
            node.pop('mesh')
            node.pop('skin', None)
            node.pop('name', None)


def rename_node(document, old, new):
    matches = [n for n in document['nodes'] if n.get('name') == old and 'mesh' in n]
    if len(matches) != 1:
        raise ValueError(f'Expected one {old} node')
    matches[0]['name'] = new


def image_bytes(document, buffers, base_dir):
    """Every image's PNG bytes, from files (glTF) or binary views (GLB)."""
    result = []
    for image in document.get('images', []):
        if 'uri' in image:
            path = base_dir / image['uri']
            # Three supplied base-character URIs have a spurious "_png" suffix.
            if not path.is_file():
                path = path.with_name(path.name.replace('_png.png', '.png'))
            data = path.read_bytes()
        else:
            view = document['bufferViews'][image['bufferView']]
            start = view.get('byteOffset', 0)
            data = bytes(buffers[view['buffer']][start:start + view['byteLength']])
        if data[:8] != b'\x89PNG\r\n\x1a\n':
            raise ValueError('Expected PNG character textures')
        result.append(data)
    return result


def body_bounds(document, buffers):
    """Bare sole and crown of the base body, independent of hair and boots."""
    ys = []
    for node in document['nodes']:
        if 'mesh' not in node or not node.get('name', '').startswith('Body_'):
            continue
        for primitive in document['meshes'][node['mesh']]['primitives']:
            positions = q.mesh_values(document, buffers, primitive['attributes']['POSITION'])
            ys.extend(positions[v[0]][1] for v in
                      q.mesh_values(document, buffers, primitive['indices']))
    if not ys:
        raise ValueError('Kit has no body geometry')
    return min(ys), max(ys)


def kit_png(data, kind):
    """Bounded runtime PNG; eye textures share a 256px layout."""
    if kind == 'eye':
        return q.runtime_png(data, 'color', 256)
    return q.runtime_png(data, kind)


def iris_png(data, color):
    """Recolour the CC0 brown iris with the pinned offline image tool."""
    result = q.subprocess.run(
        ['cargo', 'run', '--quiet', '--locked', '-p', 'arena-textures',
         '--bin', 'character-textures', '--', 'iris', color],
        input=data, capture_output=True, check=False)
    if result.returncode:
        raise ValueError('Iris recolour failed: ' + result.stderr.decode(errors='replace'))
    return result.stdout


def deduplicate_materials(document):
    primitives = [p for mesh in document['meshes'] for p in mesh['primitives']]
    unique, mapping, seen = [], {}, {}
    for index, material in enumerate(document.get('materials', [])):
        identity = json.dumps(material, sort_keys=True)
        if identity not in seen:
            seen[identity] = len(unique)
            unique.append(material)
        mapping[index] = seen[identity]
    document['materials'] = unique
    for primitive in primitives:
        primitive['material'] = mapping[primitive['material']]


def assemble(kit, library_path=q.LIBRARY, strafe_path=q.STRAFE_LIBRARY,
             texture_processor=None):
    """Build one kit. Returns (GLB bytes, manifest groups, bounds)."""
    model, model_buffers = q.read_asset(kit.base)
    document = deepcopy(model)
    buffers = list(model_buffers)
    for skin in document['skins'][1:]:
        if skin['joints'] != document['skins'][0]['joints']:
            raise ValueError('Expected one skeleton')
    if kit.skeleton is not None:
        adopt_rest_pose(document, kit.skeleton)
    drop_nodes(document, kit.drop)
    rename_node(document, kit.eyes_node, 'Eyes')
    if kit.race_hair:
        rename_node(document, kit.race_hair, 'Race_Hair')
    if kit.horns:
        rename_node(document, kit.horns, 'Race_Horns')
    if kit.outfits:
        regions = split_body(document, buffers, kit.body_node)
    else:
        rename_node(document, kit.body_node, 'Body_Full')
        regions = ['Body_Full']
    groups = {}
    hair = []
    for name, label in HAIRSTYLES:
        attach(document, buffers, HAIR / f'{name}.gltf', lambda n, name=name: name)
        hair.append({'id': name, 'label': label, 'nodes': [name]})
    if kit.race_hair:
        hair.insert(0, {'id': 'Race_Hair', 'label': f'{kit.ancestry.title()} locks',
                        'nodes': ['Race_Hair']})
    groups['hair'] = [{'id': 'none', 'label': 'None', 'nodes': []}] + hair
    groups['brows'] = [{'id': 'none', 'label': 'None', 'nodes': []}]
    for name, label in BROWS:
        attach(document, buffers, HAIR / f'{name}.gltf', lambda n, name=name: name)
        groups['brows'].append({'id': name, 'label': label, 'nodes': [name]})
    attach(document, buffers, HAIR / f'{BEARD}.gltf', lambda n: BEARD)
    groups['beard'] = [{'id': 'none', 'label': 'None', 'nodes': []},
                       {'id': BEARD, 'label': 'Full beard', 'nodes': [BEARD]}]
    if kit.horns:
        groups['horns'] = [{'id': 'none', 'label': 'None', 'nodes': []},
                           {'id': 'Race_Horns', 'label': 'Horns', 'nodes': ['Race_Horns']}]
    slots = {slot: {} for slot in OUTFIT_SLOTS}
    for path in kit.outfits:
        for name in attach(document, buffers, path, piece_name):
            outfit, piece = name.split('_', 1)
            slot, covers = PIECES[piece]
            option = slots[slot].setdefault(outfit, {
                'id': outfit, 'label': outfit, 'nodes': [], 'covers': [],
                'material': outfit})
            option['nodes'].append(name)
            option['covers'].extend(f'Body_{r}' for r in covers)
    if kit.outfits:
        for slot in OUTFIT_SLOTS:
            groups[slot] = [{'id': 'none', 'label': 'None', 'nodes': [], 'covers': []}] + [
                slots[slot][outfit] for outfit, _ in OUTFIT_SETS if outfit in slots[slot]]
    for material in document.get('materials', []):
        if material.get('name') not in MATERIALS:
            raise ValueError(f'Unmapped material: {material.get("name")}')
        material['name'] = MATERIALS[material['name']]
    low, high = body_bounds(document, buffers)

    # One binary buffer, processed and embedded PNGs, then the clips.
    images = image_bytes(document, buffers, kit.base.parent)
    binary = bytearray()

    def append(data):
        binary.extend(b'\0' * (-len(binary) % 4))
        offset = len(binary)
        binary.extend(data)
        return offset

    for view in document['bufferViews']:
        start = view.get('byteOffset', 0)
        data = buffers[view['buffer']][start:start + view['byteLength']]
        view['buffer'] = 0
        view['byteOffset'] = append(data)
    kinds = {}
    for material in document.get('materials', []):
        for key, info in q.texture_infos(material):
            source = document['textures'][info['index']]['source']
            kinds[source] = ('normal' if 'normal' in key.lower() else
                             'color' if key in ('baseColorTexture', 'emissiveTexture')
                             else 'data')
    embedded = {}
    for index, image in enumerate(document.get('images', [])):
        image.pop('uri', None)
        image.pop('name', None)
        data = images[index]
        if texture_processor is not None:
            data = texture_processor(data, kinds.get(index, 'data'))
        image['mimeType'] = 'image/png'
        if data not in embedded:
            embedded[data] = len(document['bufferViews'])
            document['bufferViews'].append(
                {'buffer': 0, 'byteOffset': append(data), 'byteLength': len(data)})
        image['bufferView'] = embedded[data]

    library, library_buffers = q.read_asset(library_path)
    strafe, strafe_buffers = q.read_asset(strafe_path)

    def accessor(samples, kind):
        width = {'SCALAR': 1, 'VEC3': 3, 'VEC4': 4}[kind]
        data = b''.join(struct.pack('<' + 'f' * width, *s) for s in samples)
        document['bufferViews'].append(
            {'buffer': 0, 'byteOffset': append(data), 'byteLength': len(data)})
        result = {'bufferView': len(document['bufferViews']) - 1, 'componentType': 5126,
                  'count': len(samples), 'type': kind}
        if kind == 'SCALAR':
            result['min'], result['max'] = [samples[0][0]], [samples[-1][0]]
        document['accessors'].append(result)
        return len(document['accessors']) - 1

    document['animations'] = []
    for clip in q.CLIPS:
        source, source_buffers = ((strafe, strafe_buffers)
                                  if clip[0] in ('Strafe_Left', 'Strafe_Right')
                                  else (library, library_buffers))
        for name, tracks in q.retarget(document, source, source_buffers, clips=(clip,)):
            animation = {'name': name, 'channels': [], 'samplers': []}
            for node, path, times, samples in tracks:
                animation['samplers'].append({
                    'input': accessor([(t,) for t in times], 'SCALAR'),
                    'output': accessor(samples, 'VEC4' if path == 'rotation' else 'VEC3'),
                    'interpolation': 'LINEAR'})
                animation['channels'].append({
                    'sampler': len(animation['samplers']) - 1,
                    'target': {'node': node, 'path': path}})
            document['animations'].append(animation)
    license_note = ('Quaternius races: QAL v1.0 (game use only; no standalone redistribution). '
                    if kit.optional else '')
    document['asset'] = {
        'version': '2.0', 'generator': 'Arena Quaternius kit adapter',
        'copyright': license_note + 'Quaternius base/hair/outfits/UAL1: CC0 1.0. '
                     'Authored strafe curves: MIT; see licenses/universal-strafes-MIT.txt.',
    }

    def strip_extras(value):
        if isinstance(value, dict):
            value.pop('extras', None)
            for child in value.values():
                strip_extras(child)
        elif isinstance(value, list):
            for child in value:
                strip_extras(child)
    strip_extras(document)
    binary = q.compact(document, binary)
    deduplicate_materials(document)
    document['buffers'] = [{'byteLength': len(binary)}]
    payload = json.dumps(document, separators=(',', ':')).encode()
    payload += b' ' * (-len(payload) % 4)
    binary.extend(b'\0' * (-len(binary) % 4))
    size = 12 + 8 + len(payload) + 8 + len(binary)
    glb = (struct.pack('<III', 0x46546C67, 2, size) +
           struct.pack('<II', len(payload), 0x4E4F534A) + payload +
           struct.pack('<II', len(binary), 0x004E4942) + binary)
    return glb, groups, regions, (low, high)


def prebuilt(out, source=PREBUILT):
    """Copy every finished kit under `source` into `out`; return their manifest entries."""
    entries = []
    for manifest in sorted(source.glob('*/kit.json')):
        entry = json.loads(manifest.read_text())
        identifier = entry.get('id')
        if identifier != manifest.parent.name or entry.get('path') != f'{DIRECTORY}/{identifier}.glb':
            raise ValueError(f'{manifest}: id and path must match the kit folder name')
        if not all(isinstance(entry.get(key), kind) for key, kind in (
                ('ancestry', str), ('body', str), ('regions', list), ('groups', dict),
                ('materials', dict), ('sole', (int, float)))):
            raise ValueError(f'{manifest}: incomplete kit manifest')
        glb = manifest.with_name(f'{identifier}.glb')
        if not glb.is_file():
            raise ValueError(f'{manifest}: {glb.name} is missing')
        (out / glb.name).write_bytes(glb.read_bytes())
        # Every kit is drawn at the one shared scale (ADR 0040).
        entries.append({**entry, 'scale': SCALE})
    return entries


def prepare(directory, texture_processor=kit_png, iris_processor=iris_png, kits=KITS,
            library_path=q.LIBRARY, strafe_path=q.STRAFE_LIBRARY, prebuilt_path=PREBUILT):
    """Write every installed kit and its variant textures; return the manifest."""
    out = directory / DIRECTORY
    textures = out / 'textures'
    textures.mkdir(parents=True, exist_ok=True)
    for stale in list(out.glob('*.glb')) + list(textures.glob('*.png')):
        stale.unlink()
    cache = {}

    def processed(data, kind):
        key = (data, kind)
        if key not in cache:
            cache[key] = texture_processor(data, kind) if texture_processor else data
        return cache[key]

    def texture(name, source, kind='color'):
        if source is None:
            return None
        (textures / f'{name}.png').write_bytes(processed(source.read_bytes(), kind))
        return f'{DIRECTORY}/textures/{name}.png'

    races = any(k.optional and available(k) for k in kits)
    eyes = [{'id': 'brown', 'label': 'Brown', 'texture': texture('eye-brown', IRIS, 'eye')}]
    for key, label, color in EYE_COLORS:
        data = iris_processor(IRIS.read_bytes(), color) if iris_processor else IRIS.read_bytes()
        (textures / f'eye-{key}.png').write_bytes(processed(data, 'eye'))
        eyes.append({'id': key, 'label': label,
                     'texture': f'{DIRECTORY}/textures/eye-{key}.png'})
    if races:
        for key, label, source in RACE_EYES:
            if source.is_file():
                # Shared 256px eye layout; the 2048px orc eye is bounded too.
                texture(f'eye-{key}', source, 'eye')
                eyes.append({'id': key, 'label': label,
                             'texture': f'{DIRECTORY}/textures/eye-{key}.png'})
    outfit_variants = {
        material: [{'id': i, 'label': label, 'texture': texture(f'{material.lower()}-{i}', s)}
                   for i, label, s in variants]
        for material, variants in OUTFIT_VARIANTS.items()}
    fantasy_hair = [{'id': i, 'label': label, 'texture': texture(f'fantasy-hair-{i}', s)}
                    for i, label, s in FANTASY_HAIR] if races else []
    manifest = []
    for kit in kits:
        if not available(kit):
            continue
        glb, groups, regions, (low, high) = assemble(
            kit, library_path, strafe_path, processed if texture_processor else None)
        (out / f'{kit.id}.glb').write_bytes(glb)
        materials = {
            'Skin': {'tint': 'skin', 'variants': [
                {'id': i, 'label': label, 'texture': texture(f'{kit.id}-skin-{i}', s)}
                for i, label, s in kit.skin_variants]},
            'Skin_Hands': {'tint': 'skin'},
            'Hair_1': {'tint': 'hair'}, 'Hair_2': {'tint': 'hair'},
            'Eyes': {'variants': eyes},
        }
        if kit.outfits:
            materials.update({m: {'variants': v} for m, v in outfit_variants.items()})
        if kit.race_hair:
            materials['Hair_Fantasy'] = {'tint': 'hair', 'variants': fantasy_hair}
        manifest.append({
            'id': kit.id, 'ancestry': kit.ancestry, 'body': kit.body,
            'path': f'{DIRECTORY}/{kit.id}.glb', 'sole': round(low, 6),
            'top': round(high, 6), 'scale': SCALE, 'license': 'QAL-1.0' if kit.optional else 'CC0-1.0',
            'regions': regions, 'groups': groups, 'materials': materials,
            'morph': 'Body_Full' if kit.morph else None,
            'eyes': 'elf' if kit.ancestry == 'elf' else 'orc' if kit.ancestry == 'orc' else 'brown',
        })
    return manifest + prebuilt(out, prebuilt_path)
