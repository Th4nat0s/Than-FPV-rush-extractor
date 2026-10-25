<script lang="ts">
  import { onMount, untrack } from "svelte";
  import { ask } from "@tauri-apps/plugin-dialog";
  import { api, bytes, message, type CacheKind, type CacheUsage, type Info } from "../api";
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
  let pictures = $derived(usage ? usage.strips + usage.posters + usage.previews : 0);
  // Fields edited from the budgets as the dialog opens.
  let proxyGb = $state(untrack(() => info.proxyBudgetGb));
  let pictureGb = $state(untrack(() => info.pictureBudgetGb));

  const ALL: CacheKind[] = ["proxies", "strips", "posters", "previews"];
  /** Each kind, with what emptying it costs: when it comes back. */
  const KINDS: { kind: CacheKind; label: string; comesBack: string }[] = [
    { kind: "strips", label: "Hover strips", comesBack: "made again in the background" },
    { kind: "posters", label: "Posters", comesBack: "made again as their cards show" },
    { kind: "previews", label: "Editor frames", comesBack: "made again when the boundary editor opens" },
    { kind: "proxies", label: "Playback previews",
      comesBack: "made again when their flight is played (a 4K take takes a while)" },
  ];

  async function saveBudgets() {
    problem = null;
    try {
      onchange(await api.setCacheBudgets(Number(proxyGb), Number(pictureGb)));
    } catch (e) {
      problem = `Cannot set the budgets: ${message(e)}`;
      proxyGb = info.proxyBudgetGb;
      pictureGb = info.pictureBudgetGb;
    }
    await measure();
  }

  async function measure() {
    usage = await api.cacheUsage().catch(() => null);
  }
  onMount(measure);

  async function empty(kinds: CacheKind[]) {
    if (!usage) return;
    const chosen = KINDS.filter((k) => kinds.includes(k.kind));
    const size = chosen.reduce((sum, k) => sum + usage![k.kind], 0);
    const whole = kinds.length === ALL.length;
    const name = whole ? "the cache" : chosen[0].label.toLowerCase();
    const confirmed = await ask(
      whole ? `Everything in it is made again when needed — posters as they are shown, hover strips in ` +
              `the background, a playback preview when its flight is played (a 4K take takes a while).`
            : `${chosen[0].label} are ${chosen[0].comesBack}.`,
      { title: `Empty ${name} (${bytes(size)})?`, kind: "warning", okLabel: whole ? "Empty Cache" : "Empty",
        cancelLabel: "Cancel" });
    if (!confirmed) return;
    emptying = true;
    problem = null;
    try {
      await track(`Empty ${name}`, () => api.clearCache(kinds),
                  { summary: (freed) => `${bytes(freed.proxies + freed.strips + freed.posters + freed.previews)} freed` });
      // The library shows its posters again, remade as they come on screen,
      // and the strips are made ahead of the pointer as usual.
      onchange(await api.info());
      prepareStrips();
    } catch (e) {
      problem = `Cannot empty ${name}: ${message(e)}`;
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
  <p class="caption">All of it is made again when needed, so any of it can be emptied at any time.
    Over its budget, the oldest goes first — of the pictures, editor frames, then hover strips,
    then posters.</p>
  {#if problem}<p class="error">{problem}</p>{/if}
  {#if usage}
    <table class="cache">
      <tbody>
        <tr class="group">
          <th>Pictures</th>
          <td class="mono">{bytes(pictures)}</td>
          <td><label class="row budget">of <input type="number" min="0.1" step="0.5" aria-label="Pictures' budget in GB"
                                                  bind:value={pictureGb} onchange={saveBudgets} /> GB</label></td>
          <td></td>
        </tr>
        {#each KINDS.slice(0, 3) as { kind, label } (kind)}
          <tr>
            <td class="sub">{label}</td>
            <td class="mono">{bytes(usage[kind])}</td>
            <td></td>
            <td><button onclick={() => empty([kind])} disabled={emptying || !usage[kind]}>Empty…</button></td>
          </tr>
        {/each}
        <tr class="group">
          <th>Playback previews</th>
          <td class="mono">{bytes(usage.proxies)}</td>
          <td><label class="row budget">of <input type="number" min="0.1" step="1" aria-label="Playback previews' budget in GB"
                                                  bind:value={proxyGb} onchange={saveBudgets} /> GB</label></td>
          <td><button onclick={() => empty(["proxies"])} disabled={emptying || !usage.proxies}>Empty…</button></td>
        </tr>
      </tbody>
    </table>
  {:else}
    <p class="caption">Measuring…</p>
  {/if}
  <div class="row usage">
    <span class="spacer"></span>
    <button onclick={() => empty(ALL)} disabled={emptying || !total}>
      {emptying ? "Emptying…" : `Empty All${total ? ` (${bytes(total)})` : ""}…`}</button>
  </div>

  {#snippet footer()}<button class="primary" onclick={onclose}>Done</button>{/snippet}
</Modal>

<style>
  h3 { font-size: 13px; margin: 14px 0 4px; }
  h3:first-child { margin-top: 0; }
  p { margin: 0 0 8px; }
  .usage { gap: 12px; }
  .cache { width: 100%; border-collapse: collapse; margin-bottom: 8px; font-size: 13px; }
  .cache th { text-align: left; font-weight: 600; }
  .cache td, .cache th { padding: 3px 6px; }
  .cache tr.group th, .cache tr.group td { padding-top: 8px; }
  .cache td:last-child { text-align: right; }
  .cache .sub { padding-left: 18px; color: var(--muted); }
  .budget { gap: 4px; }
  .budget input { width: 64px; }
  .usage button { white-space: nowrap; flex-shrink: 0; }
  .error { color: var(--danger); }
</style>
