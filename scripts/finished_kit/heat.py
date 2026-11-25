"""Render a fitted mesh coloured by a per-vertex scalar (default: distance to sculpt, 0..5 mm), plus wireframe views.
blender -b --factory-startup -P heat.py -- FIT.npz OUT_PREFIX MAX_MM [views...]"""
import bpy, sys, numpy as np
from mathutils import Vector
a = sys.argv[sys.argv.index('--') + 1:]
fit = np.load(a[0]); prefix = a[1]; vmax = float(a[2]); want = a[3:]
bpy.ops.wm.read_factory_settings(use_empty=True)
V, Q = fit['V'], fit['Q']; d = np.clip(fit['dist'] / vmax, 0, 1)
me = bpy.data.meshes.new('fit'); me.from_pydata(V.tolist(), [], Q.tolist()); me.update()
for p in me.polygons: p.use_smooth = True
col = me.color_attributes.new('Col', 'FLOAT_COLOR', 'POINT')
rgb = np.stack([np.clip(2 * d, 0, 1), np.clip(2 - 2 * d, 0, 1) * 0.85, 0.15 + 0 * d, 1 + 0 * d], 1)
if 'interior' in fit: rgb[fit['interior']] = (0.2, 0.3, 1.0, 1)
col.data.foreach_set('color', rgb.reshape(-1).astype(np.float32))
ob = bpy.data.objects.new('fit', me); bpy.context.scene.collection.objects.link(ob)
wf = ob.copy(); wf.data = me.copy(); bpy.context.scene.collection.objects.link(wf)
m = wf.modifiers.new('wf', 'WIREFRAME'); m.thickness = 0.0006; m.use_replace = True
mat = bpy.data.materials.new('w'); mat.diffuse_color = (0, 0, 0, 1); wf.data.materials.append(mat)
sc = bpy.context.scene; sc.render.engine = 'BLENDER_WORKBENCH'; sh = sc.display.shading
sh.light = 'STUDIO'; sh.show_object_outline = False
sc.world = bpy.data.worlds.new('w'); sc.world.color = (0.16, 0.16, 0.18)
cam = bpy.data.objects.new('cam', bpy.data.cameras.new('cam')); sc.collection.objects.link(cam); sc.camera = cam; cam.data.type = 'ORTHO'
H = 1.81
VIEWS = {
 'body_front': ((0, 0, H / 2), (0, -1, 0), H * 1.06, (900, 1500)), 'body_back': ((0, 0, H / 2), (0, 1, 0), H * 1.06, (900, 1500)),
 'head_front': ((0, 0, H - 0.135), (0, -1, 0), 0.34, (1000, 1000)), 'head_side': ((0, 0.02, H - 0.135), (1, 0, 0), 0.34, (1000, 1000)),
 'head_34': ((0, 0, H - 0.135), (0.7, -1, 0.1), 0.36, (1000, 1000)),
 'hand_back': ((0.455, 0.0, 0.94), (0.8, -0.5, 0.35), 0.30, (1000, 1000)), 'hand_palm': ((0.455, 0.0, 0.94), (-0.8, 0.5, -0.1), 0.30, (1000, 1000)),
 'feet': ((0, -0.05, 0.08), (0.4, -1, 0.5), 0.62, (1000, 1000)), 'torso_front': ((0, 0, H * 0.72), (0, -1, 0), 0.9, (1000, 1000)),
 'shoulder': ((0.2, 0, H * 0.8), (0.5, -1, 0.4), 0.6, (1000, 1000)), 'armpit': ((0.2, 0, H * 0.74), (0.3, -1, -0.5), 0.5, (1000, 1000)),
 'crotch': ((0, 0, 0.92), (0, -1, -0.6), 0.5, (1000, 1000)),
}
for name in want or VIEWS:
    c, dd, scale, res = VIEWS[name]
    d_ = Vector(dd).normalized(); cam.location = Vector(c) + d_ * 5; cam.rotation_euler = (-d_).to_track_quat('-Z', 'Y').to_euler()
    cam.data.ortho_scale = scale; sc.render.resolution_x, sc.render.resolution_y = res
    for tag, wire in (('heat', False), ('wire', True)):
        wf.hide_render = not wire; sh.color_type = 'VERTEX' if not wire else 'SINGLE'; sh.single_color = (0.8, 0.78, 0.75)
        sc.render.filepath = f'{prefix}_{name}_{tag}.png'; bpy.ops.render.render(write_still=True)
