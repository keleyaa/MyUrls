<script lang="ts">
  import { onMount } from 'svelte';
  import { AlertTriangle, CircleX, LoaderCircle } from '@lucide/svelte';
  import BrandHeader from './components/BrandHeader.svelte';
  import LinkComposer from './components/LinkComposer.svelte';
  import LinkResult from './components/LinkResult.svelte';
  import ProjectFooter from './components/ProjectFooter.svelte';
  import TurnstileChallenge from './components/TurnstileChallenge.svelte';
  import {
    ApiError,
    checkReady,
    createLink,
    type CreateLinkInput,
    type ErrorCode,
  } from './lib/api.js';
  import type { PageState } from './lib/page-state.js';

  let state: PageState = { kind: 'idle' };
  let url = '';
  let alias = '';
  let aliasOpen = false;
  let ready = false;
  let urlInput: HTMLInputElement | undefined;

  const messages: Record<ErrorCode, string> = {
    invalid_request: '请求格式不正确，请检查输入。',
    challenge_required: '请完成验证后继续。',
    challenge_invalid: '验证未通过，请重试。',
    alias_unavailable: '这个别名已被占用，请换一个。',
    url_not_allowed: '请输入有效的 HTTP(S) 地址。',
    alias_invalid: '别名需为 4–32 个小写字母、数字、下划线或短横线。',
    rate_limited: '请求过于频繁，请稍后再试。',
    request_timeout: '请求处理超时，请稍后重试。',
    dependency_unavailable: '服务暂时不可用，请稍后重试。',
    code_generation_exhausted: '暂时无法生成短码，请稍后重试。',
  };

  $: submitting = state.kind === 'submitting';
  $: urlError =
    state.kind === 'validation-error' && state.code === 'url_not_allowed'
      ? state.message
      : undefined;
  $: aliasError =
    (state.kind === 'validation-error' || state.kind === 'challenge-error') &&
    (state.kind === 'validation-error'
      ? state.code === 'alias_invalid' || state.code === 'alias_unavailable'
      : false)
      ? state.message
      : undefined;
  $: statusMessage = getStatusMessage(state);

  onMount(() => {
    urlInput?.focus();
    void checkReady().then((value) => (ready = value));
  });

  function getStatusMessage(current: PageState): string {
    if (current.kind === 'success-copied') {
      return '已生成并复制 · 90 天后自动失效';
    }
    if (current.kind === 'success-copy-fallback') {
      return '短链已生成 · 点击结果即可复制';
    }
    if (current.kind === 'challenge' || current.kind === 'challenge-error') {
      return current.message;
    }
    if (
      current.kind === 'rate-limited' ||
      current.kind === 'dependency-error' ||
      current.kind === 'validation-error'
    ) {
      return current.message;
    }
    return '';
  }

  function toggleAlias(): void {
    aliasOpen = !aliasOpen;
  }

  async function copy(shortUrl: string): Promise<boolean> {
    try {
      await navigator.clipboard.writeText(shortUrl);
      return true;
    } catch {
      return false;
    }
  }

  async function submit(challengeToken?: string): Promise<void> {
    if (submitting) {
      return;
    }
    if (url.trim() === '') {
      state = {
        kind: 'validation-error',
        code: 'url_not_allowed',
        message: messages.url_not_allowed,
      };
      urlInput?.focus();
      return;
    }

    state = { kind: 'submitting' };
    const input: CreateLinkInput = alias === '' ? { url } : { url, alias };
    if (challengeToken !== undefined) {
      input.challengeToken = challengeToken;
    }
    try {
      const result = await createLink(input);
      const copied = await copy(result.shortUrl);
      state = { kind: copied ? 'success-copied' : 'success-copy-fallback', result };
    } catch (error: unknown) {
      if (!(error instanceof ApiError)) {
        state = {
          kind: 'dependency-error',
          code: 'dependency_unavailable',
          message: messages.dependency_unavailable,
        };
        return;
      }
      if (error.code === 'challenge_required' && error.challenge !== undefined) {
        state = {
          kind: 'challenge',
          challenge: error.challenge,
          message: messages.challenge_required,
        };
        return;
      }
      if (error.code === 'challenge_invalid' && error.challenge !== undefined) {
        state = {
          kind: 'challenge-error',
          challenge: error.challenge,
          message: messages.challenge_invalid,
        };
        return;
      }
      if (error.code === 'rate_limited') {
        state = {
          kind: 'rate-limited',
          code: error.code,
          message: messages.rate_limited,
          ...(error.retryAfterSeconds === undefined
            ? {}
            : { retryAfterSeconds: error.retryAfterSeconds }),
        };
        return;
      }
      const validationCodes: ErrorCode[] = [
        'invalid_request',
        'url_not_allowed',
        'alias_invalid',
        'alias_unavailable',
      ];
      if (validationCodes.includes(error.code)) {
        state = {
          kind: 'validation-error',
          code: error.code,
          message: messages[error.code],
        };
        return;
      }
      state = {
        kind: 'dependency-error',
        code: error.code,
        message: messages[error.code],
      };
    }
  }

  async function copyResult(): Promise<void> {
    if (state.kind !== 'success-copy-fallback' && state.kind !== 'success-copied') {
      return;
    }
    const copied = await copy(state.result.shortUrl);
    if (copied) {
      state = { kind: 'success-copied', result: state.result };
    }
  }

  function challengeError(): void {
    if (state.kind === 'challenge' || state.kind === 'challenge-error') {
      state = {
        kind: 'challenge-error',
        challenge: state.challenge,
        message: '验证服务暂时不可用，请稍后重试。',
      };
    }
  }
</script>

<svelte:head>
  <meta name="robots" content="noindex, nofollow" />
</svelte:head>

<div class="app-shell">
  <aside class="brand-stage" aria-labelledby="page-title">
    <BrandHeader {ready} />

    <div class="brand-content">
      <p class="brand-kicker">匿名短链 · 自托管</p>
      <h1 id="page-title" class="brand-wordmark" aria-label="myurl">
        <span>my</span>
        <span>url</span>
      </h1>
      <p class="brand-statement">把长链接，<br />收成一个清晰入口。</p>

      <dl class="brand-rules">
        <div>
          <dt>01</dt>
          <dd>无需账户</dd>
        </div>
        <div>
          <dt>02</dt>
          <dd>90 天自动失效</dd>
        </div>
        <div>
          <dt>03</dt>
          <dd>不记录原始地址</dd>
        </div>
      </dl>
    </div>

    <div class="brand-fold" aria-hidden="true">
      <span>short</span>
      <span>clear</span>
      <span>yours</span>
    </div>
  </aside>

  <main class="tool-stage">
    <div class="tool-meta" aria-hidden="true">
      <span>TOOL · 01</span>
      <span>LINK COMPRESSOR</span>
    </div>

    <section class="workspace" aria-label="短链接生成器">
      <header class="workspace-header">
        <div>
          <p class="workspace-index">短链接生成器</p>
          <h2>输入长链接，<br />生成短入口。</h2>
        </div>
        <span class="workspace-expiry">90 DAYS</span>
      </header>

      <LinkComposer
        bind:url
        bind:alias
        {aliasOpen}
        {submitting}
        {urlError}
        {aliasError}
        onSubmit={() => void submit()}
        onAliasToggle={toggleAlias}
      />

      {#if state.kind === 'challenge' || state.kind === 'challenge-error'}
        <div class="challenge-region" aria-live="polite">
          <p class="challenge-message">{state.message}</p>
          <TurnstileChallenge
            siteKey={state.challenge.siteKey}
            onToken={(token) => void submit(token)}
            onError={challengeError}
          />
        </div>
      {/if}

      {#if state.kind === 'success-copied' || state.kind === 'success-copy-fallback'}
        <LinkResult
          result={state.result}
          copied={state.kind === 'success-copied'}
          onCopy={() => void copyResult()}
        />
      {/if}

      {#if state.kind === 'validation-error' || state.kind === 'rate-limited' || state.kind === 'dependency-error'}
        <div
          class:error-state={state.kind !== 'rate-limited'}
          class:warning-state={state.kind === 'rate-limited'}
          class="feedback"
          role="status"
          aria-live="polite"
        >
          {#if state.kind === 'rate-limited'}
            <AlertTriangle size={18} aria-hidden="true" />
          {:else}
            <CircleX size={18} aria-hidden="true" />
          {/if}
          <span>{statusMessage}</span>
        </div>
      {/if}

      {#if state.kind === 'submitting'}
        <p class="sr-status" role="status" aria-live="polite">
          <LoaderCircle size={14} class="spin" aria-hidden="true" />正在生成短链接
        </p>
      {:else if state.kind === 'success-copied' || state.kind === 'success-copy-fallback'}
        <p class="sr-status" role="status" aria-live="polite">{statusMessage}</p>
      {/if}
    </section>

    <p class="tool-note">原始地址不会出现在公开短链中。</p>
    <ProjectFooter />
  </main>
</div>
