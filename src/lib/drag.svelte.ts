// Pointer-based drag and drop inside the window. It avoids HTML5 drag events,
// which the webview shares with the OS file-drop handling.
//
// Drop zones are plain elements marked `data-drop="<zone>"`, optionally with
// `data-drop-index="<n>"` to say where in a list the drop lands.
import type { Entry } from "./api";

export type DragPayload =
  | { kind: "entries"; entries: Entry[] }
  | { kind: "favorite"; index: number };

export type DropHandler = (payload: DragPayload, zone: string, index: number | null) => void;

class DragState {
  payload = $state.raw<DragPayload | null>(null);
  x = $state(0);
  y = $state(0);
  /** The zone under the pointer, if any. */
  over = $state<string | null>(null);
  overIndex = $state<number | null>(null);
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

/**
 * Call from a mousedown. Nothing happens unless the pointer then moves a few
 * pixels with the button held; `make` builds the payload at that moment.
 */
export function startDrag(e: MouseEvent, make: () => DragPayload | null) {
  if (e.button !== 0) return;
  const startX = e.clientX;
  const startY = e.clientY;
  let started = false;

  const move = (ev: MouseEvent) => {
    if (!started) {
      if (Math.hypot(ev.clientX - startX, ev.clientY - startY) < 5) return;
      const payload = make();
      if (!payload) return stop();
      drag.payload = payload;
      started = true;
      document.body.classList.add("dragging");
    }
    drag.x = ev.clientX;
    drag.y = ev.clientY;
    const zone = document.elementFromPoint(ev.clientX, ev.clientY)?.closest<HTMLElement>("[data-drop]");
    drag.over = zone?.dataset.drop ?? null;
    drag.overIndex = zone?.dataset.dropIndex != null ? Number(zone.dataset.dropIndex) : null;
  };

  const up = () => {
    if (started) {
      suppressClick = true;
      // Clear it after the click (if any) that follows this mouseup.
      setTimeout(() => (suppressClick = false), 0);
      if (drag.payload && drag.over) onDrop?.(drag.payload, drag.over, drag.overIndex);
    }
    stop();
  };

  const stop = () => {
    window.removeEventListener("mousemove", move);
    window.removeEventListener("mouseup", up);
    document.body.classList.remove("dragging");
    drag.payload = null;
    drag.over = null;
    drag.overIndex = null;
  };

  window.addEventListener("mousemove", move);
  window.addEventListener("mouseup", up);
}
