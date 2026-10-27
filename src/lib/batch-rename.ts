// Works out new names for batch rename, so the dialog can preview them live.
// Rust does the renaming and checks the names again before touching anything.
import { parentOf, type Entry } from "./api";

export type RenameMode = "replace" | "add" | "format" | "case";

export type RenameOptions = {
  mode: RenameMode;
  /** Replace text */
  find: string;
  replaceWith: string;
  matchCase: boolean;
  regex: boolean;
  /** Add text */
  text: string;
  textWhere: "before" | "after";
  /** Format: "Holiday 001", "Holiday 002"… */
  base: string;
  start: number;
  digits: number;
  numberWhere: "before" | "after";
  /** Change case */
  caseTo: "lower" | "upper" | "title";
  /** Leave ".jpg" etc. alone and change only the name before it. */
  keepExtension: boolean;
};

export const defaultOptions = (): RenameOptions => ({
  mode: "replace",
  find: "",
  replaceWith: "",
  matchCase: false,
  regex: false,
  text: "",
  textWhere: "after",
  base: "",
  start: 1,
  digits: 3,
  numberWhere: "after",
  caseTo: "title",
  keepExtension: true,
});

export type Renamed = { entry: Entry; name: string; error: string | null };

/** "photo.jpg" → ["photo", ".jpg"]; folders and dotfiles keep their whole name. */
export function splitName(name: string, isDir: boolean): [string, string] {
  const dot = name.lastIndexOf(".");
  return isDir || dot <= 0 ? [name, ""] : [name.slice(0, dot), name.slice(dot)];
}

const escapeRegex = (s: string) => s.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");

function changeCase(s: string, to: RenameOptions["caseTo"]): string {
  if (to === "lower") return s.toLowerCase();
  if (to === "upper") return s.toUpperCase();
  return s.toLowerCase().replace(/(^|[\s_\-.(\[])(\p{L})/gu, (_, gap, ch) => gap + ch.toUpperCase());
}

/**
 * The new name for each entry, in order, with an error where it can't be
 * used. `existing` holds the lowercased paths already there, so items from
 * different folders (as in a tag view) only clash with their own neighbours.
 */
export function newNames(entries: Entry[], o: RenameOptions, existing: Set<string>): Renamed[] {
  let pattern: RegExp | null = null;
  let patternError: string | null = null;
  if (o.mode === "replace" && o.find) {
    try {
      pattern = new RegExp(o.regex ? o.find : escapeRegex(o.find), o.matchCase ? "g" : "gi");
    } catch {
      patternError = "Invalid pattern";
    }
  }

  const out = entries.map((entry, i): Renamed => {
    const [stem, ext] = o.keepExtension ? splitName(entry.name, entry.isDir) : [entry.name, ""];
    let s = stem;
    switch (o.mode) {
      case "replace":
        // A regular expression may use $1 and friends; plain text is taken literally.
        if (pattern) s = o.regex ? s.replace(pattern, o.replaceWith) : s.replace(pattern, () => o.replaceWith);
        break;
      case "add":
        s = o.textWhere === "before" ? o.text + s : s + o.text;
        break;
      case "format": {
        const n = String(Math.max(0, o.start) + i).padStart(Math.max(1, Math.min(9, o.digits)), "0");
        const base = o.base.trim();
        s = !base ? n : o.numberWhere === "after" ? `${base} ${n}` : `${n} ${base}`;
        break;
      }
      case "case":
        s = changeCase(s, o.caseTo);
        break;
    }
    // Rust trims names too, so preview what will actually be used.
    const name = (s + ext).trim();
    const bad = !name || name === "." || name === ".." || name.includes("/");
    return { entry, name, error: patternError ?? (bad ? "Not a valid name" : null) };
  });

  // Clashes, ignoring case as APFS does: with each other, and with files
  // outside the batch.
  const keyOf = (entry: Entry, name: string) => {
    const dir = parentOf(entry.path);
    return `${dir === "/" ? "" : dir}/${name}`.toLowerCase();
  };
  const renamedFrom = new Set(entries.map((e) => e.path.toLowerCase()));
  const counts = new Map<string, number>();
  for (const r of out) {
    const key = keyOf(r.entry, r.name);
    counts.set(key, (counts.get(key) ?? 0) + 1);
  }
  for (const r of out) {
    if (r.error) continue;
    const key = keyOf(r.entry, r.name);
    if ((counts.get(key) ?? 0) > 1) r.error = "Same name as another item";
    else if (existing.has(key) && !renamedFrom.has(key)) r.error = "Already exists";
  }
  return out;
}
