<script lang="ts">
  // A handle on the edge between two panels: drag it sideways to resize
  // them, double-click it to go back to the usual size.
  let {
    left,
    label,
    onmove,
    ondone,
    onreset,
  }: {
    /** Where the edge is, as a CSS length from the parent's left. */
    left: string;
    label: string;
    /** The pointer's x position (window coordinates) while dragging. */
    onmove: (x: number) => void;
    /** The drag has ended; a good time to save. */
    ondone: () => void;
    onreset: () => void;
  } = $props();

  let dragging = $state(false);

  function start(e: MouseEvent) {
    if (e.button !== 0) return;
    e.preventDefault();
    e.stopPropagation();
    dragging = true;
    document.body.classList.add("resizing");
    // A click, or the first half of a double-click, leaves the size alone.
    let moved = false;
    const move = (ev: MouseEvent) => {
      if (!moved && Math.abs(ev.clientX - e.clientX) < 2) return;
      moved = true;
      onmove(ev.clientX);
    };
    const up = (ev: MouseEvent) => {
      dragging = false;
      document.body.classList.remove("resizing");
      window.removeEventListener("mousemove", move);
      window.removeEventListener("mouseup", up);
      if (!moved) return;
      // The last move can be skipped when the pointer moves fast.
      onmove(ev.clientX);
      ondone();
    };
    window.addEventListener("mousemove", move);
    window.addEventListener("mouseup", up);
  }
</script>

<!-- A pointer convenience: every panel works at its usual size without it. -->
<!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
<div
  class="divider"
  class:dragging
  style:left
  role="separator"
  aria-orientation="vertical"
  aria-label={label}
  title="Drag to resize; double-click to reset"
  onmousedown={start}
  ondblclick={onreset}
></div>

<style>
  .divider {
    position: absolute;
    z-index: 6;
    top: 0;
    bottom: 0;
    width: 9px;
    translate: -50% 0;
    cursor: col-resize;
  }
  /* A thin line that lights up, so it's clear what's being dragged. */
  .divider::after {
    content: "";
    position: absolute;
    top: 0;
    bottom: 0;
    left: 4px;
    width: 1px;
    transition: background 0.15s;
  }
  .divider:hover::after,
  .divider.dragging::after {
    background: var(--accent);
  }
</style>
