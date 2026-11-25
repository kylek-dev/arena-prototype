"""Open the shipped kit GLB in Blender: structural checks and face/brow/preset renders.
blender -b --factory-startup -P check_glb.py -- KIT.glb OUT_PREFIX"""
import bpy, sys, numpy as np
from mathutils import Vector
a = sys.argv[sys.argv.index('--') + 1:]; glb, prefix = a[0], a[1]
bpy.ops.wm.read_factory_settings(use_empty=True); bpy.ops.import_scene.gltf(filepath=glb)
sc = bpy.context.scene; arm = next(o for o in sc.objects if o.type == 'ARMATURE'); arm.hide_render = True
for o in sc.objects:
    if o.type == 'MESH' and o.name.startswith('Icosphere'): o.hide_render = True
meshes = {o.name: o for o in sc.objects if o.type == 'MESH' and not o.name.startswith('Icosphere')}
print('meshes', len(meshes), 'tris', sum(sum(len(p.vertices) - 2 for p in o.data.polygons) for o in meshes.values()))
print('shape keys on Body_Head:', [k.name for k in meshes['Body_Head'].data.shape_keys.key_blocks] if meshes['Body_Head'].data.shape_keys else None)
print('sockets:', sorted(o.name for o in sc.objects if o.name.startswith('socket_')), '| bones', len(arm.data.bones))
print('materials:', sorted({m.name for o in meshes.values() for m in o.data.materials}))
# tint previews the way the game does (multiply base colour)
def tint(prefix_, rgb):
    for m in bpy.data.materials:
        if m.name.startswith(prefix_):
            b = next(n for n in m.node_tree.nodes if n.type == 'BSDF_PRINCIPLED'); link = b.inputs['Base Color'].links[0]; src = link.from_socket
            mx = m.node_tree.nodes.new('ShaderNodeMix'); mx.data_type = 'RGBA'; mx.blend_type = 'MULTIPLY'; mx.inputs['Factor'].default_value = 1; mx.inputs['B'].default_value = (*rgb, 1)
            m.node_tree.links.new(src, mx.inputs['A']); m.node_tree.links.new(mx.outputs['Result'], b.inputs['Base Color'])
tint('Brows_', (0.05, 0.035, 0.03)); tint('Eye_Iris', (0.20, 0.10, 0.05))
if '--skin' in a:                                      # a skin tone the way the game applies it: sRGB tint, multiplied in linear light
    t = [float(x) for x in a[a.index('--skin') + 1].split(',')]; lin = tuple(c / 12.92 if c <= 0.04045 else ((c + 0.055) / 1.055) ** 2.4 for c in t)
    tint('Skin', lin)
sun_shadow = '--no-shadow' not in a
sc.render.engine = 'BLENDER_EEVEE'
sun = bpy.data.objects.new('key', bpy.data.lights.new('key', 'SUN')); sun.data.energy = 3.0; sun.data.angle = 0.3; sun.rotation_euler = (0.9, 0.15, 0.6); sun.data.use_shadow = sun_shadow; sc.collection.objects.link(sun)
sc.world = bpy.data.worlds.new('w'); sc.world.use_nodes = True; bg = next(n for n in sc.world.node_tree.nodes if n.type == 'BACKGROUND'); bg.inputs['Color'].default_value = (0.5, 0.52, 0.56, 1); bg.inputs['Strength'].default_value = 0.9
sc.view_settings.view_transform = 'Standard'
cam = bpy.data.objects.new('cam', bpy.data.cameras.new('cam')); sc.collection.objects.link(cam); sc.camera = cam; cam.data.type = 'ORTHO'
arm.data.pose_position = 'REST'
def shot(name, c, d, scale, res=(900, 900)):
    d_ = Vector(d).normalized(); cam.location = Vector(c) + d_ * 5; cam.rotation_euler = (-d_).to_track_quat('-Z', 'Y').to_euler(); cam.data.ortho_scale = scale
    sc.render.resolution_x, sc.render.resolution_y = res; sc.render.filepath = f'{prefix}_{name}.png'; bpy.ops.render.render(write_still=True)
brows = [n for n in meshes if n.startswith('Brows_')]; keys = meshes['Body_Head'].data.shape_keys.key_blocks
combos = [('Brows_Natural', None), ('Brows_Bold', 'Broad'), ('Brows_Fine', 'Lean'), ('Brows_Natural', 'Rugged')]
for b, face in combos:
    for n in brows: meshes[n].hide_render = n != b
    for o in [meshes['Body_Head']] + [meshes[n] for n in brows]:
        for k in o.data.shape_keys.key_blocks[1:]: k.value = 1.0 if k.name == face else 0.0
    shot(f'{b}_{face or "Chiseled"}_front', (0, 0, 1.675), (0, -1, 0), 0.32); shot(f'{b}_{face or "Chiseled"}_34', (0, 0, 1.675), (0.7, -1, 0.05), 0.34)
for n in brows: meshes[n].hide_render = n != 'Brows_Natural'
for o in [meshes['Body_Head']] + [meshes[n] for n in brows]:
    for k in o.data.shape_keys.key_blocks[1:]: k.value = 0
shot('eyes', (0, 0, 1.692), (0, -1, 0), 0.12, (1600, 700)); shot('eye_34', (0.02, 0, 1.70), (0.75, -1, 0.1), 0.09, (1200, 800))
shot('body_front', (0, 0, 0.905), (0, -1, 0), 1.92, (800, 1300)); shot('body_34', (0, 0, 0.905), (0.7, -1, 0.1), 1.92, (800, 1300))
shot('body_back', (0, 0, 0.905), (0, 1, 0), 1.92, (800, 1300)); shot('back_close', (0, 0, 1.33), (0, 1, 0.1), 0.75); shot('waist', (0, 0, 1.08), (0.25, -1, 0.05), 0.6); shot('mouth', (0, 0, 1.652), (0.25, -1, -0.05), 0.11, (1100, 800)); shot('hems', (0, 0, 0.72), (-0.2, 1, 0.1), 0.75)
