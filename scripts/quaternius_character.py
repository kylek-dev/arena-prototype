"""Offline Quaternius glTF adapter. Standard-library only; no Blender/runtime retargeter.

Keep the female skin/rest skeleton, retarget UAL1 local rotation deltas by bone
name, and retain target bone lengths. Only the selected clips enter the output.
"""

from copy import deepcopy
import json
import math
from pathlib import Path
import struct
import subprocess


ROOT = Path('assets/quaternius')
MODEL = ROOT / 'base/Base Characters/Godot - UE/Superhero_Female_FullBody.gltf'
LIBRARY = ROOT / 'animation v1/Unreal-Godot/UAL1_Standard.glb'
STRAFE_LIBRARY = Path('assets/animations/universal-strafes.glb')
OUTFIT = ROOT / 'outfits/Exports/glTF (Godot-Unreal)/Outfits/Female_Ranger.gltf'
HAIR = ROOT / 'base/Hairstyles/Rigged to Head Bone/glTF (Godot -Unreal)/Hair_Buns.gltf'
IDENTIFIER = 'quaternius-female'
TEXTURE_EDGE = 1024
DETAIL_EDGE = 512
# Standard has no directional jogs. Backward reverses the jog cycle; lateral
# movement uses the separate Blender-authored universal library.
CLIPS = (
    ('Idle', 'Idle_Loop', False),
    ('Move_Forward', 'Jog_Fwd_Loop', False),
    ('Move_Backward', 'Jog_Fwd_Loop', True),
    ('Strafe_Left', 'Strafe_Left', False),
    ('Strafe_Right', 'Strafe_Right', False),
    ('Jump', 'Jump_Loop', False),
    ('Attack', 'Punch_Jab', False),
    ('Cast', 'Spell_Simple_Shoot', False),
    ('Jump_Start', 'Jump_Start', False),
    ('Jump_Land', 'Jump_Land', False),
)


def read_asset(path):
    data = path.read_bytes()
    if path.suffix == '.glb':
        if len(data) < 28 or struct.unpack_from('<III', data) != (
                0x46546C67, 2, len(data)):
            raise ValueError(f'Invalid GLB: {path}')
        size, kind = struct.unpack_from('<II', data, 12)
        if kind != 0x4E4F534A:
            raise ValueError('GLB must start with JSON')
        document = json.loads(data[20:20 + size])
        length, kind = struct.unpack_from('<II', data, 20 + size)
        if kind != 0x004E4942:
            raise ValueError('Expected binary chunk')
        buffers = [data[28 + size:28 + size + length]]
    else:
        document = json.loads(data)
        buffers = [(path.parent / b['uri']).read_bytes()
                   for b in document.get('buffers', [])]
    return document, buffers


def values(document, buffers, index):
    accessor = document['accessors'][index]
    if accessor['componentType'] != 5126 or 'sparse' in accessor:
        raise ValueError('Animation accessors must contain dense float32 keys')
    width = {'SCALAR': 1, 'VEC3': 3, 'VEC4': 4, 'MAT4': 16}[accessor['type']]
    view = document['bufferViews'][accessor['bufferView']]
    offset = view.get('byteOffset', 0) + accessor.get('byteOffset', 0)
    stride = view.get('byteStride', width * 4)
    return [struct.unpack_from('<' + 'f' * width, buffers[view['buffer']],
                               offset + i * stride)
            for i in range(accessor['count'])]


def multiply(a, b):
    x, y, z, w = a
    X, Y, Z, W = b
    return (w * X + x * W + y * Z - z * Y,
            w * Y - x * Z + y * W + z * X,
            w * Z + x * Y - y * X + z * W,
            w * W - x * X - y * Y - z * Z)


def inverse(q):
    norm = sum(v * v for v in q)
    return tuple((-v if i < 3 else v) / norm for i, v in enumerate(q))


def normalize(q):
    length = math.sqrt(sum(v * v for v in q))
    return tuple(v / length for v in q)


def joint_map(document):
    if len(document.get('skins', [])) != 1:
        raise ValueError('Expected one humanoid skin')
    joints = document['skins'][0]['joints']
    names = {document['nodes'][i]['name']: i for i in joints}
    if len(names) != len(joints) or not {'root', 'pelvis', 'Head'} <= names.keys():
        raise ValueError('Expected unique named humanoid joints')
    return names


def mesh_values(document, buffers, index):
    """Decode dense mesh data without requiring numpy or a DCC."""
    accessor = document['accessors'][index]
    if 'sparse' in accessor or accessor.get('normalized', False):
        raise ValueError('Expected dense, unnormalized mesh accessor')
    fmt = {5121: 'B', 5123: 'H', 5125: 'I', 5126: 'f'}[accessor['componentType']]
    width = {'SCALAR': 1, 'VEC2': 2, 'VEC3': 3, 'VEC4': 4}[accessor['type']]
    view = document['bufferViews'][accessor['bufferView']]
    offset = view.get('byteOffset', 0) + accessor.get('byteOffset', 0)
    stride = view.get('byteStride', struct.calcsize(fmt) * width)
    return [struct.unpack_from('<' + fmt * width, buffers[view['buffer']],
                               offset + i * stride)
            for i in range(accessor['count'])]


def head_only(document, buffers):
    """Retain connected head/neck surfaces, not a second body under clothing.

    This explicit crop is for the supplied female export, not arbitrary models.
    Its UV-split head and neck components are wholly above 1.46 m and reach
    1.53 m. Torso/arms/hands/legs/feet do not satisfy both bounds.
    """
    bodies = [m for m in document['meshes'] if m.get('name') == 'Superhero_Female']
    if len(bodies) != 1:
        raise ValueError('Expected the supplied Superhero_Female body mesh')
    body = bodies[0]
    for primitive in body['primitives']:
        positions = mesh_values(document, buffers, primitive['attributes']['POSITION'])
        indices = [v[0] for v in mesh_values(document, buffers, primitive['indices'])]
        if len(indices) % 3:
            raise ValueError('Expected triangle-list body geometry')
        parents = list(range(len(positions)))

        def find(index):
            while parents[index] != index:
                parents[index] = parents[parents[index]]
                index = parents[index]
            return index

        for a, b, c in zip(indices[::3], indices[1::3], indices[2::3]):
            parents[find(a)] = find(b)
            parents[find(c)] = find(b)
        bounds = {}
        for index in set(indices):
            component = find(index)
            y = positions[index][1]
            low, high = bounds.get(component, (y, y))
            bounds[component] = min(low, y), max(high, y)
        keep = {c for c, (low, high) in bounds.items() if low > 1.46 and high > 1.53}
        selected = [i for i in indices if find(i) in keep]
        if not selected or len(selected) == len(indices):
            raise ValueError('Head extraction must remove covered body geometry')
        # Index-only crop preserves all skin attributes/UVs exactly. Unused
        # vertex data is retained, but no covered body triangles are drawn.
        data = struct.pack('<' + 'I' * len(selected), *selected)
        buffer = len(buffers)
        buffers.append(data)
        view = len(document['bufferViews'])
        document['bufferViews'].append({'buffer': buffer, 'byteLength': len(data)})
        primitive['indices'] = len(document['accessors'])
        document['accessors'].append({
            'bufferView': view, 'componentType': 5125, 'type': 'SCALAR',
            'count': len(selected), 'min': [min(selected)], 'max': [max(selected)],
        })
    body['name'] = 'Human_Head_Neck'
    for node in document['nodes']:
        if node.get('name') == 'Superhero_Female':
            node['name'] = 'Human_Head_Neck'


def attach_meshes(document, buffers, path, exclude=()):
    """Merge skinned clothing/hair onto one *verified identical* rest skeleton."""
    addon, extra_buffers = read_asset(path)
    target_joints = document['skins'][0]['joints']
    addon_joints = addon['skins'][0]['joints']
    if len(target_joints) != len(addon_joints):
        raise ValueError('Equipment joint count differs from character')
    for a, b in zip(target_joints, addon_joints):
        target, source = document['nodes'][a], addon['nodes'][b]
        if target['name'] != source['name']:
            raise ValueError('Equipment joint order differs from character')
        for field, default in [('translation', [0, 0, 0]),
                               ('rotation', [0, 0, 0, 1]), ('scale', [1, 1, 1])]:
            if any(abs(x - y) > 1e-5 for x, y in
                   zip(target.get(field, default), source.get(field, default))):
                raise ValueError(f'Equipment rest transform differs: {source["name"]}')
        names = lambda doc, n: [doc['nodes'][c]['name'] for c in n.get('children', [])
                                if c in doc['skins'][0]['joints']]
        if names(document, target) != names(addon, source):
            raise ValueError('Equipment joint hierarchy differs from character')
    # Sharing a skin also requires equal inverse bind matrices, not only names.
    for a, b in zip(values(document, buffers, document['skins'][0]['inverseBindMatrices']),
                    values(addon, extra_buffers, addon['skins'][0]['inverseBindMatrices'])):
        if any(abs(x - y) > 1e-5 for x, y in zip(a, b)):
            raise ValueError('Equipment inverse bind matrices differ')
    offsets = {key: len(document.get(key, [])) for key in
               ('bufferViews', 'accessors', 'images', 'samplers', 'textures',
                'materials', 'meshes')}
    buffer_offset = len(buffers)
    buffers.extend(extra_buffers)
    for original in addon.get('bufferViews', []):
        view = deepcopy(original)
        view['buffer'] += buffer_offset
        document['bufferViews'].append(view)
    for original in addon.get('accessors', []):
        accessor = deepcopy(original)
        if 'sparse' in accessor:
            raise ValueError('Sparse equipment accessors are not supported')
        accessor['bufferView'] += offsets['bufferViews']
        document['accessors'].append(accessor)
    for original in addon.get('images', []):
        image = deepcopy(original)
        image['uri'] = str((path.parent / image['uri']).resolve())
        document.setdefault('images', []).append(image)
    document.setdefault('samplers', []).extend(deepcopy(addon.get('samplers', [])))
    for original in addon.get('textures', []):
        texture = deepcopy(original)
        texture['source'] += offsets['images']
        if 'sampler' in texture:
            texture['sampler'] += offsets['samplers']
        document.setdefault('textures', []).append(texture)
    def rebase_textures(value):
        for key, child in value.items():
            if isinstance(child, dict):
                if key.endswith('Texture') and 'index' in child:
                    child['index'] += offsets['textures']
                else:
                    rebase_textures(child)
    for original in addon.get('materials', []):
        material = deepcopy(original)
        rebase_textures(material)
        document.setdefault('materials', []).append(material)
    for original in addon['meshes']:
        mesh = deepcopy(original)
        for primitive in mesh['primitives']:
            primitive['attributes'] = {key: value + offsets['accessors']
                                       for key, value in primitive['attributes'].items()}
            primitive['indices'] += offsets['accessors']
            primitive['material'] += offsets['materials']
        document['meshes'].append(mesh)
    root = document['scenes'][document.get('scene', 0)]['nodes'][0]
    for original in addon['nodes']:
        if 'mesh' not in original or original.get('name') in exclude:
            continue
        if original.get('skin') != 0:
            raise ValueError('Expected skinned equipment')
        node = deepcopy(original)
        node['mesh'] += offsets['meshes']
        node['skin'] = 0
        document['nodes'][root].setdefault('children', []).append(len(document['nodes']))
        document['nodes'].append(node)


def texture_infos(material):
    for key, child in material.items():
        if isinstance(child, dict):
            if key.endswith('Texture') and 'index' in child:
                yield key, child
            else:
                yield from texture_infos(child)


def runtime_png(data, kind):
    """Use the existing pinned offline image crate; no new Python dependency."""
    result = subprocess.run(
        ['cargo', 'run', '--quiet', '--locked', '-p', 'arena-textures',
         '--bin', 'character-textures', '--', kind,
         str(TEXTURE_EDGE if kind == 'color' else DETAIL_EDGE)],
        input=data, capture_output=True, check=False)
    if result.returncode:
        raise ValueError('Character texture preparation failed: ' +
                         result.stderr.decode(errors='replace'))
    return result.stdout


def compact(document, binary):
    """Drop unused assets and rebase every surviving glTF reference."""
    def retain(key, used):
        entries = document.get(key, [])
        mapping = {old: new for new, old in enumerate(sorted(used))}
        if used:
            document[key] = [entries[i] for i in sorted(used)]
        else:
            document.pop(key, None)
        return mapping

    mesh_map = retain('meshes', {n['mesh'] for n in document['nodes'] if 'mesh' in n})
    for node in document['nodes']:
        if 'mesh' in node:
            node['mesh'] = mesh_map[node['mesh']]
    primitives = [p for mesh in document.get('meshes', []) for p in mesh['primitives']]
    material_map = retain('materials', {p['material'] for p in primitives if 'material' in p})
    for primitive in primitives:
        if 'material' in primitive:
            primitive['material'] = material_map[primitive['material']]
    infos = [info for material in document.get('materials', [])
             for _, info in texture_infos(material)]
    texture_map = retain('textures', {info['index'] for info in infos})
    for info in infos:
        info['index'] = texture_map[info['index']]
    textures = document.get('textures', [])
    # Shared atlases should be decoded/uploaded only once, not merely share bytes.
    used_images = {t['source'] for t in textures}
    unique_images, image_map, identities = [], {}, {}
    for index in sorted(used_images):
        image = document['images'][index]
        identity = (image['mimeType'], image['bufferView'])
        if identity not in identities:
            identities[identity] = len(unique_images)
            unique_images.append(image)
        image_map[index] = identities[identity]
    if unique_images:
        document['images'] = unique_images
    else:
        document.pop('images', None)
    for texture in textures:
        texture['source'] = image_map[texture['source']]
    sampler_map = retain('samplers', {t['sampler'] for t in textures if 'sampler' in t})
    for texture in textures:
        if 'sampler' in texture:
            texture['sampler'] = sampler_map[texture['sampler']]
    def deduplicate(key):
        unique, mapping, seen = [], {}, {}
        for index, entry in enumerate(document.get(key, [])):
            identity = json.dumps({k: v for k, v in entry.items()
                                   if k not in ('name', 'extras')}, sort_keys=True)
            if identity not in seen:
                seen[identity] = len(unique)
                unique.append(entry)
            mapping[index] = seen[identity]
        if unique:
            document[key] = unique
        return mapping
    sampler_map = deduplicate('samplers')
    for texture in textures:
        if 'sampler' in texture:
            texture['sampler'] = sampler_map[texture['sampler']]
    # Bevy decodes glTF textures, not just image records. Deduplicate these too.
    texture_map = deduplicate('textures')
    for info in infos:
        info['index'] = texture_map[info['index']]
    references = []
    for primitive in primitives:
        references.extend((primitive['attributes'], k) for k in primitive['attributes'])
        if 'indices' in primitive:
            references.append((primitive, 'indices'))
        for target in primitive.get('targets', []):
            references.extend((target, k) for k in target)
    for skin in document.get('skins', []):
        if 'inverseBindMatrices' in skin:
            references.append((skin, 'inverseBindMatrices'))
    for animation in document['animations']:
        for sampler in animation['samplers']:
            references.extend(((sampler, 'input'), (sampler, 'output')))
    accessor_map = retain('accessors', {obj[key] for obj, key in references})
    for obj, key in references:
        obj[key] = accessor_map[obj[key]]
    views = [(a, 'bufferView') for a in document['accessors']]
    if any('sparse' in a for a in document['accessors']):
        raise ValueError('Sparse runtime accessors are not supported')
    views.extend((i, 'bufferView') for i in document.get('images', []))
    view_map = retain('bufferViews', {obj[key] for obj, key in views})
    for obj, key in views:
        obj[key] = view_map[obj[key]]
    packed = bytearray()
    for view in document['bufferViews']:
        start = view.get('byteOffset', 0)
        data = binary[start:start + view['byteLength']]
        packed.extend(b'\0' * (-len(packed) % 4))
        view['byteOffset'] = len(packed)
        packed.extend(data)
    return packed


def retarget(model, library, buffers, clips=CLIPS):
    target = joint_map(model)
    source = joint_map(library)
    if target.keys() != source.keys():
        raise ValueError('Character and animation joint sets differ')
    # Matching names alone are insufficient: hierarchy must match as well.
    def parents(document, joints):
        names = {i: name for name, i in joints.items()}
        return {names[c]: names[i] for i in names
                for c in document['nodes'][i].get('children', []) if c in names}
    if parents(model, target) != parents(library, source):
        raise ValueError('Character and animation joint hierarchies differ')
    animations = {a['name']: a for a in library['animations']}
    ratio = (model['nodes'][target['pelvis']]['translation'][2] /
             library['nodes'][source['pelvis']]['translation'][2])
    for name, original, reverse in clips:
        if original not in animations:
            raise ValueError(f'Missing animation clip: {original}')
        tracks = []
        animation = animations[original]
        for channel in animation['channels']:
            source_index = channel['target']['node']
            bone = library['nodes'][source_index]['name']
            path = channel['target']['path']
            # No animated root, scale or bone-length changes. Pelvis supplies
            # small pose offsets, never world locomotion or a second jump arc.
            if bone == 'root' or path == 'scale' or (
                    path == 'translation' and bone != 'pelvis'):
                continue
            if bone not in target:
                raise ValueError(f'Animation targets unknown joint: {bone}')
            sampler = animation['samplers'][channel['sampler']]
            times = [v[0] for v in values(library, buffers, sampler['input'])]
            samples = values(library, buffers, sampler['output'])
            interpolation = sampler.get('interpolation', 'LINEAR')
            constant_step = interpolation == 'STEP' and all(v == samples[0] for v in samples)
            if interpolation != 'LINEAR' and not constant_step:
                raise ValueError('Expected linear or constant animation keys')
            if len(times) != len(samples) or not times or any(
                    a >= b for a, b in zip(times, times[1:])):
                raise ValueError('Invalid animation timeline')
            src, dst = library['nodes'][source_index], model['nodes'][target[bone]]
            if path == 'rotation':
                correction = multiply(dst.get('rotation', (0, 0, 0, 1)),
                                      inverse(src.get('rotation', (0, 0, 0, 1))))
                samples = [normalize(multiply(correction, q)) for q in samples]
            elif path == 'translation':
                samples = [tuple(t + (v - s) * ratio
                                 for v, s, t in zip(p, src['translation'],
                                                    dst['translation']))
                           for p in samples]
                if name in ('Jump', 'Jump_Start', 'Jump_Land'):
                    # In-place clips contain a real crouch/recovery, not a jump
                    # trajectory. Preserve that lowering; never add positive
                    # pelvis elevation on top of authoritative world height.
                    samples = [(p[0], p[1], min(p[2], dst['translation'][2]))
                               for p in samples]
            else:
                raise ValueError(f'Unsupported animation path: {path}')
            if reverse:
                times = [times[-1] - t for t in reversed(times)]
                samples.reverse()
            tracks.append((target[bone], path, times, samples))
        if not tracks:
            raise ValueError(f'Empty clip: {name}')
        yield name, tracks


def build(model_path=MODEL, library_path=LIBRARY, outfit_path=OUTFIT, hair_path=HAIR,
          texture_processor=None, strafe_path=STRAFE_LIBRARY):
    model, model_buffers = read_asset(model_path)
    library, library_buffers = read_asset(library_path)
    strafe, strafe_buffers = (read_asset(strafe_path) if strafe_path is not None
                              else (library, library_buffers))
    document = deepcopy(model)
    if outfit_path is not None:
        head_only(document, model_buffers)
        attach_meshes(document, model_buffers, outfit_path,
                      exclude=('Female_Ranger_Head_Hood',))
    if hair_path is not None:
        attach_meshes(document, model_buffers, hair_path)
    binary = bytearray()

    def append(data):
        binary.extend(b'\0' * (-len(binary) % 4))
        offset = len(binary)
        binary.extend(data)
        return offset

    # Rebase source views into one binary buffer; preserve mesh/skin indices.
    for view in document['bufferViews']:
        start = view.get('byteOffset', 0)
        data = model_buffers[view['buffer']][start:start + view['byteLength']]
        view['buffer'] = 0
        view['byteOffset'] = append(data)

    kinds = {}
    for material in document.get('materials', []):
        for key, info in texture_infos(material):
            source = document['textures'][info['index']]['source']
            kinds[source] = ('normal' if 'normal' in key.lower() else
                             'color' if key in ('baseColorTexture', 'emissiveTexture')
                             else 'data')
    embedded_images, processed = {}, {}
    for index, image in enumerate(document.get('images', [])):
        uri = image.pop('uri')
        path = model_path.parent / uri
        # Three supplied base-character URIs have a spurious "_png" suffix.
        if not path.is_file():
            path = path.with_name(path.name.replace('_png.png', '.png'))
        data = path.read_bytes()
        if data[:8] != b'\x89PNG\r\n\x1a\n':
            raise ValueError(f'Expected PNG texture: {path}')
        if texture_processor is not None:
            cache_key = (data, kinds.get(index, 'data'))
            if cache_key not in processed:
                processed[cache_key] = texture_processor(*cache_key)
            data = processed[cache_key]
        image['mimeType'] = 'image/png'
        # Buns and brows share the hair atlas; don't embed duplicate PNG bytes.
        if data not in embedded_images:
            embedded_images[data] = len(document['bufferViews'])
            document['bufferViews'].append(
                {'buffer': 0, 'byteOffset': append(data), 'byteLength': len(data)})
        image['bufferView'] = embedded_images[data]

    def accessor(samples, kind):
        width = {'SCALAR': 1, 'VEC3': 3, 'VEC4': 4}[kind]
        data = b''.join(struct.pack('<' + 'f' * width, *s) for s in samples)
        view = len(document['bufferViews'])
        document['bufferViews'].append(
            {'buffer': 0, 'byteOffset': append(data), 'byteLength': len(data)})
        result = {'bufferView': view, 'componentType': 5126,
                  'count': len(samples), 'type': kind}
        if kind == 'SCALAR':
            result['min'], result['max'] = [samples[0][0]], [samples[-1][0]]
        index = len(document['accessors'])
        document['accessors'].append(result)
        return index

    document['animations'] = []
    def selected_tracks():
        for clip in CLIPS:
            source, buffers = ((strafe, strafe_buffers)
                               if clip[0] in ('Strafe_Left', 'Strafe_Right')
                               else (library, library_buffers))
            yield from retarget(document, source, buffers, clips=(clip,))
    for name, tracks in selected_tracks():
        animation = {'name': name, 'channels': [], 'samplers': []}
        for node, path, times, samples in tracks:
            sampler = len(animation['samplers'])
            animation['samplers'].append({
                'input': accessor([(t,) for t in times], 'SCALAR'),
                'output': accessor(samples, 'VEC4' if path == 'rotation' else 'VEC3'),
                'interpolation': 'LINEAR',
            })
            animation['channels'].append(
                {'sampler': sampler, 'target': {'node': node, 'path': path}})
        document['animations'].append(animation)
    document['asset'] = {
        'version': '2.0', 'generator': 'Arena Quaternius adapter',
        'copyright': 'Quaternius character/outfit/UAL1: CC0 1.0. '
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
    binary = compact(document, binary)
    document['buffers'] = [{'byteLength': len(binary)}]
    payload = json.dumps(document, separators=(',', ':')).encode()
    payload += b' ' * (-len(payload) % 4)
    binary.extend(b'\0' * (-len(binary) % 4))
    size = 12 + 8 + len(payload) + 8 + len(binary)
    return (struct.pack('<III', 0x46546C67, 2, size) +
            struct.pack('<II', len(payload), 0x4E4F534A) + payload +
            struct.pack('<II', len(binary), 0x004E4942) + binary)
