#!/usr/bin/env python3
"""Check the shared strafe library against installed bases/outfits/races.

No renderer or Blender needed. QAL race files are read locally, never copied
into the animation library or runtime player by this check.
"""
import math
from pathlib import Path

import quaternius_character as q
from animation_contracts import world_pose


def main():
    library, buffers = q.read_asset(q.STRAFE_LIBRARY)
    targets = (list(Path('assets/quaternius/base/Base Characters').rglob('*.gltf')) +
               list(Path('assets/quaternius/races').rglob('*.glb')) +
               list(Path('assets/quaternius/outfits/Exports').glob('*/Outfits/*.gltf')))
    if not targets:
        raise SystemExit('No optional humanoid target sources are installed')
    for path in sorted(targets):
        target, _ = q.read_asset(path)
        joints = q.joint_map(target)
        rest = world_pose(target)
        for name in ('Strafe_Left', 'Strafe_Right'):
            tracks = dict(q.retarget(target, library, buffers, clips=((name, name, False),)))[name]
            cycle = max(times[-1] for _, _, times, _ in tracks)
            frames = round(cycle * 30)
            poses = [world_pose(target, tracks, i * cycle / frames) for i in range(frames + 1)]
            for pose in poses:
                assert all(math.isfinite(v) for p, _, _ in pose.values() for v in p), path
                assert pose[joints['foot_l']][0][0] > pose[joints['foot_r']][0][0], path
                assert pose[joints['root']] == poses[0][joints['root']], path
            for joint in joints.values():
                assert max(abs(a - b) for a, b in
                           zip(poses[0][joint][0], poses[-1][joint][0])) < 1e-5, path
                qa, qb = poses[0][joint][1], poses[-1][joint][1]
                assert abs(abs(sum(a * b for a, b in zip(qa, qb))) - 1.0) < 1e-5, path
            # Running loops have flight, so compare only the planted frames
            # (left 1-5, right 15-19, matching Jog_Fwd_Loop).
            floor_error = [poses[i][joints[f]][0][1] - rest[joints[f]][0][1]
                           for f, planted in (('foot_l', range(1, 6)), ('foot_r', range(15, 20)))
                           for i in planted]
            print(f'{path.name}: {name}, 65-joint hierarchy/loop/root/separation OK; '
                  f'planted-height deviation {min(floor_error):+.3f}…{max(floor_error):+.3f} m')
    print(f'PASS: both clips retarget to {len(targets)} installed targets. Not a rendered pass.')


if __name__ == '__main__':
    main()
