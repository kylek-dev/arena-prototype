import { defineConfig } from '@playwright/test';
// Keep CSS and render coordinates at the same density. Fractional density
// changes picking behavior in the current Bevy browser adapter.
const size = process.env.ARENA_TEST_VIEWPORT ?? '800x600';
if (!['640x500', '800x600', '1024x768'].includes(size)) {
  throw new Error('ARENA_TEST_VIEWPORT must be 640x500, 800x600, or 1024x768');
}
const [width, height] = size.split('x').map(Number);
export default defineConfig({
  testDir: './tests/browser',
  timeout: 120_000,
  expect: { timeout: 30_000 },
  workers: 1,
  reporter: [['list'], ['json', { outputFile: 'test-results/results.json' }]],
  use: { baseURL: 'http://127.0.0.1:8000', viewport: { width, height }, trace: process.env.ARENA_TRACE ? 'retain-on-failure' : 'off', screenshot: 'only-on-failure' },
  projects: [{ name: 'chromium', use: { browserName: 'chromium', launchOptions: { args: ['--use-angle=swiftshader', '--enable-unsafe-swiftshader'] } } }, ...(process.env.ARENA_TEST_FIREFOX ? [{ name: 'firefox', use: { browserName: 'firefox', launchOptions: { firefoxUserPrefs: { 'webgl.force-enabled': true, 'webgl.disabled': false } } } }] : [])],
  webServer: { command: 'npm run serve', url: 'http://127.0.0.1:8000', reuseExistingServer: !process.env.CI },
});
