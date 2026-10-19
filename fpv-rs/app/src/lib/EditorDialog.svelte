<script lang="ts">
  import { flightName, type FlightView } from "../api";
  import ClipEditor from "./ClipEditor.svelte";
  import Modal from "./Modal.svelte";

  // The review UI: nudge each clip's boundaries against the source frame. The
  // detector works on a 0.25 s grid (~6 frames); only a human makes a
  // boundary frame-exact.
  let { flight, onclose }: { flight: FlightView; onclose: () => void } = $props();
</script>

<Modal title="Boundaries — {flightName(flight)}" width={680} {onclose}>
  {#each flight.clips as clip (clip.id)}
    <ClipEditor {clip} />
  {/each}
  {#snippet footer()}<button class="primary" onclick={onclose}>Done</button>{/snippet}
</Modal>
