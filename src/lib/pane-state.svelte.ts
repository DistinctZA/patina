// State for one file pane: the folder it shows, the cursor and the selection.
import { api, basename, isApp, parentOf, type Entry, type SortBy } from "./api";

export type View = "grid" | "list" | "columns" | "gallery";

/** Locations starting with this show every file carrying a Finder tag. */
const TAG_PREFIX = "tag:";

/** The folder is gone (Rust's wording for "not found", see `describe`). */
const isMissing = (err: unknown) => String(err).includes("couldn't be found");

export class PaneState {
  readonly id: string;
  /** A folder path, or `tag:<name>` for a tag search. */
  path = $state("");
  parent = $state<string | null>(null);
  available = $state<number | null>(null);
  device = $state<number | null>(null);
  /** Everything in the folder; `entries` is this after the search filter. */
  all = $state.raw<Entry[]>([]);
  filter = $state("");
  entries = $derived.by(() => {
    const q = this.filter.trim().toLowerCase();
    return q ? this.all.filter((e) => e.name.toLowerCase().includes(q)) : this.all;
  });
  /** Paths of selected entries. Replaced, never mutated, so the UI updates. */
  selected = $state.raw<Set<string>>(new Set());
  cursor = $state(0);
  showHidden = $state(false);
  sortBy = $state<SortBy>("name");
  desc = $state(false);
  view = $state<View>("list");
  loading = $state(false);
  /** Items per row in grid view; set by the pane component as it resizes. */
  columns = 1;

  /** Where a shift-selection starts. */
  private anchor = 0;
  /** Bumped by every load, so a slow older load can't overwrite a newer one. */
  private loadId = 0;
  /** The folder the Rust side is watching for this pane. */
  private watched = "";
  /** A refresh asked for during a load; it runs once the load is done. */
  private refreshPending = false;
  private back = $state<string[]>([]);
  private forward = $state<string[]>([]);

  constructor(id: string) {
    this.id = id;
  }

  get current(): Entry | undefined {
    return this.entries[this.cursor];
  }

  get tag(): string | null {
    return this.path.startsWith(TAG_PREFIX) ? this.path.slice(TAG_PREFIX.length) : null;
  }

  /** False for tag searches, which can't receive copies or new folders. */
  get isFolder(): boolean {
    return this.tag === null;
  }

  get title() {
    return this.tag ?? basename(this.path);
  }

  /** The selected entries, or the one under the cursor when nothing is selected. */
  targets(): Entry[] {
    const picked = this.entries.filter((e) => this.selected.has(e.path));
    if (picked.length) return picked;
    return this.current ? [this.current] : [];
  }

  /**
   * Shows `location`. `focus` puts the cursor on that entry (by path); `keep`
   * preserves the cursor, selection and search, for refreshes. Resolves to
   * false if a newer load overtook this one (so nothing changed).
   */
  async load(location: string, opts: { focus?: string; keep?: boolean; history?: boolean } = {}): Promise<boolean> {
    const id = ++this.loadId;
    this.loading = true;
    try {
      const tag = location.startsWith(TAG_PREFIX) ? location.slice(TAG_PREFIX.length) : null;
      const listing = tag
        ? await api.findTagged(tag, this.showHidden, this.sortBy, this.desc)
        : await api.listDir(location, this.showHidden, this.sortBy, this.desc);
      if (id !== this.loadId) return false;

      if (opts.history !== false && this.path && listing.path !== this.path) {
        this.back.push(this.path);
        this.forward = [];
      }
      const cursorPath = opts.focus ?? (opts.keep ? this.current?.path : undefined);
      const stillThere = new Set(listing.entries.map((e) => e.path));

      if (!opts.keep) this.filter = "";
      this.path = listing.path;
      this.parent = listing.parent;
      this.available = listing.available;
      this.device = listing.device;
      this.all = listing.entries;

      const found = cursorPath ? this.entries.findIndex((e) => e.path === cursorPath) : -1;
      this.cursor = found >= 0 ? found : opts.keep ? Math.min(this.cursor, this.entries.length - 1) : 0;
      this.cursor = Math.max(0, this.cursor);
      this.anchor = this.cursor;
      this.selected = opts.keep
        ? new Set([...this.selected].filter((p) => stillThere.has(p)))
        : new Set();

      // Watching is a nicety: if it fails (e.g. no permission) the listing
      // still shows, it just won't refresh by itself.
      if (!tag && listing.path !== this.watched) {
        this.watched = listing.path;
        api.watchDir(this.id, listing.path).catch(() => (this.watched = ""));
      }
      return true;
    } finally {
      if (id === this.loadId) {
        this.loading = false;
        if (this.refreshPending) {
          this.refreshPending = false;
          queueMicrotask(() => this.refresh().catch(() => {}));
        }
      }
    }
  }

  /**
   * Re-reads the folder, keeping the cursor, selection and search. If you're
   * in the middle of opening another folder it waits, rather than undoing
   * that. If the folder has gone (deleted, or moved away), it steps up to the
   * nearest folder that still exists.
   */
  async refresh(): Promise<void> {
    if (this.loading) {
      this.refreshPending = true;
      return;
    }
    try {
      await this.load(this.path, { keep: true });
    } catch (err) {
      if (this.tag || !isMissing(err)) throw err;
      await this.climbToExisting();
    }
  }

  private async climbToExisting() {
    for (let dir = parentOf(this.path); ; dir = parentOf(dir)) {
      try {
        await this.load(dir, { history: false });
        return;
      } catch {
        if (dir === "/") return;
      }
    }
  }

  goUp() {
    if (this.parent) return this.load(this.parent, { focus: this.path });
  }

  get canGoBack() {
    return this.back.length > 0;
  }

  get canGoForward() {
    return this.forward.length > 0;
  }

  goBack() {
    return this.step(this.back, this.forward);
  }

  goForward() {
    return this.step(this.forward, this.back);
  }

  // History only moves once the folder has actually opened, so a failed or
  // overtaken step leaves it as it was. A folder that's gone for good is
  // dropped, though, so the next step can get past it.
  private async step(from: string[], to: string[]) {
    const target = from.at(-1);
    if (!target) return;
    const here = this.path;
    try {
      if (await this.load(target, { history: false })) {
        from.pop();
        to.push(here);
      }
    } catch (err) {
      if (isMissing(err) && from.at(-1) === target) from.pop();
      throw err;
    }
  }

  open(entry: Entry | undefined = this.current) {
    if (!entry) return;
    // Apps are folders on disk but should launch, not open as a folder.
    if (entry.isDir && !isApp(entry)) return this.load(entry.path);
    return api.openPath(entry.path);
  }

  setSort(by: SortBy) {
    if (this.sortBy === by) this.desc = !this.desc;
    else {
      this.sortBy = by;
      this.desc = false;
    }
    return this.refresh();
  }

  toggleHidden() {
    this.showHidden = !this.showHidden;
    return this.refresh();
  }

  setFilter(text: string) {
    this.filter = text;
    this.cursor = 0;
    this.anchor = 0;
    this.selected = new Set();
  }

  moveCursor(to: number, extend = false) {
    if (!this.entries.length) return;
    this.cursor = Math.max(0, Math.min(this.entries.length - 1, to));
    if (extend) this.selectRange(this.anchor, this.cursor);
    else {
      this.anchor = this.cursor;
      this.selected = new Set([this.entries[this.cursor].path]);
    }
  }

  /** Mouse click with Finder-style modifiers. */
  click(index: number, e: MouseEvent) {
    const path = this.entries[index].path;
    if (e.shiftKey) {
      this.cursor = index;
      this.selectRange(this.anchor, index);
    } else if (e.metaKey) {
      const next = new Set(this.selected);
      if (next.has(path)) next.delete(path);
      else next.add(path);
      this.selected = next;
      this.cursor = this.anchor = index;
    } else {
      this.moveCursor(index);
    }
  }

  /**
   * A press on an item, before any drag. As in Finder, pressing an item
   * that's part of a selection keeps all of it, so it can be dragged
   * together; letting go without dragging then narrows it to that item.
   * Right-clicks are left to the menu, which selects for itself.
   */
  press(index: number, e: MouseEvent) {
    if (e.button !== 0 || e.ctrlKey) return;
    const path = this.entries[index]?.path;
    if (path == null) return;
    if (e.shiftKey || e.metaKey || this.selected.size < 2 || !this.selected.has(path)) {
      this.click(index, e);
      return;
    }
    const release = (up: MouseEvent) => {
      window.removeEventListener("mouseup", release);
      // Same distance as a drag needs to start.
      if (Math.hypot(up.clientX - e.clientX, up.clientY - e.clientY) < 5) this.moveCursor(index);
    };
    window.addEventListener("mouseup", release);
  }

  /** Selects exactly these paths (from a drag-to-select box). */
  selectPaths(paths: Set<string>) {
    this.selected = paths;
    const first = this.entries.findIndex((e) => paths.has(e.path));
    if (first >= 0) this.cursor = this.anchor = first;
  }

  selectAll() {
    this.selected = new Set(this.entries.map((e) => e.path));
  }

  clearSelection() {
    this.selected = new Set();
  }

  /** Jumps to the first entry whose name starts with `prefix` (type-to-select). */
  jumpTo(prefix: string) {
    const p = prefix.toLowerCase();
    const i = this.entries.findIndex((e) => e.name.toLowerCase().startsWith(p));
    if (i >= 0) this.moveCursor(i);
  }

  private selectRange(a: number, b: number) {
    const [lo, hi] = a < b ? [a, b] : [b, a];
    this.selected = new Set(this.entries.slice(lo, hi + 1).map((e) => e.path));
  }
}
