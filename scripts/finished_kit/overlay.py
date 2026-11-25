"""QA overlay: target (solid grey) with the fitted mesh drawn as a coloured wireframe on top.
blender -b --factory-startup -P overlay.py -- FIT.npz(V,Q) TARGET.npz(V,F) OUT_PREFIX [view names...]"""
import bpy, sys, os, numpy as np
from mathutils import Vector
a = sys.argv[sys.argv.index('--') + 1:]
fit = np.load(a[0]); tgt = np.load(a[1]); prefix = a[2]; want = a[3:]
bpy.ops.wm.read_factory_settings(use_empty=True)
def mk(name, V, F, color):
    me = bpy.data.meshes.new(name); me.from_pydata(V.tolist(), [], F.tolist()); me.update()
    for p in me.polygons: p.use_smooth = True
    ob = bpy.data.objects.new(name, me); bpy.context.scene.collection.objects.link(ob)
    m = bpy.data.materials.new(name); m.diffuse_color = color; me.materials.append(m); return ob
T = mk('target', tgt['V'], tgt['F'], (0.78, 0.76, 0.74, 1))
paint = '--paint' in a and 'color' in tgt
if paint:
    ca = T.data.color_attributes.new('Col', 'FLOAT_COLOR', 'POINT'); c = tgt['color'].astype(np.float32) ** 2.2
    ca.data.foreach_set('color', np.concatenate([c, np.ones((len(c), 1), np.float32)], 1).reshape(-1))
Q = fit['Q'] if 'Q' in fit else fit['F']
S = mk('fit', fit['V'], Q, (0.05, 0.25, 0.9, 1))
solid = '--solid' in want; want = [w for w in want if not w.startswith('--')]
thin = 0.0005 if paint else 0.0012
if not solid:
    w = S.modifiers.new('wf', 'WIREFRAME'); w.thickness = thin; w.use_replace = True
sc = bpy.context.scene; sc.render.engine = 'BLENDER_WORKBENCH'
sh = sc.display.shading; sh.light = 'STUDIO'; sh.color_type = 'MATERIAL'; sh.show_object_outline = False; sh.show_cavity = True
sh.color_type = 'VERTEX' if paint else 'MATERIAL'
if paint: sh.light = 'FLAT'; S.data.color_attributes.new('Col', 'FLOAT_COLOR', 'POINT').data.foreach_set('color', np.tile(np.array([0.0, 0.9, 1.0, 1.0], np.float32), len(S.data.vertices)))
sc.world = bpy.data.worlds.new('w'); sc.world.color = (0.16, 0.16, 0.18)
cam = bpy.data.objects.new('cam', bpy.data.cameras.new('cam')); sc.collection.objects.link(cam); sc.camera = cam; cam.data.type = 'ORTHO'
H = 1.81
VIEWS = {
 'body_front': ((0, 0, H / 2), (0, -1, 0), H * 1.06, (900, 1500)),
 'body_side': ((0, 0, H / 2), (1, 0, 0), H * 1.06, (700, 1500)),
 'body_back': ((0, 0, H / 2), (0, 1, 0), H * 1.06, (900, 1500)),
 'head_front': ((0, 0, H - 0.135), (0, -1, 0), 0.34, (1000, 1000)),
 'head_side': ((0, 0.02, H - 0.135), (1, 0, 0), 0.34, (1000, 1000)),
 'head_34': ((0, 0, H - 0.135), (0.7, -1, 0.1), 0.36, (1000, 1000)),
 'torso_front': ((0, 0, H * 0.72), (0, -1, 0), 0.9, (1000, 1000)),
 'hand_back': ((0.455, 0.0, 0.94), (0.8, -0.5, 0.35), 0.30, (1000, 1000)),
 'hand_palm': ((0.455, 0.0, 0.94), (-0.8, 0.5, -0.1), 0.30, (1000, 1000)),
 'hand_front': ((0.47, 0.0, 0.93), (0.15, -1, 0.05), 0.26, (900, 1000)),
 'hand_side': ((0.47, 0.0, 0.93), (1, 0.1, 0.25), 0.26, (900, 1000)),
 'feet': ((0, -0.05, 0.08), (0.4, -1, 0.5), 0.62, (1000, 1000)),
 'face_zoom': ((0, 0, H - 0.155), (0, -1, 0), 0.19, (1400, 1400)),
 'eye_zoom': ((0.035, 0, H - 0.118), (0, -1, 0), 0.075, (1400, 1000)),
 'nose': ((0, -0.05, H - 0.118), (0.25, -1, -0.45), 0.075, (1100, 900)),
 'ear': ((0.085, 0.03, H - 0.115), (1, -0.25, 0.05), 0.11, (900, 1100)),
 'mouth': ((0, -0.05, H - 0.152), (0.2, -1, -0.1), 0.09, (1100, 800)),
 'shoulder': ((0.2, 0, H * 0.8), (0.5, -1, 0.4), 0.6, (1000, 1000)),
}
modes = [('ov', False, False)] if not solid else [('fit', True, False), ('tgt', False, True)]
for name in want or VIEWS:
    c, d, scale, res = VIEWS[name]
    for tag, hide_t, hide_s in modes:
        T.hide_render = hide_t; S.hide_render = hide_s
        d_ = Vector(d).normalized(); cam.location = Vector(c) + d_ * 5; cam.rotation_euler = (-d_).to_track_quat('-Z', 'Y').to_euler()
        cam.data.ortho_scale = scale; sc.render.resolution_x, sc.render.resolution_y = res
        sc.render.filepath = f'{prefix}_{name}_{tag}.png'; bpy.ops.render.render(write_still=True)
