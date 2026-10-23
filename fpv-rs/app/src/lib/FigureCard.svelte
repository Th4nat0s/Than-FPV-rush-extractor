<script lang="ts">
  import { api, clock, day, flightName, media } from "../api";
  import { batch } from "./tasks.svelte";
  import { STRIP } from "./strips";
  import type { Figure } from "./library";

  // One figure tag as a card: the picture nearest its moment, its name and
  // length, and where it is. A click plays it.
  let { figure, ffmpegOk, onplay }: { figure: Figure; ffmpegOk: boolean; onplay: () => void } = $props();

  let tag = $derived(figure.tag);
  let flight = $derived(figure.flight);
  // The hover strip's frame nearest the figure: made ahead for every flight,
  // so this costs nothing once the strips job has run. The flight's poster
  // stands in meanwhile.
  let frame = $state<string | undefined>();
  let poster = $derived(frame ?? media(flight.clips[0]?.thumbnail));

  $effect(() => {
    if (!ffmpegOk || !flight.duration) return;
    const index = Math.min(STRIP - 1, Math.floor((tag.start / flight.duration) * STRIP));
    batch(`strip:${flight.id}`, `Hover strip of ${flightName(flight)}`,
          () => api.filmstripFrame(flight.id, index, STRIP))
      .then((made) => (frame = media(made.path))).catch(() => {});
  });
</script>

<article>
  <button class="poster plain" onclick={onplay} aria-label="Play {tag.label} in {flightName(flight)}">
    {#if poster}<img src={poster} alt="" draggable="false" />{/if}
    <span class="name">{tag.label}</span>
    {#if !tag.marker}<span class="length mono">{(tag.end - tag.start).toFixed(1)} s</span>{/if}
  </button>
  <span class="flight">{flightName(flight)}</span>
  <span class="caption">{figure.session.name}{day(figure.session.date) ? ` · ${day(figure.session.date)}` : ""}</span>
  <span class="caption mono">{tag.marker ? clock(tag.start, true) : `${clock(tag.start, true)} – ${clock(tag.end, true)}`}</span>
</article>

<style>
  article { display: flex; flex-direction: column; gap: 4px; min-width: 0; }
  .poster {
    position: relative; aspect-ratio: 4 / 3; border-radius: var(--radius); overflow: hidden;
    background: #000; padding: 0; display: block; width: 100%;
  }
  .poster img { width: 100%; height: 100%; object-fit: cover; display: block; user-select: none; }
  .name {
    position: absolute; left: 6px; bottom: 6px; padding: 2px 9px; border-radius: 999px;
    background: var(--accent); color: var(--accent-text); font-size: 12px; font-weight: 600;
  }
  .length {
    position: absolute; right: 6px; bottom: 6px; padding: 1px 6px; border-radius: 4px;
    background: rgba(0, 0, 0, 0.7); color: #fff; font-size: 11px;
  }
  .flight { font-weight: 600; white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
</style>
