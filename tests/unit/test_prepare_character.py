"""Synthetic GLBs: no dependency on the user's unpublished character asset."""
import importlib.util
import json
from pathlib import Path
import struct
import tempfile
import unittest


spec = importlib.util.spec_from_file_location(
    'prepare_character', Path(__file__).resolve().parents[2] / 'scripts/prepare_character.py')
module = importlib.util.module_from_spec(spec)
spec.loader.exec_module(module)


def fixture():
    return {
        'asset': {'version': '2.0'},
        'scene': 1,
        'scenes': [{'nodes': [3]}, {'nodes': [0]}],
        'nodes': [
            {'name': 'BaseMale_Rig', 'children': [1, 2]},
            {'name': 'root'},
            {'name': 'Body_Base', 'mesh': 0, 'skin': 0},
            {'name': 'Studio_Ground'},
        ],
        'skins': [{'joints': [1]}],
        'meshes': [{'primitives': []}],
        'extras': {'authoring_path': 'private-path'},
        'animations': [
            {'name': name, 'extras': {'fps': 30}, 'channels': [
                {'sampler': 0, 'target': {'node': 1, 'path': 'translation'}},
                {'sampler': 1, 'target': {'node': 1, 'path': 'rotation'}},
            ], 'samplers': [{'input': 0, 'output': 1}, {'input': 0, 'output': 2}]}
            for name in reversed(module.CLIPS)
        ],
    }


def encode(document):
    payload = json.dumps(document).encode()
    payload += b' ' * (-len(payload) % 4)
    tail = struct.pack('<II', 4, 0x004E4942) + b'\x00\x01\x02\x03'
    return (struct.pack('<III', 0x46546C67, 2, 20 + len(payload) + len(tail)) +
            struct.pack('<II', len(payload), 0x4E4F534A) + payload + tail)


def decode(source):
    size = struct.unpack_from('<I', source, 12)[0]
    return json.loads(source[20:20 + size]), source[20 + size:]


class CharacterPreparation(unittest.TestCase):
    def test_scene_clip_order_root_motion_and_binary_preservation(self):
        source = encode(fixture())
        original = bytes(source)
        result = module.character_glb(source)
        doc, tail = decode(result)
        self.assertEqual(source, original)
        self.assertEqual(tail, decode(source)[1])
        self.assertEqual(struct.unpack_from('<I', result, 8)[0], len(result))
        self.assertEqual(doc['scene'], 0)
        self.assertEqual(doc['scenes'], [{'name': 'BaseMale | Character only', 'nodes': [0]}])
        self.assertEqual(doc['skins'], fixture()['skins'])
        self.assertEqual([a['name'] for a in doc['animations']], list(module.CLIPS))
        self.assertNotIn('extras', doc)
        for animation in doc['animations']:
            self.assertNotIn('extras', animation)
            paths = [c['target']['path'] for c in animation['channels']]
            self.assertEqual(paths, ['rotation'] if animation['name'] == 'Jump'
                             else ['translation', 'rotation'])

    def test_rejects_missing_duplicate_clips_or_wrong_rig(self):
        for change in ('missing', 'duplicate', 'rig'):
            with self.subTest(change=change):
                doc = fixture()
                if change == 'missing':
                    doc['animations'].pop()
                elif change == 'duplicate':
                    doc['animations'].append(doc['animations'][0])
                else:
                    doc['nodes'][0]['name'] = 'Other_Rig'
                with self.assertRaises(ValueError):
                    module.character_glb(encode(doc))

    def test_rejects_presentation_children_cycles_and_outside_targets_or_joints(self):
        for change in ('studio', 'camera', 'cycle', 'animation', 'skin', 'unskinned'):
            with self.subTest(change=change):
                doc = fixture()
                if change == 'studio':
                    doc['nodes'][0]['children'].append(3)
                elif change == 'camera':
                    doc['nodes'][2]['camera'] = 0
                elif change == 'cycle':
                    doc['nodes'][1]['children'] = [0]
                elif change == 'animation':
                    doc['animations'][0]['channels'][0]['target']['node'] = 3
                elif change == 'skin':
                    doc['skins'][0]['joints'].append(3)
                else:
                    del doc['nodes'][2]['skin']
                with self.assertRaises(ValueError):
                    module.character_glb(encode(doc))

    def test_rejects_incomplete_glbs(self):
        source = encode(fixture())
        for invalid in (b'', source[:18], source[:-4], b'nope' + source[4:]):
            with self.assertRaises(ValueError):
                module.character_glb(invalid)

    def test_optional_source_manifest_and_stale_output(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            source, directory = root / 'BaseMale.glb', root / 'public'
            source.write_bytes(encode(fixture()))
            module.prepare(source, directory)
            target = directory / module.MODEL
            self.assertTrue(target.is_file())
            self.assertEqual(json.loads((directory / 'character.json').read_text()),
                             {'available': True})
            source.unlink()
            module.prepare(source, directory)
            self.assertFalse(target.exists())
            self.assertEqual(json.loads((directory / 'character.json').read_text()),
                             {'available': False})


if __name__ == '__main__':
    unittest.main()
