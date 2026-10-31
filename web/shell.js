// Browser-only input/HUD adapter. Combat authority lives in arena-sim.
const canvas = document.querySelector('#arena');
const element = (id) => document.getElementById(id);
const held = new Set();
let attack = false, select = false, clear = false, reset = false;
let orbitX = 0, orbitY = 0, zoom = 0, click = null, dragging = false;
let state = null;
let lastHud = 0;
let lastFeedback = '';
const started = performance.now();
const metrics = { firstStateMs: null, frameMs: [], renderer: 'Bevy / WebGL2', errors: [] };
const debugEnabled = new URLSearchParams(location.search).has('debug');
element('debug').hidden = !debugEnabled;
element('debug-toggle').setAttribute('aria-expanded', String(debugEnabled));
const keys = new Set(['KeyW', 'KeyA', 'KeyS', 'KeyD', 'Digit1', 'Tab', 'Escape', 'KeyR']);
const gameplayFocused = () => document.activeElement === canvas;
const focusGame = () => canvas.focus({ preventScroll: true });
function releaseInput() { held.clear(); dragging = false; attack = false; select = false; clear = false; orbitX = 0; orbitY = 0; zoom = 0; click = null; }
window.addEventListener('blur', releaseInput);
canvas.addEventListener('blur', releaseInput);
document.addEventListener('visibilitychange', () => { if (document.hidden) releaseInput(); });
window.addEventListener('keydown', (event) => {
  if (!gameplayFocused() || !keys.has(event.code) || event.ctrlKey || event.metaKey || event.altKey) return;
  // Keep an ordinary keyboard path out of the game canvas.
  if (event.code === 'Tab' && event.shiftKey) {
    event.preventDefault();
    element('debug-toggle').focus();
    return;
  }
  event.preventDefault();
  held.add(event.code);
  if (event.repeat) return;
  if (event.code === 'Digit1') attack = true;
  if (event.code === 'Tab') select = true;
  if (event.code === 'Escape') clear = true;
  if (event.code === 'KeyR') reset = true;
});
window.addEventListener('keyup', (event) => { held.delete(event.code); });
canvas.addEventListener('contextmenu', (event) => event.preventDefault());
canvas.addEventListener('pointerdown', (event) => {
  focusGame();
  if (event.button === 2) { dragging = true; canvas.setPointerCapture(event.pointerId); }
  if (event.button === 0) {
    const rect = canvas.getBoundingClientRect();
    click = [event.clientX - rect.left, event.clientY - rect.top];
  }
});
canvas.addEventListener('pointermove', (event) => { if (dragging) { orbitX += event.movementX; orbitY += event.movementY; } });
canvas.addEventListener('pointerup', (event) => { if (event.button === 2) dragging = false; });
canvas.addEventListener('lostpointercapture', () => { dragging = false; });
canvas.addEventListener('wheel', (event) => { event.preventDefault(); zoom += event.deltaY; }, { passive: false });
element('attack').addEventListener('click', () => { attack = true; focusGame(); });
element('reset').addEventListener('click', () => { releaseInput(); reset = true; focusGame(); });
element('debug-toggle').addEventListener('click', () => {
  element('debug').hidden = !element('debug').hidden;
  element('debug-toggle').setAttribute('aria-expanded', String(!element('debug').hidden));
});
let lastFrame = null;
function sampleFrames(now) {
  if (state?.frames > 30 && !document.hidden && lastFrame !== null) {
    metrics.frameMs.push(now - lastFrame);
    if (metrics.frameMs.length > 3600) metrics.frameMs.shift();
  }
  lastFrame = document.hidden ? null : now;
  requestAnimationFrame(sampleFrames);
}
requestAnimationFrame(sampleFrames);
function showError(message) {
  metrics.errors.push(String(message));
  element('loading').hidden = false;
  element('loading-message').textContent = `${message} Try a current desktop browser with WebGL2 enabled. Open the developer console for details.`;
  document.querySelector('.loading-line').style.animation = 'none';
}
window.addEventListener('error', (event) => showError(event.message));
window.addEventListener('unhandledrejection', (event) => showError(event.reason?.message ?? event.reason));
window.arenaShell = {
  readInput() {
    const input = [Number(held.has('KeyD')) - Number(held.has('KeyA')), Number(held.has('KeyS')) - Number(held.has('KeyW')), Number(attack), Number(select), Number(clear), Number(reset), orbitX, orbitY, zoom, click?.[0] ?? -1, click?.[1] ?? -1];
    attack = select = clear = reset = false;
    orbitX = orbitY = zoom = 0; click = null;
    return input;
  },
  onState(next) {
    state = next;
    if (metrics.firstStateMs === null) metrics.firstStateMs = performance.now() - started;
    if (next.frames > 10 && !metrics.errors.length && !element('loading').hidden) {
      element('loading').hidden = true;
      if (document.activeElement === document.body) focusGame();
    }
    const now = performance.now();
    if (now - lastHud < 80) return;
    lastHud = now;
    element('player-health-value').textContent = `${next.player.health} / 100`;
    element('target-health-value').textContent = `${next.target.health} / 100`;
    element('player-health').style.width = `${next.player.health}%`;
    element('target-health').style.width = `${next.target.health}%`;
    element('target-detail').textContent = next.selected ? `Selected · ${next.distance.toFixed(1)} m away` : 'No target · press Tab or click Sentinel';
    element('attack').disabled = !next.selected || next.won || next.cooldown > 0;
    element('cooldown').textContent = next.cooldown > 0 ? next.cooldown.toFixed(1) : '';
    element('attack-status').textContent = next.selected ? next.attackStatus : 'Select a target';
    element('round-progress').textContent = `${(100 - next.target.health) / 25} / 4 hits`;
    element('round-title').textContent = next.won ? 'Round complete · R to reset' : 'Practice round';
    if (next.feedback !== lastFeedback) {
      element('feedback').textContent = next.feedback;
      lastFeedback = next.feedback;
    }
    if (!element('debug').hidden) {
      element('debug-state').textContent = `tick       ${next.tick}\nplayer     ${next.player.x}, ${next.player.z} mm\ntarget     ${next.target.x}, ${next.target.z} mm\nselected   ${next.selected}\ndistance   ${next.distance.toFixed(2)} m\ncooldown   ${next.cooldown.toFixed(2)} s\nattack     ${next.attackStatus}\ncamera     ${next.camera.yaw.toFixed(2)} / ${next.camera.pitch.toFixed(2)}\nfirst state ${(metrics.firstStateMs / 1000).toFixed(2)} s`;
    }
  },
};
// Explicit opt-in, read-only inspection. Tests use real keyboard/pointer events.
if (debugEnabled) {
  window.arenaDebug = {
    get state() { return state ? structuredClone(state) : null; },
    get metrics() {
      const sorted = [...metrics.frameMs].sort((a, b) => a - b);
      const percentile = (p) => sorted[Math.floor((sorted.length - 1) * p)] ?? null;
      return { firstStateMs: metrics.firstStateMs, frameSamples: sorted.length, frameP50Ms: percentile(.5), frameP95Ms: percentile(.95), errors: [...metrics.errors], renderer: metrics.renderer };
    },
  };
}
try {
  // The loader owns the existing context; the engine requests WebGL2 itself.
  if (!('WebAssembly' in window)) throw new Error('WebAssembly is unavailable.');
  const { default: init } = await import('./pkg/arena_client.js');
  await init();
} catch (error) {
  console.error(error);
  showError(error.message ?? error);
}
