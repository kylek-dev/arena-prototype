# Universal strafe animation library

`universal-strafes.glb` is a **standalone reusable animation source**, not a
female-character or outfit export. It contains:

- `Strafe_Left` and `Strafe_Right`: 0.8-second in-place loops at 30 authored
  samples/second, with an exact closing pose.
- The shared **65-joint Quaternius humanoid skeleton** and inverse bind matrices.
- One three-vertex weighted carrier triangle so engines retain a standard skin.
- **No character body, clothing, hair, textures, race asset or studio objects.**

File size: **119,272 bytes**. SHA-256:
`1274927858cee07697df51e25db1ed349a30710480884189f88616ff36847aa8`.

## Reuse and licensing

Keep this README and [LICENSE.txt](LICENSE.txt) with the GLB when sharing it.
New motion curves and carrier geometry are **MIT**, matching this repository's
new original authoring code. The reference skeleton and baseline idle pose are
by **Quaternius**, from the **CC0 1.0 Universal Animation Library 1 Standard**:

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

Authored on 2026-11-14 in **Blender 5.3.0 Release Candidate**, through Blender MCP
with **OpenAI GPT-6.1 Sol** assistance, at the user's request for missing lateral
animations that are universal/shareable across bases, outfits and races, using
neutral asset/scene names.

`scripts/blender/universal_strafes.py` imports the CC0 UAL1 mannequin into a new
authoring scene, takes a single idle baseline pose for upper-body/finger
posture, and authors new lateral motion. It does **not** reuse the forward jog
as a strafe. Analytic two-bone foot placement creates an outward lead step and
trailing close, alternating support/swing, raised-foot clearance, flexed knees,
small pelvis bob/lean and arm counter-motion. Distances scale with the reference
leg lengths. Keys are baked; no Blender constraints, drivers or runtime IK are
required by consumers. Root translation/orientation remain fixed.

Recreate from the project root:

```sh
blender --background --factory-startup --python scripts/blender/universal_strafes.py
python3 scripts/check_animation_compatibility.py
./scripts/build-web.sh
```

In a live session the script adds its own scene and retains earlier work; the
source downloads are not modified. The clean-background command avoids naming
collisions with previous authoring sessions. No model generation service,
motion capture, proprietary game animation or runtime AI was used. AI assistance
does not establish exclusive rights.

## Game integration and review

The existing character adapter inserts the two clips at canonical animation
indices 3/4, replacing only the forward-jog strafe substitutes. Existing A/D
movement selects them through the same animation graph. Forward/backward,
jump/cast, combat rules, facing, input and the disabled-weapon state are unchanged.
Backward still reverses the forward cycle.

Automated tests check source composition, timing, loop closure, fixed root,
alternating feet, separation, and distinct left/right source selection. Static
poses of the mannequin and retargeted Ranger female were inspected in Blender.
The full build and real Bevy loader passed. **Human in-game/browser review is
pending**, including both directions, diagonals, reversing/stopping, camera
steering, jump/landing transitions and frame pacing. In-place stride cadence
is not calibrated to eliminate foot sliding at every game speed.
