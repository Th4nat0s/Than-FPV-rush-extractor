<script lang="ts">
  import { onMount, untrack } from "svelte";
  import { api, message, type SessionView } from "../api";
  import Icon from "./Icon.svelte";
  import Modal from "./Modal.svelte";

  // A session's details: its name, flying day, place, notes, and the tags
  // every flight of it carries.
  let { session, onsaved, onclose }: {
    session: SessionView;
    onsaved: () => void;
    onclose: () => void;
  } = $props();

  // The fields start from the session and are the viewer's after.
  const start = untrack(() => session);
  let name = $state(start.name);
  /** The stored instant's local day, as the date field wants it. */
  let date = $state(start.date ? localDay(start.date) : "");
  let location = $state(start.location ?? "");
  let notes = $state(start.notes ?? "");
  let labels = $state<string[]>([...start.labels]);
  let newLabel = $state("");
  let suggestions = $state<string[]>([]);
  let problem = $state<string | null>(null);
  let saving = $state(false);

  function localDay(instant: string): string {
    const d = new Date(instant);
    return `${d.getFullYear()}-${String(d.getMonth() + 1).padStart(2, "0")}-${String(d.getDate()).padStart(2, "0")}`;
  }

  onMount(() => {
    api.flightLabelNames().then((names) => (suggestions = names)).catch(() => {});
  });

  function addLabel() {
    const label = newLabel.trim();
    if (label && !labels.some((l) => l.toLowerCase() === label.toLowerCase())) labels = [...labels, label];
    newLabel = "";
  }

  async function save() {
    saving = true;
    try {
      // A tag typed and not yet added counts too.
      if (newLabel.trim()) addLabel();
      await api.updateSession(session.id, { name, date: date || null, location, notes, labels });
      onsaved();
    } catch (e) {
      problem = message(e);
      saving = false;
    }
  }
</script>

<Modal title="Edit Session" width={520} {onclose}>
  <div class="form">
    <label>Name<input type="text" bind:value={name} /></label>
    <div class="row two">
      <label>Flying day<input type="date" bind:value={date} /></label>
      <label>Place<input type="text" bind:value={location} placeholder="Col du Lautaret" /></label>
    </div>
    <div class="field">
      <span>Tags <span class="caption">— every flight of the session carries them</span></span>
      <div class="labels">
        {#each labels as label (label)}
          <span class="label">{label}<button class="plain" aria-label="Remove the tag {label}"
                  onclick={() => (labels = labels.filter((l) => l !== label))}><Icon name="close" size={10} /></button></span>
        {/each}
        <input type="text" class="label-input" list="session-labels" bind:value={newLabel}
               placeholder={labels.length ? "Add a tag…" : "Tag the session: race, mountain…"} aria-label="Add a tag"
               onkeydown={(e) => { if (e.key === "Enter") { e.preventDefault(); addLabel(); } }} />
        <datalist id="session-labels">
          {#each suggestions.filter((n) => !labels.some((l) => l.toLowerCase() === n.toLowerCase())) as suggestion}
            <option value={suggestion}></option>
          {/each}
        </datalist>
      </div>
    </div>
    <label>Notes<textarea rows="4" bind:value={notes}></textarea></label>
    <p class="caption">Changing the day dates the session; its folders on disk keep their names.</p>
    {#if problem}<p class="error">{problem}</p>{/if}
  </div>
  {#snippet footer()}
    <button onclick={onclose}>Cancel</button>
    <button class="primary" disabled={!name.trim() || saving} onclick={save}>Save</button>
  {/snippet}
</Modal>

<style>
  .form { display: flex; flex-direction: column; gap: 12px; }
  label, .field { display: flex; flex-direction: column; gap: 4px; font-size: 13px; }
  .two { gap: 12px; align-items: stretch; }
  .two label { flex: 1; min-width: 0; }
  textarea { font: inherit; color: inherit; background: var(--panel); border: 1px solid var(--line); border-radius: 6px; padding: 5px 8px; resize: vertical; }
  .labels { display: flex; flex-wrap: wrap; gap: 4px; align-items: center; padding: 4px; border: 1px solid var(--line); border-radius: 6px; background: var(--panel); }
  .label { display: inline-flex; align-items: center; gap: 2px; font-size: 12px; padding: 1px 2px 1px 8px; border-radius: 999px; background: var(--selected); border: 1px solid var(--accent); }
  .label button { padding: 2px 4px; line-height: 0; }
  .label-input { flex: 1; min-width: 140px; border: none; background: none; outline: none; }
  p { margin: 0; }
  .error { color: var(--danger); }
</style>
