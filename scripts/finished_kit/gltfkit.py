"""Minimal GLB read/write helpers + skeleton world transforms (numpy)."""
import json, struct
import numpy as np

def read_glb(path):
    d = open(path, 'rb').read()
    jl = struct.unpack('<I', d[12:16])[0]
    g = json.loads(d[20:20 + jl])
    off = 20 + jl
    bl = struct.unpack('<I', d[off:off + 4])[0]
    return g, bytearray(d[off + 8: off + 8 + bl])

def write_glb(path, g, buf):
    while len(buf) % 4: buf += b'\0'
    g['buffers'] = [{'byteLength': len(buf)}]
    js = json.dumps(g, separators=(',', ':')).encode()
    while len(js) % 4: js += b' '
    out = struct.pack('<III', 0x46546C67, 2, 12 + 8 + len(js) + 8 + len(buf))
    out += struct.pack('<II', len(js), 0x4E4F534A) + js + struct.pack('<II', len(buf), 0x004E4942) + bytes(buf)
    open(path, 'wb').write(out)

CT = {5126: np.float32, 5125: np.uint32, 5123: np.uint16, 5121: np.uint8}
NC = {'SCALAR': 1, 'VEC2': 2, 'VEC3': 3, 'VEC4': 4, 'MAT4': 16}

def accessor(g, buf, i):
    a = g['accessors'][i]; bv = g['bufferViews'][a['bufferView']]
    dt = np.dtype(CT[a['componentType']]); n = NC[a['type']]
    start = bv.get('byteOffset', 0) + a.get('byteOffset', 0)
    stride = bv.get('byteStride', dt.itemsize * n)
    if stride == dt.itemsize * n:
        arr = np.frombuffer(bytes(buf[start:start + a['count'] * stride]), dt).reshape(a['count'], n)
    else:
        raw = np.frombuffer(bytes(buf[start:start + a['count'] * stride]), np.uint8).reshape(a['count'], stride)
        arr = raw[:, :dt.itemsize * n].copy().view(dt).reshape(a['count'], n)
    return arr

def quat_to_mat(q):
    x, y, z, w = q
    return np.array([[1-2*(y*y+z*z), 2*(x*y-z*w), 2*(x*z+y*w)],
                     [2*(x*y+z*w), 1-2*(x*x+z*z), 2*(y*z-x*w)],
                     [2*(x*z-y*w), 2*(y*z+x*w), 1-2*(x*x+y*y)]])

def mat_to_quat(m):
    t = np.trace(m)
    if t > 0:
        s = np.sqrt(t + 1) * 2; w = s / 4; x = (m[2,1]-m[1,2]) / s; y = (m[0,2]-m[2,0]) / s; z = (m[1,0]-m[0,1]) / s
    elif m[0,0] > m[1,1] and m[0,0] > m[2,2]:
        s = np.sqrt(1 + m[0,0] - m[1,1] - m[2,2]) * 2; w = (m[2,1]-m[1,2]) / s; x = s / 4; y = (m[0,1]+m[1,0]) / s; z = (m[0,2]+m[2,0]) / s
    elif m[1,1] > m[2,2]:
        s = np.sqrt(1 + m[1,1] - m[0,0] - m[2,2]) * 2; w = (m[0,2]-m[2,0]) / s; x = (m[0,1]+m[1,0]) / s; y = s / 4; z = (m[1,2]+m[2,1]) / s
    else:
        s = np.sqrt(1 + m[2,2] - m[0,0] - m[1,1]) * 2; w = (m[1,0]-m[0,1]) / s; x = (m[0,2]+m[2,0]) / s; y = (m[1,2]+m[2,1]) / s; z = s / 4
    q = np.array([x, y, z, w]); return q / np.linalg.norm(q)

def local_matrix(n):
    if 'matrix' in n: return np.array(n['matrix']).reshape(4, 4).T
    M = np.eye(4)
    R = quat_to_mat(n.get('rotation', [0, 0, 0, 1])) * np.array(n.get('scale', [1, 1, 1]))
    M[:3, :3] = R; M[:3, 3] = n.get('translation', [0, 0, 0]); return M

def parents(g):
    p = {}
    for i, n in enumerate(g['nodes']):
        for c in n.get('children', []): p[c] = i
    return p

def world_matrices(g):
    p = parents(g); W = {}
    def w(i):
        if i not in W: W[i] = (w(p[i]) if i in p else np.eye(4)) @ local_matrix(g['nodes'][i])
        return W[i]
    for i in range(len(g['nodes'])): w(i)
    return W
