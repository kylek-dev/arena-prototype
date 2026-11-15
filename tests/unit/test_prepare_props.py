"""Prop catalog regressions: bounds math and the tracked catalog's freshness."""

import importlib.util
from pathlib import Path
import unittest


ROOT = Path(__file__).resolve().parents[2]
spec = importlib.util.spec_from_file_location('prepare_props', ROOT / 'scripts/prepare_props.py')
props = importlib.util.module_from_spec(spec)
spec.loader.exec_module(props)


def document(node):
    return {
        'scenes': [{'nodes': [0]}],
        'nodes': [node],
        'meshes': [{'primitives': [{'attributes': {'POSITION': 0}}]}],
        'accessors': [{'min': [-1, 0, -0.5], 'max': [1, 2, 0.5]}],
    }


class PropCatalogTests(unittest.TestCase):
    def test_bounds_follow_node_rotation_scale_and_children(self):
        self.assertEqual(props.bounds(document({'mesh': 0})), ([-1.0, 0.0, -0.5], [1.0, 2.0, 0.5]))
        # A quarter turn about Y swaps the X and Z extents; scale doubles them.
        half = 0.5 ** 0.5
        turned = props.bounds(document({'mesh': 0, 'rotation': [0, half, 0, half],
                                         'scale': [2, 2, 2], 'translation': [0, 1, 0]}))
        self.assertEqual(turned, ([-1.0, 1.0, -2.0], [1.0, 5.0, 2.0]))
        parent = document({'children': [1], 'translation': [3, 0, 0]})
        parent['nodes'].append({'mesh': 0})
        self.assertEqual(props.bounds(parent)[0], [2.0, 0.0, -0.5])

    def test_runtime_materials_cap_metalness_only(self):
        source = {'materials': [
            {'name': 'Metal', 'pbrMetallicRoughness': {'baseColorTexture': {'index': 1}}},
            {'name': 'Page', 'pbrMetallicRoughness': {'metallicFactor': 0}},
            {'name': 'Bare'},
        ]}
        materials = props.runtime_gltf(source)['materials']
        self.assertEqual([m['pbrMetallicRoughness']['metallicFactor'] for m in materials],
                         [props.METALNESS, 0, props.METALNESS])
        self.assertEqual(materials[0]['pbrMetallicRoughness']['baseColorTexture'], {'index': 1})
        self.assertNotIn('baseColorFactor', materials[0]['pbrMetallicRoughness'])
        iron = props.runtime_gltf({'materials': [
            {'name': 'MI_Trim_Metal'}, {'name': 'MI_Trim_Metal_Vertex'}]})['materials']
        self.assertEqual(iron[0]['pbrMetallicRoughness']['baseColorFactor'], props.IRON)
        self.assertNotIn('baseColorFactor', iron[1]['pbrMetallicRoughness'])

    def test_categories_and_labels(self):
        self.assertEqual(props.category('Potion_1'), 'Small items')
        self.assertEqual(props.category('Pot_1_Lid'), 'Containers')
        self.assertEqual(props.category('Banner_2_Cloth'), 'Banners & lights')
        self.assertEqual(props.label('Table_Large'), 'Table Large')

    @unittest.skipUnless(props.SOURCE.is_dir(), 'props pack not present')
    def test_tracked_catalog_matches_the_sources(self):
        self.assertEqual(props.CATALOG.read_text(), props.catalog_text())


if __name__ == '__main__':
    unittest.main()
