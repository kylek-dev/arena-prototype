import test from 'node:test';
import assert from 'node:assert/strict';
import { readdirSync, readFileSync, statSync } from 'node:fs';
import { join } from 'node:path';

const root = new URL('../../web/src/', import.meta.url).pathname;
const sources = (dir) =>
  readdirSync(dir).flatMap((name) => {
    const path = join(dir, name);
    return statSync(path).isDirectory() ? sources(path) : /\.tsx$/.test(name) ? [path] : [];
  });

// Radix `ghost` buttons have no border or fill and use negative margins, so
// in a row they read as plain text and overlap their neighbours (playtest
// 2026-11-22). Only icon-only close/back controls in a header corner may use
// it: their opening tag must carry a "Close …" or "Back" accessible name.
test('ghost buttons are reserved for icon close/back controls', () => {
  const offenders = [];
  let ghosts = 0;
  for (const file of sources(root)) {
    const text = readFileSync(file, 'utf8');
    for (const match of text.matchAll(/variant=\{?\s*["'`]ghost["'`]\s*\}?/g)) {
      ghosts += 1;
      // The opening tag runs from the nearest preceding `<Button` to the next
      // line that closes it; arrow functions inside props do not end it.
      const start = text.lastIndexOf('<Button', match.index);
      const close = text.slice(match.index).search(/\n\s*>|>\s*\n|\/>/);
      const tag = text.slice(start, match.index + (close < 0 ? 400 : close + 2));
      if (start < 0 || !/aria-label=[^\n]*(Close|"Back")/.test(tag)) {
        const line = text.slice(0, match.index).split('\n').length;
        offenders.push(`${file.slice(root.length)}:${line}`);
      }
    }
  }
  assert.deepEqual(offenders, []);
  assert.ok(ghosts >= 1, 'the scan found the known close buttons');
});

test('the shared action button style keeps a minimum hit size', () => {
  const css = readFileSync(join(root, 'styles/base.css'), 'utf8');
  assert.match(css, /\.action-button\s*\{[^}]*min-height:\s*28px;[^}]*min-width:\s*28px;/s);
});

test('the pressed fill for plain buttons never overrides component backgrounds', () => {
  const css = readFileSync(join(root, 'styles/base.css'), 'utf8');
  assert.match(css, /:where\(button:not\(\.rt-Button\):active:not\(:disabled\)\)\s*\{/);
  assert.doesNotMatch(css, /^button:not\(\.rt-Button\):active/m);
});
