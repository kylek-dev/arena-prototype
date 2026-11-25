"""Game mesh for a hair or beard piece: the dense cut (hair_cut.py) reduced to a triangle budget, new UVs with
wide gutters, and a tangent-space normal map (MikkTSpace, OpenGL +Y) baked from the dense piece.
blender -b --factory-startup -P hair_low.py -- DENSE.npz OUT.npz NORMAL.png [--tris N] [--res R] [--centre x,y,z | --by-normal] [--no-split]
-> OUT.npz: V, F (triangles), UV per corner (glTF v down)"""
import bpy, bmesh, sys, math, numpy as np
a = sys.argv[sys.argv.index('--') + 1:]; src, out, nrm_out = a[:3]
opt = lambda k, d: type(d)(a[a.index(k) + 1]) if k in a else d
TRIS, R, GUTTER_PX = opt('--tris', 7600), opt('--res', 1024), 12
bpy.ops.wm.read_factory_settings(use_empty=True); sc = bpy.context.scene
d = np.load(src)
def mk(name, V, F):
    me = bpy.data.meshes.new(name); me.from_pydata(V.tolist(), [], F.tolist()); me.update()
    for p in me.polygons: p.use_smooth = True
    ob = bpy.data.objects.new(name, me); sc.collection.objects.link(ob); return ob
hi = mk('high', d['V'], d['F'])
low = mk('low', d['V'], d['F'])
def activate(ob):
    for o in sc.objects: o.select_set(False)
    ob.select_set(True); bpy.context.view_layer.objects.active = ob
activate(low)
dec = low.modifiers.new('dec', 'DECIMATE'); dec.decimate_type = 'COLLAPSE'; dec.ratio = TRIS / len(d['F']); dec.use_collapse_triangulate = True
bpy.ops.object.modifier_apply(modifier=dec.name)
bm = bmesh.new(); bm.from_mesh(low.data)
bmesh.ops.dissolve_degenerate(bm, edges=bm.edges, dist=1e-6); bmesh.ops.triangulate(bm, faces=bm.faces)
bm.to_mesh(low.data); bm.free()
for p in low.data.polygons: p.use_smooth = True
print('low: %d triangles' % len(low.data.polygons))
# UVs: a few large charts by direction from the head's centre (the locks' grooves are too noisy for angle-based
# islands), each unwrapped with low stretch, packed with gutters so mipmaps do not bleed islands together
CENTRE = np.array([float(x) for x in opt('--centre', '0,0,1.70').split(',')])     # a beard: about 0,-0.03,1.66
DIRS = np.array([[0, 0, 1], [0, -1, 0], [0, 1, 0], [1, 0, 0], [-1, 0, 0], [0, 0, -1.0]])
bm = bmesh.new(); bm.from_mesh(low.data); bm.faces.ensure_lookup_table()
if '--by-normal' in a:                                                      # a long, thin prop: chart by facing
    fc = np.array([f.normal for f in bm.faces])
else:
    fc = np.array([f.calc_center_median() for f in bm.faces]) - CENTRE; fc /= np.linalg.norm(fc, axis=1, keepdims=True)
lab = np.argmax(fc @ DIRS.T * np.array([1.25, 1, 1, 1, 1, 1]), 1)          # the top chart reaches a little further down
nbr = [[e.link_faces[0].index if e.link_faces[1] == f else e.link_faces[1].index for e in f.edges if len(e.link_faces) == 2] for f in bm.faces]
for it in range(6):                                                         # majority vote: no ragged chart borders or specks
    new = lab.copy()
    for i, nb in enumerate(nbr):
        if nb:
            cnt = np.bincount(lab[nb], minlength=len(DIRS)); cnt[lab[i]] += 1
            new[i] = np.argmax(cnt)
    lab = new
# A chart must be a disc to unwrap: a tube (a ponytail) or a ring collapses onto itself and smears the paint.
# Split each part that is not a disc across its main axis until every part is one.
fcent = np.array([f.calc_center_median() for f in bm.faces]); fverts = [[v.index for v in f.verts] for f in bm.faces]
def parts(lab):
    seen = -np.ones(len(lab), int); out = []
    for i in range(len(lab)):
        if seen[i] >= 0: continue
        stack, comp = [i], []; seen[i] = len(out)
        while stack:
            f = stack.pop(); comp.append(f)
            for g in nbr[f]:
                if seen[g] < 0 and lab[g] == lab[i]: seen[g] = len(out); stack.append(g)
        out.append(np.array(comp))
    return out
def euler(comp):
    vs = {v for f in comp for v in fverts[f]}; es = {tuple(sorted((fverts[f][k], fverts[f][(k + 1) % 3]))) for f in comp for k in range(3)}
    return len(vs) - len(es) + len(comp)
for it in range(0 if '--no-split' in a else 12):
    bad = [c for c in parts(lab) if len(c) > 3 and euler(c) != 1]
    if not bad: break
    nxt = lab.max() + 1
    for c in bad:
        X = fcent[c] - fcent[c].mean(0); axis = np.linalg.svd(X, full_matrices=False)[2][1]   # across the main axis
        lab[c[X @ axis > 0]] = nxt; nxt += 1
print('charts after splitting non-discs: %d parts, %d still not discs' % (len(parts(lab)), sum(euler(c) != 1 for c in parts(lab) if len(c) > 3)))
for e in bm.edges: e.seam = bool(len(e.link_faces) == 2 and lab[e.link_faces[0].index] != lab[e.link_faces[1].index])
bm.to_mesh(low.data); bm.free()
print('charts:', np.bincount(lab, minlength=len(DIRS)).tolist())
bpy.ops.object.mode_set(mode='EDIT'); bpy.ops.mesh.select_all(action='SELECT')
bpy.ops.uv.unwrap(method='ANGLE_BASED', margin=0.0)
bpy.ops.uv.select_all(action='SELECT')
bpy.ops.uv.pack_islands(udim_source='CLOSEST_UDIM', rotate=True, margin_method='FRACTION', margin=2 * GUTTER_PX / R)
bpy.ops.object.mode_set(mode='OBJECT')
print('uv islands packed, gutter %d px at %d' % (GUTTER_PX, R))
# normal bake: dense -> low
img = bpy.data.images.new('normal', R, R, alpha=False, float_buffer=False); img.colorspace_settings.name = 'Non-Color'
img.pixels.foreach_set(np.tile(np.array([0.5, 0.5, 1.0, 1.0], np.float32), R * R))
mat = bpy.data.materials.new('bake'); nt = mat.node_tree; t = nt.nodes.new('ShaderNodeTexImage'); t.image = img; nt.nodes.active = t; low.data.materials.append(mat)
hi.data.materials.append(bpy.data.materials.new('hi'))
sc.render.engine = 'CYCLES'; sc.cycles.device = 'CPU'; sc.cycles.samples = 1
bk = sc.render.bake; bk.use_selected_to_active = True; bk.cage_extrusion = 0.003; bk.max_ray_distance = 0.008; bk.margin = 16; bk.use_clear = False
bk.normal_space = 'TANGENT'; bk.normal_r, bk.normal_g, bk.normal_b = 'POS_X', 'POS_Y', 'POS_Z'
for o in sc.objects: o.select_set(False)
hi.select_set(True); low.select_set(True); bpy.context.view_layer.objects.active = low
with bpy.context.temp_override(object=low, active_object=low, selected_objects=[hi, low], selected_editable_objects=[hi, low]):
    print('bake', bpy.ops.object.bake(type='NORMAL'))
img.filepath_raw = nrm_out; img.file_format = 'PNG'; img.save()
me = low.data; n = len(me.vertices)
V = np.zeros(n * 3); me.vertices.foreach_get('co', V); V = V.reshape(-1, 3)
F = np.zeros(len(me.polygons) * 3, int); me.polygons.foreach_get('vertices', F); F = F.reshape(-1, 3)
uv = np.zeros(len(me.loops) * 2); me.uv_layers.active.data.foreach_get('uv', uv); uv = uv.reshape(-1, 3, 2); uv[..., 1] = 1 - uv[..., 1]
np.savez_compressed(out, V=V, F=F, UV=uv)
print('wrote', out, nrm_out)
