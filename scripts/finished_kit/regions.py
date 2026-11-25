"""Body regions for gear masking, cut along real edge loops of the mesh wherever a clean loop exists.
-> work/regions.npz (region id per kept quad, names, boundary report) and QA renders via region_view.py"""
import collections, json
import numpy as np
from kitlib import *
from mh import MH
import skeleton

REGIONS = ['Head', 'Neck', 'Chest', 'Abdomen', 'Hips', 'Shoulders', 'UpperArms', 'Forearms', 'Hands', 'Thighs', 'Knees', 'Calves', 'Shins', 'Feet']
TRUNKS = ('Hips', 'Thighs')       # the permanent trunks piece covers these two regions
mh = MH(); body = np.where(mh.body)[0]; loc = -np.ones(len(mh.V), int); loc[body] = np.arange(len(body)); mirror = loc[mh.mirror[body]]
fit = np.load(KIT + 'work/fit_eyes.npz'); V, Qall, keep = fit['V'], fit['Q'], fit['face_keep']; Q = Qall[keep]
sk = skeleton.build(mh, np.load(KIT + 'work/pose.npz')['V']); P = sk['pos']
Wm = mh.weights()[body]; bn = mh.bone_names; wsum = lambda *names: Wm[:, [bn.index(n) for n in names]].sum(1)
unit = lambda v: v / np.linalg.norm(v)

nbr = collections.defaultdict(set); edge_faces = collections.defaultdict(set)
for fi, f in enumerate(Q):
    for k in range(4):
        a, b = f[k], f[(k + 1) % 4]; nbr[a].add(b); nbr[b].add(a); edge_faces[frozenset((a, b))].add(fi)

def trace_loop(a, b):
    def step(a, b):
        if len(nbr[b]) != 4: return None
        fa = edge_faces[frozenset((a, b))]
        for x in nbr[b]:
            if x != a and not (edge_faces[frozenset((b, x))] & fa): return x
    loop = [a, b]
    while len(loop) < 400:
        x = step(loop[-2], loop[-1])
        if x is None: return None
        if x == loop[0]: return loop
        loop.append(x)

def loop_near_plane(point, normal, radius, tol=0.05, side=0):
    """The closed edge loop that lies closest to the plane, among loops passing within `radius` of `point`."""
    normal = unit(normal); cand = np.where((np.linalg.norm(V - point, axis=1) < radius) & (np.abs((V - point) @ normal) < 0.03))[0]
    best = None; seen = set()
    for a in cand:
        for b in nbr[a]:
            e = frozenset((a, b))
            if e in seen or abs(unit(V[b] - V[a]) @ normal) > 0.35: continue
            loop = trace_loop(a, b)
            if loop is None: seen.add(e); continue
            for i in range(len(loop)): seen.add(frozenset((loop[i], loop[(i + 1) % len(loop)])))
            d = (V[loop] - point) @ normal; cen = V[loop].mean(0)
            if np.abs(d).max() > tol or np.linalg.norm(cen - point - normal * ((cen - point) @ normal)) > radius * 0.6: continue
            if side and d.mean() * side < 0: continue                        # the loop must lie on the asked side of the plane
            score = np.abs(d).mean() + 0.3 * d.std()
            if best is None or score < best[0]: best = (score, loop)
    return best

def split_by_loop(loop, side_point):
    """Faces on the side of the loop that contains side_point (flood fill that never crosses the loop's edges)."""
    cut = {frozenset((loop[i], loop[(i + 1) % len(loop)])) for i in range(len(loop))}
    fc = V[Q].mean(1); seed = int(np.argmin(np.linalg.norm(fc - side_point, axis=1))); seen = {seed}; stack = [seed]
    while stack:
        f = stack.pop()
        for k in range(4):
            e = frozenset((Q[f][k], Q[f][(k + 1) % 4]))
            if e in cut: continue
            for g in edge_faces[e]:
                if g not in seen: seen.add(g); stack.append(g)
    m = np.zeros(len(Q), bool); m[list(seen)] = True; return m

report = {}; loops = {}; fc = V[Q].mean(1)
def cut(name, point, normal, radius, far_point, tol=0.05, side=0):
    """Mask of faces beyond the cut (towards far_point). Uses an edge loop if one fits, else the plane."""
    hit = loop_near_plane(np.asarray(point, float), np.asarray(normal, float), radius, tol, side)
    if hit is not None:
        m = split_by_loop(hit[1], np.asarray(far_point, float))
        if 0.002 < m.mean() < 0.6:
            report[name] = dict(kind='edge loop', verts=len(hit[1]), height=round(float(V[hit[1], 2].mean()), 3)); loops[name] = np.array(hit[1]); return m
    report[name] = dict(kind='plane'); return (fc - point) @ unit(np.asarray(normal, float)) > 0

reg = np.full(len(Q), REGIONS.index('Chest'))
up = np.array([0, 0, 1.0])
# torso
below_waist = cut('waist', (0, 0.03, 1.087), -up, 0.25, (0.1, 0, 0.9), side=-1)     # at or above the old shirt hem, so the trunks cover all cloth paint
below_ribs = cut('ribs', (0, 0.03, 1.27), -up, 0.25, (0.1, 0, 0.9))
# the neck is cut from its anatomical joints (the scaffold's), not the rig's lower neck pivot
from rigdata import scaffold_joints
_sj, _ = scaffold_joints(mh, np.load(KIT + 'work/pose.npz')['V'])
neck_dir = unit(_sj['head'] - _sj['neck_01']); neck_base = _sj['neck_01'] - neck_dir * 0.035
above_neck = cut('neck base', neck_base, neck_dir, 0.11, _sj['head'] + up * 0.1)
head_dom = (wsum('head')[Q].mean(1) > 0.5)
reg[below_ribs] = REGIONS.index('Abdomen'); reg[below_waist] = REGIONS.index('Hips')
reg[above_neck] = REGIONS.index('Neck'); reg[above_neck & head_dom] = REGIONS.index('Head')
for sx in (1, -1):
    S = np.array([sx, 1, 1.0]); J = lambda n: P[n + ('_l' if sx > 0 else '_r')]
    # legs
    leg = unit(J('foot') - J('thigh'))
    top_ring = None                                              # Hips end at the highest closed ring around the leg
    for z in np.arange(0.96, 0.80, -0.02):
        t = (z - J('thigh')[2]) / leg[2]; hit = loop_near_plane(J('thigh') + leg * t, leg, 0.11, 0.05)
        if hit is not None and len(hit[1]) == 20:
            m = split_by_loop(hit[1], J('ball'))
            if 0.02 < m.mean() < 0.3: top_ring = (hit[1], m); break
    assert top_ring is not None, 'no closed ring at the top of the thigh'
    tag = 'L' if sx > 0 else 'R'
    report[f'thigh top {tag}'] = dict(kind='edge loop', verts=20, height=round(float(V[top_ring[0], 2].mean()), 3)); loops[f'thigh top {tag}'] = np.array(top_ring[0])
    reg[top_ring[1]] = REGIONS.index('Thighs')
    # the trunks' leg opening (at or below the old shorts hem) is also where Thighs end and Knees begin
    for name, z, region, side in (('trunks hem', 0.700, 'Knees', 1), ('below knee', 0.500, 'Calves', 0), ('mid calf', 0.330, 'Shins', 0), ('ankle', 0.150, 'Feet', 0)):
        t = (z - J('thigh')[2]) / leg[2]; pt = J('thigh') + leg * t
        m = cut(f'{name} {tag}', pt, leg, 0.11, J('ball'), side=side)
        reg[m] = REGIONS.index(region)
    # arms
    arm = unit(J('lowerarm') - J('upperarm')); fore = unit(J('hand') - J('lowerarm')); L = np.linalg.norm(J('lowerarm') - J('upperarm'))
    arm_side = wsum(*[b for b in bn if b.endswith('_l' if sx > 0 else '_r') and b.split('_')[0] in ('clavicle', 'upperarm', 'lowerarm', 'hand', 'index', 'middle', 'ring', 'pinky', 'thumb')])[Q].mean(1)
    armhole = None                                               # the ring around the arm closest to the torso
    for t in np.arange(-0.10, 0.36, 0.02):
        hit = loop_near_plane(J('upperarm') + arm * L * t, arm, 0.10, 0.06)
        if hit is not None and len(hit[1]) == 20:
            m = split_by_loop(hit[1], J('middle_03'))
            if 0.01 < m.mean() < 0.25: armhole = (hit[1], m); break
    assert armhole is not None, 'no closed ring at the top of the arm'
    report[f'armhole {tag}'] = dict(kind='edge loop', verts=20, height=round(float(V[armhole[0], 2].mean()), 3)); loops[f'armhole {tag}'] = np.array(armhole[0])
    reg[armhole[1]] = REGIONS.index('Shoulders')
    for name, pt, nrm, region in ((f'deltoid {tag}', J('upperarm') + arm * L * 0.60, arm, 'UpperArms'), (f'above elbow {tag}', J('upperarm') + arm * L * 0.88, arm, 'Forearms'), (f'wrist {tag}', J('hand') - fore * 0.01, fore, 'Hands')):
        m = cut(name, pt, nrm, 0.09, J('middle_03'))
        reg[m & (arm_side > 0.3)] = REGIONS.index(region)
# symmetry: copy the left half's assignment onto mirrored faces
fkey = {tuple(sorted(f)): i for i, f in enumerate(Q.tolist())}
left = fc[:, 0] > 0
for i in np.where(left)[0]:
    j = fkey.get(tuple(sorted(mirror[Q[i]].tolist())))
    if j is not None: reg[j] = reg[i]
# --- straight garment edges: the trunks' waist and leg openings follow mesh loops that wander a centimetre
# up and down. Each loop is flattened onto its own plane (staying on the sculpt), and the two rings of
# neighbours either side follow part of the way, so the skin-to-trunks line is straight, not wavy.
import igl
tgc = np.load(KIT + 'work/target_clean.npz'); TVc, TFc = tgc['V'], tgc['F']
def flatten(loop, normal):
    n = unit(np.asarray(normal, float)); c = V[loop].mean(0); P = V[loop].copy()
    for _ in range(6):
        P = P - np.outer((P - c) @ n, n); P = igl.point_mesh_squared_distance(P, TVc, TFc)[2]
    disp = dict(zip(loop.tolist(), P - V[loop])); V[loop] = P
    front = set(loop.tolist()); seen = set(front)
    for _ring in range(2):                                      # each ring of neighbours moves half as far as the one before
        nxt = sorted({u for v in front for u in nbr[v] if u not in seen})
        moved_ = np.array([np.mean([disp[v] for v in nbr[u] if v in disp], axis=0) * 0.5 for u in nxt])
        newp = np.atleast_2d(igl.point_mesh_squared_distance(V[nxt] + moved_, TVc, TFc)[2])
        disp = dict(zip(nxt, newp - V[nxt])); V[nxt] = newp; seen |= set(nxt); front = set(nxt)
    return float(np.abs((V[loop] - c) @ n).max())
before = float(np.ptp(V[loops['waist'], 2]))
off = flatten(loops['waist'], up)
V[loops['waist']] = (V[loops['waist']] + V[mirror[loops['waist']]] * np.array([-1, 1, 1.0])) / 2
print('waist loop: wandered %.1f mm in height, now within %.2f mm of level' % (before * 1e3, off * 1e3))
legL = unit(P['foot_l'] - P['thigh_l']); hem = loops['trunks hem L']
before = float(np.ptp((V[hem] - V[hem].mean(0)) @ legL)); off = flatten(hem, legL)
print('trunks hem: wandered %.1f mm along the leg, now within %.2f mm of its plane' % (before * 1e3, off * 1e3))
touched = np.where(np.linalg.norm(V - fit['V'], axis=1) > 0)[0]; left = touched[fit['V'][touched, 0] > 1e-6]
V[mirror[left]] = V[left] * np.array([-1, 1, 1.0])              # the right side mirrors the left
out_fit = dict(fit); out_fit['V'] = V; np.savez_compressed(KIT + 'work/fit_eyes.npz', **out_fit)
counts = {r: int((reg == i).sum()) * 2 for i, r in enumerate(REGIONS)}
print('triangles per region:', counts)
for k, v in report.items(): print('  %-16s %s' % (k, v))
np.savez_compressed(KIT + 'work/regions.npz', region=reg, names=np.array(REGIONS), **{'loop_' + k.replace(' ', '_'): v for k, v in loops.items()}); json.dump(report, open(KIT + 'work/regions_report.json', 'w'), indent=1)
