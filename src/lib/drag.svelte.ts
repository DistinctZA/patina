// Pointer-based drag and drop inside the window. It avoids HTML5 drag events,
// which the webview shares with the OS file-drop handling.
//
// Drop zones are plain elements marked `data-drop="<zone>"`, optionally with
// `data-drop-path="<folder>"` (where files would go) and
// `data-drop-index="<n>"` (where in a list the drop lands).
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
  /** ⌥ forces a copy, ⌘ a move. */
  alt = $state(false);
  meta = $state(false);
  /** Null when the drop isn't allowed here. */
  verdict = $state.raw<Verdict>(null);
}

export const drag = new DragState();

let onDrop: DropHandler | null = null;
let suppressClick = false;

export function setDropHandler(handler: DropHandler) {
  onDrop = handler;
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
  const zone = document.elementFromPoint(x, y)?.closest<HTMLElement>("[data-drop]");
  drag.over = zone?.dataset.drop ?? null;
  drag.overPath = zone?.dataset.dropPath ?? null;
  drag.overIndex = zone?.dataset.dropIndex != null ? Number(zone.dataset.dropIndex) : null;
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
    }
    modifiers(ev);
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
