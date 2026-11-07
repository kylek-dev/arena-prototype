# ADR 0007: React combat overlay and local configuration

Date: 2026-11-07. **Status: accepted**, following explicit user approval of React and the complete implementation plan. Supersedes ADR 0002's plain-DOM HUD and ADRs 0004/0005's dependency-free default frontend gate, not simulation authority or manual E2E policy.

## Decision

UI interaction/style details were subsequently revised at the user's request by [ADR 0008](0008-gameplay-first-ui.md). The ownership, build and validation boundaries below remain in effect.

Native HTML spell dragging is subsequently superseded by the captured-pointer adapter in [ADR 0009](0009-pointer-spell-placement.md).

Use React / React DOM 19.2.0, TypeScript 5.9.3 and Vite 7.3.7 for the client-only overlay. Pin exact dependencies and commit `package-lock.json`. Use ordinary CSS, original glyphs and design tokens; no component suite, router, server, or gameplay LLM dependency.

HTML owns a stable canvas and sibling overlay root. React never reconciles the canvas. Rust owns rules, maximum health, Pulse metadata and availability. The browser adapter validates snapshots and supplies stable local practice entity identities. The positional input ABI remains unchanged. It represents one attack intent, **not** a general multi-ability protocol. Additional spells require explicit ability intents and matching Rust/JS contract changes first.

Pure domain modules implement binding conflicts, physical chords, held-action resolution, slot transactions, viewport layout, focus identity, validation and profiles. Small immutable external stores use `useSyncExternalStore`; rapidly changing engine snapshots do not enter a global context. UI dispatches commands rather than mutating combat state.

Use Radix Dialog 1.1.15 and Tooltip 1.2.8 for trapped settings/conflicts, focus restoration and tooltips. Spellbook is deliberately nonmodal so visible HUD slots remain drop targets; gameplay routing stays exclusive. Native HTML drag/drop suffices for same-page typed copy/move/swap transactions, with complete keyboard/non-drag alternatives. No generic DnD framework is needed.

Support unmodified and Shift-modified physical codes. Conservatively reject Ctrl, Alt, Meta, function keys and Shift+Tab rather than promise universal browser/OS shortcut capture. Escape cancels captures and exits editing; Shift+Tab leaves the canvas. Ordinary UI Tab navigates normally. Conflicts require Replace or Cancel, including alternate bindings. Quick binding edits primary; both entries are available in Settings.

Allow 1–4 bars, 1–12 stable-ID slots per bar, 60–160% saved scale, normalized top-left travel offsets, separate position/assignment locks, and eight local profiles. Oversized elements fit the usable viewport without changing saved scale. Schema 1 includes profiles; known schema 0 single-config imports migrate. Invalid/future storage is preserved and writes blocked until explicit recovery. Writes coalesce and flush on page hiding; export remains available when storage fails.

## Build and validation

Generate WASM/assets into ignored `web/public/{pkg,assets}`. Vite serves that directory during development and copies it to ignored `dist/`. Loader URLs use `document.baseURI`; bundles use relative base paths. Bevy retains its relative assets tree. `npm run build` never invokes Rust; `scripts/build-web.sh` builds both. Publish only `dist/` with WASM MIME type and compression.

Keep Node's independent input suite. Add Vitest 4.1.12, jsdom 27.2.0 and Testing Library for domain/DOM checks; Biome 2.3.4 enforces readable formatting and focused lint rules. Automatic frontend CI installs pinned packages, typechecks, lints, checks formatting, tests and builds the overlay without rebuilding Bevy. Existing selective native/WASM jobs remain. No browser download or rendered E2E gate is added.

## References and provenance

- [React 19.2](https://react.dev/blog/2025/10/01/react-19-2), [external-store API](https://react.dev/reference/react/useSyncExternalStore)
- [Vite 7 build and relative paths](https://v7.vite.dev/guide/build.html)
- [TypeScript 5.9](https://www.typescriptlang.org/docs/handbook/release-notes/typescript-5-9.html)
- [Radix Dialog](https://www.radix-ui.com/primitives/docs/components/dialog), [Tooltip](https://www.radix-ui.com/primitives/docs/components/tooltip)
- [Vitest 4](https://v4.vitest.dev/guide/), [Biome 2 formatter](https://biomejs.dev/formatter/)

React, Vite, Radix, Vitest, jsdom and Testing Library use MIT; TypeScript uses Apache-2.0; Biome uses MIT OR Apache-2.0. Transitive notices remain with installed packages. Optional stock asset terms are unchanged. New CSS/glyph composition and labels are original, AI-assisted project code. No proprietary game assets or UI were imported. Older Vite/Vitest patch selections were replaced after npm audit; the selected lockfile reported zero advisories during implementation, not a guarantee against future advisories.

Human browser/device review and hosted CI execution remain pending. Builds do not establish pointer-lock permissions or rendered quality.
