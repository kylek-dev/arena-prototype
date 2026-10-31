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
  await page.mouse.move(720, 400);
  await page.mouse.down({ button: 'right' });
  await page.mouse.move(860, 430, { steps: 10 });
  await page.mouse.up({ button: 'right' });
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
  await writeFile(info.outputPath('metrics.json'), JSON.stringify({ browser: browser.version(), viewport: '1440x900', graphics, metrics, resources }, null, 2));
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
