import test from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';

test('the live player cast bar never blocks pointer input over the arena', () => {
  const css = readFileSync(new URL('../../web/src/features/cast-bar/cast-bar.css', import.meta.url), 'utf8');
  assert.match(css, /\.placed:has\(\.player-cast-bar-live\)\s*\{\s*pointer-events:\s*none;/);
});
