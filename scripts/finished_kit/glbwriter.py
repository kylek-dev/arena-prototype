"""A small GLB writer: accessors, PNG images, materials, skinned mesh nodes, a skeleton and animations."""
import json, struct
import numpy as np

C_WG = np.array([[1, 0, 0], [0, 0, 1], [0, -1, 0.0]])        # work (Z up, -Y front) -> glTF (Y up, +Z front)


def to_gltf(p): return np.asarray(p, np.float64) @ C_WG.T


def quat(R):
    t = np.trace(R)
    if t > 0: s = np.sqrt(t + 1) * 2; q = [(R[2, 1] - R[1, 2]) / s, (R[0, 2] - R[2, 0]) / s, (R[1, 0] - R[0, 1]) / s, s / 4]
    elif R[0, 0] > R[1, 1] and R[0, 0] > R[2, 2]: s = np.sqrt(1 + R[0, 0] - R[1, 1] - R[2, 2]) * 2; q = [s / 4, (R[0, 1] + R[1, 0]) / s, (R[0, 2] + R[2, 0]) / s, (R[2, 1] - R[1, 2]) / s]
    elif R[1, 1] > R[2, 2]: s = np.sqrt(1 + R[1, 1] - R[0, 0] - R[2, 2]) * 2; q = [(R[0, 1] + R[1, 0]) / s, s / 4, (R[1, 2] + R[2, 1]) / s, (R[0, 2] - R[2, 0]) / s]
    else: s = np.sqrt(1 + R[2, 2] - R[0, 0] - R[1, 1]) * 2; q = [(R[0, 2] + R[2, 0]) / s, (R[1, 2] + R[2, 1]) / s, s / 4, (R[1, 0] - R[0, 1]) / s]
    q = np.array(q); return q / np.linalg.norm(q)


def quat_to_mat(q):
    x, y, z, w = q
    return np.array([[1 - 2 * (y * y + z * z), 2 * (x * y - z * w), 2 * (x * z + y * w)], [2 * (x * y + z * w), 1 - 2 * (x * x + z * z), 2 * (y * z - x * w)], [2 * (x * z - y * w), 2 * (y * z + x * w), 1 - 2 * (x * x + y * y)]])


class GLB:
    def __init__(self, generator):
        self.j = dict(asset=dict(version='2.0', generator=generator), scene=0, scenes=[dict(name='Scene', nodes=[])], nodes=[], meshes=[], materials=[],
                      textures=[], images=[], samplers=[dict(magFilter=9729, minFilter=9987, wrapS=33071, wrapT=33071)], accessors=[], bufferViews=[], skins=[], animations=[])
        self.buf = bytearray()

    def _view(self, data, target=None):
        while len(self.buf) % 4: self.buf += b'\0'
        v = dict(buffer=0, byteOffset=len(self.buf), byteLength=len(data))
        if target: v['target'] = target
        self.buf += data; self.j['bufferViews'].append(v); return len(self.j['bufferViews']) - 1

    def acc(self, arr, typ, ctype=5126, target=None, minmax=False, normalized=False):
        dt = {5126: np.float32, 5125: np.uint32, 5123: np.uint16, 5121: np.uint8}[ctype]; a = np.ascontiguousarray(arr, dt)
        d = dict(bufferView=self._view(a.tobytes(), target), componentType=ctype, count=len(a), type=typ)
        if minmax: d['min'] = a.min(0).tolist(); d['max'] = a.max(0).tolist()
        if normalized: d['normalized'] = True
        self.j['accessors'].append(d); return len(self.j['accessors']) - 1

    def texture(self, path, name, jpeg=None):
        """Embed an image. `jpeg` = (quality, subsampling) stores an opaque texture as JPEG, a quarter of the PNG
        or less; PIL's encoder is deterministic, so the build stays reproducible."""
        if jpeg is None:
            data, mime = open(path, 'rb').read(), 'image/png'
        else:
            import io
            from PIL import Image
            out = io.BytesIO(); Image.open(path).convert('RGB').save(out, 'JPEG', quality=jpeg[0], subsampling=jpeg[1], optimize=True)
            data, mime = out.getvalue(), 'image/jpeg'
        self.j['images'].append(dict(name=name, mimeType=mime, bufferView=self._view(data)))
        self.j['textures'].append(dict(sampler=0, source=len(self.j['images']) - 1)); return len(self.j['textures']) - 1

    def material(self, name, base=None, normal=None, alpha=None, color=(1, 1, 1, 1), roughness=0.9, double_sided=False):
        m = dict(name=name, pbrMetallicRoughness=dict(metallicFactor=0.0, roughnessFactor=roughness, baseColorFactor=list(color)))
        if base is not None: m['pbrMetallicRoughness']['baseColorTexture'] = dict(index=base)
        if normal is not None: m['normalTexture'] = dict(index=normal)
        if alpha: m['alphaMode'] = alpha
        if alpha == 'MASK': m['alphaCutoff'] = 0.5
        if double_sided: m['doubleSided'] = True
        self.j['materials'].append(m); return len(self.j['materials']) - 1

    def node(self, **kw):
        self.j['nodes'].append({k: v for k, v in kw.items() if v is not None}); return len(self.j['nodes']) - 1

    def primitive(self, P, N, UV, J, W, tris, material, targets=None):
        """Positions/normals in work space; joints/weights 4 per vertex."""
        attr = dict(POSITION=self.acc(to_gltf(P), 'VEC3', target=34962, minmax=True), NORMAL=self.acc(to_gltf(N), 'VEC3', target=34962),
                    TEXCOORD_0=self.acc(UV, 'VEC2', target=34962), JOINTS_0=self.acc(J, 'VEC4', 5123, target=34962), WEIGHTS_0=self.acc(W, 'VEC4', target=34962))
        idx = np.asarray(tris).reshape(-1)
        pr = dict(attributes=attr, indices=self.acc(idx, 'SCALAR', 5125 if idx.max() > 65535 else 5123, target=34963), material=material)
        if targets: pr['targets'] = [dict(POSITION=self.acc(to_gltf(d), 'VEC3', target=34962, minmax=True)) for d in targets]
        return pr

    def write(self, path):
        while len(self.buf) % 4: self.buf += b'\0'
        self.j['buffers'] = [dict(byteLength=len(self.buf))]
        j = {k: v for k, v in self.j.items() if v != []}
        js = json.dumps(j, separators=(',', ':')).encode()
        while len(js) % 4: js += b' '
        out = struct.pack('<III', 0x46546C67, 2, 12 + 8 + len(js) + 8 + len(self.buf)) + struct.pack('<II', len(js), 0x4E4F534A) + js + struct.pack('<II', len(self.buf), 0x004E4942) + bytes(self.buf)
        open(path, 'wb').write(out); return len(out)
