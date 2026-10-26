// State for one file pane: the folder it shows, the cursor and the selection.
import { api, basename, isApp, type Entry, type SortBy } from "./api";

export type View = "grid" | "list" | "columns" | "gallery";

/** Locations starting with this show every file carrying a Finder tag. */
const TAG_PREFIX = "tag:";

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
   * preserves the cursor, selection and search, for refreshes.
   */
  async load(location: string, opts: { focus?: string; keep?: boolean; history?: boolean } = {}) {
    const id = ++this.loadId;
    this.loading = true;
    try {
      const tag = location.startsWith(TAG_PREFIX) ? location.slice(TAG_PREFIX.length) : null;
      const listing = tag
        ? await api.findTagged(tag, this.showHidden, this.sortBy, this.desc)
        : await api.listDir(location, this.showHidden, this.sortBy, this.desc);
      if (id !== this.loadId) return;

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
    } finally {
      if (id === this.loadId) this.loading = false;
    }
  }

  refresh() {
    return this.load(this.path, { keep: true });
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
    const prev = this.back.pop();
    if (!prev) return;
    this.forward.push(this.path);
    return this.load(prev, { history: false });
  }

  goForward() {
    const next = this.forward.pop();
    if (!next) return;
    this.back.push(this.path);
    return this.load(next, { history: false });
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
