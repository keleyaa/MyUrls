<script lang="ts">
  import { Check, Clipboard, Copy } from '@lucide/svelte';

  import type { CreateLinkResponse } from '../lib/api.js';

  export let result: CreateLinkResponse;
  export let copied: boolean;
  export let onCopy: () => void;
</script>

<section class="result" aria-label="短链接结果">
  <div class="result-topline">
    <span class="result-label">短链接</span>
    <span class="expiry">90 天后自动失效</span>
  </div>
  <button class="result-copy" type="button" on:click={onCopy} aria-label="复制短链接">
    <span class="result-code">{result.code}</span>
    <span class="result-url">{result.shortUrl}</span>
    {#if copied}
      <Check class="result-icon success-icon" size={20} aria-hidden="true" />
    {:else}
      <Copy class="result-icon" size={20} aria-hidden="true" />
    {/if}
  </button>
  <p class:copy-ok={copied} class="result-status" aria-live="polite">
    {#if copied}
      <Check size={14} aria-hidden="true" />
      <span>已生成并复制</span>
    {:else}
      <Clipboard size={14} aria-hidden="true" />
      <span>短链已生成 · 点击结果即可复制</span>
    {/if}
  </p>
</section>
