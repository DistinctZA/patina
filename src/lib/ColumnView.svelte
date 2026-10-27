<script lang="ts">
  import { api, formatSize, isApp, kindOf, type Entry } from "./api";
  import ColumnList from "./ColumnList.svelte";
  import { startDrag } from "./drag.svelte";
  import FileIcon from "./FileIcon.svelte";
  import type { PaneState } from "./pane-state.svelte";

  let {
    pane,
    home,
    active,
    oncontext,
  }: {
    pane: PaneState;
    home: string;
    active: boolean;
    oncontext: (e: MouseEvent, index: number | null) => void;
  } = $props();

  type Column = { path: string; entries: Entry[]; open: string };

  /** Folders from the start (home or the disk) down to the pane's folder. */
  const chain = $derived.by(() => {
    if (pane.tag) return [pane.path];
    const underHome = !!home && (pane.path === home || pane.path.startsWith(home + "/"));
    const base = underHome ? home : "/";
    const parts = pane.path.slice(underHome ? home.length : 0).split("/").filter(Boolean);
    const dirs = [base];
    let acc = underHome ? home : "";
    for (const part of parts) {
      acc += "/" + part;
      dirs.push(acc);
    }
    return dirs;
  });

  let parents = $state.raw<Column[]>([]);
  let peek = $state.raw<{ path: string; entries: Entry[] } | null>(null);
  let strip = $state<HTMLDivElement>();

  // Parent columns: list every folder above this one, marking the open child.
  $effect(() => {
    const dirs = chain.slice(0, -1);
    const { showHidden, sortBy, desc } = pane;
    let alive = true;
    Promise.all(
      dirs.map((dir, i) =>
        api
          .listDir(dir, showHidden, sortBy, desc)
          .then((l) => ({ path: l.path, entries: l.entries, open: chain[i + 1] }))
          .catch(() => ({ path: dir, entries: [], open: chain[i + 1] })),
      ),
    ).then((cols) => alive && (parents = cols));
    return () => (alive = false);
  });

  // Peek column: a folder under the cursor shows its contents to the right.
  $effect(() => {
    const cur = pane.current;
    const { showHidden, sortBy, desc } = pane;
    if (!cur || !cur.isDir || isApp(cur)) {
      peek = null;
      return;
    }
    let alive = true;
    // A short wait so holding an arrow key doesn't list every folder it passes.
    const timer = setTimeout(() => {
      api
        .listDir(cur.path, showHidden, sortBy, desc)
        .then((l) => alive && (peek = { path: l.path, entries: l.entries }))
        .catch(() => alive && (peek = { path: cur.path, entries: [] }));
    }, 90);
    return () => {
      alive = false;
      clearTimeout(timer);
    };
  });

  // Keep the pane's own column fully in view, then as much to its right as
  // fits. Parent columns scroll off to the left when space runs out.
  $effect(() => {
    void parents;
    void peek;
    void pane.current;
    if (!strip) return;
    const own = strip.children[parents.length] as HTMLElement | undefined;
    const overflow = strip.scrollWidth - strip.clientWidth;
    strip.scrollLeft = Math.max(0, Math.min(own?.offsetLeft ?? 0, overflow));
  });

  /**
   * Clicking in another column makes that column the pane's folder, with the
   * item selected; a folder then shows its contents in the peek column.
   */
  function pickIn(dir: string, entry: Entry, e: MouseEvent) {
    pane.load(dir, { focus: entry.path }).then(() => pane.moveCursor(pane.cursor));
    startDrag(e, () => ({ kind: "entries", entries: [entry] }));
  }

  /** Right-click in another column: go there with the item selected, then show the menu. */
  function menuIn(dir: string, entry: Entry, e: MouseEvent) {
    pane.load(dir, { focus: entry.path }).then(() => {
      pane.moveCursor(pane.cursor);
      oncontext(e, pane.cursor);
    });
  }

  const current = $derived(pane.current);
</script>

<div class="strip" bind:this={strip} data-autoscroll="x">
  {#each parents as col (col.path)}
    <ColumnList
      path={col.path}
      entries={col.entries}
      highlight={col.open}
      onrowdown={(i, e) => pickIn(col.path, col.entries[i], e)}
      onrowdbl={(entry) => pane.open(entry)}
      onrowmenu={(i, e) => menuIn(col.path, col.entries[i], e)}
    />
  {/each}

  <ColumnList
    path={pane.path}
    entries={pane.entries}
    cursor={pane.cursor}
    selected={pane.selected}
    focused={active}
    onrowdown={(i, e) => {
      pane.click(i, e);
      startDrag(e, () => ({ kind: "entries", entries: pane.targets() }));
    }}
    onrowdbl={(entry) => pane.open(entry)}
    onrowmenu={(i, e) => {
      if (!pane.selected.has(pane.entries[i].path)) pane.moveCursor(i);
      oncontext(e, i);
    }}
  />

  {#if peek}
    {@const dir = peek.path}
    <ColumnList
      path={dir}
      entries={peek.entries}
      onrowdown={(i, e) => pickIn(dir, peek!.entries[i], e)}
      onrowdbl={(entry) => pane.open(entry)}
      onrowmenu={(i, e) => menuIn(dir, peek!.entries[i], e)}
    />
  {:else if current && (!current.isDir || isApp(current))}
    <div class="file">
      <FileIcon entry={current} size={128} thumbnail />
      <strong title={current.name}>{current.name}</strong>
      <span>{kindOf(current)}{#if !current.isDir} · {formatSize(current.size)}{/if}</span>
    </div>
  {/if}
</div>

<style>
  .strip {
    flex: 1;
    min-height: 0;
    display: flex;
    overflow-x: auto;
    overflow-y: hidden;
  }
  .file {
    flex: none;
    box-sizing: border-box;
    width: 240px;
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: 6px;
    padding: 16px;
    text-align: center;
  }
  .file strong {
    max-width: 100%;
    margin-top: 8px;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .file span {
    font-size: 12px;
    color: var(--faint);
  }
</style>
