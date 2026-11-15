#!/usr/bin/env python3
"""Author the shared in-place lateral run loops on the Quaternius universal rig.

Standard library only; no Blender, renderer or runtime IK. Run from the project
root, then rebuild the character:

    python3 scripts/author_universal_strafes.py
    python3 scripts/check_animation_compatibility.py
    ./scripts/build-web.sh

The loops share the UAL1 `Jog_Fwd_Loop` cadence (28 frames at 30 samples/s) and
its short planted-foot fraction, so forward and lateral movement step at the
same rate for one game speed. Each planted foot travels STANCE_TRAVEL across the
body in STANCE_FRACTION of the cycle. At the client's top playback rate (2.0)
that is the 7.2 m/s rule speed, so planted feet do not slide.

Legs use analytic two-bone placement (knees forward), feet stay in their flat
bind orientation, the pelvis bobs/leans, and the arms take the jog's own
pumping, bent-elbow rotations at the matching footfall phase. Root translation
and heading stay fixed. The existing library supplies the skeleton, inverse bind
matrices and carrier triangle (originally a Blender export of the CC0 UAL1
mannequin); only its two animations are replaced, so reruns are byte-identical.
"""
import json
import math
from pathlib import Path
import struct

import quaternius_character as q
from animation_contracts import rotate, sample, source_tracks, world_pose

FRAMES = 28                # UAL1 Jog_Fwd_Loop: 28 frames, 0.9333 s.
FPS = 30.0
STANCE_FRACTION = 0.145    # Jog: ~4-5 of 28 frames planted per foot.
STANCE_TRAVEL = 0.45       # Metres across the body per planted foot.
LAND_WIDE = 0.35           # Lead foot touchdown, metres from the body centre.
LIFT = 0.24               # Peak swing-ankle clearance, metres.
HEEL_KICK = 0.14          # Swing ankle recovery behind the body, metres.
CROUCH, BOB = 0.035, 0.045  # Pelvis lowering and stance/flight bob, metres.
LEAN = math.radians(4.0)   # Body lean into the direction of travel.
PITCH = math.radians(7.0)  # Athletic forward lean, like the jog.
ARM_BLEND = 0.7            # Share of the jog's shoulder/upper-arm pump.
JOG_LANDING = {'l': 1, 'r': 15}  # Jog frames where each foot touches down.
ARMS = ('clavicle_', 'upperarm_', 'lowerarm_', 'hand_', 'thumb_', 'index_',
        'middle_', 'ring_', 'pinky_')


def vadd(a, b):
    return tuple(x + y for x, y in zip(a, b))


def vsub(a, b):
    return tuple(x - y for x, y in zip(a, b))


def vscale(a, s):
    return tuple(x * s for x in a)


def dot(a, b):
    return sum(x * y for x, y in zip(a, b))


def length(a):
    return math.sqrt(dot(a, a))


def unit(a):
    return vscale(a, 1.0 / length(a))


def cross(a, b):
    return (a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2],
            a[0] * b[1] - a[1] * b[0])


def axis_angle(axis, angle):
    s = math.sin(angle / 2)
    return (*vscale(unit(axis), s), math.cos(angle / 2))


def between(a, b):
    """Shortest rotation taking direction a onto direction b."""
    a, b = unit(a), unit(b)
    c = dot(a, b)
    if c > 1 - 1e-9:
        return (0.0, 0.0, 0.0, 1.0)
    return q.normalize((*cross(a, b), 1.0 + c))


def slerp(a, b, t):
    return sample([0.0, 1.0], [a, b], t, rotation=True)


def smoothstep(x):
    x = min(1.0, max(0.0, x))
    return x * x * (3 - 2 * x)


def gait(phase):
    """Lateral offset (towards travel) and swing amount (0-1) of each ankle."""
    s, wide = STANCE_FRACTION, LAND_WIDE
    lead_out, lead_in = wide, wide - STANCE_TRAVEL
    trail_out = lead_in + 0.20
    trail_in = trail_out - STANCE_TRAVEL

    def foot(p, land, leave, early):
        if p < s:
            return land + (leave - land) * p / s, 0.0
        swing = (p - s) / (1 - s)
        # The lead foot reaches out early; the trailing foot closes in late, so
        # the two never cross while both are moving in the same direction.
        reach = smoothstep(swing / 0.45) if early else smoothstep((swing - 0.55) / 0.45)
        return leave + (land - leave) * reach, math.sin(math.pi * swing)

    lead = foot(phase % 1.0, lead_out, lead_in, True)
    trail = foot((phase - 0.5) % 1.0, trail_out, trail_in, False)
    return lead, trail


def author(document, source, source_buffers, direction):
    """Return {joint index: (pelvis translation or None, rotations)} for one loop."""
    joints = q.joint_map(document)
    src = q.joint_map(source)
    names = {i: n for n, i in joints.items()}
    parents = {c: i for i, n in enumerate(document['nodes']) for c in n.get('children', [])}
    order = []
    while len(order) < len(joints):
        order += [i for i in joints.values() if i not in order and
                  (parents.get(i) not in joints.values() or parents[i] in order)]
    bind = world_pose(document)
    sampled = {}
    for clip in ('Idle_Loop', 'Jog_Fwd_Loop'):
        sampled[clip] = {(source['nodes'][n]['name'], path): (times, values)
                         for n, path, times, values in source_tracks(source, source_buffers, clip)}

    def local(clip, name, path, time):
        times, values = sampled[clip][(name, path)]
        return sample(times, values, time, path == 'rotation')

    lead = 'l' if direction > 0 else 'r'
    trail = 'r' if lead == 'l' else 'l'
    floor = {s: bind[joints['foot_' + s]][0][1] for s in 'lr'}
    depth = sum(bind[joints['foot_' + s]][0][2] for s in 'lr') / 2
    centre = bind[joints['pelvis']][0][0]
    frames = []
    for frame in range(FRAMES + 1):
        # Same footfall frames as the jog, so the client can carry the step
        # phase across forward/lateral switches and the arms pump in time.
        phase = ((frame - JOG_LANDING[lead]) % FRAMES) / FRAMES
        jog_time = (frame % FRAMES) / FPS
        pose = {}
        for name in joints:
            rotation = local('Idle_Loop', name, 'rotation', 0.0)
            if name.startswith(ARMS):
                jog = local('Jog_Fwd_Loop', name, 'rotation', jog_time)
                blend = ARM_BLEND if name.startswith(('clavicle_', 'upperarm_')) else 1.0
                rotation = slerp(rotation, jog, blend)
            pose[name] = (document['nodes'][joints[name]].get('translation', (0, 0, 0)),
                          rotation)
        pose['pelvis'] = (local('Idle_Loop', 'pelvis', 'translation', 0.0), pose['pelvis'][1])
        world = {}

        def place(index):
            name = names[index]
            t, r = pose[name]
            if index in world:
                return
            if parents.get(index) in names:
                pp, pr = world[parents[index]]
                world[index] = (vadd(pp, rotate(pr, t)), q.normalize(q.multiply(pr, r)))
            else:
                world[index] = (bind[index][0], bind[index][1])
        for index in order:
            place(index)
        base = dict(world)
        # Pelvis: lowest in mid-stance, highest mid-flight, leaning into travel.
        pelvis = joints['pelvis']
        bob = -CROUCH - BOB * math.cos(4 * math.pi * (phase - STANCE_FRACTION / 2))
        sway = direction * 0.012 * math.sin(2 * math.pi * phase)
        lean = q.multiply(axis_angle((1, 0, 0), PITCH), axis_angle((0, 0, 1), -direction * LEAN))
        pp, pr = base[pelvis]
        world[pelvis] = (vadd(pp, (sway, bob, 0.0)), q.normalize(q.multiply(lean, pr)))
        for index in order:
            if index != pelvis and parents.get(index) in names and names[index] != 'root':
                parent = parents[index]
                if index in world and parent in world:
                    t, r = pose[names[index]]
                    pp, pr = world[parent]
                    world[index] = (vadd(pp, rotate(pr, t)), q.normalize(q.multiply(pr, r)))
        # Keep the head level against the lean.
        neck = joints['neck_01']
        level = q.multiply(axis_angle((0, 0, 1), direction * LEAN * 0.8),
                           axis_angle((1, 0, 0), -PITCH * 0.6))
        world_neck = q.normalize(q.multiply(level, world[neck][1]))
        pose['neck_01'] = (pose['neck_01'][0],
                           q.normalize(q.multiply(q.inverse(world[parents[neck]][1]), world_neck)))
        (lead_x, lead_lift), (trail_x, trail_lift) = gait(phase)
        for side, offset, swing in ((lead, lead_x, lead_lift), (trail, trail_x, trail_lift)):
            thigh, calf, foot = (joints[f'{b}_{side}'] for b in ('thigh', 'calf', 'foot'))
            hip = world[thigh][0]
            ankle = (centre + direction * offset, floor[side] + LIFT * swing,
                     depth - HEEL_KICK * swing)
            upper = length(vsub(bind[calf][0], bind[thigh][0]))
            lower = length(vsub(bind[foot][0], bind[calf][0]))
            toward = vsub(ankle, hip)
            distance = length(toward)
            if not abs(upper - lower) < distance < (upper + lower) * 0.995:
                raise ValueError('Foot target outside unstretched leg reach')
            axis = unit(toward)
            forward = (0.0, 0.0, 1.0)
            pole = unit(vsub(forward, vscale(axis, dot(forward, axis))))
            along = (upper * upper - lower * lower + distance * distance) / (2 * distance)
            knee = vadd(vadd(hip, vscale(axis, along)),
                        vscale(pole, math.sqrt(max(0.0, upper * upper - along * along))))
            thigh_r = q.normalize(q.multiply(
                between(vsub(base[calf][0], base[thigh][0]), vsub(knee, hip)), base[thigh][1]))
            calf_r = q.normalize(q.multiply(
                between(vsub(base[foot][0], base[calf][0]), vsub(ankle, knee)), base[calf][1]))
            world[thigh] = (hip, thigh_r)
            world[calf] = (knee, calf_r)
            world[foot] = (ankle, bind[foot][1])
            for child, world_r in ((thigh, thigh_r), (calf, calf_r), (foot, bind[foot][1])):
                parent_r = world[parents[child]][1]
                pose[names[child]] = (pose[names[child]][0],
                                      q.normalize(q.multiply(q.inverse(parent_r), world_r)))
            ball = joints['ball_' + side]
            pose['ball_' + side] = (pose['ball_' + side][0], document['nodes'][ball]['rotation'])
        # Pelvis back to its local frame under the fixed root.
        root_p, root_r = world[parents[pelvis]]
        pose['pelvis'] = (rotate(q.inverse(root_r), vsub(world[pelvis][0], root_p)),
                          q.normalize(q.multiply(q.inverse(root_r), world[pelvis][1])))
        frames.append(pose)
    tracks = []
    for name, index in joints.items():
        if name == 'root':
            continue
        rotations = [f[name][1] for f in frames]
        for i in range(1, len(rotations)):
            if dot(rotations[i], rotations[i - 1]) < 0:
                rotations[i] = vscale(rotations[i], -1.0)
        rotations[-1] = rotations[0]
        tracks.append((index, 'rotation', rotations))
        if name == 'pelvis':
            translations = [f[name][0] for f in frames]
            translations[-1] = translations[0]
            tracks.append((index, 'translation', translations))
    return tracks


def write(document, buffers, clips, path):
    """Keep the skeleton/carrier accessors; replace both animations."""
    keep = sorted({i for m in document['meshes'] for p in m['primitives']
                   for i in [*p['attributes'].values(), p['indices']]} |
                  {s['inverseBindMatrices'] for s in document['skins']})
    if keep != list(range(len(keep))):
        raise ValueError('Expected carrier/skin accessors to precede animation data')
    accessors, views, binary = [], [], bytearray()

    def add(data, accessor):
        while len(binary) % 4:
            binary.append(0)
        views.append({'buffer': 0, 'byteOffset': len(binary), 'byteLength': len(data)})
        binary.extend(data)
        accessors.append({**accessor, 'bufferView': len(views) - 1})
        return len(accessors) - 1
    for index in keep:
        accessor = dict(document['accessors'][index])
        view = document['bufferViews'][accessor.pop('bufferView')]
        start = view.get('byteOffset', 0)
        add(bytes(buffers[0][start:start + view['byteLength']]), accessor)
        if 'target' in view:
            views[-1]['target'] = view['target']
    times = [f / FPS for f in range(FRAMES + 1)]
    time_accessor = add(struct.pack(f'<{len(times)}f', *times),
                        {'componentType': 5126, 'count': len(times), 'type': 'SCALAR',
                         'min': [0.0], 'max': [times[-1]]})
    animations = []
    for name, tracks in clips:
        samplers, channels = [], []
        for node, target, samples in tracks:
            width = len(samples[0])
            data = b''.join(struct.pack(f'<{width}f', *s) for s in samples)
            output = add(data, {'componentType': 5126, 'count': len(samples),
                                'type': 'VEC4' if width == 4 else 'VEC3'})
            samplers.append({'input': time_accessor, 'interpolation': 'LINEAR', 'output': output})
            channels.append({'sampler': len(samplers) - 1,
                             'target': {'node': node, 'path': target}})
        animations.append({'name': name, 'channels': channels, 'samplers': samplers})
    while len(binary) % 4:
        binary.append(0)
    output = dict(document)
    output.update(accessors=accessors, bufferViews=views, animations=animations,
                  buffers=[{'byteLength': len(binary)}])
    output['asset'] = dict(document['asset'],
                           generator='Arena Prototype scripts/author_universal_strafes.py')
    payload = json.dumps(output, separators=(',', ':')).encode()
    payload += b' ' * (-len(payload) % 4)
    size = 12 + 8 + len(payload) + 8 + len(binary)
    path.write_bytes(struct.pack('<III', 0x46546C67, 2, size) +
                     struct.pack('<II', len(payload), 0x4E4F534A) + payload +
                     struct.pack('<II', len(binary), 0x004E4942) + bytes(binary))


def main(library_path=q.STRAFE_LIBRARY, source_path=q.LIBRARY, output=q.STRAFE_LIBRARY):
    document, buffers = q.read_asset(Path(library_path))
    source, source_buffers = q.read_asset(Path(source_path))
    clips = [(name, author(document, source, source_buffers, direction))
             for name, direction in (('Strafe_Left', 1.0), ('Strafe_Right', -1.0))]
    write(document, buffers, clips, Path(output))
    return output


if __name__ == '__main__':
    print(f'Wrote {main()}: two {FRAMES}-frame lateral run loops '
          f'({FRAMES / FPS:.4f} s), stance {STANCE_FRACTION:.2f}, '
          f'travel {STANCE_TRAVEL:.2f} m per planted foot')
