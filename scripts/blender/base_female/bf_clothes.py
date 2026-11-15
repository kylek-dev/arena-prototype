"""Base outfit as separate meshes cut from the body surface."""
import math
import bpy
import bmesh
from mathutils import Vector as V
from mathutils.bvhtree import BVHTree
import bf_lib as L
import bf_rig as R

BONES = {n: (h, t) for n, (h, t, p, r) in R.all_bones().items() if n != "root"}


def seg_info(p, h, t):
    d = t - h
    ln2 = d.length_squared
    s = max(0.0, min(1.0, (p - h).dot(d) / ln2))
    return (p - (h + d * s)).length, (p - h).dot(d) / ln2


def label(p):
    best, bn, bt = 1e9, None, 0
    for n, (h, t) in BONES.items():
        dist, s = seg_info(p, h, t)
        if dist < best:
            best, bn, bt = dist, n, s
    return bn, bt


def base(n):
    return n.split(".")[0]


def tunic_keep(p):
    n, t = label(p)
    b = base(n)
    if b in ("head", "hand", "forearm", "shin", "foot", "toe") or "_0" in b and b not in ("spine_01", "spine_02", "spine_03"):
        return False
    if b == "upper_arm":
        return t < 0.50
    if p.z < 0.835:
        return False
    if (p.x / 0.088) ** 2 + ((p.y - 0.012) / 0.085) ** 2 < 1 and p.z > 1.392 + 0.5 * (p.y + 0.05):
        return False
    if b == "neck":
        return False
    return True


def leggings_keep(p):
    n, t = label(p)
    b = base(n)
    if b in ("upper_arm", "forearm", "hand", "head", "neck", "clavicle"):
        return False
    return 0.20 < p.z < 0.985


def boots_keep(p):
    return p.z < 0.355


def bracer_keep(p):
    n, t = label(p)
    b = base(n)
    if b == "forearm":
        return t > 0.32 and t < 1.035
    if b == "hand":
        return False
    return False


def cut(body, keep, name):
    bm = bmesh.new()
    bm.from_mesh(body.data)
    bm.faces.ensure_lookup_table()
    dead = [f for f in bm.faces if not keep(f.calc_center_median())]
    bmesh.ops.delete(bm, geom=dead, context='FACES')
    # remove tiny islands
    bm.verts.ensure_lookup_table()
    loose = [v for v in bm.verts if not v.link_faces]
    bmesh.ops.delete(bm, geom=loose, context='VERTS')
    # relax boundary loops and snap back to the body surface
    tree = BVHTree.FromObject(body, bpy.context.evaluated_depsgraph_get())
    for _ in range(20):
        moves = {}
        for v in bm.verts:
            if not v.is_boundary:
                continue
            nb = [e.other_vert(v) for e in v.link_edges if e.is_boundary]
            if len(nb) != 2:
                continue
            avg = (nb[0].co + nb[1].co) * 0.5
            moves[v] = v.co.lerp(avg, 0.6)
        for v, co in moves.items():
            loc, nor, idx, d = tree.find_nearest(co)
            v.co = loc if loc else co
    me = bpy.data.meshes.new(name)
    bm.to_mesh(me)
    bm.free()
    old = bpy.data.objects.get(name)
    if old:
        bpy.data.objects.remove(old, do_unlink=True)
    ob = bpy.data.objects.new(name, me)
    body.users_collection[0].objects.link(ob)
    return ob


def keep_largest(ob, min_faces=40):
    bm = bmesh.new()
    bm.from_mesh(ob.data)
    seen = set()
    islands = []
    for f in bm.faces:
        if f.index in seen:
            continue
        stack, isl = [f], []
        seen.add(f.index)
        while stack:
            g = stack.pop()
            isl.append(g)
            for e in g.edges:
                for h in e.link_faces:
                    if h.index not in seen:
                        seen.add(h.index)
                        stack.append(h)
        islands.append(isl)
    dead = [f for isl in islands if len(isl) < min_faces for f in isl]
    bmesh.ops.delete(bm, geom=dead, context='FACES')
    loose = [v for v in bm.verts if not v.link_faces]
    bmesh.ops.delete(bm, geom=loose, context='VERTS')
    bm.to_mesh(ob.data)
    bm.free()


def offset(ob, fn):
    me = ob.data
    me.update()
    for v in me.vertices:
        v.co = v.co + v.normal * fn(v.co)


def edge_dist(ob, attr="edge", select=None):
    """Geodesic distance (along edges) from boundary vertices, stored as a float attribute."""
    import heapq
    me = ob.data
    n = len(me.vertices)
    adj = [[] for _ in range(n)]
    count = {}
    for p in me.polygons:
        for k in p.edge_keys:
            count[k] = count.get(k, 0) + 1
    for (a, b) in count:
        d = (me.vertices[a].co - me.vertices[b].co).length
        adj[a].append((b, d))
        adj[b].append((a, d))
    dist = [1e9] * n
    heap = []
    for (a, b), c in count.items():
        if c == 1:
            for i in (a, b):
                if select is None or select(me.vertices[i].co):
                    dist[i] = 0.0
                    heap.append((0.0, i))
    heapq.heapify(heap)
    while heap:
        d, i = heapq.heappop(heap)
        if d > dist[i]:
            continue
        for j, w in adj[i]:
            if d + w < dist[j]:
                dist[j] = d + w
                heapq.heappush(heap, (d + w, j))
    if attr in me.attributes:
        me.attributes.remove(me.attributes[attr])
    a = me.attributes.new(attr, 'FLOAT', 'POINT')
    for i in range(n):
        a.data[i].value = min(dist[i], 1.0)


def boundary_loops(ob):
    """Ordered closed boundary loops as lists of (co, normal)."""
    me = ob.data
    me.update()
    count = {}
    for p in me.polygons:
        for k in p.edge_keys:
            count[k] = count.get(k, 0) + 1
    nb = {}
    for (a, b), c in count.items():
        if c == 1:
            nb.setdefault(a, []).append(b)
            nb.setdefault(b, []).append(a)
    seen, loops = set(), []
    for start in nb:
        if start in seen or len(nb[start]) != 2:
            continue
        loop, prev, cur = [start], None, start
        seen.add(start)
        while True:
            nxt = [x for x in nb[cur] if x != prev]
            if not nxt:
                break
            prev, cur = cur, nxt[0]
            if cur == start or cur in seen or len(nb.get(cur, [])) != 2:
                break
            seen.add(cur)
            loop.append(cur)
        if len(loop) > 6:
            loops.append([(me.vertices[i].co.copy(), me.vertices[i].normal.copy()) for i in loop])
    return loops


def relax_boundary(ob, iters=30, f=0.5):
    me = ob.data
    count = {}
    for p in me.polygons:
        for k in p.edge_keys:
            count[k] = count.get(k, 0) + 1
    nb = {}
    for (a, b), c in count.items():
        if c == 1:
            nb.setdefault(a, []).append(b)
            nb.setdefault(b, []).append(a)
    for _ in range(iters):
        new = {}
        for i, ns in nb.items():
            if len(ns) == 2:
                co = me.vertices[i].co
                new[i] = co.lerp((me.vertices[ns[0]].co + me.vertices[ns[1]].co) * 0.5, f)
        for i, co in new.items():
            me.vertices[i].co = co
    me.update()


def piping(ob, width, depth, color, keep=None, spacing=0.015):
    parts, cols = [], []
    for loop in boundary_loops(ob):
        P = [c for c, n in loop]
        N = [n for c, n in loop]
        if keep and not keep(sum(P, V()) / len(P)):
            continue
        for _ in range(4):  # relax along the loop
            P = [P[i].lerp((P[i - 1] + P[(i + 1) % len(P)]) * 0.5, 0.7) for i in range(len(P))]
        # resample to even spacing
        total = sum((P[(i + 1) % len(P)] - P[i]).length for i in range(len(P)))
        k = max(8, int(total / spacing))
        step = total / k
        pts, nrm, acc, i = [], [], 0.0, 0
        seglen = [(P[(j + 1) % len(P)] - P[j]).length for j in range(len(P))]
        target = 0.0
        j, pos = 0, 0.0
        for s_ in range(k):
            target = s_ * step
            while pos + seglen[j] < target:
                pos += seglen[j]
                j = (j + 1) % len(P)
            t = (target - pos) / max(seglen[j], 1e-9)
            pts.append(P[j].lerp(P[(j + 1) % len(P)], t))
            nrm.append(N[j].lerp(N[(j + 1) % len(P)], t).normalized())
        pts = [p - n * 0.002 for p, n in zip(pts, nrm)]
        pts += pts[:3]
        nrm += nrm[:3]
        v, f, uv = L.sweep(pts, [(depth, width)] * len(pts), ref=nrm, ring=6, step=spacing,
                           caps=(True, True), cap_rings=2)
        parts.append((v, f, None))
        cols += [color] * len(v)
    return parts, cols


def append_parts(ob, parts, cols):
    bm = bmesh.new()
    bm.from_mesh(ob.data)
    layer = bm.verts.layers.float_color.get("Col")
    for (v, f, _), off in zip(parts, range(len(parts))):
        pass
    ci = 0
    for v, f, _ in parts:
        nv = []
        for p in v:
            vert = bm.verts.new(p)
            vert[layer] = cols[ci]
            ci += 1
            nv.append(vert)
        for face in f:
            try:
                bm.faces.new([nv[i] for i in face])
            except ValueError:
                pass
    bm.to_mesh(ob.data)
    bm.free()
    L.shade_smooth(ob)


def finish(ob, thickness, smooth_iters=4, even=False, tris=None):
    if tris:
        cur = sum(len(p.vertices) - 2 for p in ob.data.polygons)
        if cur > tris:
            L.apply_mod(ob, 'DECIMATE', decimate_type='COLLAPSE', ratio=tris / cur,
                        use_symmetry=True, symmetry_axis='X')
    if smooth_iters:
        L.apply_mod(ob, 'SMOOTH', factor=0.5, iterations=smooth_iters)
    edge_dist(ob)
    L.apply_mod(ob, 'SOLIDIFY', thickness=thickness, offset=-1.0, use_even_offset=even, use_rim=True,
                use_quality_normals=True)
    L.shade_smooth(ob)


def paint(ob, fn):
    me = ob.data
    if "Col" in me.color_attributes:
        me.color_attributes.remove(me.color_attributes["Col"])
    a = me.color_attributes.new("Col", 'FLOAT_COLOR', 'POINT')
    me.update()
    ed = me.attributes.get("edge")
    for v in me.vertices:
        e = ed.data[v.index].value if ed else 1.0
        a.data[v.index].color = fn(v.co, v.normal, e)


def shade(base_srgb, n, p, top=0.22, low=0.18):
    c = L.lin(base_srgb)
    light = n.z * 0.6 - n.y * 0.15
    hi = tuple(min(1.0, x * 1.55 + 0.02) for x in c[:3]) + (1.0,)
    lo = tuple(x * 0.55 for x in c[:3]) + (1.0,)
    if light > 0:
        c = L.mix(c, hi, light * top / 0.6)
    else:
        c = L.mix(c, lo, -light * low / 0.6)
    return c


TEAL = (0.09, 0.44, 0.53)
TEAL_D = (0.05, 0.27, 0.36)
GOLD = (0.92, 0.70, 0.30)
PLUM = (0.33, 0.19, 0.33)
LEATHER = (0.47, 0.27, 0.14)
LEATHER_L = (0.66, 0.45, 0.25)
BELT = (0.30, 0.16, 0.09)


def tunic_color(p, n, e):
    c = shade(TEAL, n, p)
    # darker toward the hem, painted gradient
    c = L.mix(c, shade(TEAL_D, n, p), 0.6 * L.smooth01((1.05 - p.z) / 0.2))
    return c


def leggings_color(p, n, e):
    c = shade(PLUM, n, p)
    # subtle painted knee/shin panel
    return L.mix(c, shade((0.26, 0.14, 0.27), n, p), 0.5 * L.smooth01((0.6 - p.z) / 0.3))


def boot_color(p, n, e):
    c = shade(LEATHER, n, p)
    c = L.mix(c, shade(LEATHER_L, n, p), 0.6 * L.smooth01((p.z - 0.285) / 0.04))
    c = L.mix(c, shade((0.22, 0.12, 0.07), n, p), 1 - L.smooth01((p.z - 0.016) / 0.006))
    return c


def bracer_color(p, n, e):
    return shade(LEATHER, n, p)


def build(body_name="BF_BodyCloth"):
    body = bpy.data.objects[body_name]
    out = {}
    t = cut(body, tunic_keep, "BF_Tunic")
    keep_largest(t)
    offset(t, lambda p: 0.007 + 0.010 * L.smooth01((0.95 - p.z) / 0.11))
    L.apply_mod(t, 'SMOOTH', factor=0.5, iterations=5)
    relax_boundary(t, 40)
    pp = piping(t, 0.0085, 0.0045, L.lin(GOLD))
    finish(t, 0.004, 0, tris=2200)
    paint(t, tunic_color)
    append_parts(t, *pp)
    out["tunic"] = t
    g = cut(body, leggings_keep, "BF_Leggings")
    keep_largest(g)
    offset(g, lambda p: 0.0032)
    finish(g, 0.003, 2, tris=1400)
    paint(g, leggings_color)
    out["leggings"] = g
    b = cut(body, boots_keep, "BF_Boots")
    keep_largest(b)
    offset(b, lambda p: 0.010 + 0.007 * L.smooth01((p.z - 0.29) / 0.05))
    for v in b.data.vertices:
        if v.co.z < 0.0:
            v.co.z = 0.0
    L.apply_mod(b, 'SMOOTH', factor=0.5, iterations=3)
    relax_boundary(b, 30)
    pp = piping(b, 0.011, 0.006, L.lin(LEATHER_L))
    finish(b, 0.005, 0, tris=1100)
    for v in b.data.vertices:
        if v.co.z < 0.0:
            v.co.z = 0.0
    paint(b, boot_color)
    append_parts(b, *pp)
    out["boots"] = b
    r = cut(body, bracer_keep, "BF_Bracers")
    keep_largest(r)
    offset(r, lambda p: 0.0065 + 0.004 * L.smooth01((label(p)[1] - 0.85) / 0.15))
    L.apply_mod(r, 'SMOOTH', factor=0.5, iterations=2)
    relax_boundary(r, 30)
    pp = piping(r, 0.0065, 0.004, L.lin(GOLD))
    finish(r, 0.004, 0, tris=360)
    paint(r, bracer_color)
    append_parts(r, *pp)
    out["bracers"] = r
    return out
