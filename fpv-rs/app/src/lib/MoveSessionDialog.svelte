<script lang="ts">
  import { onMount } from "svelte";
  import { api, bytes, message, type RelocationPreview, type SessionView } from "../api";
  import Modal from "./Modal.svelte";

  // "Move to Import Folder…": where a session's recordings are, where they
  // would go, before anything moves.
  let { session, onmove, onclose }: {
    session: SessionView;
    onmove: () => void;
    onclose: () => void;
  } = $props();

  let preview = $state<RelocationPreview | null>(null);
  let problem = $state<string | null>(null);

  onMount(() => {
    api.relocationPreview(session.id).then((p) => (preview = p)).catch((e) => (problem = message(e)));
  });
</script>

<Modal title="Move “{session.name}” to the Import Folder?" width={560} {onclose}>
  {#if problem}
    <p class="error">{problem}</p>
  {:else if !preview}
    <p class="caption">Looking at its recordings…</p>
  {:else if !preview.recordings}
    <p>Every recording of this session is already in the import folder.</p>
  {:else}
    <p>{preview.recordings} recording{preview.recordings === 1 ? "" : "s"} ({bytes(preview.bytes)}) from an earlier
      import folder move into the current one, named as new imports are.</p>
    <p class="caption">From</p>
    <ul>{#each preview.from as folder (folder)}<li class="mono">{folder}</li>{/each}</ul>
    <p class="caption">To</p>
    <ul>{#each preview.to as folder (folder)}<li class="mono">{folder}</li>{/each}</ul>
    <p class="caption">On the same disk this is instant; across disks each file is copied, checked, then removed
      from where it was. Nothing in the import folder is written over. Folders left empty are removed.</p>
  {/if}
  {#snippet footer()}
    <button onclick={onclose}>Cancel</button>
    <button class="primary" disabled={!preview?.recordings} onclick={onmove}>Move</button>
  {/snippet}
</Modal>

<style>
  ul { margin: 0 0 8px; padding-left: 20px; word-break: break-all; }
  p { margin: 0 0 6px; }
  .error { color: var(--danger); }
</style>
