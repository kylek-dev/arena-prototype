# ADR 0053: Finished character kits rebuilt from generated models

Date: 2026-11-24. **Status: accepted for the human male.** Implemented
locally and reviewed by the user in the browser on 2026-11-24. Requested by the
user: a base male "more generic with blank customizable pieces", ready for
generated equipment. Hair and facial hair are not built yet.

## Context

The first Meshy body ([ART-PRODUCTION.md](../ART-PRODUCTION.md)) went in game
as one mesh with one baked texture. Skin tone and eye colour were recovered by
detecting painted regions in that texture. The user called that approach hacky
and asked for a kit built from separate pieces instead, with clean topology,
proper UVs, a standard rest pose, attachment sockets and body regions for these
slots: head, shoulders, back, chest, shirt, tabard, bracers, gloves, belt,
pants, boots, and main hand, off hand, two-handed, ranged.

The generated model supplies the look the user approved (face, proportions,
painted style). Its structure does not fit a kit: a uniform grid mesh with no
eye sockets, fused toes, a 28-joint rig without fingers, and clothing sculpted
and painted into the body.

## Decision

**The generator supplies the art; the kit supplies the structure.** A finished
kit keeps the generated shape and paint and replaces everything else.

- **Topology.** The CC0 MakeHuman base mesh is fitted to the generated sculpt
  (pose alignment, then a non-rigid surface fit; 0.13 mm median distance on the
  human male). It is used as scaffolding only: edge loops, separate fingers and
  toes, eyelids, skin weights for a UE-named rig and face shape targets. Ears
  and the insides of the eyelids keep the scaffold's forms. The scaffold's
  mouth cavity is removed: the lips close on the generated model's painted
  mouth line, with both lip edges on one curve.
- **Paint.** The generated paint is transferred to the new UVs. Painted
  clothing becomes skin by re-colouring its own painted shading with the skin
  tones measured on the body, so the body is blank skin everywhere. The painted
  eyebrows are lifted out. Sculpted hems are smoothed out of the sculpt before
  the normal map is baked. Dark blots in the generated paint are refilled from
  the good paint around them, and every old garment edge (neckline, armholes,
  waist, leg hems) is blended across. Last, the skin is graded onto the
  palette's `skin-natural` (hue and chroma moved in Lab, shading kept), so each
  skin tone's tint lands on its swatch. The generated paint was a saturated
  salmon, which made dark tones too red.
- **Separate pieces, one material slot each.**

  | Feature | Nodes | Material | Runtime control |
  | --- | --- | --- | --- |
  | Skin | `Body_*` regions | `Skin`, `Skin_Head` | skin tone tint |
  | Eyes | `Eyes` | `Eye_White`, `Eye_Iris` (grey) | eye colour tint |
  | Eyebrows | `Brows_*` (alpha-blended) | `Brows_*` (grey) | shape; hair colour tint |
  | Hair | `Hair_*` | `Hair` (grey) | style; hair colour tint |
  | Facial hair | `Beard_*` | `FacialHair` (grey) | style; its own colour tint |
  | Face | morph targets on `Body_Head` | | preset |
  | Underclothes | `Trunks` | `Trunks` | hidden by pants |

- **Rest pose and skeleton.** The kit skeleton keeps the 65 joints, names,
  order and hierarchy of [ADR 0040](0040-character-customization-kits.md). The
  body is bound in its own A-pose, which is also the file's rest pose. Torso,
  head, clavicle, leg and foot joints keep the kit's rest orientations and the
  arm and hand joints the previous kit's, so the existing clips move the body
  and wrists as before. Finger joints sit on the fitted fingers and are aligned
  to them, and the clips' finger keys are re-based onto this hand. Pelvis translation
  keys are scaled to the body's hip height. The pelvis, spine and neck-base
  joints sit where the previous kit had them, and the torso weights are
  redistributed by height around them: the clips' back arch was tuned on those
  pivots, and on the scaffold's lower pivots it thrust the chest out. Each
  hand-over runs from the joint below to the joint above, because Attack bends
  one spine joint by 32 degrees and a short hand-over creased the bare chest.
- **Clips without finger animation.** A clip that leaves every finger at the
  kit's flat rest pose (Meshy's rig has no fingers, so its run clips do) takes
  the hands of the Idle clip. Otherwise the fingers spring flat open.
- **Regions.** The body is cut along edge loops of the mesh into `Head`,
  `Neck`, `Chest`, `Abdomen`, `Shoulders`, `UpperArms`, `Forearms`, `Hands`,
  `Knees`, `Calves`, `Shins` and `Feet`. `Hips` and `Thighs` lie under the
  `Trunks` piece and are not shipped. The waist and leg-opening loops are
  flattened onto planes, so garment edges are straight, and the trunks carry a
  plain painted waistband and hems on those edges. Each equipment piece lists
  the regions it covers, as in ADR 0040.

  | Slot | Attached | Covers by default |
  | --- | --- | --- |
  | head | rigid on `socket_head` | hair |
  | shoulders | rigid on `socket_shoulder_l/r` | nothing |
  | back | skinned to the spine, placed from `socket_back` | nothing |
  | chest | skinned | `Chest`, `Abdomen`, the shirt beneath |
  | shirt | skinned | `Chest`, `Abdomen`, plus `Shoulders`, `UpperArms`, `Forearms` by sleeve length |
  | tabard, belt | skinned | nothing |
  | bracers | skinned | `Forearms` |
  | gloves | skinned | `Hands` |
  | pants | skinned | `Trunks`, `Knees`, `Calves` by length |
  | boots | skinned | `Feet`, `Shins`, plus `Calves` when tall |
  | main hand, two-handed | `socket_weapon_r` | |
  | off hand | `socket_weapon_l`; shields on `socket_shield_l` | |
  | ranged | bow in `socket_weapon_l`, crossbow in `socket_weapon_r` | |

- **Sockets.** Plain named nodes under a joint, outside the skin, so clips and
  skinning are unaffected: `socket_head`, `socket_shoulder_l/r`,
  `socket_back`, `socket_back_l/r`, `socket_chest`, `socket_belt`,
  `socket_hip_l/r`, `socket_weapon_l/r` and `socket_shield_l`. Body sockets are
  aligned to the model's axes in the rest pose. Grip sockets have +Y along the
  gripped shaft towards the thumb side and +Z the way the knuckles point. The
  shield socket has +Z out of the forearm and +Y towards the elbow.
- **Manifest.** A kit entry may add `morphNodes` (nodes carrying the face
  targets), a `face` group whose options carry `weights`, the tint kinds
  `eyes` and `facialHair`, and informational `sockets` and `slots`. A group
  without a `none` option defaults to its first option.
- **Build.** Finished kits are built offline and placed in
  `assets/characters/kits/<id>/` as `<id>.glb` and `kit.json`.
  `scripts/character_kits.py` copies them as built and lists them after the
  Quaternius kits at the shared scale.
- **Appearance and protocol.** A look gains `facialHairColor` (the hair colour
  until chosen) and the `face` group. The armour slots join the outfit slots.
  `PROTOCOL_VERSION` is 8: `WireAppearance` carries `facial_hair_color` and up
  to 24 part groups.

## Consequences

- Skin, eye, hair and facial hair colours are plain per-material tints. No
  texture is analysed at build or run time, and no eye texture variants ship
  for finished kits.
- The human male kit is 7.0 MB. Its body is 23,156 triangles, inside the 25k
  body budget of ART-PRODUCTION.md; with trunks, eyes and one brow set it
  draws 25,796. The dense hands and feet of the scaffold account for 10,920.
- Meshy's image editor refused the "remove the top" concept edit as a content
  policy matter, so the bare torso is derived from the model's own paint
  rather than regenerated.
- A shorter underclothes piece would need the `Hips` and `Thighs` skin
  exported. Its paint exists.
- The client does not yet attach anything to the sockets. Weapons still ride
  the back carrier.
- Hairstyles and beards are not built yet. They need generated pieces.
- The offline tooling is in `scripts/finished_kit/` (see its README). It
  needs Blender, scientific Python packages and the art workspace outside
  Git, which holds the raw Meshy files and every intermediate file. It is
  written for this one body.
- Until approved, the kit is listed as its own ancestry (`meshy`, shown as
  "Human (new)") beside the Quaternius human male.

## Evidence

- Offline: clip renders of the shipped GLB in Blender for all ten clips, mesh
  checks (all quads, no non-manifold edges, exactly symmetric, 1.81 m, soles on
  the ground), and side-by-side renders against the generated original, which
  the user approved on 2026-11-24.
- Tests: `tests/unit/test_character_kits.py` (prebuilt kits),
  `tests/domain/appearance.test.ts`, `tests/ui/customizer.test.tsx`,
  `arena-client` appearance spec tests and the `arena-protocol` golden frame.
- Browser review by the user on 2026-11-24 found five faults: a half
  skin-coloured, wavy waistband; light bands at the leg openings and harsh
  dark skin tones; black marks on the back; a line through the mouth; and a
  thrust-out chest in the clips. All five are fixed in the rebuilt kit and
  checked offline (renders of the shipped GLB, a simulated `skin-ebony` tint,
  and the Idle torso outline measured against the previous kit: chest 2.5 cm
  in front of the belly, previously 2.4 cm on that kit).
- A second note from the user the same day: in the run the hand was bent and
  open. The hand joint is now bound as on the previous kit, the finger joints
  moved onto the fingers (the index knuckle had been 13 mm outside the skin),
  and the two run clips take the Idle hands. Checked in close-up renders of
  both hands through the run, and of the Idle and Attack fists.
- The user reviewed the rebuilt kit in the browser on 2026-11-24 and accepted
  it ("All that is good"). No automated browser check was run.
