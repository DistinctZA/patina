// Typed wrappers around the Rust commands in src-tauri/src/lib.rs. Nothing
// here touches the network: `invoke` calls Rust code inside the app.
import { convertFileSrc, invoke } from "@tauri-apps/api/core";

export type Tag = { name: string; color: number };

export type Entry = {
  name: string;
  path: string;
  isDir: boolean;
  isSymlink: boolean;
  isHidden: boolean;
  size: number;
  modified: number | null;
  tags: Tag[];
};

export type Listing = {
  path: string;
  parent: string | null;
  entries: Entry[];
  available: number | null;
  /** Which disk the folder is on (to tell a drag will move or copy). */
  device: number | null;
};

/** "auto" moves within a disk and copies to another one, like Finder. */
export type TransferMode = "copy" | "move" | "auto";

export type SortBy = "name" | "size" | "modified" | "kind";

export type Place = { name: string; path: string; icon: string };
export type Volume = {
  name: string;
  path: string;
  available: number | null;
  total: number | null;
  isRoot: boolean;
};
export type SidebarData = { favorites: Place[]; volumes: Volume[] };

export type Details = {
  size: number;
  created: number | null;
  modified: number | null;
  accessed: number | null;
  mode: number;
  owner: string;
  group: string;
  items: number | null;
  linkTarget: string | null;
};

export type TextPreview = { text: string; truncated: boolean };

/** Saved between launches, in ~/Library/Application Support/com.distinctza.patina. */
export type Settings = {
  /** The user's Favourites, in order; absent means the default set. */
  favorites?: string[];
  /** Sidebar sections the user has folded away. */
  collapsed?: string[];
  /** Which details the preview panel's Info section shows. */
  previewFields?: string[];
  /** Sidebar sections switched off from the right-click menu. */
  hiddenSections?: string[];
  /** Recently visited folders, newest first. */
  recents?: string[];
};

export const api = {
  homeDir: () => invoke<string>("home_dir"),
  listDir: (path: string, showHidden: boolean, sortBy: SortBy, desc: boolean) =>
    invoke<Listing>("list_dir", { path, showHidden, sortBy, desc }),
  findTagged: (tag: string, showHidden: boolean, sortBy: SortBy, desc: boolean) =>
    invoke<Listing>("find_tagged", { tag, showHidden, sortBy, desc }),
  sidebar: (favorites: string[] | null) => invoke<SidebarData>("sidebar", { favorites }),
  readSettings: () => invoke<Settings>("read_settings"),
  writeSettings: (settings: Settings) => invoke<void>("write_settings", { settings }),
  details: (path: string) => invoke<Details>("details", { path }),
  readText: (path: string) => invoke<TextPreview | null>("read_text", { path }),
  thumbnail: (path: string, size: number) => invoke<string | null>("thumbnail", { path, size }),
  transfer: (sources: string[], dest: string, mode: TransferMode) =>
    invoke<void>("transfer", { sources, dest, mode }),
  duplicate: (paths: string[]) => invoke<void>("duplicate", { paths }),
  /** The system clipboard, shared with Finder. */
  clipboardCopy: (paths: string[]) => invoke<void>("clipboard_copy", { paths }),
  clipboardPaste: () => invoke<string[]>("clipboard_paste"),
  copyInto: (sources: string[], dest: string) => invoke<void>("copy_into", { sources, dest }),
  moveInto: (sources: string[], dest: string) => invoke<void>("move_into", { sources, dest }),
  rename: (path: string, newName: string) => invoke<string>("rename", { path, newName }),
  createFolder: (dir: string, name: string) => invoke<string>("create_folder", { dir, name }),
  moveToTrash: (paths: string[]) => invoke<void>("move_to_trash", { paths }),
  openPath: (path: string) => invoke<void>("open_path", { path }),
  revealInFinder: (path: string) => invoke<void>("reveal_in_finder", { path }),
  watchDir: (pane: string, path: string) => invoke<void>("watch_dir", { pane, path }),
};

/** A URL the webview can load a local file from. */
export const fileUrl = (path: string) => convertFileSrc(path);

// Quick Look thumbnails, shared by the grid and the preview panel. Keyed by
// mtime and size too, so an edited file gets a fresh thumbnail.
const thumbCache = new Map<string, Promise<string | null>>();

export function thumbnailUrl(entry: Entry, size: number): Promise<string | null> {
  const key = `${entry.path}|${entry.modified}|${entry.size}|${size}`;
  let url = thumbCache.get(key);
  if (!url) {
    if (thumbCache.size > 4000) thumbCache.clear();
    url = api
      .thumbnail(entry.path, size)
      .then((p) => (p ? fileUrl(p) : null))
      .catch(() => null);
    thumbCache.set(key, url);
  }
  return url;
}

/** Finder's tag colours, by Finder's colour number. */
export const TAG_COLORS: Record<number, string> = {
  1: "#8e8e93",
  2: "#34c759",
  3: "#af52de",
  4: "#0a84ff",
  5: "#ffcc00",
  6: "#ff3b30",
  7: "#ff9500",
};

export const FINDER_TAGS: Tag[] = [
  { name: "Red", color: 6 },
  { name: "Orange", color: 7 },
  { name: "Yellow", color: 5 },
  { name: "Green", color: 2 },
  { name: "Blue", color: 4 },
  { name: "Purple", color: 3 },
  { name: "Grey", color: 1 },
];

/** The folder a path is in. */
export function parentOf(path: string): string {
  return path.slice(0, path.lastIndexOf("/")) || "/";
}

export function basename(path: string): string {
  return path.split("/").filter(Boolean).pop() ?? "/";
}

export function extension(name: string): string {
  const i = name.lastIndexOf(".");
  return i > 0 ? name.slice(i + 1).toLowerCase() : "";
}

const IMAGE = new Set(["jpg", "jpeg", "png", "gif", "webp", "heic", "heif", "bmp", "tif", "tiff", "svg", "avif", "ico"]);
const VIDEO = new Set(["mp4", "mov", "m4v", "webm"]);
const AUDIO = new Set(["mp3", "m4a", "aac", "wav", "aiff", "aif", "flac", "ogg"]);
const ARCHIVE = new Set(["zip", "tar", "gz", "tgz", "bz2", "xz", "7z", "rar", "dmg"]);
const CODE = new Set([
  "js", "ts", "jsx", "tsx", "svelte", "vue", "rs", "py", "rb", "go", "swift", "kt", "java", "c", "h",
  "cpp", "hpp", "cs", "php", "html", "css", "scss", "json", "yaml", "yml", "toml", "xml", "sh", "zsh",
  "lua", "sql",
]);

export type Media = "image" | "video" | "audio" | "archive" | "code" | null;

export function mediaOf(entry: Entry): Media {
  if (entry.isDir) return null;
  const ext = extension(entry.name);
  if (IMAGE.has(ext)) return "image";
  if (VIDEO.has(ext)) return "video";
  if (AUDIO.has(ext)) return "audio";
  if (ARCHIVE.has(ext)) return "archive";
  if (CODE.has(ext)) return "code";
  return null;
}

const KINDS: Record<string, string> = {
  jpg: "JPEG image", jpeg: "JPEG image", png: "PNG image", gif: "GIF image", heic: "HEIC image",
  webp: "WebP image", svg: "SVG image", pdf: "PDF document", txt: "Plain text", md: "Markdown",
  html: "HTML document", json: "JSON", zip: "ZIP archive", dmg: "Disk image", mp4: "MPEG-4 movie",
  mov: "QuickTime movie", mp3: "MP3 audio", m4a: "Apple audio", wav: "WAV audio", rs: "Rust source",
  ts: "TypeScript", js: "JavaScript", py: "Python script", sh: "Shell script", docx: "Word document",
  xlsx: "Excel workbook", pptx: "PowerPoint", pages: "Pages document", key: "Keynote", numbers: "Numbers",
};

/** App bundles are folders on disk, but behave like a single file. */
export function isApp(entry: Entry): boolean {
  return entry.isDir && entry.name.endsWith(".app");
}

export function kindOf(entry: Entry): string {
  if (entry.isDir) return isApp(entry) ? "Application" : "Folder";
  const ext = extension(entry.name);
  return KINDS[ext] ?? (ext ? `${ext.toUpperCase()} file` : "Document");
}

/** Whether to ask Rust for a thumbnail (apps show their own icon). */
export function wantsThumbnail(entry: Entry): boolean {
  return !entry.isDir || isApp(entry);
}

/** Changes when the file itself changes, not when its folder is re-listed. */
export function entryKey(entry: Entry): string {
  return `${entry.path}|${entry.modified}|${entry.size}`;
}

/** Sizes in decimal units, like Finder. */
export function formatSize(bytes: number): string {
  if (bytes < 1000) return `${bytes} bytes`;
  const units = ["KB", "MB", "GB", "TB"];
  let value = bytes / 1000;
  let i = 0;
  while (value >= 1000 && i < units.length - 1) {
    value /= 1000;
    i++;
  }
  return `${value.toFixed(value < 10 ? 1 : 0)} ${units[i]}`;
}

const dateFormat = new Intl.DateTimeFormat(undefined, { dateStyle: "medium", timeStyle: "short" });

export function formatDate(ms: number | null): string {
  return ms == null ? "—" : dateFormat.format(ms);
}
