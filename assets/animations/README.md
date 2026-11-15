# Universal strafe animation library

`universal-strafes.glb` is a **standalone reusable animation source**, not a
female-character or outfit export. It contains:

- `Strafe_Left` and `Strafe_Right`: in-place lateral **run** loops of 28 frames
  (0.9333 s) at 30 authored samples/second, with an exact closing pose. Length
  and footfall frames match UAL1 `Jog_Fwd_Loop` (left foot planted on frames
  1–5, right on 15–19).
- The shared **65-joint Quaternius humanoid skeleton** and inverse bind matrices.
- One three-vertex weighted carrier triangle so engines retain a standard skin.
- **No character body, clothing, hair, textures, race asset or studio objects.**

File size: **109,976 bytes**. SHA-256:
`b31bddd4e444e1ee32dfd2fa01f66fb2f281b370200b2997cf0190c7fcccd4ca`.

## Reuse and licensing

Keep this README and [LICENSE.txt](LICENSE.txt) with the GLB when sharing it.
New motion curves and carrier geometry are **MIT**, matching this repository's
new original authoring code. The reference skeleton, baseline idle pose and the
arm/hand rotations sampled from `Jog_Fwd_Loop` are by **Quaternius**, from the
**CC0 1.0 Universal Animation Library 1 Standard**:

- Source: <https://quaternius.com/packs/universalanimationlibrary.html>
- CC0: <https://creativecommons.org/publicdomain/zero/1.0/>
- Original included notice: `assets/quaternius/animation v1/License.txt`.

No QAL race model/texture data is included, so sharing this standalone library
does not redistribute that restricted pack. Existing source licenses are not
changed. The production build includes the new motion's MIT notice at
`assets/licenses/universal-strafes-MIT.txt`.

The names and hierarchy match the supplied universal human bases, fantasy races
and fantasy outfits. Retarget by joint name/rest transforms and preserve target
bone lengths; do not assume matching node indices or one fixed body size.
Outfits should follow their character's existing shared skeleton, not run a
separate animation player. This is not automatically compatible with arbitrary
non-Quaternius skeletons: those need an explicit bone/rest-pose mapping.
Clothing fit, body-shape morphs and race selection are separate features.

The project adapter exposes this directly:

```python
import quaternius_character as q
library, buffers = q.read_asset(q.STRAFE_LIBRARY)
clips = tuple((name, name, False) for name in ("Strafe_Left", "Strafe_Right"))
tracks = dict(q.retarget(target_document, library, buffers, clips=clips))
```

`scripts/check_animation_compatibility.py` validates both loops against installed
human bases, complete outfits and races without copying them into the library.
All **ten** supplied targets passed named-hierarchy, loop closure, zero root
travel and foot-separation checks. Proportion-only retargeting left contact
height deviations within about **2.3 cm** across those targets. This is not
universal foot-locking IK or a visual approval of every model.

## Authoring

**Current version (2026-11-14, lateral run).** The first library was a slow
walking side-step (0.8 s loop, ~0.19 m steps) whose planted feet moved at about
0.5 m/s, far below the game's 7.2 m/s, so the body slid past small steps. It was
replaced the same day, at the user's request to match the strafes to the forward
jog and keep animation consistent with character speed, by
`scripts/author_universal_strafes.py` (Python standard library; no Blender,
DCC, renderer or runtime IK). Authored with **Claude Opus 5.5** (Anthropic)
assistance in Claude Code; Blender MCP was not connected, and the poses were
checked numerically and with stick-figure renders, not in Blender.

The script reuses this library's existing skeleton, inverse bind matrices and
carrier triangle (a Blender export of the CC0 UAL1 mannequin) and replaces only
the two animations, so reruns are byte-identical. It reads the CC0 UAL1
`Idle_Loop` (spine/neck/head baseline) and `Jog_Fwd_Loop` (clavicle/arm/hand/
finger rotations at the matching footfall frame, shoulders 70% of the jog
pump). Legs use analytic two-bone placement with knees forward; feet keep their
flat bind orientation. Each planted foot sweeps 0.45 m across the body in 14.5%
of the cycle (3.32 m/s at clip speed), matching the jog's short contact; swing
feet lift 0.24 m with a 0.14 m heel recovery. The pelvis bobs 0.045 m around a
0.035 m crouch, with a 4° lean into travel and 7° forward lean; the neck
partly levels the head. Feet never cross (ankles stay over 0.15 m apart).
Root translation/orientation stay fixed.

The original walking version is in Git history (`scripts/blender/universal_strafes.py`
at commit `09df400`); that script no longer reproduces this file and was removed.

Recreate from the project root:

```sh
python3 scripts/author_universal_strafes.py
python3 scripts/check_animation_compatibility.py
./scripts/build-web.sh
```

Source downloads are not modified. No model generation service, motion capture,
proprietary game animation or runtime AI was used. AI assistance does not
establish exclusive rights.

## Game integration and review

The existing character adapter inserts the two clips at canonical animation
indices 3/4. A/D movement selects them through the same animation graph, at the
same speed-scaled playback rate as the jog (2.0× at 7.2 m/s), where planted feet
move with the ground: 3.32 m/s × 1.066 player scale × 2.0 ≈ 7.1 m/s. Because the
footfalls are frame-aligned with the jog, the client carries the step phase
across forward/lateral switches. Backward still reverses the forward cycle.

Automated tests check source composition, timing, loop closure, fixed root,
footfall frames, planted-foot speed, swing clearance, separation, reproducible
authoring and distinct left/right source selection. On the retargeted Ranger
female, planted feet need a 1.97× rate for zero slide at 7.2 m/s (the client
uses 2.0×). Retargeting to the ten supplied humanoids leaves planted-height
deviations up to about 1.8 cm. **Human in-game/browser review is pending**,
including both directions, diagonals, reversing/stopping, camera steering,
jump/landing transitions and frame pacing.
