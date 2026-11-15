"""Contracts for the small, reusable animation source shipped with the project."""
from pathlib import Path
import math
import sys
import unittest

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / 'scripts'))
import quaternius_character as q
from animation_contracts import source_tracks, world_pose


class UniversalStrafes(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.document, cls.buffers = q.read_asset(ROOT / q.STRAFE_LIBRARY)
        cls.joints = q.joint_map(cls.document)
        cls.rest = world_pose(cls.document)

    def test_library_is_standalone_and_contains_only_shared_rig_and_two_clips(self):
        d = self.document
        self.assertEqual(len(d['scenes']), 1)
        self.assertEqual(len(d['skins']), 1)
        self.assertEqual(len(self.joints), 65)
        self.assertEqual(len(d['meshes']), 1)
        self.assertEqual(len(d.get('images', [])), 0)
        self.assertEqual(len(d.get('materials', [])), 0)
        self.assertEqual([a['name'] for a in d['animations']], ['Strafe_Left', 'Strafe_Right'])
        names = {n['name'] for n in d['nodes']} - set(self.joints)
        self.assertEqual(names, {'Universal_Strafe_Rig', 'Universal_Animation_Carrier'})
        for buffer in d['buffers']:
            self.assertNotIn('uri', buffer)
        carrier = d['meshes'][0]['primitives'][0]
        self.assertEqual(d['accessors'][carrier['attributes']['POSITION']]['count'], 3)
        self.assertLess((ROOT / q.STRAFE_LIBRARY).stat().st_size, 200_000)

    def test_loops_close_root_does_not_travel_and_feet_never_cross(self):
        for name in ('Strafe_Left', 'Strafe_Right'):
            tracks = source_tracks(self.document, self.buffers, name)
            self.assertAlmostEqual(max(t[-1] for _, _, t, _ in tracks), 0.8, places=6)
            poses = [world_pose(self.document, tracks, i * 0.8 / 24) for i in range(25)]
            for joint in self.joints.values():
                for a, b in zip(poses[0][joint][0], poses[-1][joint][0]):
                    self.assertAlmostEqual(a, b, places=5)
                qa, qb = poses[0][joint][1], poses[-1][joint][1]
                self.assertAlmostEqual(abs(sum(a * b for a, b in zip(qa, qb))), 1.0, places=5)
            root = self.joints['root']
            for pose in poses:
                self.assertTrue(all(math.isfinite(v) for p, _, _ in pose.values() for v in p))
                self.assertEqual(pose[root][0], poses[0][root][0])
                self.assertEqual(pose[root][1], poses[0][root][1])
                left, right = pose[self.joints['foot_l']][0], pose[self.joints['foot_r']][0]
                self.assertGreater(left[0] - right[0], 0.15)
                floor_error = min(pose[self.joints[f]][0][1] - self.rest[self.joints[f]][0][1]
                                  for f in ('foot_l', 'foot_r'))
                self.assertLess(abs(floor_error), 0.0001)

    def test_leading_and_trailing_legs_alternate_and_directions_are_distinct(self):
        for name, leading, trailing in [
            ('Strafe_Left', 'foot_l', 'foot_r'),
            ('Strafe_Right', 'foot_r', 'foot_l'),
        ]:
            tracks = source_tracks(self.document, self.buffers, name)
            first = world_pose(self.document, tracks, 0.2)
            second = world_pose(self.document, tracks, 0.6)
            self.assertGreater(first[self.joints[leading]][0][1] -
                               first[self.joints[trailing]][0][1], 0.05)
            self.assertGreater(second[self.joints[trailing]][0][1] -
                               second[self.joints[leading]][0][1], 0.05)


if __name__ == '__main__':
    unittest.main()
