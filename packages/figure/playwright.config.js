// Playwright tests for <pencil-figure>, headless only, on Playwright's own Chromium and WebKit.
//
// Run: `pnpm exec playwright install chromium webkit` once, then `pnpm test` here.
//
// What draws where, found on macOS with Playwright 1.65 (Chromium 1255, WebKit 2388), headless:
//   - Chromium, no flags: `navigator.gpu` exists but has no adapter ("No available adapters"),
//     so the element falls back on its own to WebGL2 (`backend` is "Gl"). It draws.
//   - Chromium with `--enable-unsafe-swiftshader --use-angle=swiftshader --enable-unsafe-webgpu
//     --ignore-gpu-blocklist`: WebGPU works on SwiftShader (`backend` is "BrowserWebGpu"). The two
//     Chromium projects here use those, so they run the WebGPU path.
//   - WebKit, no flags: WebGPU works (`backend` is "BrowserWebGpu").
//   - `gl` on the element forces WebGL2 in all of them. Set PENCIL_GL=1 to run every test that
//     way (the fixture pages add `?gl`), which exercises the other path.
// PENCIL_MIN=1 runs every test on the minified bundle, packages/figure/pkg/pencil-figure.min.js,
//   which `scripts/build-web.sh` writes (the fixture pages add `?min`). It combines with PENCIL_GL=1.
// The tests never name a backend, only that `figure.backend` is set.
//
// A server for the repo root starts on 8743 (tests/serve.js, which does what scripts/serve.py does
// but with a deep listen backlog: serve.py's is 5, and the first burst from several browsers at
// once was refused, as "Failed to fetch"). One already running on that port is reused.
import { defineConfig, devices } from '@playwright/test';

const PORT = 8743;

const chromiumGpu = [
  '--enable-unsafe-swiftshader',
  '--use-angle=swiftshader',
  '--enable-unsafe-webgpu',
  '--ignore-gpu-blocklist',
];

export default defineConfig({
  testDir: './tests',
  testMatch: '**/*.spec.js',
  globalSetup: './tests/global-setup.js',
  fullyParallel: true,
  workers: process.env.CI ? 2 : 3,
  retries: 0,
  timeout: 45_000,
  expect: { timeout: 10_000 },
  reporter: [['list'], ['html', { open: 'never' }]],
  use: {
    baseURL: `http://127.0.0.1:${PORT}`,
    headless: true,
    trace: 'retain-on-failure',
  },
  webServer: {
    command: `node tests/serve.js ${PORT}`,
    url: `http://127.0.0.1:${PORT}/packages/figure/tests/pages/bounds.html`,
    reuseExistingServer: true,
    timeout: 30_000,
  },
  projects: [
    {
      name: 'desktop-chromium',
      use: {
        ...devices['Desktop Chrome'],
        // Playwright's own Chromium, never the installed Chrome.
        channel: undefined,
        viewport: { width: 1633, height: 919 },
        launchOptions: { args: chromiumGpu },
      },
    },
    {
      name: 'desktop-webkit',
      use: { ...devices['Desktop Safari'], viewport: { width: 1633, height: 919 } },
    },
    { name: 'iphone-15', use: { ...devices['iPhone 15'] } },
    {
      name: 'pixel-7',
      use: { ...devices['Pixel 7'], channel: undefined, launchOptions: { args: chromiumGpu } },
    },
  ],
});
