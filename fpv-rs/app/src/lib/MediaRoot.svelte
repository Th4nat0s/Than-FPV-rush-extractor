<script lang="ts">
  import { open } from "@tauri-apps/plugin-dialog";
  import { api, message, type Info } from "../api";
  import Icon from "./Icon.svelte";

  // Where imports copy recordings to, and the button that changes it. Shared
  // by the import dialog and the settings: one setting, saved to config.json.
  let { root, disabled = false, onchange }: {
    root: string;
    disabled?: boolean;
    onchange: (info: Info) => void;
  } = $props();

  let busy = $state(false);
  let failure = $state<string | null>(null);

  async function choose() {
    const folder = await open({ directory: true, defaultPath: root, title: "Import recordings into" });
    if (typeof folder !== "string" || folder === root) return;
    busy = true;
    failure = null;
    try {
      onchange(await api.setMediaRoot(folder));
    } catch (e) {
      failure = message(e);
    } finally {
      busy = false;
    }
  }
</script>

<div class="row destination">
  <Icon name="folder" />
  <!-- Ellipsis at the start: the folder's own name is the part that matters. -->
  <span class="path mono" title={root}><bdi>{root}</bdi></span>
  <button class="small" onclick={choose} disabled={disabled || busy}>{busy ? "…" : "Change…"}</button>
</div>
{#if failure}<p class="caption error">{failure}</p>{/if}

<style>
  .destination {
    padding: 6px 8px;
    border: 1px solid var(--line);
    border-radius: 6px;
    background: var(--bg);
  }
  .path {
    flex: 1;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    font-size: 12px;
    direction: rtl;
    text-align: left;
  }
  .error { color: var(--danger); margin: 4px 0 0; }
</style>
