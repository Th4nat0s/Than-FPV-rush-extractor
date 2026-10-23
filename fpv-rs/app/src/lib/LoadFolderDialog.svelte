<script lang="ts">
  import { onMount } from "svelte";
  import { api, message, type FolderView } from "../api";
  import Modal from "./Modal.svelte";

  // The import folder's folders, to load one or more in place. A folder the
  // library already holds whole is shown, greyed, and cannot be picked.
  let { root, onload, onclose }: {
    root: string;
    onload: (folders: string[]) => void;
    onclose: () => void;
  } = $props();

  let folders = $state<FolderView[] | null>(null);
  let problem = $state<string | null>(null);
  let picked = $state<string[]>([]);
  let loadable = $derived((folders ?? []).filter((f) => f.new > 0));

  onMount(() => {
    api.importFolderContents().then((found) => (folders = found)).catch((e) => (problem = message(e)));
  });

  function toggle(name: string) {
    picked = picked.includes(name) ? picked.filter((p) => p !== name) : [...picked, name];
  }

  const plural = (n: number, word: string) => `${n} ${word}${n === 1 ? "" : "s"}`;
</script>

<Modal title="Load from Destination Folder" width={620} {onclose}>
  <p class="caption">Recordings already in <span class="mono">{root}</span> are added where they are, without
    copying. Each folder loads into the session its name gives.</p>
  {#if problem}
    <p class="error">{problem}</p>
  {:else if !folders}
    <p class="caption">Reading the import folder…</p>
  {:else if !folders.length}
    <p class="caption">The import folder holds no folders.</p>
  {:else}
    <ul>
      {#each folders as folder (folder.name)}
        {@const done = folder.new === 0}
        <li class:done>
          <label>
            <input type="checkbox" disabled={done} checked={picked.includes(folder.name)}
                   onchange={() => toggle(folder.name)} />
            <span class="what">
              <span class="name">{folder.name}</span>
              <span class="caption">→ {folder.session}{folder.date ? ` · ${folder.date}` : ""}</span>
            </span>
            <span class="caption count">
              {done ? "Already in library"
                    : folder.new === folder.recordings ? plural(folder.recordings, "recording")
                    : `${folder.new} new of ${folder.recordings}`}</span>
          </label>
        </li>
      {/each}
    </ul>
  {/if}
  {#snippet footer()}
    {#if loadable.length > 1}
      <button onclick={() => (picked = loadable.map((f) => f.name))}>Select all new</button>
      <span class="spacer"></span>
    {/if}
    <button onclick={onclose}>Cancel</button>
    <button class="primary" disabled={!picked.length} onclick={() => onload(picked)}>
      Load {picked.length > 1 ? `${picked.length} folders` : ""}</button>
  {/snippet}
</Modal>

<style>
  ul { list-style: none; margin: 8px 0 0; padding: 0; max-height: 50vh; overflow: auto; border: 1px solid var(--line); border-radius: var(--radius); }
  li { border-bottom: 1px solid var(--line); }
  li:last-child { border-bottom: none; }
  label { display: flex; align-items: center; gap: 10px; padding: 8px 10px; cursor: pointer; }
  li:hover:not(.done) { background: var(--hover); }
  li.done { opacity: 0.45; }
  li.done label { cursor: default; }
  input { accent-color: var(--accent); margin: 0; }
  .what { flex: 1; min-width: 0; display: flex; flex-direction: column; }
  .name { white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
  .count { white-space: nowrap; }
  .error { color: var(--danger); }
</style>
