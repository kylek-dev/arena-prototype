"""Rebuild assets/BaseFemale.glb from code (run inside Blender 5.3).

Background (fresh, empty scene):
    blender --background --factory-startup --python scripts/blender/base_female/build.py -- --clean

Inside a running Blender (e.g. via Blender MCP), exec this file; it replaces only
objects it owns (BF_* working meshes, BaseFemale_* outputs and the eight clips).

Options after `--`:
    --out PATH   output GLB (default: <repo>/assets/BaseFemale.glb)
    --clean      start from an empty scene first
"""
import os
import sys

import bpy

sys.dont_write_bytecode = True  # keep __pycache__ out of the repository
HERE = os.path.dirname(os.path.abspath(__file__))
REPO = os.path.abspath(os.path.join(HERE, "..", "..", ".."))
if HERE not in sys.path:
    sys.path.insert(0, HERE)
# Always load these modules from this directory, even if a session imported others.
for _name in [k for k in sys.modules if k.startswith("bf_")]:
    del sys.modules[_name]

import bf_anim, bf_build, bf_clips, bf_rigbuild  # noqa: E402

CLIP_NAMES = ["Idle", "Move_Forward", "Move_Backward", "Strafe_Left", "Strafe_Right", "Jump", "Attack", "Cast"]


def args():
    argv = sys.argv[sys.argv.index("--") + 1:] if "--" in sys.argv else []
    out = os.path.join(REPO, "assets", "BaseFemale.glb")
    if "--out" in argv:
        out = os.path.abspath(argv[argv.index("--out") + 1])
    return out, "--clean" in argv


def export(path):
    rig = bpy.data.objects[bf_rigbuild.RIG]
    rig.animation_data.action = None
    for t in list(rig.animation_data.nla_tracks):
        rig.animation_data.nla_tracks.remove(t)
    bf_anim.rest(rig)
    bpy.context.scene.frame_set(0)
    for me in list(bpy.data.meshes):
        if me.users == 0:
            bpy.data.meshes.remove(me)
    vl = bpy.context.view_layer
    for o in vl.objects:
        o.select_set(False)
    for o in bpy.data.objects:
        if o.name.startswith("BaseFemale_"):
            if o.type == 'MESH':
                o.data.validate(verbose=False)
                o.data.name = o.name
            o.hide_set(False)
            o.select_set(True)
    vl.objects.active = rig
    bpy.ops.export_scene.gltf(
        filepath=path, export_format='GLB', use_selection=True, export_yup=True,
        export_apply=False, export_cameras=False, export_lights=False, export_extras=False,
        export_skins=True, export_def_bones=False, export_influence_nb=4, export_animations=True,
        export_animation_mode='ACTIONS', export_force_sampling=True, export_anim_single_armature=True,
        export_rest_position_armature=True, export_frame_range=False, export_anim_slide_to_zero=True,
        export_optimize_animation_size=False, export_vertex_color='MATERIAL', export_morph=False,
        export_materials='EXPORT', export_texcoords=True, export_normals=True)


def main():
    out, clean = args()
    if clean:
        bpy.ops.wm.read_factory_settings(use_empty=True)
    bpy.context.scene.render.fps = 30
    bf_build.all_geometry()
    rig, _ = bf_rigbuild.rig_all()
    for name in CLIP_NAMES:  # drop stale clips so only the eight are exported
        a = bpy.data.actions.get(name)
        if a:
            bpy.data.actions.remove(a)
    bf_clips.build_all(rig)
    export(out)
    print("BaseFemale written to", out)


main()
