'use strict'

const requestErrorMessage = '短链接生成失败，请稍后重试。'
const invalidURLMessage = '请输入以 http:// 或 https:// 开头的有效链接。'

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

async function copyText(value) {
  if (navigator.clipboard?.writeText) {
    return navigator.clipboard.writeText(value)
  }

  const input = document.querySelector('#short-url')
  if (!input || input.value !== value) {
    throw new Error('copy failed')
  }

  input.focus()
  input.select()
  input.setSelectionRange(0, input.value.length)
  if (!document.execCommand('copy')) {
    throw new Error('copy failed')
  }
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
  const shortURLInput = document.querySelector('#short-url')
  const shortenButton = document.querySelector('#shorten-button')
  const copyButton = document.querySelector('#copy-button')
  const status = document.querySelector('#status')
  const repoLogo = document.querySelector('#repo-logo')

  function setStatus(message, state = '') {
    status.textContent = message
    if (state) {
      status.dataset.state = state
    } else {
      delete status.dataset.state
    }
  }

  function setBusy(isBusy) {
    shortenButton.disabled = isBusy
    shortenButton.textContent = isBusy ? '正在生成…' : '生成短链接'
    form.setAttribute('aria-busy', String(isBusy))
  }

  form.addEventListener('submit', async (event) => {
    event.preventDefault()

    const longUrl = longURLInput.value
    if (!isValidHTTPURL(longUrl)) {
      setStatus(invalidURLMessage, 'error')
      longURLInput.focus()
      return
    }
    if (!form.checkValidity()) {
      form.reportValidity()
      return
    }

    shortURLInput.value = ''
    copyButton.disabled = true
    setBusy(true)
    setStatus('正在生成短链接…')

    try {
      const shortURL = await createShortURL(longUrl, shortKeyInput.value)
      shortURLInput.value = shortURL

      try {
        await copyText(shortURL)
        setStatus('短链接已生成并复制。', 'success')
      } catch {
        setStatus('短链接已生成，请手动复制。', 'success')
      } finally {
        copyButton.disabled = false
      }
    } catch {
      setStatus(requestErrorMessage, 'error')
    } finally {
      setBusy(false)
    }
  })

  copyButton.addEventListener('click', async () => {
    const value = shortURLInput.value
    if (!value) {
      copyButton.disabled = true
      return
    }

    try {
      await copyText(value)
      setStatus('短链接已复制。', 'success')
    } catch {
      setStatus('复制失败，请手动选择并复制。', 'error')
    }
  })

  repoLogo.addEventListener('click', (event) => {
    if (!repoLogo.href.startsWith('https://github.com/')) {
      event.preventDefault()
    }
  })
})
