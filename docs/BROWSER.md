# Browser development

**Human UI review: verified and approved by the user on 2026-11-21 for all implemented UI changes through ADR 0048 (superseding the 2026-11-08 approval and all subsequent pending UI reviews).** See [review record](UI-REVIEW.md). Earlier pending-review statements below are historical and superseded by that confirmation; the manual scenarios remain a regression checklist. Browser/device details, individual console results and performance measurements were not supplied.

This is a local solo practice scene. Start the original Rust headless example with `cargo run -p arena-headless`, or build the browser client below. The player defaults to the animated Quaternius female human when its curated sources are installed; otherwise it uses a procedural fallback without credentials. See [character setup](#quaternius-default-female-2026-11-14) and [asset provenance](../assets/README.md) before redistributing binary art. Rebuild the complete client after changing character sources.

## Build and play

The currently targeted unit has a subtle pale-gold border on its team/opponent frame and world nameplate. Focus alone does not highlight it; clearing/changing target removes/moves the border. Nameplates read selection from the render-rate scene snapshot. Human rendered review of this selection highlight is pending.

Nameplates now include the unit's name in small, plain text just above the bar, aligned to its left edge. Long names truncate to the bar width; targeting/focus, health updates and visibility settings are unchanged. Human rendered review of the label placement is pending.

**New practice modes (human rendered review pending):** Escape → **Practice mode** selects 1v1, 2v2 or 3v3 and starts a fresh round. R/menu reset keeps the selected mode; reload defaults to 1v1. Team frames appear on the left and exclude yourself (0/1/2 teammates); opponents appear on the right (1/2/3). All other units are stationary and do not attack. Left-click targets and right-click focuses every frame/model/nameplate; Tab cycles living enemies. Pulse rejects friendly targets, and the round ends only when every enemy is defeated. Layout editing includes a separately movable/saved team group. Existing positions are preserved. Rebuild WASM and frontend together. This supersedes historical single-opponent/no-teammate notes below; it does not implement multiplayer. See [ADR 0025](decisions/0025-team-practice-rosters.md) for the manual checklist.

```sh
rustup target add wasm32-unknown-unknown
cargo install wasm-bindgen-cli --version 0.2.122 --locked
npm ci
./scripts/build-web.sh
npm run serve
```

Open http://127.0.0.1:8000. Rust generates `web/public/pkg` and `web/public/assets`; Vite copies them with the React overlay to `dist/`. All output is ignored. Publish the entire `dist/` directory, not `web/`. URLs remain relative for subpath hosting. Use HTTPS, Brotli/gzip and `application/wasm`. A simple alternative is `python3 -m http.server 8000 --directory dist --bind 127.0.0.1` (no compression).

After one complete build, `npm run dev` provides frontend hot reload using the existing engine. `npm run build` rebuilds only TypeScript/CSS and copies existing engine assets; it cannot make a fresh checkout playable without the complete build. [Frontend boundaries and behavior](FRONTEND.md) documents ownership, profiles and input modes.

Controls:

- **WASD:** move relative to character facing. W/S move forward/backward; A/D strafe left/right. Movement never turns the character.
- **NumLock (or `):** toggle autorun forward (remappable in Settings → Bindings → movement). Strafing and jumping keep it running; pressing forward/back, walking with both mouse buttons, or the autorun key again stops it, as do menus, focus loss and reset. Single-button clicks and drags (look, turn, target, focus) keep it running. Existing custom NumLock/` bindings are preserved.
- **Space:** jump, carrying takeoff momentum until landing (remappable in Settings → Bindings → movement). No air steering, double-jump, or automatic repeat while held. Walls/pillars still block movement; jumping does not avoid attacks or bypass cover. Existing custom Space bindings are preserved.
- **Left-drag:** orbit the camera only. Character facing and movement direction remain independent of the camera, including after release or stopping and restarting movement.
- **Right-drag:** orbit the camera and turn the character to face that direction.
- **Ordinary right-click on a character model:** focus that character without changing target. Empty space leaves focus unchanged. Both model buttons use nearest rendered-character picking and respect pillars; nameplate buttons remain usable through pillars. Rebuild WASM and frontend together. Human rendered review of model right-click focus is pending.
- **Left + right held:** walk forward while steering and turning with the camera.
- **Wheel:** zoom from 22 m down to 1.5 m, then one more step snaps into first person at eye level (your character is hidden; zoom out to return). The orbit pivot rises from chest to eye height inside 6 m. Dragging can raise the camera to just short of directly overhead (and as far below), so it can pass up and over the character. Rebuild the complete client for these camera changes. **Tab or ordinary left-click:** select. **Escape:** game menu. **Shift+Escape:** clear target.
- **1 / Pulse:** attack. **R / Reset:** restart.
- **F:** set focus from selected target. **O:** settings. **P:** spellbook. Bind clear/target focus and other interface commands in Settings.
- **V:** toggle enemy nameplates. **Shift+V:** toggle friendly nameplates. Both are remappable in Settings → Bindings. Existing custom uses of these keys are preserved.

Reset (keyboard or menu), round completion and an engine tick rollback now release mouse capture and restore the cursor together with clearing held movement/steering. Release and press a mouse button again to begin a new gesture. Late pointer-lock grants after cancellation are released, too. Human browser review of this lifecycle fix is pending: finish/reset while right-dragging or holding both buttons, with pointer lock allowed/denied, and verify visible cursor, no stale movement and clean steering after a fresh press.

World nameplates are compact health-only bars floating above other characters (red for enemies, green for friends). Left-click a bar to target; right-click to focus. They follow rendered movement/camera motion, remain visible and clickable through pillars, hide offscreen/behind the camera, and never appear above your own character. **Settings → Nameplates** independently enables friendly/enemy bars, saved with the interface profile. The practice scene has one enemy and no teammates. Rebuild the complete client to install engine projections; older engine builds show no plates. The through-pillar correction follows the earlier UI approval and awaits human rendered review; world-hover occlusion and combat line-of-sight rules are unchanged.

Successful outgoing damage now floats above the recipient in gold, rising/fading over 1.2 seconds. Healing text supports green `+amount` numbers, but there is no healing ability yet. Other people's contributions and all numbers above your own character are excluded. Text is independent of target selection and nameplate visibility, passes pointer input through, and clears on reset. V/Shift+V and Settings nameplate changes show yellow **Enemy/Friendly nameplates enabled/disabled** notices for 2.5 seconds in the same top-center area as red cast errors. Rebuild the complete client; human rendered review of these additions is pending. See [ADR 0024](decisions/0024-outgoing-combat-text.md) for the manual checklist and verification.

The camera automatically pulls in before hitting the floor, pit walls, stands, room walls or pillars, and your own character is hidden while the camera is inside it
(including their bases/caps). Continue orbiting to move along/around obstructions;
when the view clears, it smoothly returns to your chosen wheel zoom. Collision may
bring it closer than the normal wheel minimum without changing that preference.
Only camera contact triggers pillar collision: a distant pillar may sit between
the camera and character without forcing zoom-in. Camera movement slides against
either side of a pillar rather than passing through it; orbit around/over it to
reach the other side.
The pitch range includes low angles; floor collision keeps the eye above the map.
**Human rendered review of camera collision is pending.** Rebuild the complete
client with `./scripts/build-web.sh`; a frontend-only rebuild cannot add this change.

Single-button presses leave the cursor and look/turn modes unchanged until cumulative movement reaches 4 pixels; ordinary left/right clicks do not request pointer lock or flash its browser notice. Crossing that drag threshold, or holding both buttons to walk, hides and locks the cursor. Releasing one button stops mouse-driven walking and keeps the remaining button’s look/turn mode; releasing both restores the cursor. Drags and button chords never count as targeting clicks. Lost pointer lock, focus loss, and tab hiding cancel held mouse movement. If a browser denies pointer lock, the cursor is hidden over the canvas and dragging works within the page.
All live unit frames (player, target, focus and opponents) support left-click to target their represented unit and right-click to focus it. Player self-targeting is supported, but Pulse cannot attack the player. Layout mode instead selects/drags sample frames. Unit tags reserve Opponent1–3 and Team1–3; only one opponent and no teammates exist today. Rebuild the complete client for the appended self-selection bridge field. Human browser review of these interactions is pending.

Mouse gestures hide and lock the cursor. Releasing one button stops mouse-driven walking and keeps the remaining button’s look/turn mode; releasing both restores the cursor. Drags and button chords never count as targeting clicks. Lost pointer lock, focus loss, and tab hiding cancel held mouse movement. If a browser denies pointer lock, the cursor is hidden over the canvas and dragging works within the page.

Canvas focus is reclaimed after ordinary HUD interaction; gameplay also routes while a non-keyboard-owning HUD control briefly has focus. Text fields, modal dialogs, binding capture, quick bind and layout editing own/suspend keyboard input and clear holds; plain HUD clicks and the nonmodal spellbook do not. Window focus loss, hiding and unexpected pointer-lock loss still cancel input. Shift+Tab explicitly opts into UI navigation without automatic focus reclamation; clicking in the page or explicitly refocusing the canvas resumes gameplay routing.

Escape opens the game menu instead of a top-right toolbar. Its `menu` action is remappable in Settings alongside movement, targeting, reset, focus, slots and UI toggles. Physical unmodified and Shift chords are supported; Ctrl/Alt/Meta/function keys and Shift+Tab remain reserved. Escape cancels captures except when assigning the menu action; use Cancel to leave that capture. Old default Escape-clear bindings migrate to the menu and Shift+Escape-clear; custom Escape uses and saved locks are preserved. Printed labels may differ on non-US layouts. Mobile controls are not implemented.

Spellbook contains only Pulse as an action-slot-sized draggable icon; tooltips show rule-owned metadata. New/default profiles start unlocked without a warning banner. Catalog drag copies; slot-to-slot drags move/swap; Shift-drag copies. Drop a slotted spell on the canvas to remove its assignment while retaining slot/bindings. Catalog-to-canvas and cancelled/invalid drops do nothing. Assignment is deliberately drag-only per user request: no dropdown or alternative placement/removal forms. Lock/unlock in Settings → Action bars; saved locks are preserved. Click/key activation shares assignments and authoritative cooldowns; dragging never casts. Unavailable actions explain why without submitting an attack; Rust validates submitted intents.

Quick bind edits a hovered/focused slot's primary key, offers Unbind, and exits with Escape/Done. Settings edits both entries with explicit Replace/Cancel for conflicts. Target/focus hide when absent. Layout editing always shows labelled sample frames, including hidden/absent units. Click a frame/bar to select it and drag its whole body (or use arrows): no frame dropdown, X/Y sliders or frame drag bars. Move the editor window by its title; adjust selected visibility/scale/lock and reset selected/all; Done/Escape exits. Assignment/position locks are independent. Limits remain four bars, twelve slots each, eight local profiles. Import adds unique profiles without overwriting another; invalid/future storage is preserved until explicit recovery. Export when saving fails. Minimum desktop viewport: 800×600; oversized content fits the viewport. Styling uses Radix Themes ([ADR 0008](decisions/0008-gameplay-first-ui.md)).


The opponent is stationary and does not attack. Its health reaching zero ends the round. The playable arena is 108×84 metres, sunk in a pit beneath empty tiered stands with wooden benches, inside an enclosing textured colosseum hung with painted heraldic banners and rooftop pennants, under a high-resolution painted sky. Four 9×9 metre square pillars centered at (±21, ±15) metres rise 48 metres, leaving open central lanes. Base movement is 7.2 m/s. Each team spawns facing the other inside its own starting room behind a gateway at either end of the pit, so an opening approach takes about 12 seconds (see [ADR 0032](decisions/0032-starting-rooms-camera-and-weapon-fit.md)). Walls and pillars block movement. Pillars also block Pulse's line of sight. Human rendered review of the venue and enlarged arena is pending; see [ADR 0031](decisions/0031-colosseum-venue-and-larger-arena.md). All unit frames show only the unit name, green health bar and blue resource bar, with current/maximum values centered inside the taller bars. Resource values remain placeholders (identified in the tooltip and accessible text): resource costs and regeneration are not implemented. Role, selection/focus labels and frame descriptions are not displayed. Default/reset placement puts player and target side by side at the upper left, focus toward the center-right, and opponents at the right; existing saved positions are preserved. Use Layout → Reset all positions to adopt the new defaults in an existing profile.

The practice-round box is removed; reset remains available through R and the game menu. A dedicated top-center area shows bold red errors only when an attempted spell cast fails (including missing target, range, line of sight, recharge or an authoritative rejection). Passive availability changes do not show errors. Messages expire after 2.5 seconds; repeated failed attempts restart that interval, while a valid cast attempt or reset clears them immediately. Empty slots and suspended gameplay/editing do not count as cast attempts. Startup failures and recovery remain in the loading panel, not the combat-error area. Human rendered review of these HUD changes is pending; automated DOM tests, frontend lint/format checks, WASM Clippy and the complete web build passed locally on 2026-11-07. No rendered/browser pass is claimed.

Action-bar slots show spell icons and keybinding labels, without printed spell names or separate slot-number labels. Tooltips retain spell details and bindings, accessible names retain slot identification, and cooldown/key-held feedback is unchanged. Human rendered review of this simplification is pending.

## Graphics quality

Open **Settings → Graphics** (O, or Escape → Settings). **4× MSAA** and **High** procedural model detail are the defaults. Anti-aliasing smooths floor seams and character/object outlines; Off reduces GPU cost. High adds curved-surface geometry to procedural characters only, including the optional model's fallback. It does not alter imported model geometry, square pillars, collision or combat. Changes apply immediately and save on this device separately from HUD profiles; Reset graphics restores the defaults. Storage failures show a warning while keeping the change active for this session. Rendering remains at native resolution; render-scale controls are not implemented.

**Weapon glow (bloom)** is on by default and adds thresholded soft halos around bright energy. Toggle it off to reduce post-processing cost; HDR/Reinhard tone mapping and explicit exposure stay active. Existing saved graphics settings acquire the on default without being overwritten simply by loading. After user review found the initial HDR revision too dark, lighting is restored to brighter ambient/key levels with explicit EV100 8.0 exposure and gentler tone mapping. The lit staff crystal now emits above the bloom threshold in every palette. Independent camera-facing soft halos at the crystal/shaft channels and twelve animated orbiting sparks remain visible even with bloom off; the toggle controls post-processing, not those local effects. Source geometry, facets and GLBs are preserved. This approximates the intended glow, not Blender's exact studio render or diffraction streaks. Review Off/On, all colors, arena readability, both MSAA settings, halo depth/occlusion, switching during loading, console errors and intended GPU performance manually. **Human rendered review of this correction is pending.** See [ADR 0019](decisions/0019-weapon-lighting-and-visibility.md).

After rebuilding the complete client, manually compare Off/4× and Low/High while orbiting/zooming, check targeting/world hover, reload persistence, optional-model/fallback behavior, and browser console errors on the intended GPU/browser. **Human rendered/browser review is pending**; automated tests/builds do not establish appearance or performance.

## Map editor

Open **Escape → Map editor**. The movable window shows the arena from above
(north up), with the stands and building around the floor. Its parts:

- **Files:** **Open map** switches between the maps in `assets/maps`;
  **New map…** starts a blank arena (name, file id, ground). The map opened
  or saved last is the one the arena starts with on this device.
- **Plan:** drag pillars, rooms, spawns and props. Selected pillars get a corner size
  handle and selected rooms a back-wall depth handle. Moves snap to 0.5 m,
  or 0.1 m while holding Alt.
- **Inspector:** typed values for the selection. With nothing selected it
  shows the map name, ground (stone, sand, snow or grass), floor size and venue settings (pit wall, tiers,
  concourse, outer wall, pillar height). Values apply on Enter or when the
  field loses focus; Escape reverts a field.
- **Props:** pick a model and **Add prop**, then drag it; the inspector sets
  model, position, height, turn and scale. Props are decoration only:
  fighters, spells and the camera pass through them.
- **Keys:** arrows nudge 0.5 m (Shift: 2 m); Delete removes; Ctrl+D duplicates
  a pillar or prop; R / Shift+R turns a prop 15°; Ctrl+Z and Ctrl+Shift+Z (or Ctrl+Y) undo and redo; Escape clears
  the selection, then closes.

Each committed edit goes live in the arena. The round continues unless a unit
would end up inside a wall, in which case it restarts at the new spawns. The
game validates every edit and lists the problems (room ceilings, hiding
corners, blocked openings, unreachable spawns and so on). An invalid map is
never applied; the last valid one stays in play.

**Save as <id>.json** appears under `npm run dev` and `npm run serve`. It
writes `assets/maps/<id>.json` in the repository and only accepts
requests from the local machine, and only after the game has accepted the
map. Reload the page to start from the saved file; rebuild to embed it in the
WASM. Static hosting has no save endpoint, so use **Download** and **Import…**
there. Closing the window keeps your edits for the session, and the browser
warns before leaving with unsaved changes. See
[ADR 0037](decisions/0037-map-files-and-browser-map-editor.md) and
[ADR 0041](decisions/0041-map-props-grounds-and-new-maps.md). Human browser
review is pending.

## Temporary live lighting tweaks

After one complete client rebuild/reload to install the controls, open **Escape → Lighting tweaks (temporary)**, or **O → Graphics → Open lighting tweaks**. Drag the title to move the nonmodal window; there is no dark scrim. Adjust background RGB, ambient/main/fill lighting, EV100 exposure, bloom intensity/threshold/wide halo and see the changes live without reloading. **Lower EV100 is brighter.** The starting exposure preserves the user's manual **10.0** tuning; older references to EV100 8.0 describe the preceding iteration.

Slider focus owns keyboard input and clears held gameplay controls. Click the arena to orbit/move with the panel open. Close/reopen keeps your session's values; **Reset lighting tweaks** restores defaults. Expand **Current values** and select/copy the JSON before reloading. Experimental slider values are **not saved** and reset on refresh; the separately labelled bloom enable toggle remains a device preference. Bloom sliders have no visible effect while bloom is off and do not control the staff's independent local halos/sparks. Human browser review of the live panel is pending; see [ADR 0020](decisions/0020-temporary-live-lighting-panel.md).

The expanded panel adds **saturation, contrast, gamma, shadow lift, midtone/highlight gain, warm/cool balance and tint**, plus **main/fill RGB and direction** and **three tone-mapping choices**. Grading affects the world, not the HUD. All new grading defaults are neutral; Reinhard and the original light colors/directions remain the starting look. Light direction controls do not enable cast shadows.

For tuning, first turn bloom off (this device preference stays off until you re-enable it). Set exposure for overall brightness, then adjust main/fill direction and strength for readable faces. If colors look washed out, reduce ambient or gently increase saturation/contrast. Raise shadow lift or fill for dark faces without brightening everything; lower highlight gain to recover bright-area detail. Compare tone mapping before fine-tuning gamma/color balance. Finish by restoring bloom and tuning its intensity/threshold. Copy JSON before refresh. Rebuild the **complete client** to install the expanded controls; a frontend-only build cannot add engine support. Human browser review of the extension is pending, including continuous response, all tone mappers, reset, keyboard focus, small-viewport scrolling and console/GPU behavior.

## Fast automated verification

Assigned action buttons use a subtle red tint for defeated targets, round completion, out-of-range targets or blocked line of sight. With no target selected, buttons stay neutral. Other unavailable states tint grey, including recharge and any future resource rejection reported by the engine; resource costs are not implemented today. Empty slots stay neutral. Keybinding labels and cooldown overlays remain visible. Human rendered review of these tints is pending.

```sh
npm test
npm run test:watch
cargo test -p arena-sim --locked
```

`npm test` runs the original Node input suite plus Vitest domain/DOM/runtime tests after `npm ci`, without a browser, server, assets, renderer or WASM build. `npm run test:input` remains Node-only with no npm install. Tests cover physical chords, alternate holds/modifier release, repeats, conflicts, capture/quick-bind isolation, typing, focus selection, atomic slots, bounds, profile operations/import, malformed/future storage, dialogs/focus, errors and settings. The original mouse/cursor and positional Rust input contract regressions remain intact. DOM tests do not implement browser layout. Rust tests validate combat and presentation contracts.

For the complete code checks and a browser build:

```sh
./scripts/check.sh
cargo clippy -p arena-client --target wasm32-unknown-unknown --locked -- -D warnings
./scripts/build-web.sh
cargo run -p arena-headless
```

CI runs Rust rules/headless linting/tests and workspace formatting. Its required `browser` job installs pinned npm packages and checks frontend typechecking, lint, formatting, input/domain/DOM tests and a frontend production build. Neither automatic fast job compiles Bevy/WASM or installs a browser. See [ADR 0007](decisions/0007-react-combat-ui.md).

The separate `Full client build` workflow checks changed files on every PR and main push. Changes to Rust crates, Cargo manifests/locks, the Rust toolchain, `.cargo` configuration, assets, scripts, or workflow files trigger full native workspace checks, WASM Clippy, and the web build, uploading `browser-build`. Docs-only and JavaScript-only changes skip those expensive jobs. The required `client-build` check passes only when both builds succeed or change detection confirms neither is needed; detection failures block it too. It does not install or launch a browser.

Before release, or to check any branch on demand, run the complete local checks above or dispatch `gh workflow run client-build.yml --ref YOUR_BRANCH`, which always requests full builds. A successful `client-build` check on an unrelated change means compilation was intentionally skipped, not observed to pass. Record human review separately. See [ADR 0006](decisions/0006-selective-client-builds.md).

## Manual E2E acceptance

E2E testing is a human review step, not an automated development or CI gate. Build, run `npm run serve`, and review these scenarios in the intended browser:

1. **Startup and assets:** a clean clone loads the procedural player with no missing model requests. If the optional character is installed, verify its appearance and movement. An unavailable configured model should retain a playable fallback and show a notice.
2. **Combat loop:** WASD approaches the target; Tab and an ordinary click select it; 1 reports no target/out of range/recharge correctly, deals four hits to finish, and R or Reset starts a fresh round.
3. **Mouse modes:** left drag changes only the view; hold W, left-drag, and release left-click to verify that character heading, walking direction, and camera angle remain unchanged. Stop, orbit the camera, release, then press each WASD key: movement must remain character-relative, with A/D strafing and S backing up without turning. Right drag turns without walking; both buttons walk and steer in either press order. Releasing either button stops mouse walking and retains the remaining look/turn mode. Releasing both restores the cursor. Drags/chords must not target or focus. Ordinary model right-clicks focus player/opponent without changing target, attacking, pointer lock or a context menu; right-click misses preserve focus. Verify rendered jumping positions, nearest-unit selection, pillar blocking, movement held during clicks and cancellation by menus/blur. Model-focus review is pending separately from earlier UI approval.
   **Jumping:** Space at rest jumps in place; WASD or both-button walking plus Space carries movement through the air. Releasing/reversing movement or turning midair must not redirect the jump. Check walls/pillars, exact landing, no held-Space repeat/double-jump, reset during flight, rebindings and typing/menu suppression. Review the follow camera and both procedural/optional characters. **Human rendered review of jumping is pending.** Rebuild the complete client; a frontend-only build cannot add the rule/engine change.
4. **Cancellation and focus:** movement survives ordinary mouse release and HUD focus/clicks. Typing/modal/capture/editing owners, window blur, hiding and unexpected lock loss clear holds. Confirm canvas recovery never steals typing focus or Shift+Tab navigation and the spellbook stays open while moving. Verify Escape menu/rebinding and Shift+Escape clear. Independent focus follows current health and clears on reset; absent target/focus hide.
5. **Presentation and errors:** inspect the HUD, arena, target indicator, model, and camera at desktop dimensions; check the console. `?debug=1` exposes read-only diagnostics, while `/` does not expose `arenaDebug`. Loading failures must remain visible.
   **Camera collision:** orbit behind each pillar at low/high angles and walk/steer alongside its faces/corners. A pillar well between the eye and player must not force zoom-in; actual camera contact must stop/slide against the near or far face without clipping or tunneling. Include pillar bases/caps and arena rims. Pitch down to floor contact and continue orbiting, then raise the view. The camera should pull in at contact, remain responsive along surfaces and ease back to the chosen zoom when clear. Repeat while jumping, at minimum/maximum wheel zoom, after reset and with rapid direction changes. Check close character views and intended viewport aspect ratios. Human rendered review of this contact-only revision remains pending.
6. **Configuration:** remap movement and both binding entries, exercise Replace/Cancel, reserved shortcuts and capture cancellation. Quick-hover/focus binding must not cast, reset, select or move. Tab must navigate UI normally.
7. **Spell placement:** default unlocked without warning, catalog-copy, slot move/swap/Shift-copy, cancelled/invalid drops and slot-to-canvas removal (binding retained). Confirm icon sizes and no assignment forms. Drags must never turn the camera or cast. Lock to protect assignments; verify shared click/key mapping and cooldown.
   Icons and bar tracks now use a shared 64×64 px base size. Check the captured-pointer ghost/target outline, short click versus drag, release over icon text/cooldown layers, fast drags, Escape cancellation and outside release. If a saved profile is locked, explicitly use Unlock spell dragging in the spellbook. Native HTML drag events are no longer used.
   Verify the cooldown reveals clockwise smoothly rather than stepping with snapshots; duplicated Pulse slots must stay in phase. Test tab hiding/resume and reduced-motion static shading. Animation must never enable casting before Rust reports ready.
   Bound slots show a simple white highlight while their key is held, including empty slots and rejected casts (no target/recharging). Verify primary/alternate/Shift bindings, release and focus-loss cleanup, and no highlight while typing or binding. This feedback reflects input only, not spell activation. Human rendered review of this highlight is pending.
8. **Layout and persistence:** always-sample frames, direct click selection, whole-frame dragging/arrows, movable editor title, visibility/scale/locks and resets; no frame dropdown/coordinate sliders/drag bars. Resize at 800×600 and larger. Verify menu reachability, profiles/import/export/reload and storage-failure feedback.
9. **Unit tooltips:** hover the player/opponent in the arena and every live unit frame. Verify one shared tooltip stays at its saved anchor (bottom-right initially), displays current health without changing selection/focus, clears on exit and ignores units behind pillars. Check camera dragging, Escape, window blur, menus, spell dragging and binding suppression. In Layout, drag the `unitTooltip` preview, change scale/visibility/lock, reset it and reload/export/import the profile. Check pointer pass-through and bounds at 800×600 and larger. World hover requires a rebuilt engine. Human review remains pending.
10. **World nameplates:** verify a health-only bar above the enemy, never the player (including self-target/focus). Orbit/zoom/move/jump and resize/change display density: the bar must remain anchored without HUD-throttle stepping. Check offscreen/behind-camera hiding, continued visibility and target/focus clicks through pillars, damage/defeat/reset, no camera gestures or context menus on bar clicks, and no target from a dragged left press. World-model hover and Pulse must still respect pillar occlusion/line of sight. Toggle V/Shift+V, remap them, verify typing/menu suppression, independent settings, profile switch/reload/export/import and preservation of custom V bindings. Friendly appearance awaits a real teammate roster. Human rendered review of the through-pillar correction is pending, separately from the earlier approval.

**Revised UI review status (2026-11-08): verified and approved by the user**, including all earlier pending UI revisions. Earlier failed feedback remains implementation history, not the current acceptance status. See [human review record](UI-REVIEW.md) and [implementation verification](../planning/REACT-UI-VERIFICATION.md). Browser/device and individual console results were not supplied.

Record browser/device, scenarios, console errors, and actual review status in the PR when known. Unit tests and a successful build do not establish DOM wiring, pointer-lock permissions, GPU rendering, ray picking, or browser compatibility. Current human UI approval is recorded above; future rendering/input changes still require human review.

## Optional Playwright diagnostics

The existing suite is retained for a person to invoke explicitly when useful; agents and CI should not run it automatically:

```sh
npm ci
npx playwright install --with-deps chromium
npm run test:browser
```

This command builds on an existing web build and starts its own static server. The default interaction viewport is 800×600 at normal density; the diagnostic scenario retains 1024×768. Use `ARENA_TEST_VIEWPORT=1024x768` for all scenarios at desktop dimensions. General control scenarios force the procedural character; character scenarios cover the optional asset separately. Artifacts and per-test timings stay in ignored `test-results/` and are not uploaded by CI. Keep one worker and tracing off for software-GPU diagnostics; use `ARENA_TRACE=1` only when a trace is useful. These runs are not hardware performance benchmarks.

On Linux, Playwright may require system libraries and sudo for `--with-deps`. Browser installation is optional for the normal unit-test loop. To opt into Firefox diagnostics, install its browser/runtime dependencies, then use `ARENA_TEST_FIREFOX=1 npm run test:browser -- --project=firefox`.

## Inspect and measure

**Frame-pacing correction (human review pending):** the overlay distinguishes **browser FPS** from **engine Hz**, with engine P99/Max and a cyan graph (browser intervals remain gold). The previous browser-only counter could show 240 FPS while the game actually updated around 60–100 Hz. The WASM client now keeps continuous Winit updates across ordinary canvas/HUD focus changes; native defaults, 30 Hz simulation, keyboard ownership and blur/hidden-tab input cancellation are unchanged. Engine Hz counts published engine frames, not GPU presentations. Reset samples clears both histories. Rebuild the complete client and compare movement/targeting in the production build. See [ADR 0027](decisions/0027-browser-focus-frame-pacing.md) for observed evidence and limits.

With `?debug=1`, the default diagnostics panel is now a compact **FPS/frame-time overlay**, not the large JSON dump. It displays FPS averaged over the last second, P50/P95/**P99**, **Max**, and the number of intervals **over 33.3 ms** within the last 3600 samples. **Reset samples** starts a fresh measurement without resetting combat. The six-second graph preserves peaks in 50 ms buckets, with guides at 16.7/33.3 ms and a fixed 100 ms ceiling (larger spikes remain visible in Max). At very high FPS the sample capacity can retain less than six seconds. Overlay statistics refresh at most four times per second; hidden-tab gaps and startup frames are excluded. **Show engine snapshot** explicitly expands the old JSON view; leave it collapsed when investigating stutters. Escape → Toggle diagnostics hides/shows the panel on debug URLs. Browser intervals cannot identify CPU/GPU/simulation causes on their own; human rendered/performance review is pending. See [ADR 0026](decisions/0026-frame-timing-overlay.md).

Open `/?debug=1` for the diagnostics panel and read-only `window.arenaDebug.state` and `.metrics`; toggle it through Menu → Toggle diagnostics. State includes rules tick, integer positions, selection, cooldown, feedback and camera. `firstStateMs` measures initialization to the first snapshot, **not** the first GPU frame or a network benchmark. Frame percentiles are requestAnimationFrame intervals after warmup, not isolated GPU time. Optional diagnostics save browser version and transfer sizes.

The JavaScript shell-to-engine adapter exists to support this practice scene. No debug command injection or remote control endpoint is exposed. Remove or gate the bridge when designing public competitive sessions.

For an incremental build timing, change a small client detail and rerun the build script; it prints wall time and module size. Record the profile, machine, dependency cache, and what changed. Fresh dependency compilation, warmed no-change builds and real incremental edits are different measurements. Shipping bindings strip WASM debug names; use `ARENA_DEBUG_SYMBOLS=1 ./scripts/build-web.sh` to retain them for readable crash stacks at the cost of download size.

The mouse adapter tracks `mousedown`/`mouseup` because `pointerdown` fires only on the first button press in a chord ([MDN](https://developer.mozilla.org/en-US/docs/Web/API/Element/mousedown_event)). Cursor capture uses the [Pointer Lock API](https://developer.mozilla.org/en-US/docs/Web/API/Pointer_Lock_API); zero-button recentering events are ignored instead of being treated as drags.

## Stylized surface textures (2026-11-14)

The arena floor and stand tiers use hand-painted CC0 flagstones; the pillars, pit wall and trim use painted stone bricks; the outer wall, benches and doors, portcullis gates and gold trim use further CC0 stone, wood and metal sets, all tinted in code (see [ADR 0031](decisions/0031-colosseum-venue-and-larger-arena.md)). Procedural characters get original painted shading generated in code: warm tops with cool undersides, cloth hems, leather boots and belt, and hair strands. Rebuild with `./scripts/build-web.sh`; it copies `assets/textures/*.png` into the build. Surfaces show their flat colours until each texture loads, and keep them if loading fails. The former floor grid seams are removed.

**Human rendered review is pending.** Check floor readability and distance shimmer, brick scale and seams, the load-time switch, character/nameplate/spell contrast against the ground, the painted character shading in motion, Low/High, MSAA on/off and console errors. To change or add textures, see [free texture sources](research/free-texture-sources.md) and [ADR 0030](decisions/0030-stylized-surface-textures.md).

## Pulse lightning (2026-11-15)

A successful Pulse now throws five thin, forking blue lightning strands from the caster's raised left hand (the `middle_01_l` knuckles of a Quaternius kit, or the procedural model's left hand) to the target's chest, replacing the single gold bar from the stomach. The strands bow apart so they read separately, sprout short branches, grow to the target over 0.05 s, re-fork 26 times a second and fade out over 0.42 s, with soft flares at the hand and the impact. It is one additive, unlit mesh rebuilt in world space each frame after transform propagation, so it tracks the animated hand. Its HDR colour feeds the existing bloom. Presentation only: the rules still resolve the hit instantly, and the client never reports anything back.

**Human rendered review is pending.** Check that the strands leave the hand during the cast at every orbit angle and when running, that they hit the target and do not pass through it, how readable they are against the floor and sky, Bloom off/on, MSAA off/on, any first-cast hitch after loading, close/inside-player camera views, kit swaps and the procedural fallback, and console errors. Automated tests cover geometry, timing, hand selection and visibility, not appearance.

## Character asset

### Character customization (2026-11-14)

After loading, a start screen shows your saved character with **Enter the
arena** and **Customize character**; customizing is optional. The customizer
(also Menu → **Customize character** during play) has Body / Head / Outfit
tabs: body type, ancestry (elf/orc only when the races pack is installed
locally), skin tone, underwear/complexion, build (races), hairstyle, hair
colour, eyebrows, facial hair, horns, eyes, each outfit slot with Ranger and
Peasant colours, plus Randomize and Reset. Drag the character to turn it;
scroll or use Full body / Face to zoom. Choices save on the device at once.
Before the match, **Save** returns to the start screen and **Enter the arena**
starts a fresh round; during play, Done or Escape returns. The combat HUD is
hidden meanwhile and movement is ignored. Rebuild with `scripts/build-web.sh`
after changing Quaternius sources. See
[ADR 0040](decisions/0040-character-customization-kits.md).

Manual regression checklist (approved by the user on 2026-11-21; see [review record](UI-REVIEW.md)): every option visibly changes the model;
no skin gaps or clipping at hems for mixed outfit pieces; cross-body
hairstyles sit on the head; switching body type/ancestry swaps without a flash
of every part; face view framing in the starting room; HUD returns on close; start screen
appears after loading and both of its paths start a fresh round.

### Quaternius default female (2026-11-14)

This supersedes the historical default-off/BaseFemale notes below. A complete
`./scripts/build-web.sh` now uses the CC0 Quaternius female full-body human base
and selected UAL1 Standard clips from `assets/quaternius` by default, generating
`characters/quaternius-female.glb` with embedded textures. The female now has
bun hair, source eyes/brows and Ranger armor (torso/belts, arms/bracers,
pauldrons, leggings and boots), without the hood. Covered base-body triangles
are excluded to prevent outfit clipping. Other practice units remain procedural.

Idle, forward jogging, airborne posing and successful Pulse spell presentation
use the source library. The actual included **`Jump_Start` → `Jump_Loop` →
`Jump_Land`** clips provide takeoff, flight and recovery, without delaying the
rule-owned jump or locking movement/casting during landing.
The complete takeoff now plays over the first 70% of flight and hands over on
the airborne loop's first pose. A standing landing plays the 0.80 second
recovery. Landing while moving, or starting to move during recovery, goes
straight into the gait ([ADR 0038](decisions/0038-matched-locomotion-and-jump-transitions.md)).
**Left/right use the shared universal lateral run clips**, re-authored
2026-11-14 to the jog's cadence and footfall frames so planted feet keep up
with 7.2 m/s. Diagonals (W/S with A/D) always play the strafe, and
switching keeps the step phase. **Backward still reverses the jog cycle**;
the downloaded free Standard libraries lack a dedicated backward jog. Jump height and all
movement/combat rules remain simulation-owned.

The [110 KB shared strafe library](../assets/animations/README.md) contains no
character, outfit or race meshes/textures. Both directions passed numerical
retargeting checks on all ten supplied humanoid base/race/outfit rigs. Numeric
and stick-figure checks are not a browser pass. Review A/D at full speed,
diagonals, rapid forward↔strafe switching, stopping, steering, standing/running
jumps and landing into a run in the actual game.

Dark foot discs and the gold world target ring are removed. Selection feedback
remains in HUD frames/nameplates.

**Entry-hitch mitigation:** character preparation now caps color images at
1024px and normal/detail maps at 512px, shares duplicate textures and drops
unused assets. The dressed GLB fell from 57.7 MB to 9.2 MB. This is measured
payload reduction, not a verified browser frame-time improvement; review a
fresh entry, texture quality and console errors on the intended device.
The loading screen now stays up while the scene renders behind it. The camera
sweeps around the player and the Pulse lightning is drawn once, until GPU uploads
and shader compilation settle; warmup gives up after 15 s
([ADR 0039](decisions/0039-render-warmup-behind-loading-screen.md)).
**The player is temporarily unequipped:** no weapon/effect models are requested
or spawned. Existing weapon settings are retained but cannot equip anything
until the engine-side flag is re-enabled. See [ADR 0035](decisions/0035-character-startup-budget.md).

`ARENA_RIGGED_CHARACTER=0 ./scripts/build-web.sh` forces procedural presentation.
Missing Quaternius sources also use the procedural fallback; installed malformed
sources fail preparation. Only an explicit `ARENA_RIGGED_CHARACTER=1` permits
the historical BaseFemale/BaseMale fallback sources if Quaternius is absent.
Their publication licensing remains unconfirmed. The normal build does not
silently ship them.

**Human browser review is pending:** check source-base appearance, deformation,
scale/facing/feet, all WASD directions and transitions, jump/landing, Pulse
success/rejection, both weapons' clearance, reset/tab resume and load failure.
Check hair/face visibility, neck/hand skin seams, armor clipping, repeated jumps
and movement/casts during landing recovery.
The real-loader headless test and static HTTP checks are not a rendered pass.
See [inventory and license distinctions](../assets/quaternius/README.md) and
[ADR 0033](decisions/0033-quaternius-default-female.md).

### Rounded procedural model and strafe refinement (2026-11-08)

The procedural fallback and practice units now use an oval torso, rounded tapered limbs, overlapping shoulder/elbow/knee/wrist/ankle shapes and rounded hands with thumbs. Chest/head movement, relaxed shoulders and variable elbow flexion add expression to breathing and locomotion. Strafing uses alternating outward steps that close back to each leg's resting lane, rather than swinging across the other leg. This remains a procedural gait, not foot-planting IK; some ground sliding is possible. The optional BaseMale model/clips are unchanged.

Rebuild with `./scripts/build-web.sh` and reload; a frontend-only build cannot install these changes. **Human rendered review is pending for this refinement**, independently of earlier approvals. Check close front/side/rear silhouettes and joint seams, hands, Low/High detail, A/D and all diagonals/reversals, W/S, stopping against pillars, idle transitions, standing/moving jumps and landing/reset/tab resume. Check both back weapons for clearance, picking/hover and optional-model success/fallback. See [ADR 0028](decisions/0028-rounded-procedural-motion.md).

### BaseFemale player (2026-11-14)

**Rigged players are currently disabled.** By default the build ignores installed character sources and the player uses the procedural model. Run `ARENA_RIGGED_CHARACTER=1 ./scripts/build-web.sh` to use the behavior described in this section and the next. The procedural body was also slimmed on 2026-11-14 (narrower torso, hips and limbs; same height and weapon mounts). Human rendered review is pending; see the [ADR 0028 update](decisions/0028-rounded-procedural-motion.md#update--2026-11-14-slimmer-build).

When enabled, the build prefers **`assets/BaseFemale.glb`** for the player and falls back to `assets/BaseMale.glb`, then to the procedural character. Only the chosen model is generated, as `web/public/assets/characters/base-female.glb` or `base-male.glb`, and `character.json` names it (`{"available": true, "model": "base-female"}`). Rebuild with `./scripts/build-web.sh` and reload; a frontend-only build cannot install the change. The opponent stays procedural, and there is no in-game body selector. BaseFemale is normalized to the shared 1.9 m height and uses her own Jump/Cast timing; the locomotion, jump, Pulse and fallback behavior described below applies to her as well.

**Human rendered/browser review is pending.** Check her scale, facing and feet, back weapon clearance against hair/ponytail/shoulders for both weapons, all movement directions, jumps and landing, Pulse success/rejection, reset/tab resume, loading/failure fallback and console errors. See [ADR 0029](decisions/0029-base-female-player-character.md).

### BaseMale player and animations

Place the authored model at **`assets/BaseMale.glb`** and run `./scripts/build-web.sh`. The build generates a character-only `web/public/assets/characters/base-male.glb` with the rig and all eight clips; the source is unchanged. The player switches from its procedural fallback only after the skinned scene and animation player are ready. Missing sources use the fallback without a model request; load errors retain it with a notice. The opponent stays procedural. The former optional unrigged stock model is no longer selected by the build.

W/S and A/D select forward/backward and left/right strafe from **actual movement**, without turning the character. Idle and locomotion loop with short crossfades. Space seeks the authored Jump limb poses against the simulation flight; exported vertical root animation is removed so jump height is not doubled. Successful Pulse plays a shortened cosmetic Cast release; it does not introduce a cast time or delay damage. Invalid/recharging/out-of-range attempts do not animate a cast. Jump takes priority in flight. Attack remains in the asset for future melee presentation; no new ability/key is added. Reset and tab-resume gaps clear stale playback. The rig replaces carrier bob, while cosmetic back weapons retain their existing separate mounts; future armor/hand attachment is not implemented.

The model has separate outfit parts and a 45-bone FK skeleton, not a facial/IK rig or complete unclothed body. Movement rate is bounded rather than foot-locked, and full-body Cast can slide during movement. Review animation speed, feet, clipping and weapon fit in-browser. With `?debug=1`, `window.arenaDebug.state.character` reports status, spawned mesh count and `animated: true` for both the ready rig and procedural fallback.

**Human rendered/browser review is pending.** Manually check all directions/diagonals, blocked movement, free-look/turning, idle transitions, standing/running jumps, landing/reset/tab resume, Pulse acceptance/rejection, close views, both weapons/all palettes, loading/failure fallback and console errors. See [ADR 0021](decisions/0021-base-male-skeletal-character.md). Historical unrigged-model descriptions below are superseded by this section.

### Imported back weapons

Staff mount follow-up: Aetherthorn's shaft now crosses the center of the player's back at torso height, retaining its slight diagonal, back depth and floor clearance rather than sitting beside the body. Its spill light follows the transformed crystal position. Human rendered review of the centered mount is pending.

Open **Settings → Weapons** (O, then Weapons) to switch between **Verdant Eclipse paired glaives** and **Aetherthorn crystal staff**, each in **purple, yellow, red, blue, green or cyan**. Changes apply immediately and save on this device separately from interface profiles. This replaces random startup colors; defaults are purple glaives. Resetting practice does not change the selection. Both are cosmetic back weapons, not new abilities or combat stats; hand gripping and weapon attack animations are not implemented. Metal/grip finishes and source GLBs remain unchanged. Fallback effects and spill light match the palette. Storage failures show a warning but keep the session selection active.

The full build also extracts `assets/Aetherthorn.glb` into a staff-only runtime scene, excluding the unrelated cube. Its bottom display plate is absent. A procedural shaft/crystal remains visible until the imported staff is ready or if loading fails. Rebuild with `./scripts/build-web.sh` after changing either source GLB. Human rendered review is pending: inspect all twelve combinations, rapid switches during loading, hidden old weapons/lights, rear/side/front clearance, movement/jumping/reset, reload persistence, optional character/fallback and console errors. See [ADR 0018](decisions/0018-weapon-appearance-selector.md).

Screenshot follow-up: missing visibility components on both mount types broke inherited hiding and let old cached weapons remain visible. Those mounts now explicitly inherit visibility; regression tests check actual imported descendant visibility rather than only selector groups. Review rapid switching, late loading and hidden old spill lights in-browser. The bloom/material/lighting revision above supersedes historical no-bloom descriptions below.

Latest lighting feedback: the user confirmed visible staff glow but found the scene too contrasty. The next pass lifts the near-black background to muted slate, increases ambient light and balances key/fill lighting to about 2:1 instead of 5:1. Glow, exposure and tone mapping remain unchanged. Human review of this softer lighting is pending; the glow confirmation is not a full browser acceptance pass.

Mount-angle refinement (2026-11-07): user browser feedback showed the imported pair spreading too horizontally. Both grip axes now sit 20° off vertical rather than 45°, retaining the centered intersection, scale and back depth. Human browser review of the more upright stance is pending.

The player now uses two copies of the original **Verdant Eclipse** Blender model at the existing crossed back mounts. Run the complete `./scripts/build-web.sh` after changing `assets/Verdant Eclipse.glb`; a frontend-only build does not regenerate this asset. The build creates a weapon-only scene, excluding the cube scene and studio backdrop without changing the saved source. Procedural glaives remain visible while loading or if loading fails, and are hidden separately when each imported instance has mesh descendants. The GLB retains its emerald emissive materials; Blender's compositor bloom does not export, and no game bloom is added. Existing green light spill remains. See [provenance and publication status](../assets/README.md) and [ADR 0017](decisions/0017-imported-glaives.md).

After a complete build, review both blades/grips/guards from rear, side and front, close back fit, glow, movement/jumping/reset, Low/High detail, optional character/fallback and console errors. **Human browser review is pending.** Earlier procedural weapon descriptions below describe the retained fallback, not the ready imported model.

The default public models are stylized procedural humanoids with separate arms/hands, legs/boots, ears, nose, mouth, eyebrows, layered eyes and hair. Player and opponent use different appearance presets. Colors, height/build proportions and hairstyle live in internal presentation data, ready for a later customization UI; no character editor or saved appearance settings exist yet. Procedural shoulder/elbow/hip/knee pivots now animate opposed limbs from actual movement, including backward and lateral steps, with subtle body bob/idle breathing and a tucked-knee jumping pose that blends out on landing. Internal settings expose animation tuning; no animation editor, foot-planting IK or attack clips exist. Cosmetic back weapons share body bob; ground markers and collision do not. The optional imported model still takes precedence when loaded; its unrigged limbs remain static, with only shared body motion. Debug `character.animated` is true for procedural limb animation and false for a ready imported model. After a complete rebuild, review front/side/back silhouettes, facial/hair readability, grounded feet, weapon fit, Low/High detail, backward/strafe/diagonal motion, stopping against pillars, idle transitions, standing/running jumps, landing/reset/tab resume and targeting/hover. **Human rendered review is pending.**

The player carries two original center-gripped, double-ended storm glaives crossed diagonally in an X on the back. Each has opposing hooked blades, green emissive cutting edges, additive green halo ribbons, and animated mint-green electrical arcs; a small shadowless light supplies green spill. The refined blades have narrower profiles, thinner bevels and needle-like tapered tips. Both grips share a close-fitting back plane and intentionally intersect at the X rather than stacking in depth. This replaces the initial purple sword presentation. They are cosmetic children of the fighter, shared by the procedural fallback and optional model; movement, turning, jumping and reset carry them without changing collision or combat. Electricity alternates cached jagged meshes using presentation time, without changing simulation ticks or allocating geometry each frame. No bloom/postprocessing dependency is added. After a complete client rebuild, manually inspect the center grips, four blade ends, X silhouette, electrical flicker/halo, close back fit and intentional intersection from rear/side/front views on both character variants, including movement/jumping and graphics settings. **Human rendered review of the thinner, flush-mounted refinement is pending.**

Latest glaive profile revision (2026-11-07): broaden the flat blade faces, superseding the earlier narrow-face refinement above. Maximum face width is now 0.35 m in authored mesh space (previously 0.21 m), while the 0.05 m maximum thickness, length, pointed tips, grips and crossed mounts stay unchanged. The glowing cutting-edge band is limited to 0.018 m rather than scaling with face width; halo/electrical traces follow the wider outline. Review broad-face readability from behind, thin edge-on views, tip sharpness, glow and character clearance while moving/jumping. **Human rendered review of this wider-face revision is pending.**

Wider-face verification: `./scripts/check.sh` (including 31 client tests), WASM Clippy with warnings denied and the complete web build passed locally on 2026-11-07. The build used procedural characters. No browser was launched; no rendered pass is claimed.

Crescent/angular refinement (2026-11-07): each of the four blade ends now has a real open crescent recess along its inner rim and three sharp outer teeth with inward-returning notches. The section-by-section mesh follows the concave outline instead of filling the cutout with a flat face. Broad faces, thin depth, pointed ends, center grips and crossed mounting are retained; green edge bands, halos and electrical paths follow the new outline. Cached electrical ribbons sample every profile corner. Review the recess as negative space against both floor and sky, the tooth/notch silhouettes, edge glow and weapon/body clearance while moving and jumping. **Human rendered review is pending.**

Crescent/angular verification: `./scripts/check.sh` (including 32 client tests), WASM Clippy with warnings denied and the complete web build passed locally on 2026-11-07. Geometry tests check that the crescent contains no blade triangles, all three outer teeth exist, and the broad face/thin edge/sharp tip constraints remain intact. No browser was launched.

Bevy's glTF loader and `gltf_animation` support load the optional local BaseMale player and rig. The build generates an availability manifest; the browser requests the model only when present. Failure retains the procedural humanoid with a visible notice. Source geometry/materials and the saved GLB are preserved; only the generated runtime copy is adapted. Binary redistribution terms remain unconfirmed.

## Historical browser test timing

The previous E2E runtime investigation and rejected optimization experiments are recorded in [browser-test-runtime.md](research/browser-test-runtime.md). Those rendered timings predate the unit-first policy and do not describe the current CI gates.

## Model-focus approval — 2026-11-08

The user approved the completed model right-click focus change, including shared model-click picking. This supersedes pending model-focus review statements above. See [review record](UI-REVIEW.md#subsequent-changes); browser/device and individual console results were not supplied.
