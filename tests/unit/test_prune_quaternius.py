"""Cleanup contracts: referenced dependencies and distinct license packs survive."""
import importlib.util
import json
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch


spec = importlib.util.spec_from_file_location(
    'prune_quaternius',
    Path(__file__).resolve().parents[2] / 'scripts/prune_quaternius.py')
p = importlib.util.module_from_spec(spec)
spec.loader.exec_module(p)


class QuaterniusPruning(unittest.TestCase):
    def test_plan_preserves_dependencies_variants_notices_and_qal_models(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            for pack in ('base', 'races'):
                folder = root / pack
                folder.mkdir()
                (folder / 'License.txt').write_text('CC0' if pack == 'base' else 'QAL')
                (folder / 'model.glb').write_bytes(b'model')
                (folder / 'normal.png').write_bytes(b'same pixels')
            folder = root / 'base'
            (folder / 'duplicate.png').write_bytes(b'same pixels')
            (folder / 'variant.png').write_bytes(b'different pixels')
            (folder / 'model.bin').write_bytes(b'mesh')
            (folder / 'model.fbx').write_bytes(b'duplicate mesh')
            (folder / 'model.gltf').write_text(json.dumps({
                'buffers': [{'uri': 'model.bin'}],
                'images': [{'uri': 'normal_png.png'}],
            }))
            with patch.object(p, 'ROOT', root):
                remove, repairs = p.plan()
            self.assertEqual(set(remove), {folder / 'model.fbx',
                                           folder / 'duplicate.png'})
            self.assertEqual(
                json.loads(repairs[folder / 'model.gltf'])['images'],
                [{'uri': 'normal.png'}])
            self.assertTrue((folder / 'model.fbx').exists(), 'dry-run must not delete')
            self.assertIn('normal_png.png', (folder / 'model.gltf').read_text())

    def test_missing_dependency_aborts_instead_of_pruning_an_incomplete_pack(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            (root / 'model.gltf').write_text(
                json.dumps({'buffers': [{'uri': 'missing.bin'}]}))
            with patch.object(p, 'ROOT', root), self.assertRaises(ValueError):
                p.plan()


if __name__ == '__main__':
    unittest.main()
