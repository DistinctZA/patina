<script lang="ts">
  import { fileUrl, formatDate, formatSize, kindOf, mediaOf } from "./api";
  import { startDrag } from "./drag.svelte";
  import FileIcon from "./FileIcon.svelte";
  import type { PaneState } from "./pane-state.svelte";

  let { pane, active }: { pane: PaneState; active: boolean } = $props();

  // The filmstrip only draws the thumbnails in view.
  const CELL = 86;
  const OVERSCAN = 6;

  let strip = $state<HTMLDivElement>();
  let scrollLeft = $state(0);
  let width = $state(0);
  let imageFailed = $state(false);

  const first = $derived(Math.max(0, Math.floor(scrollLeft / CELL) - OVERSCAN));
  const last = $derived(Math.min(pane.entries.length, Math.ceil((scrollLeft + width) / CELL) + OVERSCAN));
  const visible = $derived(pane.entries.slice(first, last));
  const current = $derived(pane.current);

  $effect(() => {
    void current?.path;
    imageFailed = false;
  });

  // Keep the cursor's thumbnail in view, centred when it jumps.
  $effect(() => {
    const left = pane.cursor * CELL;
    void pane.entries;
    if (!strip) return;
    if (left < strip.scrollLeft || left + CELL > strip.scrollLeft + strip.clientWidth)
      strip.scrollLeft = left - strip.clientWidth / 2 + CELL / 2;
    scrollLeft = strip.scrollLeft;
  });
</script>

<div class="gallery">
  <div class="stage">
    {#if current}
      {#key current.path}
        {#if mediaOf(current) === "image" && !imageFailed}
          <img src={fileUrl(current.path)} alt={current.name} draggable="false" onerror={() => (imageFailed = true)} />
        {:else}
          <FileIcon entry={current} size={220} thumbnail />
        {/if}
      {/key}
    {/if}
  </div>

  {#if current}
    <div class="caption">
      <strong title={current.name}>{current.name}</strong>
      <span>
        {kindOf(current)}{#if !current.isDir} · {formatSize(current.size)}{/if} · {formatDate(current.modified)}
      </span>
    </div>
  {/if}

  <div
    class="filmstrip"
    bind:this={strip}
    bind:clientWidth={width}
    onscroll={() => (scrollLeft = strip!.scrollLeft)}
  >
    <div class="spacer" style:width="{pane.entries.length * CELL + 16}px">
      {#each visible as entry, i (entry.path)}
        {@const index = first + i}
        <div
          class="thumb"
          class:cursor={index === pane.cursor}
          class:selected={pane.selected.has(entry.path)}
          class:active
          style:transform="translateX({index * CELL + 8}px)"
          onmousedown={(e) => {
            pane.click(index, e);
            startDrag(e, () => ({ kind: "entries", entries: pane.targets() }));
          }}
          ondblclick={() => pane.open(entry)}
          title={entry.name}
          role="option"
          aria-selected={index === pane.cursor}
          tabindex="-1"
        >
          <FileIcon {entry} size={56} thumbnail />
        </div>
      {/each}
    </div>
  </div>
</div>

<style>
  .gallery {
    flex: 1;
    min-height: 0;
    display: flex;
    flex-direction: column;
  }
  .stage {
    position: relative;
    flex: 1;
    min-height: 0;
    display: grid;
    place-items: center;
    overflow: hidden;
  }
  /* Same fit as the preview panel: whole image, letterboxed, never cropped. */
  .stage img {
    position: absolute;
    inset: 18px;
    margin: auto;
    max-width: calc(100% - 36px);
    max-height: calc(100% - 36px);
    object-fit: contain;
    border-radius: 6px;
    filter: drop-shadow(0 6px 18px rgba(0, 0, 0, 0.45));
  }
  .caption {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 2px;
    padding: 0 16px 10px;
    text-align: center;
  }
  .caption strong {
    max-width: 100%;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .caption span {
    font-size: 12px;
    color: var(--faint);
  }
  .filmstrip {
    flex: none;
    height: 92px;
    overflow-x: auto;
    overflow-y: hidden;
    border-top: 1px solid var(--line);
    background: var(--well);
  }
  .spacer {
    position: relative;
    height: 100%;
  }
  .thumb {
    position: absolute;
    top: 8px;
    left: 0;
    width: 76px;
    height: 72px;
    display: grid;
    place-items: center;
    border-radius: 8px;
  }
  .thumb.selected {
    background: var(--selection-muted);
  }
  .thumb.selected.active {
    background: var(--selection);
  }
  .thumb.cursor.active {
    box-shadow: inset 0 0 0 2px var(--blue);
  }
</style>
