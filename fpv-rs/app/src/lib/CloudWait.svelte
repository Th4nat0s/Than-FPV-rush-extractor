<script lang="ts">
  import { onMount } from "svelte";
  import { api, bytes, flightName, type FlightView } from "../api";
  import Modal from "./Modal.svelte";
  import { estimate, Flow, lastRate, rememberRate, timeLeft } from "./cloudwait";
  import { download } from "./sessions";
  import { tasks } from "./tasks.svelte";

  // A flight only in the cloud is brought down before it plays: the player
  // opens once it is here. Closing leaves the download running in the task
  // bar, and the flight unplayed.
  let { flight, onready, onclose }: {
    flight: FlightView;
    onready: () => void;
    onclose: () => void;
  } = $props();

  let key = $derived(`download:${flight.id}`);
  const rate = lastRate();
  let open = true;
  let now = $state(Date.now());
  let task = $derived(tasks.find((t) => t.key === key && t.state === "running"));
  let wait = $derived(task ? estimate(task.done, task.total, now - task.started, rate) : null);
  // Bytes are shown only once they are seen arriving: a drive that hands each
  // recording over whole would read 0 bytes, then all of them.
  const flow = new Flow();
  let live = $state(false);
  $effect(() => {
    if (task?.part) live = flow.see(task.done, task.total, task.part[0], Date.now());
  });

  onMount(() => {
    const timer = setInterval(() => (now = Date.now()), 500);
    const started = Date.now();
    download(flightName(flight), key, (id) => api.downloadFlight(id, flight.id)).then((summary) => {
      if (summary?.downloaded) rememberRate(summary.bytes, Date.now() - started);
      if (!open) return;
      if (summary && !summary.failed.length) onready();
      else onclose();
    });
    return () => { open = false; clearInterval(timer); };
  });
</script>

<Modal title="Downloading from the cloud" width={440} {onclose}>
  <p>{flightName(flight)} is only in the cloud: it plays as soon as it is on this computer.</p>
  {#if live && wait?.fraction != null}<progress value={wait.fraction}></progress>{:else}<progress></progress>{/if}
  <p class="caption">
    {#if live && task?.total}{bytes(task.done)} of {bytes(task.total)}
    {:else if task?.part}Video {task.part[0]} of {task.part[1]} · {bytes(task.total)}
    {:else}Starting…{/if}
    {#if timeLeft(wait?.remaining ?? null)} · {timeLeft(wait?.remaining ?? null)}{/if}
  </p>
  {#if task?.part && !live}
    <p class="caption">The cloud drive does not say how far along a video is: each one arrives whole.</p>
  {/if}
  {#snippet footer()}<button onclick={onclose}>Continue in Background</button>{/snippet}
</Modal>

<style>
  progress { width: 100%; margin: 6px 0; }
  p { margin: 0 0 6px; }
</style>
