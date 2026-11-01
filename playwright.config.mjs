import { defineConfig } from '@playwright/test';
export default defineConfig({
  testDir: './tests/browser',
  timeout: 120_000,
  expect: { timeout: 30_000 },
  workers: 1,
  use: { baseURL: 'http://127.0.0.1:8000', viewport: { width: 1024, height: 768 }, trace: process.env.ARENA_TRACE ? 'retain-on-failure' : 'off', screenshot: 'only-on-failure' },
  projects: [{ name: 'chromium', use: { browserName: 'chromium', launchOptions: { args: ['--use-angle=swiftshader', '--enable-unsafe-swiftshader'] } } }, ...(process.env.ARENA_TEST_FIREFOX ? [{ name: 'firefox', use: { browserName: 'firefox', launchOptions: { firefoxUserPrefs: { 'webgl.force-enabled': true, 'webgl.disabled': false } } } }] : [])],
  webServer: { command: 'npm run serve', url: 'http://127.0.0.1:8000', reuseExistingServer: !process.env.CI },
});
