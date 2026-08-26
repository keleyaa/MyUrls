<script lang="ts">
  import { onMount } from 'svelte';
  import { ShieldCheck } from '@lucide/svelte';

  export let siteKey: string;
  export let onToken: (token: string) => void;
  export let onError: () => void;

  let container: HTMLDivElement;

  function loadScript(): Promise<void> {
    if (window.turnstile !== undefined) {
      return Promise.resolve();
    }
    const existing = document.querySelector<HTMLScriptElement>('script[data-turnstile]');
    if (existing !== null) {
      return new Promise((resolve, reject) => {
        existing.addEventListener('load', () => resolve(), { once: true });
        existing.addEventListener('error', () => reject(new Error('turnstile unavailable')), {
          once: true,
        });
      });
    }
    return new Promise((resolve, reject) => {
      const script = document.createElement('script');
      script.dataset.turnstile = 'true';
      script.src = 'https://challenges.cloudflare.com/turnstile/v0/api.js?render=explicit';
      script.async = true;
      script.defer = true;
      script.onload = () => resolve();
      script.onerror = () => reject(new Error('turnstile unavailable'));
      document.head.appendChild(script);
    });
  }

  onMount(() => {
    let widgetId: string | undefined;
    let disposed = false;
    void loadScript()
      .then(() => {
        if (disposed || window.turnstile === undefined) {
          return;
        }
        widgetId = window.turnstile.render(container, {
          sitekey: siteKey,
          action: 'create_link',
          callback: (token) => onToken(token),
          'error-callback': onError,
          'expired-callback': onError,
        });
      })
      .catch(() => onError());

    return () => {
      disposed = true;
      if (widgetId !== undefined) {
        window.turnstile?.reset?.(widgetId);
      }
    };
  });
</script>

<div class="challenge-box">
  <div class="challenge-heading">
    <ShieldCheck size={18} aria-hidden="true" />
    <span>请完成验证</span>
  </div>
  <div bind:this={container} class="turnstile-host" aria-label="Turnstile verification"></div>
</div>
