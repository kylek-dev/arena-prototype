import { test, expect } from '@playwright/test';
import { writeFile } from 'node:fs/promises';

async function ready(page) {
  const errors = [];
  page.on('pageerror', (error) => errors.push(error.message));
  page.on('console', (message) => { if (message.type() === 'error') errors.push(message.text()); });
  await page.goto('/?debug=1');
  await expect(page.locator('#loading')).toBeHidden();
  await expect.poll(() => page.evaluate(() => window.arenaDebug?.state?.tick ?? 0)).toBeGreaterThan(10);
  await page.locator('#arena').focus();
  return errors;
}
const state = (page) => page.evaluate(() => window.arenaDebug.state);

test('move, target, attack, finish and reset through real browser controls', async ({ page }, info) => {
  const errors = await ready(page);
  expect((await state(page)).selected).toBe(false);
  await page.keyboard.press('1');
  await expect(page.locator('#feedback')).toHaveText('Select a target first');
  await page.keyboard.press('Tab');
  await expect.poll(async () => (await state(page)).selected).toBe(true);
  await page.keyboard.press('1');
  await expect(page.locator('#feedback')).toHaveText('Out of range · move closer');
  expect((await state(page)).target.health).toBe(100);
  await page.keyboard.down('w');
  try {
    await expect.poll(async () => (await state(page)).player.z).toBeLessThan(2000);
  } finally {
    await page.keyboard.up('w');
  }
  const approached = await state(page);
  expect(approached.player.z).toBeLessThan(2500);
  expect(approached.distance).toBeLessThan(6);
  await page.keyboard.press('Escape');
  await expect.poll(async () => (await state(page)).selected).toBe(false);
  const target = (await state(page)).target;
  await page.mouse.click(target.screenX, target.screenY);
  await expect.poll(async () => (await state(page)).selected).toBe(true);
  await page.keyboard.press('1');
  await expect.poll(async () => (await state(page)).target.health).toBe(75);
  await page.keyboard.press('1');
  await expect(page.locator('#feedback')).toHaveText('Pulse is recharging');
  expect((await state(page)).target.health).toBe(75);
  for (const health of [50, 25, 0]) {
    await expect.poll(async () => (await state(page)).cooldown).toBe(0);
    await page.keyboard.press('1');
    await expect.poll(async () => (await state(page)).target.health).toBe(health);
  }
  expect((await state(page)).won).toBe(true);
  await expect(page.locator('#round-title')).toHaveText('Round complete · R to reset');
  await page.screenshot({ path: info.outputPath('round-complete.png') });
  await page.getByRole('button', { name: 'Reset R', exact: true }).click();
  await expect.poll(async () => (await state(page)).target.health).toBe(100);
  expect((await state(page)).player.z).toBe(5500);
  expect((await state(page)).won).toBe(false);
  expect((await state(page)).selected).toBe(false);
  expect(errors).toEqual([]);
});

test('camera orbit, zoom, focus loss and frame diagnostics', async ({ page, browser }, info) => {
  const errors = await ready(page);
  const before = await state(page);
  await page.mouse.move(400, 300);
  await mouseButton(page, 'right', true, 2);
  // Small movements avoid absolute-coordinate injection overshooting the
  // virtual screen while the browser is pointer-locked.
  await dragMotion(page, 80, 20, 2);
  await mouseButton(page, 'right', false, 0);
  await expect.poll(async () => (await state(page)).camera.yaw).not.toBe(before.camera.yaw);
  await page.mouse.wheel(0, -200);
  await expect.poll(async () => (await state(page)).camera.distance).toBeLessThan(before.camera.distance);
  await page.locator('#arena').focus();
  await page.keyboard.down('w');
  await page.waitForTimeout(300);
  await page.getByRole('button', { name: 'Diagnostics' }).click();
  const focusFrame = (await state(page)).frames;
  await expect.poll(async () => (await state(page)).frames).toBeGreaterThan(focusFrame + 1);
  const stopped = (await state(page)).player;
  await expect.poll(async () => (await state(page)).frames).toBeGreaterThan(focusFrame + 3);
  expect((await state(page)).player).toEqual(stopped);
  await page.keyboard.up('w');
  await expect.poll(() => page.evaluate(() => window.arenaDebug.metrics.frameSamples), { timeout: 45_000 }).toBeGreaterThan(20);
  const metrics = await page.evaluate(() => window.arenaDebug.metrics);
  expect(metrics.frameSamples).toBeGreaterThan(20);
  expect(metrics.errors).toEqual([]);
  const resources = await page.evaluate(() => performance.getEntriesByType('resource').map(({ name, transferSize, encodedBodySize, duration }) => ({ name, transferSize, encodedBodySize, duration })));
  const graphics = await page.evaluate(() => {
    const gl = document.querySelector('#arena').getContext('webgl2');
    const debug = gl?.getExtension('WEBGL_debug_renderer_info');
    return gl ? { version: gl.getParameter(gl.VERSION), renderer: debug ? gl.getParameter(debug.UNMASKED_RENDERER_WEBGL) : gl.getParameter(gl.RENDERER) } : null;
  });
  await writeFile(info.outputPath('metrics.json'), JSON.stringify({ browser: browser.version(), viewport: page.viewportSize(), graphics, metrics, resources }, null, 2));
  await page.screenshot({ path: info.outputPath('practice.png') });
  await page.locator('#arena').focus();
  await page.keyboard.press('Shift+Tab');
  await expect(page.locator('#arena')).not.toBeFocused();
  expect(errors).toEqual([]);
});

test('debug bridge is opt-in and load failures remain visible', async ({ page }) => {
  await page.goto('/');
  await expect(page.locator('#loading')).toBeHidden();
  expect(await page.evaluate(() => typeof window.arenaDebug)).toBe('undefined');
  await page.route('**/pkg/arena_client_bg.wasm', (route) => route.abort());
  await page.reload();
  await expect(page.locator('#loading-message')).toContainText('Try a current desktop browser');
  await expect(page.locator('#loading')).toBeVisible();
});

const locked = (page) => page.evaluate(() => document.pointerLockElement?.id ?? null);
async function nextFrames(page, count = 2) {
  const frame = (await state(page)).frames;
  await expect.poll(async () => (await state(page)).frames).toBeGreaterThan(frame + count);
}
// Chromium's high-level drag interception can stall during pointer lock.
// Keep one CDP input session per page so pressed buttons survive transitions.
// These are trusted browser events, never synthetic DOM events or game-state edits.
const inputSessions = new WeakMap();
async function chromiumInput(page, event) {
  let session = inputSessions.get(page);
  if (!session) {
    session = await page.context().newCDPSession(page);
    inputSessions.set(page, session);
  }
  await session.send('Input.dispatchMouseEvent', event);
}
async function dragMotion(page, x, y, buttons) {
  if (page.context().browser().browserType().name() === 'chromium') {
    await chromiumInput(page, { type: 'mouseMoved', x, y, buttons, pointerType: 'mouse' });
  } else {
    await page.mouse.move(x, y);
  }
}
async function mouseButton(page, button, down, buttons) {
  if (page.context().browser().browserType().name() === 'chromium') {
    await chromiumInput(page, {
      type: down ? 'mousePressed' : 'mouseReleased', x: 400, y: 300,
      button, buttons, clickCount: 1, pointerType: 'mouse',
    });
  } else {
    await page.mouse[down ? 'down' : 'up']({ button });
  }
}
async function beginDrag(page, button) {
  await page.mouse.move(400, 300);
  await mouseButton(page, button, true, button === 'left' ? 1 : 2);
  await expect.poll(() => locked(page)).toBe('arena');
  expect(await page.locator('#arena').evaluate((canvas) => getComputedStyle(canvas).cursor)).toBe('none');
}
async function expectReleased(page) {
  await expect.poll(() => locked(page)).toBe(null);
  expect(await page.locator('#arena').evaluate((canvas) => getComputedStyle(canvas).cursor)).not.toBe('none');
}

test('left drag is free-look, preserves heading and target, and restores the cursor', async ({ page }) => {
  const errors = await ready(page);
  await page.keyboard.press('Tab');
  await expect.poll(async () => (await state(page)).selected).toBe(true);
  const before = await state(page);
  await beginDrag(page, 'left');
  // Small movements avoid absolute-coordinate injection overshooting the
  // virtual screen while the browser is pointer-locked.
  await dragMotion(page, 80, 20, 1);
  await expect.poll(async () => (await state(page)).camera.yaw).not.toBe(before.camera.yaw);
  const looking = await state(page);
  expect(looking.player).toEqual(before.player);
  expect(looking.selected).toBe(true);
  // Free-looking while walking must not steer the movement or the character.
  await page.keyboard.down('w');
  await expect.poll(async () => (await state(page)).player.z).toBeLessThan(before.player.z - 300);
  await page.keyboard.up('w');
  expect((await state(page)).player.x).toBe(before.player.x);
  expect((await state(page)).player.yaw).toBe(before.player.yaw);
  await mouseButton(page, 'left', false, 0);
  await expectReleased(page);
  await nextFrames(page);
  expect((await state(page)).selected).toBe(true); // A drag is never a targeting click.
  expect(errors).toEqual([]);
});

test('right drag steers character facing without walking and preserves held keyboard movement on release', async ({ page }) => {
  const errors = await ready(page);
  const before = await state(page);
  await beginDrag(page, 'right');
  // Small movements avoid absolute-coordinate injection overshooting the
  // virtual screen while the browser is pointer-locked.
  await dragMotion(page, 80, 20, 2);
  await expect.poll(async () => (await state(page)).camera.yaw).not.toBe(before.camera.yaw);
  const steered = await state(page);
  expect(steered.player.yaw).toBeCloseTo(steered.camera.yaw, 4);
  expect(steered.player.x).toBe(before.player.x);
  expect(steered.player.z).toBe(before.player.z);
  await page.keyboard.down('w');
  await mouseButton(page, 'right', false, 0);
  await expectReleased(page);
  await expect.poll(async () => {
    const player = (await state(page)).player;
    return Math.hypot(player.x - before.player.x, player.z - before.player.z);
  }).toBeGreaterThan(300);
  await page.keyboard.up('w');
  expect(errors).toEqual([]);
});

for (const [first, second] of [['left', 'right'], ['right', 'left']]) {
  test(`both buttons walk and steer (${first} then ${second}); releasing one retains its mode`, async ({ page }) => {
    const errors = await ready(page);
    await page.keyboard.press('Tab');
    await expect.poll(async () => (await state(page)).selected).toBe(true);
    const before = await state(page);
    await beginDrag(page, first);
    await mouseButton(page, second, true, 3);
    // Hold real input for a defined interval. Repeated DevTools state polling
    // while a software-GPU Chromium has both buttons captured can stall it.
    await page.waitForTimeout(400);
    await dragMotion(page, 80, 15, 3);
    await page.waitForTimeout(400);
    await mouseButton(page, first, false, second === 'right' ? 2 : 1);
    await page.waitForTimeout(600); // Allow the last fixed input sample to settle.
    const stopped = await state(page);
    expect(await locked(page)).toBe('arena');
    expect(stopped.player.z).toBeLessThan(before.player.z - 300);
    expect(Math.abs(stopped.player.x)).toBeGreaterThan(100);
    expect(stopped.player.yaw).toBeCloseTo(stopped.camera.yaw, 4);
    await page.waitForTimeout(1500);
    const waiting = await state(page);
    expect(waiting.frames).toBeGreaterThan(stopped.frames);
    expect(waiting.player).toEqual(stopped.player);
    await dragMotion(page, 40, 10, second === 'right' ? 2 : 1);
    await page.waitForTimeout(350);
    await mouseButton(page, second, false, 0);
    await expectReleased(page);
    await expect.poll(async () => (await state(page)).camera.yaw).not.toBe(stopped.camera.yaw);
    const remaining = await state(page);
    if (second === 'right') expect(remaining.player.yaw).toBeCloseTo(remaining.camera.yaw, 4);
    else expect(remaining.player.yaw).toBe(stopped.player.yaw);
    expect(remaining.player.x).toBe(stopped.player.x);
    expect(remaining.player.z).toBe(stopped.player.z);
    expect(remaining.selected).toBe(true);
    expect(errors).toEqual([]);
  });
}

test('losing pointer lock cancels both-button walking and restores the cursor', async ({ page }) => {
  const errors = await ready(page);
  await beginDrag(page, 'right');
  await mouseButton(page, 'left', true, 3);
  await expect.poll(async () => (await state(page)).player.z).toBeLessThan(5000);
  // Use the actual browser API to exercise lock loss without relying on an OS
  // escape gesture that headless browser automation does not consistently send.
  await page.evaluate(() => document.exitPointerLock());
  await expectReleased(page);
  await nextFrames(page);
  const stopped = (await state(page)).player;
  await nextFrames(page);
  expect((await state(page)).player).toEqual(stopped);
  await mouseButton(page, 'left', false, 2);
  await mouseButton(page, 'right', false, 0);
  expect(errors).toEqual([]);
});
