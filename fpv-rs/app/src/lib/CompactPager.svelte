<script lang="ts">
  import type { Page } from "./library";
  import Icon from "./Icon.svelte";

  // A pager for narrow lists — the sidebar's sessions, a dialog's folders:
  // the range shown and a step either way. Nothing when one page holds all.
  let { shown, label, onpage }: {
    shown: Page<unknown>;
    /** What is listed, for screen readers: "Sessions". */
    label: string;
    onpage: (page: number) => void;
  } = $props();
</script>

{#if shown.pages > 1}
  <nav class="row" aria-label="{label} pages">
    <button class="plain step" aria-label="Previous page" disabled={shown.page <= 1}
            onclick={() => onpage(shown.page - 1)}><Icon name="prev" /></button>
    <span class="caption">{shown.from}–{shown.to} of {shown.total}</span>
    <button class="plain step" aria-label="Next page" disabled={shown.page >= shown.pages}
            onclick={() => onpage(shown.page + 1)}><Icon name="next" /></button>
  </nav>
{/if}

<style>
  nav { justify-content: center; gap: 4px; padding: 6px 0; }
  .step { padding: 4px 8px; line-height: 0; }
  .caption { min-width: 90px; text-align: center; }
</style>
