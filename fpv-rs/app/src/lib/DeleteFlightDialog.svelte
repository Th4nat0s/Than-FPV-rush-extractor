<script lang="ts">
  import { onMount } from "svelte";
  import { api, flightName, message, type FlightDeletion, type FlightView } from "../api";
  import Modal from "./Modal.svelte";

  // "Delete Flight…": what goes with the flight — only its place in the
  // library, or files too — said before anything is touched.
  let { flight, ondeleted, onclose }: {
    flight: FlightView;
    ondeleted: () => void;
    onclose: () => void;
  } = $props();

  let plan = $state<FlightDeletion | null>(null);
  let problem = $state<string | null>(null);
  let deleting = $state(false);

  onMount(() => {
    api.flightDeletion(flight.id).then((p) => (plan = p)).catch((e) => (problem = message(e)));
  });

  async function confirm() {
    deleting = true;
    try {
      await api.deleteFlight(flight.id);
      ondeleted();
    } catch (e) {
      problem = message(e);
      deleting = false;
    }
  }
</script>

<Modal title="Delete “{flightName(flight)}”?" width={520} {onclose}>
  {#if problem}
    <p class="error">{problem}</p>
  {:else if !plan}
    <p class="caption">Looking at its files…</p>
  {:else}
    <p>Its title, like, tags and figures are removed from the library.</p>
    {#if plan.trashed.length}
      <p class="warning">{plan.trashed.length === 1 ? "This file holds" : "These files hold"} nothing but this
        flight and {plan.trashed.length === 1 ? "goes" : "go"} to the trash:</p>
      <ul>{#each plan.trashed as name (name)}<li class="mono">{name}</li>{/each}</ul>
    {/if}
    {#if plan.shared.length}
      <p class="caption">Kept, because another flight lies in {plan.shared.length === 1 ? "it" : "them"}:
        <span class="mono">{plan.shared.join(", ")}</span>.</p>
    {/if}
    {#if plan.recordings.length}
      <p class="caption">The recording{plan.recordings.length === 1 ? "" : "s"}
        <span class="mono">{plan.recordings.join(", ")}</span> {plan.recordings.length === 1 ? "stays" : "stay"} whole:
        only the flight goes, and re-segmenting the session would find it again.</p>
    {/if}
  {/if}
  {#snippet footer()}
    <button onclick={onclose}>Cancel</button>
    <button class="primary danger-fill" disabled={!plan || deleting} onclick={confirm}>
      {plan?.trashed.length ? "Delete and Trash" : "Delete"}</button>
  {/snippet}
</Modal>

<style>
  .warning { font-weight: 600; color: var(--warn); margin-bottom: 4px; }
  ul { margin: 0 0 8px; padding-left: 20px; }
  .error { color: var(--danger); }
  .danger-fill { background: var(--danger); border-color: var(--danger); }
  .danger-fill:hover:not(:disabled) { background: var(--danger); }
</style>
