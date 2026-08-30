import { defineConfig, devices } from '@playwright/test';

const port = 4310;

export default defineConfig({
  testDir: './tests/e2e',
  fullyParallel: false,
  workers: 1,
  timeout: 30000,
  expect: { timeout: 8000 },
  reporter: [['list'], ['html', { open: 'never' }]],
  use: {
    baseURL: `http://127.0.0.1:${port}`,
    trace: 'retain-on-failure',
    screenshot: 'only-on-failure',
  },
  projects: [
    {
      name: 'chromium',
      use: { ...devices['Desktop Chrome'], viewport: { width: 1440, height: 900 } },
    },
    {
      name: 'mobile-chromium',
      use: { ...devices['Pixel 7'], viewport: { width: 390, height: 844 } },
    },
    {
      name: 'webkit',
      use: { ...devices['Desktop Safari'], viewport: { width: 1440, height: 900 } },
    },
  ],
  webServer: {
    command:
      'corepack pnpm --filter @myurl/contracts build && corepack pnpm --filter @myurl/web build && WEB_ROOT=apps/web/dist cargo run -p myurl-server --features test-support',
    url: `http://127.0.0.1:${port}/health/live`,
    reuseExistingServer: false,
    timeout: 120000,
    env: {
      ...process.env,
      NODE_ENV: 'test',
      APP_PORT: String(port),
      PUBLIC_BASE_URL: `http://127.0.0.1:${port}`,
      REDIS_URL: 'redis://127.0.0.1:63999/0',
      IP_HASH_SECRET: 'e2e-only-secret-that-is-at-least-32-bytes-long',
      TURNSTILE_ENABLED: 'false',
      TURNSTILE_MODE: 'test',
      TURNSTILE_SITE_KEY: 'test-site-key',
      TURNSTILE_SECRET_KEY: 'test-secret-key',
      TURNSTILE_HOSTNAME: '127.0.0.1',
      TEST_FORCE_CHALLENGE: 'false',
      TEST_STORE: 'memory',
      CREATE_DIRECT_LIMIT_10M: '1000',
      CREATE_HARD_LIMIT_10M: '1100',
      CREATE_HARD_LIMIT_1D: '1200',
    },
  },
});
