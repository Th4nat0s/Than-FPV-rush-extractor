<script lang="ts">
  import { onMount } from "svelte";
  import { api, bytes, message, type ReducePreview, type SessionView } from "../api";
  import Modal from "./Modal.svelte";

  // "Reduce…": what cutting a session's DVR recordings down to their flights
  // would free, and what it gives up for good, before anything is touched.
  let { session, onreduce, onclose }: {
    session: SessionView;
    onreduce: () => void;
    onclose: () => void;
  } = $props();

  let preview = $state<ReducePreview | null>(null);
  let problem = $state<string | null>(null);
  const plural = (n: number, word: string) => `${n} ${word}${n === 1 ? "" : "s"}`;

  onMount(() => {
    api.reducePreview(session.id).then((p) => (preview = p)).catch((e) => (problem = message(e)));
  });
</script>

<Modal title="Reduce “{session.name}”?" width={540} {onclose}>
  {#if problem}
    <p class="error">{problem}</p>
  {:else if !preview}
    <p class="caption">Measuring…</p>
  {:else if !preview.recordings}
    <p>Nothing to reduce: every recording is already cut down to its flights, or holds flights from end to end.</p>
  {:else}
    <p>Each DVR recording is cut down to its flights, 5 s kept on either side, losslessly; the
      snow between them goes.</p>
    <p class="saving"><span class="mono">{bytes(preview.bytesNow)}</span> → <span class="mono">{bytes(preview.bytesAfter)}</span>
      <span class="caption">— about {bytes(preview.bytesNow - preview.bytesAfter)} freed, over
        {plural(preview.recordings, "recording")}</span></p>
    <p class="warning">This cannot be undone.</p>
    <ul>
      <li>A flight the detector missed is lost with the snow.</li>
      <li>The session can no longer be re-segmented, and a boundary can only move within what is kept.</li>
      {#if preview.emptied}<li>{plural(preview.emptied, "recording")} without a flight {preview.emptied === 1 ? "goes" : "go"} whole.</li>{/if}
      {#if preview.figuresLost}<li>{plural(preview.figuresLost, "figure tag")} in the snow {preview.figuresLost === 1 ? "is" : "are"} lost.</li>{/if}
    </ul>
    <p class="caption">Titles, likes, tags and figures on the flights stay. The original recordings go to
      the trash: the space is freed when it is emptied.{#if preview.unsegmented}
        {plural(preview.unsegmented, "recording")} never segmented {preview.unsegmented === 1 ? "is" : "are"} left as {preview.unsegmented === 1 ? "it is" : "they are"}.{/if}</p>
  {/if}
  {#snippet footer()}
    <button onclick={onclose}>Cancel</button>
    <button class="primary danger-fill" disabled={!preview?.recordings} onclick={onreduce}>Reduce</button>
  {/snippet}
</Modal>

<style>
  .saving { font-size: 15px; }
  .warning { font-weight: 600; color: var(--warn); margin-bottom: 4px; }
  ul { margin: 0 0 8px; padding-left: 20px; }
  .error { color: var(--danger); }
  .danger-fill { background: var(--danger); border-color: var(--danger); }
  .danger-fill:hover:not(:disabled) { background: var(--danger); }
</style>
