<script lang="ts">
  import { onMount } from "svelte";
  import { api, clock, flightName, message, type FlightView, type TagView } from "../api";
  import Icon from "./Icon.svelte";
  import Modal from "./Modal.svelte";

  // The player only ever opens the flight's proxy: a web view plays neither
  // MJPEG-in-AVI nor, on Linux, HEVC. The proxy is resampled, so a tag marked
  // here goes straight back to source frames in Rust (FlightTimeline) and is
  // never stored on the proxy's clock.
  let { flight, onclose }: { flight: FlightView; onclose: () => void } = $props();

  let video: HTMLVideoElement | undefined = $state();
  let source = $state<string | undefined>();
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
      source = await api.proxy(flight.id);
    } catch (e) {
      failure = message(e);
    }
  });

  function seek(seconds: number) {
    // Setting currentTime seeks to the exact frame (fastSeek is the
    // keyframe-snapping one); a tag lands on its figure.
    if (video) video.currentTime = seconds;
  }

  async function commit() {
    const name = label.trim();
    if (!name || !video) return;
    const at = video.currentTime;
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
      markIn = video.currentTime;
    }
  }
</script>

<svelte:window onkeydown={keydown} />

<Modal title={flightName(flight)} width={1100} {onclose}>
  <div class="layout">
    <div class="player">
      {#if source}
        <!-- svelte-ignore a11y_media_has_caption -->
        <video bind:this={video} src={source} controls autoplay
               ontimeupdate={() => (now = video?.currentTime ?? 0)}
               onerror={() => (failure = `The player refused the proxy (${video?.error?.message || `code ${video?.error?.code}`}): ${source}`)}></video>
        {#if failure}<p class="overlay caption">{failure}</p>{/if}
      {:else if failure}
        <div class="empty"><h2>Cannot build the preview</h2><p>{failure}</p></div>
      {:else}
        <div class="empty">
          <div class="spinner"></div>
          <p>Building the preview…</p>
          <p class="caption">A transcode of the whole flight, once; it is cached afterwards.</p>
        </div>
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
  .overlay { position: absolute; left: 12px; right: 12px; bottom: 12px; margin: 0; color: #f88; }
  video { width: 100%; height: 100%; object-fit: contain; }
  .player .empty { color: #aaa; }
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
