<script lang="ts">
  import { day, type SessionView } from "../api";
  import FlightsPane from "./FlightsPane.svelte";
  import { browse, show } from "./browse.svelte";
  import { placed } from "./library";
  import { runningTask } from "./tasks.svelte";
  import { segment } from "./sessions";

  // One session's flights. Its Files, detection profile, labels export and
  // the rest live in its menu in the sidebar.
  let { session, ffmpegOk, onchange }: {
    session: SessionView;
    ffmpegOk: boolean;
    onchange: () => void;
  } = $props();

  // The segmentation is a task: it outlives this pane when another session is
  // selected, and is found again on coming back.
  let segmentTask = $derived(runningTask(`segment:${session.id}`));
  let segmenting = $derived(segmentTask !== undefined);
  let status = $state<string | null>(null);

  let all = $derived(placed([session]));
  let items = $derived(browse.likedInSession ? all.filter((p) => p.flight.liked) : all);

  async function run() {
    status = null;
    status = await segment(session); // null: the viewer kept the flights
  }
</script>

<FlightsPane view="sessions" title={session.name} subtitle={day(session.date)} {items} showSession={false}
             {ffmpegOk} busy={segmenting} {onchange}>
  {#snippet toolbar()}
    <button class:active={browse.likedInSession} title="Show only liked flights" disabled={!all.length}
            onclick={() => show(() => (browse.likedInSession = !browse.likedInSession))}>
      {browse.likedInSession ? "♥" : "♡"} Liked</button>
    {#if session.segmentable && all.length}
      <button onclick={run} disabled={segmenting || !ffmpegOk}
              title="Find the flights in the DVR recordings again. Action-cam takes are left as they are.">
        ✂ {session.flights.some((f) => !f.actionCam) ? "Re-segment" : "Segment"}</button>
    {/if}
  {/snippet}
  {#snippet empty()}
    {#if !all.length}
      <div class="empty">
        <h2>No flights yet</h2>
        {#if session.segmentable}
          <p>Segment the recordings to find the flights in them.</p>
          <button class="primary" onclick={run} disabled={segmenting || !ffmpegOk}>Segment</button>
        {:else}
          <p>This session holds no recordings.</p>
        {/if}
      </div>
    {:else}
      <div class="empty"><h2>No liked flights</h2><p>Like a flight, or turn the filter off.</p></div>
    {/if}
  {/snippet}
  {#snippet footer()}
    {#if segmentTask || status}
      <footer class="row">
        {#if segmentTask?.total}
          <progress value={segmentTask.done} max={segmentTask.total} style:width="160px"></progress>
        {/if}
        <span class="caption">{segmentTask?.line ?? status}</span>
        <span class="spacer"></span>
        {#if !segmenting && status}<button class="plain small" onclick={() => (status = null)}>✕</button>{/if}
      </footer>
    {/if}
  {/snippet}
</FlightsPane>

<style>
  button.active { background: var(--selected); border-color: var(--accent); }
  footer { padding: 8px 18px; border-top: 1px solid var(--line); background: var(--panel); }
</style>
