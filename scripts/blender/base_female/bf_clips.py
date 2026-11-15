"""The eight BaseFemale clips."""
import math
from mathutils import Vector as V
import bf_anim as A
import bf_rig as R

X, Y, Z = A.X, A.Y, A.Z
CURL_AX = R.D.cross(R.N).normalized()          # left-hand finger curl axis (+deg = toward palm)
_T = (R.D * 0.62 + R.F * 0.62 + R.N * 0.42).normalized()
THUMB_AX = _T.cross(R.N).normalized()
FINGERS = ("index", "middle", "ring", "pinky")


def ss(t):
    t = max(0.0, min(1.0, t))
    return t * t * (3 - 2 * t)


def keyed(keys, t):
    """keys: [(time, value)] with floats or Vectors; smoothstep between keys."""
    if t <= keys[0][0]:
        return keys[0][1]
    for (t0, a), (t1, b) in zip(keys, keys[1:]):
        if t <= t1:
            u = ss((t - t0) / (t1 - t0))
            if isinstance(a, V):
                return a.lerp(b, u)
            return a + (b - a) * u
    return keys[-1][1]


def mirror(v):
    return V((-v.x, v.y, v.z))


def hand(p, side, curl, thumb=None, spread=0.0):
    ax = CURL_AX if side > 0 else A.mirror_axis(CURL_AX)
    tax = THUMB_AX if side > 0 else A.mirror_axis(THUMB_AX)
    sfx = ".L" if side > 0 else ".R"
    for i, f in enumerate(FINGERS):
        c = curl * (1.0 + 0.12 * i)
        p.r(f"{f}_01{sfx}", (ax, c))
        p.r(f"{f}_02{sfx}", (ax, c * 1.15))
    th = curl * 0.6 if thumb is None else thumb
    p.r(f"thumb_02{sfx}", (tax, th * 0.6))
    p.r(f"thumb_03{sfx}", (tax, th))


def hands(p, curl, thumb=None):
    hand(p, 1, curl, thumb)
    hand(p, -1, curl, thumb)


def legs_stance(p, bend=4.0, spread=3.0):
    p.rs("thigh", (Y, -spread), (X, -bend))
    p.rs("shin", (X, bend * 2))
    p.rs("foot", (X, -bend), (Y, spread))


# ---------------------------------------------------------------- Idle
def idle(t, p):
    ph = 2 * math.pi * t
    br = math.sin(ph * 2)          # two breaths per 2 s loop
    sh = math.sin(ph)              # one weight shift per loop
    p.move("pelvis", (0.012 * sh, 0, -0.008 + 0.002 * br))
    p.r("pelvis", (Y, 2.0 * sh))
    p.r("spine_01", (Y, -1.2 * sh), (X, 0.5))
    p.r("spine_02", (X, -1.2 * br), (Y, -0.6 * sh))
    p.r("spine_03", (X, -1.0 * br))
    p.r("neck", (X, 0.6 * br))
    p.r("head", (Y, 1.2 * math.sin(ph + 0.6)), (Z, 2.0 * math.sin(ph * 0.5 * 2 + 1.0)), (X, 1.0))
    p.rs("clavicle", (Y, -1.0 * br))
    # legs compensate the weight shift so the feet stay planted
    for s in (1, -1):
        p.side("thigh", s, (Y, -3.0 - 2.0 * sh * s), (X, -4))
        p.side("shin", s, (X, 9 + 3 * max(0.0, -sh * s)))
        p.side("foot", s, (X, -5), (Y, 3.0))
    p.rs("upper_arm", (Y, 33 + 1.5 * br), (X, -5))
    p.rs("forearm", (X, -16 - 2 * br))
    p.rs("hand", (X, -4))
    hands(p, 16 + 3 * br, 10)


# ---------------------------------------------------------------- Run
def run(t, p, direction=1):
    ph = 2 * math.pi * t * direction
    s, c = math.sin(ph), math.cos(ph)
    lean = 9.0 if direction > 0 else -3.0
    tilt = 4.0 if direction > 0 else 0.0
    p.move("pelvis", (0, 0, -0.022 + 0.022 * math.cos(2 * ph)))
    p.r("pelvis", (X, tilt), (Z, -9 * s), (Y, 3 * c))
    p.r("spine_01", (X, lean * 0.35), (Z, 4 * s))
    p.r("spine_02", (X, lean * 0.35), (Z, 4 * s))
    p.r("spine_03", (X, lean * 0.3 + 1.5 * math.cos(2 * ph)), (Z, 2 * s))
    p.r("neck", (X, -lean * 0.4))
    p.r("head", (X, -lean * 0.3 - 1.5 * math.cos(2 * ph)))
    for side, phase in ((1, 0.0), (-1, math.pi)):
        q = ph + phase
        ls, lc = math.sin(q), math.cos(q)
        if direction < 0:
            lc = -lc
        thigh = -(12 + 32 * ls) - tilt
        swing = max(0.0, math.cos(q + 0.45 * direction)) if direction > 0 else max(0.0, -math.cos(q - 0.45))
        knee = 22 + 78 * swing ** 1.3
        st = ss(-lc * 2)  # stance factor
        ankle = -(thigh + tilt + knee) * (0.45 + 0.55 * st) + 14 * (1 - st)
        p.side("thigh", side, (Y, -3), (X, thigh))
        p.side("shin", side, (X, knee))
        p.side("foot", side, (X, ankle))
        p.side("toe", side, (X, -12 * st * max(0.0, ls * -direction)))
        arm = 30 * ls * (1 if direction > 0 else 0.6)
        p.side("upper_arm", side, (Y, 52), (X, arm - 6))
        p.side("forearm", side, (X, -75 - 15 * max(0.0, -ls)))
    hands(p, 45, 30)


def move_forward(t, p):
    run(t, p, 1)


def move_backward(t, p):
    run(t, p, -1)


# ---------------------------------------------------------------- Strafe
def strafe(t, p, toward):
    """toward=+1: character's left (+X); -1: right. Side-step without crossing."""
    ph = 2 * math.pi * t
    open_ = 0.5 - 0.5 * math.cos(ph)          # 0 together (hip width) -> 1 wide
    abd = 3 + 15 * open_
    lead = max(0.0, math.sin(ph))             # leading leg airborne while opening
    trail = max(0.0, -math.sin(ph))           # trailing leg airborne while closing
    p.move("pelvis", (0, 0, -0.018 - 0.022 * open_ + 0.008 * math.cos(2 * ph)))
    p.r("pelvis", (Y, 3 * toward * (lead - trail)), (X, 3))
    p.r("spine_01", (Y, 2.5 * toward), (X, 2))
    p.r("spine_02", (Y, 1.5 * toward), (X, 2))
    p.r("spine_03", (Y, -1.0 * toward))
    p.r("head", (Y, -3 * toward), (X, -2))
    for side in (1, -1):
        air = lead if side == toward else trail
        p.side("thigh", side, (Y, -abd), (X, -10 - 26 * air))
        p.side("shin", side, (X, 20 + 48 * air))
        p.side("foot", side, (Y, abd), (X, -10 - 22 * air + 8 * air))
        p.side("upper_arm", side, (Y, 44 - 6 * open_), (X, -14))
        p.side("forearm", side, (X, -62 + 8 * math.sin(ph + (0 if side == toward else math.pi))))
    hands(p, 40, 25)


def strafe_left(t, p):
    strafe(t, p, 1)


def strafe_right(t, p):
    strafe(t, p, -1)


# ---------------------------------------------------------------- Jump
JUMP_FRAMES = 33
JUMP_H = 0.45


def jump(t, p):
    f = t * JUMP_FRAMES
    crouch = keyed([(0, 0.0), (6, 1.0), (9, 0.0), (24, 0.0), (26, 0.85), (33, 0.0)], f)
    ext = keyed([(0, 0.0), (6, 0.0), (9, 1.0), (13, 0.3), (17, 0.0)], f)
    tuck = keyed([(0, 0.0), (11, 0.0), (16, 1.0), (21, 0.0)], f)
    reach = keyed([(0, 0.0), (21, 0.0), (24, 0.6), (26, 0.0)], f)
    z = 0.0
    if 8 < f < 25:
        z = 4 * JUMP_H * (f - 8) * (25 - f) / (17.0 ** 2)
    p.move("root", (0, 0, z))
    tilt = 14 * crouch + 6 * tuck
    p.move("pelvis", (0, 0.02 * crouch, -0.15 * crouch + 0.03 * ext - 0.04 * tuck))
    p.r("pelvis", (X, tilt))
    p.r("spine_01", (X, 6 * crouch + 4 * tuck - 4 * ext))
    p.r("spine_02", (X, 6 * crouch + 6 * tuck - 3 * ext))
    p.r("spine_03", (X, 2 * crouch - 2 * ext))
    p.r("head", (X, -10 * crouch - 6 * tuck + 4 * ext))
    thigh = -62 * crouch - 75 * tuck - 18 * reach - tilt + 4 * ext
    knee = 105 * crouch + 110 * tuck + 30 * reach
    ankle = -(thigh + tilt + knee) * (1 - ext) + 30 * ext + 10 * tuck
    for side in (1, -1):
        p.side("thigh", side, (Y, -5), (X, thigh))
        p.side("shin", side, (X, knee))
        p.side("foot", side, (Y, 5), (X, ankle))
        up = ext * 0.8 + tuck * 0.3
        back = crouch
        # arms: back on crouch, up on takeoff, out for balance in the air
        d = V((0.28, 0.0, -0.96)) * (1 - up - back) + V((0.25, 0.75, -0.6)).normalized() * back \
            + V((0.35, -0.55, 0.75)).normalized() * up + V((0.7, -0.2, 0.1)) * tuck * 0.6
        if d.length < 1e-4:
            d = V((0.3, 0, -0.95))
        d = d.normalized()
        p.point("upper_arm" + (".L" if side > 0 else ".R"), d if side > 0 else mirror(d))
        p.side("forearm", side, (X, -25 - 30 * tuck))
    hands(p, 20 + 30 * crouch, 15)


# ---------------------------------------------------------------- Attack
ATTACK_FRAMES = 27


def attack(t, p):
    f = t * ATTACK_FRAMES
    ks = [0, 9, 11, 13, 15, 19, 27]
    # right arm world directions (upper arm, forearm)
    up = [V((-0.42, -0.05, -0.90)), V((-0.70, 0.35, 0.62)), V((-0.65, -0.35, 0.65)), V((-0.45, -0.85, 0.20)),
          V((-0.05, -0.97, -0.05)), V((0.40, -0.80, -0.40)), V((-0.42, -0.05, -0.90))]
    fo = [V((-0.15, -0.70, -0.70)), V((0.15, 0.55, 0.82)), V((-0.45, -0.25, 0.85)), V((-0.40, -0.90, 0.15)),
          V((0.30, -0.95, -0.10)), V((0.65, -0.55, -0.50)), V((-0.15, -0.70, -0.70))]
    twist = [0, -28, -18, 2, 20, 30, 0]
    lunge = [0, 0.2, 0.5, 0.9, 1.0, 1.0, 0]
    u = keyed(list(zip(ks, [v.normalized() for v in up])), f).normalized()
    fa = keyed(list(zip(ks, [v.normalized() for v in fo])), f).normalized()
    tw = keyed(list(zip(ks, twist)), f)
    lg = keyed(list(zip(ks, lunge)), f)
    p.move("pelvis", (0, -0.03 * lg, -0.02 - 0.05 * lg))
    p.r("pelvis", (Z, tw * 0.35), (X, 4 * lg))
    p.r("spine_01", (Z, tw * 0.25), (X, 3 * lg))
    p.r("spine_02", (Z, tw * 0.25), (X, 4 * lg))
    p.r("spine_03", (Z, tw * 0.15))
    p.r("head", (Z, -tw * 0.6), (X, -6 * lg))
    # left leg steps forward, right leg braces back
    p.side("thigh", 1, (Y, -4), (X, -10 - 30 * lg))
    p.side("shin", 1, (X, 12 + 30 * lg))
    p.side("foot", 1, (X, -2 + 0 * lg))
    p.side("thigh", -1, (Y, -4), (X, -4 + 16 * lg))
    p.side("shin", -1, (X, 10 + 16 * lg))
    p.side("foot", -1, (X, -10 - 10 * lg))
    p.point("upper_arm.R", u)
    p.point("forearm.R", fa)
    p.r("hand.R", (X, -10))
    hand(p, -1, 72, 40)
    # left arm counter-balances
    lu = keyed([(0, V((0.42, -0.05, -0.9))), (9, V((0.55, -0.60, -0.10))), (14, V((0.50, 0.30, -0.70))),
                (27, V((0.42, -0.05, -0.9)))], f).normalized()
    p.point("upper_arm.L", lu)
    p.r("forearm.L", (X, -45))
    hand(p, 1, 30, 20)


# ---------------------------------------------------------------- Cast
CAST_FRAMES = 36


def cast(t, p):
    f = t * CAST_FRAMES
    gather = keyed([(0, 0.0), (12, 1.0), (17, 1.0), (21, 0.0)], f)
    release = keyed([(0, 0.0), (17, 0.0), (21, 1.0), (26, 1.0), (36, 0.0)], f)
    pulse = math.sin(f * 1.6) * 0.5 * gather * ss((f - 11) / 3)
    rest_u, rest_f = V((0.42, -0.05, -0.9)), V((0.15, -0.70, -0.70))
    g_u, g_f = V((0.55, -0.45, -0.70)), V((-0.80, -0.50, 0.30))
    r_u, r_f = V((0.16, -0.97, 0.12)), V((0.04, -1.0, 0.06))
    w0 = max(0.0, 1 - gather - release)
    u = (rest_u.normalized() * w0 + g_u.normalized() * gather + r_u.normalized() * release).normalized()
    fa = (rest_f.normalized() * w0 + g_f.normalized() * gather + r_f.normalized() * release).normalized()
    p.move("pelvis", (0, 0.02 * gather - 0.04 * release, -0.03 * gather - 0.04 * release))
    p.r("pelvis", (X, -3 * gather + 6 * release))
    p.r("spine_01", (X, -3 * gather + 4 * release))
    p.r("spine_02", (X, -4 * gather + 5 * release + pulse))
    p.r("spine_03", (X, -2 * gather + 3 * release))
    p.r("head", (X, 8 * gather - 8 * release))
    p.side("thigh", 1, (Y, -4), (X, -6 - 26 * release + 3 * gather))
    p.side("shin", 1, (X, 10 + 22 * release + 10 * gather))
    p.side("foot", 1, (X, -4 + 4 * release - 8 * gather))
    p.side("thigh", -1, (Y, -4), (X, -6 + 16 * release - 3 * gather))
    p.side("shin", -1, (X, 10 + 12 * release + 12 * gather))
    p.side("foot", -1, (X, -4 - 22 * release - 8 * gather))
    for side in (1, -1):
        m = (lambda v: v) if side > 0 else mirror
        sfx = ".L" if side > 0 else ".R"
        p.point("upper_arm" + sfx, m(u))
        p.point("forearm" + sfx, m(fa))
        # palms push forward on release (wrist bends back)
        p.side("hand", side, (X, 25 * release))
        hand(p, side, 18 + 22 * gather - 18 * release, 10)


CLIPS = [
    ("Idle", 60, idle, True),
    ("Move_Forward", 30, move_forward, True),
    ("Move_Backward", 30, move_backward, True),
    ("Strafe_Left", 30, strafe_left, True),
    ("Strafe_Right", 30, strafe_right, True),
    ("Jump", JUMP_FRAMES, jump, False),
    ("Attack", ATTACK_FRAMES, attack, False),
    ("Cast", CAST_FRAMES, cast, False),
]


def build_all(rig):
    out = []
    for name, frames, fn, loop in CLIPS:
        out.append(A.make_action(rig, name, frames, fn, loop))
    A.rest(rig)
    return out
