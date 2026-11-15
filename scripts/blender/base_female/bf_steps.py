import bpy
import bf_lib as L
import bf_body


def remesh(src, dst, vox, smooth_iters, head_protect=False, hand_iters=0):
    s = bpy.data.objects[src]
    o = bpy.data.objects.get(dst)
    if o:
        bpy.data.objects.remove(o, do_unlink=True)
    o = s.copy()
    o.data = s.data.copy()
    o.name = dst
    s.users_collection[0].objects.link(o)
    L.apply_mod(o, 'REMESH', mode='VOXEL', voxel_size=vox, adaptivity=0.0)
    if head_protect:
        vg = o.vertex_groups.new(name="smoothmask")
        for v in o.data.vertices:
            z = v.co.z
            w = 1.0 - L.smooth01((z - 1.47) / 0.05) * 0.85
            vg.add([v.index], w, 'REPLACE')
        L.apply_mod(o, 'SMOOTH', factor=0.9, iterations=smooth_iters, vertex_group="smoothmask")
        L.apply_mod(o, 'SMOOTH', factor=0.5, iterations=4)
        o.vertex_groups.clear()
    else:
        L.apply_mod(o, 'SMOOTH', factor=0.6, iterations=smooth_iters)
    L.shade_smooth(o)
    s.hide_set(True)
    s.hide_render = True
    return o


def rebuild_raw():
    b, h = bf_body.build()
    return b, h

