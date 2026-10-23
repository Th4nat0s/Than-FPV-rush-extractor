<script lang="ts">
  import Modal from "./Modal.svelte";
  import { answer, asking } from "./confirm.svelte";
  import { resegmentLosses } from "./library";

  // Mounted once in the window; shows whenever a re-segment asks.
  let losses = $derived(asking.session ? resegmentLosses(asking.session) : null);
  const plural = (n: number, word: string) => `${n} ${word}${n === 1 ? "" : "s"}`;
</script>

{#if asking.session && losses}
  <Modal title="Re-segment “{asking.session.name}”?" width={520} onclose={() => answer(false)}>
    <p class="warning">The {plural(losses.flights, "DVR flight")} of this session will be found again from
      scratch and replaced.</p>
    <p>Lost with them:</p>
    <ul>
      {#if losses.titled}<li>{plural(losses.titled, "title")}</li>{/if}
      {#if losses.liked}<li>{plural(losses.liked, "like")}</li>{/if}
      {#if losses.tagged}<li>the tags of {plural(losses.tagged, "flight")}</li>{/if}
      <li>every boundary corrected by hand</li>
    </ul>
    <p class="caption">Figures stay: they belong to the recordings. Action-cam takes are not touched.</p>
    {#snippet footer()}
      <button onclick={() => answer(false)}>Cancel</button>
      <button class="primary danger-fill" onclick={() => answer(true)}>Re-segment</button>
    {/snippet}
  </Modal>
{/if}

<style>
  .warning { font-weight: 600; color: var(--warn); }
  ul { margin: 0 0 8px; padding-left: 20px; }
  .danger-fill { background: var(--danger); border-color: var(--danger); }
  .danger-fill:hover:not(:disabled) { background: var(--danger); }
</style>
