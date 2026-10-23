<script lang="ts">
  import { untrack } from "svelte";
  import { api, message, type SessionView } from "../api";
  import Modal from "./Modal.svelte";
  import { runningTask } from "./tasks.svelte";
  import { segment } from "./sessions";

  // The detector thresholds for a session's DVR footage. A change applies at
  // the next segmentation, which can start from here.
  let { session, onclose, onchange }: {
    session: SessionView;
    onclose: () => void;
    onchange: () => void;
  } = $props();

  // The choice starts at the session's profile and is the viewer's after.
  let profile = $state(untrack(() => session.detectionProfile));
  let problem = $state<string | null>(null);
  let segmenting = $derived(runningTask(`segment:${session.id}`) !== undefined);
  let segmented = $derived(session.flights.some((f) => !f.actionCam));

  async function apply(andSegment: boolean) {
    try {
      if (profile !== session.detectionProfile) await api.setDetectionProfile(session.id, profile);
      onchange();
      onclose();
      // The task bar shows it from here; the dialog need not stay.
      if (andSegment) segment(session);
    } catch (e) {
      problem = `Cannot change the detection profile: ${message(e)}`;
    }
  }
</script>

<Modal title="Detection profile — {session.name}" width={520} {onclose}>
  <label class="choice">
    <input type="radio" name="profile" value="standard" bind:group={profile} />
    <span><strong>Standard</strong><br /><span class="caption">The tuned thresholds, right for most DVR footage.</span></span>
  </label>
  <label class="choice">
    <input type="radio" name="profile" value="textured" bind:group={profile} />
    <span><strong>Textured terrain</strong><br /><span class="caption">For footage whose clean picture reads as
      snow — rock faces, alpine slopes, a softer camera — shown as one flight shattered into many
      short ones. Also absorbs dropouts under 1 s.</span></span>
  </label>
  <p class="caption">A change applies at the next segmentation.</p>
  {#if problem}<p class="error caption">{problem}</p>{/if}
  {#snippet footer()}
    <button onclick={onclose}>Cancel</button>
    <button onclick={() => apply(false)} disabled={profile === session.detectionProfile}>Save</button>
    <button class="primary" onclick={() => apply(true)} disabled={segmenting}>
      Save and {segmented ? "Re-segment" : "Segment"}</button>
  {/snippet}
</Modal>

<style>
  .choice { display: flex; gap: 10px; align-items: flex-start; padding: 8px 0; }
  .choice input { margin-top: 3px; accent-color: var(--accent); }
  .error { color: var(--danger); }
</style>
