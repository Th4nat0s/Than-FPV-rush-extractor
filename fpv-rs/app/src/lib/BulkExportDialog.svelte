<script lang="ts">
  import { onMount } from "svelte";
  import { open } from "@tauri-apps/plugin-dialog";
  import { api, message, onProgress, type BulkExport, type FlightView } from "../api";
  import Icon from "./Icon.svelte";
  import Modal from "./Modal.svelte";

  // Several flights into one folder. Each keeps the single export's rules —
  // lossless, frame-exact, never overwriting — and the one question a batch
  // cannot ask flight by flight is asked here, once.
  let { flights, onclose }: { flights: FlightView[]; onclose: () => void } = $props();

  let folder = $state<string | null>(null);
  let onMismatch = $state<"parts" | "reencode">("parts");
  let running = $state(false);
  let done = $state(0);
  let line = $state("");
  let result = $state<BulkExport | null>(null);
  let failure = $state<string | null>(null);

  onMount(() => {
    const unlisten = onProgress("export-progress", (progress) => {
      done = progress.done;
      line = progress.line;
    });
    return () => { unlisten.then((stop) => stop()); };
  });

  async function choose() {
    const picked = await open({ directory: true, title: "Export the flights into" });
    if (typeof picked === "string") folder = picked;
  }

  async function start() {
    if (!folder) return;
    running = true;
    try {
      result = await api.exportFlights(flights.map((f) => f.id), folder, onMismatch);
    } catch (e) {
      failure = message(e);
    } finally {
      running = false;
    }
  }

  const name = (path: string) => path.split(/[\\/]/).pop();
</script>

<Modal title="Export {flights.length} flight{flights.length === 1 ? '' : 's'}" width={540}
       onclose={running ? undefined : onclose}>
  {#if !result && !failure}
    <p class="caption">Each flight becomes “Session — Title” in the folder: lossless and frame-exact
      for the DVR, the take as recorded for action cams. Existing files are never overwritten.</p>

    <div class="row destination">
      <Icon name="folder" />
      <span class="path mono"><bdi>{folder ?? "No folder chosen"}</bdi></span>
      <button class="small" onclick={choose} disabled={running}>Choose…</button>
    </div>

    <fieldset disabled={running}>
      <legend class="caption">A flight whose recordings disagree on their frame rate:</legend>
      <label class="row"><input type="radio" bind:group={onMismatch} value="parts" />
        One file per recording, lossless</label>
      <label class="row"><input type="radio" bind:group={onMismatch} value="reencode" />
        One file, re-encoded at the main part's rate (DVR only)</label>
    </fieldset>

    {#if running}
      <progress value={done} max={flights.length}></progress>
      <p class="caption">{line}</p>
    {/if}
  {:else if result}
    <p><strong>{result.written.length} file{result.written.length === 1 ? "" : "s"} written</strong>
      {#if result.failed.length}, {result.failed.length} flight{result.failed.length === 1 ? "" : "s"} failed{/if}</p>
    <div class="log mono">
      {#each result.written as path}<div>{name(path)}</div>{/each}
      {#each result.failed as [flight, reason]}<div class="error">FAILED {flight}: {reason}</div>{/each}
    </div>
  {:else}
    <p class="error">Export failed: {failure}</p>
  {/if}

  {#snippet footer()}
    {#if result || failure}
      <button class="primary" onclick={onclose}>Done</button>
    {:else}
      <button onclick={onclose} disabled={running}>Cancel</button>
      <button class="primary" onclick={start} disabled={running || !folder}>
        {running ? "Exporting…" : `Export ${flights.length}`}
      </button>
    {/if}
  {/snippet}
</Modal>

<style>
  .destination {
    margin: 10px 0;
    padding: 6px 8px;
    border: 1px solid var(--line);
    border-radius: 6px;
    background: var(--bg);
  }
  .path {
    flex: 1;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    font-size: 12px;
    direction: rtl;
    text-align: left;
  }
  fieldset { border: none; padding: 0; margin: 0 0 10px; min-width: 0; display: flex; flex-direction: column; gap: 4px; }
  legend { padding: 0; margin-bottom: 4px; }
  .log {
    max-height: 200px;
    overflow: auto;
    font-size: 12px;
    background: var(--bg);
    border: 1px solid var(--line);
    border-radius: 6px;
    padding: 6px 8px;
  }
  .error { color: var(--danger); }
</style>
