"""Colour the fitted body by region (wireframe on top) to check the mask boundaries.
blender -b --factory-startup -P region_view.py -- OUT_PREFIX"""
import bpy, os, sys, numpy as np, colorsys
from mathutils import Vector
prefix = sys.argv[sys.argv.index('--') + 1]; KIT = os.environ.get('PITBORN_ART', os.path.expanduser('~/pitborn-art')) + '/human-male-kit/'
fit = np.load(KIT + 'work/fit_eyes.npz'); rg = np.load(KIT + 'work/regions.npz'); V, Q = fit['V'], fit['Q'][fit['face_keep']]; reg = rg['region']
bpy.ops.wm.read_factory_settings(use_empty=True); sc = bpy.context.scene
me = bpy.data.meshes.new('b'); me.from_pydata(V.tolist(), [], Q.tolist()); me.update()
for i in range(len(rg['names'])):
    m = bpy.data.materials.new(str(rg['names'][i])); h = (i * 0.618) % 1.0; m.diffuse_color = (*colorsys.hsv_to_rgb(h, 0.55 + 0.3 * (i % 2), 0.95 - 0.25 * (i % 3 == 0)), 1); me.materials.append(m)
me.polygons.foreach_set('material_index', reg.astype(np.int32))
for p in me.polygons: p.use_smooth = True
ob = bpy.data.objects.new('b', me); sc.collection.objects.link(ob)
w = ob.copy(); w.data = me.copy(); sc.collection.objects.link(w); md = w.modifiers.new('wf', 'WIREFRAME'); md.thickness = 0.0007; md.use_replace = True
bm = bpy.data.materials.new('wire'); bm.diffuse_color = (0, 0, 0, 1); w.data.materials.clear(); w.data.materials.append(bm)
sc.render.engine = 'BLENDER_WORKBENCH'; sh = sc.display.shading; sh.light = 'STUDIO'; sh.color_type = 'MATERIAL'; sh.show_object_outline = False; sh.show_specular_highlight = False
sc.world = bpy.data.worlds.new('w'); sc.world.color = (0.16, 0.16, 0.18)
cam = bpy.data.objects.new('cam', bpy.data.cameras.new('cam')); sc.collection.objects.link(cam); sc.camera = cam; cam.data.type = 'ORTHO'
for name, c, d, s, res in (('front', (0, 0, 0.905), (0, -1, 0), 1.92, (900, 1500)), ('back', (0, 0, 0.905), (0, 1, 0), 1.92, (900, 1500)), ('shoulder', (0.2, 0, 1.42), (0.5, -1, 0.5), 0.75, (1000, 1000)), ('side', (0, 0, 0.905), (1, 0, 0), 1.92, (700, 1500))):
    d_ = Vector(d).normalized(); cam.location = Vector(c) + d_ * 5; cam.rotation_euler = (-d_).to_track_quat('-Z', 'Y').to_euler(); cam.data.ortho_scale = s
    sc.render.resolution_x, sc.render.resolution_y = res; sc.render.filepath = f'{prefix}_{name}.png'; bpy.ops.render.render(write_still=True)
