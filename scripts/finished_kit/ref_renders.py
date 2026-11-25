"""Reference renders of the raw Meshy models (flat texture, no lighting) for side-by-side review.
blender -b --factory-startup -P ref_renders.py -- OUT_DIR"""
import bpy, sys, os, math
from mathutils import Vector
out = sys.argv[sys.argv.index('--') + 1]
D = os.environ.get('PITBORN_ART', os.path.expanduser('~/pitborn-art')) + '/character-sheets/human/male/'
bpy.ops.wm.read_factory_settings(use_empty=True)
bpy.ops.import_scene.gltf(filepath=D + 'Meshy_AI_male_human.glb')
low = next(o for o in bpy.context.scene.objects if o.type == 'MESH'); arm = next(o for o in bpy.context.scene.objects if o.type == 'ARMATURE')
arm.data.pose_position = 'REST'; arm.hide_render = True
before = set(bpy.context.scene.objects)
bpy.ops.import_scene.gltf(filepath=D + 'Meshy_AI_male_human_detailed.glb')
hi = next(o for o in bpy.context.scene.objects if o not in before and o.type == 'MESH')
bpy.context.view_layer.update(); dg = bpy.context.evaluated_depsgraph_get()
def bounds(o):
    e = o.evaluated_get(dg); bb = [e.matrix_world @ Vector(c) for c in e.bound_box]
    return Vector([min(v[i] for v in bb) for i in range(3)]), Vector([max(v[i] for v in bb) for i in range(3)])
lmin, lmax = bounds(low); hmin, hmax = bounds(hi)
s = (lmax.z - lmin.z) / (hmax.z - hmin.z); hi.scale = hi.scale * s; bpy.context.view_layer.update(); hmin, hmax = bounds(hi)
hi.location += Vector(((lmin.x + lmax.x) / 2 - (hmin.x + hmax.x) / 2, (lmin.y + lmax.y) / 2 - (hmin.y + hmax.y) / 2, lmin.z - hmin.z))
print('dense->low scale', s, 'loc', tuple(hi.location))
sc = bpy.context.scene; sc.render.engine = 'BLENDER_WORKBENCH'
sh = sc.display.shading; sh.light = 'FLAT'; sh.color_type = 'TEXTURE'; sh.show_object_outline = False
sc.view_settings.view_transform = 'Standard'
sc.world = bpy.data.worlds.new('w'); sc.world.color = (0.35, 0.35, 0.37)
cam = bpy.data.objects.new('cam', bpy.data.cameras.new('cam')); sc.collection.objects.link(cam); sc.camera = cam; cam.data.type = 'ORTHO'
H = lmax.z
def shot(name, center, direction, scale, which, res=(1200, 1200)):
    low.hide_render = which != 'low'; hi.hide_render = which != 'dense'; name = name + ('_lit' if LIT else '')
    sc.render.resolution_x, sc.render.resolution_y = res
    c = Vector(center); d = Vector(direction).normalized(); cam.location = c + d * 5
    cam.rotation_euler = (-d).to_track_quat('-Z', 'Y').to_euler(); cam.data.ortho_scale = scale
    sc.render.filepath = os.path.join(out, f'{name}_{which}.png'); bpy.ops.render.render(write_still=True)
LIT = '--lit' in sys.argv
if LIT:
    sc.render.engine = 'BLENDER_EEVEE'
    sun = bpy.data.objects.new('key', bpy.data.lights.new('key', 'SUN')); sun.data.energy = 3.0; sun.data.angle = 0.3; sun.rotation_euler = (0.9, 0.15, 0.6); sc.collection.objects.link(sun)
    sc.world.use_nodes = True; bg = next(n for n in sc.world.node_tree.nodes if n.type == 'BACKGROUND'); bg.inputs['Color'].default_value = (0.5, 0.52, 0.56, 1); bg.inputs['Strength'].default_value = 0.9
extra = [x for x in sys.argv[sys.argv.index('--') + 2:] if not x.startswith('--')]
if extra:                                              # name:cx,cy,cz:dx,dy,dz:scale[:w,h]  (coordinates in the 1.70 m space)
    for spec in extra:
        p = spec.split(':'); c = tuple(map(float, p[1].split(','))); d = tuple(map(float, p[2].split(','))); res = tuple(map(int, p[4].split(','))) if len(p) > 4 else (1200, 1200)
        for which in ('low', 'dense'): shot(p[0], c, d, float(p[3]), which, res)
    sys.exit(0)
for which in ('low', 'dense'):
    shot('ref_face_front', (0, 0, H - 0.125), (0, -1, 0), 0.30, which)
    shot('ref_face_34', (0, 0, H - 0.125), (0.7, -1, 0.05), 0.32, which)
    shot('ref_face_side', (0, 0, H - 0.125), (1, 0, 0), 0.32, which)
    shot('ref_torso_front', (0, 0, H * 0.72), (0, -1, 0), 0.95, which)
    shot('ref_body_front', (0, 0, H / 2), (0, -1, 0), H * 1.05, which, (900, 1500))
    shot('ref_body_back', (0, 0, H / 2), (0, 1, 0), H * 1.05, which, (900, 1500))
    shot('ref_hand', (0.42, 0, 0.88), (0.5, -1, 0.3), 0.3, which)
    shot('ref_feet', (0, -0.05, 0.07), (0.4, -1, 0.5), 0.6, which)
