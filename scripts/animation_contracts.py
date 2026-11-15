"""Renderer-free glTF pose checks for reusable humanoid animation libraries."""
from bisect import bisect_right
import math

from quaternius_character import inverse, multiply, normalize, values


def sample(times, samples, time, rotation=False):
    index = max(0, min(len(times) - 2, bisect_right(times, time) - 1))
    if len(times) == 1:
        return samples[0]
    phase = max(0.0, min(1.0, (time - times[index]) / (times[index + 1] - times[index])))
    a, b = samples[index], samples[index + 1]
    if rotation:
        dot = sum(x * y for x, y in zip(a, b))
        if dot < 0:
            b, dot = tuple(-v for v in b), -dot
        if dot < 0.9995:
            angle = math.acos(max(-1.0, min(1.0, dot)))
            sine = math.sin(angle)
            return tuple((math.sin((1 - phase) * angle) * x +
                          math.sin(phase * angle) * y) / sine for x, y in zip(a, b))
    result = tuple(x + (y - x) * phase for x, y in zip(a, b))
    return normalize(result) if rotation else result


def rotate(rotation, vector):
    return multiply(multiply(rotation, (*vector, 0.0)), inverse(rotation))[:3]


def source_tracks(document, buffers, name):
    animation = next(a for a in document['animations'] if a['name'] == name)
    result = []
    for channel in animation['channels']:
        sampler = animation['samplers'][channel['sampler']]
        samples = values(document, buffers, sampler['output'])
        interpolation = sampler.get('interpolation', 'LINEAR')
        if interpolation != 'LINEAR' and not (
                interpolation == 'STEP' and all(v == samples[0] for v in samples)):
            raise ValueError('Contract evaluator requires linear/constant glTF tracks')
        result.append((channel['target']['node'], channel['target']['path'],
                       [v[0] for v in values(document, buffers, sampler['input'])],
                       samples))
    return result


def world_pose(document, tracks=(), time=0.0):
    """Evaluate TRS joints, including parent scales and quaternion interpolation."""
    nodes = document['nodes']
    parents = {c: i for i, n in enumerate(nodes) for c in n.get('children', [])}
    animated = {(node, path): sample(times, samples, time, path == 'rotation')
                for node, path, times, samples in tracks}
    result = {}

    def evaluate(index):
        if index in result:
            return result[index]
        node = nodes[index]
        if 'matrix' in node:
            raise ValueError('Contract evaluator requires decomposed TRS nodes')
        position = animated.get((index, 'translation'), node.get('translation', (0, 0, 0)))
        rotation = animated.get((index, 'rotation'), node.get('rotation', (0, 0, 0, 1)))
        scale = animated.get((index, 'scale'), node.get('scale', (1, 1, 1)))
        if index in parents:
            pp, pr, ps = evaluate(parents[index])
            offset = rotate(pr, tuple(s * v for s, v in zip(ps, position)))
            position = tuple(p + v for p, v in zip(pp, offset))
            rotation = normalize(multiply(pr, rotation))
            scale = tuple(a * b for a, b in zip(ps, scale))
        result[index] = (position, rotation, scale)
        return result[index]
    for index in range(len(nodes)):
        evaluate(index)
    return result
