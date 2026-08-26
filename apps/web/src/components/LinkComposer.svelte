<script lang="ts">
  import { ChevronDown, Link2 } from '@lucide/svelte';

  export let url: string;
  export let alias: string;
  export let aliasOpen: boolean;
  export let submitting: boolean;
  export let urlError: string | undefined;
  export let aliasError: string | undefined;
  export let onSubmit: () => void;
  export let onAliasToggle: () => void;

  function submit(event: SubmitEvent): void {
    event.preventDefault();
    onSubmit();
  }
</script>

<form class="composer" on:submit={submit} novalidate>
  <div class="field-group">
    <label for="target-url">目标 URL</label>
    <input
      id="target-url"
      name="url"
      type="url"
      bind:value={url}
      placeholder="https://example.com/article"
      autocomplete="url"
      spellcheck="false"
      aria-invalid={urlError ? 'true' : 'false'}
      aria-describedby={urlError ? 'url-error' : 'url-hint'}
      disabled={submitting}
    />
    <div class="field-meta">
      <span id="url-hint">仅支持 HTTP(S) 地址</span>
      {#if urlError}<span id="url-error" class="field-error">{urlError}</span>{/if}
    </div>
  </div>

  {#if aliasOpen}
    <div class="field-group alias-field">
      <label for="custom-alias">自定义别名 <span>可选</span></label>
      <input
        id="custom-alias"
        name="alias"
        type="text"
        bind:value={alias}
        maxlength="32"
        placeholder="launch"
        autocomplete="off"
        spellcheck="false"
        aria-invalid={aliasError ? 'true' : 'false'}
        aria-describedby="alias-hint"
        disabled={submitting}
        on:input={() => (alias = alias.toLowerCase())}
      />
      <div class="field-meta">
        <span id="alias-hint">4–32 位小写字母、数字、_ 或 -</span>
        <span class:field-error={Boolean(aliasError)}>{aliasError ?? `${alias.length}/32`}</span>
      </div>
    </div>
  {/if}

  <div class="composer-actions">
    <button
      class="alias-toggle"
      type="button"
      on:click={onAliasToggle}
      aria-expanded={aliasOpen}
      disabled={submitting}
    >
      <span>{aliasOpen ? '收起别名' : '自定义别名'}</span>
      <ChevronDown size={16} class={aliasOpen ? 'rotated' : ''} aria-hidden="true" />
    </button>
    <button class="primary-action" type="submit" disabled={submitting}>
      {#if submitting}
        <span class="spinner" aria-hidden="true"></span>
        <span>生成中</span>
      {:else}
        <Link2 size={18} aria-hidden="true" />
        <span>生成并复制</span>
      {/if}
    </button>
  </div>
</form>
