// Typed wrappers around the Rust commands in src-tauri/src/lib.rs.
import { invoke } from "@tauri-apps/api/core";

export type Entry = {
  name: string;
  path: string;
  isDir: boolean;
  isSymlink: boolean;
  isHidden: boolean;
  size: number;
  modified: number | null;
};

export type Listing = {
  path: string;
  parent: string | null;
  entries: Entry[];
};

export type SortBy = "name" | "size" | "modified" | "kind";

export const api = {
  homeDir: () => invoke<string>("home_dir"),
  listDir: (path: string, showHidden: boolean, sortBy: SortBy, desc: boolean) =>
    invoke<Listing>("list_dir", { path, showHidden, sortBy, desc }),
  copyInto: (sources: string[], dest: string) => invoke<void>("copy_into", { sources, dest }),
  moveInto: (sources: string[], dest: string) => invoke<void>("move_into", { sources, dest }),
  rename: (path: string, newName: string) => invoke<string>("rename", { path, newName }),
  createFolder: (dir: string, name: string) => invoke<string>("create_folder", { dir, name }),
  moveToTrash: (paths: string[]) => invoke<void>("move_to_trash", { paths }),
  openPath: (path: string) => invoke<void>("open_path", { path }),
  revealInFinder: (path: string) => invoke<void>("reveal_in_finder", { path }),
  watchDir: (pane: string, path: string) => invoke<void>("watch_dir", { pane, path }),
};

export function basename(path: string): string {
  return path.split("/").filter(Boolean).pop() ?? "/";
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
