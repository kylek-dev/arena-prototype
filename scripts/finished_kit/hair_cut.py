"""Cut the hair (or a beard) out of a fitted bust: the raw surface that stands clear of the kit head.
The piece's open edge is sunk under the kit skin, so no gap shows at the hairline, and the rest of it is
kept at least MIN_GAP above the skin, so the scalp never pokes through.
python hair_cut.py FIT.npz OUT.npz [--cut MM] [--zmin Z] [--smooth N] [--no-ears]
                   [--paint RAW_BASECOLOR.png [--paint-smooth N] [--beard [--no-nose]]]   (FIT.npz from hair_fit.py)
By default the piece is found by geometry alone (below). With --paint, where the generator painted hair or skin
decides instead, for pieces too thin to tell from the generated skin by thickness (a fade, a moustache): build
time only, the piece is still its own mesh and material. --beard limits the piece to the lower face.
-> OUT.npz: dense piece V, F, CUV (UVs per face corner, raw texture space), and which vertices are on its edge"""
import sys
import numpy as np
import igl
from scipy import sparse
from scipy.sparse.csgraph import connected_components
from kitlib import *
from mh import MH
import layout

a = sys.argv[1:]; fit_path, out = a[0], a[1]
opt = lambda k, d: float(a[a.index(k) + 1]) if k in a else d
CUT, FACE_CUT, CORE, GROW, ZMIN = (opt('--cut', 2.0) / 1e3, opt('--face-cut', 8.0) / 1e3, opt('--core', 6.0) / 1e3,
                                   opt('--grow', 2.0) / 1e3, opt('--zmin', 1.58))
# The generated face is not the kit face: below the hairline it stands up to 7.5 mm proud of the kit's cheeks,
# brow and jaw, and its head is wider behind the ears. Hair is thicker than that at its edge, so in front of the
# ears below the hairline and on the jaw below the ear tops a face must stand FACE_CUT clear to count as hair.
# Below the back hairline there is no hair at all: the generated neck is thicker than the kit's, so its skin
# under the nape stands as clear as hair does. The line's heights on the kit (by distance from the middle) were
# read off the swept-back concept, 1 mm under its painted hairline: flat across the nape, rising behind the ears.
# Tested on the closest kit point, which does not drift with the generated head. Heights are for this body.
face_zone = lambda C: ((C[:, 1] < -0.02) & (C[:, 2] < 1.745)) | ((C[:, 1] < 0.01) & (C[:, 2] < 1.68))
NAPE_X, NAPE_Z = [0.0, 0.02, 0.03, 0.04, 0.05, 0.06, 0.09], [1.636, 1.638, 1.645, 1.654, 1.681, 1.703, 1.703]
nape_line = lambda C: np.interp(np.abs(C[:, 0]), NAPE_X, NAPE_Z)
no_hair = lambda C: (C[:, 1] > -0.02) & (C[:, 2] < nape_line(C))
SINK, MIN_GAP, SMOOTH_EDGE = 0.0012, 0.0015, int(opt('--smooth', 8))

mh = MH(); L = layout.compute(mh); KV, KQ, region, RN = L['V'], L['Q'], L['region'], L['names']
HT = quads_to_tris(KQ[(region == RN.index('Head')) | (region == RN.index('Neck'))])
d = np.load(fit_path)
V, F, inv, ok = weld(d['V'], d['F'], 1e-7)                                  # glTF splits vertices at UV seams; weld for topology
CUV = d['UV'][d['F'][ok]]                                                   # and keep the UVs per face corner
sd = np.zeros(len(V)); sd[inv] = d['sd']

def face_components(Fs):
    """Connected components of faces that share an edge."""
    E = np.concatenate([Fs[:, [0, 1]], Fs[:, [1, 2]], Fs[:, [2, 0]]]); E.sort(1)
    fid = np.tile(np.arange(len(Fs)), 3)
    key = E[:, 0].astype(np.int64) * (len(V) + 1) + E[:, 1]; order = np.argsort(key); k = key[order]; f = fid[order]
    same = k[1:] == k[:-1]; A = sparse.coo_matrix((np.ones(same.sum()), (f[:-1][same], f[1:][same])), shape=(len(Fs), len(Fs)))
    return connected_components(A, directed=False)[1]

# The hair is a thick mass whose edge tapers off within a few millimetres; the generated skin around it is
# thin but spreads over centimetres. So: a core that stands CORE clear, grown at most GROW along the surface
# into faces that still stand CUT clear.
CK = igl.signed_distance(V, KV, HT, sign_type=igl.SIGNED_DISTANCE_TYPE_PSEUDONORMAL)[2]   # closest kit point
zone = face_zone(CK); above = (V[:, 2] > ZMIN) & ~no_hair(CK)
core_v = (sd > np.where(zone, FACE_CUT, CORE)) & above; thin_v = (sd > np.where(zone, FACE_CUT, CUT)) & above
Eg = np.concatenate([F[:, [0, 1]], F[:, [1, 2]], F[:, [2, 0]]]); Eg = Eg[thin_v[Eg].all(1)]
G = sparse.coo_matrix((np.linalg.norm(V[Eg[:, 0]] - V[Eg[:, 1]], axis=1), (Eg[:, 0], Eg[:, 1])), shape=(len(V), len(V))).tocsr()
from scipy.sparse.csgraph import dijkstra
reach = np.isfinite(dijkstra(G, directed=False, indices=np.where(core_v)[0], limit=GROW, min_only=True))
outside = (reach & thin_v)[F].all(1)
if '--paint' in a:
    # Hair and skin by their painted luminance, smoothed over neighbouring faces (a highlight on a lock is still
    # hair, and the edge follows the painted line rather than single triangles), split at the Otsu threshold of this bust. Lashes, brows and other dark specks are small parts and
    # are dropped below; for hair the brow and eye box is excluded, for a beard everything above the cheekbones
    # and behind the front of the ears.
    from PIL import Image
    tex = np.asarray(Image.open(a[a.index('--paint') + 1]).convert('RGB'), np.float64) / 255
    lum = (bilinear(tex, CUV.reshape(-1, 2)) @ np.array([0.2126, 0.7152, 0.0722])).reshape(-1, 3).mean(1)
    cnt = np.bincount(F.ravel(), minlength=len(V))
    for it in range(int(opt('--paint-smooth', 8))):
        vl = np.bincount(F.ravel(), np.repeat(lum, 3), minlength=len(V)) / np.maximum(cnt, 1); lum = vl[F].mean(1)
    region_f = (V[F][:, :, 2] > ZMIN).all(1)
    hist, edges = np.histogram(lum[region_f], 256, (0, 1)); w0 = np.cumsum(hist); w1 = w0[-1] - w0
    m0 = np.cumsum(hist * edges[:-1]) / np.maximum(w0, 1); m1 = (np.sum(hist * edges[:-1]) - np.cumsum(hist * edges[:-1])) / np.maximum(w1, 1)
    thr = edges[np.argmax(w0 * w1 * (m0 - m1) ** 2)]
    CF = CK[F].mean(1)
    if '--beard' in a:
        nose = (CF[:, 2] > 1.667) & (np.abs(CF[:, 0]) < 0.025) & (CF[:, 1] < -0.08) & ('--no-nose' not in a)   # the moustache stops under the nose
        allowed = (CF[:, 2] < 1.70) & (CF[:, 1] < -0.003) & ~nose
    else:
        allowed = ~((CF[:, 1] < -0.03) & (CF[:, 2] > 1.675) & (CF[:, 2] < 1.757) & (np.abs(CF[:, 0]) < 0.068))
    outside = (lum < thr) & region_f & allowed
    print('paint: hair below luminance %.3f; %.1f%% of the head painted as hair' % (thr, 100 * outside.sum() / region_f.sum()))
lab = face_components(F)                                                    # the raw mesh may have loose parts
lab_hair = -np.ones(len(F), int); idx = np.where(outside)[0]; lab_hair[idx] = face_components(F[idx])
sizes = np.bincount(lab_hair[idx])
piece = np.isin(lab_hair, np.where(sizes >= 0.05 * sizes.max())[0]) if '--paint' in a else lab_hair == np.argmax(sizes)
print('outside faces %d in %d parts; largest %d (%.1f%%)' % (len(idx), len(sizes), sizes.max(), 100 * sizes.max() / len(idx)))
# fill holes: parts of the rest of the head that the piece encloses (every part but the biggest)
rest = np.where(~piece & (V[F][:, :, 2] > ZMIN - 0.05).all(1))[0]; lr = face_components(F[rest]); rs = np.bincount(lr)
small = rs < 2000 if '--paint' in a else np.ones(len(rs), bool)            # with paint, a bare mouth inside a beard stays open
small[np.argmax(rs)] = False; holes = rest[small[lr]]; piece[holes] = True
print('filled %d holes (%d faces)' % (small.sum(), len(holes)))

# The generated ears are not where the kit's are, so hair that wrapped behind the generated ear runs through the
# kit's. Carve the hair EAR_CLEAR around the kit ears; the new edge sinks into the skull beside the ear (below).
def kit_ears():
    """Kit ear vertices: the flap a ray from the vertex in towards the head's axis leaves through the skull behind."""
    A_, B_, C_ = KV[HT[:, 0]], KV[HT[:, 1]], KV[HT[:, 2]]; e1, e2 = B_ - A_, C_ - A_
    def hit_t(o, dvec):                                                     # Moller-Trumbore against every triangle
        p_ = np.cross(dvec, e2); det = (e1 * p_).sum(1); ok_ = np.abs(det) > 1e-12; inv_ = np.where(ok_, 1 / np.where(ok_, det, 1), 0)
        s_ = o - A_; u = (s_ * p_).sum(1) * inv_; q_ = np.cross(s_, e1); v = (dvec * q_).sum(1) * inv_; t = (e2 * q_).sum(1) * inv_
        return t[ok_ & (u >= 0) & (v >= 0) & (u + v <= 1) & (t > 0)]
    head = np.unique(KQ[region == RN.index('Head')])
    box = head[(KV[head, 0] > 0.045) & (KV[head, 2] > 1.62) & (KV[head, 2] < 1.78) & (KV[head, 1] > -0.025) & (KV[head, 1] < 0.07)]
    ear = []
    for v in box:                                                           # left side; the kit is symmetric
        o = KV[v]; dvec = np.array([0.0, 0.01, o[2]]) - o; L0 = np.linalg.norm(dvec); dvec /= L0
        t = hit_t(o + dvec * 2e-4, dvec)
        if np.any((t > 0.0015) & (t < L0 - 0.01)): ear.append(v)
    body = np.where(mh.body)[0]; loc = -np.ones(len(mh.V), int); loc[body] = np.arange(len(body))
    ear = np.array(ear); ear = np.unique(np.r_[ear, loc[mh.mirror[body]][ear]])
    ring = KQ[np.isin(KQ, ear).any(1) & ((region == RN.index('Head')))]          # one ring more: the helix rim and the root
    return np.unique(ring)
EAR_CLEAR = 0.006
ears = kit_ears(); ear_face = np.isin(HT, ears).all(1); HT_skull = HT[~np.isin(HT, ears).any(1)]
near_ear = (np.sqrt(igl.point_mesh_squared_distance(V, KV, HT[ear_face])[0]) < EAR_CLEAR) & ('--no-ears' not in a)
carved = piece & near_ear[F].any(1); piece &= ~carved
print('ears: %d kit vertices; carved %d faces from around them' % (len(ears), carved.sum()))
lab_p = -np.ones(len(F), int); ip = np.where(piece)[0]; lab_p[ip] = face_components(F[ip])   # drop slivers the carve cut off
sz = np.bincount(lab_p[ip]); piece &= np.isin(lab_p, np.where(sz >= 0.01 * sz.max())[0])
Fp = F[piece]; used = np.unique(Fp); remap = -np.ones(len(V), int); remap[used] = np.arange(len(used))
P = V[used].copy(); T = remap[Fp]
E = np.concatenate([T[:, [0, 1]], T[:, [1, 2]], T[:, [2, 0]]]); Es = np.sort(E, 1)
u, c = np.unique(Es, axis=0, return_counts=True); B = np.unique(u[c == 1].ravel())
sdp, fi, C = igl.signed_distance(P, KV, HT, sign_type=igl.SIGNED_DISTANCE_TYPE_PSEUDONORMAL)[:3]
fn = np.cross(KV[HT[:, 1]] - KV[HT[:, 0]], KV[HT[:, 2]] - KV[HT[:, 0]]); fn /= np.linalg.norm(fn, axis=1, keepdims=True)
far = np.abs(sdp) > 0.003                                                   # which way the kit's faces point
flip = 1.0 if np.mean(np.einsum('ij,ij->i', (P - C)[far], fn[fi][far]) * np.sign(sdp[far]) > 0) >= 0.5 else -1.0
edge = np.zeros(len(P), bool); edge[B] = True
# the cut follows triangles, so its line is ragged: smooth the edge along itself before sinking it
be = u[c == 1]; nb = [[] for _ in range(len(P))]
for i, j in be: nb[i].append(j); nb[j].append(i)
two = np.array([v for v in B if len(nb[v]) == 2]); n1 = np.array([nb[v][0] for v in two]); n2 = np.array([nb[v][1] for v in two])
for it in range(SMOOTH_EDGE): P[two] = 0.5 * P[two] + 0.25 * (P[n1] + P[n2])
sdp, fi, C = igl.signed_distance(P, KV, HT, sign_type=igl.SIGNED_DISTANCE_TYPE_PSEUDONORMAL)[:3]
Nk = fn[fi] * flip                                                          # outward kit normal at the closest point
low = ~edge & (sdp < MIN_GAP); P[low] = C[low] + Nk[low] * MIN_GAP
fn_s = np.cross(KV[HT_skull[:, 1]] - KV[HT_skull[:, 0]], KV[HT_skull[:, 2]] - KV[HT_skull[:, 0]]); fn_s /= np.linalg.norm(fn_s, axis=1, keepdims=True)
_, fi_s, C_s = igl.point_mesh_squared_distance(P[edge], KV, HT_skull)       # the edge sinks into the skin, never into an ear
P[edge] = C_s - fn_s[fi_s] * flip * SINK
print('piece: %d verts, %d tris, %d edge verts; lifted %d verts to %.1f mm' % (len(P), len(T), edge.sum(), low.sum(), MIN_GAP * 1e3))
print('thickness above the skin mm: p5 %.1f p50 %.1f p95 %.1f max %.1f' % tuple(np.quantile(sdp[~edge], [0.05, 0.5, 0.95, 1.0]) * 1e3))
np.savez_compressed(out, V=P, F=T, CUV=CUV[piece], edge=edge)
print('wrote', out)
