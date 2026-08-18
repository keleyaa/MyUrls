const { expect, test } = require('@playwright/test')

const browserErrors = new WeakMap()

test.beforeEach(async ({ page }) => {
  const errors = []
  browserErrors.set(page, errors)
  page.on('console', (message) => {
    if (message.type() === 'error') errors.push(message.text())
  })
  page.on('pageerror', (error) => errors.push(error.message))
})

test.afterEach(async ({ page }) => {
  expect(browserErrors.get(page)).toEqual([])
})

test('校验输入并通过 Enter 创建短链接', async ({ page }) => {
  let requests = 0
  await page.route('**/short', async (route) => {
    requests += 1
    await route.fulfill({
      status: 200,
      contentType: 'application/json',
      body: JSON.stringify({ Code: 1, ShortUrl: 'https://sho.rt/valid' }),
    })
  })

  await page.goto('/')
  await expect(page.getByRole('heading', { name: 'MyUrls', exact: true })).toBeVisible()

  await page.locator('#long-url').fill('ftp://example.com')
  await page.locator('#long-url').press('Enter')
  await expect(page.locator('#status')).toHaveAttribute('data-state', 'invalid')
  expect(requests).toBe(0)

  await page.locator('#long-url').fill('https://example.com/valid')
  await page.locator('#long-url').press('Enter')
  await expect(page.locator('#short-url')).toHaveText('https://sho.rt/valid')
  await expect(page.locator('#copy-button')).toBeVisible()
  expect(requests).toBe(1)
})

test('显示业务失败且不暴露服务端错误', async ({ page }) => {
  await page.route('**/short', async (route) => {
    await route.fulfill({
      status: 200,
      contentType: 'application/json',
      body: JSON.stringify({ Code: 1001, Message: 'internal redis key already exists' }),
    })
  })

  await page.goto('/')
  await page.locator('#long-url').fill('https://example.com/failure')
  await page.locator('#long-url').press('Enter')

  await expect(page.locator('#status')).toHaveText('短链接生成失败，请稍后重试。')
  await expect(page.locator('#status')).not.toContainText('redis')
  await expect(page.locator('#copy-button')).toBeHidden()
})

test('Clipboard 不可用时保留手动复制路径', async ({ page }) => {
  await page.addInitScript(() => {
    Object.defineProperty(Navigator.prototype, 'clipboard', {
      configurable: true,
      get() { return { writeText: () => Promise.reject(new Error('denied')) } },
    })
    Object.defineProperty(Document.prototype, 'execCommand', {
      configurable: true,
      value: () => false,
    })
  })
  await page.route('**/short', async (route) => {
    await route.fulfill({
      status: 200,
      contentType: 'application/json',
      body: JSON.stringify({ Code: 1, ShortUrl: 'https://sho.rt/fallback' }),
    })
  })

  await page.goto('/')
  await page.locator('#long-url').fill('https://example.com/fallback')
  await page.locator('#long-url').press('Enter')

  await expect(page.locator('#status')).toHaveText('已生成，请手动复制。')
  await expect(page.locator('#short-url')).toHaveText('https://sho.rt/fallback')
  await expect(page.locator('#copy-button')).toBeVisible()
})

test('在移动视口提供可访问控件', async ({ page }) => {
  await page.goto('/')
  await page.locator('details.custom-key > summary').click()
  await expect(page.locator('#short-key')).toBeVisible()
  await expect(page.locator('#shorten-button')).toBeVisible()
  await expect(page.locator('#shorten-button')).toBeEnabled()
  expect(await page.locator('html').evaluate((element) => element.scrollWidth <= element.clientWidth)).toBe(true)
})
