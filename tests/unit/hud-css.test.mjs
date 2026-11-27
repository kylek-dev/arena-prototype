import test from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync, readdirSync, statSync } from 'node:fs';

test('the live player cast bar never blocks pointer input over the arena', () => {
  const css = readFileSync(new URL('../../web/src/features/cast-bar/cast-bar.css', import.meta.url), 'utf8');
  assert.match(css, /\.placed:has\(\.player-cast-bar-live\)\s*\{\s*pointer-events:\s*none;/);
});

test('dock windows share one size and open at the left middle, side by side', () => {
  // Declarations only: comments may explain what must not be there.
  const css = readFileSync(new URL('../../web/src/ui/dock.css', import.meta.url), 'utf8').replace(
    /\/\*[\s\S]*?\*\//g,
    '',
  );
  const rule = (selector) => css.match(new RegExp(`${selector.replace('.', '\\.')}\\s*\\{([^}]*)\\}`))[1];
  const dock = rule('.window-dock');
  assert.match(dock, /top:\s*0/);
  assert.match(dock, /bottom:\s*0/);
  assert.match(dock, /left:\s*16px/);
  assert.match(dock, /display:\s*flex/);
  assert.match(dock, /align-items:\s*center/);
  // No transform on the dock or its windows: a transform makes them the
  // containing block of the fixed ⋮ menus, which then open off screen.
  assert.doesNotMatch(dock, /transform:/);
  assert.doesNotMatch(rule('.dock-window'), /transform:/);
  assert.doesNotMatch(rule('.dock-window-body'), /transform:/);
  const window = rule('.dock-window');
  assert.match(window, /width:\s*320px/);
  assert.match(window, /height:\s*min\(460px/);
  // Several open windows shrink to fit instead of overlapping.
  assert.match(window, /flex:\s*0 1 320px/);
});

test('the Queue tab stacks full-width bracket buttons', () => {
  const css = readFileSync(new URL('../../web/src/ui/dock.css', import.meta.url), 'utf8');
  assert.match(css, /\.play-queue-modes\s*\{[^}]*display:\s*grid/);
  assert.match(css, /\.play-queue-modes > \*\s*\{[^}]*width:\s*100%/);
  // The Play window uses Radix tabs; no hand-made tab styles remain.
  assert.doesNotMatch(css, /\.dock-tab/);
});

test('the active Play tab carries a visible highlight', () => {
  const css = readFileSync(new URL('../../web/src/ui/dock.css', import.meta.url), 'utf8');
  assert.match(
    css,
    /\.play-tabs \.rt-TabsTrigger\[data-state="active"\] \.rt-BaseTabListTriggerInner\s*\{[^}]*background-color:/,
  );
});

test('the notification panel stacks its cards from the top center, without a transform', () => {
  const css = readFileSync(new URL('../../web/src/features/center/center.css', import.meta.url), 'utf8')
    .replace(/\/\*[\s\S]*?\*\//g, '');
  const panel = css.match(/\.center-panel\s*\{([^}]*)\}/)[1];
  assert.match(panel, /justify-content:\s*flex-start/);
  assert.match(panel, /align-items:\s*center/);
  assert.doesNotMatch(panel, /transform:/);
});

test('no stylesheet styles a bare chat channel class, which every chat line carries', () => {
  // Chat lines are `chat-line <channel>`: a rule for a bare `.hub` (or any
  // other channel name) elsewhere restyles every line of that channel, as
  // the hub window's `.hub { display: grid }` once did.
  
  const channels = ['public', 'match', 'team', 'spectator', 'whisper', 'hub', 'system'];
  const root = new URL('../../web/src/', import.meta.url);
  const files = [];
  const walk = (dir) => {
    for (const name of readdirSync(dir)) {
      const path = new URL(name, dir);
      if (statSync(path).isDirectory()) walk(new URL(`${name}/`, dir));
      else if (name.endsWith('.css')) files.push(path);
    }
  };
  walk(root);
  for (const file of files) {
    const css = readFileSync(file, 'utf8').replace(/\/\*[\s\S]*?\*\//g, '');
    for (const channel of channels) {
      // `.hub` not preceded by another class on the same compound selector
      // (`.chat-line.hub` is fine) and not part of a longer name (`.hub-panel`).
      const bare = new RegExp(`(^|[\\s,>+~])\\.${channel}(?![\\w-])`, 'm');
      assert.doesNotMatch(css, bare, `${file.pathname} styles a bare .${channel}`);
    }
  }
});

