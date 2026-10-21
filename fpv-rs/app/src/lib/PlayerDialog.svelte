<script lang="ts">
  import { onMount } from "svelte";
  import { api, clock, flightName, message, type FlightView, type TagView } from "../api";
  import Icon from "./Icon.svelte";
  import Modal from "./Modal.svelte";
  import { track } from "./tasks.svelte";
  import { advance, duration, flightTime, locate, type Segment } from "./playlist";

  // An action-cam take plays from its own files, chapter after chapter; a DVR
  // flight from its proxy — no web view reads MJPEG-in-AVI — which is the
  // one-segment case. Either way the player's time is a file's, and the
  // flight's clock comes from playlist.ts; a tag marked on it goes back to
  // source frames in Rust (FlightTimeline), never stored on a file's clock.
  let { flight, onclose }: { flight: FlightView; onclose: () => void } = $props();

  let video: HTMLVideoElement | undefined = $state();
  let segments = $state<Segment[]>([]);
  let index = $state(0);
  /** A file time to go to once the current segment's file has loaded. */
  let pending: number | null = null;
  let direct = $state(false);
  let building = $state(false);
  let note = $state<string | null>(null);
  let source = $derived(segments[index]?.url);
  let total = $derived(duration(segments));
  let failure = $state<string | null>(null);
  let tags = $state<TagView[]>([]);
  let suggestions = $state<string[]>([]);
  let label = $state("");
  let markIn = $state<number | null>(null);
  let now = $state(0);

  async function reload() {
    [tags, suggestions] = await Promise.all([api.flightTags(flight.id), api.tagLabels()]);
  }

  onMount(async () => {
    reload().catch((e) => (failure = message(e)));
    try {
      const playback = await api.playback(flight.id);
      if (playback.kind === "direct") {
        direct = true;
        segments = playback.segments;
        pending = segments[0]?.start ?? 0;
      } else {
        await openProxy(0);
      }
    } catch (e) {
      failure = message(e);
    }
  });

  /** The proxy, from flight time `at`. Instant on a cache hit; a transcode
   * otherwise, which the task bar shows. */
  async function openProxy(at: number) {
    building = true;
    try {
      const url = await track(`Preview of ${flightName(flight)}`, () => api.proxy(flight.id),
                              { key: `proxy:${flight.id}`, line: "Transcoding the flight for playback…",
                                summary: () => "Ready" });
      direct = false;
      pending = at;
      index = 0;
      segments = [{ url, start: 0, end: flight.duration, offset: 0 }];
    } finally {
      building = false;
    }
  }

  function loaded() {
    if (video && pending !== null) video.currentTime = pending;
    pending = null;
  }

  function go(next: number, time: number) {
    if (next === index && video) {
      video.currentTime = time;
    } else {
      // A new file: the player loads it and plays on (autoplay), from `time`.
      pending = time;
      index = next;
    }
  }

  function tick() {
    if (!video || !segments.length) return;
    now = flightTime(segments, index, video.currentTime);
    const step = advance(segments, index, video.currentTime);
    if (step.kind === "next") go(step.index, step.time);
    else if (step.kind === "end" && !video.paused) video.pause();
  }

  function refused() {
    const reason = video?.error?.message || `code ${video?.error?.code}`;
    if (direct) {
      // HEVC on Linux without the GStreamer plugin, say: the proxy plays it.
      note = `This recording cannot be played directly here (${reason}); playing a preview.`;
      openProxy(now).catch((e) => (failure = message(e)));
    } else {
      failure = `The player refused the preview (${reason}): ${source}`;
    }
  }

  function seek(seconds: number) {
    // Setting currentTime seeks to the exact frame (fastSeek is the
    // keyframe-snapping one); a tag lands on its figure.
    const target = locate(segments, seconds);
    go(target.index, target.time);
  }

  async function commit() {
    const name = label.trim();
    if (!name || !video) return;
    const at = now;
    try {
      // A range across the DVR's cut, or a GoPro chapter's, is clipped to the
      // recording holding its start: a tag lives in one recording.
      await api.addTag(flight.id, name, markIn ?? at, at);
      label = "";
      markIn = null;
      await reload();
    } catch (e) {
      failure = message(e);
    }
  }

  async function remove(tag: TagView) {
    await api.deleteTag(tag.id).catch((e) => (failure = message(e)));
    await reload();
  }

  function keydown(event: KeyboardEvent) {
    // I marks the start of a range, unless typing a name.
    if (event.key.toLowerCase() === "i" && !(event.target instanceof HTMLInputElement) && video) {
      markIn = now;
    }
  }
</script>

<svelte:window onkeydown={keydown} />

<Modal title={flightName(flight)} width={1100} {onclose}>
  <div class="layout">
    <div class="player">
      {#if source && !building}
        <!-- svelte-ignore a11y_media_has_caption -->
        <video bind:this={video} src={source} controls autoplay
               onloadedmetadata={loaded} ontimeupdate={tick} onended={tick} onerror={refused}></video>
        <div class="top">
          {#if note && !failure}<p class="tag-on note">{note}</p>{/if}
          <span class="spacer"></span>
          <!-- The native controls show the file's time; this is the flight's. -->
          <p class="tag-on clock mono">{clock(now, true)} / {clock(total, true)}{#if segments.length > 1}
            · chapter {index + 1}/{segments.length}{/if}</p>
        </div>
        {#if failure}<p class="overlay caption">{failure}</p>{/if}
      {:else if failure}
        <div class="empty"><h2>Cannot play this flight</h2><p>{failure}</p></div>
      {:else if building}
        <div class="empty">
          <div class="spinner"></div>
          <p>Building the preview…</p>
          <p class="caption">{note ?? "A transcode of the whole flight, once; it is cached afterwards."}</p>
        </div>
      {:else}
        <div class="empty"><div class="spinner"></div></div>
      {/if}
    </div>

    <aside>
      <h3>Figures</h3>
      <ul>
        {#each tags as tag (tag.id)}
          <li>
            <button class="plain jump" onclick={() => seek(tag.start)}>
              <span>{tag.label}</span>
              <span class="caption mono">
                {tag.marker ? clock(tag.start, true) : `${clock(tag.start, true)} – ${clock(tag.end, true)}`}
              </span>
            </button>
            <button class="plain danger" title="Delete" onclick={() => remove(tag)}><Icon name="trash" /></button>
          </li>
        {:else}
          <li class="caption">No tags yet.</li>
        {/each}
      </ul>

      <div class="form">
        {#if markIn !== null}
          <div class="row caption">
            <span>● Marked at {clock(markIn, true)}</span><span class="spacer"></span>
            <button class="plain small" onclick={() => (markIn = null)}>Cancel</button>
          </div>
        {/if}
        <input type="text" placeholder="Figure name (split-s, power loop…)" list="figure-labels"
               bind:value={label} disabled={!source}
               onkeydown={(e) => e.key === "Enter" && commit()} />
        <datalist id="figure-labels">
          {#each suggestions as suggestion}<option value={suggestion}></option>{/each}
        </datalist>
        <div class="row">
          {#if markIn === null}
            <button onclick={() => (markIn = video?.currentTime ?? 0)} disabled={!source}
                    title="Start a range here (I)">Mark In</button>
            <span class="spacer"></span>
            <button class="primary" onclick={commit} disabled={!label.trim() || !source}>Add Point</button>
          {:else}
            <span class="spacer"></span>
            <button class="primary" onclick={commit} disabled={!label.trim()}>
              Add Range to {clock(now, true)}</button>
          {/if}
        </div>
        <p class="caption">Mark In (or I) at the start, play to the end, name it, Enter.</p>
      </div>
    </aside>
  </div>
  {#snippet footer()}<button class="primary" onclick={onclose}>Done</button>{/snippet}
</Modal>

<style>
  .layout { display: flex; gap: 16px; height: min(600px, calc(100vh - 170px)); }
  .player { position: relative; flex: 1; min-width: 0; background: #000; border-radius: var(--radius); overflow: hidden; display: flex; }
  /* Above the picture, clear of the native controls at the bottom. */
  .top { position: absolute; top: 8px; left: 8px; right: 8px; display: flex; gap: 8px; pointer-events: none; }
  .tag-on { margin: 0; padding: 2px 6px; font-size: 12px; color: #eee; background: rgba(0, 0, 0, 0.6); border-radius: 4px; }
  .clock { white-space: nowrap; }
  .overlay { position: absolute; left: 12px; right: 12px; bottom: 12px; margin: 0; color: #f88; }
  video { width: 100%; height: 100%; object-fit: contain; }
  .player .empty { flex: 1; color: #aaa; }
  .player .empty h2 { color: #eee; }
  aside { width: 280px; display: flex; flex-direction: column; gap: 8px; }
  h3 { margin: 0; font-size: 14px; }
  ul { list-style: none; margin: 0; padding: 0; flex: 1; overflow: auto; }
  li { display: flex; align-items: center; border-bottom: 1px solid var(--line); }
  .jump { flex: 1; display: flex; flex-direction: column; align-items: flex-start; text-align: left; padding: 6px 4px; }
  .form { display: flex; flex-direction: column; gap: 6px; border-top: 1px solid var(--line); padding-top: 8px; }
  .spinner { width: 28px; height: 28px; border: 3px solid #555; border-top-color: var(--accent); border-radius: 50%; animation: spin 0.9s linear infinite; }
  @keyframes spin { to { transform: rotate(360deg); } }
</style>
