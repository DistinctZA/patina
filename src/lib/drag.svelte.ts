// Pointer-based drag and drop inside the window. It avoids HTML5 drag events,
// which the webview shares with the OS file-drop handling.
//
// Drop zones are plain elements marked `data-drop="<zone>"`, optionally with
// `data-drop-path="<folder>"` (where files would go),
// `data-drop-index="<n>"` (where in a list the drop lands) and `data-spring`
// (holding a drag over it opens the folder). Scrolling areas marked
// `data-autoscroll="x"` or `"y"` scroll when the pointer nears their edge, and
// `data-pane="<id>"` says which pane an element belongs to.
import type { Entry } from "./api";

export type DragPayload =
  | { kind: "entries"; entries: Entry[] }
  | { kind: "favorite"; index: number }
  /** Files dragged in from Finder or another app. */
  | { kind: "files"; paths: string[] };

export type DropTarget = { zone: string; path: string | null; index: number | null };

/** What a drop would do, worked out by the page as the pointer moves. */
export type Verdict = { verb: "Copy" | "Move" | "Add"; dest: string | null } | null;

export type DropHandler = (payload: DragPayload, target: DropTarget, mods: { alt: boolean; meta: boolean }) => void;

class DragState {
  payload = $state.raw<DragPayload | null>(null);
  x = $state(0);
  y = $state(0);
  /** The zone under the pointer, if any. */
  over = $state<string | null>(null);
  overPath = $state<string | null>(null);
  overIndex = $state<number | null>(null);
  /** The pane under the pointer, and whether the zone opens if you hover. */
  overPane = $state<string | null>(null);
  overSpring = $state(false);
  /** ⌥ forces a copy, ⌘ a move. */
  alt = $state(false);
  meta = $state(false);
  /** Null when the drop isn't allowed here. */
  verdict = $state.raw<Verdict>(null);
}

export const drag = new DragState();

let onDrop: DropHandler | null = null;
let onLeaveWindow: ((payload: DragPayload, mods: { alt: boolean; meta: boolean }) => void) | null = null;
let suppressClick = false;

export function setDropHandler(handler: DropHandler) {
  onDrop = handler;
}

/** Called when a drag of files leaves the window, to hand it to macOS. */
export function setLeaveWindowHandler(handler: typeof onLeaveWindow) {
  onLeaveWindow = handler;
}

/** True once, right after a drag ends, so the click that follows is ignored. */
export function consumeClick(): boolean {
  const was = suppressClick;
  suppressClick = false;
  return was;
}

/** Updates which drop zone is under a point (client coordinates). */
export function hover(x: number, y: number) {
  drag.x = x;
  drag.y = y;
  const under = document.elementFromPoint(x, y);
  const zone = under?.closest<HTMLElement>("[data-drop]");
  drag.over = zone?.dataset.drop ?? null;
  drag.overPath = zone?.dataset.dropPath ?? null;
  drag.overIndex = zone?.dataset.dropIndex != null ? Number(zone.dataset.dropIndex) : null;
  drag.overSpring = !!zone?.hasAttribute("data-spring");
  drag.overPane = under?.closest<HTMLElement>("[data-pane]")?.dataset.pane ?? null;
}

const EDGE = 36;

/** Scrolls whatever the pointer is near the edge of; true if anything moved. */
function autoScroll(x: number, y: number): boolean {
  let moved = false;
  for (let el = document.elementFromPoint(x, y); el; el = el.parentElement) {
    const axis = (el as HTMLElement).dataset?.autoscroll;
    if (!axis) continue;
    const r = el.getBoundingClientRect();
    // Faster the closer the pointer gets to the edge.
    const speed = (d: number) => Math.ceil(((EDGE - d) / EDGE) * 18);
    if (axis === "y") {
      if (y - r.top < EDGE) el.scrollTop -= speed(y - r.top);
      else if (r.bottom - y < EDGE) el.scrollTop += speed(r.bottom - y);
      else continue;
    } else {
      if (x - r.left < EDGE) el.scrollLeft -= speed(x - r.left);
      else if (r.right - x < EDGE) el.scrollLeft += speed(r.right - x);
      else continue;
    }
    moved = true;
  }
  return moved;
}

export function currentTarget(): DropTarget | null {
  return drag.over ? { zone: drag.over, path: drag.overPath, index: drag.overIndex } : null;
}

export function endDrag() {
  document.body.classList.remove("dragging");
  drag.payload = null;
  drag.over = null;
  drag.overPath = null;
  drag.overIndex = null;
  drag.overPane = null;
  drag.overSpring = false;
  drag.alt = false;
  drag.meta = false;
}

/** Drops the current payload where the pointer is, if anything is there. */
export function dropHere() {
  const target = currentTarget();
  if (drag.payload && target) onDrop?.(drag.payload, target, { alt: drag.alt, meta: drag.meta });
}

/**
 * Call from a mousedown. Nothing happens unless the pointer then moves a few
 * pixels with the button held; `make` builds the payload at that moment.
 * Escape cancels.
 */
export function startDrag(e: MouseEvent, make: () => DragPayload | null) {
  if (e.button !== 0) return;
  const startX = e.clientX;
  const startY = e.clientY;
  let started = false;
  let lastX = startX;
  let lastY = startY;
  let frame = 0;

  // While the pointer rests near a list's edge, keep scrolling it.
  const tick = () => {
    if (autoScroll(lastX, lastY)) hover(lastX, lastY);
    frame = requestAnimationFrame(tick);
  };

  const modifiers = (ev: MouseEvent | KeyboardEvent) => {
    drag.alt = ev.altKey;
    drag.meta = ev.metaKey;
  };

  const move = (ev: MouseEvent) => {
    if (!started) {
      if (Math.hypot(ev.clientX - startX, ev.clientY - startY) < 5) return;
      const payload = make();
      if (!payload) return stop();
      drag.payload = payload;
      started = true;
      document.body.classList.add("dragging");
      frame = requestAnimationFrame(tick);
    }
    modifiers(ev);
    lastX = ev.clientX;
    lastY = ev.clientY;
    // Leaving the window turns it into a system drag (to Finder, Mail…).
    const outside = ev.clientX < 0 || ev.clientY < 0 || ev.clientX >= innerWidth || ev.clientY >= innerHeight;
    if (outside && drag.payload?.kind === "entries" && onLeaveWindow) {
      onLeaveWindow(drag.payload, { alt: drag.alt, meta: drag.meta });
      suppressClick = true;
      setTimeout(() => (suppressClick = false), 0);
      return stop();
    }
    hover(ev.clientX, ev.clientY);
  };

  const key = (ev: KeyboardEvent) => {
    if (!started) return;
    if (ev.key === "Escape") return stop();
    modifiers(ev);
  };

  const up = () => {
    if (started) {
      suppressClick = true;
      // Clear it after the click (if any) that follows this mouseup.
      setTimeout(() => (suppressClick = false), 0);
      dropHere();
    }
    stop();
  };

  const stop = () => {
    cancelAnimationFrame(frame);
    window.removeEventListener("mousemove", move);
    window.removeEventListener("mouseup", up);
    window.removeEventListener("keydown", key, true);
    window.removeEventListener("keyup", key, true);
    if (started) endDrag();
  };

  window.addEventListener("mousemove", move);
  window.addEventListener("mouseup", up);
  window.addEventListener("keydown", key, true);
  window.addEventListener("keyup", key, true);
}
