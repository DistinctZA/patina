<script lang="ts">
  import { X } from "@lucide/svelte";
  import { formatSize } from "./api";
  import { transfers, type Job } from "./transfers.svelte";

  const shown = $derived(transfers.jobs.filter((j) => j.shown));

  // Re-render the time-left estimate about once a second.
  let now = $state(Date.now());
  $effect(() => {
    if (!shown.length) return;
    const timer = setInterval(() => (now = Date.now()), 1000);
    return () => clearInterval(timer);
  });

  /** 0–1, by bytes when there are any to copy (renames are items only). */
  function fraction(job: Job): number {
    if (job.totalBytes > 0) return Math.min(1, job.doneBytes / job.totalBytes);
    return job.totalItems > 0 ? Math.min(1, job.doneItems / job.totalItems) : 0;
  }

  function timeLeft(job: Job): string | null {
    const f = fraction(job);
    const elapsed = (now - job.started) / 1000;
    if (f <= 0.02 || elapsed < 3) return null;
    const left = (elapsed / f) * (1 - f);
    if (left < 5) return "a few seconds left";
    if (left < 60) return `about ${Math.round(left / 5) * 5} seconds left`;
    const minutes = Math.round(left / 60);
    return `about ${minutes} minute${minutes === 1 ? "" : "s"} left`;
  }
</script>

{#if shown.length}
  <div class="panel" role="status">
    {#each shown as job (job.id)}
      {@const left = timeLeft(job)}
      <div class="job">
        <div class="head">
          <strong>
            {job.verb}
            {job.totalItems > 1 ? `${Math.min(job.doneItems + 1, job.totalItems)} of ${job.totalItems} items` : (job.current || "…")}
          </strong>
          <button class="cancel" onclick={() => transfers.cancel(job.id)} title="Cancel">
            <X size={13} />
          </button>
        </div>
        <div class="bar"><div class="fill" style:width="{fraction(job) * 100}%"></div></div>
        <div class="meta">
          {#if job.totalBytes > 0}{formatSize(job.doneBytes)} of {formatSize(job.totalBytes)}{/if}
          {#if left} · {left}{/if}
          {#if job.totalItems > 1 && job.current}<span class="file" title={job.current}>{job.current}</span>{/if}
        </div>
      </div>
    {/each}
  </div>
{/if}

<style>
  .panel {
    position: fixed;
    right: 16px;
    bottom: 16px;
    z-index: 55;
    display: flex;
    flex-direction: column;
    gap: 8px;
    width: min(340px, calc(100vw - 32px));
  }
  .job {
    padding: 10px 12px;
    border-radius: 10px;
    background: var(--panel);
    border: 1px solid var(--line);
    box-shadow: 0 12px 30px rgba(0, 0, 0, 0.45);
  }
  .head {
    display: flex;
    align-items: center;
    gap: 8px;
  }
  .head strong {
    flex: 1;
    min-width: 0;
    font-size: 12px;
    font-weight: 600;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .cancel {
    display: grid;
    place-items: center;
    width: 20px;
    height: 20px;
    border-radius: 50%;
    background: var(--button-on);
    color: var(--text);
  }
  .cancel:hover {
    background: #c9483a;
  }
  .bar {
    height: 5px;
    margin: 8px 0 6px;
    border-radius: 3px;
    background: var(--well);
    overflow: hidden;
  }
  .fill {
    height: 100%;
    border-radius: 3px;
    background: var(--blue);
    transition: width 0.15s linear;
  }
  .meta {
    display: flex;
    gap: 6px;
    font-size: 11px;
    color: var(--muted);
    white-space: nowrap;
  }
  .file {
    flex: 1;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    text-align: right;
    color: var(--faint);
  }
</style>
