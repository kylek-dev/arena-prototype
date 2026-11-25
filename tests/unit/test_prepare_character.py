"""Synthetic GLBs: no dependency on the user's unpublished character assets."""
import importlib.util
import json
import os
import sys
from pathlib import Path
import struct
import tempfile
import unittest
from unittest.mock import patch


sys.path.insert(0, str(Path(__file__).resolve().parents[2] / 'scripts'))
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

    def test_named_rig_is_required_for_each_character(self):
        doc = fixture()
        doc['nodes'][0]['name'] = 'BaseFemale_Rig'
        result, _ = decode(module.character_glb(encode(doc), 'BaseFemale'))
        self.assertEqual(result['scenes'],
                         [{'name': 'BaseFemale | Character only', 'nodes': [0]}])
        with self.assertRaises(ValueError):
            module.character_glb(encode(doc), 'BaseMale')
        with self.assertRaises(ValueError):
            module.character_glb(encode(fixture()), 'BaseFemale')

    def test_first_installed_character_manifest_and_stale_output(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            female, male = root / 'BaseFemale.glb', root / 'BaseMale.glb'
            characters = (('base-female', female, 'BaseFemale'),
                          ('base-male', male, 'BaseMale'))
            directory = root / 'public'
            female_doc = fixture()
            female_doc['nodes'][0]['name'] = 'BaseFemale_Rig'

            def outputs():
                manifest = json.loads((directory / 'character.json').read_text())
                present = [i for i, _, _ in characters
                           if (directory / module.model_path(i)).is_file()]
                return manifest, present

            male.write_bytes(encode(fixture()))
            module.prepare(characters, directory, enabled=True)
            self.assertEqual(outputs(), ({'available': True, 'model': 'base-male'},
                                         ['base-male']))
            female.write_bytes(encode(female_doc))
            module.prepare(characters, directory, enabled=True)
            self.assertEqual(outputs(), ({'available': True, 'model': 'base-female'},
                                         ['base-female']))
            result, _ = decode((directory / module.model_path('base-female')).read_bytes())
            self.assertEqual(result['scenes'][0]['name'], 'BaseFemale | Character only')
            female.unlink()
            male.unlink()
            module.prepare(characters, directory, enabled=True)
            self.assertEqual(outputs(), ({'available': False, 'model': None}, []))

    def test_malformed_preferred_character_fails_instead_of_falling_back(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            female, male = root / 'BaseFemale.glb', root / 'BaseMale.glb'
            female.write_bytes(encode(fixture()))  # Wrong rig name.
            male.write_bytes(encode(fixture()))
            with self.assertRaises(ValueError):
                module.prepare((('base-female', female, 'BaseFemale'),
                                ('base-male', male, 'BaseMale')), root / 'public',
                               enabled=True)

    def test_explicitly_disabled_removes_outputs_even_when_sources_exist(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            female, male = root / 'BaseFemale.glb', root / 'BaseMale.glb'
            female_doc = fixture()
            female_doc['nodes'][0]['name'] = 'BaseFemale_Rig'
            female.write_bytes(encode(female_doc))
            male.write_bytes(encode(fixture()))
            characters = (('base-female', female, 'BaseFemale'),
                          ('base-male', male, 'BaseMale'))
            directory = root / 'public'
            module.prepare(characters, directory, enabled=True)
            self.assertTrue((directory / module.model_path('base-female')).is_file())
            environment = dict(os.environ)
            try:
                os.environ[module.ENV] = '0'
                module.prepare(characters, directory)
            finally:
                os.environ.clear()
                os.environ.update(environment)
            self.assertEqual(json.loads((directory / 'character.json').read_text()),
                             {'available': False, 'model': None})
            for identifier, _, _ in characters:
                self.assertFalse((directory / module.model_path(identifier)).exists())

    def test_default_quaternius_selection_and_procedural_override(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            model, library = root / 'female.gltf', root / 'library.glb'
            model.touch()
            library.touch()
            directory = root / 'public'
            with patch.dict(os.environ, {}, clear=True), \
                    patch.object(module.quaternius_character, 'MODEL', model), \
                    patch.object(module.quaternius_character, 'LIBRARY', library), \
                    patch.object(module.character_kits, 'prepare',
                                 return_value=[{'id': 'human-female'}]) as kits:
                module.prepare(directory=directory)
                self.assertEqual(json.loads((directory / 'character.json').read_text()),
                                 {'available': True, 'model': 'quaternius-kits',
                                  'kits': [{'id': 'human-female'}]})
                kits.assert_called_once_with(directory)
                self.assertFalse((directory / module.model_path('quaternius-female')).exists())
                output = directory / module.character_kits.DIRECTORY
                output.mkdir(parents=True)
                os.environ[module.ENV] = '0'
                module.prepare(directory=directory)
                self.assertFalse(output.exists())
                self.assertEqual(json.loads((directory / 'character.json').read_text()),
                                 {'available': False, 'model': None})

    def test_default_without_quaternius_does_not_ship_unconfirmed_legacy_sources(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            legacy = root / 'legacy.glb'
            doc = fixture()
            doc['nodes'][0]['name'] = 'BaseFemale_Rig'
            legacy.write_bytes(encode(doc))
            characters = (('base-female', legacy, 'BaseFemale'),)
            with patch.dict(os.environ, {}, clear=True), \
                    patch.object(module, 'CHARACTERS', characters), \
                    patch.object(module.quaternius_character, 'MODEL', root / 'absent'), \
                    patch.object(module.character_kits, 'prepare', return_value=[]):
                module.prepare(characters=characters, directory=root / 'public')
                self.assertEqual(
                    json.loads((root / 'public/character.json').read_text()),
                    {'available': False, 'model': None})

    def test_finished_kits_ship_without_quaternius_sources(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            directory = root / 'public'
            finished = [{'id': 'meshy-human-male'}]
            with patch.dict(os.environ, {}, clear=True), \
                    patch.object(module.quaternius_character, 'MODEL', root / 'absent'), \
                    patch.object(module.quaternius_character, 'LIBRARY', root / 'absent'), \
                    patch.object(module.character_kits, 'prepare', return_value=finished):
                module.prepare(directory=directory)
            self.assertEqual(json.loads((directory / 'character.json').read_text()),
                             {'available': True, 'model': 'quaternius-kits', 'kits': finished})

if __name__ == '__main__':
    unittest.main()
