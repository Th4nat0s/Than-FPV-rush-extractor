<script lang="ts">
  import type { Info } from "../api";
  import FolderPattern from "./FolderPattern.svelte";
  import MediaRoot from "./MediaRoot.svelte";
  import Modal from "./Modal.svelte";

  let { info, onchange, onclose }: {
    info: Info;
    onchange: (info: Info) => void;
    onclose: () => void;
  } = $props();
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

  {#snippet footer()}<button class="primary" onclick={onclose}>Done</button>{/snippet}
</Modal>

<style>
  h3 { font-size: 13px; margin: 14px 0 4px; }
  h3:first-child { margin-top: 0; }
  p { margin: 0 0 8px; }
</style>
