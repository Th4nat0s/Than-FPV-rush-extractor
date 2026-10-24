<script lang="ts">
  import { onMount } from "svelte";
  import { clearFinished, fraction, tasks, type Task } from "./tasks.svelte";
  import { clearNotices, dismiss, markRead, notices, unread } from "./notices.svelte";
  import Icon from "./Icon.svelte";

  // The strip along the bottom of the window: how many jobs are running and
  // how far along they are together, and the errors nobody has looked at yet;
  // a click lists either.
  let open = $state<"tasks" | "notices" | null>(null);
  let unseen = $derived(unread());
  let now = $state(Date.now());

  let running = $derived(tasks.filter((t) => t.state === "running"));
  let failed = $derived(tasks.filter((t) => t.state === "failed").length);
  let finished = $derived(tasks.length - running.length);
  // The mean of what the running jobs can measure; a job that cannot say
  // (a transcode) leaves the bar to the others, or makes it indeterminate.
  let measured = $derived(running.map(fraction).filter((f): f is number => f !== null));
  let overall = $derived(measured.length ? measured.reduce((a, b) => a + b, 0) / measured.length : null);

  onMount(() => {
    // Elapsed times tick while the list is open.
    const timer = setInterval(() => (now = Date.now()), 1000);
    return () => clearInterval(timer);
  });

  function elapsed(task: Task): string {
    const seconds = Math.max(0, Math.round(((task.ended ?? now) - task.started) / 1000));
    return seconds < 60 ? `${seconds} s` : `${Math.floor(seconds / 60)} min ${String(seconds % 60).padStart(2, "0")} s`;
  }

  function toggle(panel: "tasks" | "notices") {
    open = open === panel ? null : panel;
    // Opening the list is reading it: the counter goes.
    if (open === "notices") markRead();
  }

  const time = (at: number) => new Date(at).toLocaleTimeString([], { hour: "2-digit", minute: "2-digit" });
</script>

<svelte:window onkeydown={(e) => { if (e.key === "Escape") open = null; }} />

{#if open}
  <!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
  <div class="backdrop" onclick={() => (open = null)}></div>
{/if}
{#if open === "notices"}
  <div class="panel" role="dialog" aria-label="Notifications">
    <header class="row">
      <strong>Notifications</strong>
      <span class="spacer"></span>
      <button class="plain small" onclick={clearNotices} disabled={!notices.length}>Clear all</button>
    </header>
    {#if notices.length}
      <ul>
        {#each notices as notice (notice.id)}
          <li class="notice">
            <div class="row">
              <span class="mark">✕</span>
              <span class="text">{notice.text}</span>
              <button class="plain small dismiss" aria-label="Dismiss" onclick={() => dismiss(notice.id)}>
                <Icon name="close" size={10} /></button>
            </div>
            <div class="caption">{time(notice.at)}{notice.count > 1 ? ` · ${notice.count} times` : ""}</div>
          </li>
        {/each}
      </ul>
    {:else}
      <p class="caption idle">No errors.</p>
    {/if}
  </div>
{:else if open === "tasks"}
  <div class="panel" role="dialog" aria-label="Tasks">
    <header class="row">
      <strong>Tasks</strong>
      <span class="spacer"></span>
      <button class="plain small" onclick={clearFinished} disabled={!finished}>Clear finished</button>
    </header>
    {#if tasks.length}
      <ul>
        {#each tasks as task (task.id)}
          {@const part = fraction(task)}
          <li class={task.state}>
            <div class="row">
              <span class="mark">
                {#if task.state === "running"}<span class="spinner"></span>
                {:else if task.state === "done"}✓{:else}✕{/if}
              </span>
              <span class="title">{task.title}</span>
              <span class="spacer"></span>
              <span class="caption mono">{elapsed(task)}</span>
            </div>
            {#if task.state === "running"}
              {#if part !== null}<progress value={part}></progress>{:else}<progress></progress>{/if}
            {/if}
            <div class="caption line" title={task.line}>{task.line}</div>
          </li>
        {/each}
      </ul>
    {:else}
      <p class="caption idle">Nothing has run yet.</p>
    {/if}
  </div>
{/if}

<footer>
  <button class="plain summary" onclick={() => toggle("tasks")} aria-expanded={open === "tasks"}
          title="Show the tasks">
    {#if running.length}
      <span class="spinner"></span>
      <span>{running.length} task{running.length === 1 ? "" : "s"} running</span>
      {#if overall !== null}<progress value={overall}></progress>{:else}<progress></progress>{/if}
      <span class="caption line">{running[0].title} — {running[0].line}</span>
    {:else}
      <span class="caption">No task running</span>
      {#if failed}<span class="failed">{failed} failed</span>{/if}
    {/if}
    <span class="spacer"></span>
    <span class="caption chevron">{open === "tasks" ? "▾" : "▴"}</span>
  </button>
  <button class="plain bell" class:unseen onclick={() => toggle("notices")} aria-expanded={open === "notices"}
          title={unseen ? `${unseen} new error${unseen === 1 ? "" : "s"}` : "Show the notifications"}>
    <Icon name="bell" size={13} />
    {#if unseen}<span class="count">{unseen}</span>{/if}
  </button>
</footer>

<style>
  footer {
    flex-shrink: 0;
    height: 28px;
    border-top: 1px solid var(--line);
    background: var(--panel);
    position: relative;
    z-index: 21;
    display: flex;
  }
  .summary {
    flex: 1;
    min-width: 0;
    height: 100%;
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 0 12px;
    border-radius: 0;
    font-size: 12px;
    text-align: left;
  }
  .summary progress { width: 140px; flex-shrink: 0; }
  .line { min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .failed { color: var(--danger); }
  .bell {
    display: flex; align-items: center; gap: 5px; padding: 0 12px; border-radius: 0;
    border-left: 1px solid var(--line); color: var(--muted);
  }
  .bell.unseen { color: var(--danger); }
  .count {
    min-width: 16px; padding: 0 5px; border-radius: 999px; background: var(--danger); color: #fff;
    font-size: 11px; font-weight: 600; line-height: 16px; text-align: center;
  }
  li.notice .mark { color: var(--danger); align-self: flex-start; }
  li.notice .row { align-items: flex-start; }
  .text { flex: 1; min-width: 0; overflow-wrap: anywhere; user-select: text; }
  .dismiss { line-height: 0; padding: 3px; }
  .backdrop { position: fixed; inset: 0; z-index: 20; }
  .panel {
    position: fixed;
    right: 12px;
    bottom: 36px;
    width: min(440px, calc(100vw - 24px));
    max-height: 60vh;
    display: flex;
    flex-direction: column;
    background: var(--panel);
    border: 1px solid var(--line);
    border-radius: var(--radius);
    box-shadow: 0 8px 28px rgba(0, 0, 0, 0.25);
    z-index: 21;
  }
  .panel header { padding: 8px 12px; border-bottom: 1px solid var(--line); }
  ul { list-style: none; margin: 0; padding: 0; overflow: auto; }
  li { padding: 8px 12px; border-bottom: 1px solid var(--line); display: flex; flex-direction: column; gap: 4px; }
  li:last-child { border-bottom: none; }
  li progress { width: 100%; }
  li.failed .mark, li.failed .line { color: var(--danger); }
  li.done .mark { color: var(--accent); }
  .mark { width: 16px; display: inline-flex; justify-content: center; }
  .title { font-weight: 500; min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .idle { padding: 12px; margin: 0; }
  .spinner {
    width: 11px; height: 11px; flex-shrink: 0;
    border: 2px solid var(--line); border-top-color: var(--accent); border-radius: 50%;
    animation: spin 0.9s linear infinite;
  }
  @keyframes spin { to { transform: rotate(360deg); } }
</style>
