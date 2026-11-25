"""Write a weapon GLB for the game: one unskinned mesh in the grip-socket frame (weapon.py, hair_low.py,
hair_tex.py --colour), opaque textures as JPEG like the kits. The client parents it under a kit's
socket_weapon_r node, so the node's origin is where the hand holds it.
python weapon_build.py LOW.npz TEX_PREFIX NAME OUT.glb"""
import sys
import numpy as np
from glbwriter import GLB

low, tex, name, out = sys.argv[1:5]
d = np.load(low); V, F, UV = d['V'], d['F'], d['UV']                         # UVs per corner, rows down
fn = np.cross(V[F[:, 1]] - V[F[:, 0]], V[F[:, 2]] - V[F[:, 0]]); N = np.zeros_like(V)
for k in range(3): np.add.at(N, F[:, k], fn)
N /= np.maximum(np.linalg.norm(N, axis=1, keepdims=True), 1e-20)
cv = F.reshape(-1); cu = UV.reshape(-1, 2)                                   # split vertices at UV seams
key = np.concatenate([cv[:, None], np.round(cu * 1e5).astype(np.int64)], 1)
_, first, inv = np.unique(key, axis=0, return_index=True, return_inverse=True); sv = cv[first]
glb = GLB(f'pitborn weapon_build.py {name}')
mat = glb.material(name, glb.texture(tex + '.png', name + '_BaseColor', (90, 2)), glb.texture(tex + '_n.png', name + '_Normal', (95, 0)))
mesh = dict(name=name, primitives=[glb.static_primitive(V[sv], N[sv], cu[first], inv.reshape(-1, 3), mat)])
glb.j['meshes'].append(mesh); node = glb.node(name=name, mesh=0)
glb.j['scenes'][0]['nodes'] = [node]
size = glb.write(out)
print('wrote %s: %.2f MB, %d triangles, %d vertices; extent X %.3f Y %.3f..%.3f Z %.3f m'
      % (out, size / 1e6, len(F), len(sv), np.ptp(V[:, 0]), V[:, 1].min(), V[:, 1].max(), np.ptp(V[:, 2])))
