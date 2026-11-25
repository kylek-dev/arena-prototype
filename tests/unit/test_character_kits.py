"""Synthetic glTFs exercise kit assembly helpers without downloaded assets."""
import importlib.util
import json
from pathlib import Path
import struct
import sys
import tempfile
import unittest

SCRIPTS = Path(__file__).resolve().parents[2] / 'scripts'
sys.path.insert(0, str(SCRIPTS))
sys.dont_write_bytecode = True
spec = importlib.util.spec_from_file_location('character_kits', SCRIPTS / 'character_kits.py')
k = importlib.util.module_from_spec(spec)
spec.loader.exec_module(k)
q = k.q

IDENTITY = tuple(1.0 if i % 5 == 0 else 0.0 for i in range(16))
SHIFTED = IDENTITY[:12] + (0.0, -0.5, 0.0, 1.0)


def skeleton(binary, document, matrices=IDENTITY):
    """root > pelvis > (spine_03 > Head, thigh_l > calf_l > foot_l)."""
    names = ['root', 'pelvis', 'spine_03', 'Head', 'thigh_l', 'calf_l', 'foot_l']
    children = {0: [1], 1: [2, 4], 2: [3], 4: [5], 5: [6]}
    document['nodes'] = [{'name': n, 'translation': [0, 0, 0.1 * i],
                          **({'children': children[i]} if i in children else {})}
                         for i, n in enumerate(names)]
    document['nodes'].append({'name': 'Armature', 'children': [0]})
    document['scenes'] = [{'nodes': [7]}]
    data = struct.pack('<' + 'f' * 16 * len(names), *(matrices * len(names)))
    document['bufferViews'].append({'buffer': 0, 'byteOffset': len(binary),
                                    'byteLength': len(data)})
    binary.extend(data)
    document['accessors'].append({'bufferView': len(document['bufferViews']) - 1,
                                  'componentType': 5126, 'type': 'MAT4', 'count': len(names)})
    document['skins'] = [{'joints': list(range(len(names))),
                          'inverseBindMatrices': len(document['accessors']) - 1}]


def add(binary, document, values, fmt, kind, component):
    data = b''.join(struct.pack('<' + fmt, *v) for v in values)
    document['bufferViews'].append({'buffer': 0, 'byteOffset': len(binary),
                                    'byteLength': len(data)})
    binary.extend(data)
    accessor = {'bufferView': len(document['bufferViews']) - 1, 'componentType': component,
                'type': kind, 'count': len(values)}
    if kind == 'VEC3':
        accessor['min'] = [min(v[i] for v in values) for i in range(3)]
        accessor['max'] = [max(v[i] for v in values) for i in range(3)]
    document['accessors'].append(accessor)
    return len(document['accessors']) - 1


def body_document():
    """Four triangles weighted to Head, spine, calf (high and low) and foot."""
    binary, document = bytearray(), {'asset': {'version': '2.0'}, 'bufferViews': [],
                                     'accessors': [], 'meshes': [], 'materials': [
                                         {'name': 'MI_Superhero_Female'}]}
    skeleton(binary, document)
    positions, joints, indices = [], [], []
    for joint, y in [(3, 1.7), (2, 1.3), (5, 0.6), (5, 0.3), (6, 0.05)]:
        base = len(positions)
        positions += [(0, y, 0), (0.1, y, 0), (0, y + 0.01, 0)]
        joints += [(joint, 0, 0, 0)] * 3
        indices += [base, base + 1, base + 2]
    primitive = {'attributes': {
        'POSITION': add(binary, document, positions, '3f', 'VEC3', 5126),
        'JOINTS_0': add(binary, document, joints, '4B', 'VEC4', 5121),
        'WEIGHTS_0': add(binary, document, [(1, 0, 0, 0)] * len(positions), '4f', 'VEC4', 5126)},
        'indices': add(binary, document, [(i,) for i in indices], 'I', 'SCALAR', 5125),
        'material': 0}
    document['meshes'].append({'name': 'Body', 'primitives': [primitive]})
    document['nodes'].append({'name': 'Superhero_Female', 'mesh': 0, 'skin': 0})
    document['nodes'][7]['children'].append(len(document['nodes']) - 1)
    return document, [bytes(binary)]


def part_file(root, name, matrices=IDENTITY, node='Hair_Long'):
    binary, document = bytearray(), {'asset': {'version': '2.0'}, 'bufferViews': [],
                                     'accessors': [], 'materials': [{'name': 'MI_Hair_2'}]}
    skeleton(binary, document, matrices)
    primitive = {'attributes': {
        'POSITION': add(binary, document, [(0, 1.8, 0), (0.1, 1.8, 0), (0, 1.9, 0)],
                        '3f', 'VEC3', 5126)},
        'indices': add(binary, document, [(0,), (1,), (2,)], 'I', 'SCALAR', 5125),
        'material': 0}
    document['meshes'] = [{'name': node, 'primitives': [primitive]}]
    document['nodes'].append({'name': node, 'mesh': 0, 'skin': 0})
    document['buffers'] = [{'uri': f'{name}.bin', 'byteLength': len(binary)}]
    (root / f'{name}.bin').write_bytes(binary)
    path = root / f'{name}.gltf'
    path.write_text(json.dumps(document))
    return path


class KitTests(unittest.TestCase):
    def test_outfit_pieces_get_gender_neutral_names_and_unknown_pieces_fail(self):
        self.assertEqual(k.piece_name('Female_Ranger_Acc_Pauldrons'), 'Ranger_Pauldrons')
        self.assertEqual(k.piece_name('Male_Ranger_Acc_Pauldron'), 'Ranger_Pauldrons')
        self.assertEqual(k.piece_name('Male_Ranger_Feet_Boots'), 'Ranger_Feet')
        self.assertEqual(k.piece_name('Female_Ranger_Arms_Bracer'), 'Ranger_Bracers')
        self.assertEqual(k.piece_name('Male_Ranger_Body_Belt_1.001'), 'Ranger_Belt_1')
        self.assertEqual(k.piece_name('Female_Peasant_Legs'), 'Peasant_Legs')
        with self.assertRaises(ValueError):
            k.piece_name('Female_Ranger_Cape')

    def test_body_split_by_dominant_bone_and_calf_height(self):
        document, buffers = body_document()
        buffers = list(buffers)
        created = k.split_body(document, buffers, 'Superhero_Female')
        self.assertEqual(created, ['Body_Head', 'Body_Torso', 'Body_Calves',
                                   'Body_Shins', 'Body_Feet'])
        nodes = {n['name']: n for n in document['nodes'] if 'mesh' in n}
        self.assertNotIn('Superhero_Female', nodes)
        self.assertFalse(any(n.get('name') == 'Superhero_Female' for n in document['nodes']))
        meshes = {name: document['meshes'][n['mesh']] for name, n in nodes.items()}
        # Index-only: all regions share the body's vertex attributes and skin.
        attributes = {json.dumps(m['primitives'][0]['attributes']) for m in meshes.values()}
        self.assertEqual(len(attributes), 1)
        self.assertTrue(all(n['skin'] == 0 for n in nodes.values()))
        triangles = {name: [v[0] for v in q.mesh_values(document, buffers,
                                                          m['primitives'][0]['indices'])]
                     for name, m in meshes.items()}
        self.assertEqual(triangles['Body_Head'], [0, 1, 2])
        self.assertEqual(triangles['Body_Calves'], [6, 7, 8])
        self.assertEqual(triangles['Body_Shins'], [9, 10, 11])
        low, high = k.body_bounds(document, buffers)
        self.assertAlmostEqual(low, 0.05, places=6)
        self.assertAlmostEqual(high, 1.71, places=6)

    def test_parts_reuse_or_add_skins_by_bind_pose_and_reject_other_skeletons(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            document, buffers = body_document()
            buffers = list(buffers)
            added = k.attach(document, buffers, part_file(root, 'same'), lambda n: n)
            self.assertEqual(added, ['Hair_Long'])
            self.assertEqual(len(document['skins']), 1)
            k.attach(document, buffers, part_file(root, 'shifted', SHIFTED, 'Hair_Beard'),
                     lambda n: n)
            self.assertEqual(len(document['skins']), 2)
            self.assertEqual(document['skins'][1]['joints'], document['skins'][0]['joints'])
            beard = next(n for n in document['nodes'] if n.get('name') == 'Hair_Beard')
            self.assertEqual(beard['skin'], 1)
            self.assertEqual(q.values(document, buffers,
                                      document['skins'][1]['inverseBindMatrices'])[0], SHIFTED)
            # A second part with the same offset bind pose shares that skin.
            k.attach(document, buffers, part_file(root, 'again', SHIFTED, 'Hair_Buzzed'),
                     lambda n: n)
            self.assertEqual(len(document['skins']), 2)
            # Renaming to None skips a node entirely.
            self.assertEqual(k.attach(document, buffers, part_file(root, 'skip'),
                                      lambda n: None), [])
            # Several skins over one skeleton still retarget as one skeleton.
            self.assertEqual(set(q.joint_map(document)), {
                'root', 'pelvis', 'spine_03', 'Head', 'thigh_l', 'calf_l', 'foot_l'})
            other = json.loads((root / 'same.gltf').read_text())
            other['nodes'][3]['name'] = 'head'
            (root / 'other.gltf').write_text(json.dumps(other))
            (root / 'other.bin').write_bytes((root / 'same.bin').read_bytes())
            other['buffers'][0]['uri'] = 'other.bin'
            with self.assertRaises(ValueError):
                k.attach(document, buffers, root / 'other.gltf', lambda n: n)

    def test_rest_pose_adoption_and_material_slots(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            document, _ = body_document()
            source = json.loads(part_file(root, 'pose').read_text())
            source['nodes'][4]['translation'] = [0.2, 0.0, 0.3]
            (root / 'pose.gltf').write_text(json.dumps(source))
            k.adopt_rest_pose(document, root / 'pose.gltf')
            self.assertEqual(document['nodes'][4]['translation'], [0.2, 0.0, 0.3])
        self.assertEqual({k.MATERIALS[m] for m in ('MI_Superhero_Male', 'MI_Orc_Female')},
                         {'Skin'})
        self.assertEqual(k.MATERIALS['MI_Regular_Female'], 'Skin_Hands')
        # Outfit coverage: legs hide hips/thighs/calves, feet hide shins/feet.
        self.assertEqual(k.PIECES['Legs'], ('legs', ('Hips', 'Thighs', 'Calves')))
        self.assertEqual(k.PIECES['Feet'], ('feet', ('Shins', 'Feet')))
        self.assertEqual(k.PIECES['Belt_1'][1], ())

    def test_duplicate_materials_merge_after_rename(self):
        document = {'materials': [{'name': 'Hair_2', 'x': 1}, {'name': 'Hair_2', 'x': 1},
                                  {'name': 'Hair_2', 'x': 2}],
                    'meshes': [{'primitives': [{'material': 0}, {'material': 1},
                                               {'material': 2}]}]}
        k.deduplicate_materials(document)
        self.assertEqual(len(document['materials']), 2)
        self.assertEqual([p['material'] for p in document['meshes'][0]['primitives']], [0, 0, 1])

    def test_prebuilt_kits_ship_as_built_at_the_shared_scale(self):
        with tempfile.TemporaryDirectory() as temp:
            source, out = Path(temp) / 'source', Path(temp) / 'out'
            out.mkdir()
            self.assertEqual(k.prebuilt(out, source), [])
            entry = {'id': 'hero-male', 'ancestry': 'human', 'body': 'male', 'sole': 0.0,
                     'path': 'characters/kits/hero-male.glb', 'regions': ['Body_Head'],
                     'groups': {}, 'materials': {'Skin': {'tint': 'skin'}}, 'sockets': {}}
            folder = source / 'hero-male'
            folder.mkdir(parents=True)
            (folder / 'kit.json').write_text(json.dumps(entry))
            with self.assertRaisesRegex(ValueError, 'hero-male.glb is missing'):
                k.prebuilt(out, source)
            (folder / 'hero-male.glb').write_bytes(b'glTF-bytes')
            self.assertEqual(k.prebuilt(out, source), [{**entry, 'scale': k.SCALE}])
            self.assertEqual((out / 'hero-male.glb').read_bytes(), b'glTF-bytes')
            for bad in ({**entry, 'path': 'characters/kits/other.glb'}, {**entry, 'id': 'other'},
                        {k_: v for k_, v in entry.items() if k_ != 'regions'}):
                (folder / 'kit.json').write_text(json.dumps(bad))
                with self.assertRaises(ValueError):
                    k.prebuilt(out, source)

    def test_optional_race_kits_are_skipped_when_not_installed(self):
        missing = k.race('elf', 'female')
        missing.base = Path('/nonexistent/Elf_Female.glb')
        self.assertTrue(missing.optional)
        self.assertFalse(k.available(missing))
        with tempfile.TemporaryDirectory() as tmp:
            # No finished kits either: the test must not pick up locally installed ones.
            self.assertEqual(k.prepare(Path(tmp), texture_processor=None, iris_processor=None,
                                       kits=(missing,), prebuilt_path=Path(tmp) / 'none'), [])


if __name__ == '__main__':
    unittest.main()
