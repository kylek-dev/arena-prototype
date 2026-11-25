"""Concept inputs for hair and beards: the bald kit head on flat grey, lit softly, without shadows.
blender -b --factory-startup -P hair_refs.py -- KIT.glb OUT_DIR [--res N] [--scale S] [--center Z]
Writes head_front.png, head_side.png, head_back.png and head_34.png (front, character's left, back, three-quarter)."""
import bpy, sys, os
from mathutils import Vector
a = sys.argv[sys.argv.index('--') + 1:]; glb, out = a[0], a[1]
opt = lambda k, d: type(d)(a[a.index(k) + 1]) if k in a else d
res, scale, cz = opt('--res', 1280), opt('--scale', 0.42), opt('--center', 1.67)
os.makedirs(out, exist_ok=True)
bpy.ops.wm.read_factory_settings(use_empty=True); bpy.ops.import_scene.gltf(filepath=glb)
sc = bpy.context.scene; arm = next(o for o in sc.objects if o.type == 'ARMATURE'); arm.hide_render = True; arm.data.pose_position = 'REST'
for o in sc.objects:                                   # bust only: head, neck, shoulders, chest; one brow set; no trunks
    if o.type != 'MESH': continue
    o.hide_render = not (o.name.startswith(('Body_Head', 'Body_Neck', 'Body_Chest', 'Body_Shoulders', 'Body_UpperArms', 'Eyes')) or o.name == 'Brows_Natural')
def tint(prefix_, rgb):                                # multiply base colour, the way the game tints
    for m in bpy.data.materials:
        if m.name.startswith(prefix_):
            b = next(n for n in m.node_tree.nodes if n.type == 'BSDF_PRINCIPLED'); src = b.inputs['Base Color'].links[0].from_socket
            mx = m.node_tree.nodes.new('ShaderNodeMix'); mx.data_type = 'RGBA'; mx.blend_type = 'MULTIPLY'; mx.inputs['Factor'].default_value = 1; mx.inputs['B'].default_value = (*rgb, 1)
            m.node_tree.links.new(src, mx.inputs['A']); m.node_tree.links.new(mx.outputs['Result'], b.inputs['Base Color'])
tint('Brows_', (0.05, 0.035, 0.03)); tint('Eye_Iris', (0.20, 0.10, 0.05))
sc.render.engine = 'BLENDER_EEVEE'; sc.view_settings.view_transform = 'Standard'
for rot, e in (((0.9, 0.15, 0.6), 2.2), ((0.9, 0.0, 3.6), 1.4)):   # key from the front-left, fill from behind; no shadows
    s = bpy.data.objects.new('l', bpy.data.lights.new('l', 'SUN')); s.data.energy = e; s.data.angle = 0.5; s.data.use_shadow = False; s.rotation_euler = rot; sc.collection.objects.link(s)
sc.world = bpy.data.worlds.new('w'); sc.world.use_nodes = True; bg = next(n for n in sc.world.node_tree.nodes if n.type == 'BACKGROUND')
bg.inputs['Color'].default_value = (0.42, 0.42, 0.42, 1); bg.inputs['Strength'].default_value = 1.0
cam = bpy.data.objects.new('cam', bpy.data.cameras.new('cam')); sc.collection.objects.link(cam); sc.camera = cam; cam.data.type = 'ORTHO'; cam.data.ortho_scale = scale
sc.render.resolution_x = sc.render.resolution_y = res
for name, d in (('front', (0, -1, 0)), ('side', (1, 0, 0)), ('back', (0, 1, 0)), ('34', (0.7, -1, 0))):
    d_ = Vector(d).normalized(); cam.location = Vector((0, 0, cz)) + d_ * 5; cam.rotation_euler = (-d_).to_track_quat('-Z', 'Y').to_euler()
    sc.render.filepath = os.path.join(out, f'head_{name}.png'); bpy.ops.render.render(write_still=True)
