<script lang="ts">
  import type { Figure } from "./library";
  import FigureCard from "./FigureCard.svelte";
  import Pager from "./Pager.svelte";
  import PlayerDialog from "./PlayerDialog.svelte";
  import CloudWait from "./CloudWait.svelte";
  import { browse, setPageSize } from "./browse.svelte";
  import { paginate, sortFigures, type Sort } from "./library";

  // Every figure tagged in the library, one card each; filtered in the sidebar.
  let { items, subtitle, ffmpegOk, onchange }: {
    items: Figure[];
    subtitle: string;
    ffmpegOk: boolean;
    onchange: () => void;
  } = $props();

  let playing = $state<Figure | null>(null);
  /** A figure whose flight is only in the cloud, coming down before it plays. */
  let fetching = $state<Figure | null>(null);
  let content = $state<HTMLElement>();
  let shown = $derived(paginate(sortFigures(items, browse.sort.figures), browse.page, browse.pageSize));

  function goTo(page: number) {
    browse.page = page;
    content?.scrollTo({ top: 0 });
  }
</script>

<section>
  <header class="row">
    <div class="title"><h1>Figures</h1><span class="caption">{subtitle}</span></div>
    <span class="spacer"></span>
    {#if items.length}
      <label class="row caption sort">Sort
        <select value={browse.sort.figures} onchange={(e) => (browse.sort.figures = e.currentTarget.value as Sort)}>
          <option value="newest">Newest first</option>
          <option value="oldest">Oldest first</option>
          <option value="longest">Longest first</option>
        </select>
      </label>
    {/if}
  </header>
  <div class="content" bind:this={content}>
    {#if !items.length}
      <div class="empty">
        <h2>No figures here</h2>
        <p>Tag figures in the player — Mark In, name it, Enter — or change the filters.</p>
      </div>
    {:else}
      <div class="grid">
        {#each shown.items as figure (figure.tag.id)}
          <FigureCard {figure} {ffmpegOk} onplay={() => (figure.flight.cloud ? (fetching = figure) : (playing = figure))} />
        {/each}
      </div>
    {/if}
  </div>
  {#if items.length}
    <Pager {shown} noun="figures" pageSize={browse.pageSize} onpage={goTo} onsize={setPageSize} />
  {/if}
</section>

{#if fetching}
  {@const figure = fetching}
  <CloudWait flight={figure.flight} onready={() => { playing = figure; fetching = null; }}
             onclose={() => (fetching = null)} />
{/if}
{#if playing}
  <PlayerDialog flight={playing.flight} at={playing.tag.start}
                onclose={() => { playing = null; onchange(); }} />
{/if}

<style>
  section { display: flex; flex-direction: column; height: 100%; }
  header { padding: 12px 18px; border-bottom: 1px solid var(--line); background: var(--panel); flex-wrap: wrap; }
  .title { display: flex; align-items: baseline; gap: 10px; min-width: 0; }
  h1 { margin: 0; font-size: 18px; font-weight: 600; }
  .sort { gap: 6px; }
  .content { flex: 1; overflow: auto; }
  .grid { display: grid; grid-template-columns: repeat(auto-fill, minmax(200px, 1fr)); gap: 16px; padding: 16px 18px; }
</style>
