<script lang="ts">
  import { api, day, message, type Card, type SessionView } from "../api";
  import Icon from "./Icon.svelte";
  import Modal from "./Modal.svelte";

  let { sessions, cards, selectedId, ffmpegOk, error, onselect, onimportcard, onimportfolder,
        onsettings, onchange, onerror }: {
    sessions: SessionView[];
    cards: Card[];
    selectedId: string | null;
    ffmpegOk: boolean;
    error: string | null;
    onselect: (id: string) => void;
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
    <button class="small" onclick={onimportfolder} disabled={!ffmpegOk}>Import Folder…</button>
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

  <h3>Sessions</h3>
  <ul>
    {#each sessions as session (session.id)}
      <li class:selected={session.id === selectedId}>
        <div class="session" role="button" tabindex="0"
             onclick={() => onselect(session.id)}
             onkeydown={(e) => e.key === "Enter" && e.target === e.currentTarget && onselect(session.id)}>
          <input class="inline name" value={session.name} aria-label="Session name"
                 onfocus={() => onselect(session.id)}
                 onkeydown={(e) => e.key === "Enter" && (e.currentTarget as HTMLInputElement).blur()}
                 onblur={(e) => rename(session, e.currentTarget)} />
          <span class="caption">{subtitle(session)}</span>
        </div>
        <button class="plain delete" title="Delete session" onclick={() => (removing = session)}>✕</button>
      </li>
    {:else}
      <li class="caption none">No sessions yet.</li>
    {/each}
  </ul>
</aside>

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
  .delete { visibility: hidden; color: var(--muted); }
  li:hover .delete { visibility: visible; }
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
