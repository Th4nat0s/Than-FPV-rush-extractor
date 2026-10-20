<script lang="ts">
  import { onMount, tick, untrack } from "svelte";
  import { api, message, onProgress, type ImportSummary, type Info } from "../api";
  import MediaRoot from "./MediaRoot.svelte";
  import { track } from "./tasks.svelte";
  import Modal from "./Modal.svelte";

  let { folder, suggestedName, existing, root, onrootchange, onclose }: {
    folder: string;
    suggestedName: string;
    /** Session names already in the library: importing under one joins it. */
    existing: string[];
    /** Where the copies go: the import folder setting. */
    root: string;
    onrootchange: (info: Info) => void;
    onclose: () => void;
  } = $props();

  const today = new Date();
  // A suggestion: seeded once, then it is the user's.
  let name = $state(untrack(() => suggestedName));
  // The analog DVR has no clock, so its date can only come from the pilot.
  // Unticked, an action-cam session dates itself from its first take.
  let hasDate = $state(true);
  let date = $state(
    `${today.getFullYear()}-${String(today.getMonth() + 1).padStart(2, "0")}-${String(today.getDate()).padStart(2, "0")}`);
  let deleteOriginals = $state(false);
  let running = $state(false);
  let log = $state<string[]>([]);
  let done = $state(0);
  let total = $state(0);
  let summary = $state<ImportSummary | null>(null);
  let failure = $state<string | null>(null);
  let logBox: HTMLDivElement | undefined = $state();
  let taskId: string | null = null;

  let joins = $derived(existing.includes(name.trim()));

  onMount(() => {
    const unlisten = onProgress("import-progress", async (progress) => {
      if (progress.task !== taskId) return;
      log.push(progress.line);
      done = progress.done;
      total = progress.total;
      await tick();
      logBox?.scrollTo({ top: logBox.scrollHeight });
    });
    return () => { unlisten.then((stop) => stop()); };
  });

  async function start() {
    running = true;
    try {
      // A task, so the window can close and the import go on in the task bar.
      summary = await track(`Import “${name.trim()}”`, (id) => {
        taskId = id;
        return api.importCard(id, folder, name.trim(), hasDate ? date : null, deleteOriginals);
      }, { key: "import", summary: (s) => `${s.imported} imported, ${s.skipped} skipped, ${s.failed} failed` });
    } catch (e) {
      failure = `Import failed: ${message(e)}`;
    } finally {
      running = false;
    }
  }

  let finished = $derived(summary !== null || failure !== null);
</script>

<Modal title="Import recordings" width={520} {onclose}>
  <p class="caption path">{folder}</p>

  <fieldset disabled={running || finished}>
    <label>Session name
      <input type="text" bind:value={name} />
    </label>
    {#if joins}
      <p class="caption">Joins the existing session of that name — action-cam takes land beside its flights.</p>
    {/if}
    <label class="row"><input type="checkbox" bind:checked={hasDate} /> Flying date</label>
    {#if hasDate}
      <input type="date" bind:value={date} />
    {:else}
      <p class="caption">The DVR has no clock: without a date its session stays undated. Action-cam sessions take their first take's time.</p>
    {/if}
    <div class="destination">
      <span>Copy to</span>
      <MediaRoot {root} disabled={running || finished} onchange={onrootchange} />
      <span class="caption">Each session gets its own folder in there. The choice is kept for later imports.</span>
    </div>
    <label class="row" title="Only files imported and size-verified in this run are removed. Skipped files always stay.">
      <input type="checkbox" bind:checked={deleteOriginals} /> Delete from card after import
    </label>
  </fieldset>

  {#if running && total > 0}<progress value={done} max={total}></progress>{/if}

  {#if log.length}
    <div class="log mono" bind:this={logBox}>
      {#each log as line}<div>{line}</div>{/each}
    </div>
  {/if}

  {#if summary}
    <p class="summary">
      {summary.imported} imported, {summary.skipped} skipped, {summary.failed} failed{#if summary.flights}, {summary.flights} action-cam flight(s){/if}{#if deleteOriginals} — {summary.deletedFromCard} deleted from card{/if}
    </p>
    {#each summary.kept as kept}<p class="caption">KEPT {kept}</p>{/each}
  {/if}
  {#if failure}<p class="summary error">{failure}</p>{/if}

  {#snippet footer()}
    {#if finished}
      <button class="primary" onclick={onclose}>Done</button>
    {:else}
      <button onclick={onclose}>{running ? "Continue in background" : "Cancel"}</button>
      <button class="primary" onclick={start} disabled={running || !name.trim()}>
        {running ? "Importing…" : "Import"}
      </button>
    {/if}
  {/snippet}
</Modal>

<style>
  .path { margin-top: 0; word-break: break-all; }
  /* A fieldset is as wide as its content by default; the destination path would push it out. */
  fieldset { border: none; padding: 0; margin: 0; min-width: 0; display: flex; flex-direction: column; gap: 8px; }
  label { display: flex; flex-direction: column; gap: 4px; }
  label.row { flex-direction: row; }
  .destination { display: flex; flex-direction: column; gap: 4px; }
  .log {
    margin-top: 12px;
    height: 150px;
    overflow: auto;
    font-size: 12px;
    background: var(--bg);
    border: 1px solid var(--line);
    border-radius: 6px;
    padding: 6px 8px;
  }
  .summary { font-weight: 600; }
  .error { color: var(--danger); }
</style>
