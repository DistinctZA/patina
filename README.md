<div align="center">

<img src="docs/logo.svg" width="112" alt="Patina logo" />

# Patina

### A fast, keyboard-first dual-pane file manager for macOS.

**Two panes. Zero lag. No sandbox. Built in Rust.**

<p>
  <img alt="Version" src="https://img.shields.io/badge/version-0.3.0-36373c?style=flat-square" />
  <img alt="Rust" src="https://img.shields.io/badge/Rust-1.85+-b7410e?style=flat-square&logo=rust&logoColor=white" />
  <img alt="Tauri 2" src="https://img.shields.io/badge/Tauri-2-24c8db?style=flat-square&logo=tauri&logoColor=white" />
  <img alt="Svelte 5" src="https://img.shields.io/badge/Svelte-5-ff3e00?style=flat-square&logo=svelte&logoColor=white" />
  <img alt="macOS 13+" src="https://img.shields.io/badge/macOS-13+-000000?style=flat-square&logo=apple&logoColor=white" />
  <img alt="Signed" src="https://img.shields.io/badge/signed-Developer%20ID-f2a33a?style=flat-square" />
</p>

<p>
  <a href="#-features"><b>Features</b></a> ·
  <a href="#%EF%B8%8F-keyboard-shortcuts"><b>Shortcuts</b></a> ·
  <a href="#-quick-start"><b>Quick start</b></a> ·
  <a href="#-install-on-your-mac"><b>Install</b></a> ·
  <a href="#%EF%B8%8F-versions-and-releases"><b>Releases</b></a> ·
  <a href="#-how-it-works"><b>How it works</b></a> ·
  <a href="#-roadmap"><b>Roadmap</b></a>
</p>

```bash
npm install && npm run tauri dev
```

<br />

<img src="docs/screenshot-icons.png" width="900" alt="Patina with this project in List view and the Mac's apps in Icons view, previewing Calculator" />

<sub><b>List + Icons</b>: file-type icons on the left, real app icons on the right, and the preview panel.</sub>

<br /><br />

<img src="docs/screenshot-panes.png" width="900" alt="Patina with two panes in List view showing the home folder, with the preview panel on the right" />

<sub><b>Two panes</b>: your folders side by side in List view, each with its own icon, and the preview panel.</sub>

</div>

> [!NOTE]
> Patina runs outside the Mac App Store, so it isn't sandboxed and can browse your whole disk. No microphone, accessibility or screen-recording permissions are ever needed.

---

## ✨ Features

| | |
|---|---|
| 🪟 **Dual panes** | Two folders side by side. <kbd>Tab</kbd> flips between them. |
| ⚡ **Instant, even when huge** | Only visible rows are drawn, so 50,000-file folders scroll smoothly. |
| 🖱️ **Drag and drop** | Drag between panes, onto any folder or sidebar item, or in from Finder. Moves on the same disk and copies to another, like Finder; hold ⌥ to copy or ⌘ to move. |
| 📋 **Copy & paste with Finder** | ⌘C / ⌘V share the system clipboard with Finder, ⌥⌘V moves, ⌘D duplicates. Clashes become `name copy`, never overwritten. |
| 🖱️ **Right-click menus** | Open, Copy, Paste, Duplicate, Rename, Add to Favourites, Copy Path, Move to Bin, and view options. |
| 🍏 **APFS clones** | Copies on the same disk are instant and take no extra space. |
| 🔄 **Live refresh** | Panes update by themselves when files change on disk (FSEvents). |
| 🔤 **Natural sorting** | `file 2` before `file 10`; sort by name, size or date, folders first. |
| ⌨️ **Type to jump** | Start typing a name and the cursor jumps to it. |
| 🗑️ **Move to Bin** | Deletes go to the Bin, never straight to oblivion. |
| 🗂️ **Sidebar** | Drives with free space, Favourites, Recents and Finder tags. Drag folders in to add Favourites, drag to reorder, fold sections, or right-click to choose which ones show. |
| 🪟 **Four views** | Icons (Quick Look thumbnails and real app icons), List, Columns and Gallery, per pane. |
| 🎨 **File-type icons** | The Material Icon Theme: a distinct icon for 1,300+ file types and named folders. |
| 👁️ **Preview panel** | Images, video, audio, text and code, plus an Info section where you choose which details to show. |
| 🔎 **Search** | Filter the current folder as you type. |
| 🌑 **Slate dark theme** | Soft greys, an inset sidebar with purple icons, and orange accents. |

## ⌨️ Keyboard shortcuts

| Action | Keys |
|---|---|
| Switch pane | <kbd>Tab</kbd> |
| Move cursor / extend selection | <kbd>↑</kbd> <kbd>↓</kbd> / <kbd>⇧</kbd><kbd>↑</kbd> <kbd>⇧</kbd><kbd>↓</kbd> |
| Open file or folder | <kbd>Return</kbd> or <kbd>⌘</kbd><kbd>↓</kbd> |
| Enclosing folder | <kbd>⌫</kbd> or <kbd>⌘</kbd><kbd>↑</kbd> |
| Back / forward | <kbd>⌘</kbd><kbd>[</kbd> / <kbd>⌘</kbd><kbd>]</kbd> |
| Copy to other pane | <kbd>F5</kbd> |
| Move to other pane | <kbd>F6</kbd> |
| Rename | <kbd>F2</kbd> |
| New folder | <kbd>F7</kbd> or <kbd>⌘</kbd><kbd>⇧</kbd><kbd>N</kbd> |
| Move to Bin | <kbd>F8</kbd> or <kbd>⌘</kbd><kbd>⌫</kbd> |
| Show / hide hidden files | <kbd>⌘</kbd><kbd>⇧</kbd><kbd>.</kbd> |
| Reveal in Finder | <kbd>⌘</kbd><kbd>⌥</kbd><kbd>R</kbd> |
| Refresh | <kbd>⌘</kbd><kbd>R</kbd> |
| Select all / clear | <kbd>⌘</kbd><kbd>A</kbd> / <kbd>Esc</kbd> |
| Copy / paste files (shared with Finder) | <kbd>⌘</kbd><kbd>C</kbd> / <kbd>⌘</kbd><kbd>V</kbd> |
| Move copied files here | <kbd>⌥</kbd><kbd>⌘</kbd><kbd>V</kbd> |
| Duplicate | <kbd>⌘</kbd><kbd>D</kbd> |
| Go to folder | <kbd>⇧</kbd><kbd>⌘</kbd><kbd>G</kbd> |
| While dragging: copy / move | hold <kbd>⌥</kbd> / <kbd>⌘</kbd> |
| Icons / List / Columns / Gallery | <kbd>⌘</kbd><kbd>1</kbd> / <kbd>⌘</kbd><kbd>2</kbd> / <kbd>⌘</kbd><kbd>3</kbd> / <kbd>⌘</kbd><kbd>4</kbd> |
| Columns: open folder / go back | <kbd>→</kbd> / <kbd>←</kbd> |
| Search this folder | <kbd>⌘</kbd><kbd>F</kbd> |
| Show / hide preview | <kbd>⌘</kbd><kbd>⇧</kbd><kbd>P</kbd> |
| Show / hide sidebar | <kbd>⌃</kbd><kbd>⌘</kbd><kbd>S</kbd> |

> [!TIP]
> On a MacBook keyboard, hold <kbd>fn</kbd> for the F-keys.

## 🚀 Quick start

You need [Rust](https://rustup.rs) and [Node.js](https://nodejs.org).

```bash
git clone git@github.com:DistinctZA/patina.git
cd patina
npm install
npm run tauri dev
```

Run the Rust tests for the file operations:

```bash
cd src-tauri && cargo test
```

The file-type icons are copied from the `material-icon-theme` package by `npm run icons`, which runs automatically before `dev`, `build` and `check`.

## 📦 Install on your Mac

Download the latest `.dmg` from **[Releases](https://github.com/DistinctZA/patina/releases)**, open it, and drag Patina into **Applications**.

Or build it yourself:

```bash
npm run tauri build -- --bundles app
```

and drag `src-tauri/target/release/bundle/macos/Patina.app` into **Applications**. Builds are signed with the Developer ID certificate named in `src-tauri/tauri.conf.json`, so they open without Gatekeeper warnings on this Mac.

> [!IMPORTANT]
> macOS asks once per protected folder (Desktop, Documents, Downloads, external drives). To skip those prompts entirely, add Patina under **System Settings → Privacy & Security → Full Disk Access**. Every build is signed with the same certificate, so the permission survives rebuilds.

<details>
<summary><b>Sharing Patina with other Macs (notarization)</b></summary>

<br />

A copy downloaded on another Mac must also be **notarized**, or Gatekeeper blocks it. Tauri notarizes during the build when these are set:

```bash
export APPLE_ID="you@example.com"
export APPLE_PASSWORD="app-specific-password"   # create at appleid.apple.com
export APPLE_TEAM_ID="Z8DX5X3H4R"
npm run tauri build -- --bundles app,dmg
```

</details>

## 🏷️ Versions and releases

Patina uses [semantic versioning](https://semver.org): **MAJOR.MINOR.PATCH**. New features bump the minor number (0.3.0 → 0.4.0) and fixes bump the patch number (0.4.0 → 0.4.1). Every push to GitHub is a release:

```bash
npm run release -- minor            # or: patch, major
npm run release -- minor --dry-run  # preview the version and notes
```

The script bumps the version everywhere it appears, runs the checks and tests, builds the signed app and a signed `.dmg`, then commits, tags `vX.Y.Z`, pushes, and publishes a [GitHub Release](https://github.com/DistinctZA/patina/releases) with the `.dmg` and notes from the commits since the last release.

## 🧠 How it works

```
┌───────────────────────────── Svelte 5 UI ─────────────────────────────┐
│   Left pane  ◄── Tab ──►  Right pane      virtual list · keyboard     │
└───────────────┬───────────────────────────────────────▲───────────────┘
                │ invoke (list_dir, copy_into, …)       │ dir-changed events
┌───────────────▼────────────────── Rust core ──────────┴───────────────┐
│   fs_ops.rs: list · sort · copy · move · rename   notify (FSEvents)   │
│   runs on blocking threads, so the window never freezes               │
└───────────────────────────────────────────────────────────────────────┘
```

| Path | What lives there |
|---|---|
| `src-tauri/src/fs_ops.rs` | Listing, natural sort, copy, move, rename, with tests |
| `src-tauri/src/lib.rs` | Tauri commands and the folder watcher |
| `src/lib/pane-state.svelte.ts` | One pane's folder, cursor, selection and history |
| `src-tauri/src/thumbs.rs` | Quick Look thumbnails and app icons, with a disk cache |
| `src/lib/Pane.svelte` | A pane: list and icon views (virtualized), or columns and gallery |
| `src/lib/ColumnView.svelte`, `GalleryView.svelte` | The Columns and Gallery views |
| `src/lib/material.ts`, `scripts/material-icons.mjs` | Material Icon Theme lookup and its generator |
| `src/lib/drag.svelte.ts` | Drag and drop within the window |
| `src/lib/ContextMenu.svelte` | Right-click menus |
| `src-tauri/src/pasteboard.rs` | Files on the system clipboard, shared with Finder |
| `src/lib/Sidebar.svelte` | Drives, favourites (drag to add or reorder) and tags |
| `src/lib/Preview.svelte` | Preview and the customisable Info section |
| `src/routes/+page.svelte` | Layout, toolbar, search, shortcuts and dialogs |
| `scripts/release.mjs` | Version bump, signed build and `.dmg`, tag and GitHub Release |

## 🗺️ Roadmap

- [x] Dual-pane browsing, copy, move, rename, new folder, Bin
- [x] Live refresh, natural sort, hidden files, type-to-jump
- [x] Signed, unsandboxed macOS build
- [x] Sidebar with drives, favourites and tags
- [x] Icon view, preview panel, search
- [x] Columns and Gallery views, Material file-type icons
- [x] Drag folders into Favourites; collapsible sidebar sections
- [x] Recents, and a right-click menu to show or hide sidebar sections
- [ ] Tabs in each pane
- [ ] Quick Open (fuzzy search across folders)
- [ ] Batch rename
- [ ] Folder compare & sync
- [ ] Full-screen Quick Look (<kbd>Space</kbd>)
- [x] Drag and drop between panes, onto folders and the sidebar, and in from Finder
- [x] Copy and paste files with Finder, duplicate, right-click menus, Go to Folder
- [ ] Drag files out to Finder and other apps
- [ ] Copy progress for large transfers

---

<div align="center">
  <sub>Made with 🦀 Rust. File icons from <a href="https://github.com/material-extensions/vscode-material-icon-theme">Material Icon Theme</a> (MIT).</sub>
</div>
