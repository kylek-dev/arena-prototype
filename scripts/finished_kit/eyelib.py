"""Find the concentric eyelid edge loops of the scaffold around one eye (left), innermost-socket to outer skin."""
import collections
import numpy as np


def eye_loops(V, Q, c_eye, reach=0.03):
    nxt = {}                                             # directed edge -> next edge along its edge loop
    v_faces = collections.defaultdict(list)
    for fi, f in enumerate(Q):
        for k in range(4): v_faces[f[k]].append(fi)
    edge_faces = collections.defaultdict(set)
    for fi, f in enumerate(Q):
        for k in range(4): edge_faces[frozenset((f[k], f[(k + 1) % 4]))].add(fi)
    nbr = collections.defaultdict(set)
    for f in Q:
        for k in range(4): nbr[f[k]].add(f[(k + 1) % 4]); nbr[f[(k + 1) % 4]].add(f[k])

    def step(a, b):                                      # continue a->b through b: the edge at b sharing no face with (a,b)
        if len(nbr[b]) != 4: return None
        fa = edge_faces[frozenset((a, b))]
        for x in nbr[b]:
            if x != a and not (edge_faces[frozenset((b, x))] & fa): return x
        return None

    near = np.where(np.linalg.norm(V - c_eye, axis=1) < reach)[0]; near_set = set(near.tolist())
    seen = set(); loops = []
    for a in near:
        for b in nbr[a]:
            if b not in near_set or frozenset((a, b)) in seen: continue
            loop = [a, b]; ok = True
            while True:
                x = step(loop[-2], loop[-1])
                if x is None or len(loop) > 200: ok = False; break
                if x == loop[0]: break
                loop.append(x)
            for i in range(len(loop)): seen.add(frozenset((loop[i], loop[(i + 1) % len(loop)])))
            if not ok or len(loop) < 8: continue
            P = V[loop] - c_eye; ang = np.unwrap(np.arctan2(P[:, 2], P[:, 0]))
            turns = (ang[-1] - ang[0] + np.arctan2(np.sin(np.arctan2(P[0, 2], P[0, 0]) - np.arctan2(P[-1, 2], P[-1, 0])), np.cos(np.arctan2(P[0, 2], P[0, 0]) - np.arctan2(P[-1, 2], P[-1, 0])))) / (2 * np.pi)
            if abs(abs(turns) - 1) < 0.2: loops.append(loop)
    # order by adjacency: neighbouring loops share quads
    return loops
