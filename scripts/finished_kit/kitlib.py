"""Shared helpers for the human-male kit build. Work space: Blender axes (Z up, character faces -Y,
character's left on +X), metres, final game scale (1.81 m to the scalp), soles at z = 0."""
import json, os, sys
import numpy as np
from gltfkit import read_glb, write_glb, accessor, world_matrices, parents, local_matrix, quat_to_mat, mat_to_quat

# The art workspace holds the raw generator output and every intermediate file; it stays outside Git
# (docs/ART-PRODUCTION.md, Files). Set PITBORN_ART to move it.
ART = os.environ.get('PITBORN_ART', os.path.expanduser('~/pitborn-art'))
KIT = ART + '/human-male-kit/'
RAW = ART + '/character-sheets/human/male/'
OLD_KIT = ART + '/build/meshy-human-male/meshy-human-male.glb'    # the first Meshy kit: bind pose and clips
LOW_GLB = RAW + 'Meshy_AI_male_human.glb'
DENSE_GLB = RAW + 'Meshy_AI_male_human_detailed.glb'
GAME_SCALE = 1.81 / 1.70                    # Meshy rigged export is 1.70 m; the kit male is 1.81 m
DENSE_TO_LOW = (0.8945951500322434, 0.8503527641296387)  # uniform scale, then +z (measured from bounds)


def gltf_to_blender(p):
    p = np.asarray(p, np.float64)
    return np.stack([p[..., 0], -p[..., 2], p[..., 1]], -1)


def low_to_work(p_gltf):
    return gltf_to_blender(p_gltf) * GAME_SCALE


def dense_to_work(p_gltf):
    s, dz = DENSE_TO_LOW
    q = gltf_to_blender(p_gltf) * s
    q[..., 2] += dz
    return q * GAME_SCALE


def load_mesh(path):
    """First primitive of a GLB: positions, uvs, triangle indices (glTF space)."""
    g, buf = read_glb(path)
    pr = g['meshes'][0]['primitives'][0]
    P = accessor(g, buf, pr['attributes']['POSITION']).astype(np.float64)
    UV = accessor(g, buf, pr['attributes']['TEXCOORD_0']).astype(np.float64)
    F = accessor(g, buf, pr['indices']).reshape(-1, 3).astype(np.int64)
    return g, buf, P, UV, F


def weld(P, F, tol=1e-6):
    """Merge vertices that share a position. Returns welded verts, faces, and old->new map."""
    key = np.round(P / tol).astype(np.int64)
    _, first, inv = np.unique(key, axis=0, return_index=True, return_inverse=True)
    inv = inv.reshape(-1)
    Fw = inv[F]
    ok = (Fw[:, 0] != Fw[:, 1]) & (Fw[:, 1] != Fw[:, 2]) & (Fw[:, 0] != Fw[:, 2])
    return P[first], Fw[ok], inv, ok


def vertex_normals(V, F):
    n = np.cross(V[F[:, 1]] - V[F[:, 0]], V[F[:, 2]] - V[F[:, 0]])
    N = np.zeros_like(V)
    for k in range(F.shape[1] if F.shape[1] == 3 else 3):
        np.add.at(N, F[:, k], n)
    return N / np.maximum(np.linalg.norm(N, axis=1, keepdims=True), 1e-20)


def quads_to_tris(Q):
    return np.concatenate([Q[:, [0, 1, 2]], Q[:, [0, 2, 3]]])


def bilinear(img, uv):
    """Sample an HxWxC float image at glTF UVs (v down)."""
    h, w = img.shape[:2]
    x = np.clip(uv[:, 0] % 1.0 * w - 0.5, 0, w - 1.001); y = np.clip(uv[:, 1] % 1.0 * h - 0.5, 0, h - 1.001)
    x0 = x.astype(int); y0 = y.astype(int); fx = (x - x0)[:, None]; fy = (y - y0)[:, None]
    return (img[y0, x0] * (1 - fx) * (1 - fy) + img[y0, x0 + 1] * fx * (1 - fy)
            + img[y0 + 1, x0] * (1 - fx) * fy + img[y0 + 1, x0 + 1] * fx * fy)
