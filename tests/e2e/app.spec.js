const { expect, test } = require('@playwright/test')

function deferred() {
  let resolve
  const promise = new Promise((resolvePromise) => {
    resolve = resolvePromise
  })
  return { promise, resolve }
}

const browserErrors = new WeakMap()

test.beforeEach(async ({ page }) => {
  const errors = []
  browserErrors.set(page, errors)
  page.on('console', (message) => {
    if (message.type() === 'error') {
      errors.push(`console: ${message.text()}`)
    }
  })
  page.on('pageerror', (error) => errors.push(`pageerror: ${error.message}`))
})

test.afterEach(async ({ page }) => {
  expect(browserErrors.get(page)).toEqual([])
})

test('默认单操作与前置校验', async ({ page }) => {
  let requestCount = 0
  await page.route('**/short', async (route) => {
    if (route.request().method() === 'POST') {
      requestCount += 1
    }
    await route.fulfill({
      status: 200,
      contentType: 'application/json',
      body: JSON.stringify({ Code: 1, ShortUrl: 'https://sho.rt/unexpected' }),
    })
  })

  await page.goto('/')

  await expect(page).toHaveTitle('MyUrls')
  await expect(page.getByRole('heading', { name: 'MyUrls', exact: true })).toBeVisible()
  await expect(page.getByText('把长链接，变得简单。')).toBeVisible()
  await expect(page.locator('#long-url')).toBeVisible()
  await expect(page.locator('details.custom-key')).not.toHaveAttribute('open', '')
  await expect(page.locator('#short-key')).toBeHidden()
  await expect(page.locator('#copy-button')).toBeHidden()
  await expect(page.locator('#status')).toHaveText('粘贴链接后按回车，或点按箭头。')

  await page.locator('#long-url').fill('ftp://example.com/file')
  await page.locator('#long-url').press('Enter')

  expect(requestCount).toBe(0)
  await expect(page.locator('#status')).toHaveAttribute('data-state', 'invalid')
  await expect(page.locator('#status')).toHaveText('请输入以 http:// 或 https:// 开头的有效链接。')
  await expect(page.locator('#long-url')).toBeFocused()
  await expect(page.locator('#copy-button')).toBeHidden()
  await expect(page.locator('#short-url')).toBeEmpty()

  await page.locator('#long-url').fill('https://example.com/valid')
  await page.locator('#long-url').press('Enter')
  await expect(page.locator('#short-url')).toHaveText('https://sho.rt/unexpected')
  await expect(page.locator('#copy-button')).toBeVisible()
  expect(requestCount).toBe(1)

  await page.locator('#long-url').fill('not a url')
  await page.locator('#long-url').press('Enter')

  await expect.soft(page.locator('#copy-button')).toBeHidden()
  await expect.soft(page.locator('#short-url')).toBeEmpty()
  await expect.soft(page.locator('#status')).toHaveAttribute('data-state', 'invalid')
  await expect.soft(page.locator('#status')).toHaveText('请输入以 http:// 或 https:// 开头的有效链接。')
  await expect.soft(page.locator('#long-url')).toBeFocused()
  expect.soft(requestCount).toBe(1)

  await page.locator('details.custom-key > summary').click()
  await page.locator('#short-key').fill('bad key!')
  await page.locator('details.custom-key > summary').click()
  await page.locator('#long-url').fill('https://example.com/valid-again')
  await page.locator('#long-url').press('Enter')

  expect(requestCount).toBe(1)
  await expect(page.locator('#copy-button')).toBeHidden()
  await expect(page.locator('#short-url')).toBeEmpty()
  await expect(page.locator('#status')).toHaveAttribute('data-state', 'invalid')
  await expect(page.locator('#status')).toHaveText('自定义短码只能使用 1–64 位字母、数字、下划线或连字符。')
  await expect(page.locator('details.custom-key')).toHaveAttribute('open', '')
  await expect(page.locator('#short-key')).toBeFocused()

  const repoLink = page.getByRole('link', { name: '在 GitHub 打开 keleyaa/MyUrls 仓库' })
  await expect(repoLink).toHaveAttribute('href', 'https://github.com/keleyaa/MyUrls')
  await expect(repoLink).toHaveAttribute('rel', 'noopener noreferrer')
})

test('Enter提交/loading/自动复制/再次复制', async ({ page }) => {
  await page.addInitScript(() => {
    window.__clipboardMode = 'success'
    window.__clipboardText = ''
    window.__execCommandCount = 0
    Object.defineProperty(Navigator.prototype, 'clipboard', {
      configurable: true,
      get() {
        return {
          writeText(value) {
            if (window.__clipboardMode === 'pending') {
              return new Promise((resolve, reject) => {
                window.__pendingCopy = { resolve, reject, value }
              })
            }
            if (window.__clipboardMode === 'reject') {
              return Promise.reject(new Error('clipboard denied'))
            }
            window.__clipboardText = value
            return Promise.resolve()
          },
          readText() {
            return Promise.resolve(window.__clipboardText)
          },
        }
      },
    })
    Object.defineProperty(Document.prototype, 'execCommand', {
      configurable: true,
      value() {
        window.__execCommandCount += 1
        return false
      },
    })
  })
  const requestObserved = deferred()
  const releaseRequest = deferred()
  let requestCount = 0
  let requestContract
  await page.route('**/short', async (route) => {
    if (route.request().method() !== 'POST') {
      await route.continue()
      return
    }

    requestCount += 1
    if (requestCount === 1) {
      const request = route.request()
      requestContract = {
        contentType: request.headers()['content-type'],
        postData: request.postData(),
      }
      requestObserved.resolve()
      await releaseRequest.promise
    }
    await route.fulfill({
      status: 200,
      contentType: 'application/json',
      body: JSON.stringify({ Code: 1, ShortUrl: 'https://sho.rt/luminous' }),
    })
  })

  await page.goto('/')
  await page.locator('details.custom-key > summary').click()
  await page.locator('#short-key').fill('luminous')
  await page.locator('#long-url').fill('https://example.com/long/path')
  await page.locator('#long-url').press('Enter')

  await requestObserved.promise
  expect(requestContract.contentType).toContain('multipart/form-data; boundary=')
  expect(requestContract.postData).toContain('name="longUrl"')
  expect(requestContract.postData).toContain('https://example.com/long/path')
  expect(requestContract.postData).toContain('name="shortKey"')
  expect(requestContract.postData).toContain('luminous')
  await expect(page.locator('#shorten-button')).toBeDisabled()
  await expect(page.locator('#shorten-form')).toHaveAttribute('aria-busy', 'true')
  await expect(page.locator('#shorten-form')).toHaveAttribute('data-state', 'loading')
  await expect(page.locator('#status')).toHaveText('正在生成短链接…')

  releaseRequest.resolve()

  await expect(page.locator('#short-url')).toHaveText('https://sho.rt/luminous')
  await expect(page.locator('#copy-button')).toBeVisible()
  await expect(page.locator('#copy-button')).toBeEnabled()
  await expect(page.locator('#status')).toHaveText('已生成并自动复制。')
  await expect.poll(() => page.evaluate(() => navigator.clipboard.readText())).toBe('https://sho.rt/luminous')

  await page.locator('#copy-button').click()
  await expect(page.locator('#status')).toHaveText('短链接已复制。')
  await expect.poll(() => page.evaluate(() => navigator.clipboard.readText())).toBe('https://sho.rt/luminous')

  await page.evaluate(() => {
    window.__clipboardMode = 'pending'
  })
  await page.locator('#copy-button').click()
  await expect.poll(() => page.evaluate(() => Boolean(window.__pendingCopy))).toBe(true)

  await page.evaluate(() => {
    window.__clipboardMode = 'success'
  })
  await page.locator('#long-url').press('Enter')
  await expect(page.locator('#status')).toHaveText('已生成并自动复制。')
  expect(requestCount).toBe(2)

  await page.evaluate(() => {
    window.__pendingCopy.reject(new Error('old copy failed'))
  })
  await expect.poll(() => page.evaluate(() => window.__execCommandCount)).toBe(1)
  await expect(page.locator('#status')).toHaveText('已生成并自动复制。')
})

test('业务错误脱敏与旧结果清除', async ({ page }) => {
  let requestCount = 0
  await page.route('**/short', async (route) => {
    if (route.request().method() !== 'POST') {
      await route.continue()
      return
    }

    requestCount += 1
    const payload = requestCount === 1
      ? { Code: 1, ShortUrl: 'https://sho.rt/first' }
      : { Code: 1001, Message: 'internal redis key already exists' }
    await route.fulfill({
      status: 200,
      contentType: 'application/json',
      body: JSON.stringify(payload),
    })
  })

  await page.goto('/')
  await page.locator('#long-url').fill('https://example.com/first')
  await page.locator('#long-url').press('Enter')
  await expect(page.locator('#short-url')).toHaveText('https://sho.rt/first')
  await expect(page.locator('#copy-button')).toBeVisible()

  await page.locator('#long-url').fill('https://example.com/second')
  await page.locator('#long-url').press('Enter')

  await expect(page.locator('#copy-button')).toBeHidden()
  await expect(page.locator('#short-url')).toBeEmpty()
  await expect(page.locator('#status')).toHaveAttribute('data-state', 'request-error')
  await expect(page.locator('#status')).toHaveText('短链接生成失败，请稍后重试。')
  await expect(page.locator('#status')).not.toContainText('redis')
  await expect(page.locator('#status')).not.toContainText('internal redis key already exists')
})

test('Clipboard不可用时textarea fallback，两条复制路径都失败仍保留结果', async ({ page }) => {
  await page.addInitScript(() => {
    window.__copyShouldSucceed = false
    Object.defineProperty(Navigator.prototype, 'clipboard', {
      configurable: true,
      get() {
        return {
          writeText: () => Promise.reject(new Error('clipboard denied')),
        }
      },
    })
    Object.defineProperty(Document.prototype, 'execCommand', {
      configurable: true,
      value(command) {
        const element = this.activeElement
        window.__copyFallback = {
          command,
          tagName: element?.tagName,
          value: element?.value,
          readOnly: element?.readOnly,
        }
        return command === 'copy' && window.__copyShouldSucceed
      },
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

  await expect(page.locator('#status')).toHaveAttribute('data-state', 'copy-error')
  await expect(page.locator('#status')).toHaveText('已生成，请手动复制。')
  await expect(page.locator('#copy-button')).toBeVisible()
  await expect(page.locator('#short-url')).toHaveText('https://sho.rt/fallback')
  await expect(page.locator('#long-url')).toBeFocused()
  expect(await page.evaluate(() => window.__copyFallback)).toEqual({
    command: 'copy',
    tagName: 'TEXTAREA',
    value: 'https://sho.rt/fallback',
    readOnly: true,
  })
  await expect(page.locator('textarea')).toHaveCount(0)

  await page.evaluate(() => {
    window.__copyShouldSucceed = true
  })
  await page.locator('#copy-button').click()
  await expect(page.locator('#status')).toHaveText('短链接已复制。')
  await expect(page.locator('#copy-button')).toBeFocused()
})
