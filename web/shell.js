import { InputState } from './input.js';

// Browser-only input/HUD adapter. Combat authority lives in arena-sim.
const canvas = document.querySelector('#arena');
const element = (id) => document.getElementById(id);
const input = new InputState();
let state = null;
let lastHud = 0;
let lastFeedback = '';
const started = performance.now();
const metrics = { firstStateMs: null, frameMs: [], renderer: 'Bevy / WebGL2', errors: [] };
const debugEnabled = new URLSearchParams(location.search).has('debug');
element('debug').hidden = !debugEnabled;
element('debug-toggle').setAttribute('aria-expanded', String(debugEnabled));
const gameplayFocused = () => document.activeElement === canvas;
const focusGame = () => canvas.focus({ preventScroll: true });
function endMouseGesture() {
  canvas.style.cursor = '';
  if (document.pointerLockElement === canvas) document.exitPointerLock();
}
function releaseInput() {
  input.cancel();
  endMouseGesture();
}
window.addEventListener('blur', releaseInput);
canvas.addEventListener('blur', releaseInput);
document.addEventListener('visibilitychange', () => { if (document.hidden) releaseInput(); });
window.addEventListener('keydown', (event) => {
  const action = input.keyDown(event, gameplayFocused());
  if (action === 'ignore') return;
  event.preventDefault();
  if (action === 'leave') element('debug-toggle').focus();
});
window.addEventListener('keyup', (event) => input.keyUp(event.code));
canvas.addEventListener('contextmenu', (event) => event.preventDefault());
// Mouse events report each button in a chord; pointerdown only reports the
// first press. Pointer lock keeps dragging unlimited and hides the OS cursor.
canvas.addEventListener('mousedown', (event) => {
  if (event.button !== 0 && event.button !== 2) return;
  event.preventDefault();
  focusGame();
  const started = !input.dragging;
  const rect = canvas.getBoundingClientRect();
  input.mouseDown({ button: event.button, buttons: event.buttons, x: event.clientX - rect.left, y: event.clientY - rect.top });
  if (started) {
    // CSS also hides the cursor if a browser refuses pointer lock. Mouse
    // dragging still works within the page in that fallback.
    canvas.style.cursor = 'none';
    try {
      const request = canvas.requestPointerLock?.();
      request?.catch(() => {}); // Denial is a nonfatal drag fallback.
    } catch { /* Older implementations can throw instead of returning a promise. */ }
  }
});
document.addEventListener('mousemove', (event) => input.mouseMove(event));
document.addEventListener('mouseup', (event) => {
  if (input.mouseUp(event) && !input.dragging) endMouseGesture();
});
document.addEventListener('pointerlockchange', () => {
  const exitLock = input.pointerLockChanged(document.pointerLockElement === canvas);
  if (!input.dragging) canvas.style.cursor = '';
  if (exitLock) document.exitPointerLock();
});
canvas.addEventListener('wheel', (event) => { event.preventDefault(); input.wheel(event.deltaY); }, { passive: false });
element('attack').addEventListener('click', () => { input.requestAttack(); focusGame(); });
element('reset').addEventListener('click', () => { releaseInput(); input.requestReset(); focusGame(); });
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
  readInput() { return input.readInput(); },
  onState(next) {
    state = next;
    if (metrics.firstStateMs === null) metrics.firstStateMs = performance.now() - started;
    const characterReady = next.character?.status === 'ready';
    const characterFailed = next.character?.status === 'failed';
    const usingPlaceholder = next.character?.status === 'placeholder';
    if (!characterReady && !characterFailed && !usingPlaceholder && !metrics.errors.length) {
      element('loading-message').textContent = 'Loading the character model and textures…';
    }
    element('model-notice').hidden = !characterFailed;
    if (next.frames > 10 && (characterReady || characterFailed || usingPlaceholder) && !metrics.errors.length && !element('loading').hidden) {
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
  const assetResponse = await fetch(new URL('./assets/character.json', import.meta.url));
  if (!assetResponse.ok) throw new Error('Asset configuration is missing. Run scripts/build-web.sh.');
  const assetConfig = await assetResponse.json();
  window.arenaShell.hasCharacterModel = assetConfig.available === true;
  const { default: init } = await import('./pkg/arena_client.js');
  await init();
} catch (error) {
  console.error(error);
  showError(error.message ?? error);
}
