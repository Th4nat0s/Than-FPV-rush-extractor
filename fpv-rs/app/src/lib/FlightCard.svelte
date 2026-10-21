<script lang="ts">
  import { api, clock, flightName, media, message, type FlightView } from "../api";
  import { batch } from "./tasks.svelte";
  import { STRIP } from "./strips";
  import { poster as remake } from "./posters";
  import Icon, { type IconName } from "./Icon.svelte";

  let { flight, exporting, ffmpegOk, selecting = false, selected = false, ontoggle,
        onplay, onedit, onexport, onchange }: {
    flight: FlightView;
    exporting: boolean;
    ffmpegOk: boolean;
    /** In selection mode a click ticks the card instead of playing it. */
    selecting?: boolean;
    selected?: boolean;
    ontoggle?: () => void;
    onplay: () => void;
    onedit: () => void;
    onexport: () => void;
    onchange: () => void;
  } = $props();

  // Hover-scrub, DaVinci style: moving across the poster shows the frame under
  // the pointer. The frames come from a pre-extracted strip — never decoded
  // live during the hover — cached on disk under the flight's cache key, so
  // a strip fills once ever and a boundary edit gets a fresh one.
  let frames = $state<(string | undefined)[]>(Array(STRIP).fill(undefined));
  let hover = $state<number | null>(null);
  let menuOpen = $state(false);
  let filling = false;
  let stripKey = "";

  $effect(() => {
    if (flight.cacheKey !== stripKey) {
      stripKey = flight.cacheKey;
      frames = Array(STRIP).fill(undefined);
    }
  });

  async function fill() {
    if (filling || !ffmpegOk) return;
    filling = true;
    const key = stripKey;
    try {
      for (let index = 0; index < STRIP && hover !== null && key === stripKey; index++) {
        if (frames[index]) continue;
        const strip = await batch(`strip:${flight.id}`, `Hover strip of ${flightName(flight)}`,
                                  () => api.filmstripFrame(flight.id, index, STRIP)).catch(() => null);
        // One failure stands for the rest — the flight is gone or its
        // recording unreadable — so the strip stops instead of asking 24 times.
        if (!strip) break;
        if (key === stripKey) frames[index] = media(strip.path);
      }
    } finally {
      filling = false;
    }
  }

  function move(event: MouseEvent) {
    const box = (event.currentTarget as HTMLElement).getBoundingClientRect();
    hover = Math.min(1, Math.max(0, (event.clientX - box.left) / box.width));
    fill();
  }

  /** The loaded strip frame nearest the pointer, so scrubbing works while it fills. */
  let scrub = $derived.by(() => {
    if (hover === null) return undefined;
    const wanted = Math.min(STRIP - 1, Math.floor(hover * STRIP));
    for (let distance = 0; distance < STRIP; distance++) {
      for (const index of [wanted - distance, wanted + distance]) {
        if (frames[index]) return frames[index];
      }
    }
    return undefined;
  });

  // The poster is the first clip's — deliberately not its first frame, which
  // on analog is a fade of sync bands. Posters live in the cache, so one may
  // be gone; it is remade on first sight.
  let remade = $state<string | undefined>();
  let poster = $derived(media(flight.clips[0]?.thumbnail) ?? remade);

  $effect(() => {
    const clip = flight.clips[0];
    if (clip && !clip.thumbnail && !remade && ffmpegOk) {
      // Every card missing one asks at once: one "Posters" task for them all.
      batch("posters", "Posters", () => remake(clip.id))
        .then((made) => (remade = media(made.path))).catch(() => {});
    }
  });

  let badges = $derived(([
    [flight.actionCam ? "camera" : "goggles", flight.source],
    flight.recordings > 1 ? ["link", `${flight.recordings} ${flight.actionCam ? "chapters" : "recordings"}`] : null,
  ] as ([IconName, string] | null)[]).filter((badge) => badge !== null));

  async function like() {
    try {
      await api.toggleLike(flight.id);
      onchange();
    } catch (e) {
      alert(message(e));
    }
  }

  async function rename(input: HTMLInputElement) {
    if (input.value.trim() === (flight.title ?? "")) return;
    await api.setFlightTitle(flight.id, input.value).catch((e) => alert(message(e)));
    onchange();
  }

  function act(action: () => void) {
    menuOpen = false;
    action();
  }
</script>

<article class:selected>
  <div class="poster" role="button" tabindex="0" onclick={() => (selecting ? ontoggle?.() : onplay())}
       onkeydown={(e) => e.key === "Enter" && (selecting ? ontoggle?.() : onplay())}
       onmousemove={move} onmouseleave={() => (hover = null)}>
    {#if scrub ?? poster}
      <img src={scrub ?? poster} alt="" draggable="false" />
    {:else}
      <div class="placeholder"><Icon name="film" size={36} /></div>
    {/if}
    {#if hover !== null}<div class="playhead" style:left="{hover * 100}%"></div>{/if}
    {#if selecting}
      <span class="tick" class:on={selected} aria-hidden="true">{selected ? "✓" : ""}</span>
    {/if}
    <button class="plain heart" class:liked={flight.liked} title={flight.liked ? "Unlike" : "Like"}
            onclick={(e) => { e.stopPropagation(); like(); }}>{flight.liked ? "♥" : "♡"}</button>
  </div>

  <div class="row">
    <input class="inline title" value={flight.title ?? ""} placeholder="Flight {flight.order}"
           onkeydown={(e) => e.key === "Enter" && (e.currentTarget as HTMLInputElement).blur()}
           onblur={(e) => rename(e.currentTarget)} />
    <span class="mono muted">{clock(hover === null ? flight.duration : hover * flight.duration)}</span>
    <button class="plain icon" title="Export the video…" aria-label="Export the video"
            disabled={exporting || !ffmpegOk} onclick={onexport}><Icon name="export" size={15} /></button>
    <div class="menu">
      <button class="plain" title="More" onclick={() => (menuOpen = !menuOpen)}>⋯</button>
      {#if menuOpen}
        <div class="popup" role="menu">
          <button class="plain" onclick={() => act(onplay)}>Play &amp; tag</button>
          <button class="plain" onclick={() => act(like)}>{flight.liked ? "Unlike" : "Like"}</button>
          <button class="plain" onclick={() => act(onedit)}>Edit boundaries…</button>
          <button class="plain" disabled={exporting || !ffmpegOk} onclick={() => act(onexport)}>Export…</button>
        </div>
      {/if}
    </div>
  </div>
  {#if badges.length}
    <div class="badges caption">
      {#each badges as [icon, text]}<span class="row"><Icon name={icon} size={12} />{text}</span>{/each}
    </div>
  {/if}
</article>

<svelte:window onclick={(e) => menuOpen && !(e.target as Element).closest(".menu") && (menuOpen = false)} />

<style>
  article { display: flex; flex-direction: column; gap: 6px; min-width: 0; }
  .poster {
    position: relative;
    aspect-ratio: 4 / 3;
    border-radius: var(--radius);
    overflow: hidden;
    background: #000;
    cursor: pointer;
  }
  .poster img { width: 100%; height: 100%; object-fit: cover; display: block; user-select: none; }
  .placeholder { height: 100%; display: flex; align-items: center; justify-content: center; color: #888; }
  .playhead { position: absolute; top: 0; bottom: 0; width: 2px; margin-left: -1px; background: #fff; box-shadow: 0 0 2px #000; pointer-events: none; }
  .tick {
    position: absolute;
    top: 6px;
    left: 6px;
    width: 22px;
    height: 22px;
    border-radius: 50%;
    border: 2px solid #fff;
    background: rgba(0, 0, 0, 0.35);
    color: #fff;
    font-size: 13px;
    font-weight: 700;
    display: flex;
    align-items: center;
    justify-content: center;
    box-shadow: 0 0 3px #000;
  }
  .tick.on { background: var(--accent); border-color: var(--accent); }
  article.selected .poster { outline: 3px solid var(--accent); outline-offset: 2px; }
  .heart { position: absolute; top: 4px; right: 4px; color: #fff; font-size: 20px; text-shadow: 0 0 3px #000; }
  .heart.liked { color: #ff3b47; }
  .title { font-weight: 600; flex: 1; min-width: 0; }
  .icon { display: flex; align-items: center; color: var(--muted); }
  .icon:hover:not(:disabled) { color: var(--accent); }
  .badges { display: flex; gap: 12px; flex-wrap: wrap; }
  .badges .row { gap: 4px; }
  .menu { position: relative; }
  .popup {
    position: absolute;
    right: 0;
    top: 100%;
    z-index: 5;
    min-width: 170px;
    display: flex;
    flex-direction: column;
    padding: 4px;
    background: var(--panel);
    border: 1px solid var(--line);
    border-radius: var(--radius);
    box-shadow: 0 8px 24px rgba(0, 0, 0, 0.25);
  }
  .popup button { text-align: left; padding: 6px 10px; border-radius: 4px; }
  .popup button:hover:not(:disabled) { background: var(--hover); }
</style>
