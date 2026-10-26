<script lang="ts">
  import { untrack } from "svelte";
  import { entryKey, isApp, thumbnailUrl, wantsThumbnail, type Entry } from "./api";
  import { materialIcon } from "./material";

  let {
    entry,
    size = 16,
    thumbnail = false,
  }: { entry: Entry; size?: number; thumbnail?: boolean } = $props();

  let url = $state<string | null>(null);

  // Apps always show their real icon, even in the list.
  const show = $derived((thumbnail || isApp(entry)) && wantsThumbnail(entry));
  // Re-listing a folder makes new entry objects; only reload when the file changed.
  const key = $derived(show ? `${entryKey(entry)}|${size}` : "");
  // Large type icons look heavy next to real thumbnails, so draw them smaller.
  const iconSize = $derived(thumbnail && size > 32 ? Math.round(size * 0.66) : size);

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
  <img class="thumb" src={url} alt="" draggable="false" style:max-width="{size}px" style:max-height="{size}px" />
{:else if isApp(entry)}
  <!-- Holds the space while the app's own icon loads. -->
  <span class="blank" style:width="{size}px" style:height="{size}px"></span>
{:else}
  <span class="icon" style:width="{size}px" style:height="{size}px">
    <img src={materialIcon(entry)} alt="" draggable="false" width={iconSize} height={iconSize} />
  </span>
{/if}

<style>
  .thumb {
    display: block;
    object-fit: contain;
    /* Follows the image's shape, so transparent app icons don't get a box. */
    filter: drop-shadow(0 2px 5px rgba(0, 0, 0, 0.4));
  }
  .icon,
  .blank {
    flex: none;
    display: inline-grid;
    place-items: center;
  }
</style>
