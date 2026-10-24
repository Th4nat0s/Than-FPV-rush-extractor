<script lang="ts">
  import { day, type SessionView } from "../api";
  import Icon from "./Icon.svelte";
  import { browse, show } from "./browse.svelte";
  import { counted, figures, filterFigures, filterFlights, labelsOf, placed, sameLabel } from "./library";

  // The All flights and Figures tabs' filters: tags or figures picked from
  // what the library holds, one session or all, liked only.
  let { sessions }: { sessions: SessionView[] } = $props();

  let onFigures = $derived(browse.view === "figures");
  let filter = $derived(onFigures ? browse.figures : browse.flights);
  let picked = $derived(onFigures ? browse.figures.figures : browse.flights.labels);
  let noun = $derived(onFigures ? "figure" : "tag");

  // What can be picked, counted over what the other filters leave: a tag
  // offered with its count shows what choosing it would list.
  let names = $derived(onFigures
    ? counted(filterFigures(figures(sessions), { ...browse.figures, figures: [] }).map((f) => f.tag.label))
    : counted(filterFlights(placed(sessions), { ...browse.flights, labels: [] }).flatMap(labelsOf)));
  let matching = $derived(onFigures
    ? filterFigures(figures(sessions), browse.figures).length
    : filterFlights(placed(sessions), browse.flights).length);

  let query = $state("");
  let focused = $state(false);
  let suggestions = $derived(names
    .filter(({ name }) => !picked.some((p) => sameLabel(p, name))
                          && name.toLowerCase().includes(query.trim().toLowerCase()))
    .slice(0, 8));

  function setPicked(next: string[]) {
    show(() => {
      if (onFigures) browse.figures.figures = next;
      else browse.flights.labels = next;
    });
  }

  function add(name: string) {
    setPicked([...picked, name]);
    query = "";
  }

  function keydown(event: KeyboardEvent) {
    if (event.key === "Enter" && suggestions.length) {
      event.preventDefault();
      add(suggestions[0].name);
    } else if (event.key === "Backspace" && !query && picked.length) {
      setPicked(picked.slice(0, -1));
    } else if (event.key === "Escape") {
      (event.currentTarget as HTMLInputElement).blur();
    }
  }
</script>

<div class="filters">
  <div class="row heading">
    <h3>{onFigures ? "Figures" : "Tags"}</h3>
    <span class="spacer"></span>
    {#if picked.length}<button class="plain small clear" onclick={() => setPicked([])}>Clear</button>{/if}
  </div>
  {#if !onFigures}
    <div class="segmented" role="radiogroup" aria-label="Match the tags">
      <button role="radio" aria-checked={browse.flights.match === "any"} class:on={browse.flights.match === "any"}
              onclick={() => show(() => (browse.flights.match = "any"))}>Any of them</button>
      <button role="radio" aria-checked={browse.flights.match === "all"} class:on={browse.flights.match === "all"}
              onclick={() => show(() => (browse.flights.match = "all"))}>All of them</button>
    </div>
  {/if}
  <label class="combo" class:focused>
    <Icon name="search" />
    {#each picked as name (name)}
      <span class="chip">{name}<button class="plain" aria-label="Remove {name}"
                                       onclick={() => setPicked(picked.filter((p) => p !== name))}><Icon name="close" size={10} /></button></span>
    {/each}
    <input type="text" role="combobox" aria-expanded={focused} aria-controls="filter-suggestions" aria-label="Filter by {noun}"
           placeholder={picked.length ? `Add a ${noun}…` : onFigures ? "Type a figure: split-s, power loop…" : "Type a tag: freestyle, gates…"}
           bind:value={query} onfocus={() => (focused = true)} onblur={() => (focused = false)} onkeydown={keydown} />
  </label>
  {#if focused}
    <div class="suggestions" id="filter-suggestions" role="listbox" aria-label="{noun} suggestions">
      {#each suggestions as { name, count }, index (name)}
        <!-- mousedown, not click: the input's blur would close the list first. -->
        <button role="option" aria-selected={index === 0} class:first={index === 0}
                onmousedown={(e) => { e.preventDefault(); add(name); }}>
          <span>{name}</span><span class="caption mono">{count}</span></button>
      {:else}
        <span class="caption none">{names.length ? `No matching ${noun}` : `No ${noun}s in the library yet`}</span>
      {/each}
    </div>
  {/if}

  <h3>Session</h3>
  <select aria-label="Session" value={filter.session ?? ""}
          onchange={(e) => show(() => (filter.session = e.currentTarget.value || null))}>
    <option value="">All sessions</option>
    {#each sessions as session (session.id)}
      <option value={session.id}>{session.name}{day(session.date) ? ` · ${day(session.date)}` : ""}</option>
    {/each}
  </select>

  <h3>Other</h3>
  <label class="row liked">
    <input type="checkbox" checked={filter.liked} onchange={(e) => show(() => (filter.liked = e.currentTarget.checked))} />
    Liked {onFigures ? "flights" : ""} only
  </label>

  <p class="caption">{matching} {onFigures ? (matching === 1 ? "figure" : "figures") : (matching === 1 ? "flight" : "flights")} match</p>
</div>

<style>
  .filters { display: flex; flex-direction: column; gap: 8px; padding: 0 2px; }
  h3 { font-size: 11px; text-transform: uppercase; letter-spacing: 0.06em; color: var(--muted); margin: 10px 4px 0; }
  .heading h3 { margin-top: 12px; }
  .clear { color: var(--accent); text-decoration: underline; }
  .segmented { display: flex; gap: 2px; padding: 2px; background: var(--bg); border: 1px solid var(--line); border-radius: var(--radius); }
  .segmented button { flex: 1; border: none; background: transparent; color: var(--muted); font-size: 12px; padding: 3px 8px; }
  .segmented button.on { background: var(--panel); color: var(--text); font-weight: 600; box-shadow: 0 1px 2px rgba(0, 0, 0, 0.15); }
  .combo {
    display: flex; align-items: center; gap: 4px; flex-wrap: wrap; min-height: 30px; color: var(--muted);
    background: var(--panel); border: 1px solid var(--line); border-radius: 6px; padding: 3px 6px; cursor: text;
  }
  .combo.focused { border-color: var(--accent); }
  .combo input { flex: 1; min-width: 110px; border: none; background: none; outline: none; padding: 3px 2px; font-size: 13px; color: var(--text); }
  .chip {
    display: inline-flex; align-items: center; gap: 2px; font-size: 12px; padding: 1px 2px 1px 9px;
    border-radius: 999px; background: var(--accent); color: var(--accent-text); white-space: nowrap;
  }
  .chip button { color: inherit; padding: 2px 5px; line-height: 0; }
  .suggestions {
    display: flex; flex-direction: column; padding: 4px; margin-top: -4px; background: var(--panel);
    border: 1px solid var(--line); border-radius: var(--radius); box-shadow: 0 6px 16px rgba(0, 0, 0, 0.12);
  }
  .suggestions button { display: flex; justify-content: space-between; gap: 8px; border: none; text-align: left; padding: 5px 8px; font-size: 13px; }
  .suggestions button.first { background: var(--selected); }
  .none { padding: 5px 8px; }
  select { width: 100%; }
  .liked { font-size: 13px; padding: 2px 4px; }
  .liked input { accent-color: var(--accent); margin: 0; }
  p { margin: 4px 4px 0; }
</style>
