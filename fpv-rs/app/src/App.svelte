<script lang="ts">
  import { onMount } from "svelte";
  import { open } from "@tauri-apps/plugin-dialog";
  import { api, day, message, type Card, type Info, type SessionView } from "./api";
  import Sidebar from "./lib/Sidebar.svelte";
  import SessionPane from "./lib/SessionPane.svelte";
  import FlightsPane from "./lib/FlightsPane.svelte";
  import FiguresPane from "./lib/FiguresPane.svelte";
  import { browse, show } from "./lib/browse.svelte";
  import { figures, filterFigures, filterFlights, placed } from "./lib/library";
  import ImportDialog from "./lib/ImportDialog.svelte";
  import SettingsDialog from "./lib/SettingsDialog.svelte";
  import TaskBar from "./lib/TaskBar.svelte";
  import ResegmentConfirm from "./lib/ResegmentConfirm.svelte";
  import { onTaskEnd } from "./lib/tasks.svelte";
  import { prepareStrips } from "./lib/strips";

  let sessions = $state<SessionView[]>([]);
  let cards = $state<Card[]>([]);
  let info = $state<Info>({ root: "", configDirectory: "", cacheDirectory: "", folderPattern: "",
                           ffmpegOk: true });
  let ffmpegOk = $derived(info.ffmpegOk);
  let settingsOpen = $state(false);
  let error = $state<string | null>(null);
  let importing = $state<{ folder: string; name: string } | null>(null);

  let selected = $derived(sessions.find((s) => s.id === browse.session) ?? null);
  let selectedId = $derived(selected?.id ?? null);
  let flights = $derived(filterFlights(placed(sessions), browse.flights));
  let figured = $derived(filterFigures(figures(sessions), browse.figures));
  /** "All sessions", or the one the filter keeps. */
  function scope(id: string | null): string {
    const session = sessions.find((s) => s.id === id);
    return session ? session.name + (day(session.date) ? ` · ${day(session.date)}` : "") : "All sessions";
  }

  async function refresh() {
    try {
      sessions = await api.snapshot();
      if (browse.session && !sessions.some((s) => s.id === browse.session)) browse.session = null;
      // A filter on a session that is gone would list nothing, silently.
      for (const filter of [browse.flights, browse.figures]) {
        if (filter.session && !sessions.some((s) => s.id === filter.session)) filter.session = null;
      }
    } catch (e) {
      error = `Cannot read the library: ${message(e)}`;
    }
  }

  async function pickFolder() {
    const folder = await open({ directory: true, title: "Folder of recordings" });
    if (typeof folder === "string") {
      importing = { folder, name: folder.split(/[\\/]/).filter(Boolean).pop() ?? "Session" };
    }
  }

  onMount(() => {
    api.info().then((loaded) => (info = loaded));
    // Strips missing since the last run — never made, or the cache emptied.
    refresh().then(() => prepareStrips(selectedId));
    // Takes imported before the library kept their camera: read once.
    api.identifyCameras().then((named) => { if (named) refresh(); }).catch(() => {});
    // Removable media, polled: cheap, and the same on every desktop.
    const poll = async () => { cards = await api.cards().catch(() => []); };
    poll();
    const timer = setInterval(poll, 3000);
    // A job finishing changes the library even when its window is long closed.
    const stop = onTaskEnd((task) => {
      refresh();
      // New flights, or flights re-keyed: their strips are made now, not on hover.
      if (task.state === "done" && (task.key === "import" || task.key?.startsWith("segment:"))) {
        prepareStrips(selectedId);
      }
    });
    return () => { clearInterval(timer); stop(); };
  });
</script>

<div class="window">
<div class="layout">
  <Sidebar {sessions} {cards} {ffmpegOk} {error}
           onimportcard={(card) => (importing = { folder: card.folder, name: card.name })}
           onimportfolder={pickFolder}
           onsettings={() => (settingsOpen = true)}
           onchange={refresh}
           onerror={(text) => (error = text)} />
  <main>
    {#if browse.view === "flights"}
      <FlightsPane view="flights" title="All flights" subtitle={scope(browse.flights.session)} items={flights}
                   showSession={browse.flights.session === null} {ffmpegOk} onchange={refresh}>
        {#snippet empty()}
          <div class="empty">
            <h2>{sessions.some((s) => s.flights.length) ? "No flights match these filters" : "No flights yet"}</h2>
            <button onclick={() => show(() => { browse.flights = { session: null, liked: false, labels: [], match: "any" }; })}>
              Reset filters</button>
          </div>
        {/snippet}
      </FlightsPane>
    {:else if browse.view === "figures"}
      <FiguresPane items={figured} subtitle={scope(browse.figures.session)} {ffmpegOk} onchange={refresh} />
    {:else if selected}
      {#key selected.id}
        <SessionPane session={selected} {ffmpegOk} onchange={refresh} />
      {/key}
    {:else}
      <div class="empty">
        <h2>No session selected</h2>
        <p>Insert a goggle or action-cam SD card, or import a folder of recordings.</p>
        <button class="primary" onclick={pickFolder} disabled={!ffmpegOk}>Import Folder…</button>
        {#if info.root}
          <p class="caption">Imports go to {info.root}
            — <button class="plain small link" onclick={() => (settingsOpen = true)}>change</button></p>
        {/if}
      </div>
    {/if}
  </main>
</div>
<TaskBar />
</div>
<ResegmentConfirm />

{#if importing}
  <ImportDialog folder={importing.folder} suggestedName={importing.name}
                existing={sessions.map((s) => s.name)}
                root={info.root} onrootchange={(loaded) => (info = loaded)}
                onclose={() => { importing = null; refresh(); }} />
{/if}

{#if settingsOpen}
  <SettingsDialog {info} onchange={(loaded) => { info = loaded; refresh(); }}
                  onclose={() => (settingsOpen = false)} />
{/if}

<style>
  .link { color: var(--accent); text-decoration: underline; padding: 0; }
  .window { display: flex; flex-direction: column; height: 100%; }
  .layout { display: flex; flex: 1; min-height: 0; }
  main { flex: 1; min-width: 0; height: 100%; overflow: hidden; }
</style>
