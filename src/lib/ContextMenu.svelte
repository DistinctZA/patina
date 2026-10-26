<script module lang="ts">
  export type MenuItem =
    | { label: string; action: () => void; shortcut?: string; disabled?: boolean; checked?: boolean; danger?: boolean }
    | { heading: string }
    | "separator";
</script>

<script lang="ts">
  import { Check } from "@lucide/svelte";

  let { x, y, items, onclose }: { x: number; y: number; items: MenuItem[]; onclose: () => void } = $props();

  let menu = $state<HTMLDivElement>();
  let left = $state(0);
  let top = $state(0);

  // Open at the pointer, nudged back inside the window near its edges.
  $effect(() => {
    if (!menu) return;
    const { width, height } = menu.getBoundingClientRect();
    left = Math.max(6, Math.min(x, window.innerWidth - width - 6));
    top = Math.max(6, Math.min(y, window.innerHeight - height - 6));
  });

  const hasTicks = $derived(items.some((i) => typeof i === "object" && "checked" in i));

  function choose(item: Extract<MenuItem, { label: string }>) {
    if (item.disabled) return;
    onclose();
    item.action();
  }
</script>

<svelte:window
  onmousedown={(e) => !menu?.contains(e.target as Node) && onclose()}
  onkeydown={(e) => e.key === "Escape" && (e.stopPropagation(), onclose())}
  onblur={onclose}
/>

<div class="menu" role="menu" bind:this={menu} style:left="{left}px" style:top="{top}px">
  {#each items as item, i (i)}
    {#if item === "separator"}
      <hr />
    {:else if "heading" in item}
      <p>{item.heading}</p>
    {:else}
      <button
        role={"checked" in item ? "menuitemcheckbox" : "menuitem"}
        aria-checked={"checked" in item ? item.checked : undefined}
        class:danger={item.danger}
        disabled={item.disabled}
        onclick={() => choose(item)}
      >
        {#if hasTicks}<span class="tick">{#if item.checked}<Check size={13} />{/if}</span>{/if}
        <span class="label">{item.label}</span>
        {#if item.shortcut}<kbd>{item.shortcut}</kbd>{/if}
      </button>
    {/if}
  {/each}
</div>

<style>
  .menu {
    position: fixed;
    z-index: 70;
    min-width: 210px;
    padding: 5px;
    border-radius: 9px;
    background: var(--panel);
    border: 1px solid var(--line);
    box-shadow: 0 12px 30px rgba(0, 0, 0, 0.5);
  }
  p {
    margin: 3px 8px 5px;
    font-size: 11px;
    font-weight: 600;
    color: var(--faint);
  }
  button {
    display: flex;
    align-items: center;
    gap: 6px;
    width: 100%;
    padding: 4px 8px;
    border-radius: 5px;
    font-size: 13px;
    text-align: left;
  }
  button:hover:not(:disabled) {
    background: var(--blue);
    color: #fff;
  }
  button:hover:not(:disabled) kbd {
    color: rgba(255, 255, 255, 0.8);
  }
  button.danger:hover:not(:disabled) {
    background: #c9483a;
  }
  button:disabled {
    color: var(--faint);
  }
  .label {
    flex: 1;
  }
  .tick {
    display: grid;
    width: 14px;
  }
  kbd {
    font: 11px -apple-system, BlinkMacSystemFont, system-ui, sans-serif;
    color: var(--faint);
  }
  hr {
    margin: 5px 4px;
    border: 0;
    border-top: 1px solid var(--line);
  }
</style>
