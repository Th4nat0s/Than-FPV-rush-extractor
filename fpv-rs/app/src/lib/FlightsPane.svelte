<script lang="ts">
  import type { Snippet } from "svelte";
  import { save } from "@tauri-apps/plugin-dialog";
  import { api, flightName, message, type FlightView } from "../api";
  import FlightCard from "./FlightCard.svelte";
  import BulkExportDialog from "./BulkExportDialog.svelte";
  import Icon from "./Icon.svelte";
  import PlayerDialog from "./PlayerDialog.svelte";
  import EditorDialog from "./EditorDialog.svelte";
  import Modal from "./Modal.svelte";
  import Pager from "./Pager.svelte";
  import DeleteFlightDialog from "./DeleteFlightDialog.svelte";
  import { browse, setPageSize } from "./browse.svelte";
  import { paginate, sortFlights, type Placed, type Sort, type View } from "./library";
  import { track } from "./tasks.svelte";
  import { prepareStrips } from "./strips";

  // A grid of flights, a page at a time: one session's, or the whole
  // library's. What is listed is the caller's choice; how it is sorted, paged,
  // played, edited and exported is the same everywhere.
  let { view, title, subtitle = null, items, showSession, ffmpegOk, busy = false, onchange,
        toolbar, empty, footer }: {
    view: View;
    title: string;
    subtitle?: string | null;
    items: Placed[];
    /** Name each card's session: the list spans several. */
    showSession: boolean;
    ffmpegOk: boolean;
    /** Cards that may name flights about to be replaced: shown, not usable. */
    busy?: boolean;
    onchange: () => void;
    toolbar?: Snippet;
    /** What shows when nothing is listed. */
    empty: Snippet;
    footer?: Snippet;
  } = $props();

  // Selection mode, for exporting several flights at once.
  let selecting = $state(false);
  let picked = $state<string[]>([]);
  let bulk = $state<FlightView[] | null>(null);
  let exporting = $state(false);
  let notice = $state<string | null>(null);
  let playing = $state<FlightView | null>(null);
  let editing = $state<Placed | null>(null);
  let deleting = $state<FlightView | null>(null);
  let conflict = $state<{ flight: FlightView; destination: string; reason: string } | null>(null);
  let content = $state<HTMLElement>();

  let sorted = $derived(sortFlights(items, browse.sort[view]));
  let shown = $derived(paginate(sorted, browse.page, browse.pageSize));
  let pickedFlights = $derived(items.filter((p) => picked.includes(p.flight.id)).map((p) => p.flight));

  function toggle(id: string) {
    picked = picked.includes(id) ? picked.filter((p) => p !== id) : [...picked, id];
  }

  function stopSelecting() {
    selecting = false;
    picked = [];
  }

  function goTo(page: number) {
    browse.page = page;
    content?.scrollTo({ top: 0 });
  }

  async function exportFlight({ flight, session }: Placed) {
    const extension = flight.actionCam ? "mp4" : "avi";
    const destination = await save({
      title: "Export flight",
      defaultPath: `${session.name} — ${flightName(flight)}`.replaceAll("/", "-") + `.${extension}`,
      filters: [{ name: extension.toUpperCase(), extensions: [extension] }],
    });
    if (destination) await runExport(flight, destination, "lossless");
  }

  async function runExport(flight: FlightView, destination: string,
                           mode: "lossless" | "parts" | "reencode") {
    exporting = true;
    conflict = null;
    try {
      const outcome = await track(`Export ${flightName(flight)}`, () => api.exportFlight(flight.id, destination, mode),
                                  { summary: (o) => o.kind === "mismatch" ? "Waiting for a choice" : "Written" });
      if (outcome.kind === "mismatch") {
        // Never write a file that looks fine and plays its second half at
        // the wrong speed: the user chooses what happens instead.
        conflict = { flight, destination, reason: outcome.reason };
      } else {
        const names = outcome.files.map((f) => f.split(/[\\/]/).pop()).join(", ");
        notice = `Exported ${names}${mode === "reencode" ? " (re-encoded)" : ""}`;
      }
    } catch (e) {
      notice = `Export failed: ${message(e)}`;
    } finally {
      exporting = false;
    }
  }
</script>

<section>
  <header class="row">
    <div class="title">
      <h1>{title}</h1>
      {#if subtitle}<span class="caption">{subtitle}</span>{/if}
    </div>
    <span class="spacer"></span>
    {#if items.length}
      <label class="row caption sort">Sort
        <select value={browse.sort[view]} onchange={(e) => (browse.sort[view] = e.currentTarget.value as Sort)}>
          <option value="newest">Newest first</option>
          <option value="oldest">Oldest first</option>
          <option value="longest">Longest first</option>
        </select>
      </label>
      {#if selecting}
        <button onclick={() => (picked = sorted.map((p) => p.flight.id))}>Select all</button>
        <button onclick={() => (picked = [])} disabled={!picked.length}>None</button>
        <button class="primary" onclick={() => (bulk = pickedFlights)}
                disabled={!picked.length || exporting || !ffmpegOk}>
          <Icon name="export" /> Export {picked.length}…</button>
        <button onclick={stopSelecting}>Cancel</button>
      {:else}
        <button onclick={() => (selecting = true)} title="Pick several flights to export together">Select</button>
      {/if}
    {/if}
    {#if !selecting}{@render toolbar?.()}{/if}
  </header>

  <div class="content" bind:this={content}>
    {#if !items.length}
      {@render empty()}
    {:else}
      {#if busy}
        <p class="caption busy-note">Re-segmenting — the flights below are replaced when it ends.</p>
      {/if}
      <div class="grid" class:busy inert={busy}>
        {#each shown.items as entry (entry.flight.id)}
          <FlightCard flight={entry.flight} session={showSession ? entry.session : null} sessionLabels={entry.session.labels}
                      {exporting} {ffmpegOk} {onchange} {selecting}
                      selected={picked.includes(entry.flight.id)} ontoggle={() => toggle(entry.flight.id)}
                      onplay={() => (playing = entry.flight)} onedit={() => (editing = entry)}
                      onexport={() => exportFlight(entry)} ondelete={() => (deleting = entry.flight)} />
        {/each}
      </div>
    {/if}
  </div>

  {#if items.length}
    <Pager {shown} noun="flights" pageSize={browse.pageSize} onpage={goTo} onsize={setPageSize} />
  {/if}
  {@render footer?.()}
</section>

{#if bulk}
  <BulkExportDialog flights={bulk} onclose={() => { bulk = null; stopSelecting(); }} />
{/if}
{#if playing}
  <PlayerDialog flight={playing} onclose={() => { playing = null; onchange(); }} />
{/if}
{#if editing}
  {@const edited = editing}
  <!-- A boundary edit re-keys the flight's strip: it is remade straight away. -->
  <EditorDialog flight={edited.flight}
                onclose={() => { editing = null; onchange(); prepareStrips(edited.session.id); }} />
{/if}
{#if deleting}
  <DeleteFlightDialog flight={deleting} onclose={() => (deleting = null)}
                      ondeleted={() => { deleting = null; onchange(); }} />
{/if}
{#if conflict}
  {@const pending = conflict}
  <Modal title="Different frame rates" width={520} onclose={() => (conflict = null)}>
    <p>{pending.reason}.</p>
    <p class="muted">Each part can be exported losslessly to its own file, or the flight
      re-encoded into one file at the rate of its longest part.</p>
    {#snippet footer()}
      <button onclick={() => (conflict = null)}>Cancel</button>
      <button onclick={() => runExport(pending.flight, pending.destination, "parts")}>
        Export {pending.flight.recordings} parts separately</button>
      <button class="primary" onclick={() => runExport(pending.flight, pending.destination, "reencode")}>
        Re-encode into one file</button>
    {/snippet}
  </Modal>
{/if}
{#if notice}
  <Modal title="Export" onclose={() => (notice = null)}>
    <p>{notice}</p>
    {#snippet footer()}<button class="primary" onclick={() => (notice = null)}>OK</button>{/snippet}
  </Modal>
{/if}

<style>
  section { display: flex; flex-direction: column; height: 100%; }
  header {
    padding: 12px 18px;
    border-bottom: 1px solid var(--line);
    background: var(--panel);
    flex-wrap: wrap;
  }
  .title { display: flex; align-items: baseline; gap: 10px; min-width: 0; }
  h1 { margin: 0; font-size: 18px; font-weight: 600; white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
  .sort { gap: 6px; }
  .content { flex: 1; overflow: auto; }
  .grid {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(230px, 1fr));
    gap: 16px;
    padding: 16px 18px;
  }
  .grid.busy { opacity: 0.4; }
  .busy-note { margin: 12px 18px 0; }
</style>
