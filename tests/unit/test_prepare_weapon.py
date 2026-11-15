"""Renderer-free regressions for the actual Blender export/build adaptation."""

import importlib.util
import json
from pathlib import Path
import struct
import unittest


ROOT = Path(__file__).resolve().parents[2]
spec = importlib.util.spec_from_file_location('prepare_weapon', ROOT / 'scripts/prepare_weapon.py')
prepare = importlib.util.module_from_spec(spec)
spec.loader.exec_module(prepare)


def document(data):
    size = struct.unpack_from('<I', data, 12)[0]
    return json.loads(data[20:20 + size]), data[20 + size:]


class WeaponExportTests(unittest.TestCase):
    def test_staff_keeps_only_complete_weapon_hierarchy_and_preserves_source(self):
        path = ROOT / 'assets/Aetherthorn.glb'
        source = path.read_bytes()
        original, binary = document(source)
        result = prepare.staff_glb(source)
        game, game_binary = document(result)
        self.assertEqual(len(result), struct.unpack_from('<I', result, 8)[0])
        self.assertEqual(binary, game_binary)
        self.assertEqual(game['materials'], original['materials'])
        self.assertEqual(game['scenes'][0]['nodes'], [0])
        self.assertEqual(game['nodes'][0]['name'], 'Crystal Staff')
        children = game['nodes'][0]['children']
        self.assertEqual(children, list(range(1, len(game['nodes']))))
        names = {node['name'] for node in game['nodes']}
        self.assertTrue({'Primary six sided power crystal', 'Gold pointed ferrule', 'Long wrapped handgrip'} <= names)
        self.assertFalse({'Cube', 'Display plinth', 'Plinth gold rim'} & names)
        self.assertTrue(all('mesh' in game['nodes'][i] for i in children))
        self.assertEqual(source, path.read_bytes())

    def test_staff_rejects_missing_root_and_presentation_geometry(self):
        source = (ROOT / 'assets/Aetherthorn.glb').read_bytes()
        original, _ = document(source)
        for replacement in ['Unrelated', 'Display plinth']:
            data = json.loads(json.dumps(original))
            node = next(n for n in data['nodes'] if n['name'] == ('Crystal Staff' if replacement == 'Unrelated' else 'Tapered lacquer shaft'))
            node['name'] = replacement
            payload = json.dumps(data).encode()
            payload += b' ' * (-len(payload) % 4)
            wrong = struct.pack('<IIIII', 0x46546C67, 2, 20 + len(payload), len(payload), 0x4E4F534A) + payload
            with self.assertRaises(ValueError):
                prepare.staff_glb(wrong)

    def test_actual_export_keeps_weapon_materials_and_binary_but_no_studio(self):
        source = (ROOT / prepare.SOURCE).read_bytes()
        original, binary = document(source)
        result = prepare.weapon_glb(source)
        game, game_binary = document(result)
        self.assertEqual(binary, game_binary)
        self.assertEqual(len(result), struct.unpack_from('<I', result, 8)[0])
        self.assertEqual(len(game['scenes']), 1)
        self.assertEqual(game['scene'], 0)
        self.assertEqual(game['scenes'][0]['nodes'], list(range(len(game['nodes']))))
        names = {node['name'] for node in game['nodes']}
        self.assertTrue({'Central handgrip', 'Dawn swept blade', 'Dusk swept blade'} <= names)
        self.assertFalse({'Cube', 'Backdrop', 'Hero camera'} & names)
        self.assertTrue(all('mesh' in n and 'camera' not in n and 'children' not in n for n in game['nodes']))
        self.assertEqual(game['materials'], original['materials'])
        self.assertEqual(game['extensionsUsed'], original['extensionsUsed'])
        bounds = [game['accessors'][p['attributes']['POSITION']] for n in game['nodes'] for p in game['meshes'][n['mesh']]['primitives']]
        self.assertAlmostEqual(max(a['max'][0] for a in bounds) - min(a['min'][0] for a in bounds), 9.3, places=5)
        self.assertEqual(source, (ROOT / prepare.SOURCE).read_bytes())

    def test_invalid_export_is_rejected_instead_of_shipping_the_wrong_scene(self):
        with self.assertRaises(ValueError):
            prepare.weapon_glb(struct.pack('<III', 0, 2, 12))
        source = (ROOT / prepare.SOURCE).read_bytes()
        original, _ = document(source)
        for scene in original['scenes']:
            scene['name'] = 'Unrelated scene'
        payload = json.dumps(original).encode()
        payload += b' ' * (-len(payload) % 4)
        wrong = struct.pack('<IIIII', 0x46546C67, 2, 20 + len(payload), len(payload), 0x4E4F534A) + payload
        with self.assertRaisesRegex(ValueError, 'named Verdant Eclipse scene'):
            prepare.weapon_glb(wrong)
