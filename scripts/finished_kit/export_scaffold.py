"""One-off export of the scaffold (the MakeHuman base mesh with the `game_engine` rig) from a Blender
session that has the MPFB add-on: create a human with MPFB (objects "Human" and "Human.rig"), then run
this in that session. It writes work/mh_base.npz, which mh.py loads.

This is the code that was run once in the live Blender on 2026-11-24 (MPFB 2.0.18, Blender 5.3). It has
not been re-run since; `src/rig.game_engine.json` and `src/targets/` were copied from the same add-on's
CC0 asset pack."""
import bpy, json, os
import numpy as np

OUT = os.environ.get('PITBORN_ART', os.path.expanduser('~/pitborn-art')) + '/human-male-kit/work'
h = bpy.data.objects["Human"]; rig = bpy.data.objects["Human.rig"]
# evaluated coordinates with shape keys only (modifiers off)
state = [(m.name, m.show_viewport) for m in h.modifiers]
for m in h.modifiers: m.show_viewport = False
dg = bpy.context.evaluated_depsgraph_get(); dg.update()
ev = h.evaluated_get(dg); me = ev.to_mesh()
n = len(me.vertices); V = np.empty(n * 3, np.float32); me.vertices.foreach_get("co", V); V = V.reshape(n, 3)
ev.to_mesh_clear()
for name, s in state: h.modifiers[name].show_viewport = s
me = h.data
sizes = np.empty(len(me.polygons), np.int32); me.polygons.foreach_get("loop_total", sizes)
lv = np.empty(len(me.loops), np.int32); me.loops.foreach_get("vertex_index", lv)
uv = np.empty(len(me.loops) * 2, np.float32); me.uv_layers[0].data.foreach_get("uv", uv); uv = uv.reshape(-1, 2)
ls = np.empty(len(me.polygons), np.int32); me.polygons.foreach_get("loop_start", ls)
gnames = [g.name for g in h.vertex_groups]
gi, gv, gw = [], [], []
for v in me.vertices:
    for ge in v.groups:
        gi.append(ge.group); gv.append(v.index); gw.append(ge.weight)
bones = [{"name": b.name, "parent": b.parent.name if b.parent else None, "head": list(b.head_local), "tail": list(b.tail_local),
          "matrix": [list(r) for r in b.matrix_local]} for b in rig.data.bones]
np.savez_compressed(os.path.join(OUT, 'mh_base.npz'), V=V, loop_vert=lv, loop_uv=uv, poly_start=ls, poly_size=sizes,
                    g_group=np.array(gi, np.int32), g_vert=np.array(gv, np.int32), g_weight=np.array(gw, np.float32),
                    group_names=np.array(gnames), bones=json.dumps(bones), rig_world=np.array(rig.matrix_world), obj_world=np.array(h.matrix_world))
print('saved', n, 'verts', len(me.polygons), 'polys', len(gnames), 'groups', len(bones), 'bones')
