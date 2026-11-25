"""Preview assembly: body (socket pockets removed) + eye pieces with their materials, flat-textured and lit renders.
blender -b --factory-startup -P assemble_preview.py -- BODY_TEXTURE.png OUT_PREFIX [--save FILE.blend] [--iris R,G,B]"""
import bpy, os, sys, numpy as np
from mathutils import Vector
a = sys.argv[sys.argv.index('--') + 1:]
KIT = os.environ.get('PITBORN_ART', os.path.expanduser('~/pitborn-art')) + '/human-male-kit/'
body_tex, prefix = a[0], a[1]
iris_tint = tuple(map(float, a[a.index('--iris') + 1].split(','))) if '--iris' in a else (0.42, 0.25, 0.14)
bpy.ops.wm.read_factory_settings(use_empty=True)
fit = np.load(KIT + 'work/fit_eyes.npz'); base = np.load(KIT + 'work/mh_base.npz'); ep = np.load(KIT + 'work/eye_parts.npz')
sc = bpy.context.scene

def material(name, tex=None, tint=(1, 1, 1), rough=0.9, normal=None):
    m = bpy.data.materials.new(name); nt = m.node_tree; b = next(n for n in nt.nodes if n.type == 'BSDF_PRINCIPLED')
    b.inputs['Roughness'].default_value = rough; b.inputs['Metallic'].default_value = 0
    if tex:
        t = nt.nodes.new('ShaderNodeTexImage'); t.image = bpy.data.images.load(tex); nt.nodes.active = t
        if tint != (1, 1, 1):                             # preview only: bake the runtime tint (sRGB multiply) into a copy of the texture
            px = np.array(t.image.pixels[:], np.float32).reshape(-1, 4); srgb = np.where(px[:, :3] <= 0.0031308, px[:, :3] * 12.92, 1.055 * np.maximum(px[:, :3], 1e-9) ** (1 / 2.4) - 0.055)
            srgb *= np.array(tint, np.float32); px[:, :3] = np.where(srgb <= 0.04045, srgb / 12.92, ((srgb + 0.055) / 1.055) ** 2.4)
            im = bpy.data.images.new(name + '_tinted', *t.image.size); im.pixels.foreach_set(px.reshape(-1)); im.pack(); t.image = im
        nt.links.new(t.outputs['Color'], b.inputs['Base Color'])
    if normal:
        tn = nt.nodes.new('ShaderNodeTexImage'); tn.image = bpy.data.images.load(normal); tn.image.colorspace_settings.name = 'Non-Color'
        nm = nt.nodes.new('ShaderNodeNormalMap'); nt.links.new(tn.outputs['Color'], nm.inputs['Color']); nt.links.new(nm.outputs['Normal'], b.inputs['Normal'])
    return m

def mesh(name, V, faces, uvs, mat):
    me = bpy.data.meshes.new(name); me.from_pydata([tuple(v) for v in V], [], [tuple(int(i) for i in f) for f in faces]); me.update()
    uv = me.uv_layers.new(name='UVMap'); uv.data.foreach_set('uv', np.asarray(uvs, np.float32).reshape(-1))
    for p in me.polygons: p.use_smooth = True
    me.materials.append(mat); ob = bpy.data.objects.new(name, me); sc.collection.objects.link(ob); return ob

# body: kept quads only, unused verts dropped
V, Q, keep = fit['V'], fit['Q'], fit['face_keep']
if '--morph' in a:                                    # preview a face preset
    fm = np.load(KIT + 'work/face_morphs.npz'); V = V + fm['deltas'][[str(x) for x in fm['names']].index(a[a.index('--morph') + 1])]
names = [str(x) for x in base['group_names']]; bm = np.zeros(len(base['V']), bool); bm[base['g_vert'][base['g_group'] == names.index('body')]] = True
is_body = bm[base['loop_vert'].reshape(-1, 4)].all(1); uvq = base['loop_uv'].reshape(-1, 4, 2)[is_body][keep]
Qk = Q[keep]; used = np.unique(Qk); remap = -np.ones(len(V), int); remap[used] = np.arange(len(used))
body = mesh('PB_Body', V[used], remap[Qk], uvq.reshape(-1, 2), material('Skin', body_tex, normal=KIT + 'work/bake_normal.png' if '--normal' in a else None))
print('body: %d verts, %d quads, %d tris' % (len(used), len(Qk), 2 * len(Qk)))
# eyes
m_white = material('Eye_White', KIT + 'work/tex_eye_white.png'); m_iris = material('Eye_Iris', KIT + 'work/tex_eye_iris.png', iris_tint)
for side, sx in (('L', 1), ('R', -1)):
    S = np.array([sx, 1, 1.0])
    flip = (lambda f: f[::-1]) if sx < 0 else (lambda f: f)
    cuv = ep['cap_UV'].copy(); cuv[:, 0] *= 0.75                                         # eye white lives on the left 3/4 of its sheet
    cq = [flip(list(q)) for q in ep['cap_T']] + [flip(list(q)) for q in ep['cap_Q']]
    mesh('PB_EyeWhite_' + side, ep['cap_V'] * S, cq, [cuv[i] for q in cq for i in q], m_white)
    faces = [flip(list(t)) for t in ep['iris_tris']] + [flip(list(q)) for q in ep['iris_quads']]
    iuv = ep['iris_UV'].copy()
    if sx < 0: iuv[:, 0] = 1 - iuv[:, 0]
    mesh('PB_Iris_' + side, ep['iris_V'] * S, faces, [iuv[i] for f in faces for i in f], m_iris)
    # catchlights: two small discs just in front of the iris, on the same side in both eyes (as painted)
    c = ep['iris_centre'] * np.array([sx, 1.0]); r = float(ep['iris_radius']); y0 = ep['iris_V'][0, 1] - 0.0001
    gv, gf = [], []
    for (ox, oz, rr) in ((-0.22, 0.30, 0.17), (0.25, 0.22, 0.11)):
        b0 = len(gv); gv.append((c[0] + ox * r, y0, c[1] + oz * r))
        for s in range(10):
            ang = 2 * np.pi * s / 10; gv.append((c[0] + ox * r + rr * r * np.cos(ang) * 0.8, y0, c[1] + oz * r + rr * r * np.sin(ang)))
        gf += [(b0, b0 + 1 + s, b0 + 1 + (s + 1) % 10) for s in range(10)]
    mesh('PB_EyeGlint_' + side, gv, gf, [(0.875, 0.5)] * (3 * len(gf)), m_white)
tris = sum(len(p.vertices) - 2 for o in sc.objects if o.type == 'MESH' for p in o.data.polygons)
print('total triangles (body + eyes): %d' % tris)
if '--save' in a:
    bpy.ops.file.pack_all(); bpy.ops.wm.save_as_mainfile(filepath=a[a.index('--save') + 1])

cam = bpy.data.objects.new('cam', bpy.data.cameras.new('cam')); sc.collection.objects.link(cam); sc.camera = cam; cam.data.type = 'ORTHO'
sc.view_settings.view_transform = 'Standard'; sc.world = bpy.data.worlds.new('w'); sc.world.color = (0.35, 0.35, 0.37)
K = 1.81 / 1.70; h = 1.70
VIEWS = {'face_front': ((0, 0, h - 0.125), (0, -1, 0), 0.30, (1200, 1200)), 'face_34': ((0, 0, h - 0.125), (0.7, -1, 0.05), 0.32, (1200, 1200)),
         'face_side': ((0, 0, h - 0.125), (1, 0, 0), 0.32, (1200, 1200)), 'eyes': ((0, 0, 1.589), (0, -1, 0), 0.1127, (1600, 700)),
         'eye_34': ((0.02, 0, 1.6), (0.75, -1, 0.1), 0.085, (1400, 900)),
         'torso_front': ((0, 0, h * 0.72), (0, -1, 0), 0.95, (1200, 1200)), 'body_front': ((0, 0, h / 2), (0, -1, 0), h * 1.05, (900, 1500)),
         'body_back': ((0, 0, h / 2), (0, 1, 0), h * 1.05, (900, 1500)), 'hand': ((0.42, 0, 0.88), (0.5, -1, 0.3), 0.3, (1200, 1200)), 'feet': ((0, -0.05, 0.07), (0.4, -1, 0.5), 0.6, (1200, 1200))}
want = [x for x in a[2:] if x in VIEWS] or list(VIEWS)
def shots(tag):
    for name in want:
        c, d, scale, res = VIEWS[name]; sc.render.resolution_x, sc.render.resolution_y = res
        d_ = Vector(d).normalized(); cam.location = Vector(c) * K + d_ * 5; cam.rotation_euler = (-d_).to_track_quat('-Z', 'Y').to_euler(); cam.data.ortho_scale = scale * K
        sc.render.filepath = f'{prefix}_{name}_{tag}.png'; bpy.ops.render.render(write_still=True)
sc.render.engine = 'BLENDER_WORKBENCH'; sh = sc.display.shading; sh.light = 'FLAT'; sh.color_type = 'TEXTURE'; sh.show_object_outline = False
shots('flat')
if '--lit' in a:
    sc.render.engine = 'BLENDER_EEVEE'
    sun = bpy.data.objects.new('key', bpy.data.lights.new('key', 'SUN')); sun.data.energy = 3.0; sun.data.angle = 0.3; sun.rotation_euler = (0.9, 0.15, 0.6); sc.collection.objects.link(sun)
    sc.world.use_nodes = True; bg = next(n for n in sc.world.node_tree.nodes if n.type == 'BACKGROUND'); bg.inputs['Color'].default_value = (0.5, 0.52, 0.56, 1); bg.inputs['Strength'].default_value = 0.9
    shots('lit')
