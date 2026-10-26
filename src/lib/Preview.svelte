<script module lang="ts">
  /** Details the Info section can show, in display order. */
  export const INFO_FIELDS = [
    { id: "kind", label: "Kind" },
    { id: "size", label: "Size" },
    { id: "items", label: "Items in folder" },
    { id: "where", label: "Where" },
    { id: "modified", label: "Modified" },
    { id: "created", label: "Created" },
    { id: "opened", label: "Last opened" },
    { id: "extension", label: "Extension" },
    { id: "tags", label: "Tags" },
    { id: "hidden", label: "Hidden" },
    { id: "owner", label: "Owner" },
    { id: "permissions", label: "Permissions" },
  ] as const;

  export type InfoField = (typeof INFO_FIELDS)[number]["id"];

  export const DEFAULT_INFO_FIELDS: InfoField[] = ["kind", "size", "items", "where", "modified", "created", "tags"];
</script>

<script lang="ts">
  import { untrack } from "svelte";
  import { ExternalLink, Search, SlidersHorizontal } from "@lucide/svelte";
  import {
    TAG_COLORS,
    api,
    entryKey,
    extension,
    isApp,
    fileUrl,
    formatDate,
    formatSize,
    kindOf,
    mediaOf,
    thumbnailUrl,
    wantsThumbnail,
    type Details,
    type Entry,
    type TextPreview,
  } from "./api";
  import FileIcon from "./FileIcon.svelte";

  let {
    entry,
    fields,
    onfields,
    onerror,
  }: {
    entry: Entry | undefined;
    /** The fields the user chose, or undefined for the defaults. */
    fields: string[] | undefined;
    onfields: (fields: InfoField[]) => void;
    onerror: (err: unknown) => void;
  } = $props();

  let details = $state<Details | null>(null);
  let text = $state<TextPreview | null>(null);
  let thumb = $state<string | null>(null);
  let imageFailed = $state(false);
  let choosing = $state(false);

  const shown = $derived(new Set(fields ?? DEFAULT_INFO_FIELDS));
  const media = $derived(entry ? mediaOf(entry) : null);
  // Re-listing a folder makes new entry objects; only reload when the file changed.
  const key = $derived(entry ? entryKey(entry) : "");

  $effect(() => {
    void key;
    const e = untrack(() => entry);
    details = null;
    text = null;
    thumb = null;
    imageFailed = false;
    if (!e) return;
    let alive = true;

    api.details(e.path).then((d) => alive && (details = d)).catch(() => {});

    const quickLook = () => {
      if (wantsThumbnail(e)) thumbnailUrl(e, 640).then((u) => alive && (thumb = u));
    };
    const kind = mediaOf(e);
    // Wallpaper descriptors are XML that points at a picture: show the picture.
    if (e.isDir || extension(e.name) === "madesktop") quickLook();
    else if (kind !== "image" && kind !== "video" && kind !== "audio") {
      // Text and code show their contents; everything else (PDFs, Office
      // files, …) gets a Quick Look picture.
      api
        .readText(e.path)
        .then((t) => {
          if (!alive) return;
          if (t && t.text.trim()) text = t;
          else quickLook();
        })
        .catch(quickLook);
    }
    return () => (alive = false);
  });

  function imageError() {
    // WebKit can't decode a few formats (e.g. some TIFFs); Quick Look can.
    imageFailed = true;
    if (entry) thumbnailUrl(entry, 640).then((u) => (thumb = u));
  }

  function symbolic(mode: number) {
    return [6, 3, 0]
      .map((s) => ((mode >> s) & 4 ? "r" : "-") + ((mode >> s) & 2 ? "w" : "-") + ((mode >> s) & 1 ? "x" : "-"))
      .join("");
  }

  function toggleField(id: InfoField) {
    const next = new Set(shown);
    if (next.has(id)) next.delete(id);
    else next.add(id);
    onfields(INFO_FIELDS.map((f) => f.id).filter((f) => next.has(f)));
  }

  /** The rows to show for the chosen fields; ones that don't apply are skipped. */
  const rows = $derived.by(() => {
    const e = entry;
    if (!e) return [];
    const d = details;
    const out: { label: string; value: string }[] = [];
    const add = (label: string, value: string | null | undefined) => {
      if (value != null) out.push({ label, value });
    };
    for (const { id, label } of INFO_FIELDS) {
      if (!shown.has(id)) continue;
      switch (id) {
        case "kind":
          add(label, kindOf(e));
          break;
        case "size":
          if (!e.isDir) add(label, `${formatSize(e.size)} (${e.size.toLocaleString()} bytes)`);
          break;
        case "items":
          if (e.isDir && !isApp(e) && d?.items != null) add("Items", d.items.toLocaleString());
          break;
        case "where":
          add(label, e.path.slice(0, e.path.lastIndexOf("/")) || "/");
          break;
        case "modified":
          add(label, formatDate(d?.modified ?? e.modified));
          break;
        case "created":
          add(label, formatDate(d?.created ?? null));
          break;
        case "opened":
          add(label, formatDate(d?.accessed ?? null));
          break;
        case "extension":
          if (!e.isDir) add(label, extension(e.name) || "—");
          break;
        case "hidden":
          add(label, e.isHidden ? "Yes" : "No");
          break;
        case "owner":
          if (d) add(label, `${d.owner} (${d.group})`);
          break;
        case "permissions":
          if (d) add(label, `${symbolic(d.mode)} · ${d.mode.toString(8).padStart(3, "0")}`);
          break;
      }
    }
    if (d?.linkTarget) add("Alias of", d.linkTarget);
    return out;
  });
</script>

<svelte:window
  onmousedown={(e) => choosing && !(e.target as Element).closest(".chooser, .customise") && (choosing = false)}
/>

<aside class="preview">
  {#if entry}
    <div class="stage">
      {#if media === "image" && !imageFailed}
        <img class="media" src={fileUrl(entry.path)} alt={entry.name} onerror={imageError} draggable="false" />
      {:else if media === "video"}
        <!-- svelte-ignore a11y_media_has_caption -->
        <video class="media" src={fileUrl(entry.path)} controls preload="metadata"></video>
      {:else if media === "audio"}
        <div class="audio">
          <FileIcon {entry} size={96} thumbnail />
          <audio src={fileUrl(entry.path)} controls preload="metadata"></audio>
        </div>
      {:else if text}
        <pre class="text">{text.text}{text.truncated ? "\n…" : ""}</pre>
      {:else if thumb}
        <img class="media" src={thumb} alt={entry.name} draggable="false" />
      {:else}
        <FileIcon {entry} size={112} />
      {/if}
    </div>

    <div class="title">
      <h2 title={entry.name}>{entry.name}</h2>
      <p>
        {kindOf(entry)}{#if !entry.isDir} · {formatSize(entry.size)}{/if}
        {#if details?.items != null && !isApp(entry)} · {details.items} item{details.items === 1 ? "" : "s"}{/if}
      </p>
      {#if shown.has("tags") && entry.tags.length}
        <div class="tags">
          {#each entry.tags as t (t.name)}
            <span class="tag">
              {#if t.color}<span class="dot" style:background={TAG_COLORS[t.color]}></span>{/if}{t.name}
            </span>
          {/each}
        </div>
      {/if}
      <div class="buttons">
        <button onclick={() => api.openPath(entry.path).catch(onerror)}><ExternalLink size={13} /> Open</button>
        <button onclick={() => api.revealInFinder(entry.path).catch(onerror)}><Search size={13} /> Show in Finder</button>
      </div>
    </div>

    <section>
      <div class="section-head">
        <span>Info</span>
        <button class="customise" class:on={choosing} onclick={() => (choosing = !choosing)} title="Choose what to show">
          <SlidersHorizontal size={14} />
        </button>
        {#if choosing}
          <div class="chooser" role="menu">
            <p>Show in Info</p>
            {#each INFO_FIELDS as f (f.id)}
              <label>
                <input type="checkbox" checked={shown.has(f.id)} onchange={() => toggleField(f.id)} />
                {f.label}
              </label>
            {/each}
            <button class="reset" onclick={() => onfields([...DEFAULT_INFO_FIELDS])}>Reset to defaults</button>
          </div>
        {/if}
      </div>
      {#if rows.length}
        <dl>
          {#each rows as row (row.label)}
            <dt>{row.label}</dt>
            <dd title={row.value}>{row.value}</dd>
          {/each}
        </dl>
      {:else}
        <p class="none">No details chosen</p>
      {/if}
    </section>
  {:else}
    <p class="nothing">Nothing selected</p>
  {/if}
</aside>

<style>
  .preview {
    display: flex;
    flex-direction: column;
    min-height: 0;
    overflow-y: auto;
    background: var(--panel);
    border-left: 1px solid var(--border);
  }

  .stage {
    position: relative;
    flex: none;
    /* Grows and shrinks with the window, within sensible bounds. */
    height: clamp(180px, 38vh, 560px);
    margin: 14px 14px 0;
    display: grid;
    place-items: center;
    border-radius: 10px;
    background: var(--well);
    overflow: hidden;
  }
  /* Placed absolutely so the max sizes resolve against the stage's real
     size: the whole picture always fits, letterboxed, never cropped. */
  .media {
    position: absolute;
    inset: 10px;
    margin: auto;
    max-width: calc(100% - 20px);
    max-height: calc(100% - 20px);
    object-fit: contain;
    border-radius: 4px;
  }
  .audio {
    display: grid;
    justify-items: center;
    gap: 14px;
    width: 90%;
  }
  .audio audio {
    width: 100%;
  }
  .text {
    align-self: stretch;
    justify-self: stretch;
    margin: 0;
    padding: 10px 12px;
    overflow: auto;
    font: 11px/1.45 ui-monospace, "SF Mono", Menlo, monospace;
    color: var(--muted);
    white-space: pre-wrap;
    word-break: break-word;
    -webkit-user-select: text;
    user-select: text;
  }

  .title {
    padding: 14px 16px 6px;
  }
  h2 {
    margin: 0;
    font-size: 15px;
    font-weight: 600;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .title p {
    margin: 2px 0 0;
    color: var(--muted);
    font-size: 12px;
  }
  .tags {
    display: flex;
    flex-wrap: wrap;
    gap: 4px;
    margin-top: 8px;
  }
  .tag {
    display: inline-flex;
    align-items: center;
    gap: 5px;
    padding: 2px 8px;
    border-radius: 10px;
    background: var(--well);
    font-size: 11px;
  }
  .dot {
    width: 8px;
    height: 8px;
    border-radius: 50%;
  }
  .buttons {
    display: flex;
    gap: 6px;
    margin-top: 10px;
  }
  .buttons button {
    display: inline-flex;
    align-items: center;
    gap: 5px;
    padding: 4px 10px;
    border-radius: 6px;
    background: var(--button);
    border: 1px solid var(--border);
    font-size: 12px;
  }
  .buttons button:hover {
    background: var(--button-hover);
  }

  section {
    padding: 4px 16px 8px;
    border-top: 1px solid var(--line);
    margin-top: 8px;
  }
  .section-head {
    position: relative;
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 8px 0;
    font-weight: 600;
  }
  .customise {
    display: grid;
    place-items: center;
    width: 26px;
    height: 22px;
    border-radius: 6px;
    color: var(--muted);
  }
  .customise:hover,
  .customise.on {
    background: var(--button-hover);
    color: var(--text);
  }
  .chooser {
    position: absolute;
    top: 34px;
    right: 0;
    z-index: 10;
    width: 200px;
    padding: 6px;
    border-radius: 10px;
    background: var(--panel);
    border: 1px solid var(--line);
    box-shadow: 0 12px 30px rgba(0, 0, 0, 0.5);
    font-weight: 400;
  }
  .chooser p {
    margin: 2px 8px 6px;
    font-size: 11px;
    font-weight: 600;
    color: var(--faint);
  }
  .chooser label {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 4px 8px;
    border-radius: 5px;
    font-size: 12px;
  }
  .chooser label:hover {
    background: var(--hover);
  }
  .chooser input {
    margin: 0;
    accent-color: var(--blue);
  }
  .reset {
    width: 100%;
    margin-top: 4px;
    padding: 5px 8px;
    border-top: 1px solid var(--line);
    border-radius: 0 0 5px 5px;
    text-align: left;
    font-size: 12px;
    color: var(--muted);
  }
  .reset:hover {
    color: var(--text);
  }
  .none {
    margin: 0 0 8px;
    font-size: 12px;
    color: var(--faint);
  }

  dl {
    display: grid;
    grid-template-columns: auto minmax(0, 1fr);
    gap: 7px 12px;
    margin: 0;
    font-size: 12px;
  }
  dt {
    color: var(--faint);
  }
  dd {
    margin: 0;
    text-align: right;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    -webkit-user-select: text;
    user-select: text;
  }


  .nothing {
    margin: auto;
    color: var(--faint);
  }
</style>
