"""Posture check: the torso's front and back outline at five heights, in the bind pose and in Idle.
Compare two kit GLBs to see whether a clip thrusts the chest out (ADR 0053, Evidence).
blender -b --factory-startup -P torso_profile.py -- KIT.glb"""
import bpy, sys, numpy as np
glb = sys.argv[sys.argv.index('--') + 1]
bpy.ops.wm.read_factory_settings(use_empty=True); bpy.ops.import_scene.gltf(filepath=glb)
sc = bpy.context.scene; arm = next(o for o in sc.objects if o.type == 'ARMATURE')
def profile(tag):
    dg = bpy.context.evaluated_depsgraph_get(); pts = []
    for o in sc.objects:
        if o.type != 'MESH' or o.name.startswith(('Brows', 'Eyes', 'Icosphere')): continue
        e = o.evaluated_get(dg); me = e.to_mesh(); co = np.empty(len(me.vertices) * 3); me.vertices.foreach_get('co', co)
        M = np.array(e.matrix_world); pts.append(co.reshape(-1, 3) @ M[:3, :3].T + M[:3, 3]); e.to_mesh_clear()
    P = np.concatenate(pts); P = P[np.abs(P[:, 0]) < 0.12]; top = P[:, 2].max(); s = 1.81 / top if tag == 'rest' else profile.s
    profile.s = s; P = P * s
    row = []
    for z0, z1 in ((1.36, 1.42), (1.28, 1.34), (1.20, 1.26), (1.12, 1.18), (1.04, 1.10)):
        b = P[(P[:, 2] > z0) & (P[:, 2] < z1)]; row.append('%.3f/%.3f' % (-b[:, 1].min(), -b[:, 1].max()))
    print('PROFILE %-5s front/back (m, +front) at chest 1.39, 1.31, 1.23, belly 1.15, waist 1.07: %s' % (tag, '  '.join(row)))
if arm.animation_data: arm.animation_data.action = None
for pb in arm.pose.bones: pb.location = (0, 0, 0); pb.rotation_quaternion = (1, 0, 0, 0); pb.rotation_euler = (0, 0, 0); pb.scale = (1, 1, 1)
bpy.context.view_layer.update(); profile('rest')
act = next(x for x in bpy.data.actions if x.name == 'Idle' or x.name.startswith('Idle_'))
if arm.animation_data is None: arm.animation_data_create()
arm.animation_data.action = act
if getattr(act, 'slots', None) and len(act.slots): arm.animation_data.action_slot = act.slots[0]
sc.frame_set(int(act.frame_range[0])); bpy.context.view_layer.update(); profile('idle')
