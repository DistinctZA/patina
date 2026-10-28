<script lang="ts">
  // A small player for the preview: the video (or nothing, for audio) with a
  // slim control bar, in place of WebKit's large built-in controls.
  import { Maximize2, Pause, Play, Volume2, VolumeX } from "@lucide/svelte";

  let { src, kind }: { src: string; kind: "video" | "audio" } = $props();

  let el = $state<HTMLMediaElement>();
  let paused = $state(true);
  let muted = $state(false);
  let time = $state(0);
  let duration = $state(0);
  const progress = $derived(duration ? (time / duration) * 100 : 0);

  function toggle() {
    if (!el) return;
    if (el.paused) el.play().catch(() => {});
    else el.pause();
  }

  function seek(e: Event & { currentTarget: HTMLInputElement }) {
    if (el) el.currentTime = Number(e.currentTarget.value);
  }

  function fullScreen() {
    const video = el as (HTMLVideoElement & { webkitEnterFullscreen?: () => void }) | undefined;
    if (!video) return;
    if (document.fullscreenEnabled) video.requestFullscreen().catch(() => {});
    else video.webkitEnterFullscreen?.();
  }

  /** 75 → "1:15". */
  function clock(seconds: number) {
    if (!Number.isFinite(seconds)) return "0:00";
    const s = Math.floor(seconds);
    const h = Math.floor(s / 3600);
    const mm = String(Math.floor((s % 3600) / 60));
    const ss = String(s % 60).padStart(2, "0");
    return h ? `${h}:${mm.padStart(2, "0")}:${ss}` : `${mm}:${ss}`;
  }
</script>

{#key src}
  <div class="player {kind}">
    {#if kind === "video"}
      <!-- svelte-ignore a11y_media_has_caption -->
      <!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_noninteractive_element_interactions -->
      <!-- "auto", so the first frame shows before Play; these are local files. -->
      <video
        bind:this={el}
        {src}
        preload="auto"
        playsinline
        bind:paused
        bind:muted
        bind:currentTime={time}
        bind:duration
        onclick={toggle}
      ></video>
    {:else}
      <audio bind:this={el} {src} preload="metadata" bind:paused bind:muted bind:currentTime={time} bind:duration
      ></audio>
    {/if}

    <div class="bar">
      <button onclick={toggle} title={paused ? "Play" : "Pause"} aria-label={paused ? "Play" : "Pause"}>
        {#if paused}<Play size={13} fill="currentColor" />{:else}<Pause size={13} fill="currentColor" />{/if}
      </button>
      <span class="time">{clock(time)}</span>
      <input
        class="seek"
        type="range"
        min="0"
        max={duration || 0}
        step="any"
        value={time}
        style:--progress="{progress}%"
        aria-label="Position"
        oninput={seek}
        onchange={(e) => e.currentTarget.blur()}
      />
      <span class="time">{clock(duration)}</span>
      <button onclick={() => (muted = !muted)} title={muted ? "Unmute" : "Mute"} aria-label={muted ? "Unmute" : "Mute"}>
        {#if muted}<VolumeX size={13} />{:else}<Volume2 size={13} />{/if}
      </button>
      {#if kind === "video"}
        <button onclick={fullScreen} title="Full screen" aria-label="Full screen"><Maximize2 size={12} /></button>
      {/if}
    </div>
  </div>
{/key}

<style>
  .player.video {
    position: absolute;
    inset: 10px;
    display: grid;
    /* Without minmax(0, …), the video's own width would widen the player. */
    grid-template-columns: minmax(0, 1fr);
    grid-template-rows: minmax(0, 1fr) auto;
    gap: 6px;
  }
  .player.audio {
    width: 100%;
  }
  video {
    width: 100%;
    height: 100%;
    min-height: 0;
    object-fit: contain;
    border-radius: 4px;
    cursor: pointer;
  }

  .bar {
    min-width: 0;
    display: flex;
    align-items: center;
    gap: 6px;
    height: 26px;
    padding: 0 6px;
    border-radius: 6px;
    background: var(--panel);
    font-size: 11px;
    color: var(--muted);
  }
  .bar button {
    display: grid;
    place-items: center;
    flex: none;
    width: 22px;
    height: 22px;
    border-radius: 5px;
    color: var(--text);
  }
  .bar button:hover {
    background: var(--hover);
  }
  .time {
    flex: none;
    font-variant-numeric: tabular-nums;
  }

  /* A thin track that fills as it plays, with a small round handle. */
  .seek {
    flex: 1;
    min-width: 40px;
    height: 14px;
    margin: 0;
    background: transparent;
    -webkit-appearance: none;
    appearance: none;
    cursor: pointer;
  }
  .seek::-webkit-slider-runnable-track {
    height: 3px;
    border-radius: 2px;
    background: linear-gradient(to right, var(--text) var(--progress), var(--line) var(--progress));
  }
  .seek::-webkit-slider-thumb {
    -webkit-appearance: none;
    width: 10px;
    height: 10px;
    margin-top: -3.5px;
    border-radius: 50%;
    background: var(--text);
  }
</style>
