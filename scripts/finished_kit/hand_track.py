"""Hand review: render close-ups that follow one hand through a clip, on the kit GLB the game loads.
blender -b --factory-startup -P hand_track.py -- KIT.glb OUT_PREFIX CLIP [bone=hand_r] [frames=6] [dir=x,y,z] [scale=0.55]"""
import bpy, sys, os
from mathutils import Vector
a = sys.argv[sys.argv.index('--') + 1:]; glb, out, clip = a[0], a[1], a[2]
bone = a[3] if len(a) > 3 else 'hand_r'; nf = int(a[4]) if len(a) > 4 else 6
d = Vector([float(x) for x in a[5].split(',')]).normalized() if len(a) > 5 else Vector((-0.7, -1, 0.25)).normalized(); scale = float(a[6]) if len(a) > 6 else 0.55
bpy.ops.wm.read_factory_settings(use_empty=True); bpy.ops.import_scene.gltf(filepath=glb)
sc = bpy.context.scene; arm = next(o for o in sc.objects if o.type == 'ARMATURE'); arm.hide_render = True
for o in sc.objects:
    if o.type == 'MESH' and o.name.startswith(('Icosphere', 'Brows_')): o.hide_render = True
sc.render.engine = 'BLENDER_WORKBENCH'; sh = sc.display.shading; sh.light = 'STUDIO'; sh.color_type = os.environ.get('COLOR', 'TEXTURE'); sh.show_object_outline = False; sh.show_cavity = os.environ.get('CAVITY', '1') == '1'; sh.show_specular_highlight = False
sc.view_settings.view_transform = 'Standard'; sc.world = bpy.data.worlds.new('w'); sc.world.color = (0.30, 0.31, 0.33)
cam = bpy.data.objects.new('cam', bpy.data.cameras.new('cam')); sc.collection.objects.link(cam); sc.camera = cam; cam.data.type = 'ORTHO'; cam.data.ortho_scale = scale
sc.render.resolution_x, sc.render.resolution_y = 520, 520
act = next(x for x in bpy.data.actions if x.name == clip or x.name.startswith(clip + '_'))
if arm.animation_data is None: arm.animation_data_create()
arm.animation_data.action = act
if getattr(act, 'slots', None) and len(act.slots): arm.animation_data.action_slot = act.slots[0]
f0, f1 = act.frame_range
for k in range(nf):
    sc.frame_set(int(round(f0 + (f1 - f0) * k / nf))); bpy.context.view_layer.update()
    pb = arm.pose.bones[bone]; c = arm.matrix_world @ pb.head
    cam.location = c + d * 6; cam.rotation_euler = (-d).to_track_quat('-Z', 'Y').to_euler()
    sc.render.filepath = f'{out}_{k}.png'; bpy.ops.render.render(write_still=True)
