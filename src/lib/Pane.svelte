<script lang="ts">
  import { ChevronRight, Folder, HardDrive, House, Tag } from "@lucide/svelte";
  import { TAG_COLORS, formatDate, formatSize, type SortBy } from "./api";
  import FileIcon from "./FileIcon.svelte";
  import type { PaneState } from "./pane-state.svelte";

  let {
    pane,
    active,
    home,
    onactivate,
  }: { pane: PaneState; active: boolean; home: string; onactivate: () => void } = $props();

  // Only rows on screen are drawn, so huge folders stay smooth.
  const ROW = 26;
  const CELL_W = 124;
  const CELL_H = 132;
  const THUMB = 76;
  const OVERSCAN = 6;

  let viewport = $state<HTMLDivElement>();
  let scrollTop = $state(0);
  let height = $state(0);
  let width = $state(0);

  const grid = $derived(pane.view === "grid");
  const perRow = $derived(grid ? Math.max(1, Math.floor((width - 16) / CELL_W)) : 1);
  const rowH = $derived(grid ? CELL_H : ROW);
  const rows = $derived(Math.ceil(pane.entries.length / perRow));
  const firstRow = $derived(Math.max(0, Math.floor(scrollTop / rowH) - OVERSCAN));
  const lastRow = $derived(Math.min(rows, Math.ceil((scrollTop + height) / rowH) + OVERSCAN));
  const first = $derived(firstRow * perRow);
  const visible = $derived(pane.entries.slice(first, Math.min(pane.entries.length, lastRow * perRow)));
  // Centre the grid's columns in the available width.
  const gridPad = $derived(Math.max(8, (width - perRow * CELL_W) / 2));

  $effect(() => {
    pane.columns = perRow;
  });

  const crumbs = $derived.by(() => {
    if (pane.tag) return [{ label: pane.tag, path: pane.path }];
    const underHome = pane.path === home || pane.path.startsWith(home + "/");
    const base = underHome ? home : "";
    const parts = pane.path.slice(base.length).split("/").filter(Boolean);
    const root = underHome
      ? { label: home.split("/").pop() ?? "Home", path: home }
      : { label: "Macintosh HD", path: "/" };
    return [root, ...parts.map((label, i) => ({ label, path: base + "/" + parts.slice(0, i + 1).join("/") }))];
  });

  const HeaderIcon = $derived(pane.tag ? Tag : pane.path === home ? House : pane.path === "/" ? HardDrive : Folder);

  const summary = $derived.by(() => {
    const count = pane.entries.length;
    const items = `${count} item${count === 1 ? "" : "s"}`;
    const picked = pane.entries.filter((e) => pane.selected.has(e.path));
    let text = items;
    if (picked.length > 0) {
      const bytes = picked.reduce((sum, e) => sum + (e.isDir ? 0 : e.size), 0);
      text = `${picked.length} of ${count} selected` + (bytes ? ` (${formatSize(bytes)})` : "");
    }
    if (pane.filter) text += ` matching “${pane.filter}”`;
    if (pane.available != null) text += `, ${formatSize(pane.available)} available`;
    return text;
  });

  // Keep the cursor in view when it moves or the folder changes.
  $effect(() => {
    const top = Math.floor(pane.cursor / perRow) * rowH;
    void pane.entries;
    if (!viewport) return;
    if (top < viewport.scrollTop) viewport.scrollTop = top;
    else if (top + rowH > viewport.scrollTop + viewport.clientHeight)
      viewport.scrollTop = top + rowH - viewport.clientHeight;
    scrollTop = viewport.scrollTop;
  });

  const columns: { key: SortBy; label: string }[] = [
    { key: "name", label: "Name" },
    { key: "size", label: "Size" },
    { key: "modified", label: "Date Modified" },
  ];
</script>

<section class="pane" class:active onmousedown={onactivate} role="presentation">
  <header class="location">
    <span class="loc-icon"><HeaderIcon size={22} strokeWidth={1.5} /></span>
    <div class="loc-text">
      <nav class="crumbs">
        {#each crumbs as crumb, i (crumb.path)}
          {#if i > 0}<ChevronRight size={12} class="sep" />{/if}
          <button class="crumb" onclick={() => pane.load(crumb.path)} title={crumb.path}>{crumb.label}</button>
        {/each}
      </nav>
      <div class="summary">{summary}</div>
    </div>
  </header>

  {#if !grid}
    <div class="header row-grid">
      {#each columns as col (col.key)}
        <button class="col" class:sorted={pane.sortBy === col.key} onclick={() => pane.setSort(col.key)}>
          {col.label}
          {#if pane.sortBy === col.key}<span class="arrow">{pane.desc ? "▾" : "▴"}</span>{/if}
        </button>
      {/each}
    </div>
  {/if}

  <div
    class="list"
    bind:this={viewport}
    bind:clientHeight={height}
    bind:clientWidth={width}
    onscroll={() => (scrollTop = viewport!.scrollTop)}
  >
    <div class="spacer" style:height="{rows * rowH + (grid ? 12 : 0)}px">
      {#each visible as entry, i (entry.path)}
        {@const index = first + i}
        {#if grid}
          <div
            class="cell"
            class:cursor={index === pane.cursor}
            class:selected={pane.selected.has(entry.path)}
            class:hidden-file={entry.isHidden}
            style:transform="translate({gridPad + (index % perRow) * CELL_W}px, {Math.floor(index / perRow) * CELL_H + 8}px)"
            onmousedown={(e) => pane.click(index, e)}
            ondblclick={() => pane.open(entry)}
            role="gridcell"
            tabindex="-1"
          >
            <div class="thumb"><FileIcon {entry} size={THUMB} thumbnail /></div>
            <span class="cell-name" title={entry.name}>{entry.name}</span>
            {#if entry.tags.some((t) => t.color)}
              <span class="tags">
                {#each entry.tags.filter((t) => t.color) as t (t.name)}
                  <span class="dot" style:background={TAG_COLORS[t.color]}></span>
                {/each}
              </span>
            {/if}
          </div>
        {:else}
          <div
            class="row row-grid"
            class:stripe={index % 2 === 1}
            class:cursor={index === pane.cursor}
            class:selected={pane.selected.has(entry.path)}
            class:hidden-file={entry.isHidden}
            style:transform="translateY({index * ROW}px)"
            onmousedown={(e) => pane.click(index, e)}
            ondblclick={() => pane.open(entry)}
            role="row"
            tabindex="-1"
          >
            <span class="name">
              <FileIcon {entry} size={16} />
              <span class="label">{entry.name}</span>
              {#if entry.isSymlink}<span class="link" title="Alias">↗</span>{/if}
              {#each entry.tags.filter((t) => t.color) as t (t.name)}
                <span class="dot" style:background={TAG_COLORS[t.color]} title={t.name}></span>
              {/each}
            </span>
            <span class="size">{entry.isDir ? "—" : formatSize(entry.size)}</span>
            <span class="date">{formatDate(entry.modified)}</span>
          </div>
        {/if}
      {/each}
    </div>
    {#if !pane.entries.length && !pane.loading}
      <p class="empty">{pane.filter ? "No matches" : pane.tag ? "No files with this tag" : "This folder is empty"}</p>
    {/if}
  </div>
</section>

<style>
  .pane {
    position: relative;
    display: flex;
    flex-direction: column;
    min-width: 0;
    min-height: 0;
    background: var(--pane);
    container-type: inline-size;
  }
  /* In a narrow pane, drop the date so names keep their room. */
  @container (max-width: 440px) {
    .row-grid {
      grid-template-columns: minmax(0, 1fr) 72px;
    }
    .date,
    .col:nth-child(3) {
      display: none;
    }
  }
  /* A thin bar along the top marks the pane that keys act on. */
  .pane::before {
    content: "";
    position: absolute;
    inset: 0 10px auto;
    height: 2px;
    border-radius: 0 0 2px 2px;
    background: transparent;
  }
  .pane.active::before {
    background: var(--active-bar);
  }

  .location {
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 10px 14px 9px;
    border-bottom: 1px solid var(--border);
  }
  .loc-icon {
    color: var(--muted);
    display: grid;
  }
  .loc-text {
    min-width: 0;
  }
  .crumbs {
    display: flex;
    align-items: center;
    gap: 1px;
    overflow-x: auto;
    white-space: nowrap;
    scrollbar-width: none;
  }
  .crumbs :global(.sep) {
    color: var(--faint);
    flex: none;
  }
  .crumb {
    padding: 1px 3px;
    border-radius: 4px;
    font-weight: 500;
    color: var(--muted);
  }
  .crumb:last-child {
    color: var(--text);
  }
  .crumb:hover {
    background: var(--hover);
  }
  .summary {
    margin: 1px 3px 0;
    font-size: 11px;
    color: var(--faint);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .row-grid {
    display: grid;
    grid-template-columns: minmax(0, 1fr) 84px 156px;
    align-items: center;
    column-gap: 12px;
    padding: 0 12px;
  }
  .header {
    height: 26px;
    border-bottom: 1px solid var(--border);
    font-size: 11px;
    color: var(--faint);
  }
  .col {
    text-align: left;
    padding: 0;
  }
  .col:nth-child(2) {
    text-align: right;
  }
  .col.sorted {
    color: var(--text);
    font-weight: 600;
  }
  .arrow {
    margin-left: 3px;
  }

  .list {
    flex: 1;
    overflow-y: auto;
    position: relative;
  }
  .spacer {
    position: relative;
  }

  .row {
    position: absolute;
    top: 0;
    left: 6px;
    right: 6px;
    height: 26px;
    border-radius: 5px;
    padding: 0 6px;
  }
  .row.stripe {
    background: var(--stripe);
  }
  .row.selected,
  .cell.selected .thumb {
    background: var(--selection-muted);
  }
  .active .row.selected,
  .active .cell.selected .thumb {
    background: var(--selection);
  }
  .active .row.cursor {
    box-shadow: inset 0 0 0 1px var(--cursor);
  }
  .hidden-file {
    opacity: 0.5;
  }

  .name {
    display: flex;
    align-items: center;
    gap: 8px;
    min-width: 0;
  }
  .label {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .link {
    font-size: 10px;
    color: var(--muted);
  }
  .dot {
    flex: none;
    width: 8px;
    height: 8px;
    border-radius: 50%;
    box-shadow: 0 0 0 1.5px var(--pane);
  }
  .dot + .dot {
    margin-left: -10px;
  }
  .size,
  .date {
    color: var(--muted);
    font-size: 12px;
    font-variant-numeric: tabular-nums;
    white-space: nowrap;
    overflow: hidden;
  }
  .size {
    text-align: right;
  }

  .cell {
    position: absolute;
    top: 0;
    left: 0;
    width: 124px;
    height: 124px;
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 6px;
  }
  .thumb {
    width: 96px;
    height: 88px;
    display: grid;
    place-items: center;
    border-radius: 8px;
  }
  .cell-name {
    max-width: 112px;
    padding: 1px 5px;
    border-radius: 4px;
    font-size: 12px;
    text-align: center;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .active .cell.cursor .cell-name {
    background: var(--cursor-label);
    color: var(--cursor-label-text);
  }
  .tags {
    display: flex;
    gap: 2px;
  }
  .tags .dot + .dot {
    margin-left: -4px;
  }

  .empty {
    position: absolute;
    inset: 40% 0 auto;
    text-align: center;
    color: var(--faint);
  }
</style>
