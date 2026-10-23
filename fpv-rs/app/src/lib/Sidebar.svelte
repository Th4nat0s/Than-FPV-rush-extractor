<script lang="ts">
  import { tick } from "svelte";
  import { api, day, message, type Card, type SessionView } from "../api";
  import Icon from "./Icon.svelte";
  import Modal from "./Modal.svelte";
  import FilterPanel from "./FilterPanel.svelte";
  import ProfileDialog from "./ProfileDialog.svelte";
  import LoadFolderDialog from "./LoadFolderDialog.svelte";
  import { browse, openSession, show } from "./browse.svelte";
  import { figures, placed, type View } from "./library";
  import { runningTask } from "./tasks.svelte";
  import { exportLabels, loadImportFolder, segment } from "./sessions";

  let { sessions, cards, root, ffmpegOk, error, onimportcard, onimportfolder,
        onsettings, onchange, onerror }: {
    sessions: SessionView[];
    cards: Card[];
    /** The import folder, where Load from Destination Folder looks. */
    root: string;
    ffmpegOk: boolean;
    error: string | null;
    onimportcard: (card: Card) => void;
    onimportfolder: () => void;
    onsettings: () => void;
    onchange: () => void;
    onerror: (text: string) => void;
  } = $props();

  function subtitle(session: SessionView): string {
    const files = `${session.files.length} file${session.files.length === 1 ? "" : "s"}`;
    const flights = session.flights.length ? ` · ${session.flights.length} flights` : "";
    const date = day(session.date);
    return (date ? `${date} — ` : "") + files + flights;
  }

  async function rename(session: SessionView, input: HTMLInputElement) {
    const name = input.value.trim();
    if (!name || name === session.name) {
      input.value = session.name; // an empty or unchanged edit snaps back
      return;
    }
    try {
      await api.renameSession(session.id, name);
      onchange();
    } catch (e) {
      input.value = session.name;
      onerror(`Cannot rename the session: ${message(e)}`);
    }
  }

  // Removing a session asks what becomes of its recordings: kept on disk (the
  // default, and the one that loses nothing) or moved to the trash.
  let removing = $state<SessionView | null>(null);

  let tabs = $derived([
    { view: "sessions", label: "Sessions", count: sessions.length },
    { view: "flights", label: "All flights", count: placed(sessions).length },
    { view: "figures", label: "Figures", count: figures(sessions).length },
  ] as { view: View; label: string; count: number }[]);

  // A session's menu, from its ⋯ or a right-click, where the pointer is.
  let menu = $state<{ session: SessionView; x: number; y: number } | null>(null);
  let profileOf = $state<SessionView | null>(null);
  let notice = $state<string | null>(null);
  let noticeTitle = $state("Export Labels");
  // The arrow beside Import Folder…: the other way recordings come in.
  let importMenu = $state<{ x: number; y: number } | null>(null);

  let choosing = $state(false);

  async function load(folders: string[]) {
    choosing = false;
    // The task bar shows the loading; the summary comes when it ends.
    const done = await loadImportFolder(folders);
    noticeTitle = "Load from Destination Folder";
    notice = done;
  }
  let inputs: Record<string, HTMLInputElement> = {};

  function openMenu(event: MouseEvent, session: SessionView) {
    event.preventDefault();
    const at = event.type === "contextmenu"
      ? { x: event.clientX, y: event.clientY }
      : (({ right, bottom }) => ({ x: right - 4, y: bottom }))((event.currentTarget as HTMLElement).getBoundingClientRect());
    menu = { session, ...at };
  }

  function act(action: (session: SessionView) => void) {
    const session = menu!.session;
    menu = null;
    action(session);
  }

  async function startRename(session: SessionView) {
    openSession(session.id);
    await tick();
    inputs[session.id]?.focus();
    inputs[session.id]?.select();
  }

  async function writeLabels(session: SessionView) {
    const done = await exportLabels(session);
    if (done) {
      noticeTitle = "Export Labels";
      notice = done;
    }
  }

  function resegment(session: SessionView) {
    openSession(session.id);
    // Progress shows in the task bar and the session's footer.
    segment(session).then((line) => line?.startsWith("Segmentation failed") && onerror(line));
  }

  async function remove(session: SessionView, keepRecordings: boolean) {
    removing = null;
    try {
      await api.deleteSession(session.id, keepRecordings);
      onchange();
    } catch (e) {
      onerror(`Cannot delete the session: ${message(e)}`);
    }
  }
</script>

<aside>
  <div class="top row">
    <strong>FPV Rush</strong>
    <span class="spacer"></span>
    <div class="split">
      <button class="small" onclick={onimportfolder} disabled={!ffmpegOk}>Import Folder…</button>
      <button class="small arrow" aria-label="More ways to import" aria-haspopup="menu" disabled={!ffmpegOk}
              onclick={(e) => { const box = e.currentTarget.getBoundingClientRect(); importMenu = { x: box.left, y: box.bottom + 2 }; }}>
        <Icon name="down" size={12} /></button>
    </div>
    <button class="small" onclick={onsettings} title="Settings"><Icon name="settings" /></button>
  </div>

  {#if !ffmpegOk}
    <p class="notice warn">ffmpeg not found — install it (<code>sudo apt install ffmpeg</code>) and restart.</p>
  {/if}
  {#if error}<p class="notice error">{error}</p>{/if}

  {#each cards as card (card.volume)}
    <button class="card" onclick={() => onimportcard(card)} disabled={!ffmpegOk}>
      <Icon name="card" size={20} />
      <span>Import “{card.name}”<br /><span class="caption">{card.fileCount} recordings</span></span>
    </button>
  {/each}

  <div class="tabs" role="tablist" aria-label="Library view">
    {#each tabs as tab (tab.view)}
      <button role="tab" aria-selected={browse.view === tab.view} class:on={browse.view === tab.view}
              onclick={() => show(() => (browse.view = tab.view))}>
        {tab.label}<span class="count mono">{tab.count}</span></button>
    {/each}
  </div>

  {#if browse.view === "sessions"}
    <h3>Sessions</h3>
    <ul>
      {#each sessions as session (session.id)}
        <li class:selected={session.id === browse.session} oncontextmenu={(e) => openMenu(e, session)}>
          <div class="session" role="button" tabindex="0"
               onclick={() => openSession(session.id)}
               onkeydown={(e) => e.key === "Enter" && e.target === e.currentTarget && openSession(session.id)}>
            <input class="inline name" value={session.name} aria-label="Session name" bind:this={inputs[session.id]}
                   onfocus={() => openSession(session.id)}
                   onkeydown={(e) => e.key === "Enter" && (e.currentTarget as HTMLInputElement).blur()}
                   onblur={(e) => rename(session, e.currentTarget)} />
            <span class="caption">{subtitle(session)}</span>
          </div>
          <button class="plain more" aria-label="Actions for {session.name}" aria-haspopup="menu"
                  onclick={(e) => openMenu(e, session)}>⋯</button>
        </li>
      {:else}
        <li class="caption none">No sessions yet.</li>
      {/each}
    </ul>
    {#if sessions.length}<p class="caption hint">Right-click a session to open its folder, change its detection profile and more.</p>{/if}
  {:else}
    <FilterPanel {sessions} />
  {/if}
</aside>

{#if menu}
  {@const session = menu.session}
  <button class="backdrop" aria-label="Close the menu" onclick={() => (menu = null)}
          oncontextmenu={(e) => { e.preventDefault(); menu = null; }}></button>
  <div class="popup" role="menu" aria-label="{session.name}" style:left="{menu.x}px" style:top="{menu.y}px">
    <span class="caption heading">{session.name}{day(session.date) ? ` · ${day(session.date)}` : ""}</span>
    <button class="plain" role="menuitem" disabled={!session.files.length}
            onclick={() => act((s) => api.revealSession(s.id).catch((e) => onerror(message(e))))}>Open Folder</button>
    {#if session.segmentable}
      <button class="plain" role="menuitem" onclick={() => act((s) => (profileOf = s))}>Detection Profile…</button>
      <button class="plain" role="menuitem" disabled={!ffmpegOk || !!runningTask(`segment:${session.id}`)}
              onclick={() => act(resegment)}>{session.flights.some((f) => !f.actionCam) ? "Re-segment" : "Segment"}</button>
      <button class="plain" role="menuitem" disabled={!session.flights.length}
              onclick={() => act(writeLabels)}>Export Labels…</button>
    {/if}
    <hr />
    <button class="plain" role="menuitem" onclick={() => act(startRename)}>Rename</button>
    <button class="plain danger" role="menuitem" onclick={() => act((s) => (removing = s))}>Remove from Library…</button>
  </div>
{/if}

{#if importMenu}
  <button class="backdrop" aria-label="Close the menu" onclick={() => (importMenu = null)}></button>
  <div class="popup" role="menu" aria-label="Import" style:left="{importMenu.x}px" style:top="{importMenu.y}px">
    <button class="plain" role="menuitem" onclick={() => { importMenu = null; onimportfolder(); }}>Import Folder…</button>
    <button class="plain" role="menuitem" onclick={() => { importMenu = null; choosing = true; }}
            title="Add what the import folder holds and the library does not — where it is, without copying">
      Load from Destination Folder…</button>
  </div>
{/if}

{#if choosing}
  <LoadFolderDialog {root} onload={load} onclose={() => (choosing = false)} />
{/if}

{#if profileOf}
  <ProfileDialog session={profileOf} {onchange} onclose={() => (profileOf = null)} />
{/if}
{#if notice}
  <Modal title={noticeTitle} onclose={() => (notice = null)}>
    <p>{notice}</p>
    {#snippet footer()}<button class="primary" onclick={() => (notice = null)}>OK</button>{/snippet}
  </Modal>
{/if}

{#if removing}
  {@const session = removing}
  <Modal title="Delete “{session.name}”?" width={500} onclose={() => (removing = null)}>
    <p>Its flights, titles, likes and figure tags are removed from the library.</p>
    <p class="caption">Keep the videos to leave its {session.files.length} recording{session.files.length === 1 ? "" : "s"}
      in the import folder: importing the same card again brings the session back
      without copying them twice. The SD card originals are untouched either way.</p>
    {#snippet footer()}
      <button onclick={() => (removing = null)}>Cancel</button>
      <span class="spacer"></span>
      <button class="danger" onclick={() => remove(session, false)}>Also Trash the Videos</button>
      <button class="primary" onclick={() => remove(session, true)}>Keep the Videos</button>
    {/snippet}
  </Modal>
{/if}

<style>
  aside {
    width: 270px;
    flex-shrink: 0;
    background: var(--sidebar);
    border-right: 1px solid var(--line);
    overflow-y: auto;
    padding: 12px 10px;
  }
  .top { padding: 0 6px 10px; }
  .top strong, .top button { white-space: nowrap; }
  h3 {
    font-size: 11px;
    text-transform: uppercase;
    letter-spacing: 0.06em;
    color: var(--muted);
    margin: 16px 6px 6px;
  }
  ul { list-style: none; margin: 0; padding: 0; }
  li { display: flex; align-items: center; border-radius: var(--radius); }
  li:hover { background: var(--hover); }
  li.selected { background: var(--selected); }
  .session { flex: 1; min-width: 0; padding: 6px 8px; display: flex; flex-direction: column; cursor: pointer; }
  .name { font-weight: 500; }
  .split { display: flex; }
  .split button:first-child { border-radius: 6px 0 0 6px; }
  .split .arrow { border-radius: 0 6px 6px 0; border-left: none; padding: 2px 5px; line-height: 0; }
  .more { color: var(--muted); font-size: 16px; line-height: 1; padding: 4px 8px; }
  .hint { margin: 10px 8px 0; }
  .tabs { display: flex; gap: 2px; padding: 2px; margin: 14px 0 4px; background: var(--bg); border: 1px solid var(--line); border-radius: var(--radius); }
  .tabs button {
    flex: 1; min-width: 0; border: none; background: transparent; color: var(--muted); font-size: 12px;
    padding: 4px; display: flex; flex-direction: column; align-items: center; white-space: nowrap;
  }
  .tabs button.on { background: var(--panel); color: var(--text); font-weight: 600; box-shadow: 0 1px 2px rgba(0, 0, 0, 0.15); }
  .count { font-size: 11px; font-weight: 500; color: var(--muted); }
  .backdrop { position: fixed; inset: 0; z-index: 20; background: transparent; border: none; padding: 0; cursor: default; }
  .popup {
    position: fixed; z-index: 21; min-width: 220px; display: flex; flex-direction: column; padding: 4px;
    background: var(--panel); border: 1px solid var(--line); border-radius: var(--radius);
    box-shadow: 0 10px 30px rgba(0, 0, 0, 0.22);
  }
  .popup .heading { padding: 4px 10px 6px; white-space: nowrap; }
  .popup button { text-align: left; padding: 6px 10px; border-radius: 4px; }
  .popup button:hover:not(:disabled) { background: var(--hover); }
  .popup hr { border: none; border-top: 1px solid var(--line); margin: 4px 6px; width: auto; }
  .none { padding: 6px 8px; }
  .notice { margin: 6px; padding: 8px 10px; border-radius: var(--radius); font-size: 12px; }
  .warn { background: rgba(183, 121, 31, 0.15); color: var(--warn); }
  .error { background: rgba(192, 57, 43, 0.12); color: var(--danger); }
  .card {
    display: flex;
    gap: 10px;
    align-items: center;
    width: 100%;
    text-align: left;
    margin: 6px 0;
    border-style: dashed;
  }
</style>
