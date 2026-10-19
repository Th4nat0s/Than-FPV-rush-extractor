<script lang="ts">
  import type { Snippet } from "svelte";

  let { title, width = 480, onclose, children, footer }: {
    title: string;
    width?: number;
    /** Escape and the backdrop call this; leave it out to make the modal sticky. */
    onclose?: () => void;
    children: Snippet;
    footer?: Snippet;
  } = $props();

  function keydown(event: KeyboardEvent) {
    if (event.key === "Escape" && onclose) onclose();
  }
</script>

<svelte:window onkeydown={keydown} />

<div class="backdrop" role="presentation" onclick={(e) => e.target === e.currentTarget && onclose?.()}>
  <div class="modal" role="dialog" aria-modal="true" aria-label={title} style:width="{width}px">
    <header><h2>{title}</h2></header>
    <div class="body">{@render children()}</div>
    {#if footer}<footer>{@render footer()}</footer>{/if}
  </div>
</div>

<style>
  .backdrop {
    position: fixed;
    inset: 0;
    background: rgba(0, 0, 0, 0.45);
    display: flex;
    align-items: center;
    justify-content: center;
    z-index: 10;
  }
  .modal {
    max-width: calc(100vw - 32px);
    max-height: calc(100vh - 32px);
    display: flex;
    flex-direction: column;
    background: var(--panel);
    border: 1px solid var(--line);
    border-radius: 12px;
    box-shadow: 0 20px 60px rgba(0, 0, 0, 0.35);
  }
  header { padding: 16px 20px 0; }
  h2 { margin: 0; font-size: 16px; font-weight: 600; }
  .body { padding: 14px 20px; overflow: auto; flex: 1; }
  footer {
    padding: 12px 20px 16px;
    display: flex;
    justify-content: flex-end;
    gap: 8px;
  }
</style>
