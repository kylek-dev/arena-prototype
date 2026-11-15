"""Small synthetic glTFs exercise offline retargeting without downloaded assets."""
from copy import deepcopy
import importlib.util
import json
from pathlib import Path
import struct
import tempfile
import unittest


spec = importlib.util.spec_from_file_location(
    'quaternius_character',
    Path(__file__).resolve().parents[2] / 'scripts/quaternius_character.py')
q = importlib.util.module_from_spec(spec)
spec.loader.exec_module(q)


def fixture():
    binary = bytearray()
    document = {
        'asset': {'version': '2.0'}, 'scene': 0, 'scenes': [{'nodes': [0]}],
        'nodes': [
            {'name': 'root', 'children': [1, 3]},
            {'name': 'pelvis', 'translation': [0, 0, 1], 'children': [2]},
            {'name': 'Head', 'translation': [0, 1, 0]},
            {'name': 'Body', 'mesh': 0, 'skin': 0},
        ],
        'skins': [{'joints': [0, 1, 2]}],
        'meshes': [{'primitives': []}],
        'bufferViews': [], 'accessors': [], 'animations': [],
    }

    def add(samples, kind):
        data = b''.join(struct.pack('<' + 'f' * len(s), *s) for s in samples)
        view = len(document['bufferViews'])
        document['bufferViews'].append(
            {'buffer': 0, 'byteOffset': len(binary), 'byteLength': len(data)})
        binary.extend(data)
        document['accessors'].append(
            {'bufferView': view, 'componentType': 5126,
             'type': kind, 'count': len(samples)})
        return len(document['accessors']) - 1

    times = add([(0,), (0.5,)], 'SCALAR')
    rotations = add([(0, 0, 0, 1), (0.1, 0, 0, 0.994987437)], 'VEC4')
    translations = add([(0, 0, 1), (0.1, 0.2, 1.4)], 'VEC3')
    scales = add([(1, 1, 1), (1, 1, 1)], 'VEC3')
    for name in sorted({source for _, source, _ in q.CLIPS}):
        output = rotations
        if name in ('Strafe_Left', 'Strafe_Right'):
            sign = 1 if name == 'Strafe_Left' else -1
            output = add([(0, 0, 0, 1), (0, 0, sign * 0.1, 0.994987437)], 'VEC4')
        document['animations'].append({
            'name': name,
            'samplers': [{'input': times, 'output': a}
                         for a in [output, translations, scales]],
            'channels': [
                {'sampler': 0, 'target': {'node': 1, 'path': 'rotation'}},
                {'sampler': 1, 'target': {'node': 1, 'path': 'translation'}},
                {'sampler': 1, 'target': {'node': 0, 'path': 'translation'}},
                {'sampler': 1, 'target': {'node': 2, 'path': 'translation'}},
                {'sampler': 2, 'target': {'node': 1, 'path': 'scale'}},
            ],
        })
    matrices = add([tuple(1.0 if i % 5 == 0 else 0.0 for i in range(16))] * 3, 'MAT4')
    document['skins'][0]['inverseBindMatrices'] = matrices
    document['buffers'] = [{'uri': 'data.bin', 'byteLength': len(binary)}]
    return document, bytes(binary)


class QuaterniusCharacter(unittest.TestCase):
    def test_rest_rotations_lengths_root_and_jump_are_rule_safe(self):
        source, binary = fixture()
        target = deepcopy(source)
        target['nodes'][1]['translation'] = [0, 0, 2]
        target['nodes'][1]['rotation'] = [0, 0, 1, 0]
        original = deepcopy(source)
        clips = dict(q.retarget(target, source, [binary]))
        self.assertEqual(list(clips), [name for name, _, _ in q.CLIPS])
        for name, tracks in clips.items():
            self.assertEqual([(n, p) for n, p, _, _ in tracks],
                             [(1, 'rotation'), (1, 'translation')])
            for rotation in tracks[0][3]:
                self.assertAlmostEqual(sum(v * v for v in rotation), 1, places=6)
            # Source identity maps exactly onto target rest orientation.
            if name != 'Move_Backward':
                self.assertEqual(tracks[0][3][0], (0, 0, 1, 0))
            if name in ('Jump', 'Jump_Start', 'Jump_Land'):
                self.assertTrue(all(p[2] == 2 for p in tracks[1][3]))
        self.assertEqual(source, original)
        forward = clips['Move_Forward']
        backward = clips['Move_Backward']
        self.assertEqual(backward[0][2], forward[0][2])
        self.assertEqual(backward[0][3], list(reversed(forward[0][3])))
        self.assertNotEqual(clips['Strafe_Left'], forward)
        self.assertNotEqual(clips['Strafe_Right'], forward)
        self.assertNotEqual(clips['Strafe_Left'], clips['Strafe_Right'])

    def test_rejects_wrong_skeleton_hierarchy_missing_clip_and_bad_interpolation(self):
        for change in ('bones', 'hierarchy', 'clip', 'interpolation'):
            source, binary = fixture()
            target = deepcopy(source)
            if change == 'bones':
                target['nodes'][2]['name'] = 'OtherHead'
            elif change == 'hierarchy':
                target['nodes'][0]['children'].append(2)
                target['nodes'][1]['children'] = []
            elif change == 'clip':
                source['animations'].pop()
            else:
                source['animations'][0]['samplers'][0]['interpolation'] = 'CUBICSPLINE'
            with self.subTest(change=change), self.assertRaises(ValueError):
                list(q.retarget(target, source, [binary]))

    def test_build_embeds_textures_and_only_selected_animation_data(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            source, binary = fixture()
            source['extras'] = {'private': 'not-runtime'}
            source['images'] = [{'uri': 'eye_png.png'}]
            source['textures'] = [{'source': 0}]
            source['materials'] = [{'pbrMetallicRoughness': {
                'baseColorTexture': {'index': 0},
            }}]
            source['meshes'][0]['primitives'] = [{
                'attributes': {'POSITION': 2}, 'indices': 0, 'material': 0,
            }]
            (root / 'eye.png').write_bytes(b'\x89PNG\r\n\x1a\n')
            (root / 'data.bin').write_bytes(binary)
            (root / 'model.gltf').write_text(json.dumps(source))
            (root / 'library.gltf').write_text(json.dumps(source))
            before = (root / 'model.gltf').read_bytes()
            result = q.build(root / 'model.gltf', root / 'library.gltf',
                             outfit_path=None, hair_path=None, strafe_path=None)
            (root / 'result.glb').write_bytes(result)
            document, buffers = q.read_asset(root / 'result.glb')
            self.assertEqual(len(result), struct.unpack_from('<I', result, 8)[0])
            self.assertEqual(before, (root / 'model.gltf').read_bytes())
            self.assertNotIn('extras', document)
            self.assertNotIn('uri', document['buffers'][0])
            self.assertNotIn('uri', document['images'][0])
            self.assertEqual(document['images'][0]['mimeType'], 'image/png')
            self.assertEqual(document['skins'][0]['joints'], source['skins'][0]['joints'])
            self.assertEqual(
                q.values(document, buffers, document['skins'][0]['inverseBindMatrices']),
                q.values(source, [binary], source['skins'][0]['inverseBindMatrices']))
            self.assertEqual([a['name'] for a in document['animations']],
                             [name for name, _, _ in q.CLIPS])
            for animation in document['animations']:
                for sampler in animation['samplers']:
                    keys = q.values(document, buffers, sampler['input'])
                    self.assertEqual(keys, [(0,), (0.5,)])
                    self.assertEqual(len(q.values(document, buffers, sampler['output'])), 2)

    def test_equipment_merges_meshes_not_a_second_skeleton_and_rejects_bad_rest_pose(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            source, binary = fixture()
            source['nodes'][3]['name'] = 'Hair_Buns'
            source['meshes'][0]['primitives'] = [{
                'attributes': {'POSITION': 2}, 'indices': 0, 'material': 0,
            }]
            source['materials'] = [{'pbrMetallicRoughness': {}}]
            (root / 'data.bin').write_bytes(binary)
            path = root / 'hair.gltf'
            path.write_text(json.dumps(source))
            target = deepcopy(source)
            buffers = [binary]
            original_nodes, original_accessors = len(target['nodes']), len(target['accessors'])
            q.attach_meshes(target, buffers, path)
            self.assertEqual(len(target['nodes']), original_nodes + 1)
            self.assertEqual(len(target['skins']), 1)
            self.assertEqual(target['nodes'][-1]['skin'], 0)
            self.assertEqual(target['nodes'][-1]['mesh'], 1)
            self.assertEqual(target['meshes'][-1]['primitives'][0]['attributes']['POSITION'],
                             original_accessors + 2)
            self.assertEqual(q.values(target, buffers, original_accessors + 2),
                             q.values(source, [binary], 2))
            source['nodes'][1]['translation'][2] += 0.1
            path.write_text(json.dumps(source))
            with self.assertRaises(ValueError):
                q.attach_meshes(target, buffers, path)

    def test_head_crop_removes_body_triangles_but_keeps_neck_face_and_skin(self):
        positions = [(0, 1.6, 0), (0.1, 1.7, 0), (0.2, 1.6, 0),
                     (0, 1.47, 0), (0.1, 1.55, 0), (0.2, 1.5, 0),
                     (0, 0, 0), (0.1, 1.0, 0), (0.2, 1.4, 0)]
        data = b''.join(struct.pack('<fff', *p) for p in positions)
        indices = struct.pack('<9H', *range(9))
        document = {
            'meshes': [{'name': 'Superhero_Female', 'primitives': [{
                'attributes': {'POSITION': 0}, 'indices': 1,
            }]}],
            'nodes': [{'name': 'Superhero_Female', 'mesh': 0, 'skin': 0}],
            'bufferViews': [{'buffer': 0, 'byteLength': len(data)},
                            {'buffer': 1, 'byteLength': len(indices)}],
            'accessors': [{'bufferView': 0, 'componentType': 5126,
                           'count': 9, 'type': 'VEC3'},
                          {'bufferView': 1, 'componentType': 5123,
                           'count': 9, 'type': 'SCALAR'}],
        }
        buffers = [data, indices]
        q.head_only(document, buffers)
        primitive = document['meshes'][0]['primitives'][0]
        self.assertEqual(q.mesh_values(document, buffers, primitive['indices']),
                         [(i,) for i in range(6)])
        self.assertEqual(document['nodes'][0]['skin'], 0)
        self.assertEqual(document['nodes'][0]['name'], 'Human_Head_Neck')

    def test_compaction_drops_unused_data_and_deduplicates_image_texture_and_sampler_records(self):
        document, binary = fixture()
        dead = b'UNUSED-MESH-AND-TEXTURE'
        pixels = b'LIVE-PNG'
        image_view = len(document['bufferViews'])
        document['bufferViews'].extend([
            {'buffer': 0, 'byteOffset': len(binary), 'byteLength': len(pixels)},
            {'buffer': 0, 'byteOffset': len(binary) + len(pixels), 'byteLength': len(dead)},
        ])
        binary += pixels + dead
        document['images'] = [
            {'mimeType': 'image/png', 'bufferView': image_view},
            {'mimeType': 'image/png', 'bufferView': image_view},
            {'mimeType': 'image/png', 'bufferView': image_view + 1},
        ]
        document['samplers'] = [{'wrapS': 10497}, {'wrapS': 10497}]
        document['textures'] = [{'source': 0, 'sampler': 0},
                                {'source': 1, 'sampler': 1}, {'source': 2}]
        document['materials'] = [
            {'normalTexture': {'index': 1}, 'pbrMetallicRoughness': {
                'baseColorTexture': {'index': 0}}},
            {'pbrMetallicRoughness': {'baseColorTexture': {'index': 2}}},
        ]
        document['meshes'][0]['primitives'] = [{
            'attributes': {'POSITION': 2}, 'indices': 0, 'material': 0,
        }]
        document['meshes'].append({'name': 'unused hood', 'primitives': []})
        packed = q.compact(document, binary)
        self.assertNotIn(dead, packed)
        for key in ('images', 'textures', 'samplers', 'materials', 'meshes'):
            self.assertEqual(len(document[key]), 1, key)
        self.assertEqual(document['textures'][0], {'source': 0, 'sampler': 0})
        for _, info in q.texture_infos(document['materials'][0]):
            self.assertEqual(info['index'], 0)
        index = document['skins'][0]['inverseBindMatrices']
        self.assertEqual(q.values(document, [packed], index),
                         [tuple(1.0 if i % 5 == 0 else 0.0 for i in range(16))] * 3)

    def test_offline_texture_processor_caches_by_pixels_and_color_space(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            source, binary = fixture()
            source['images'] = [{'uri': 'test.png'} for _ in range(3)]
            source['textures'] = [{'source': i} for i in range(3)]
            source['materials'] = [{
                'pbrMetallicRoughness': {'baseColorTexture': {'index': 0}},
                'normalTexture': {'index': 1}, 'emissiveTexture': {'index': 2},
            }]
            source['meshes'][0]['primitives'] = [{
                'attributes': {'POSITION': 2}, 'indices': 0, 'material': 0,
            }]
            (root / 'test.png').write_bytes(b'\x89PNG\r\n\x1a\n')
            (root / 'data.bin').write_bytes(binary)
            (root / 'model.gltf').write_text(json.dumps(source))
            calls = []
            def process(data, kind):
                calls.append(kind)
                return data + kind.encode()
            result = q.build(root / 'model.gltf', root / 'model.gltf',
                             outfit_path=None, hair_path=None, texture_processor=process,
                             strafe_path=None)
            (root / 'output.glb').write_bytes(result)
            document, _ = q.read_asset(root / 'output.glb')
            self.assertEqual(calls, ['color', 'normal'])
            self.assertEqual(len(document['images']), 2)
            self.assertEqual(len(document['textures']), 2)

    def test_distinct_strafe_library_is_required_and_mixed_in_canonical_order(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            document, binary = fixture()
            main = deepcopy(document)
            main['animations'] = [a for a in main['animations']
                                  if not a['name'].startswith('Strafe_')]
            lateral = deepcopy(document)
            lateral['animations'] = [a for a in lateral['animations']
                                     if a['name'].startswith('Strafe_')]
            (root / 'data.bin').write_bytes(binary)
            (root / 'base.gltf').write_text(json.dumps(document))
            (root / 'ual1.gltf').write_text(json.dumps(main))
            (root / 'strafes.gltf').write_text(json.dumps(lateral))
            result = q.build(root / 'base.gltf', root / 'ual1.gltf',
                             outfit_path=None, hair_path=None, strafe_path=root / 'strafes.gltf')
            (root / 'result.glb').write_bytes(result)
            output, _ = q.read_asset(root / 'result.glb')
            self.assertEqual([a['name'] for a in output['animations']],
                             [name for name, _, _ in q.CLIPS])
            with self.assertRaises(FileNotFoundError):
                q.build(root / 'base.gltf', root / 'ual1.gltf',
                        outfit_path=None, hair_path=None, strafe_path=root / 'missing.glb')

    def test_constant_step_keys_are_valid_but_animated_step_keys_are_rejected(self):
        document, binary = fixture()
        animation = document['animations'][0]
        animation['samplers'][0]['interpolation'] = 'STEP'
        selected = (('Idle', animation['name'], False),)
        with self.assertRaises(ValueError):
            list(q.retarget(document, document, [binary], clips=selected))
        accessor = document['accessors'][animation['samplers'][0]['output']]
        view = document['bufferViews'][accessor['bufferView']]
        data = bytearray(binary)
        offset = view.get('byteOffset', 0)
        data[offset:offset + 32] = struct.pack('<8f', 0, 0, 0, 1, 0, 0, 0, 1)
        tracks = dict(q.retarget(document, document, [bytes(data)], clips=selected))['Idle']
        self.assertEqual(tracks[0][3], [(0, 0, 0, 1), (0, 0, 0, 1)])


if __name__ == '__main__':
    unittest.main()
