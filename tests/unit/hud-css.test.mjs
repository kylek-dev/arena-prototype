import test from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';

test('the live player cast bar never blocks pointer input over the arena', () => {
  const css = readFileSync(new URL('../../web/src/features/cast-bar/cast-bar.css', import.meta.url), 'utf8');
  assert.match(css, /\.placed:has\(\.player-cast-bar-live\)\s*\{\s*pointer-events:\s*none;/);
});

test('dock windows share one size and open at the left middle, side by side', () => {
  const css = readFileSync(new URL('../../web/src/ui/dock.css', import.meta.url), 'utf8');
  const rule = (selector) => css.match(new RegExp(`${selector.replace('.', '\\.')}\\s*\\{([^}]*)\\}`))[1];
  const dock = rule('.window-dock');
  assert.match(dock, /top:\s*50%/);
  assert.match(dock, /left:\s*16px/);
  assert.match(dock, /display:\s*flex/);
  assert.match(dock, /translateY\(-50%\)/);
  const window = rule('.dock-window');
  assert.match(window, /width:\s*320px/);
  assert.match(window, /height:\s*min\(460px/);
  // Several open windows shrink to fit instead of overlapping.
  assert.match(window, /flex:\s*0 1 320px/);
});

test('the Queue tab stacks full-width bracket buttons, and tabs carry no top highlight', () => {
  const css = readFileSync(new URL('../../web/src/ui/dock.css', import.meta.url), 'utf8');
  assert.match(css, /\.play-queue-modes\s*\{[^}]*display:\s*grid/);
  assert.match(css, /\.play-queue-modes > \*\s*\{[^}]*width:\s*100%/);
  const selected = css.match(/\.dock-tab\[aria-selected="true"\]\s*\{([^}]*)\}/)[1];
  assert.doesNotMatch(selected, /box-shadow|border-top/);
});
