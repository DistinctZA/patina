<script lang="ts">
  // The version, at the foot of the sidebar; click it to check for, and
  // install, a new version.
  import { CircleArrowUp, LoaderCircle } from "@lucide/svelte";
  import { transfers } from "./transfers.svelte";
  import { updates } from "./updates.svelte";

  let {
    autoCheck,
    onautocheck,
  }: {
    /** Whether Patina looks for new versions by itself. */
    autoCheck: boolean;
    onautocheck: (on: boolean) => void;
  } = $props();

  let open = $state(false);
  const status = $derived(updates.status);
  // Restarting would cut a copy off halfway.
  const copying = $derived(transfers.jobs.length > 0);
  const percent = $derived(
    status.kind === "downloading" && status.total ? Math.min(100, Math.round((status.done / status.total) * 100)) : null,
  );
  const notes = $derived(status.kind === "available" ? tidy(status.update.body ?? "") : "");

  /** Release notes are Markdown; show them as plain text, without the install footer. */
  function tidy(md: string) {
    return md
      .replace(/\n-{3,}[\s\S]*$/, "")
      .replace(/^#+\s*/gm, "")
      .replace(/\n{3,}/g, "\n\n")
      .trim();
  }

  function toggle() {
    open = !open;
    // Opening it before anything is known looks straight away.
    if (open && (status.kind === "idle" || status.kind === "error")) updates.check();
  }
</script>

<svelte:window
  onmousedown={(e) => open && !(e.target as Element).closest(".version") && (open = false)}
  onkeydown={(e) => open && e.key === "Escape" && (open = false)}
/>

<div class="version">
  {#if open}
    <div class="popover" role="dialog" aria-label="Version and updates">
      <p class="name">Patina</p>
      <p class="muted">Version {updates.version}</p>

      <div class="status">
        {#if status.kind === "checking"}
          <p class="muted row"><span class="spin"><LoaderCircle size={13} /></span> Checking for updates…</p>
        {:else if status.kind === "current"}
          <p>You're up to date.</p>
        {:else if status.kind === "available"}
          <p><b>Version {status.update.version}</b> is available.</p>
          {#if notes}<pre class="notes">{notes}</pre>{/if}
          <button class="primary" disabled={copying} onclick={() => updates.install()}>Update and Restart</button>
          {#if copying}<p class="hint">Finish or cancel the copy in progress first.</p>{/if}
        {:else if status.kind === "downloading"}
          <p>Downloading version {status.update.version}…{percent != null ? ` ${percent}%` : ""}</p>
          <div class="bar"><div style:width="{percent ?? 0}%"></div></div>
        {:else if status.kind === "restarting"}
          <p>Installed. Restarting…</p>
        {:else if status.kind === "error"}
          <p class="error">{status.message}</p>
        {/if}
      </div>

      {#if status.kind === "idle" || status.kind === "current" || status.kind === "error"}
        <button class="plain" onclick={() => updates.check()}>Check for Updates</button>
      {/if}
      <label class="auto">
        <input type="checkbox" checked={autoCheck} onchange={(e) => onautocheck(e.currentTarget.checked)} />
        Check automatically
      </label>
    </div>
  {/if}

  <button class="footer" class:open onclick={toggle} title="Version and updates">
    <span>Patina {updates.version}</span>
    {#if updates.available}
      <span class="badge"><CircleArrowUp size={12} /> Update</span>
    {/if}
  </button>
</div>

<style>
  .version {
    position: relative;
    flex: none;
    padding: 6px 8px 8px;
    border-top: 1px solid var(--line);
  }

  .footer {
    display: flex;
    align-items: center;
    justify-content: space-between;
    width: 100%;
    padding: 4px 8px;
    border-radius: 6px;
    font-size: 11px;
    color: var(--faint);
  }
  .footer:hover,
  .footer.open {
    background: var(--hover);
    color: var(--muted);
  }
  .badge {
    display: inline-flex;
    align-items: center;
    gap: 4px;
    padding: 1px 6px;
    border-radius: 9px;
    background: var(--accent);
    color: #1d1d1f;
    font-weight: 600;
  }

  .popover {
    position: absolute;
    z-index: 60;
    left: 8px;
    bottom: calc(100% + 6px);
    width: 280px;
    padding: 12px;
    border-radius: 10px;
    background: var(--panel);
    border: 1px solid var(--line);
    box-shadow: 0 12px 30px rgba(0, 0, 0, 0.5);
    font-size: 12px;
    -webkit-user-select: text;
    user-select: text;
  }
  .name {
    font-size: 13px;
    font-weight: 600;
  }
  .muted {
    color: var(--faint);
  }
  .status {
    margin: 10px 0;
    display: grid;
    gap: 8px;
  }
  .row {
    display: flex;
    align-items: center;
    gap: 6px;
  }
  .error {
    color: #ff8a80;
  }
  .hint {
    color: var(--faint);
    font-size: 11px;
  }
  .notes {
    max-height: 140px;
    overflow-y: auto;
    padding: 8px;
    border-radius: 6px;
    background: var(--well);
    font: inherit;
    font-size: 11px;
    line-height: 1.45;
    color: var(--muted);
    white-space: pre-wrap;
  }

  .primary,
  .plain {
    width: 100%;
    padding: 5px 10px;
    border-radius: 6px;
    font-size: 12px;
  }
  .primary {
    background: var(--blue);
    color: #fff;
    font-weight: 600;
  }
  .primary:disabled {
    opacity: 0.5;
  }
  .plain {
    background: var(--button);
    border: 1px solid var(--line);
  }
  .plain:hover {
    background: var(--button-hover);
  }

  .bar {
    height: 4px;
    border-radius: 2px;
    background: var(--well);
    overflow: hidden;
  }
  .bar div {
    height: 100%;
    background: var(--blue);
    transition: width 0.2s;
  }

  .auto {
    display: flex;
    align-items: center;
    gap: 6px;
    margin-top: 10px;
    color: var(--muted);
  }

  .spin {
    display: inline-flex;
    animation: spin 1s linear infinite;
  }
  @keyframes spin {
    to {
      transform: rotate(360deg);
    }
  }
</style>
