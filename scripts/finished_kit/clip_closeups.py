"""Close-up deformation checks on chosen clip frames.
blender -b --factory-startup -P clip_closeups.py -- KIT.glb OUT_DIR  clip:fraction:view:cx,cy,cz:scale ..."""
import bpy, sys, os
from mathutils import Vector
a = sys.argv[sys.argv.index('--') + 1:]; glb, out = a[0], a[1]; os.makedirs(out, exist_ok=True)
bpy.ops.wm.read_factory_settings(use_empty=True); bpy.ops.import_scene.gltf(filepath=glb)
sc = bpy.context.scene; arm = next(o for o in sc.objects if o.type == 'ARMATURE'); arm.hide_render = True
for o in sc.objects:
    if o.type == 'MESH' and o.name.startswith('Icosphere'): o.hide_render = True
sc.render.engine = 'BLENDER_WORKBENCH'; sh = sc.display.shading; sh.light = 'STUDIO'; sh.color_type = 'TEXTURE'; sh.show_object_outline = False; sh.show_cavity = True; sh.show_specular_highlight = False
sc.view_settings.view_transform = 'Standard'; sc.world = bpy.data.worlds.new('w'); sc.world.color = (0.30, 0.31, 0.33)
cam = bpy.data.objects.new('cam', bpy.data.cameras.new('cam')); sc.collection.objects.link(cam); sc.camera = cam; cam.data.type = 'ORTHO'
DIR = {'front': (0, -1, 0.05), 'side': (1, -0.05, 0.05), 'sideL': (-1, -0.05, 0.05), '34': (0.65, -1, 0.15), '34L': (-0.65, -1, 0.15), 'back': (-0.3, 1, 0.15), 'top': (0.2, -0.6, 1)}
sc.render.resolution_x = sc.render.resolution_y = 800
if arm.animation_data is None: arm.animation_data_create()
for i, spec in enumerate(a[2:]):
    clip, frac, view, c, scale = spec.split(':'); act = bpy.data.actions[clip]
    arm.animation_data.action = act
    if getattr(act, 'slots', None) and len(act.slots): arm.animation_data.action_slot = act.slots[0]
    f0, f1 = act.frame_range; sc.frame_set(int(round(f0 + (f1 - f0) * float(frac))))
    d = Vector(DIR[view]).normalized(); cam.location = Vector(tuple(map(float, c.split(',')))) + d * 6; cam.rotation_euler = (-d).to_track_quat('-Z', 'Y').to_euler(); cam.data.ortho_scale = float(scale)
    sc.render.filepath = os.path.join(out, f'{i:02d}_{clip}_{view}.png'); bpy.ops.render.render(write_still=True)
