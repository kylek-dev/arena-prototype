"""Procedural keyframe authoring for BaseFemale.

Poses are written as rotations about *armature rest* axes (relative to the
parent), or as world aim directions. Left-side poses are mirrored to the right
when requested. Character faces -Y, left is +X, Z up.
"""
import math
import bpy
from mathutils import Vector as V, Matrix, Quaternion

FPS = 30
X, Y, Z = V((1, 0, 0)), V((0, 1, 0)), V((0, 0, 1))


def rot(axis, deg):
    return Matrix.Rotation(math.radians(deg), 3, V(axis).normalized())


def mirror_axis(a):
    a = V(a)
    return V((a.x, -a.y, -a.z))


def mirror_vec(v):
    v = V(v)
    return V((-v.x, v.y, v.z))


def mirror_name(n):
    if n.endswith(".L"):
        return n[:-2] + ".R"
    if n.endswith(".R"):
        return n[:-2] + ".L"
    return n


class Pose:
    def __init__(self, rig):
        self.rig = rig
        self.rel = {}    # bone -> 3x3 rotation in rest axes (relative to parent)
        self.aim = {}    # bone -> (world dir, twist deg)
        self.loc = {}    # bone -> world offset

    def r(self, bone, *rots):
        """rots: (axis, deg) pairs applied in order (first applied first)."""
        m = self.rel.get(bone, Matrix.Identity(3))
        for axis, deg in rots:
            m = rot(axis, deg) @ m
        self.rel[bone] = m
        return self

    def rs(self, bone, *rots):
        """symmetric: left as given, right mirrored."""
        self.r(bone + ".L", *rots)
        self.r(bone + ".R", *[(mirror_axis(a), d) for a, d in rots])
        return self

    def side(self, bone, side, *rots):
        if side > 0:
            return self.r(bone + ".L", *rots)
        return self.r(bone + ".R", *[(mirror_axis(a), d) for a, d in rots])

    def point(self, bone, direction, twist=0.0):
        self.aim[bone] = (V(direction).normalized(), twist)
        return self

    def move(self, bone, offset):
        self.loc[bone] = V(offset)
        return self

    def apply(self):
        rig = self.rig
        bones = rig.data.bones
        world = {}
        for b in bones:  # bones are parent-first
            Wp = world[b.parent.name] if b.parent else Matrix.Identity(3)
            if b.name in self.aim:
                d1, tw = self.aim[b.name]
                d0 = (b.tail_local - b.head_local).normalized()
                Wt = d0.rotation_difference(d1).to_matrix()
                if tw:
                    Wt = rot(d1, tw) @ Wt
                R = Wp.inverted() @ Wt
            else:
                R = self.rel.get(b.name, Matrix.Identity(3))
            world[b.name] = Wp @ R
            M = b.matrix_local.to_3x3().normalized()
            q = (M.inverted() @ R @ M).to_quaternion()
            pb = rig.pose.bones[b.name]
            pb.rotation_mode = 'QUATERNION'
            pb.rotation_quaternion = q
            off = self.loc.get(b.name)
            if off is not None:
                pb.location = M.inverted() @ off
            else:
                pb.location = (0, 0, 0)
            pb.scale = (1, 1, 1)


def key_all(rig, frame):
    for pb in rig.pose.bones:
        pb.keyframe_insert("rotation_quaternion", frame=frame)
        pb.keyframe_insert("location", frame=frame)


def make_action(rig, name, frames, pose_fn, loop):
    """pose_fn(t, Pose) where t in [0,1] across the clip."""
    old = bpy.data.actions.get(name)
    if old:
        bpy.data.actions.remove(old)
    if not rig.animation_data:
        rig.animation_data_create()
    act = bpy.data.actions.new(name)
    act.use_fake_user = True
    rig.animation_data.action = act
    prev = None
    for f in range(frames + 1):
        t = f / frames
        if loop and f == frames:
            t = 0.0
        p = Pose(rig)
        pose_fn(t, p)
        p.apply()
        key_all(rig, f)
    act.frame_range = (0, frames)
    act.use_frame_range = True
    act.use_cyclic = loop
    # make loops seamless in Blender preview
    rig.animation_data.action = None
    return act


def rest(rig):
    Pose(rig).apply()
