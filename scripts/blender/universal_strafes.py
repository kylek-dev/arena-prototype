"""Author reusable in-place lateral steps on the Quaternius universal rig.

Blender: --background --factory-startup --python scripts/blender/universal_strafes.py
Prepare the project normally first only if a dressed preview is wanted. Authoring
uses the CC0 UAL1 mannequin, not a female body, outfit, race or runtime GLB.
No previous scene, object, action or source file is removed or overwritten.
"""

import bpy
import math
from mathutils import Matrix, Vector


def build(source, destination):
    before = set(bpy.data.actions)
    scene = bpy.data.scenes.new("Universal_Strafe_Authoring")
    bpy.context.window.scene = scene
    bpy.ops.import_scene.gltf(filepath=source)
    rig = next(obj for obj in scene.objects if obj.type == 'ARMATURE')
    rig.name = "Universal_Strafe_Rig"
    rig.data.name = "Universal_Strafe_Skeleton"
    idle = next(a for a in bpy.data.actions if a not in before
                and a.name.split('.')[0] == "Idle_Loop")
    for track in rig.animation_data.nla_tracks:
        track.mute = True
    rig.animation_data.action = idle
    scene.frame_set(0)
    bpy.context.view_layer.update()
    base = {p.name: p.matrix.copy() for p in rig.pose.bones}
    rest = {b.name: b.matrix_local.copy() for b in rig.data.bones}
    parent = {b.name: b.parent.name if b.parent else None for b in rig.data.bones}
    order = []
    while len(order) < len(parent):
        additions = [n for n in parent if n not in order
                     and (parent[n] is None or parent[n] in order)]
        if not additions:
            raise ValueError("Skeleton hierarchy is not a tree")
        order.extend(additions)
    local = {n: base[parent[n]].inverted() @ base[n] if parent[n] else base[n]
             for n in order}
    rest_local = {n: rest[parent[n]].inverted() @ rest[n] if parent[n] else rest[n]
                  for n in order}
    leg = sum(rig.data.bones[n].length for n in ("thigh_l", "calf_l"))
    lane = max(abs(rest[n].translation.x) for n in ("thigh_l", "thigh_r")) + leg * 0.025
    step, lift = leg * 0.22, leg * 0.11
    quaternion_mode = next(i.identifier for i in
                           bpy.types.PoseBone.bl_rna.properties["rotation_mode"].enum_items
                           if i.name.startswith("Quaternion"))
    for p in rig.pose.bones:
        p.rotation_mode = quaternion_mode
    for track in list(rig.animation_data.nla_tracks):
        rig.animation_data.nla_tracks.remove(track)
    scene.render.fps = 30
    scene.render.fps_base = 1.0
    scene.frame_start, scene.frame_end = 0, 24
    authored = []

    def orient(start, end, reference, original_direction):
        turn = original_direction.rotation_difference(end - start)
        rotation = turn @ reference.to_quaternion()
        return Matrix.Translation(start) @ rotation.to_matrix().to_4x4()

    for label, direction in (("Strafe_Left", 1.0), ("Strafe_Right", -1.0)):
        action = bpy.data.actions.new("Universal_" + label)
        rig.animation_data.action = action
        previous = {}
        for frame in range(25):
            phase = (frame % 24) / 24.0
            half = phase * 2.0 if phase < 0.5 else (phase - 0.5) * 2.0
            smooth = half * half * (3.0 - 2.0 * half)
            if phase < 0.5:
                leading, trailing = lane + step * smooth, -lane - step * half
                lead_lift, trail_lift = lift * math.sin(math.pi * half), 0.0
            else:
                leading, trailing = lane + step * (1.0 - half), -lane - step * (1.0 - smooth)
                lead_lift, trail_lift = 0.0, lift * math.sin(math.pi * half)
            desired = {}
            for name in order:
                desired[name] = (desired[parent[name]] @ local[name]
                                 if parent[name] else base[name].copy())
                if name == "pelvis":
                    origin = base[name].translation
                    sway = direction * leg * (0.012 + 0.008 * math.cos(2 * math.pi * phase))
                    bob = leg * (-0.075 + 0.012 * (math.cos(4 * math.pi * phase) - 1))
                    desired[name] = (
                        Matrix.Translation(Vector((sway, 0, bob))) @
                        Matrix.Translation(origin) @
                        Matrix.Rotation(direction * math.radians(3.5), 4, 'Y') @
                        Matrix.Translation(-origin) @ base[name])
                elif name.startswith("upperarm_"):
                    side = 1.0 if name.endswith("_l") else -1.0
                    desired[name] @= Matrix.Rotation(
                        side * direction * 0.035 * math.sin(2 * math.pi * phase), 4, 'Z')
            for side in ("l", "r"):
                anatomical = 1.0 if side == "l" else -1.0
                is_leading = anatomical == direction
                x = direction * (leading if is_leading else trailing)
                foot_name, thigh_name, calf_name = "foot_" + side, "thigh_" + side, "calf_" + side
                ankle = rest[foot_name].translation.copy()
                ankle.x = x
                ankle.z += lead_lift if is_leading else trail_lift
                hip = desired[thigh_name].translation
                toward = ankle - hip
                distance = toward.length
                upper, lower = rig.data.bones[thigh_name].length, rig.data.bones[calf_name].length
                if not abs(upper - lower) < distance < upper + lower:
                    raise ValueError("Foot target outside unstretched leg reach")
                axis = toward.normalized()
                forward = Vector((0, -1, 0))
                pole = (forward - axis * forward.dot(axis)).normalized()
                along = (upper * upper - lower * lower + distance * distance) / (2 * distance)
                bend = math.sqrt(max(0.0, upper * upper - along * along))
                knee = hip + axis * along + pole * bend
                desired[thigh_name] = orient(hip, knee, base[thigh_name],
                                             base[calf_name].translation - base[thigh_name].translation)
                desired[calf_name] = orient(knee, ankle, base[calf_name],
                                            base[foot_name].translation - base[calf_name].translation)
                desired[foot_name] = rest[foot_name].copy()
                desired[foot_name].translation = ankle
            # Toes retain their flat rest pose relative to the solved feet.
            for name in order:
                if name.startswith("ball_"):
                    desired[name] = desired[parent[name]] @ rest_local[name]
            for name in order:
                basis = rest_local[name].inverted() @ (
                    desired[parent[name]].inverted() @ desired[name] if parent[name] else desired[name])
                p = rig.pose.bones[name]
                p.matrix_basis = basis
                if name in previous and p.rotation_quaternion.dot(previous[name]) < 0:
                    p.rotation_quaternion.negate()
                previous[name] = p.rotation_quaternion.copy()
                for channel in ("location", "rotation_quaternion", "scale"):
                    p.keyframe_insert(data_path=channel, frame=frame, group=name)
        linear = next(i.identifier for i in
                      bpy.types.Keyframe.bl_rna.properties["interpolation"].enum_items
                      if i.name == "Linear")
        for layer in action.layers:
            for strip in layer.strips:
                for bag in strip.channelbags:
                    for curve in bag.fcurves:
                        for key in curve.keyframe_points:
                            key.interpolation = linear
        authored.append((label, action))
    rig.animation_data.action = None
    for label, action in authored:
        track = rig.animation_data.nla_tracks.new()
        track.name = label
        track.strips.new(label, 0, action)
    # Tiny weighted carrier retains a standard skin, all 65 named joints and
    # inverse bind matrices, without shipping any mannequin/body/outfit geometry.
    mesh = bpy.data.meshes.new("Universal_Animation_Carrier_Mesh")
    z = rest["pelvis"].translation.z
    mesh.from_pydata([(0, 0, z), (0.001, 0, z), (0, 0.001, z)], [], [(0, 1, 2)])
    carrier = bpy.data.objects.new("Universal_Animation_Carrier", mesh)
    scene.collection.objects.link(carrier)
    carrier.parent = rig
    group = carrier.vertex_groups.new(name="pelvis")
    replace = next(i.identifier for i in
                   bpy.types.VertexGroup.bl_rna.functions["add"].parameters["type"].enum_items
                   if i.name == "Replace")
    group.add([0, 1, 2], 1.0, replace)
    armature_type = next(i.identifier for i in
                         bpy.types.Modifier.bl_rna.properties["type"].enum_items
                         if i.name == "Armature")
    modifier = carrier.modifiers.new("Universal_Skin", armature_type)
    modifier.object = rig
    for obj in scene.objects:
        obj.select_set(False)
    rig.select_set(True)
    carrier.select_set(True)
    bpy.context.view_layer.objects.active = rig
    scene.frame_set(0)
    props = bpy.ops.export_scene.gltf.get_rna_type().properties
    mode = next(i.identifier for i in props["export_animation_mode"].enum_items
                if i.name == "NLA Tracks")
    kwargs = dict(filepath=destination, use_selection=True,
                  export_animations=True, export_skins=True, export_animation_mode=mode)
    for key, value in (("export_force_sampling", True), ("export_def_bones", False),
                       ("export_frame_range", True), ("export_anim_slide_to_zero", True),
                       ("export_rest_position_armature", True), ("use_active_scene", True)):
        if key in props:
            kwargs[key] = value
    if "export_copyright" in props:
        kwargs["export_copyright"] = (
            "Reference humanoid skeleton/idle pose: Quaternius, CC0 1.0. "
            "New strafe motion: MIT, 2026 Arena Prototype contributors; see LICENSE.txt.")
    # Leave the export format at the operator's binary default. Its dynamic enum
    # is not exposed through RNA; no guessed enum identifiers are assigned.
    bpy.ops.export_scene.gltf(**kwargs)
    for track in rig.animation_data.nla_tracks:
        track.mute = True
    rig.animation_data.action = authored[0][1]
    rig.animation_data.action_slot = authored[0][1].slots[0]
    scene.frame_set(6)
    bpy.context.view_layer.update()
    print("Authored universal clips:", [label for label, _ in authored])
    print("Shared skeleton:", len(rig.data.bones), "joints; 0.8 second loops")
    print("Output:", destination)
    return rig


if __name__ == "__main__":
    from pathlib import Path
    root = Path(__file__).resolve().parents[2]
    destination = root / "assets/animations/universal-strafes.glb"
    destination.parent.mkdir(parents=True, exist_ok=True)
    build(str(root / "assets/quaternius/animation v1/Unreal-Godot/UAL1_Standard.glb"),
          str(destination))
