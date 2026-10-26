// State for one file pane: the folder it shows, the cursor and the selection.
import { api, basename, type Entry, type SortBy } from "./api";

export class PaneState {
  readonly id: string;
  path = $state("");
  parent = $state<string | null>(null);
  entries = $state.raw<Entry[]>([]);
  /** Paths of selected entries. Replaced, never mutated, so the UI updates. */
  selected = $state.raw<Set<string>>(new Set());
  cursor = $state(0);
  showHidden = $state(false);
  sortBy = $state<SortBy>("name");
  desc = $state(false);
  loading = $state(false);

  /** Where a shift-selection starts. */
  private anchor = 0;
  private back: string[] = [];
  private forward: string[] = [];

  constructor(id: string) {
    this.id = id;
  }

  get current(): Entry | undefined {
    return this.entries[this.cursor];
  }

  /** The selected entries, or the one under the cursor when nothing is selected. */
  targets(): Entry[] {
    const picked = this.entries.filter((e) => this.selected.has(e.path));
    if (picked.length) return picked;
    return this.current ? [this.current] : [];
  }

  /**
   * Shows `path`. `focus` puts the cursor on that entry (by path); `keep`
   * preserves the cursor and selection, for refreshes.
   */
  async load(path: string, opts: { focus?: string; keep?: boolean; history?: boolean } = {}) {
    this.loading = true;
    try {
      const listing = await api.listDir(path, this.showHidden, this.sortBy, this.desc);
      if (opts.history !== false && this.path && listing.path !== this.path) {
        this.back.push(this.path);
        this.forward = [];
      }
      const cursorPath = opts.focus ?? (opts.keep ? this.current?.path : undefined);
      const stillThere = new Set(listing.entries.map((e) => e.path));

      this.path = listing.path;
      this.parent = listing.parent;
      this.entries = listing.entries;

      const found = cursorPath ? listing.entries.findIndex((e) => e.path === cursorPath) : -1;
      this.cursor = found >= 0 ? found : opts.keep ? Math.min(this.cursor, listing.entries.length - 1) : 0;
      this.cursor = Math.max(0, this.cursor);
      this.anchor = this.cursor;
      this.selected = opts.keep
        ? new Set([...this.selected].filter((p) => stillThere.has(p)))
        : new Set();

      await api.watchDir(this.id, listing.path);
    } finally {
      this.loading = false;
    }
  }

  refresh() {
    return this.load(this.path, { keep: true });
  }

  goUp() {
    if (this.parent) return this.load(this.parent, { focus: this.path });
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
    return entry.isDir ? this.load(entry.path) : api.openPath(entry.path);
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

  get title() {
    return basename(this.path);
  }

  private selectRange(a: number, b: number) {
    const [lo, hi] = a < b ? [a, b] : [b, a];
    this.selected = new Set(this.entries.slice(lo, hi + 1).map((e) => e.path));
  }
}
