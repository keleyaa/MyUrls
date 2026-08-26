import { expect, test, type Page } from '@playwright/test';

async function openTool(page: Page): Promise<void> {
  await page.goto('/');
  await expect(page.getByLabel('目标 URL')).toBeVisible();
}

function scopedAlias(base: string, projectName: string): string {
  const projectKey = projectName === 'mobile-chromium' ? 'm' : projectName[0];
  return `${base}_${projectKey}_${Date.now().toString(36)}`;
}

function resultCode(page: Page, code: string) {
  return page.getByLabel('短链接结果').getByText(code, { exact: true });
}

test.describe('myurl user flows', () => {
  test('creates by click and keeps the result visible', async ({ page }, testInfo) => {
    if (testInfo.project.name !== 'webkit') {
      await page.context().grantPermissions(['clipboard-read', 'clipboard-write']);
    }
    await openTool(page);
    await page.getByLabel('目标 URL').fill('https://example.com/click-flow');
    await page.getByRole('button', { name: '生成并复制' }).click();
    await expect(page.getByRole('button', { name: '复制短链接' })).toBeVisible();
    await expect(
      page.getByLabel('短链接结果').getByText('已生成并复制', { exact: true }),
    ).toBeVisible();
  });

  test('submits the same path with Enter and normalizes an alias', async ({ page }, testInfo) => {
    const aliasValue = scopedAlias('readme_42', testInfo.project.name);
    await openTool(page);
    await page.getByRole('button', { name: '自定义别名' }).click();
    const alias = page.locator('#custom-alias');
    await alias.fill(aliasValue.toUpperCase());
    await expect(alias).toHaveValue(aliasValue);
    const url = page.getByLabel('目标 URL');
    await url.fill('https://example.com/enter-flow');
    await url.press('Enter');
    await expect(resultCode(page, aliasValue)).toBeVisible();
  });

  test('allows an alias conflict to be corrected in place', async ({ page }, testInfo) => {
    const conflictAlias = scopedAlias('conflict', testInfo.project.name);
    const correctedAlias = scopedAlias('corrected', testInfo.project.name);
    await openTool(page);
    await page.getByRole('button', { name: '自定义别名' }).click();
    await page.locator('#custom-alias').fill(conflictAlias);
    await page.getByLabel('目标 URL').fill('https://example.com/first');
    await page.getByRole('button', { name: '生成并复制' }).click();
    await expect(resultCode(page, conflictAlias)).toBeVisible();

    await page.getByLabel('目标 URL').fill('https://example.com/second');
    await page.locator('#custom-alias').fill(conflictAlias);
    await page.getByRole('button', { name: '生成并复制' }).click();
    await expect(
      page.getByRole('status').getByText('这个别名已被占用，请换一个。', { exact: true }),
    ).toBeVisible();

    await page.locator('#custom-alias').fill(correctedAlias);
    await page.getByRole('button', { name: '生成并复制' }).click();
    await expect(resultCode(page, correctedAlias)).toBeVisible();
  });

  test('shows the copy fallback without claiming a false success', async ({ page }) => {
    await page.addInitScript(() => {
      Object.defineProperty(window.navigator, 'clipboard', {
        configurable: true,
        value: { writeText: async () => Promise.reject(new Error('clipboard denied')) },
      });
    });
    await openTool(page);
    await page.getByLabel('目标 URL').fill('https://example.com/clipboard-fallback');
    await page.getByRole('button', { name: '生成并复制' }).click();
    await expect(
      page.getByLabel('短链接结果').getByText('短链已生成 · 点击结果即可复制', { exact: true }),
    ).toBeVisible();
    await page.getByRole('button', { name: '复制短链接' }).click();
    await expect(
      page.getByLabel('短链接结果').getByText('短链已生成 · 点击结果即可复制', { exact: true }),
    ).toBeVisible();
  });

  test('loads Turnstile only after challenge_required and retries automatically', async ({
    page,
  }) => {
    await page.addInitScript(() => {
      window.turnstile = {
        render: (_element, options) => {
          (
            window as Window & { testTurnstileCallback?: (token: string) => void }
          ).testTurnstileCallback = options.callback;
          return 'test-widget';
        },
        reset: () => undefined,
      };
    });
    let firstRequest = true;
    await page.route('**/api/v1/links', async (route) => {
      if (!firstRequest) {
        await route.continue();
        return;
      }
      firstRequest = false;
      await route.fulfill({
        status: 403,
        contentType: 'application/json',
        body: JSON.stringify({
          error: { code: 'challenge_required', requestId: 'req_test' },
          challenge: { provider: 'turnstile', siteKey: 'test-site-key' },
        }),
      });
    });
    await openTool(page);
    await page.getByLabel('目标 URL').fill('https://example.com/challenge-flow');
    await page.getByRole('button', { name: '生成并复制' }).click();
    await expect(
      page.locator('.challenge-region').getByText('请完成验证后继续。', { exact: true }),
    ).toBeVisible();
    await page.evaluate(() => {
      (
        window as Window & { testTurnstileCallback?: (token: string) => void }
      ).testTurnstileCallback?.('test-token');
    });
    await expect(page.getByRole('button', { name: '复制短链接' })).toBeVisible();
  });

  test('reports a degraded readiness state without blocking the editor', async ({ page }) => {
    await page.route('**/health/ready', (route) =>
      route.fulfill({
        status: 503,
        contentType: 'application/json',
        body: '{"status":"degraded"}',
      }),
    );
    await openTool(page);
    await expect(page.getByText('service degraded')).toBeVisible();
    await expect(page.getByLabel('目标 URL')).toBeEnabled();
  });

  test('supports redirect, HEAD, and browser-safe 404 responses', async ({ page }) => {
    await openTool(page);
    const response = await page.request.post('/api/v1/links', {
      data: { url: 'https://example.com/redirect-flow' },
    });
    expect(response.status()).toBe(201);
    const created = (await response.json()) as { code: string };
    const redirect = await page.request.get(`/${created.code}`, { maxRedirects: 0 });
    expect(redirect.status()).toBe(302);
    expect(redirect.headers().location).toBe('https://example.com/redirect-flow');
    const head = await page.request.head(`/${created.code}`, { maxRedirects: 0 });
    expect(head.status()).toBe(302);
    expect(await head.body()).toHaveLength(0);

    const missing = await page.request.get('/missing-browser-link');
    expect(missing.status()).toBe(404);
    expect(missing.headers()['content-type']).toContain('text/html');
    expect(await missing.text()).not.toContain('Redis');
  });

  test('captures the required viewport visual states without horizontal overflow', async ({
    page,
  }, testInfo) => {
    await openTool(page);
    const idleMetrics = await page.evaluate(() => ({
      width: innerWidth,
      scrollWidth: document.documentElement.scrollWidth,
    }));
    expect(idleMetrics.scrollWidth).toBeLessThanOrEqual(idleMetrics.width);
    await page.screenshot({ path: testInfo.outputPath('idle.png'), fullPage: true });

    await page.getByLabel('目标 URL').fill('https://example.com/visual-success');
    await page.getByRole('button', { name: '生成并复制' }).click();
    await expect(page.getByRole('button', { name: '复制短链接' })).toBeVisible();
    const successMetrics = await page.evaluate(() => ({
      width: innerWidth,
      scrollWidth: document.documentElement.scrollWidth,
    }));
    expect(successMetrics.scrollWidth).toBeLessThanOrEqual(successMetrics.width);
    await page.screenshot({ path: testInfo.outputPath('success.png'), fullPage: true });

    const viewportWidth = await page.evaluate(() => innerWidth);
    if (viewportWidth > 320) {
      await page.setViewportSize({ width: 320, height: 844 });
      const minimumMetrics = await page.evaluate(() => ({
        width: innerWidth,
        scrollWidth: document.documentElement.scrollWidth,
      }));
      expect(minimumMetrics.scrollWidth).toBeLessThanOrEqual(minimumMetrics.width);
      await page.screenshot({ path: testInfo.outputPath('minimum-width.png'), fullPage: true });
    }
  });
});
