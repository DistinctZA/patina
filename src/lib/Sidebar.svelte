<script lang="ts">
  import {
    AppWindow,
    CircleArrowDown,
    Clapperboard,
    Cloud,
    FileMusic,
    FileText,
    HardDrive,
    House,
    Image,
    Monitor,
    Usb,
  } from "@lucide/svelte";
  import { FINDER_TAGS, TAG_COLORS, formatSize, type SidebarData } from "./api";

  let {
    data,
    current,
    onopen,
  }: { data: SidebarData | null; current: string; onopen: (location: string) => void } = $props();

  const ICONS = {
    home: House,
    desktop: Monitor,
    documents: FileText,
    downloads: CircleArrowDown,
    applications: AppWindow,
    movies: Clapperboard,
    music: FileMusic,
    pictures: Image,
    icloud: Cloud,
  } as Record<string, typeof House>;
</script>

<aside class="sidebar">
  <!-- Room for the window's traffic-light buttons, and a handle to drag the window. -->
  <div class="titlebar" data-tauri-drag-region></div>

  <nav>
    <h3>Devices</h3>
    {#each data?.volumes ?? [] as vol (vol.path)}
      <button class="item" class:current={current === vol.path} onclick={() => onopen(vol.path)} title={vol.path}>
        <span class="ico">{#if vol.isRoot}<HardDrive size={16} />{:else}<Usb size={16} />{/if}</span>
        <span class="label">{vol.name}</span>
        {#if vol.available != null}<span class="meta">{formatSize(vol.available)}</span>{/if}
      </button>
    {/each}

    <h3>Favourites</h3>
    {#each data?.favorites ?? [] as place (place.path)}
      {@const Icon = ICONS[place.icon] ?? House}
      <button class="item" class:current={current === place.path} onclick={() => onopen(place.path)} title={place.path}>
        <span class="ico" class:green={place.icon === "downloads"}><Icon size={16} /></span>
        <span class="label">{place.name}</span>
      </button>
    {/each}

    <h3>Tags</h3>
    {#each FINDER_TAGS as tag (tag.name)}
      <button class="item" class:current={current === `tag:${tag.name}`} onclick={() => onopen(`tag:${tag.name}`)}>
        <span class="dot" style:background={TAG_COLORS[tag.color]}></span>
        <span class="label">{tag.name}</span>
      </button>
    {/each}
  </nav>
</aside>

<style>
  /* An inset card, darker than the window around it. */
  .sidebar {
    display: flex;
    flex-direction: column;
    min-height: 0;
    margin: 6px 0 6px 6px;
    border-radius: 12px;
    background: var(--sidebar);
    border: 1px solid var(--line);
  }
  .titlebar {
    flex: none;
    height: 44px;
  }
  nav {
    flex: 1;
    overflow-y: auto;
    padding: 0 8px 12px;
  }
  h3 {
    margin: 14px 8px 4px;
    font-size: 11px;
    font-weight: 600;
    color: var(--faint);
  }
  h3:first-child {
    margin-top: 2px;
  }
  .item {
    display: flex;
    align-items: center;
    gap: 9px;
    width: 100%;
    height: 28px;
    padding: 0 8px;
    border-radius: 6px;
    color: var(--text);
    text-align: left;
  }
  .item:hover {
    background: var(--hover);
  }
  .item.current {
    background: var(--selection);
  }
  .ico {
    display: grid;
    color: var(--accent);
  }
  .ico.green {
    color: #3ecf6e;
  }
  .label {
    flex: 1;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .meta {
    font-size: 11px;
    color: var(--faint);
    font-variant-numeric: tabular-nums;
  }
  .dot {
    width: 10px;
    height: 10px;
    margin: 0 2px;
    border-radius: 50%;
  }
</style>
