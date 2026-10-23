<script lang="ts">
  import { PAGE_SIZES } from "./browse.svelte";
  import { pageNumbers, type Page } from "./library";
  import Icon from "./Icon.svelte";

  // Below a grid: where the page sits in the list, the pages, and how many
  // cards a page holds. Only the page on screen makes its posters.
  let { shown, noun, pageSize, onpage, onsize }: {
    shown: Page<unknown>;
    noun: string;
    pageSize: number;
    onpage: (page: number) => void;
    onsize: (size: number) => void;
  } = $props();
</script>

<nav class="row" aria-label="Pagination">
  <span class="caption range">
    {shown.total ? `Showing ${shown.from}–${shown.to} of ${shown.total} ${noun}` : `No ${noun}`}</span>
  <span class="spacer"></span>
  <button class="step" aria-label="Previous page" disabled={shown.page <= 1}
          onclick={() => onpage(shown.page - 1)}><Icon name="prev" /></button>
  {#each pageNumbers(shown.page, shown.pages) as number, index (number ?? `gap${index}`)}
    {#if number === null}
      <span class="muted">…</span>
    {:else}
      <button class="number mono" class:current={number === shown.page}
              aria-current={number === shown.page ? "page" : undefined}
              onclick={() => onpage(number)}>{number}</button>
    {/if}
  {/each}
  <button class="step" aria-label="Next page" disabled={shown.page >= shown.pages}
          onclick={() => onpage(shown.page + 1)}><Icon name="next" /></button>
  <span class="spacer"></span>
  <label class="row caption size">Per page
    <select value={pageSize} onchange={(e) => onsize(Number(e.currentTarget.value))}>
      {#each PAGE_SIZES as size (size)}<option value={size}>{size}</option>{/each}
    </select>
  </label>
</nav>

<style>
  nav { padding: 8px 18px; border-top: 1px solid var(--line); background: var(--panel); flex-wrap: wrap; gap: 6px; }
  .range, .size { min-width: 200px; }
  .size { justify-content: flex-end; gap: 6px; }
  .step { padding: 4px 8px; line-height: 0; }
  .number { min-width: 30px; padding: 3px 8px; }
  .number.current { background: var(--accent); border-color: var(--accent); color: var(--accent-text); }
</style>
