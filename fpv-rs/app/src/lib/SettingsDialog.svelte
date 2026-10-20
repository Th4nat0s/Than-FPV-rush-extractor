<script lang="ts">
  import { onMount } from "svelte";
  import { ask } from "@tauri-apps/plugin-dialog";
  import { api, bytes, message, type CacheUsage, type Info } from "../api";
  import FolderPattern from "./FolderPattern.svelte";
  import MediaRoot from "./MediaRoot.svelte";
  import Modal from "./Modal.svelte";
  import { prepareStrips } from "./strips";
  import { track } from "./tasks.svelte";

  let { info, onchange, onclose }: {
    info: Info;
    onchange: (info: Info) => void;
    onclose: () => void;
  } = $props();

  let usage = $state<CacheUsage | null>(null);
  let emptying = $state(false);
  let problem = $state<string | null>(null);
  let total = $derived(usage ? usage.proxies + usage.strips + usage.posters + usage.previews : 0);

  async function measure() {
    usage = await api.cacheUsage().catch(() => null);
  }
  onMount(measure);

  async function empty() {
    const confirmed = await ask(
      `Everything in it is made again when needed — posters as they are shown, hover strips in ` +
      `the background, a playback preview when its flight is played (a 4K take takes a while).`,
      { title: `Empty the cache (${bytes(total)})?`, kind: "warning", okLabel: "Empty Cache",
        cancelLabel: "Cancel" });
    if (!confirmed) return;
    emptying = true;
    problem = null;
    try {
      await track("Empty the cache", () => api.clearCache(),
                  { summary: (freed) => `${bytes(freed.proxies + freed.strips + freed.posters + freed.previews)} freed` });
      // The library shows its posters again, remade as they come on screen,
      // and the strips are made ahead of the pointer as usual.
      onchange(await api.info());
      prepareStrips();
    } catch (e) {
      problem = `Cannot empty the cache: ${message(e)}`;
    } finally {
      emptying = false;
      await measure();
    }
  }
</script>

<Modal title="Settings" width={560} {onclose}>
  <h3>Import folder</h3>
  <p class="caption">Imported recordings are copied here, one folder per session. Changing it
    moves nothing: what is already imported stays where it is and keeps working; new imports go
    to the new folder.</p>
  <MediaRoot root={info.root} {onchange} />

  <h3>Folder names</h3>
  <FolderPattern pattern={info.folderPattern} {onchange} />

  <h3>Database and settings</h3>
  <p class="mono caption">{info.configDirectory}</p>
  <p class="caption">library.sqlite and config.json — small, and the part worth backing up.</p>

  <h3>Cache</h3>
  <p class="mono caption">{info.cacheDirectory}</p>
  <p class="caption">Posters, playback previews and hover strips. All of it rebuilds on demand,
    so it can be emptied at any time; config.json caps the previews (proxyBudgetGb).</p>
  <div class="row usage">
    {#if usage}
      <span class="caption">Playback previews {bytes(usage.proxies)} · Hover strips {bytes(usage.strips)}
        · Posters {bytes(usage.posters)} · Editor frames {bytes(usage.previews)}</span>
    {:else}
      <span class="caption">Measuring…</span>
    {/if}
    <span class="spacer"></span>
    <button onclick={empty} disabled={emptying || !total}>
      {emptying ? "Emptying…" : `Empty Cache${total ? ` (${bytes(total)})` : ""}…`}</button>
  </div>
  {#if problem}<p class="error">{problem}</p>{/if}

  {#snippet footer()}<button class="primary" onclick={onclose}>Done</button>{/snippet}
</Modal>

<style>
  h3 { font-size: 13px; margin: 14px 0 4px; }
  h3:first-child { margin-top: 0; }
  p { margin: 0 0 8px; }
  .usage { gap: 12px; }
  .usage button { white-space: nowrap; flex-shrink: 0; }
  .error { color: var(--danger); }
</style>
