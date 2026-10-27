<script lang="ts">
  import { ChevronRight } from "@lucide/svelte";
  import { isApp, type Entry } from "./api";
  import { drag } from "./drag.svelte";
  import FileIcon from "./FileIcon.svelte";

  let {
    path,
    entries,
    cursor = -1,
    selected = new Set<string>(),
    highlight = null,
    focused = false,
    onrowdown,
    onrowdbl,
    onrowmenu,
  }: {
    /** The folder this column lists; dropping on empty space goes here. */
    path: string;
    entries: Entry[];
    /** Row with the keyboard cursor (the pane's own column only). */
    cursor?: number;
    selected?: Set<string>;
    /** Path shown as "open" in a parent column. */
    highlight?: string | null;
    /** Whether this column has the keyboard (draws selection in full colour). */
    focused?: boolean;
    onrowdown: (index: number, e: MouseEvent) => void;
    onrowdbl: (entry: Entry) => void;
    onrowmenu: (index: number, e: MouseEvent) => void;
  } = $props();

  const isDropTarget = (p: string) => !!drag.verdict && drag.overPath === p;

  const ROW = 24;
  const OVERSCAN = 8;

  let viewport = $state<HTMLDivElement>();
  let scrollTop = $state(0);
  let height = $state(0);

  const first = $derived(Math.max(0, Math.floor(scrollTop / ROW) - OVERSCAN));
  const last = $derived(Math.min(entries.length, Math.ceil((scrollTop + height) / ROW) + OVERSCAN));
  const visible = $derived(entries.slice(first, last));

  // Keep the cursor (or the open folder in a parent column) in view.
  $effect(() => {
    const target = cursor >= 0 ? cursor : highlight ? entries.findIndex((e) => e.path === highlight) : -1;
    if (!viewport || target < 0) return;
    const top = target * ROW;
    if (top < viewport.scrollTop) viewport.scrollTop = top;
    else if (top + ROW > viewport.scrollTop + viewport.clientHeight)
      viewport.scrollTop = top + ROW - viewport.clientHeight;
    scrollTop = viewport.scrollTop;
  });
</script>

<div
  class="column"
  class:drop-into={isDropTarget(path)}
  data-drop={path.startsWith("tag:") ? undefined : "folder"}
  data-drop-path={path.startsWith("tag:") ? undefined : path}
  data-autoscroll="y"
  bind:this={viewport}
  bind:clientHeight={height}
  onscroll={() => (scrollTop = viewport!.scrollTop)}
>
  <div class="spacer" style:height="{entries.length * ROW + 8}px">
    {#each visible as entry, i (entry.path)}
      {@const index = first + i}
      <div
        class="row"
        class:cursor={focused && index === cursor}
        class:selected={selected.has(entry.path) || entry.path === highlight}
        class:focused
        class:hidden-file={entry.isHidden}
        class:drop-target={isDropTarget(entry.path)}
        style:transform="translateY({index * ROW + 4}px)"
        onmousedown={(e) => onrowdown(index, e)}
        ondblclick={() => onrowdbl(entry)}
        oncontextmenu={(e) => {
          e.preventDefault();
          e.stopPropagation();
          onrowmenu(index, e);
        }}
        data-drop={entry.isDir && !isApp(entry) ? "folder" : undefined}
        data-drop-path={entry.isDir && !isApp(entry) ? entry.path : undefined}
        data-spring={entry.isDir && !isApp(entry) ? "" : undefined}
        role="row"
        tabindex="-1"
      >
        <FileIcon {entry} size={16} />
        <span class="label">{entry.name}</span>
        {#if entry.isDir && !isApp(entry)}<ChevronRight size={13} class="more" />{/if}
      </div>
    {/each}
  </div>
  {#if !entries.length}<p class="empty">Empty</p>{/if}
</div>

<style>
  .column {
    flex: none;
    position: relative;
    width: 230px;
    height: 100%;
    overflow-y: auto;
    border-right: 1px solid var(--border);
  }
  .spacer {
    position: relative;
  }
  .row {
    position: absolute;
    top: 0;
    left: 5px;
    right: 5px;
    height: 24px;
    display: flex;
    align-items: center;
    gap: 7px;
    padding: 0 6px;
    border-radius: 5px;
  }
  .row.selected {
    background: var(--selection-muted);
  }
  .row.selected.focused {
    background: var(--selection);
  }
  .column.drop-into {
    box-shadow: inset 0 0 0 2px var(--blue);
  }
  .row.drop-target {
    background: rgba(47, 111, 228, 0.28) !important;
    box-shadow: inset 0 0 0 1.5px var(--blue);
  }
  .row.cursor {
    box-shadow: inset 0 0 0 1px var(--cursor);
  }
  .hidden-file {
    opacity: 0.5;
  }
  .label {
    flex: 1;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .row :global(.more) {
    flex: none;
    color: var(--faint);
  }
  .empty {
    position: absolute;
    inset: 30% 0 auto;
    text-align: center;
    color: var(--faint);
    font-size: 12px;
  }
</style>
