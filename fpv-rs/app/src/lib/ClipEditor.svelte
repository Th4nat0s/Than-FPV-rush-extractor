<script lang="ts">
  import { untrack } from "svelte";
  import { api, media, message, type ClipView } from "../api";
  import { batch } from "./tasks.svelte";

  let { clip }: { clip: ClipView } = $props();

  // Edited locally, saved explicitly; `saved` is what the library holds. The
  // clip's bounds seed the editor once: it does not follow later refreshes.
  const initial = untrack(() => ({ start: clip.startFrame, end: clip.endFrame }));
  let saved = $state(initial);
  let start = $state(initial.start);
  let end = $state(initial.end);
  let boundary = $state<"start" | "end">("start");
  let preview = $state<string | undefined>();
  let failure = $state<string | null>(null);

  let frame = $derived(boundary === "start" ? start : end);
  let changed = $derived(start !== saved.start || end !== saved.end);
  let second = $derived(Math.max(1, Math.round(clip.fps)));
  let quarter = $derived(Math.max(1, Math.round(clip.fps / 4)));

  function set(value: number) {
    if (boundary === "start") start = Math.min(Math.max(0, value), end - 1);
    else end = Math.min(Math.max(start + 1, value), clip.lastFrame);
  }

  // The frame under the boundary, decoded from the recording itself — never
  // the proxy, whose frames are resampled. Debounced while dragging.
  $effect(() => {
    const wanted = frame;
    const timer = setTimeout(async () => {
      try {
        preview = media((await batch("previews", "Editor previews",
                                     () => api.previewFrame(clip.fileId, wanted))).path);
        failure = null;
      } catch (e) {
        failure = message(e);
      }
    }, 150);
    return () => clearTimeout(timer);
  });

  async function save() {
    try {
      await api.updateClip(clip.id, start, end);
      saved = { start, end };
    } catch (e) {
      failure = message(e);
    }
  }

  const seconds = (value: number) => (value / clip.fps).toFixed(3);
</script>

<div class="clip">
  <div class="row">
    <strong>{clip.fileName}</strong>
    <span class="spacer"></span>
    <span class="caption mono">{seconds(start)} s → {seconds(end)} s at {clip.fps.toFixed(3)} fps</span>
  </div>

  <div class="frame">
    {#if preview}<img src={preview} alt="Frame {frame}" />{/if}
    {#if failure}<p class="caption">{failure}</p>{/if}
  </div>

  <div class="tabs">
    <button class:active={boundary === "start"} onclick={() => (boundary = "start")}>Start</button>
    <button class:active={boundary === "end"} onclick={() => (boundary = "end")}>End</button>
  </div>

  <input type="range" min="0" max={clip.lastFrame} step="1" value={frame}
         oninput={(e) => set(Number((e.currentTarget as HTMLInputElement).value))} />

  <div class="row nudges">
    <button class="small" onclick={() => set(frame - second)}>−1 s</button>
    <button class="small" onclick={() => set(frame - quarter)}>−¼ s</button>
    <button class="small" onclick={() => set(frame - 1)}>−1 fr</button>
    <span class="spacer mono center">{seconds(frame)} s · frame {frame}</span>
    <button class="small" onclick={() => set(frame + 1)}>+1 fr</button>
    <button class="small" onclick={() => set(frame + quarter)}>+¼ s</button>
    <button class="small" onclick={() => set(frame + second)}>+1 s</button>
  </div>

  <div class="row">
    <span class="caption">Detected on a 0.25 s grid — nudge to the exact frame.</span>
    <span class="spacer"></span>
    <button onclick={() => { start = saved.start; end = saved.end; }} disabled={!changed}>Reset</button>
    <button class="primary" onclick={save} disabled={!changed}>Save</button>
  </div>
</div>

<style>
  .clip { display: flex; flex-direction: column; gap: 8px; padding-bottom: 14px; margin-bottom: 14px; border-bottom: 1px solid var(--line); }
  .frame { height: 300px; background: #000; border-radius: var(--radius); display: flex; align-items: center; justify-content: center; overflow: hidden; }
  .frame img { max-width: 100%; max-height: 100%; }
  .frame .caption { color: #aaa; }
  .tabs { display: flex; }
  .tabs button { flex: 1; border-radius: 0; }
  .tabs button:first-child { border-radius: 6px 0 0 6px; }
  .tabs button:last-child { border-radius: 0 6px 6px 0; border-left: none; }
  .tabs button.active { background: var(--selected); border-color: var(--accent); }
  input[type="range"] { width: 100%; accent-color: var(--accent); }
  .center { text-align: center; font-size: 12px; }
</style>
