<script lang="ts">
  import { pageNumbers, type Page } from "./library";
  import Icon from "./Icon.svelte";

  // A pager for narrow lists — the sidebar's sessions, a dialog's folders:
  // a step either way and the page numbers between, the range shown on hover.
  // Nothing when one page holds all.
  let { shown, label, onpage }: {
    shown: Page<unknown>;
    /** What is listed, for screen readers: "Sessions". */
    label: string;
    onpage: (page: number) => void;
  } = $props();
</script>

{#if shown.pages > 1}
  <nav class="row" aria-label="{label} pages" title="{shown.from}–{shown.to} of {shown.total}">
    <button class="plain step" aria-label="Previous page" disabled={shown.page <= 1}
            onclick={() => onpage(shown.page - 1)}><Icon name="prev" /></button>
    {#each pageNumbers(shown.page, shown.pages) as number, index (number ?? `gap${index}`)}
      {#if number === null}
        <span class="caption gap">…</span>
      {:else}
        <button class="plain number mono" class:current={number === shown.page}
                aria-current={number === shown.page ? "page" : undefined}
                onclick={() => onpage(number)}>{number}</button>
      {/if}
    {/each}
    <button class="plain step" aria-label="Next page" disabled={shown.page >= shown.pages}
            onclick={() => onpage(shown.page + 1)}><Icon name="next" /></button>
  </nav>
{/if}

<style>
  nav { justify-content: center; gap: 2px; padding: 6px 0; }
  .step { padding: 4px 6px; line-height: 0; }
  .number { min-width: 24px; padding: 2px 5px; font-size: 12px; border-radius: var(--radius); }
  .number.current { background: var(--accent); color: var(--accent-text); }
  .gap { padding: 0 2px; }
</style>
