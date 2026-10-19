<script lang="ts">
  import { untrack } from "svelte";
  import { api, message, type Info } from "../api";

  // How each session's folder is named in the import folder. Previewed as it
  // is typed, saved on demand; folders already imported keep their names.
  let { pattern, onchange }: { pattern: string; onchange: (info: Info) => void } = $props();

  let draft = $state(untrack(() => pattern));
  let preview = $state("");
  let problem = $state<string | null>(null);
  let saving = $state(false);

  $effect(() => {
    const text = draft;
    api.folderPreview(text, "Alpes")
      .then((name) => { preview = name; problem = null; })
      .catch((e) => { preview = ""; problem = message(e); });
  });

  async function save() {
    saving = true;
    try {
      onchange(await api.setFolderPattern(draft));
    } catch (e) {
      problem = message(e);
    } finally {
      saving = false;
    }
  }
</script>

<div class="row">
  <input type="text" class="mono" bind:value={draft} spellcheck="false" aria-label="Folder name pattern" />
  <button onclick={() => (draft = "{date} {session} {source}")} disabled={saving}>Default</button>
  <button class="primary" onclick={save} disabled={saving || !!problem || draft.trim() === pattern}>Save</button>
</div>
{#if problem}
  <p class="caption error">{problem}</p>
{:else}
  <p class="caption">Example: <span class="mono">{preview}</span></p>
{/if}
<p class="caption">
  <span class="mono">{"{date}"}</span> the flying date (left out when there is none),
  <span class="mono">{"{session}"}</span> the session's name,
  <span class="mono">{"{source}"}</span> what recorded it — FATSHARK, GOPRO, DJI, INSTA360 or CAM.
  With <span class="mono">{"{source}"}</span>, a session holding DVR and GoPro footage gets one folder for each.
</p>

<style>
  input { flex: 1; min-width: 0; }
  .error { color: var(--danger); }
  p { margin: 6px 0 0; }
</style>
