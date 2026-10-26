<script lang="ts">
  import { onMount, tick } from "svelte";
  import { listen } from "@tauri-apps/api/event";
  import {
    ChevronLeft,
    ChevronRight,
    Copy,
    Eye,
    EyeOff,
    FolderInput,
    FolderPlus,
    LayoutGrid,
    List,
    PanelLeft,
    PanelRight,
    PencilLine,
    RefreshCw,
    Search,
    Trash,
    X,
  } from "@lucide/svelte";
  import { api, type SidebarData } from "$lib/api";
  import { PaneState } from "$lib/pane-state.svelte";
  import Pane from "$lib/Pane.svelte";
  import Preview from "$lib/Preview.svelte";
  import Sidebar from "$lib/Sidebar.svelte";

  const left = new PaneState("left");
  const right = new PaneState("right");
  const panes = { left, right };

  let activeId = $state<"left" | "right">("left");
  const active = $derived(panes[activeId]);
  const other = $derived(activeId === "left" ? right : left);

  let home = $state("");
  let sidebarData = $state<SidebarData | null>(null);
  let showSidebar = $state(true);
  let showPreview = $state(true);
  let searchInput = $state<HTMLInputElement>();

  type Prompt = { title: string; action: string; value: string; submit: (value: string) => Promise<void> };
  let prompt = $state<Prompt | null>(null);
  let toast = $state<string | null>(null);
  let busy = $state(false);
  let toastTimer: ReturnType<typeof setTimeout>;

  function showError(err: unknown) {
    toast = String(err);
    clearTimeout(toastTimer);
    toastTimer = setTimeout(() => (toast = null), 5000);
  }

  async function run(task: () => Promise<unknown>) {
    busy = true;
    try {
      await task();
    } catch (err) {
      showError(err);
    } finally {
      busy = false;
    }
  }

  async function refreshBoth() {
    await Promise.all([left.refresh(), right.refresh()]);
  }

  // ── Commands ───────────────────────────────────────────────────────────

  function transfer(kind: "copy" | "move") {
    const paths = active.targets().map((e) => e.path);
    if (!paths.length) return;
    if (!other.isFolder) return showError("Open a folder in the other pane first.");
    const op = kind === "copy" ? api.copyInto : api.moveInto;
    run(() => op(paths, other.path).then(refreshBoth));
  }

  function trash() {
    const paths = active.targets().map((e) => e.path);
    if (paths.length) run(() => api.moveToTrash(paths).then(refreshBoth));
  }

  function newFolder() {
    const pane = active;
    if (!pane.isFolder) return showError("Open a folder first.");
    prompt = {
      title: "New folder",
      action: "Create",
      value: "untitled folder",
      submit: async (name) => {
        const created = await api.createFolder(pane.path, name);
        await pane.load(pane.path, { focus: created });
        pane.moveCursor(pane.cursor);
      },
    };
  }

  function rename() {
    const pane = active;
    const entry = pane.current;
    if (!entry) return;
    prompt = {
      title: `Rename “${entry.name}”`,
      action: "Rename",
      value: entry.name,
      submit: async (name) => {
        const renamed = await api.rename(entry.path, name);
        await pane.load(pane.path, { focus: renamed, keep: true });
        pane.moveCursor(pane.cursor);
      },
    };
  }

  function openLocation(location: string) {
    run(() => active.load(location));
  }

  async function submitPrompt(e: SubmitEvent) {
    e.preventDefault();
    if (!prompt) return;
    const { submit, value } = prompt;
    try {
      await submit(value);
      prompt = null;
    } catch (err) {
      showError(err);
    }
  }

  /** Focuses the dialog input and selects the name without its extension. */
  function selectName(input: HTMLInputElement) {
    tick().then(() => {
      input.focus();
      const dot = input.value.lastIndexOf(".");
      input.setSelectionRange(0, dot > 0 ? dot : input.value.length);
    });
  }

  function searchKey(e: KeyboardEvent) {
    if (e.key === "Escape") {
      active.setFilter("");
      searchInput?.blur();
    } else if (e.key === "Enter" || e.key === "ArrowDown") {
      // Hand the keyboard back to the list, keeping the filter.
      e.preventDefault();
      searchInput?.blur();
      active.moveCursor(0);
    }
  }

  // ── Keyboard ───────────────────────────────────────────────────────────

  let typed = "";
  let typedTimer: ReturnType<typeof setTimeout>;

  function onKey(e: KeyboardEvent) {
    if (prompt) {
      if (e.key === "Escape") prompt = null;
      return;
    }
    // Let text fields have their keys; checkboxes (permissions) shouldn't
    // swallow the shortcuts after being clicked.
    if (e.target instanceof HTMLInputElement && e.target.type !== "checkbox") return;

    const pane = active;
    const grid = pane.view === "grid";
    const step = grid ? pane.columns : 1;
    const page = Math.max(1, Math.floor((window.innerHeight - 200) / (grid ? 132 : 26)) - 1) * step;
    const cmd = e.metaKey;

    const handled = (() => {
      switch (true) {
        case e.key === "Tab":
          activeId = activeId === "left" ? "right" : "left";
          return true;
        case e.key === "ArrowDown" && cmd:
        case e.key === "Enter":
          run(async () => pane.open());
          return true;
        case e.key === "ArrowUp" && cmd:
        case e.key === "Backspace" && !cmd:
          run(async () => pane.goUp());
          return true;
        case e.key === "[" && cmd:
          run(async () => pane.goBack());
          return true;
        case e.key === "]" && cmd:
          run(async () => pane.goForward());
          return true;
        case e.key === "ArrowDown":
          pane.moveCursor(pane.cursor + step, e.shiftKey);
          return true;
        case e.key === "ArrowUp":
          pane.moveCursor(pane.cursor - step, e.shiftKey);
          return true;
        case e.key === "ArrowRight" && grid:
          pane.moveCursor(pane.cursor + 1, e.shiftKey);
          return true;
        case e.key === "ArrowLeft" && grid:
          pane.moveCursor(pane.cursor - 1, e.shiftKey);
          return true;
        case e.key === "PageDown":
          pane.moveCursor(pane.cursor + page, e.shiftKey);
          return true;
        case e.key === "PageUp":
          pane.moveCursor(pane.cursor - page, e.shiftKey);
          return true;
        case e.key === "Home":
          pane.moveCursor(0, e.shiftKey);
          return true;
        case e.key === "End":
          pane.moveCursor(pane.entries.length - 1, e.shiftKey);
          return true;
        case e.key === "F5":
          transfer("copy");
          return true;
        case e.key === "F6":
          transfer("move");
          return true;
        case e.key === "F2":
          rename();
          return true;
        case e.key === "F7":
        case e.key.toLowerCase() === "n" && cmd && e.shiftKey:
          newFolder();
          return true;
        case e.key === "F8":
        case e.key === "Backspace" && cmd:
          trash();
          return true;
        // Shift and Option change e.key on a Mac (⇧. is ">", ⌥R is "®"), so
        // these match the physical key instead.
        case e.code === "Period" && cmd && e.shiftKey:
          run(() => pane.toggleHidden());
          return true;
        case e.code === "KeyR" && cmd && e.altKey:
          if (pane.current) run(() => api.revealInFinder(pane.current!.path));
          return true;
        case e.code === "KeyR" && cmd:
          run(refreshBoth);
          return true;
        case e.code === "KeyP" && cmd && e.shiftKey:
          showPreview = !showPreview;
          return true;
        case e.code === "KeyS" && cmd && e.ctrlKey:
          showSidebar = !showSidebar;
          return true;
        case (e.code === "Digit1" || e.key === "1") && cmd:
          pane.view = "list";
          return true;
        case (e.code === "Digit2" || e.key === "2") && cmd:
          pane.view = "grid";
          return true;
        case e.key === "f" && cmd:
          searchInput?.focus();
          return true;
        case e.key === "a" && cmd:
          pane.selectAll();
          return true;
        case e.key === "Escape":
          if (pane.filter) pane.setFilter("");
          else pane.clearSelection();
          return true;
        case e.key.length === 1 && !cmd && !e.ctrlKey && !e.altKey:
          // Type the start of a name to jump to it.
          clearTimeout(typedTimer);
          typed += e.key;
          typedTimer = setTimeout(() => (typed = ""), 800);
          pane.jumpTo(typed);
          return true;
        default:
          return false;
      }
    })();

    if (handled) e.preventDefault();
  }

  // ── Startup ────────────────────────────────────────────────────────────

  const loadSidebar = () =>
    api
      .sidebar()
      .then((d) => (sidebarData = d))
      .catch(showError);

  onMount(() => {
    // Refresh a pane shortly after its folder changes on disk; bursts of
    // events (e.g. a big copy) collapse into one reload.
    const timers: Record<string, ReturnType<typeof setTimeout>> = {};
    const unlistenDir = listen<{ pane: "left" | "right"; path: string }>("dir-changed", ({ payload }) => {
      const pane = panes[payload.pane];
      if (!pane || pane.path !== payload.path) return;
      clearTimeout(timers[payload.pane]);
      timers[payload.pane] = setTimeout(() => {
        // Skip if the user has since moved on, so a refresh of the old folder
        // can't override their navigation.
        if (pane.loading || pane.path !== payload.path) return;
        pane.refresh().catch(showError);
      }, 150);
    });
    const unlistenVolumes = listen("volumes-changed", () => {
      clearTimeout(timers.volumes);
      timers.volumes = setTimeout(loadSidebar, 500);
    });

    loadSidebar();
    api
      .homeDir()
      .then((h) => {
        home = h;
        return Promise.all([left.load(h), right.load(h)]);
      })
      .catch(showError);

    return () => {
      unlistenDir.then((fn) => fn());
      unlistenVolumes.then((fn) => fn());
    };
  });

  const hasTargets = $derived(active.selected.size > 0 || !!active.current);
</script>

<svelte:window onkeydown={onKey} />
<svelte:head><title>Patina — {active.title}</title></svelte:head>

<!-- Clicking a button keeps keyboard focus on the panes (no focus ring, and
     Return still opens the selected file). -->
<div
  class="app"
  class:no-sidebar={!showSidebar}
  role="presentation"
  onmousedown={(e) => (e.target as Element).closest("button") && e.preventDefault()}
>
  {#if showSidebar}
    <Sidebar data={sidebarData} current={active.path} onopen={openLocation} />
  {/if}

  <div class="main">
    <header class="toolbar" data-tauri-drag-region>
      <div class="group">
        <button onclick={() => run(async () => active.goBack())} disabled={!active.canGoBack} title="Back (⌘[)">
          <ChevronLeft size={17} />
        </button>
        <button onclick={() => run(async () => active.goForward())} disabled={!active.canGoForward} title="Forward (⌘])">
          <ChevronRight size={17} />
        </button>
      </div>
      <h1 data-tauri-drag-region>{active.title}</h1>

      <span class="flex" data-tauri-drag-region></span>

      <div class="group">
        <button class:on={active.view === "grid"} onclick={() => (active.view = "grid")} title="Icons (⌘2)">
          <LayoutGrid size={16} />
        </button>
        <button class:on={active.view === "list"} onclick={() => (active.view = "list")} title="List (⌘1)">
          <List size={16} />
        </button>
      </div>

      <div class="group">
        <button class:on={showSidebar} onclick={() => (showSidebar = !showSidebar)} title="Sidebar (⌃⌘S)">
          <PanelLeft size={16} />
        </button>
        <button class:on={showPreview} onclick={() => (showPreview = !showPreview)} title="Preview (⌘⇧P)">
          <PanelRight size={16} />
        </button>
        <button class:on={active.showHidden} onclick={() => run(() => active.toggleHidden())} title="Hidden files (⌘⇧.)">
          {#if active.showHidden}<Eye size={16} />{:else}<EyeOff size={16} />{/if}
        </button>
      </div>

      <div class="group">
        <button onclick={() => run(refreshBoth)} title="Refresh (⌘R)"><RefreshCw size={15} /></button>
        <button onclick={() => transfer("copy")} disabled={busy || !hasTargets} title="Copy to other pane (F5)">
          <Copy size={16} />
        </button>
        <button onclick={() => transfer("move")} disabled={busy || !hasTargets} title="Move to other pane (F6)">
          <FolderInput size={16} />
        </button>
        <button onclick={newFolder} disabled={busy} title="New folder (F7)"><FolderPlus size={16} /></button>
        <button onclick={rename} disabled={busy || !active.current} title="Rename (F2)"><PencilLine size={16} /></button>
        <button onclick={trash} disabled={busy || !hasTargets} title="Move to Bin (⌘⌫)"><Trash size={16} /></button>
      </div>

      <label class="search">
        <Search size={14} />
        <input
          bind:this={searchInput}
          value={active.filter}
          oninput={(e) => active.setFilter(e.currentTarget.value)}
          onkeydown={searchKey}
          placeholder="Search"
          spellcheck="false"
          autocomplete="off"
        />
        {#if active.filter}
          <button class="clear" onclick={() => active.setFilter("")} title="Clear"><X size={12} /></button>
        {/if}
      </label>
    </header>

    <div class="body" class:with-preview={showPreview}>
      <div class="panes">
        <Pane pane={left} {home} active={activeId === "left"} onactivate={() => (activeId = "left")} />
        <Pane pane={right} {home} active={activeId === "right"} onactivate={() => (activeId = "right")} />
      </div>
      {#if showPreview}
        <Preview entry={active.current} onerror={showError} />
      {/if}
    </div>
  </div>

  {#if toast}
    <div class="toast" role="alert">{toast}</div>
  {/if}

  {#if prompt}
    <div
      class="backdrop"
      role="presentation"
      onmousedown={(e) => e.target === e.currentTarget && (prompt = null)}
    >
      <form class="dialog" onsubmit={submitPrompt}>
        <h2>{prompt.title}</h2>
        <input bind:value={prompt.value} use:selectName spellcheck="false" autocomplete="off" />
        <div class="actions">
          <button type="button" onclick={() => (prompt = null)}>Cancel</button>
          <button type="submit" class="primary" disabled={!prompt.value.trim()}>{prompt.action}</button>
        </div>
      </form>
    </div>
  {/if}
</div>

<style>
  /* Slate greys with a darker inset sidebar; blue folders, orange accents. */
  :global(:root) {
    --bg: #36373c;
    --pane: #36373c;
    --sidebar: #2b2c30;
    --panel: #313236;
    --well: #2a2b2f;
    --border: #2a2b2f;
    --line: #45464b;
    --text: #f2f2f3;
    --muted: #c3c4c8;
    --faint: #8f9095;
    --hover: rgba(255, 255, 255, 0.06);
    --stripe: rgba(255, 255, 255, 0.015);
    --selection: #4c4d53;
    --selection-muted: #414247;
    --cursor: rgba(255, 255, 255, 0.16);
    --cursor-label: #2f6fe4;
    --cursor-label-text: #ffffff;
    --active-bar: #f2a33a;
    --accent: #f2a33a;
    --blue: #2f6fe4;
    --folder: #5ab0f5;
    --file: #e4e5e8;
    --button: #2b2c30;
    --button-hover: #3a3b40;
    --button-on: #4c4d53;
    color-scheme: dark;
  }

  :global(html, body) {
    margin: 0;
    height: 100%;
    overflow: hidden;
    background: var(--bg);
    color: var(--text);
    font: 13px/1.3 -apple-system, BlinkMacSystemFont, "SF Pro Text", system-ui, sans-serif;
    -webkit-user-select: none;
    user-select: none;
    cursor: default;
  }
  :global(button) {
    font: inherit;
    color: inherit;
    background: none;
    border: 0;
    padding: 0;
    cursor: default;
  }
  :global(::-webkit-scrollbar) {
    width: 10px;
    height: 10px;
  }
  :global(button:focus) {
    outline: none;
  }
  :global(::-webkit-scrollbar-thumb) {
    background: #55565c;
    border-radius: 5px;
    border: 2px solid transparent;
    background-clip: padding-box;
  }

  .app {
    display: grid;
    grid-template-columns: 210px minmax(0, 1fr);
    height: 100vh;
  }
  .app.no-sidebar {
    grid-template-columns: minmax(0, 1fr);
  }

  .main {
    display: flex;
    flex-direction: column;
    min-width: 0;
    min-height: 0;
  }

  .toolbar {
    flex: none;
    display: flex;
    align-items: center;
    gap: 8px;
    height: 52px;
    padding: 0 12px;
    background: var(--bg);
  }
  /* The window's traffic lights sit here when the sidebar is hidden. */
  .no-sidebar .toolbar {
    padding-left: 84px;
  }
  .toolbar h1 {
    margin: 0 4px;
    min-width: 0;
    font-size: 14px;
    font-weight: 600;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .flex {
    flex: 1;
    align-self: stretch;
  }
  .group {
    flex: none;
    display: flex;
    gap: 2px;
    padding: 3px;
    border-radius: 9px;
    background: var(--button);
    border: 1px solid var(--border);
  }
  .group button {
    display: grid;
    place-items: center;
    width: 30px;
    height: 26px;
    border-radius: 6px;
    color: var(--muted);
  }
  .group button:hover:not(:disabled) {
    background: var(--button-hover);
    color: var(--text);
  }
  .group button.on {
    background: var(--button-on);
    color: var(--text);
  }
  .group button:disabled {
    opacity: 0.35;
  }

  .search {
    flex: 0 1 220px;
    min-width: 120px;
    display: flex;
    align-items: center;
    gap: 6px;
    height: 32px;
    padding: 0 10px;
    border-radius: 9px;
    background: var(--button);
    border: 1px solid var(--border);
    color: var(--faint);
  }
  .search:focus-within {
    border-color: var(--line);
  }
  .search input {
    flex: 1;
    min-width: 0;
    border: 0;
    outline: none;
    background: none;
    color: var(--text);
    font: inherit;
  }
  .search .clear {
    display: grid;
    place-items: center;
    width: 16px;
    height: 16px;
    border-radius: 50%;
    background: var(--button-on);
    color: var(--text);
  }

  .body {
    flex: 1;
    min-height: 0;
    display: grid;
    grid-template-columns: minmax(0, 1fr);
  }
  .body.with-preview {
    grid-template-columns: minmax(0, 1fr) 320px;
  }
  .panes {
    min-height: 0;
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 1px;
    background: var(--border);
  }

  .toast {
    position: fixed;
    left: 50%;
    bottom: 24px;
    transform: translateX(-50%);
    max-width: min(560px, 90vw);
    padding: 9px 14px;
    border-radius: 8px;
    background: var(--panel);
    border: 1px solid var(--line);
    color: var(--text);
    box-shadow: 0 10px 30px rgba(0, 0, 0, 0.5);
  }

  .backdrop {
    position: fixed;
    inset: 0;
    display: grid;
    place-items: start center;
    padding-top: 18vh;
    background: rgba(0, 0, 0, 0.45);
  }
  .dialog {
    width: min(380px, 90vw);
    padding: 16px;
    border-radius: 12px;
    background: var(--panel);
    border: 1px solid var(--line);
    box-shadow: 0 20px 50px rgba(0, 0, 0, 0.6);
  }
  .dialog h2 {
    margin: 0 0 10px;
    font-size: 13px;
    font-weight: 600;
  }
  .dialog input {
    width: 100%;
    box-sizing: border-box;
    padding: 7px 9px;
    border-radius: 7px;
    border: 1px solid var(--border);
    background: var(--well);
    color: var(--text);
    font: inherit;
    outline: none;
    -webkit-user-select: text;
    user-select: text;
  }
  .dialog input:focus {
    border-color: var(--blue);
  }
  .actions {
    display: flex;
    justify-content: flex-end;
    gap: 8px;
    margin-top: 14px;
  }
  .actions button {
    padding: 5px 12px;
    border-radius: 7px;
    background: var(--button);
    border: 1px solid var(--border);
  }
  .actions .primary {
    background: var(--blue);
    border-color: var(--blue);
    color: #fff;
    font-weight: 500;
  }
  .actions .primary:disabled {
    opacity: 0.5;
  }
</style>
