"""Geometry pipeline: raw volumes -> remesh -> decimate -> face/hair/clothes."""
import importlib
import bpy
import bf_lib, bf_rig, bf_body, bf_steps, bf_face, bf_hair, bf_clothes, bf_paint

BODY_TRIS = 10000
CLOTH_BASE_TRIS = 50000
HAND_TRIS = 3000


def reload_all():
    for m in (bf_lib, bf_rig, bf_body, bf_steps, bf_face, bf_hair, bf_clothes, bf_paint):
        importlib.reload(m)


def decimate(src, dst, tris, sym=True):
    L = bf_lib
    s = bpy.data.objects[src]
    o = bpy.data.objects.get(dst)
    if o:
        bpy.data.objects.remove(o, do_unlink=True)
    o = s.copy()
    o.data = s.data.copy()
    o.name = dst
    s.users_collection[0].objects.link(o)
    kw = dict(use_symmetry=True, symmetry_axis='X') if sym else {}
    L.apply_mod(o, 'DECIMATE', decimate_type='COLLAPSE', ratio=tris / sum(len(p.vertices) - 2 for p in s.data.polygons), **kw)
    L.shade_smooth(o)
    o.hide_set(False)
    return o


def geometry():
    L = bf_lib
    bf_steps.rebuild_raw()
    bf_steps.remesh("BF_BodyRaw", "BF_BodyRM", 0.0042, 30, head_protect=True)
    bf_steps.remesh("BF_HandsRaw", "BF_HandsRM", 0.0017, 6)
    body = decimate("BF_BodyRM", "BF_Body", BODY_TRIS)
    hands = decimate("BF_HandsRM", "BF_HandsLo", HAND_TRIS)
    decimate("BF_BodyRM", "BF_BodyCloth", CLOTH_BASE_TRIS).hide_set(True)
    face = bf_face.build("BF_BodyRM")
    hair = bf_hair.build("BF_BodyRM")
    bpy.data.objects["BF_BodyRM"].hide_set(True)
    bpy.data.objects["BF_HandsRM"].hide_set(True)
    return body, hands, face, hair


def all_geometry():
    L = bf_lib
    body, hands, face, hair = geometry()
    cl = bf_clothes.build()
    bf_paint.paint(body, bf_paint.skin)
    bf_paint.paint(hands, bf_paint.hand_skin)
    skin = L.vc_material("BF_Skin", rough=0.85, spec=0.2)
    L.set_material(body, skin)
    L.set_material(hands, skin)
    for k in ("tunic", "leggings", "boots", "bracers"):
        L.set_material(cl[k], L.vc_material("BF_" + k.capitalize(), rough=0.92, spec=0.1))
    L.set_material(hair, L.vc_material("BF_Hair", rough=0.8, spec=0.2))
    L.set_material(face, L.vc_material("BF_Face", rough=0.6, spec=0.25))
    objs = [body, hands, face, hair] + list(cl.values())
    tot = 0
    for o in objs:
        c = L.counts(o)
        tot += c[1]
        print(o.name, c)
    print("total tris", tot)
    return objs
