<script lang="ts">
  import { untrack } from "svelte";
  import { ChevronDown, ExternalLink, Search } from "@lucide/svelte";
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

  let { entry, onerror }: { entry: Entry | undefined; onerror: (err: unknown) => void } = $props();

  let details = $state<Details | null>(null);
  let text = $state<TextPreview | null>(null);
  let thumb = $state<string | null>(null);
  let imageFailed = $state(false);
  let showGeneral = $state(true);
  let showPermissions = $state(true);

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
    if (e.isDir) quickLook();
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

  // Permission bits, rows owner/group/others × columns read/write/execute.
  const WHO = [
    { label: "Owner", shift: 6 },
    { label: "Group", shift: 3 },
    { label: "Others", shift: 0 },
  ];
  const RWX = [4, 2, 1];

  function symbolic(mode: number) {
    return [6, 3, 0]
      .map((s) => ((mode >> s) & 4 ? "r" : "-") + ((mode >> s) & 2 ? "w" : "-") + ((mode >> s) & 1 ? "x" : "-"))
      .join("");
  }

  async function toggleBit(bit: number) {
    if (!entry || !details) return;
    const path = entry.path;
    try {
      await api.setMode(path, details.mode ^ bit);
      details = await api.details(path);
    } catch (err) {
      onerror(err);
    }
  }

  const where = $derived(entry ? entry.path.slice(0, entry.path.lastIndexOf("/")) || "/" : "");
</script>

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
      {#if entry.tags.length}
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
      <button class="section-head" onclick={() => (showGeneral = !showGeneral)}>
        General <span class:closed={!showGeneral}><ChevronDown size={14} /></span>
      </button>
      {#if showGeneral}
        <dl>
          <dt>Where</dt>
          <dd title={where}>{where}</dd>
          {#if !entry.isDir}
            <dt>Size</dt>
            <dd>{formatSize(entry.size)} ({entry.size.toLocaleString()} bytes)</dd>
          {/if}
          <dt>Modified</dt>
          <dd>{formatDate(details?.modified ?? entry.modified)}</dd>
          <dt>Created</dt>
          <dd>{formatDate(details?.created ?? null)}</dd>
          <dt>Last opened</dt>
          <dd>{formatDate(details?.accessed ?? null)}</dd>
          {#if !entry.isDir}
            <dt>Extension</dt>
            <dd>{extension(entry.name) || "—"}</dd>
          {/if}
          <dt>Hidden</dt>
          <dd>{entry.isHidden ? "Yes" : "No"}</dd>
          {#if details?.linkTarget}
            <dt>Alias of</dt>
            <dd title={details.linkTarget}>{details.linkTarget}</dd>
          {/if}
        </dl>
      {/if}
    </section>

    {#if details}
      <section>
        <button class="section-head" onclick={() => (showPermissions = !showPermissions)}>
          Permissions <span class:closed={!showPermissions}><ChevronDown size={14} /></span>
        </button>
        {#if showPermissions}
          <div class="perms">
            <span></span><span class="h">Read</span><span class="h">Write</span><span class="h">Run</span>
            {#each WHO as who (who.label)}
              <span class="who">{who.label}</span>
              {#each RWX as bit (bit)}
                <input
                  type="checkbox"
                  checked={(details.mode & (bit << who.shift)) !== 0}
                  onchange={() => toggleBit(bit << who.shift)}
                />
              {/each}
            {/each}
          </div>
          <dl>
            <dt>Mode</dt>
            <dd class="mono">{symbolic(details.mode)} · {details.mode.toString(8).padStart(3, "0")}</dd>
            <dt>Owner</dt>
            <dd>{details.owner}</dd>
            <dt>Group</dt>
            <dd>{details.group}</dd>
          </dl>
        {/if}
      </section>
    {/if}
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
    flex: none;
    height: 240px;
    margin: 14px 14px 0;
    display: grid;
    place-items: center;
    border-radius: 10px;
    background: var(--well);
    overflow: hidden;
  }
  .media {
    max-width: 100%;
    max-height: 100%;
    object-fit: contain;
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
    display: flex;
    align-items: center;
    justify-content: space-between;
    width: 100%;
    padding: 8px 0;
    font-weight: 600;
  }
  .section-head span {
    display: grid;
    color: var(--muted);
    transition: transform 0.15s;
  }
  .section-head span.closed {
    transform: rotate(-90deg);
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
  .mono {
    font-family: ui-monospace, "SF Mono", Menlo, monospace;
  }

  .perms {
    display: grid;
    grid-template-columns: 1fr repeat(3, 48px);
    align-items: center;
    justify-items: center;
    row-gap: 6px;
    margin-bottom: 12px;
    font-size: 12px;
  }
  .perms .h {
    font-size: 11px;
    color: var(--faint);
  }
  .perms .who {
    justify-self: start;
    color: var(--faint);
  }
  .perms input {
    margin: 0;
    accent-color: var(--blue);
  }

  .nothing {
    margin: auto;
    color: var(--faint);
  }
</style>
