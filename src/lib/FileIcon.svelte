<script lang="ts">
  import { untrack } from "svelte";
  import { File, FileArchive, FileCode, FileImage, FileMusic, FilePlay, Folder } from "@lucide/svelte";
  import { entryKey, isApp, mediaOf, thumbnailUrl, wantsThumbnail, type Entry } from "./api";

  let {
    entry,
    size = 16,
    thumbnail = false,
  }: { entry: Entry; size?: number; thumbnail?: boolean } = $props();

  const Icon = $derived.by(() => {
    if (entry.isDir) return Folder;
    switch (mediaOf(entry)) {
      case "image":
        return FileImage;
      case "video":
        return FilePlay;
      case "audio":
        return FileMusic;
      case "archive":
        return FileArchive;
      case "code":
        return FileCode;
      default:
        return File;
    }
  });

  let url = $state<string | null>(null);

  // Apps always show their real icon, even in the list.
  const show = $derived((thumbnail || isApp(entry)) && wantsThumbnail(entry));
  // Re-listing a folder makes new entry objects; only reload when the file changed.
  const key = $derived(show ? `${entryKey(entry)}|${size}` : "");

  $effect(() => {
    url = null;
    if (!key) return;
    let alive = true;
    // Ask for 2× pixels so thumbnails stay sharp on Retina screens.
    untrack(() => thumbnailUrl(entry, Math.round(size * 2))).then((u) => alive && (url = u));
    return () => (alive = false);
  });
</script>

{#if url}
  <img src={url} alt="" draggable="false" style:max-width="{size}px" style:max-height="{size}px" />
{:else}
  <span class="icon" class:folder={entry.isDir} style:width="{size}px" style:height="{size}px">
    <Icon size={size * (thumbnail ? 0.62 : 1)} strokeWidth={thumbnail ? 1.25 : 1.75} fill={entry.isDir ? "currentColor" : "none"} />
  </span>
{/if}

<style>
  img {
    display: block;
    object-fit: contain;
    /* Follows the image's shape, so transparent app icons don't get a box. */
    filter: drop-shadow(0 2px 5px rgba(0, 0, 0, 0.4));
  }
  .icon {
    flex: none;
    display: inline-grid;
    place-items: center;
    color: var(--file);
  }
  .icon.folder {
    color: var(--folder);
  }
</style>
