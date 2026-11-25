"""Deformation review: import the kit GLB (what the game loads) and render frames of every clip.
blender -b --factory-startup -P clip_sheet.py -- KIT.glb OUT_DIR [frames_per_clip] [view: front|side|34|back] [--show NODE,NODE]
Hairstyles and beards are hidden except those named with --show."""
import bpy, sys, os
from mathutils import Vector
a = sys.argv[sys.argv.index('--') + 1:]
show = a[a.index('--show') + 1].split(',') if '--show' in a else []
if '--show' in a: del a[a.index('--show'):a.index('--show') + 2]
glb, out = a[0], a[1]; nf = int(a[2]) if len(a) > 2 else 5; views = a[3:] or ['34']
os.makedirs(out, exist_ok=True)
bpy.ops.wm.read_factory_settings(use_empty=True)
bpy.ops.import_scene.gltf(filepath=glb)
sc = bpy.context.scene; arm = next(o for o in sc.objects if o.type == 'ARMATURE')
arm.hide_render = True
for o in sc.objects:                                   # this flat preview cannot show the alpha-blended brow layers
    if o.type == 'MESH' and o.name.startswith(('Brows_', 'Icosphere')): o.hide_render = True
    if o.type == 'MESH' and o.name.startswith(('Hair_', 'Beard_')): o.hide_render = o.name not in show
print('imported: bones', len(arm.data.bones), 'meshes', [o.name for o in sc.objects if o.type == 'MESH'], 'actions', [x.name for x in bpy.data.actions])
sc.render.engine = 'BLENDER_WORKBENCH'; sh = sc.display.shading; sh.light = 'STUDIO'; sh.color_type = 'TEXTURE'; sh.show_object_outline = False; sh.show_shadows = False; sh.show_specular_highlight = False
sc.view_settings.view_transform = 'Standard'; sc.world = bpy.data.worlds.new('w'); sc.world.color = (0.30, 0.31, 0.33)
cam = bpy.data.objects.new('cam', bpy.data.cameras.new('cam')); sc.collection.objects.link(cam); sc.camera = cam; cam.data.type = 'ORTHO'
DIR = {'front': (0, -1, 0.05), 'side': (1, -0.05, 0.05), '34': (0.65, -1, 0.15), 'back': (-0.3, 1, 0.15)}
sc.render.resolution_x, sc.render.resolution_y = 560, 760
order = ['Idle', 'Move_Forward', 'Move_Backward', 'Strafe_Left', 'Strafe_Right', 'Jump', 'Attack', 'Cast', 'Jump_Start', 'Jump_Land']
if arm.animation_data is None: arm.animation_data_create()
for name in order:
    act = next((x for x in bpy.data.actions if x.name == name or x.name.startswith(name + '_')), None)
    if act is None: print('missing clip', name); continue
    arm.animation_data.action = act
    if getattr(act, 'slots', None) and len(act.slots): arm.animation_data.action_slot = act.slots[0]
    f0, f1 = act.frame_range
    for v in views:
        d = Vector(DIR[v]).normalized(); cam.location = Vector((0, 0, 0.98)) + d * 6; cam.rotation_euler = (-d).to_track_quat('-Z', 'Y').to_euler(); cam.data.ortho_scale = 2.35
        for k in range(nf):
            sc.frame_set(int(round(f0 + (f1 - f0) * k / max(nf - 1, 1))))
            sc.render.filepath = os.path.join(out, f'{name}_{v}_{k}.png'); bpy.ops.render.render(write_still=True)
