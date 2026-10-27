<script lang="ts">
  import {
    AppWindow,
    Check,
    ChevronDown,
    CircleArrowDown,
    Clapperboard,
    Clock,
    Cloud,
    FileMusic,
    FileText,
    Folder,
    HardDrive,
    House,
    Image,
    Monitor,
    Usb,
    X,
  } from "@lucide/svelte";
  import { FINDER_TAGS, TAG_COLORS, basename, formatSize, type SidebarData } from "./api";
  import { consumeClick, drag, startDrag } from "./drag.svelte";

  let {
    data,
    current,
    collapsed,
    hidden,
    recents,
    customFavorites,
    onopen,
    ontoggle,
    onremove,
    onshow,
    onresetfavorites,
    onclearrecents,
  }: {
    data: SidebarData | null;
    current: string;
    /** Section names the user has folded away. */
    collapsed: string[];
    /** Section names switched off entirely (right-click menu). */
    hidden: string[];
    /** Recently visited folders, newest first. */
    recents: string[];
    /** Whether Favourites differ from the defaults (enables "Reset"). */
    customFavorites: boolean;
    onopen: (location: string) => void;
    ontoggle: (section: string) => void;
    onremove: (path: string) => void;
    onshow: (section: string) => void;
    onresetfavorites: () => void;
    onclearrecents: () => void;
  } = $props();

  /** Sections that can be switched on and off, in sidebar order. */
  const SECTIONS = [
    { id: "devices", label: "Devices" },
    { id: "favorites", label: "Favourites" },
    { id: "recents", label: "Recents" },
    { id: "tags", label: "Tags" },
  ];

  let menu = $state<{ x: number; y: number } | null>(null);

  function openMenu(e: MouseEvent) {
    e.preventDefault();
    // Keep the menu on screen near the bottom of the window.
    menu = { x: e.clientX, y: Math.min(e.clientY, window.innerHeight - 250) };
  }

  function choose(action: () => void) {
    action();
    menu = null;
  }

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
    folder: Folder,
  } as Record<string, typeof House>;

  const open = (location: string) => {
    if (!consumeClick()) onopen(location);
  };

  // Reordering Favourites, or adding folders to them, shows an insertion
  // line. Files dropped onto a favourite go into that folder instead.
  const inserting = $derived(
    drag.payload?.kind === "favorite" || (drag.payload?.kind === "entries" && drag.overPath == null),
  );
  const dropAt = $derived(
    inserting && drag.over === "favorites" ? (drag.overIndex ?? data?.favorites.length ?? 0) : -1,
  );
  const isDropTarget = (path: string) => !!drag.verdict && drag.overPath === path;
</script>

{#snippet heading(name: string, label: string)}
  <button class="heading" onclick={() => ontoggle(name)} title={collapsed.includes(name) ? `Show ${label}` : `Hide ${label}`}>
    {label}
    <span class="chevron" class:closed={collapsed.includes(name)}><ChevronDown size={13} /></span>
  </button>
{/snippet}

<svelte:window
  onmousedown={(e) => menu && !(e.target as Element).closest(".menu") && (menu = null)}
  onkeydown={(e) => menu && e.key === "Escape" && (menu = null)}
/>

<aside class="sidebar" oncontextmenu={openMenu}>
  <!-- Room for the window's traffic-light buttons, and a handle to drag the window. -->
  <div class="titlebar" data-tauri-drag-region></div>

  <nav data-autoscroll="y">
    {#if !hidden.includes("devices")}
      {@render heading("devices", "Devices")}
      {#if !collapsed.includes("devices")}
        {#each data?.volumes ?? [] as vol (vol.path)}
          <button
            class="item"
            class:current={current === vol.path}
            class:drop-target={isDropTarget(vol.path)}
            onclick={() => open(vol.path)}
            title={vol.path}
            data-drop="folder"
            data-drop-path={vol.path}
            data-spring
          >
            <span class="ico">{#if vol.isRoot}<HardDrive size={16} />{:else}<Usb size={16} />{/if}</span>
            <span class="label">{vol.name}</span>
            {#if vol.available != null}<span class="meta">{formatSize(vol.available)}</span>{/if}
          </button>
        {/each}
      {/if}
    {/if}

    {#if !hidden.includes("favorites")}
      {@render heading("favorites", "Favourites")}
      {#if !collapsed.includes("favorites")}
        <!-- The whole list is a drop zone; each row says "insert before me". -->
        <div class="favorites" class:dropping={dropAt >= 0} data-drop="favorites">
          {#each data?.favorites ?? [] as place, i (place.path)}
            {@const Icon = ICONS[place.icon] ?? Folder}
            <div
              class="row"
              class:insert-before={dropAt === i}
              data-drop="favorites"
              data-drop-index={i}
              data-drop-path={drag.payload?.kind === "favorite" ? undefined : place.path}
              data-spring={drag.payload?.kind === "favorite" ? undefined : ""}
            >
              <button
                class="item"
                class:current={current === place.path}
                class:drop-target={isDropTarget(place.path)}
                onclick={() => open(place.path)}
                onmousedown={(e) => startDrag(e, () => ({ kind: "favorite", index: i }))}
                title={place.path}
              >
                <span class="ico"><Icon size={16} /></span>
                <span class="label">{place.name}</span>
              </button>
              <button class="remove" onclick={() => onremove(place.path)} title="Remove from Favourites">
                <X size={12} />
              </button>
            </div>
          {/each}
          <div class="tail" class:insert-before={dropAt === (data?.favorites.length ?? 0)}>
            {#if drag.payload?.kind === "entries"}Drop folders here to add them{/if}
          </div>
        </div>
      {/if}
    {/if}

    {#if !hidden.includes("recents") && recents.length}
      {@render heading("recents", "Recents")}
      {#if !collapsed.includes("recents")}
        {#each recents as path (path)}
          <button
            class="item"
            class:current={current === path}
            class:drop-target={isDropTarget(path)}
            onclick={() => open(path)}
            title={path}
            data-drop="folder"
            data-drop-path={path}
            data-spring
          >
            <span class="ico"><Clock size={16} /></span>
            <span class="label">{basename(path)}</span>
          </button>
        {/each}
      {/if}
    {/if}

    {#if !hidden.includes("tags")}
      {@render heading("tags", "Tags")}
      {#if !collapsed.includes("tags")}
        {#each FINDER_TAGS as tag (tag.name)}
          <button class="item" class:current={current === `tag:${tag.name}`} onclick={() => open(`tag:${tag.name}`)}>
            <span class="dot" style:background={TAG_COLORS[tag.color]}></span>
            <span class="label">{tag.name}</span>
          </button>
        {/each}
      {/if}
    {/if}

    {#if SECTIONS.every((sec) => hidden.includes(sec.id))}
      <p class="empty">Right-click to choose what the sidebar shows.</p>
    {/if}
  </nav>
</aside>

{#if menu}
  <div class="menu" role="menu" style:left="{menu.x}px" style:top="{menu.y}px">
    <p>Show in Sidebar</p>
    {#each SECTIONS as sec (sec.id)}
      <button role="menuitemcheckbox" aria-checked={!hidden.includes(sec.id)} onclick={() => choose(() => onshow(sec.id))}>
        <span class="tick">{#if !hidden.includes(sec.id)}<Check size={13} />{/if}</span>
        {sec.label}
      </button>
    {/each}
    <hr />
    <button role="menuitem" disabled={!customFavorites} onclick={() => choose(onresetfavorites)}>
      <span class="tick"></span>Reset Favourites
    </button>
    <button role="menuitem" disabled={!recents.length} onclick={() => choose(onclearrecents)}>
      <span class="tick"></span>Clear Recents
    </button>
  </div>
{/if}

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

  .heading {
    display: flex;
    align-items: center;
    justify-content: space-between;
    width: 100%;
    margin-top: 10px;
    padding: 4px 8px;
    border-radius: 5px;
    font-size: 11px;
    font-weight: 600;
    color: var(--faint);
  }
  .heading:first-child {
    margin-top: 0;
  }
  .heading:hover {
    color: var(--muted);
  }
  /* Like Finder: the chevron only shows on hover, or when folded. */
  .chevron {
    display: grid;
    opacity: 0;
    transition:
      opacity 0.12s,
      transform 0.15s;
  }
  .heading:hover .chevron,
  .chevron.closed {
    opacity: 1;
  }
  .chevron.closed {
    transform: rotate(-90deg);
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
  .item.drop-target {
    background: rgba(47, 111, 228, 0.3);
    box-shadow: inset 0 0 0 1.5px var(--blue);
  }
  .ico {
    display: grid;
    color: var(--sidebar-icon);
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

  .favorites {
    border-radius: 7px;
  }
  .favorites.dropping {
    background: var(--hover);
  }
  .row {
    position: relative;
  }
  .row .item {
    padding-right: 26px;
  }
  .remove {
    position: absolute;
    top: 6px;
    right: 6px;
    display: grid;
    place-items: center;
    width: 16px;
    height: 16px;
    border-radius: 50%;
    color: var(--muted);
    opacity: 0;
  }
  .row:hover .remove {
    opacity: 1;
  }
  .remove:hover {
    background: var(--button-on);
    color: var(--text);
  }
  /* The insertion line while dragging. */
  .insert-before::before {
    content: "";
    position: absolute;
    left: 8px;
    right: 8px;
    top: -1px;
    height: 2px;
    border-radius: 1px;
    background: var(--sidebar-icon);
  }
  .empty {
    margin: 12px 8px;
    font-size: 12px;
    color: var(--faint);
  }

  .menu {
    position: fixed;
    z-index: 60;
    min-width: 190px;
    padding: 5px;
    border-radius: 9px;
    background: var(--panel);
    border: 1px solid var(--line);
    box-shadow: 0 12px 30px rgba(0, 0, 0, 0.5);
  }
  .menu p {
    margin: 3px 8px 5px;
    font-size: 11px;
    font-weight: 600;
    color: var(--faint);
  }
  .menu button {
    display: flex;
    align-items: center;
    gap: 6px;
    width: 100%;
    padding: 4px 8px;
    border-radius: 5px;
    font-size: 13px;
    text-align: left;
  }
  .menu button:hover:not(:disabled) {
    background: var(--blue);
    color: #fff;
  }
  .menu button:disabled {
    color: var(--faint);
  }
  .tick {
    display: grid;
    width: 14px;
  }
  .menu hr {
    margin: 5px 4px;
    border: 0;
    border-top: 1px solid var(--line);
  }

  .tail {
    position: relative;
    min-height: 6px;
    padding: 0 8px;
    font-size: 11px;
    line-height: 22px;
    color: var(--faint);
  }
</style>
