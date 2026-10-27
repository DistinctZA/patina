<script lang="ts">
  import { onMount, tick, untrack } from "svelte";
  import { listen } from "@tauri-apps/api/event";
  import { getCurrentWebview } from "@tauri-apps/api/webview";
  import {
    ChevronLeft,
    ChevronRight,
    Columns3,
    Copy,
    Eye,
    EyeOff,
    FolderInput,
    FolderPlus,
    GalleryHorizontalEnd,
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
  import { api, basename, isApp, parentOf, type Entry, type Settings, type SidebarData, type TransferMode } from "$lib/api";
  import BatchRename from "$lib/BatchRename.svelte";
  import ContextMenu, { type MenuItem } from "$lib/ContextMenu.svelte";
  import TransferPanel from "$lib/TransferPanel.svelte";
  import { isCancelled, transfers, type JobProgress } from "$lib/transfers.svelte";
  import {
    drag,
    dropHere,
    endDrag,
    hover,
    setDropHandler,
    setLeaveWindowHandler,
    type DragPayload,
    type DropTarget,
    type Verdict,
  } from "$lib/drag.svelte";
  import { PaneState, type View } from "$lib/pane-state.svelte";
  import Pane from "$lib/Pane.svelte";
  import Preview from "$lib/Preview.svelte";
  import FileIcon from "$lib/FileIcon.svelte";
  import Sidebar from "$lib/Sidebar.svelte";

  const left = new PaneState("left");
  const right = new PaneState("right");
  const panes = { left, right };

  let activeId = $state<"left" | "right">("left");
  const active = $derived(panes[activeId]);
  const other = $derived(activeId === "left" ? right : left);

  let home = $state("");
  let sidebarData = $state<SidebarData | null>(null);
  let settings = $state<Settings>({});
  /** False until saved settings are read, so nothing overwrites them on startup. */
  let settingsReady = $state(false);
  let showSidebar = $state(true);
  let showPreview = $state(true);
  let searchInput = $state<HTMLInputElement>();

  type Prompt = {
    title: string;
    action: string;
    value: string;
    submit: (value: string) => Promise<void>;
    /** Select all of the text, rather than a file name without its extension. */
    selectAll?: boolean;
  };
  let prompt = $state<Prompt | null>(null);
  let toast = $state<string | null>(null);
  let busy = $state(false);
  let toastTimer: ReturnType<typeof setTimeout>;

  function showToast(message: string, ms: number) {
    toast = message;
    clearTimeout(toastTimer);
    toastTimer = setTimeout(() => (toast = null), ms);
  }

  const showError = (err: unknown) => showToast(String(err), 5000);
  const showNote = (message: string) => showToast(message, 2500);

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
    moveOrCopy(paths, other.path, kind);
  }

  function trash() {
    const paths = active.targets().map((e) => e.path);
    if (paths.length) run(() => api.moveToTrash(paths).then(refreshBoth));
  }

  /** A copy or move ended badly: say so (unless it was cancelled), then refresh. */
  function jobFailed(err: unknown) {
    if (isCancelled(err)) showNote("Cancelled");
    else showError(err);
    refreshBoth().catch(showError);
  }

  /**
   * Copies or moves files into a folder as a job: long ones show progress
   * with a Cancel button, and the rest of the app stays usable meanwhile.
   */
  function moveOrCopy(paths: string[], dest: string, mode: TransferMode) {
    if (!paths.length) return;
    transfers
      .run(mode === "copy" ? "Copying" : "Moving", (id) => api.transfer(id, paths, dest, mode))
      .then(refreshBoth, jobFailed);
  }

  const itemsLabel = (entries: Entry[]) => (entries.length === 1 ? `“${entries[0].name}”` : `${entries.length} items`);

  /** Puts files on the system clipboard, so they paste in Finder too. */
  function copyToClipboard(entries = active.targets()) {
    if (!entries.length) return;
    run(() => api.clipboardCopy(entries.map((e) => e.path)).then(() => showNote(`Copied ${itemsLabel(entries)}`)));
  }

  /** Pastes files copied here or in Finder into the active pane; ⌥ moves them. */
  function paste(move = false) {
    const pane = active;
    if (!pane.isFolder) return showError("Open a folder to paste into.");
    api
      .clipboardPaste()
      .then((paths) => (paths.length ? moveOrCopy(paths, pane.path, move ? "move" : "copy") : showNote("Nothing to paste")))
      .catch(showError);
  }

  function duplicate() {
    const paths = active.targets().map((e) => e.path);
    if (!paths.length) return;
    transfers.run("Duplicating", (id) => api.duplicate(id, paths)).then(refreshBoth, jobFailed);
  }

  function copyPaths(entries: Entry[]) {
    navigator.clipboard
      .writeText(entries.map((e) => e.path).join("\n"))
      .then(() => showNote(entries.length === 1 ? "Copied path" : `Copied ${entries.length} paths`))
      .catch(showError);
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

  /** Batch rename: the items being renamed, and the pane they're in. */
  let batch = $state<{ pane: PaneState; entries: Entry[]; existing: Set<string> } | null>(null);

  async function applyBatch(renames: [string, string][]) {
    const pane = batch!.pane;
    await api.renameMany(renames);
    await pane.refresh();
    // Keep the renamed items selected under their new names.
    pane.selectPaths(new Set(renames.map(([from, name]) => parentOf(from) + "/" + name)));
  }

  /** F2: one item gets a quick rename; several open Batch Rename. */
  function rename() {
    const pane = active;
    const targets = pane.targets();
    if (targets.length > 1) {
      batch = { pane, entries: targets, existing: new Set(pane.all.map((e) => e.path.toLowerCase())) };
      return;
    }
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
      const dot = prompt?.selectAll ? -1 : input.value.lastIndexOf(".");
      input.setSelectionRange(0, dot > 0 ? dot : input.value.length);
    });
  }

  /** Jumps the active pane to a typed or pasted path (⇧⌘G, like Finder). */
  function goToFolder() {
    const pane = active;
    prompt = {
      title: "Go to folder",
      action: "Go",
      value: pane.isFolder ? pane.path : home,
      selectAll: true,
      submit: async (value) => {
        let path = value.trim().replace(/^~(?=\/|$)/, home);
        // "Documents" or "../Other" are relative to the folder you're in.
        if (!path.startsWith("/")) path = `${pane.isFolder ? pane.path : home}/${path}`;
        await pane.load(path.length > 1 ? path.replace(/\/+$/, "") : path);
      },
    };
  }

  function appMouseDown(e: MouseEvent) {
    const target = e.target as Element;
    // Buttons don't take keyboard focus from the panes.
    if (target.closest("button")) e.preventDefault();
    // Clicking anywhere but text drops a text selection (say, in the
    // preview), so ⌘C goes back to copying files.
    if (!target.closest("pre, input, textarea, select, dd")) window.getSelection()?.removeAllRanges();
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
    if (batch) {
      if (e.key === "Escape") batch = null;
      return;
    }
    if (menu) return;
    // Let text fields have their keys; checkboxes (permissions) shouldn't
    // swallow the shortcuts after being clicked.
    if (e.target instanceof HTMLInputElement && e.target.type !== "checkbox") return;

    const pane = active;
    const grid = pane.view === "grid";
    const columns = pane.view === "columns";
    const sideways = grid || pane.view === "gallery";
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
        // Columns: right opens the folder, left goes back to its parent.
        case e.key === "ArrowRight" && columns:
          if (pane.current?.isDir && !isApp(pane.current)) run(async () => pane.open());
          return true;
        case e.key === "ArrowLeft" && columns:
          run(async () => pane.goUp());
          return true;
        case e.key === "ArrowRight" && sideways:
          pane.moveCursor(pane.cursor + 1, e.shiftKey);
          return true;
        case e.key === "ArrowLeft" && sideways:
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
        case e.code === "KeyG" && cmd && e.shiftKey:
          goToFolder();
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
        case /^[1-4]$/.test(e.key) && cmd:
          pane.view = VIEWS[Number(e.key) - 1].view;
          return true;
        case e.key === "f" && cmd:
          searchInput?.focus();
          return true;
        case e.key === "a" && cmd:
          pane.selectAll();
          return true;
        // ⌘C/⌘V use the physical key: ⌥ changes e.key ("√" for ⌥V).
        case e.code === "KeyC" && cmd && !e.shiftKey && !e.altKey:
          // Copying text selected in the preview keeps working.
          if (window.getSelection()?.toString()) return false;
          copyToClipboard();
          return true;
        case e.code === "KeyV" && cmd && !e.shiftKey:
          paste(e.altKey);
          return true;
        case e.code === "KeyD" && cmd:
          duplicate();
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
      .sidebar(settings.favorites ?? null)
      .then((d) => (sidebarData = d))
      .catch(showError);

  // Saves go out one at a time, in order, so a quick run of changes can't
  // finish out of order and leave an older version on disk.
  let saving: Promise<unknown> = Promise.resolve();
  function saveSettings(next: Settings) {
    settings = next;
    if (!settingsReady) return;
    saving = saving.then(() => api.writeSettings(next)).catch(showError);
  }

  /** The Favourites as shown, so the first edit starts from the defaults. */
  const favoritePaths = () => sidebarData?.favorites.map((f) => f.path) ?? [];

  function setFavorites(paths: string[]) {
    saveSettings({ ...settings, favorites: paths });
    loadSidebar();
  }

  /** Adds folders at `index` (or the end); ones already there just move. */
  function addFavorites(paths: string[], index: number | null) {
    const list = favoritePaths().filter((p) => !paths.includes(p));
    const at = Math.min(index ?? list.length, list.length);
    list.splice(at, 0, ...paths);
    setFavorites(list);
  }

  function moveFavorite(from: number, to: number | null) {
    const list = favoritePaths();
    const [item] = list.splice(from, 1);
    if (item === undefined) return;
    // Dropping below its old slot shifts the target up by one.
    const at = to == null ? list.length : to > from ? to - 1 : to;
    list.splice(at, 0, item);
    setFavorites(list);
  }

  /** Adds the folders among files dropped from Finder (only paths are known). */
  async function addDroppedFolders(paths: string[], index: number | null) {
    const checked = await Promise.all(
      paths.map((p) =>
        api
          .details(p)
          .then((d) => (d.items != null && !p.endsWith(".app") ? p : null))
          .catch(() => null),
      ),
    );
    const folders = checked.filter((p): p is string => p != null);
    if (folders.length) addFavorites(folders, index);
    else showError("Only folders can be added to Favourites.");
  }

  function removeFavorite(path: string) {
    setFavorites(favoritePaths().filter((p) => p !== path));
  }

  function resetFavorites() {
    const { favorites: _, ...rest } = settings;
    saveSettings(rest);
    loadSidebar();
  }

  /** Shows or hides a whole sidebar section (from its right-click menu). */
  function showSection(name: string) {
    const hidden = settings.hiddenSections ?? [];
    saveSettings({
      ...settings,
      hiddenSections: hidden.includes(name) ? hidden.filter((n) => n !== name) : [...hidden, name],
    });
  }

  /** Puts a folder at the top of Recents, unless the sidebar already lists it. */
  function remember(path: string) {
    if (!settingsReady || !path || path.startsWith("tag:")) return;
    const pinned = [...(sidebarData?.favorites ?? []), ...(sidebarData?.volumes ?? [])];
    if (pinned.some((p) => p.path === path)) return;
    const recents = settings.recents ?? [];
    if (recents[0] === path) return;
    saveSettings({ ...settings, recents: [path, ...recents.filter((p) => p !== path)].slice(0, 8) });
  }

  // A folder counts as visited once you stay in it for a moment, so the ones
  // you pass through on the way somewhere don't fill up Recents.
  const rememberTimers: Record<string, ReturnType<typeof setTimeout>> = {};
  function rememberSoon(pane: PaneState) {
    const path = pane.path;
    clearTimeout(rememberTimers[pane.id]);
    rememberTimers[pane.id] = setTimeout(() => pane.path === path && remember(path), 1500);
  }

  $effect(() => {
    void left.path;
    untrack(() => rememberSoon(left));
  });
  $effect(() => {
    void right.path;
    untrack(() => rememberSoon(right));
  });

  function toggleSection(name: string) {
    const collapsed = settings.collapsed ?? [];
    saveSettings({
      ...settings,
      collapsed: collapsed.includes(name) ? collapsed.filter((n) => n !== name) : [...collapsed, name],
    });
  }

  // ── Drag and drop ──────────────────────────────────────────────────────

  /** The disk a path is on, when one of the panes is showing that disk. */
  function deviceOf(path: string): number | null {
    for (const p of [left, right]) {
      if (p.isFolder && p.device != null && (path === p.path || path.startsWith(p.path + "/"))) return p.device;
    }
    return null;
  }

  // Fallback when no pane shows the folder: other disks are under /Volumes.
  const volumeOf = (path: string) => (path.startsWith("/Volumes/") ? path.split("/").slice(0, 3).join("/") : "/");

  const payloadPaths = (p: DragPayload) =>
    p.kind === "entries" ? p.entries.map((e) => e.path) : p.kind === "files" ? p.paths : [];

  /** What dropping here would do, or null if it isn't allowed. */
  function judge(p: DragPayload | null, target: DropTarget | null, alt: boolean, meta: boolean): Verdict {
    if (!p || !target || p.kind === "favorite") return null;
    // The gap under Favourites adds folders as new favourites.
    if (target.zone === "favorites" && target.path == null) {
      // From Finder we only know paths; which are folders is checked on drop.
      if (p.kind === "files") return { verb: "Add", dest: null };
      return p.kind === "entries" && p.entries.some((e) => e.isDir && !isApp(e)) ? { verb: "Add", dest: null } : null;
    }
    const dest = target.path;
    const sources = payloadPaths(p);
    if (!dest || dest.startsWith("tag:") || !sources.length) return null;
    // Never into itself or a folder inside it.
    if (sources.some((src) => dest === src || dest.startsWith(src + "/"))) return null;
    // Already there; ⌥ duplicates instead, as in Finder.
    if (!alt && sources.every((src) => parentOf(src) === dest)) return null;
    if (alt) return { verb: "Copy", dest };
    if (meta) return { verb: "Move", dest };
    // Like Finder: move within a disk, copy to another one.
    const from = deviceOf(parentOf(sources[0]));
    const to = deviceOf(dest);
    const sameDisk = from != null && to != null ? from === to : volumeOf(sources[0]) === volumeOf(dest);
    return { verb: sameDisk ? "Move" : "Copy", dest };
  }

  $effect(() => {
    const target = drag.over ? { zone: drag.over, path: drag.overPath, index: drag.overIndex } : null;
    drag.verdict = judge(drag.payload, target, drag.alt, drag.meta);
  });

  setDropHandler((payload, target, { alt, meta }) => {
    if (payload.kind === "favorite") {
      if (target.zone === "favorites") moveFavorite(payload.index, target.index);
      return;
    }
    const verdict = judge(payload, target, alt, meta);
    if (!verdict) {
      if (target.zone === "favorites" && target.path == null) showError("Only folders can be added to Favourites.");
      return;
    }
    if (verdict.verb === "Add") {
      if (payload.kind === "entries") {
        addFavorites(payload.entries.filter((e) => e.isDir && !isApp(e)).map((e) => e.path), target.index);
      } else if (payload.kind === "files") {
        addDroppedFolders(payload.paths, target.index);
      }
      return;
    }
    moveOrCopy(payloadPaths(payload), verdict.dest!, alt ? "copy" : meta ? "move" : "auto");
  });

  // Dragging past the window's edge hands the files to macOS, so they can be
  // dropped into Finder, Mail or any app. Copies unless ⌘ is held.
  setLeaveWindowHandler((payload, { meta }) => {
    if (payload.kind === "entries") api.dragOut(payload.entries.map((e) => e.path), meta).catch(showError);
  });

  // Spring-loaded folders: hold a drag over a folder and it opens, so you can
  // drop deeper. Sidebar folders open in the active pane.
  $effect(() => {
    const { overPath: path, overSpring, overPane } = drag;
    const kind = drag.payload?.kind;
    if (!path || !overSpring || (kind !== "entries" && kind !== "files")) return;
    const pane = overPane === "left" || overPane === "right" ? panes[overPane] : active;
    const timer = setTimeout(() => {
      if (pane.path !== path) pane.load(path).catch(showError);
    }, 900);
    return () => clearTimeout(timer);
  });

  // ── Right-click menus ─────────────────────────────────────────────────

  let menu = $state<{ x: number; y: number; items: MenuItem[] } | null>(null);

  function openMenu(pane: PaneState, e: MouseEvent, index: number | null) {
    activeId = pane.id as "left" | "right";
    menu = { x: e.clientX, y: e.clientY, items: index == null ? spaceMenu(pane) : itemMenu(pane) };
  }

  /** For the selected files and folders. */
  function itemMenu(pane: PaneState): MenuItem[] {
    const targets = pane.targets();
    const one = targets.length === 1 ? targets[0] : null;
    const folders = targets.filter((t) => t.isDir && !isApp(t));
    const openAll = () => (one ? run(async () => pane.open(one)) : targets.forEach((t) => !t.isDir && api.openPath(t.path)));
    return [
      { label: "Open", shortcut: "⌘↓", action: openAll },
      { label: "Show in Finder", shortcut: "⌥⌘R", action: () => run(() => api.revealInFinder(targets[0].path)) },
      "separator",
      { label: one ? "Copy" : `Copy ${targets.length} Items`, shortcut: "⌘C", action: () => copyToClipboard(targets) },
      { label: "Paste", shortcut: "⌘V", disabled: !pane.isFolder, action: () => paste() },
      { label: "Duplicate", shortcut: "⌘D", action: duplicate },
      "separator",
      { label: "Copy to Other Pane", shortcut: "F5", disabled: !other.isFolder, action: () => transfer("copy") },
      { label: "Move to Other Pane", shortcut: "F6", disabled: !other.isFolder, action: () => transfer("move") },
      "separator",
      { label: one ? "Rename…" : `Rename ${targets.length} Items…`, shortcut: "F2", action: rename },
      { label: "Add to Favourites", disabled: !folders.length, action: () => addFavorites(folders.map((f) => f.path), null) },
      { label: one ? "Copy Path" : "Copy Paths", action: () => copyPaths(targets) },
      "separator",
      { label: "Move to Bin", shortcut: "⌘⌫", danger: true, action: trash },
    ];
  }

  /** For empty space in a pane. */
  function spaceMenu(pane: PaneState): MenuItem[] {
    return [
      { label: "New Folder", shortcut: "⇧⌘N", disabled: !pane.isFolder, action: newFolder },
      { label: "Paste", shortcut: "⌘V", disabled: !pane.isFolder, action: () => paste() },
      "separator",
      { heading: "View" },
      ...VIEWS.map((v) => ({
        label: v.label,
        shortcut: v.key,
        checked: pane.view === v.view,
        action: () => (pane.view = v.view),
      })),
      "separator",
      { label: "Show Hidden Files", shortcut: "⇧⌘.", checked: pane.showHidden, action: () => run(() => pane.toggleHidden()) },
      { label: "Refresh", shortcut: "⌘R", action: () => run(refreshBoth) },
    ];
  }

  const VIEWS: { view: View; label: string; key: string }[] = [
    { view: "grid", label: "Icons", key: "⌘1" },
    { view: "list", label: "List", key: "⌘2" },
    { view: "columns", label: "Columns", key: "⌘3" },
    { view: "gallery", label: "Gallery", key: "⌘4" },
  ];
  const VIEW_ICONS = { grid: LayoutGrid, list: List, columns: Columns3, gallery: GalleryHorizontalEnd };

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
    // Files dragged in from Finder or another app. On macOS wry reports the
    // position in points (the page's own units) even though it's typed as a
    // PhysicalPosition; elsewhere it's in device pixels.
    const dropScale = navigator.userAgent.includes("Mac") ? 1 : devicePixelRatio;
    const unlistenDrop = getCurrentWebview().onDragDropEvent(({ payload: e }) => {
      if (e.type === "leave") return endDrag();
      const x = e.position.x / dropScale;
      const y = e.position.y / dropScale;
      if (e.type === "enter") drag.payload = { kind: "files", paths: e.paths };
      hover(x, y);
      if (e.type === "drop") {
        if (!drag.payload) drag.payload = { kind: "files", paths: e.paths };
        dropHere();
        endDrag();
      }
    });
    const unlistenProgress = listen<JobProgress>("transfer-progress", ({ payload }) => transfers.update(payload));
    // After a drag out (files may have moved), refresh.
    const unlistenDragOut = listen<boolean>("drag-out-done", ({ payload: dropped }) => {
      if (dropped) refreshBoth().catch(showError);
    });
    const unlistenVolumes = listen("volumes-changed", () => {
      clearTimeout(timers.volumes);
      timers.volumes = setTimeout(loadSidebar, 500);
    });

    api
      .readSettings()
      .then((saved) => (settings = saved))
      .catch(() => {})
      .finally(() => {
        settingsReady = true;
        loadSidebar();
      });
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
      unlistenDrop.then((fn) => fn());
      unlistenDragOut.then((fn) => fn());
      unlistenProgress.then((fn) => fn());
    };
  });

  const hasTargets = $derived(active.selected.size > 0 || !!active.current);
</script>

<!-- The webview's own menu (Reload, Inspect…) isn't useful here; text keeps it
     for copy and paste. -->
<svelte:window
  onkeydown={onKey}
  oncontextmenu={(e) => !(e.target as Element).closest("input, textarea, pre") && e.preventDefault()}
/>
<svelte:head><title>Patina — {active.title}</title></svelte:head>

<!-- Clicking a button keeps keyboard focus on the panes (no focus ring, and
     Return still opens the selected file). -->
<div
  class="app"
  class:no-sidebar={!showSidebar}
  role="presentation"
  onmousedown={appMouseDown}
>
  {#if showSidebar}
    <Sidebar
      data={sidebarData}
      current={active.path}
      collapsed={settings.collapsed ?? []}
      hidden={settings.hiddenSections ?? []}
      recents={settings.recents ?? []}
      customFavorites={settings.favorites != null}
      onopen={openLocation}
      ontoggle={toggleSection}
      onremove={removeFavorite}
      onshow={showSection}
      onresetfavorites={resetFavorites}
      onclearrecents={() => saveSettings({ ...settings, recents: [] })}
    />
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
        {#each VIEWS as v (v.view)}
          {@const Icon = VIEW_ICONS[v.view]}
          <button class:on={active.view === v.view} onclick={() => (active.view = v.view)} title="{v.label} ({v.key})">
            <Icon size={16} />
          </button>
        {/each}
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
        <Pane
          pane={left}
          {home}
          active={activeId === "left"}
          onactivate={() => (activeId = "left")}
          oncontext={(e, i) => openMenu(left, e, i)}
        />
        <Pane
          pane={right}
          {home}
          active={activeId === "right"}
          onactivate={() => (activeId = "right")}
          oncontext={(e, i) => openMenu(right, e, i)}
        />
      </div>
      {#if showPreview}
        <Preview
          entry={active.current}
          fields={settings.previewFields}
          onfields={(previewFields) => saveSettings({ ...settings, previewFields })}
          onerror={showError}
        />
      {/if}
    </div>
  </div>

  {#if drag.payload}
    {@const p = drag.payload}
    {@const label =
      p.kind === "favorite"
        ? (sidebarData?.favorites[p.index]?.name ?? "")
        : p.kind === "files"
          ? p.paths.length === 1
            ? basename(p.paths[0])
            : `${p.paths.length} items`
          : p.entries.length === 1
            ? p.entries[0].name
            : `${p.entries.length} items`}
    <div
      class="ghost"
      class:refused={p.kind !== "favorite" && !!drag.over && !drag.verdict}
      style:left="{drag.x + 14}px"
      style:top="{drag.y + 10}px"
    >
      {#if p.kind === "entries"}<FileIcon entry={p.entries[0]} size={16} />{/if}
      <span>{label}</span>
      {#if drag.verdict}
        <em class="verb">{drag.verdict.verb === "Add" ? "+ Favourites" : drag.verdict.verb}</em>
      {/if}
    </div>
  {/if}

  <TransferPanel />

  {#if batch}
    <BatchRename entries={batch.entries} existing={batch.existing} onclose={() => (batch = null)} onapply={applyBatch} />
  {/if}

  {#if menu}
    <ContextMenu x={menu.x} y={menu.y} items={menu.items} onclose={() => (menu = null)} />
  {/if}

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
  /* Slate greys with a darker inset sidebar; purple sidebar icons, orange accents. */
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
    --sidebar-icon: #b48cff;
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
  /* Rows and tiles take focus when clicked; the cursor highlight already
     shows where you are, so the browser's focus ring would be a second one. */
  :global([tabindex="-1"]:focus) {
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
    /* The preview panel widens on bigger windows. */
    grid-template-columns: minmax(0, 1fr) clamp(280px, 24vw, 440px);
  }
  .panes {
    min-height: 0;
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 1px;
    background: var(--border);
  }

  .ghost {
    position: fixed;
    z-index: 50;
    display: flex;
    align-items: center;
    gap: 6px;
    max-width: 260px;
    padding: 4px 10px;
    border-radius: 7px;
    background: var(--panel);
    border: 1px solid var(--line);
    box-shadow: 0 8px 24px rgba(0, 0, 0, 0.45);
    font-size: 12px;
    pointer-events: none;
  }
  .ghost span {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .ghost.refused {
    opacity: 0.55;
  }
  .ghost .verb {
    flex: none;
    padding: 1px 6px;
    border-radius: 4px;
    background: var(--blue);
    color: #fff;
    font-style: normal;
    font-size: 11px;
    font-weight: 600;
  }
  :global(body.dragging),
  :global(body.dragging *) {
    cursor: grabbing !important;
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
