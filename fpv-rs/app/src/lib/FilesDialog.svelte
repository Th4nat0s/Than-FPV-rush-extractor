<script lang="ts">
  import { api, clock, message, type SessionView } from "../api";
  import Icon from "./Icon.svelte";
  import Modal from "./Modal.svelte";

  // A session's recordings, from its menu: what recorded each, its format,
  // and a way to it in the file manager.
  let { session, onclose }: { session: SessionView; onclose: () => void } = $props();
  let problem = $state<string | null>(null);
</script>

<Modal title="Files of “{session.name}”" width={820} {onclose}>
  <table>
    <thead><tr><th>File</th><th>Source</th><th>Format</th><th>Size</th><th>Duration</th><th></th></tr></thead>
    <tbody>
      {#each session.files as file (file.id)}
        <tr>
          <td>{file.name}</td>
          <td>{file.source}</td>
          <!-- The rate is per file and non-standard by design (CLAUDE.md 2). -->
          <td>{file.format}</td>
          <td class="mono">{file.width}×{file.height}</td>
          <td class="mono">{clock(file.duration)}</td>
          <td class="actions">
            <button class="plain small" title="Show in the file manager" aria-label="Show {file.name} in the file manager"
                    onclick={() => api.revealFile(file.id).catch((e) => (problem = message(e)))}>
              <Icon name="folder" /></button>
          </td>
        </tr>
      {:else}
        <tr><td colspan="6" class="caption">No recordings.</td></tr>
      {/each}
    </tbody>
  </table>
  {#if problem}<p class="error caption">{problem}</p>{/if}
  {#snippet footer()}<button class="primary" onclick={onclose}>Done</button>{/snippet}
</Modal>

<style>
  table { width: 100%; border-collapse: collapse; }
  th, td { text-align: left; padding: 7px 10px; border-bottom: 1px solid var(--line); }
  td.actions { width: 1%; padding-top: 2px; padding-bottom: 2px; text-align: right; }
  th { font-weight: 500; color: var(--muted); font-size: 12px; position: sticky; top: 0; background: var(--panel); }
  .error { color: var(--danger); }
</style>
