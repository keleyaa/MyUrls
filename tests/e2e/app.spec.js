const { expect, test } = require('@playwright/test')

function deferred() {
  let resolve
  const promise = new Promise((resolvePromise) => {
    resolve = resolvePromise
  })
  return { promise, resolve }
}

function uniqueShortKey(projectName) {
  const project = projectName.toLowerCase().replace(/[^a-z0-9]+/g, '-')
  return `e2e-${project}-${Date.now()}-${Math.random().toString(36).slice(2, 10)}`
}

test('creates and copies a short URL, reports business errors, and fits the viewport', async ({ page }, testInfo) => {
  const browserErrors = []
  page.on('console', (message) => {
    if (message.type() === 'error') {
      browserErrors.push(`console: ${message.text()}`)
    }
  })
  page.on('pageerror', (error) => browserErrors.push(`pageerror: ${error.message}`))

  await page.goto('/')
  await expect(page).toHaveTitle('MyUrls')
  await expect(page.getByRole('heading', { name: '创建短链接' })).toBeVisible()

  const shortKey = uniqueShortKey(testInfo.project.name)
  await page.getByLabel('长链接').fill(`https://example.com/${shortKey}`)
  await page.getByLabel('自定义短码').fill(shortKey)

  const requestObserved = deferred()
  const releaseRequest = deferred()
  await page.route('**/short', async (route) => {
    if (route.request().method() === 'POST') {
      requestObserved.resolve()
      await releaseRequest.promise
    }
    await route.continue()
  })

  const responsePromise = page.waitForResponse(
    (response) => response.url().endsWith('/short') && response.request().method() === 'POST',
  )
  let loadingAssertionError
  try {
    await page.getByRole('button', { name: '生成短链接' }).click()
    await requestObserved.promise
    await expect(page.locator('#shorten-button')).toBeDisabled()
    await expect(page.locator('#shorten-button')).toHaveText('正在生成…')
    await expect(page.locator('#status')).toHaveText('正在生成短链接…')
  } catch (error) {
    loadingAssertionError = error
  } finally {
    releaseRequest.resolve()
  }

  const response = await responsePromise
  await page.unroute('**/short')
  if (loadingAssertionError) {
    throw loadingAssertionError
  }
  expect(response.ok()).toBeTruthy()

  const expectedShortURL = `http://127.0.0.1:8080/${shortKey}`
  await expect(page.locator('#short-url')).toHaveValue(expectedShortURL)
  await expect(page.locator('#status')).toHaveText('短链接已生成并复制。')
  await expect(page.locator('#copy-button')).toBeEnabled()
  await expect.poll(() => page.evaluate(() => navigator.clipboard.readText())).toBe(expectedShortURL)

  await page.locator('#copy-button').click()
  await expect(page.locator('#status')).toHaveText('短链接已复制。')
  await expect.poll(() => page.evaluate(() => navigator.clipboard.readText())).toBe(expectedShortURL)

  const businessResponsePromise = page.waitForResponse(
    (candidate) => candidate.url().endsWith('/short') && candidate.request().method() === 'POST',
  )
  await page.getByLabel('自定义短码').fill('healthz')
  await page.getByRole('button', { name: '生成短链接' }).click()
  const businessResponse = await businessResponsePromise
  const businessPayload = await businessResponse.json()
  expect(businessPayload.Code).toBe(1001)
  expect(typeof businessPayload.Message).toBe('string')
  await expect(page.locator('#status')).toHaveText('短链接生成失败，请稍后重试。')
  await expect(page.locator('#status')).not.toContainText(businessPayload.Message)

  const fitsViewport = await page.evaluate(
    () => document.documentElement.scrollWidth <= document.documentElement.clientWidth,
  )
  expect(fitsViewport).toBeTruthy()
  expect(browserErrors).toEqual([])
})
