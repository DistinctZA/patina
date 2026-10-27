<div align="center">

<img src="docs/logo.svg" width="112" alt="Patina logo" />

# Patina

**A free, open-source dual-pane file manager for Mac.**<br />
A fast, keyboard-friendly Finder alternative, built with Rust.

<p>
  <a href="https://github.com/DistinctZA/patina/releases/latest"><img alt="Latest version" src="https://img.shields.io/badge/version-0.6.0-36373c?style=flat-square" /></a>
  <a href="LICENSE"><img alt="MIT License" src="https://img.shields.io/badge/license-MIT-3fb950?style=flat-square" /></a>
  <img alt="macOS 13 or later" src="https://img.shields.io/badge/macOS-13+-000000?style=flat-square&logo=apple&logoColor=white" />
  <img alt="Built with Rust" src="https://img.shields.io/badge/built_with-Rust-b7410e?style=flat-square&logo=rust&logoColor=white" />
</p>

<p>
  <a href="https://github.com/DistinctZA/patina/releases/latest"><b>Download for Mac</b></a> ·
  <a href="#features">Features</a> ·
  <a href="#keyboard-shortcuts">Shortcuts</a> ·
  <a href="#faq">FAQ</a> ·
  <a href="#build-from-source">Build from source</a>
</p>

<img src="docs/screenshot-panes.png" width="900" alt="Patina, a dual-pane file manager for macOS, showing two folders side by side with a preview panel on the right" />

</div>

Patina shows two folders side by side, so moving and copying files takes one step instead of juggling Finder windows. It works the way your Mac does, with drag and drop, copy and paste, and Move to Bin, and adds what Finder is missing: two panes, a preview panel, batch rename, copy progress with a Cancel button, and a keyboard shortcut for everything.

## Download

1. Download the `.dmg` from the [latest release](https://github.com/DistinctZA/patina/releases/latest).
2. Open it and drag **Patina** into **Applications**.
3. The first time you open Patina, macOS may say it can't check the app. Open **System Settings → Privacy & Security**, scroll down and click **Open Anyway**. You only need to do this once.

Patina runs on Macs with Apple silicon (M1 or later) and macOS 13 Ventura or later. On an Intel Mac, [build it from source](#build-from-source).

After that, Patina keeps itself up to date: when a new version is out, click **Update** at the bottom of the sidebar.

> [!TIP]
> macOS asks for permission the first time you open Desktop, Documents, Downloads or an external drive. To skip those prompts, add Patina under **System Settings → Privacy & Security → Full Disk Access**.

## Features

- **Two panes side by side.** Press <kbd>Tab</kbd> to switch between them, <kbd>F5</kbd> to copy to the other pane and <kbd>F6</kbd> to move.
- **Drag and drop everywhere.** Between panes, onto folders, the sidebar or the path at the top, in from Finder and out to other apps. Hold a file over a folder to open it.
- **Copy and paste with Finder.** Copy files in one app and paste them in the other.
- **Four views.** Icons, List, Columns and Gallery, chosen separately for each pane.
- **Preview panel.** Images, video, audio, text and code, plus details such as size and dates.
- **Batch rename.** Rename many files at once: find and replace, add text, number them or change case, with a live preview.
- **Copy progress.** See how long a big copy will take, and cancel it safely at any time.
- **Fast with huge folders.** Even a folder of 50,000 files scrolls smoothly.
- **Sidebar.** Drives, Favourites, Recent folders and Finder tags. Drag folders in to add them.
- **Icons for 1,300+ file types**, so you can tell files apart at a glance.
- **Safe by default.** Deleted files go to the Bin, and nothing is ever overwritten: a clash becomes "name copy".
- **Updates itself.** Patina checks for new versions and installs them in one click. The version number is at the bottom of the sidebar.
- **Private.** Everything stays on your Mac. No account, no ads, no tracking.

<div align="center">
<img src="docs/screenshot-icons.png" width="900" alt="Patina showing a project folder in List view and the Mac's apps in Icons view, previewing Calculator" />
<br />
<sub>List view on the left, Icons view on the right, and the preview panel.</sub>
</div>

## Keyboard shortcuts

| Action | Keys |
|---|---|
| Switch pane | <kbd>Tab</kbd> |
| Open | <kbd>Return</kbd> or <kbd>⌘</kbd><kbd>↓</kbd> |
| Go to the enclosing folder | <kbd>⌫</kbd> or <kbd>⌘</kbd><kbd>↑</kbd> |
| Back / forward | <kbd>⌘</kbd><kbd>[</kbd> / <kbd>⌘</kbd><kbd>]</kbd> |
| Go to folder | <kbd>⇧</kbd><kbd>⌘</kbd><kbd>G</kbd> |
| Copy / move to the other pane | <kbd>F5</kbd> / <kbd>F6</kbd> |
| Copy / paste files | <kbd>⌘</kbd><kbd>C</kbd> / <kbd>⌘</kbd><kbd>V</kbd> |
| Move copied files here | <kbd>⌥</kbd><kbd>⌘</kbd><kbd>V</kbd> |
| Duplicate | <kbd>⌘</kbd><kbd>D</kbd> |
| Rename (several files: batch rename) | <kbd>F2</kbd> |
| New folder | <kbd>F7</kbd> or <kbd>⇧</kbd><kbd>⌘</kbd><kbd>N</kbd> |
| Move to Bin | <kbd>F8</kbd> or <kbd>⌘</kbd><kbd>⌫</kbd> |
| Select all / clear selection | <kbd>⌘</kbd><kbd>A</kbd> / <kbd>Esc</kbd> |
| Search this folder | <kbd>⌘</kbd><kbd>F</kbd> |
| Icons / List / Columns / Gallery | <kbd>⌘</kbd><kbd>1</kbd> to <kbd>⌘</kbd><kbd>4</kbd> |
| Show hidden files | <kbd>⇧</kbd><kbd>⌘</kbd><kbd>.</kbd> |
| Show in Finder | <kbd>⌥</kbd><kbd>⌘</kbd><kbd>R</kbd> |
| Refresh | <kbd>⌘</kbd><kbd>R</kbd> |
| Show or hide the preview / sidebar | <kbd>⇧</kbd><kbd>⌘</kbd><kbd>P</kbd> / <kbd>⌃</kbd><kbd>⌘</kbd><kbd>S</kbd> |
| While dragging: copy / move | hold <kbd>⌥</kbd> / <kbd>⌘</kbd> |

Start typing a name to jump to it. On a MacBook, hold <kbd>fn</kbd> for the F-keys.

## FAQ

**Is Patina free?**<br />
Yes. Patina is free and open source under the [MIT License](LICENSE).

**How is it different from Finder?**<br />
Finder shows one folder per window. Patina shows two side by side, so you can see where files are going. It also adds batch rename, copy progress you can cancel, a preview panel and shortcuts for every action.

**Is it like Total Commander or ForkLift?**<br />
Yes. Patina is a two-pane file manager in the same tradition, made for macOS.

**Why does macOS warn me the first time I open it?**<br />
Patina is signed by its developer but isn't notarized by Apple yet, so macOS asks you to confirm once. See step 3 under [Download](#download).

**Does Patina collect any data?**<br />
No. Your files never leave your Mac. The only time Patina goes online is to check GitHub for a new version, which you can turn off: click the version number at the bottom of the sidebar and untick **Check automatically**.

**How do updates work?**<br />
Patina checks for a new version when it starts and twice a day. Click **Update and Restart** and it downloads, installs and reopens itself. Every update is signed, and Patina refuses any download that isn't.

**Is there a Windows or Linux version?**<br />
Not yet. Patina is Mac-only for now.

## Build from source

You need [Rust](https://rustup.rs), [Node.js](https://nodejs.org) 20.19 or later, and Apple's command line tools (`xcode-select --install`).

```bash
git clone https://github.com/DistinctZA/patina.git
cd patina
npm install
npm run tauri dev
```

To build the app:

```bash
APPLE_SIGNING_IDENTITY=- npm run tauri build -- --bundles app
```

The app is saved to `src-tauri/target/release/bundle/macos/Patina.app`; drag it into Applications. `APPLE_SIGNING_IDENTITY=-` signs it for your own Mac. Leave it out if you have an Apple Developer ID and have put its name in `src-tauri/tauri.conf.json`.

To run the checks and tests:

```bash
npm run check
cd src-tauri && cargo test
```

<details>
<summary><b>How Patina is built</b></summary>

<br />

Patina is a [Tauri 2](https://tauri.app) app. The file operations are written in Rust and the interface in [Svelte 5](https://svelte.dev) and TypeScript. Folder listings, copies and moves run on background threads, so the window never freezes.

| Folder or file | What's in it |
|---|---|
| `src-tauri/src/fs_ops.rs` | Listing, sorting, renaming and new folders |
| `src-tauri/src/transfer.rs` | Copy and move, with progress and Cancel |
| `src-tauri/src/thumbs.rs` | Thumbnails and app icons, with a disk cache |
| `src-tauri/src/pasteboard.rs` | Files on the clipboard, shared with Finder |
| `src-tauri/src/lib.rs` | The commands the interface calls, and the folder watcher |
| `src/routes/+page.svelte` | The window: toolbar, panes, shortcuts and dialogs |
| `src/lib/Pane.svelte`, `pane-state.svelte.ts` | One pane and its folder, selection and history |
| `src/lib/drag.svelte.ts` | Drag and drop |
| `src/lib/Sidebar.svelte`, `Preview.svelte` | The sidebar and the preview panel |
| `scripts/release.mjs` | Builds, signs and publishes a release |

</details>

## Contributing

Bug reports, ideas and pull requests are welcome. For bigger changes, please [open an issue](https://github.com/DistinctZA/patina/issues) first so we can talk it through. Release notes are made from commit messages, so write them for a reader: a short subject, then what changed.

## Roadmap

- [ ] Tabs in each pane
- [ ] Quick Open: find any folder by typing part of its name
- [ ] Compare and sync two folders
- [ ] Full-screen Quick Look with <kbd>Space</kbd>

## License

Patina is released under the [MIT License](LICENSE). File-type icons come from [Material Icon Theme](https://github.com/material-extensions/vscode-material-icon-theme), also MIT.
