"""Armature creation, skinning and weight fixes."""
import bpy
import bmesh
from mathutils import Vector as V
import bf_lib as L
import bf_rig as R

RIG = "BaseFemale_Rig"
ORDER = ["root", "pelvis", "spine_01", "spine_02", "spine_03", "neck", "head"]


def ordered_bones():
    b = R.all_bones()
    names = list(ORDER)
    rest = [n for n in b if n not in names]
    # parents before children
    while rest:
        for n in list(rest):
            if b[n][2] in names:
                names.append(n)
                rest.remove(n)
    return b, names


def ensure_object_mode():
    if bpy.context.object and bpy.context.object.mode != 'OBJECT':
        bpy.ops.object.mode_set(mode='OBJECT')


def build_armature():
    ensure_object_mode()
    old = bpy.data.objects.get(RIG)
    if old:
        arm = old.data
        bpy.data.objects.remove(old, do_unlink=True)
        if arm.users == 0:
            bpy.data.armatures.remove(arm)
    arm = bpy.data.armatures.new(RIG)
    ob = bpy.data.objects.new(RIG, arm)
    bpy.data.collections["BaseFemale"].objects.link(ob)
    vl = bpy.context.view_layer
    for o in vl.objects:
        o.select_set(False)
    vl.objects.active = ob
    ob.select_set(True)
    bpy.ops.object.mode_set(mode='EDIT')
    b, names = ordered_bones()
    for n in names:
        h, t, p, roll = b[n]
        eb = arm.edit_bones.new(n)
        eb.head, eb.tail = h, t
        eb.align_roll(roll)
        if p:
            eb.parent = arm.edit_bones[p]
            eb.use_connect = (arm.edit_bones[p].tail - h).length < 1e-4
        eb.use_deform = n != "root"
    bpy.ops.object.mode_set(mode='OBJECT')
    arm.display_type = 'STICK'
    ob.show_in_front = True
    return ob


def join_into(dst, src):
    bm = bmesh.new()
    bm.from_mesh(dst.data)
    bm.from_mesh(src.data)
    bm.to_mesh(dst.data)
    bm.free()
    bpy.data.objects.remove(src, do_unlink=True)


def rename(old, new):
    o = bpy.data.objects[old]
    o.name = new
    o.data.name = new
    return o


def auto_weights(mesh, rig):
    ensure_object_mode()
    vl = bpy.context.view_layer
    for o in vl.objects:
        o.select_set(False)
    mesh.select_set(True)
    rig.select_set(True)
    vl.objects.active = rig
    bpy.ops.object.parent_set(type='ARMATURE_AUTO')
    return mesh


def rigid(mesh, rig, bone):
    mesh.vertex_groups.clear()
    vg = mesh.vertex_groups.new(name=bone)
    vg.add(list(range(len(mesh.data.vertices))), 1.0, 'REPLACE')
    mesh.parent = rig
    m = mesh.modifiers.new("Armature", 'ARMATURE')
    m.object = rig


def normalize_limit(mesh, limit=4):
    """Keep the strongest `limit` influences per vertex and normalize (pure Python)."""
    names = [g.name for g in mesh.vertex_groups]
    data = []
    for v in mesh.data.vertices:
        ws = sorted(((g.weight, g.group) for g in v.groups if g.weight > 1e-4), reverse=True)[:limit]
        tot = sum(w for w, _ in ws) or 1.0
        data.append([(gi, w / tot) for w, gi in ws])
    used = {gi for ws in data for gi, _ in ws}
    for vg in mesh.vertex_groups:
        vg.remove(list(range(len(mesh.data.vertices))))
    for i, ws in enumerate(data):
        for gi, w in ws:
            mesh.vertex_groups[gi].add([i], w, 'REPLACE')
    dead = [names[i] for i in range(len(names)) if i not in used]
    for n in dead:
        mesh.vertex_groups.remove(mesh.vertex_groups[n])


def transfer_weights_py(dst, src, rig):
    """Nearest-surface barycentric weight transfer from src (rest mesh data)."""
    from mathutils.bvhtree import BVHTree
    sme = src.data
    sme.calc_loop_triangles()
    verts = [v.co.copy() for v in sme.vertices]
    tris = [tuple(t.vertices) for t in sme.loop_triangles]
    tree = BVHTree.FromPolygons(verts, tris)
    names = [g.name for g in src.vertex_groups]
    vw = [{} for _ in verts]
    for v in sme.vertices:
        for g in v.groups:
            if g.weight > 1e-5:
                vw[v.index][names[g.group]] = g.weight
    dst.vertex_groups.clear()
    groups = {n: dst.vertex_groups.new(name=n) for n in names}
    for v in dst.data.vertices:
        loc, nor, idx, d = tree.find_nearest(v.co)
        a, b, c = tris[idx]
        pa, pb, pc = verts[a], verts[b], verts[c]
        # barycentric coordinates
        v0, v1, v2 = pb - pa, pc - pa, loc - pa
        d00, d01, d11 = v0.dot(v0), v0.dot(v1), v1.dot(v1)
        d20, d21 = v2.dot(v0), v2.dot(v1)
        den = d00 * d11 - d01 * d01
        if abs(den) < 1e-14:
            w = (1.0, 0.0, 0.0)
        else:
            wb = (d11 * d20 - d01 * d21) / den
            wc = (d00 * d21 - d01 * d20) / den
            w = (max(0.0, 1 - wb - wc), max(0.0, wb), max(0.0, wc))
        acc = {}
        for vi, wi in zip((a, b, c), w):
            for n, x in vw[vi].items():
                acc[n] = acc.get(n, 0.0) + x * wi
        tot = sum(acc.values()) or 1.0
        for n, x in acc.items():
            groups[n].add([v.index], x / tot, 'REPLACE')
    dst.parent = rig
    dst.modifiers.clear()
    m = dst.modifiers.new("Armature", 'ARMATURE')
    m.object = rig


def smooth_weights(mesh, regions, iters=4, factor=0.5):
    """Laplacian-smooth all weights for vertices inside spherical regions [(center, radius)]."""
    me = mesh.data
    n = len(me.vertices)
    adj = [set() for _ in range(n)]
    for e in me.edges:
        a, b = e.vertices
        adj[a].add(b)
        adj[b].add(a)
    names = [g.name for g in mesh.vertex_groups]
    W = [{g.group: g.weight for g in v.groups} for v in me.vertices]
    mask = []
    for v in me.vertices:
        m = 0.0
        for c, r in regions:
            d = (v.co - V(c)).length
            m = max(m, 1.0 - L.smooth01((d - r * 0.6) / (r * 0.4)))
        mask.append(m)
    idx = [i for i in range(n) if mask[i] > 0]
    for _ in range(iters):
        new = {}
        for i in idx:
            if not adj[i]:
                continue
            acc = {}
            for j in adj[i]:
                for g, w in W[j].items():
                    acc[g] = acc.get(g, 0.0) + w
            k = len(adj[i])
            f = factor * mask[i]
            keys = set(acc) | set(W[i])
            new[i] = {g: W[i].get(g, 0.0) * (1 - f) + acc.get(g, 0.0) / k * f for g in keys}
        for i, w in new.items():
            W[i] = w
    for i in idx:
        for g, w in W[i].items():
            if w > 1e-5:
                mesh.vertex_groups[g].add([i], w, 'REPLACE')
            else:
                mesh.vertex_groups[g].remove([i])


def shrink_under(body, garments):
    """Pull body skin inward where a garment covers it (keeps the body complete)."""
    from mathutils.bvhtree import BVHTree
    me = body.data
    me.update()
    trees = []
    for g, depth in garments:
        dg = g.data
        dg.calc_loop_triangles()
        trees.append((BVHTree.FromPolygons([v.co for v in dg.vertices],
                                           [tuple(t.vertices) for t in dg.loop_triangles]), depth))
    moved = 0
    for v in me.vertices:
        push = 0.0
        for tree, depth in trees:
            hit = tree.ray_cast(v.co + v.normal * 0.0005, v.normal, 0.03)
            if hit[0] is not None:
                push = max(push, depth)
        if push:
            v.co = v.co - v.normal * push
            moved += 1
    me.update()
    return moved


def cull_hidden(body, garments, rings=2, reach=0.035):
    """Delete body faces fully covered by garments, keeping a margin of `rings` edge rings."""
    from mathutils.bvhtree import BVHTree
    me = body.data
    me.update()
    trees = []
    for g in garments:
        dg = g.data
        dg.calc_loop_triangles()
        trees.append(BVHTree.FromPolygons([v.co for v in dg.vertices],
                                          [tuple(t.vertices) for t in dg.loop_triangles]))
    n = len(me.vertices)
    cov = []
    for v in me.vertices:
        c = False
        for t in trees:
            if t.ray_cast(v.co + v.normal * 0.0005, v.normal, reach)[0] is not None:
                c = True
                break
        cov.append(c)
    adj = [set() for _ in range(n)]
    for e in me.edges:
        a, b = e.vertices
        adj[a].add(b)
        adj[b].add(a)
    deep = cov[:]
    for _ in range(rings):
        deep = [deep[i] and all(deep[j] for j in adj[i]) for i in range(n)]
    bm = bmesh.new()
    bm.from_mesh(me)
    bm.verts.ensure_lookup_table()
    dead = [f for f in bm.faces if all(deep[v.index] for v in f.verts)]
    bmesh.ops.delete(bm, geom=dead, context='FACES')
    loose = [v for v in bm.verts if not v.link_faces]
    bmesh.ops.delete(bm, geom=loose, context='VERTS')
    bm.to_mesh(me)
    bm.free()
    me.update()
    return len(dead)


GARMENTS = ("Tunic", "Leggings", "Boots", "Bracers")


def dup(src, name):
    o = bpy.data.objects.get(name)
    if o:
        bpy.data.objects.remove(o, do_unlink=True)
    s = bpy.data.objects[src]
    n = s.copy()
    n.data = s.data.copy()
    n.name = name
    n.data.name = name
    n.modifiers.clear()
    n.vertex_groups.clear()
    n.parent = None
    s.users_collection[0].objects.link(n)
    n.hide_set(False)
    return n


def rig_all():
    import bf_anim as A
    for o in list(bpy.data.objects):
        if o.name.startswith("BaseFemale_") and o.type == 'MESH':
            bpy.data.objects.remove(o, do_unlink=True)
    rig = build_armature()
    body = dup("BF_Body", "BaseFemale_Body")
    hands = dup("BF_HandsLo", "BF_HandsTmp")
    join_into(body, hands)
    L.shade_smooth(body)
    auto_weights(body, rig)
    regions = []
    for s in (1, -1):
        m = lambda v: (v.x * s, v.y, v.z)
        regions += [(m(R.S), 0.10), (m(R.HIP), 0.12), (m(R.KNEE), 0.08), (m(R.E), 0.06)]
    smooth_weights(body, regions, iters=6, factor=0.6)
    normalize_limit(body)
    G = {}
    for n in GARMENTS:
        o = dup("BF_" + n, "BaseFemale_" + n)
        transfer_weights_py(o, body, rig)
        normalize_limit(o)
        G[n] = o
    for src, name in (("BF_Face", "BaseFemale_Face"), ("BF_Hair", "BaseFemale_Hair")):
        rigid(dup(src, name), rig, "head")
    culled = cull_hidden(body, list(G.values()), rings=2)
    culled += cull_hidden(G["Leggings"], [G["Tunic"], G["Boots"]], rings=1, reach=0.04)
    shrink_under(body, [(G["Tunic"], 0.007), (G["Leggings"], 0.006), (G["Boots"], 0.009),
                        (G["Bracers"], 0.006)])
    normalize_limit(body)
    for o in bpy.data.objects:
        if o.name.startswith("BF_"):
            o.hide_set(True)
    return rig, culled
