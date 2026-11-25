"""Flat-textured renders of the fitted mesh with a baked texture, same cameras as ref_renders.py.
blender -b --factory-startup -P render_fit.py -- FIT.npz MH_BASE.npz TEXTURE.png OUT_PREFIX"""
import bpy, sys, numpy as np
from mathutils import Vector
a = sys.argv[sys.argv.index('--') + 1:]
fit = np.load(a[0]); base = np.load(a[1]); tex = a[2]; prefix = a[3]
bpy.ops.wm.read_factory_settings(use_empty=True)
V, Q = fit['V'], fit['Q']
me = bpy.data.meshes.new('fit'); me.from_pydata(V.tolist(), [], Q.tolist()); me.update()
names = [str(x) for x in base['group_names']]; body = np.zeros(len(base['V']), bool); body[base['g_vert'][base['g_group'] == names.index('body')]] = True
is_body = body[base['loop_vert'].reshape(-1, 4)].all(1); uv = base['loop_uv'].reshape(-1, 4, 2)[is_body].reshape(-1, 2)
me.uv_layers.new(name='UVMap').data.foreach_set('uv', uv.reshape(-1).astype(np.float32))
for p in me.polygons: p.use_smooth = True
mat = bpy.data.materials.new('paint'); nt = mat.node_tree
bsdf = next(n for n in nt.nodes if n.type == 'BSDF_PRINCIPLED'); img = nt.nodes.new('ShaderNodeTexImage'); img.image = bpy.data.images.load(tex)
nt.links.new(img.outputs['Color'], bsdf.inputs['Base Color']); nt.nodes.active = img
me.materials.append(mat)
ob = bpy.data.objects.new('fit', me); bpy.context.scene.collection.objects.link(ob)
sc = bpy.context.scene; sc.render.engine = 'BLENDER_WORKBENCH'; sh = sc.display.shading; sh.light = 'FLAT'; sh.color_type = 'TEXTURE'; sh.show_object_outline = False
sc.view_settings.view_transform = 'Standard'; sc.world = bpy.data.worlds.new('w'); sc.world.color = (0.35, 0.35, 0.37)
cam = bpy.data.objects.new('cam', bpy.data.cameras.new('cam')); sc.collection.objects.link(cam); sc.camera = cam; cam.data.type = 'ORTHO'
K = 1.81 / 1.70; H = 1.81
def shot(name, center, direction, scale, res=(1200, 1200)):
    sc.render.resolution_x, sc.render.resolution_y = res; c = Vector(center) * K; d = Vector(direction).normalized(); cam.location = c + d * 5
    cam.rotation_euler = (-d).to_track_quat('-Z', 'Y').to_euler(); cam.data.ortho_scale = scale * K
    sc.render.filepath = f'{prefix}_{name}.png'; bpy.ops.render.render(write_still=True)
h = 1.70                                               # same framing as the reference renders (which are at 1.70 m)
shot('face_front', (0, 0, h - 0.125), (0, -1, 0), 0.30); shot('face_34', (0, 0, h - 0.125), (0.7, -1, 0.05), 0.32); shot('face_side', (0, 0, h - 0.125), (1, 0, 0), 0.32)
shot('torso_front', (0, 0, h * 0.72), (0, -1, 0), 0.95); shot('body_front', (0, 0, h / 2), (0, -1, 0), h * 1.05, (900, 1500)); shot('body_back', (0, 0, h / 2), (0, 1, 0), h * 1.05, (900, 1500))
shot('hand', (0.42, 0, 0.88), (0.5, -1, 0.3), 0.3); shot('feet', (0, -0.05, 0.07), (0.4, -1, 0.5), 0.6)
