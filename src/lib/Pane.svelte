<script lang="ts">
  import { formatDate, formatSize, type SortBy } from "./api";
  import type { PaneState } from "./pane-state.svelte";

  let { pane, active, onactivate }: { pane: PaneState; active: boolean; onactivate: () => void } =
    $props();

  // Fixed-height rows let us render only what's on screen, so folders with
  // tens of thousands of files scroll as smoothly as small ones.
  const ROW = 24;
  const OVERSCAN = 12;

  let viewport = $state<HTMLDivElement>();
  let scrollTop = $state(0);
  let height = $state(0);

  const first = $derived(Math.max(0, Math.floor(scrollTop / ROW) - OVERSCAN));
  const last = $derived(Math.min(pane.entries.length, Math.ceil((scrollTop + height) / ROW) + OVERSCAN));
  const visible = $derived(pane.entries.slice(first, last));

  const crumbs = $derived.by(() => {
    const parts = pane.path.split("/").filter(Boolean);
    return [
      { label: "/", path: "/" },
      ...parts.map((label, i) => ({ label, path: "/" + parts.slice(0, i + 1).join("/") })),
    ];
  });

  const summary = $derived.by(() => {
    const count = pane.entries.length;
    const sel = pane.selected.size;
    const items = `${count} item${count === 1 ? "" : "s"}`;
    return sel > 1 ? `${sel} of ${items} selected` : items;
  });

  // Keep the cursor row in view when it moves or the folder changes.
  $effect(() => {
    const top = pane.cursor * ROW;
    void pane.entries;
    if (!viewport) return;
    if (top < viewport.scrollTop) viewport.scrollTop = top;
    else if (top + ROW > viewport.scrollTop + viewport.clientHeight)
      viewport.scrollTop = top + ROW - viewport.clientHeight;
    scrollTop = viewport.scrollTop;
  });

  const columns: { key: SortBy; label: string }[] = [
    { key: "name", label: "Name" },
    { key: "size", label: "Size" },
    { key: "modified", label: "Date Modified" },
  ];
</script>

<section class="pane" class:active onmousedown={onactivate} role="presentation">
  <nav class="crumbs">
    {#each crumbs as crumb, i (crumb.path)}
      {#if i > 1}<span class="sep">›</span>{/if}
      <button class="crumb" onclick={() => pane.load(crumb.path)} title={crumb.path}>
        {crumb.label}
      </button>
    {/each}
  </nav>

  <div class="header row-grid">
    {#each columns as col (col.key)}
      <button class="col" class:sorted={pane.sortBy === col.key} onclick={() => pane.setSort(col.key)}>
        {col.label}
        {#if pane.sortBy === col.key}<span class="arrow">{pane.desc ? "▾" : "▴"}</span>{/if}
      </button>
    {/each}
  </div>

  <div
    class="list"
    bind:this={viewport}
    bind:clientHeight={height}
    onscroll={() => (scrollTop = viewport!.scrollTop)}
  >
    <div class="spacer" style:height="{pane.entries.length * ROW}px">
      {#each visible as entry, i (entry.path)}
        {@const index = first + i}
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
            {#if entry.isDir}
              <svg class="icon folder" viewBox="0 0 16 16" aria-hidden="true">
                <path d="M1.5 3.5A1.5 1.5 0 0 1 3 2h3.2l1.6 1.6H13A1.5 1.5 0 0 1 14.5 5v7.5A1.5 1.5 0 0 1 13 14H3a1.5 1.5 0 0 1-1.5-1.5z" />
              </svg>
            {:else}
              <svg class="icon file" viewBox="0 0 16 16" aria-hidden="true">
                <path d="M4 1.5h5.2L12.5 4.8V13A1.5 1.5 0 0 1 11 14.5H4A1.5 1.5 0 0 1 2.5 13V3A1.5 1.5 0 0 1 4 1.5z" />
              </svg>
            {/if}
            <span class="label">{entry.name}</span>
            {#if entry.isSymlink}<span class="link" title="Alias">↗</span>{/if}
          </span>
          <span class="size">{entry.isDir ? "—" : formatSize(entry.size)}</span>
          <span class="date">{formatDate(entry.modified)}</span>
        </div>
      {/each}
    </div>
    {#if !pane.entries.length && !pane.loading}
      <p class="empty">This folder is empty</p>
    {/if}
  </div>

  <footer class="status">
    <span>{summary}</span>
    {#if pane.showHidden}<span class="badge">hidden files shown</span>{/if}
  </footer>
</section>

<style>
  .pane {
    display: flex;
    flex-direction: column;
    min-width: 0;
    min-height: 0;
    background: var(--pane);
    border: 1px solid var(--border);
    border-radius: 8px;
    overflow: hidden;
  }
  .pane.active {
    border-color: var(--accent-soft);
    box-shadow: 0 0 0 1px var(--accent-soft);
  }

  .crumbs {
    display: flex;
    align-items: center;
    gap: 2px;
    padding: 6px 8px;
    border-bottom: 1px solid var(--border);
    overflow-x: auto;
    white-space: nowrap;
    scrollbar-width: none;
  }
  .crumb {
    padding: 2px 6px;
    border-radius: 4px;
    color: var(--muted);
  }
  .crumb:last-child {
    color: var(--text);
    font-weight: 600;
  }
  .crumb:hover {
    background: var(--hover);
  }
  .sep {
    color: var(--faint);
  }

  .row-grid {
    display: grid;
    grid-template-columns: minmax(0, 1fr) 88px 168px;
    align-items: center;
    column-gap: 12px;
    padding: 0 10px;
  }

  .header {
    height: 26px;
    border-bottom: 1px solid var(--border);
    font-size: 11px;
    color: var(--muted);
  }
  .col {
    text-align: left;
    padding: 0;
  }
  .col.sorted {
    color: var(--text);
    font-weight: 600;
  }
  .arrow {
    margin-left: 3px;
    color: var(--accent);
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
    left: 0;
    right: 0;
    height: 24px;
    border-radius: 5px;
    margin: 0 4px;
    padding: 0 6px;
  }
  .row.stripe {
    background: var(--stripe);
  }
  .row.selected {
    background: var(--selection-muted);
  }
  .active .row.selected {
    background: var(--selection);
    color: var(--selection-text);
  }
  .row.cursor {
    outline: 1px solid var(--accent-soft);
    outline-offset: -1px;
  }
  .hidden-file {
    opacity: 0.55;
  }

  .name {
    display: flex;
    align-items: center;
    gap: 6px;
    min-width: 0;
  }
  .label {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .icon {
    flex: none;
    width: 15px;
    height: 15px;
  }
  .icon.folder {
    fill: var(--folder);
  }
  .icon.file {
    fill: none;
    stroke: var(--muted);
    stroke-width: 1.1;
  }
  .active .selected .icon.file {
    stroke: currentColor;
  }
  .link {
    font-size: 10px;
    color: var(--muted);
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
  .active .selected .size,
  .active .selected .date {
    color: inherit;
    opacity: 0.85;
  }

  .empty {
    position: absolute;
    inset: 40% 0 auto;
    text-align: center;
    color: var(--faint);
  }

  .status {
    display: flex;
    justify-content: space-between;
    padding: 4px 10px;
    border-top: 1px solid var(--border);
    font-size: 11px;
    color: var(--muted);
  }
  .badge {
    color: var(--accent);
  }
</style>
