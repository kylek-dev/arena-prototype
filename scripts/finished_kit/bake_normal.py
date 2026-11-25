"""Bake a tangent-space normal map (MikkTSpace, OpenGL +Y) from the cleaned dense sculpt onto the fitted mesh.
Texels the rays miss stay neutral.   blender -b --factory-startup -P bake_normal.py -- RES OUT.png"""
import bpy, os, sys, numpy as np
a = sys.argv[sys.argv.index('--') + 1:]; R = int(a[0]); out = a[1]
KIT = os.environ.get('PITBORN_ART', os.path.expanduser('~/pitborn-art')) + '/human-male-kit/'
bpy.ops.wm.read_factory_settings(use_empty=True); sc = bpy.context.scene
fit = np.load(KIT + 'work/fit_eyes.npz'); base = np.load(KIT + 'work/mh_base.npz'); tg = np.load(KIT + 'work/target_clean.npz')
V, Q, keep = fit['V'], fit['Q'], fit['face_keep']
names = [str(x) for x in base['group_names']]; bm = np.zeros(len(base['V']), bool); bm[base['g_vert'][base['g_group'] == names.index('body')]] = True
is_body = bm[base['loop_vert'].reshape(-1, 4)].all(1); uvq = base['loop_uv'].reshape(-1, 4, 2)[is_body][keep]
Qk = Q[keep]; used = np.unique(Qk); remap = -np.ones(len(V), int); remap[used] = np.arange(len(used))
def mk(name, V, F):
    me = bpy.data.meshes.new(name); me.from_pydata(V.tolist(), [], F.tolist()); me.update()
    for p in me.polygons: p.use_smooth = True
    ob = bpy.data.objects.new(name, me); sc.collection.objects.link(ob); return ob
low = mk('low', V[used], remap[Qk]); low.data.uv_layers.new(name='UVMap').data.foreach_set('uv', uvq.reshape(-1).astype(np.float32))
hi = mk('high', tg['V'], tg['F'])
img = bpy.data.images.new('normal', R, R, alpha=False, float_buffer=False); img.colorspace_settings.name = 'Non-Color'
img.generated_color = (0.5, 0.5, 1.0, 1.0); px = np.tile(np.array([0.5, 0.5, 1.0, 1.0], np.float32), R * R); img.pixels.foreach_set(px)
mat = bpy.data.materials.new('bake'); nt = mat.node_tree; t = nt.nodes.new('ShaderNodeTexImage'); t.image = img; nt.nodes.active = t; low.data.materials.append(mat)
hi.data.materials.append(bpy.data.materials.new('hi'))
sc.render.engine = 'CYCLES'; sc.cycles.device = 'CPU'; sc.cycles.samples = 1
bk = sc.render.bake; bk.use_selected_to_active = True; bk.cage_extrusion = 0.004; bk.max_ray_distance = 0.009; bk.margin = 16; bk.use_clear = False
bk.normal_space = 'TANGENT'; bk.normal_r, bk.normal_g, bk.normal_b = 'POS_X', 'POS_Y', 'POS_Z'
for o in sc.objects: o.select_set(False)
hi.select_set(True); low.select_set(True); bpy.context.view_layer.objects.active = low
with bpy.context.temp_override(object=low, active_object=low, selected_objects=[hi, low], selected_editable_objects=[hi, low]):
    print('bake', bpy.ops.object.bake(type='NORMAL'))
img.filepath_raw = out; img.file_format = 'PNG'; img.save()
print('saved', out)
