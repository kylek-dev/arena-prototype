"""Front and back reference views of the rebuilt body (flat paint, plain grey ground) for the repaint edits,
plus the body-only GLB that is sent for retexturing (our topology and UVs, glTF axes).
blender -b --factory-startup -P ref_views.py -- BODY_TEXTURE.png OUT_DIR"""
import bpy, sys, os, numpy as np
from mathutils import Vector
a = sys.argv[sys.argv.index('--') + 1:]; tex, out = a[0], a[1]; os.makedirs(out, exist_ok=True)
bpy.ops.wm.open_mainfile(filepath=os.environ.get('PITBORN_ART', os.path.expanduser('~/pitborn-art')) + '/human-male-kit/work/preview_dense.blend')
sc = bpy.context.scene
body = bpy.data.objects['PB_Body']
for o in sc.objects:
    if o.type in ('CAMERA', 'LIGHT'): bpy.data.objects.remove(o, do_unlink=True)
cam = bpy.data.objects.new('cam', bpy.data.cameras.new('cam')); sc.collection.objects.link(cam); sc.camera = cam; cam.data.type = 'ORTHO'
sc.render.engine = 'BLENDER_WORKBENCH'; sh = sc.display.shading; sh.light = 'FLAT'; sh.color_type = 'TEXTURE'; sh.show_object_outline = False
sc.view_settings.view_transform = 'Standard'
if sc.world is None: sc.world = bpy.data.worlds.new('w')
sc.world.use_nodes = False; sc.world.color = (0.52, 0.52, 0.52)
sc.render.resolution_x, sc.render.resolution_y = 1152, 1536; H = 1.81
for name, d in (('front', (0, -1, 0)), ('back', (0, 1, 0))):
    d_ = Vector(d); cam.location = Vector((0, 0, H / 2)) + d_ * 5; cam.rotation_euler = (-d_).to_track_quat('-Z', 'Y').to_euler(); cam.data.ortho_scale = H / 0.88
    sc.render.filepath = os.path.join(out, f'view_{name}.png'); bpy.ops.render.render(write_still=True)
# body-only GLB for the retexture upload: geometry + UVs, no textures
for o in list(sc.objects):
    if o is not body and o.type == 'MESH': bpy.data.objects.remove(o, do_unlink=True)
body.data.materials.clear(); body.data.materials.append(bpy.data.materials.new('Skin'))
for o in sc.objects: o.select_set(o is body)
bpy.context.view_layer.objects.active = body
bpy.ops.export_scene.gltf(filepath=os.path.join(out, 'upload_body.glb'), export_format='GLB', use_selection=True, export_apply=True, export_yup=True,
                          export_texcoords=True, export_normals=True, export_materials='EXPORT', export_animations=False, export_skins=False)
print('glb bytes', os.path.getsize(os.path.join(out, 'upload_body.glb')))
