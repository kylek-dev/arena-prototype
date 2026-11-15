"""Contracts for the small, reusable animation source shipped with the project."""
from pathlib import Path
import math
import sys
import unittest

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / 'scripts'))
import quaternius_character as q
from animation_contracts import source_tracks, world_pose

CYCLE = 28 / 30  # UAL1 Jog_Fwd_Loop length; footfalls are frame-aligned with it.
FRAMES = 28
# Jog_Fwd_Loop plants the left foot over frames 1-5 and the right over 15-19.
PLANTED = {'foot_l': range(1, 5), 'foot_r': range(15, 19)}


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

    def poses(self, name):
        tracks = source_tracks(self.document, self.buffers, name)
        self.assertAlmostEqual(max(t[-1] for _, _, t, _ in tracks), CYCLE, places=6)
        return [world_pose(self.document, tracks, i * CYCLE / FRAMES) for i in range(FRAMES + 1)]

    def test_loops_close_root_does_not_travel_and_feet_never_cross(self):
        for name in ('Strafe_Left', 'Strafe_Right'):
            poses = self.poses(name)
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
                for foot in ('foot_l', 'foot_r'):
                    height = pose[self.joints[foot]][0][1] - self.rest[self.joints[foot]][0][1]
                    self.assertGreater(height, -0.0001)

    def test_authoring_script_reproduces_the_shipped_library(self):
        import tempfile
        import author_universal_strafes as authoring
        with tempfile.TemporaryDirectory() as directory:
            output = Path(directory) / 'strafes.glb'
            authoring.main(ROOT / q.STRAFE_LIBRARY, ROOT / q.LIBRARY, output)
            self.assertEqual(output.read_bytes(), (ROOT / q.STRAFE_LIBRARY).read_bytes())

    def test_planted_feet_match_the_jog_footfalls_and_sweep_at_running_speed(self):
        for name, direction in (('Strafe_Left', 1), ('Strafe_Right', -1)):
            poses = self.poses(name)
            for foot, frames in PLANTED.items():
                joint = self.joints[foot]
                floor = self.rest[joint][0][1]
                for frame in range(FRAMES):
                    height = poses[frame][joint][0][1] - floor
                    if frame in frames or frame == frames.stop:
                        self.assertLess(abs(height), 0.0001, (name, foot, frame))
                        if frame in frames:
                            # Ground passes under the body opposite to travel:
                            # 3.32 m/s x 1.066 player scale x the client's 2.0
                            # playback rate at full speed ~= the 7.2 m/s rules.
                            step = poses[frame + 1][joint][0][0] - poses[frame][joint][0][0]
                            self.assertAlmostEqual(-direction * step * 30, 3.32, delta=0.1)
                    elif frame not in (frames.start - 1, frames.stop + 1):
                        self.assertGreater(height, 0.02, (name, foot, frame))
                # Swing clearance is a running lift, not a shuffle.
                self.assertGreater(max(p[joint][0][1] for p in poses) - floor, 0.2)


if __name__ == '__main__':
    unittest.main()
