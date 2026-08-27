<script lang="ts">
  import { Circle } from '@lucide/svelte';

  export let ready: boolean;
</script>

<header class="brand-header">
  <a class="brand-mark" href="/" aria-label="myurl 首页">myurl</a>
  <div class:offline={!ready} class="service-state" aria-live="polite">
    <Circle size={9} strokeWidth={0} fill="currentColor" aria-hidden="true" />
    <span>{ready ? '服务正常' : '服务异常'}</span>
  </div>
</header>
