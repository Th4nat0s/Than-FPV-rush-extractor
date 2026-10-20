<script lang="ts">
  import { save } from "@tauri-apps/plugin-dialog";
  import { api, clock, day, flightName, message, type FlightView,
           type SessionView } from "../api";
  import FlightCard from "./FlightCard.svelte";
  import BulkExportDialog from "./BulkExportDialog.svelte";
  import Icon from "./Icon.svelte";
  import PlayerDialog from "./PlayerDialog.svelte";
  import EditorDialog from "./EditorDialog.svelte";
  import Modal from "./Modal.svelte";
  import { runningTask, track } from "./tasks.svelte";

  let { session, ffmpegOk, onchange }: {
    session: SessionView;
    ffmpegOk: boolean;
    onchange: () => void;
  } = $props();

  let tab = $state<"flights" | "files">("flights");
  let likedOnly = $state(false);
  // Selection mode, for exporting several flights at once.
  let selecting = $state(false);
  let picked = $state<string[]>([]);
  let bulk = $state<FlightView[] | null>(null);
  // The segmentation is a task: it outlives this pane when another session is
  // selected, and is found again on coming back.
  let segmentTask = $derived(runningTask(`segment:${session.id}`));
  let segmenting = $derived(segmentTask !== undefined);
  let status = $state<string | null>(null);
  let exporting = $state(false);
  let notice = $state<string | null>(null);
  let playing = $state<FlightView | null>(null);
  let editing = $state<FlightView | null>(null);
  let conflict = $state<{ flight: FlightView; destination: string; reason: string } | null>(null);

  let visible = $derived(likedOnly ? session.flights.filter((f) => f.liked) : session.flights);
  let pickedFlights = $derived(session.flights.filter((f) => picked.includes(f.id)));

  function toggle(id: string) {
    picked = picked.includes(id) ? picked.filter((p) => p !== id) : [...picked, id];
  }

  function stopSelecting() {
    selecting = false;
    picked = [];
  }

  async function segment() {
    status = null;
    try {
      status = await track(`Segment “${session.name}”`, (id) => api.segment(id, session.id),
                           { key: `segment:${session.id}`, summary: (s) => s });
    } catch (e) {
      status = `Segmentation failed: ${message(e)}`;
    }
  }

  async function changeProfile(event: Event) {
    const profile = (event.currentTarget as HTMLSelectElement).value;
    try {
      await api.setDetectionProfile(session.id, profile);
      status = session.flights.some((f) => !f.actionCam)
        ? "Detection profile changed — Re-segment to apply it."
        : "Detection profile changed — Segment to apply it.";
    } catch (e) {
      status = `Cannot change the detection profile: ${message(e)}`;
    }
    onchange();
  }

  async function exportFlight(flight: FlightView) {
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

  async function exportLabels() {
    const destination = await save({
      title: "Export labels", defaultPath: "labels.csv",
      filters: [{ name: "CSV", extensions: ["csv"] }],
    });
    if (!destination) return;
    try {
      const rows = await api.exportLabels(session.id, destination);
      notice = `Wrote ${rows} label row(s)`;
    } catch (e) {
      notice = `Cannot write labels: ${message(e)}`;
    }
  }
</script>

<section>
  <header class="row">
    <div class="title">
      <h1>{session.name}</h1>
      {#if day(session.date)}<span class="caption">{day(session.date)}</span>{/if}
    </div>
    <span class="spacer"></span>
    <div class="tabs">
      <button class:active={tab === "flights"} onclick={() => (tab = "flights")}>Flights</button>
      <button class:active={tab === "files"} onclick={() => (tab = "files")}>Files</button>
    </div>
    {#if tab === "flights" && session.flights.length}
      {#if selecting}
        <button onclick={() => (picked = visible.map((f) => f.id))}>Select all</button>
        <button onclick={() => (picked = [])} disabled={!picked.length}>None</button>
        <button class="primary" onclick={() => (bulk = pickedFlights)}
                disabled={!picked.length || exporting || !ffmpegOk}>
          <Icon name="export" /> Export {picked.length}…</button>
        <button onclick={stopSelecting}>Cancel</button>
      {:else}
        <button onclick={() => (selecting = true)} title="Pick several flights to export together">Select</button>
      {/if}
    {/if}
    {#if !selecting}
      <button class:active={likedOnly} title="Show only liked flights" disabled={tab !== "flights"}
              onclick={() => (likedOnly = !likedOnly)}>{likedOnly ? "♥" : "♡"} Liked</button>
      {#if session.segmentable}
        <select value={session.detectionProfile} disabled={segmenting} onchange={changeProfile}
                aria-label="Detection profile"
                title="Standard: the tuned thresholds. Textured terrain: for footage whose clean picture reads as snow — rock faces, alpine slopes, softer cameras — and to absorb dropouts under 1 s.">
          <option value="standard">Detection: Standard</option>
          <option value="textured">Detection: Textured terrain</option>
        </select>
        <button onclick={segment} disabled={segmenting || !ffmpegOk}
                title="Find the flights in the DVR recordings. Action-cam takes are left as they are.">
          ✂ {session.flights.some((f) => !f.actionCam) ? "Re-segment" : "Segment"}
        </button>
        <button onclick={exportLabels} disabled={!session.flights.length}
                title="Write the clip boundaries as ground truth for the detector's tuning harness">Export Labels…</button>
      {/if}
    {/if}
  </header>

  <div class="content">
    {#if tab === "files"}
      <table>
        <thead><tr><th>File</th><th>Source</th><th>Size</th><th>Rate</th><th>Duration</th></tr></thead>
        <tbody>
          {#each session.files as file (file.id)}
            <tr>
              <td>{file.name}</td>
              <td>{file.actionCam ? `Action cam (${file.codec})` : "DVR"}</td>
              <td class="mono">{file.width}×{file.height}</td>
              <!-- Per file and non-standard by design (CLAUDE.md 2). -->
              <td class="mono">{file.fps.toFixed(3)} fps</td>
              <td class="mono">{clock(file.duration)}</td>
            </tr>
          {/each}
        </tbody>
      </table>
    {:else if !session.flights.length}
      <div class="empty">
        <h2>No flights yet</h2>
        {#if session.segmentable}
          <p>Segment the recordings to find the flights in them.</p>
          <button class="primary" onclick={segment} disabled={segmenting || !ffmpegOk}>Segment</button>
        {:else}
          <p>This session holds no recordings.</p>
        {/if}
      </div>
    {:else if !visible.length}
      <div class="empty"><h2>No liked flights</h2><p>Like a flight, or turn the filter off.</p></div>
    {:else}
      <div class="grid">
        {#each visible as flight (flight.id)}
          <FlightCard {flight} {exporting} {ffmpegOk} onchange={onchange} {selecting}
                      selected={picked.includes(flight.id)} ontoggle={() => toggle(flight.id)}
                      onplay={() => (playing = flight)} onedit={() => (editing = flight)}
                      onexport={() => exportFlight(flight)} />
        {/each}
      </div>
    {/if}
  </div>

  {#if segmentTask || status || exporting}
    <footer class="row">
      {#if segmentTask?.total}
        <progress value={segmentTask.done} max={segmentTask.total} style:width="160px"></progress>
      {/if}
      <span class="caption">{exporting ? "Exporting…" : segmentTask?.line ?? status}</span>
      <span class="spacer"></span>
      {#if !segmenting && status}<button class="plain small" onclick={() => (status = null)}>✕</button>{/if}
    </footer>
  {/if}
</section>

{#if bulk}
  <BulkExportDialog flights={bulk} onclose={() => { bulk = null; stopSelecting(); }} />
{/if}
{#if playing}
  <PlayerDialog flight={playing} onclose={() => (playing = null)} />
{/if}
{#if editing}
  <EditorDialog flight={editing} onclose={() => { editing = null; onchange(); }} />
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
  .tabs { display: flex; }
  .tabs button { border-radius: 0; }
  .tabs button:first-child { border-radius: 6px 0 0 6px; }
  .tabs button:last-child { border-radius: 0 6px 6px 0; border-left: none; }
  button.active { background: var(--selected); border-color: var(--accent); }
  .content { flex: 1; overflow: auto; }
  .grid {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(230px, 1fr));
    gap: 16px;
    padding: 16px 18px;
  }
  table { width: 100%; border-collapse: collapse; }
  th, td { text-align: left; padding: 7px 18px; border-bottom: 1px solid var(--line); }
  th { font-weight: 500; color: var(--muted); font-size: 12px; position: sticky; top: 0; background: var(--bg); }
  footer { padding: 8px 18px; border-top: 1px solid var(--line); background: var(--panel); }
</style>
