<script lang="ts">
  import { onMount } from "svelte";
  import { open } from "@tauri-apps/plugin-dialog";
  import { api, message, type Card, type Info, type SessionView } from "./api";
  import Sidebar from "./lib/Sidebar.svelte";
  import SessionPane from "./lib/SessionPane.svelte";
  import ImportDialog from "./lib/ImportDialog.svelte";
  import SettingsDialog from "./lib/SettingsDialog.svelte";

  let sessions = $state<SessionView[]>([]);
  let cards = $state<Card[]>([]);
  let selectedId = $state<string | null>(null);
  let info = $state<Info>({ root: "", configDirectory: "", cacheDirectory: "", folderPattern: "",
                           ffmpegOk: true });
  let ffmpegOk = $derived(info.ffmpegOk);
  let settingsOpen = $state(false);
  let error = $state<string | null>(null);
  let importing = $state<{ folder: string; name: string } | null>(null);

  let selected = $derived(sessions.find((s) => s.id === selectedId) ?? null);

  async function refresh() {
    try {
      sessions = await api.snapshot();
      if (selectedId && !sessions.some((s) => s.id === selectedId)) selectedId = null;
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
    refresh();
    // Removable media, polled: cheap, and the same on every desktop.
    const poll = async () => { cards = await api.cards().catch(() => []); };
    poll();
    const timer = setInterval(poll, 3000);
    return () => clearInterval(timer);
  });
</script>

<div class="layout">
  <Sidebar {sessions} {cards} {selectedId} {ffmpegOk} {error}
           onselect={(id) => (selectedId = id)}
           onimportcard={(card) => (importing = { folder: card.folder, name: card.name })}
           onimportfolder={pickFolder}
           onsettings={() => (settingsOpen = true)}
           onchange={refresh}
           onerror={(text) => (error = text)} />
  <main>
    {#if selected}
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
  .layout { display: flex; height: 100%; }
  main { flex: 1; min-width: 0; height: 100%; overflow: hidden; }
</style>
