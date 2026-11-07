import { test, expect } from '@playwright/test';

const state = (page) => page.evaluate(() => window.arenaDebug?.state);

test('optional textured character loads and moves with the player', async ({ page }, info) => {
  const config = await (await page.request.get('/assets/character.json')).json();
  test.skip(!config.available, 'Third-party character is an optional local download, not a public repository asset.');
  const errors = [];
  const failedResponses = [];
  let modelDownloaded = false;
  page.on('pageerror', (error) => errors.push(error.message));
  page.on('response', (response) => {
    if (response.status() >= 400) failedResponses.push(response.url());
    if (response.url().endsWith('pixellabs-undead-3417.glb') && response.ok()) modelDownloaded = true;
  });
  await page.goto('/?debug=1');
  await expect(page.locator('#loading')).toBeHidden({ timeout: 60_000 });
  await expect.poll(async () => (await state(page))?.character.status).toBe('ready');
  expect(modelDownloaded).toBe(true);
  expect(failedResponses).toEqual([]);
  expect((await state(page)).character.meshes).toBeGreaterThan(0);
  expect((await state(page)).character.animated).toBe(false);
  await expect(page.locator('#model-notice')).toBeHidden();
  await page.locator('#arena').focus();
  const start = (await state(page)).player;
  await page.keyboard.down('w');
  try {
    await expect.poll(async () => (await state(page)).player.z).toBeLessThan(start.z - 300);
  } finally {
    await page.keyboard.up('w');
  }
  await page.screenshot({ path: info.outputPath('textured-character.png') });
  expect((await state(page)).character.status).toBe('ready');
  expect(errors).toEqual([]);
});

test('clean clones use the procedural character without fetching a missing model', async ({ page }) => {
  let modelRequests = 0;
  const errors = [];
  page.on('pageerror', (error) => errors.push(error.message));
  page.on('request', (request) => { if (request.url().endsWith('.glb')) modelRequests++; });
  await page.route('**/assets/character.json', (route) => route.fulfill({ json: { available: false } }));
  await page.goto('/?debug=1');
  await expect(page.locator('#loading')).toBeHidden();
  expect((await state(page)).character.status).toBe('placeholder');
  expect((await state(page)).character.meshes).toBe(0);
  expect(modelRequests).toBe(0);
  await expect(page.locator('#model-notice')).toBeHidden();
  expect(errors).toEqual([]);
});

test('a failed character download retains the playable fallback and shows a notice', async ({ page }) => {
  const errors = [];
  page.on('pageerror', (error) => errors.push(error.message));
  await page.route('**/assets/character.json', (route) => route.fulfill({ json: { available: true } }));
  await page.route('**/pixellabs-undead-3417.glb', (route) => route.abort());
  await page.goto('/?debug=1');
  await expect(page.locator('#loading')).toBeHidden();
  await expect(page.locator('#model-notice')).toBeVisible();
  expect((await state(page)).character.status).toBe('failed');
  await page.locator('#arena').focus();
  const start = (await state(page)).player.z;
  await page.keyboard.down('w');
  try {
    await expect.poll(async () => (await state(page)).player.z).toBeLessThan(start - 300);
  } finally {
    await page.keyboard.up('w');
  }
  expect(errors).toEqual([]);
});
