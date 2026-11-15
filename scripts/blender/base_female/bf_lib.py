"""Procedural helpers for the BaseFemale character (run inside Blender)."""
import math
import bpy
from mathutils import Vector, Matrix, Euler

V = Vector


def lin(c):
    """sRGB tuple -> linear tuple (alpha 1)."""
    def f(x):
        return x / 12.92 if x <= 0.04045 else ((x + 0.055) / 1.055) ** 2.4
    return (f(c[0]), f(c[1]), f(c[2]), 1.0)


def mix(a, b, t):
    t = max(0.0, min(1.0, t))
    return tuple(a[i] * (1 - t) + b[i] * t for i in range(len(a)))


def smooth01(t):
    t = max(0.0, min(1.0, t))
    return t * t * (3 - 2 * t)


def catmull(pts, vals, step):
    """Densify a polyline (with per-point scalar tuples) using Catmull-Rom."""
    P = [V(p) for p in pts]
    out_p, out_v = [], []
    n = len(P)
    for i in range(n - 1):
        p0 = P[i - 1] if i > 0 else P[i] * 2 - P[i + 1]
        p1, p2 = P[i], P[i + 1]
        p3 = P[i + 2] if i + 2 < n else P[i + 1] * 2 - P[i]
        seg = max(2, int(math.ceil((p2 - p1).length / step)))
        for k in range(seg):
            t = k / seg
            t2, t3 = t * t, t * t * t
            p = 0.5 * ((2 * p1) + (-p0 + p2) * t + (2 * p0 - 5 * p1 + 4 * p2 - p3) * t2 +
                       (-p0 + 3 * p1 - 3 * p2 + p3) * t3)
            v = tuple(vals[i][j] * (1 - t) + vals[i + 1][j] * t for j in range(len(vals[i])))
            out_p.append(p)
            out_v.append(v)
    out_p.append(P[-1])
    out_v.append(tuple(vals[-1]))
    return out_p, out_v


def sweep(pts, rads, ref=(1, 0, 0), ring=20, step=0.012, caps=(True, True),
          cap_scale=(1.0, 1.0), cap_rings=5):
    """Tube along a smooth path with elliptical sections (r_side, r_other).
    Returns verts, faces, uv (u along 0..1, v around 0..1)."""
    per_point = isinstance(ref, list)
    if per_point:
        rads = [tuple(r) + tuple(V(f).normalized()) for r, f in zip(rads, ref)]
    P, R = catmull(pts, rads, step)
    if per_point:
        REFS = [V(r[2:5]).normalized() for r in R]
        R = [r[:2] for r in R]
    else:
        REFS = [V(ref).normalized()] * len(P)
    m = len(P)
    T = []
    for i in range(m):
        a = P[max(i - 1, 0)]
        b = P[min(i + 1, m - 1)]
        T.append((b - a).normalized())
    rings, uvs = [], []

    def frame(t, ref):
        side = ref - t * ref.dot(t)
        if side.length < 1e-5:
            side = V((0, 0, 1)) - t * t.z
        side.normalize()
        other = t.cross(side)
        return side, other

    def add_ring(c, t, rs, ro, u, ref):
        side, other = frame(t, ref)
        r = []
        for k in range(ring):
            a = 2 * math.pi * k / ring
            r.append(c + side * (math.cos(a) * rs) + other * (math.sin(a) * ro))
        rings.append(r)
        uvs.append([(u, k / ring) for k in range(ring)])

    verts, faces, uv = [], [], []
    seq = []  # list of (center, tangent, rs, ro, u)
    if caps[0]:
        rs, ro = R[0][0], R[0][1]
        rc = min(rs, ro) * cap_scale[0]
        for j in range(cap_rings, 0, -1):
            ph = (math.pi / 2) * j / (cap_rings + 0.0)
            seq.append((P[0] - T[0] * (rc * math.sin(ph)), T[0], rs * math.cos(ph), ro * math.cos(ph), 0.0, REFS[0]))
    for i in range(m):
        seq.append((P[i], T[i], R[i][0], R[i][1], i / (m - 1), REFS[i]))
    if caps[1]:
        rs, ro = R[-1][0], R[-1][1]
        rc = min(rs, ro) * cap_scale[1]
        for j in range(1, cap_rings + 1):
            ph = (math.pi / 2) * j / (cap_rings + 0.0)
            seq.append((P[-1] + T[-1] * (rc * math.sin(ph)), T[-1], rs * math.cos(ph), ro * math.cos(ph), 1.0, REFS[-1]))
    for c, t, rs, ro, u, rf in seq:
        add_ring(c, t, max(rs, 1e-4), max(ro, 1e-4), u, rf)
    for r, ur in zip(rings, uvs):
        verts.extend(r)
        uv.extend(ur)
    nr = len(rings)
    for i in range(nr - 1):
        for k in range(ring):
            a = i * ring + k
            b = i * ring + (k + 1) % ring
            c = (i + 1) * ring + (k + 1) % ring
            d = (i + 1) * ring + k
            faces.append((a, b, c, d))
    # close ends with fans to a pole
    if True:  # start pole (also closes uncapped tubes)
        pole = len(verts)
        verts.append(sum(rings[0], V()) / ring)
        uv.append((0.0, 0.0))
        for k in range(ring):
            faces.append((pole, (k + 1) % ring, k))
    pole = len(verts)
    verts.append(sum(rings[-1], V()) / ring)
    uv.append((1.0, 0.0))
    base = (nr - 1) * ring
    for k in range(ring):
        faces.append((pole, base + k, base + (k + 1) % ring))
    return verts, faces, uv


def ellipsoid(center, radii, rot=(0, 0, 0), seg=24, rings=14):
    M = Euler([math.radians(a) for a in rot]).to_matrix()
    c = V(center)
    verts, faces, uv = [], [], []
    verts.append(c + M @ V((0, 0, -radii[2])))
    uv.append((0, 0))
    for i in range(1, rings):
        th = math.pi * i / rings - math.pi / 2
        for k in range(seg):
            ph = 2 * math.pi * k / seg
            p = V((math.cos(th) * math.cos(ph) * radii[0], math.cos(th) * math.sin(ph) * radii[1],
                   math.sin(th) * radii[2]))
            verts.append(c + M @ p)
            uv.append((i / rings, k / seg))
    verts.append(c + M @ V((0, 0, radii[2])))
    uv.append((1, 0))
    top = len(verts) - 1
    for k in range(seg):
        faces.append((0, 1 + (k + 1) % seg, 1 + k))
    for i in range(rings - 2):
        for k in range(seg):
            a = 1 + i * seg + k
            b = 1 + i * seg + (k + 1) % seg
            faces.append((a, b, b + seg, a + seg))
    last = 1 + (rings - 2) * seg
    for k in range(seg):
        faces.append((top, last + k, last + (k + 1) % seg))
    return verts, faces, uv


def mirror_part(part):
    verts, faces, uv = part
    mv = [V((-p.x, p.y, p.z)) for p in verts]
    mf = [tuple(reversed(f)) for f in faces]
    return mv, mf, uv


def build_object(name, parts, collection=None):
    verts, faces = [], []
    for pv, pf, _ in parts:
        off = len(verts)
        verts.extend([tuple(p) for p in pv])
        faces.extend([tuple(i + off for i in f) for f in pf])
    old = bpy.data.objects.get(name)
    if old:
        bpy.data.objects.remove(old, do_unlink=True)
    me = bpy.data.meshes.new(name)
    me.from_pydata(verts, [], faces)
    me.validate()
    me.update()
    ob = bpy.data.objects.new(name, me)
    (collection or bpy.context.scene.collection).objects.link(ob)
    return ob


def apply_mod(ob, kind, **props):
    m = ob.modifiers.new("tmp", kind)
    for k, v in props.items():
        setattr(m, k, v)
    dg = bpy.context.evaluated_depsgraph_get()
    ev = ob.evaluated_get(dg)
    me = bpy.data.meshes.new_from_object(ev)
    old = ob.data
    ob.modifiers.clear()
    ob.data = me
    me.name = ob.name
    if old.users == 0:
        bpy.data.meshes.remove(old)
    return ob


def shade_smooth(ob):
    for p in ob.data.polygons:
        p.use_smooth = True


def counts(ob):
    me = ob.data
    tris = sum(len(p.vertices) - 2 for p in me.polygons)
    return len(me.vertices), tris


def vc_material(name, rough=0.88, spec=0.15, sheen=0.0):
    """Matte material whose base color is the 'Col' color attribute."""
    if name.startswith("BF_"):
        name = "BaseFemale_" + name[3:]
    mat = bpy.data.materials.get(name) or bpy.data.materials.new(name)
    mat.use_nodes = True
    nt = mat.node_tree
    for n in list(nt.nodes):
        nt.nodes.remove(n)
    out = nt.nodes.new('ShaderNodeOutputMaterial')
    bsdf = nt.nodes.new('ShaderNodeBsdfPrincipled')
    attr = nt.nodes.new('ShaderNodeVertexColor')
    attr.layer_name = "Col"
    nt.links.new(attr.outputs[0], bsdf.inputs['Base Color'])
    nt.links.new(bsdf.outputs[0], out.inputs[0])
    bsdf.inputs['Roughness'].default_value = rough
    bsdf.inputs['Metallic'].default_value = 0.0
    for key in ('Specular IOR Level', 'Specular'):
        if key in bsdf.inputs:
            bsdf.inputs[key].default_value = spec
            break
    attr.location = (-300, 0)
    out.location = (300, 0)
    return mat


def set_material(ob, mat):
    ob.data.materials.clear()
    ob.data.materials.append(mat)
