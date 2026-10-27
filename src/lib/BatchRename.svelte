<script lang="ts">
  import { tick } from "svelte";
  import { ArrowRight } from "@lucide/svelte";
  import type { Entry } from "./api";
  import { defaultOptions, newNames, type RenameMode } from "./batch-rename";
  import FileIcon from "./FileIcon.svelte";

  let {
    entries,
    existing,
    onclose,
    onapply,
  }: {
    entries: Entry[];
    /** Lowercased paths of what's already there, to catch clashes early. */
    existing: Set<string>;
    onclose: () => void;
    onapply: (renames: [string, string][]) => Promise<void>;
  } = $props();

  const o = $state(defaultOptions());
  let busy = $state(false);
  let error = $state<string | null>(null);

  const MODES: { id: RenameMode; label: string }[] = [
    { id: "replace", label: "Replace text" },
    { id: "add", label: "Add text" },
    { id: "format", label: "Format" },
    { id: "case", label: "Change case" },
  ];

  const results = $derived(newNames(entries, o, existing));
  const changed = $derived(results.filter((r) => r.name !== r.entry.name));
  const problems = $derived(results.filter((r) => r.error).length);
  const canApply = $derived(!busy && changed.length > 0 && problems === 0);

  async function apply(e: SubmitEvent) {
    e.preventDefault();
    if (!canApply) return;
    busy = true;
    error = null;
    try {
      await onapply(changed.map((r) => [r.entry.path, r.name]));
      onclose();
    } catch (err) {
      error = String(err);
    } finally {
      busy = false;
    }
  }

  // Focus the first field of each mode.
  let form = $state<HTMLFormElement>();
  $effect(() => {
    void o.mode;
    tick().then(() => form?.querySelector<HTMLInputElement>(".fields input")?.focus());
  });
</script>

<div class="backdrop" role="presentation" onmousedown={(e) => e.target === e.currentTarget && onclose()}>
  <form class="dialog" bind:this={form} onsubmit={apply}>
    <h2>Rename {entries.length} items</h2>

    <div class="modes" role="tablist">
      {#each MODES as m (m.id)}
        <button type="button" role="tab" aria-selected={o.mode === m.id} class:on={o.mode === m.id} onclick={() => (o.mode = m.id)}>
          {m.label}
        </button>
      {/each}
    </div>

    <div class="fields">
      {#if o.mode === "replace"}
        <label>Find <input bind:value={o.find} spellcheck="false" autocomplete="off" /></label>
        <label>Replace with <input bind:value={o.replaceWith} spellcheck="false" autocomplete="off" /></label>
        <div class="checks">
          <label class="check"><input type="checkbox" bind:checked={o.matchCase} /> Match case</label>
          <label class="check"><input type="checkbox" bind:checked={o.regex} /> Regular expression</label>
        </div>
      {:else if o.mode === "add"}
        <label>Text <input bind:value={o.text} spellcheck="false" autocomplete="off" /></label>
        <label>
          Where
          <select bind:value={o.textWhere}>
            <option value="before">Before the name</option>
            <option value="after">After the name</option>
          </select>
        </label>
      {:else if o.mode === "format"}
        <label>Name <input bind:value={o.base} placeholder="e.g. Holiday" spellcheck="false" autocomplete="off" /></label>
        <div class="row">
          <label>Start at <input type="number" min="0" bind:value={o.start} /></label>
          <label>Digits <input type="number" min="1" max="9" bind:value={o.digits} /></label>
          <label>
            Number
            <select bind:value={o.numberWhere}>
              <option value="after">After the name</option>
              <option value="before">Before the name</option>
            </select>
          </label>
        </div>
      {:else}
        <label>
          Change to
          <select bind:value={o.caseTo}>
            <option value="title">Title Case</option>
            <option value="lower">lowercase</option>
            <option value="upper">UPPERCASE</option>
          </select>
        </label>
      {/if}
      <label class="check"><input type="checkbox" bind:checked={o.keepExtension} /> Keep extensions (.jpg, .pdf…)</label>
    </div>

    <div class="preview">
      {#each results as r (r.entry.path)}
        <div class="item" class:changed={r.name !== r.entry.name} class:bad={!!r.error}>
          <FileIcon entry={r.entry} size={14} />
          <span class="old" title={r.entry.name}>{r.entry.name}</span>
          <ArrowRight size={12} class="arrow" />
          <span class="new" title={r.error ?? r.name}>{r.error ? `${r.name} — ${r.error}` : r.name}</span>
        </div>
      {/each}
    </div>

    {#if error}<p class="error">{error}</p>{/if}

    <div class="actions">
      <span class="count">
        {problems ? `${problems} name${problems === 1 ? "" : "s"} can't be used` : `${changed.length} of ${entries.length} will change`}
      </span>
      <button type="button" onclick={onclose}>Cancel</button>
      <button type="submit" class="primary" disabled={!canApply}>Rename</button>
    </div>
  </form>
</div>

<style>
  .backdrop {
    position: fixed;
    inset: 0;
    z-index: 60;
    display: grid;
    place-items: start center;
    padding-top: 12vh;
    background: rgba(0, 0, 0, 0.45);
  }
  .dialog {
    width: min(560px, 92vw);
    padding: 16px;
    border-radius: 12px;
    background: var(--panel);
    border: 1px solid var(--line);
    box-shadow: 0 20px 50px rgba(0, 0, 0, 0.6);
  }
  h2 {
    margin: 0 0 12px;
    font-size: 13px;
    font-weight: 600;
  }
  .modes {
    display: flex;
    gap: 2px;
    padding: 3px;
    margin-bottom: 12px;
    border-radius: 8px;
    background: var(--well);
  }
  .modes button {
    flex: 1;
    padding: 4px 8px;
    border-radius: 6px;
    font-size: 12px;
    color: var(--muted);
  }
  .modes button.on {
    background: var(--button-on);
    color: var(--text);
  }
  .fields {
    display: flex;
    flex-direction: column;
    gap: 8px;
  }
  .fields label:not(.check) {
    display: grid;
    grid-template-columns: 96px minmax(0, 1fr);
    align-items: center;
    gap: 8px;
    font-size: 12px;
    color: var(--muted);
  }
  .row {
    display: grid;
    grid-template-columns: 1fr 1fr 1.4fr;
    gap: 8px;
  }
  .row label:not(.check) {
    grid-template-columns: auto minmax(0, 1fr);
  }
  input:not([type="checkbox"]),
  select {
    width: 100%;
    box-sizing: border-box;
    padding: 5px 8px;
    border-radius: 6px;
    border: 1px solid var(--line);
    background: var(--well);
    color: var(--text);
    font: inherit;
    outline: none;
    -webkit-user-select: text;
    user-select: text;
  }
  input:focus,
  select:focus {
    border-color: var(--blue);
  }
  .checks {
    display: flex;
    gap: 16px;
    padding-left: 104px;
  }
  .check {
    display: flex;
    align-items: center;
    gap: 6px;
    font-size: 12px;
    color: var(--muted);
  }
  .check input {
    margin: 0;
    accent-color: var(--blue);
  }
  .preview {
    max-height: 240px;
    margin-top: 14px;
    overflow-y: auto;
    border-radius: 8px;
    border: 1px solid var(--line);
    background: var(--well);
  }
  .item {
    display: grid;
    grid-template-columns: auto minmax(0, 1fr) auto minmax(0, 1fr);
    align-items: center;
    gap: 8px;
    padding: 4px 10px;
    font-size: 12px;
  }
  .item + .item {
    border-top: 1px solid var(--line);
  }
  .old,
  .new {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .old {
    color: var(--muted);
  }
  .item:not(.changed) .new {
    color: var(--faint);
  }
  .item.changed .new {
    color: var(--text);
    font-weight: 500;
  }
  .item.bad .new {
    color: #ff7b6b;
  }
  .item :global(.arrow) {
    color: var(--faint);
  }
  .error {
    margin: 10px 0 0;
    font-size: 12px;
    color: #ff7b6b;
  }
  .actions {
    display: flex;
    align-items: center;
    gap: 8px;
    margin-top: 14px;
  }
  .count {
    flex: 1;
    font-size: 12px;
    color: var(--faint);
  }
  .actions button {
    padding: 5px 12px;
    border-radius: 7px;
    background: var(--button);
    border: 1px solid var(--border);
  }
  .actions .primary {
    background: var(--blue);
    border-color: var(--blue);
    color: #fff;
    font-weight: 500;
  }
  .actions .primary:disabled {
    opacity: 0.5;
  }
</style>
