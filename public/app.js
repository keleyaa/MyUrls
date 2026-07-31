'use strict'

const invalidURLMessage = '请输入以 http:// 或 https:// 开头的有效链接。'
const invalidKeyMessage = '自定义短码只能使用 1–64 位字母、数字、下划线或连字符。'
const loadingMessage = '正在生成短链接…'
const requestErrorMessage = '短链接生成失败，请稍后重试。'
const copiedAutomaticallyMessage = '已生成并自动复制。'
const copyAfterCreateFailedMessage = '已生成，请手动复制。'
const copiedAgainMessage = '短链接已复制。'
const copyAgainFailedMessage = '复制失败，请手动选择并复制。'

async function createShortURL(longUrl, shortKey) {
  const data = new FormData()
  data.append('longUrl', longUrl)
  data.append('shortKey', shortKey)

  const response = await fetch('/short', {
    method: 'POST',
    body: data,
  })
  if (!response.ok) {
    throw new Error('request failed')
  }

  const payload = await response.json()
  if (payload?.Code !== 1 || typeof payload.ShortUrl !== 'string' || payload.ShortUrl === '') {
    throw new Error('request failed')
  }
  return payload.ShortUrl
}

function copyWithTemporaryTextarea(value) {
  const previousActiveElement = document.activeElement
  const textarea = document.createElement('textarea')
  textarea.value = value
  textarea.readOnly = true
  textarea.tabIndex = -1
  textarea.setAttribute('aria-hidden', 'true')
  textarea.style.position = 'fixed'
  textarea.style.insetInlineStart = '-9999px'
  textarea.style.insetBlockStart = '0'
  document.body.append(textarea)

  try {
    textarea.focus()
    textarea.select()
    textarea.setSelectionRange(0, textarea.value.length)
    if (!document.execCommand('copy')) {
      throw new Error('copy failed')
    }
  } finally {
    textarea.remove()
    if (previousActiveElement instanceof HTMLElement && previousActiveElement.isConnected) {
      previousActiveElement.focus({ preventScroll: true })
    }
  }
}

async function copyText(value) {
  if (navigator.clipboard?.writeText) {
    try {
      await navigator.clipboard.writeText(value)
      return
    } catch {
      // Continue with the local fallback when the Clipboard API is denied.
    }
  }

  copyWithTemporaryTextarea(value)
}

function isValidHTTPURL(value) {
  if (!value || value !== value.trim()) {
    return false
  }

  try {
    const parsed = new URL(value)
    return (parsed.protocol === 'http:' || parsed.protocol === 'https:') && parsed.hostname !== ''
  } catch {
    return false
  }
}

document.addEventListener('DOMContentLoaded', () => {
  const form = document.querySelector('#shorten-form')
  const longURLInput = document.querySelector('#long-url')
  const shortKeyInput = document.querySelector('#short-key')
  const shortURL = document.querySelector('#short-url')
  const shortenButton = document.querySelector('#shorten-button')
  const copyButton = document.querySelector('#copy-button')
  const status = document.querySelector('#status')
  let resultVersion = 0
  let copyAttemptVersion = 0

  form.noValidate = true

  function setStatus(message, state) {
    form.dataset.state = state
    status.dataset.state = state
    status.textContent = message
  }

  function setBusy(isBusy) {
    shortenButton.disabled = isBusy
    shortenButton.setAttribute('aria-label', isBusy ? '正在生成短链接' : '生成短链接')
    form.setAttribute('aria-busy', String(isBusy))
  }

  function clearResult() {
    resultVersion += 1
    copyAttemptVersion += 1
    shortURL.replaceChildren()
    copyButton.hidden = true
    copyButton.disabled = true
    copyButton.removeAttribute('aria-label')
    copyButton.removeAttribute('title')
  }

  function showResult(value) {
    shortURL.replaceChildren(document.createTextNode(value))
    copyButton.hidden = false
    copyButton.disabled = false
    copyButton.setAttribute('aria-label', `复制短链接 ${value}`)
    copyButton.title = value
  }

  form.addEventListener('submit', async (event) => {
    event.preventDefault()

    const longUrl = longURLInput.value
    if (!isValidHTTPURL(longUrl)) {
      clearResult()
      setStatus(invalidURLMessage, 'invalid')
      longURLInput.focus()
      return
    }

    if (!shortKeyInput.checkValidity()) {
      clearResult()
      setStatus(invalidKeyMessage, 'invalid')
      const details = shortKeyInput.closest('details')
      if (details) {
        details.open = true
      }
      shortKeyInput.focus()
      shortKeyInput.reportValidity()
      return
    }

    clearResult()
    setBusy(true)
    setStatus(loadingMessage, 'loading')

    try {
      const value = await createShortURL(longUrl, shortKeyInput.value)
      let copiedAutomatically = true
      try {
        await copyText(value)
      } catch {
        copiedAutomatically = false
      }

      showResult(value)
      if (copiedAutomatically) {
        setStatus(copiedAutomaticallyMessage, 'success')
      } else {
        setStatus(copyAfterCreateFailedMessage, 'copy-error')
      }
    } catch {
      clearResult()
      setStatus(requestErrorMessage, 'request-error')
    } finally {
      setBusy(false)
    }
  })

  copyButton.addEventListener('click', async () => {
    copyAttemptVersion += 1
    const attemptVersion = copyAttemptVersion
    const value = shortURL.textContent
    if (!value) {
      clearResult()
      return
    }
    const version = resultVersion

    try {
      await copyText(value)
      if (version !== resultVersion || shortURL.textContent !== value || attemptVersion !== copyAttemptVersion) {
        return
      }
      setStatus(copiedAgainMessage, 'success')
    } catch {
      if (version !== resultVersion || shortURL.textContent !== value || attemptVersion !== copyAttemptVersion) {
        return
      }
      setStatus(copyAgainFailedMessage, 'copy-error')
    }
  })
})
