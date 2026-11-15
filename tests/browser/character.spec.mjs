import { test, expect } from '@playwright/test';

const state = (page) => page.evaluate(() => window.arenaDebug?.state);

test('the optional rigged character loads and moves with the player', async ({ page }, info) => {
  const config = await (await page.request.get('/assets/character.json')).json();
  test.skip(!config.available, 'Character models are optional local source assets.');
  const model = `/assets/characters/${config.model}.glb`;
  const errors = [];
  const failedResponses = [];
  let modelDownloaded = false;
  page.on('pageerror', (error) => errors.push(error.message));
  page.on('response', (response) => {
    if (response.status() >= 400) failedResponses.push(response.url());
    if (response.url().endsWith(model) && response.ok()) modelDownloaded = true;
  });
  await page.goto('/?debug=1');
  await expect(page.locator('#loading')).toBeHidden({ timeout: 60_000 });
  await expect.poll(async () => (await state(page))?.character.status).toBe('ready');
  expect(modelDownloaded).toBe(true);
  expect(failedResponses).toEqual([]);
  expect((await state(page)).character.meshes).toBeGreaterThan(0);
  expect((await state(page)).character.animated).toBe(true);
  await expect(page.locator('#model-notice')).toBeHidden();
  await page.locator('#arena').focus();
  const start = (await state(page)).player;
  await page.keyboard.down('w');
  try {
    await expect.poll(async () => (await state(page)).player.z).toBeLessThan(start.z - 300);
  } finally {
    await page.keyboard.up('w');
  }
  await page.screenshot({ path: info.outputPath(`${config.model}-character.png`) });
  expect((await state(page)).character.status).toBe('ready');
  expect(errors).toEqual([]);
});

test('clean clones use the procedural character without fetching a missing model', async ({ page }) => {
  let modelRequests = 0;
  const errors = [];
  page.on('pageerror', (error) => errors.push(error.message));
  page.on('request', (request) => { if (request.url().includes('/assets/characters/')) modelRequests++; });
  await page.route('**/assets/character.json', (route) => route.fulfill({ json: { available: false, model: null } }));
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
  await page.route('**/assets/character.json', (route) => route.fulfill({ json: { available: true, model: 'base-female' } }));
  await page.route('**/assets/characters/*.glb', (route) => route.abort());
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
