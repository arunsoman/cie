"use client";

// The camera over the chart canvas: scroll zooms at the pointer, drag pans,
// buttons and the keyboard do the same without a pointer (US-140 parity).
//
// Behaviour lineage (github.com/arunsoman/code-intelligence apps/web G6Canvas):
//   — wheel zooms toward the cursor with an exponential curve, clamped to
//     [0.15, 4] so a big map stays pannable instead of unreadable;
//   — zoom/pan is a pure transform over the drawn content — the content's
//     own geometry (row order, bar widths, node positions) never changes;
//   — a "Fit" action brings everything back into view, and the zoom hint
//     tells the truth about what is off-screen.
//
// Unlike the reference we sit over DOM/SVG views rather than a G6 canvas,
// so the same camera is a CSS transform with transform-origin 0 0.

import { useCallback, useEffect, useRef, useState } from "react";
import { Maximize2, Minus, Plus, Scan } from "lucide-react";

export const ZOOM_MIN = 0.15;
export const ZOOM_MAX = 4;

export interface CameraState {
  zoom: number;
  pan: { x: number; y: number };
}

const clamp = (n: number, lo: number, hi: number) => Math.max(lo, Math.min(hi, n));

export function useCamera(active: boolean) {
  const hostRef = useRef<HTMLDivElement>(null);
  const camera = useRef<CameraState>({ zoom: 1, pan: { x: 0, y: 0 } });
  // A drag that actually moved the camera cancels the next click, so panning
  // never selects a bubble by accident.
  const drag = useRef<{ sx: number; sy: number; px: number; py: number; moved: boolean } | null>(null);
  // Zoom lives in state (the toolbar reads it); pan lives only in the ref —
  // it is consumed by the transform, never by a render.
  const [zoom, setZoom] = useState(1);

  const apply = useCallback(() => {
    const el = hostRef.current;
    if (el) {
      const c = camera.current;
      el.style.transform = `translate(${c.pan.x}px, ${c.pan.y}px) scale(${c.zoom})`;
      el.dataset.zoom = String(c.zoom);
    }
    setZoom(camera.current.zoom);
  }, []);

  const zoomAt = useCallback(
    (factor: number, cx?: number, cy?: number) => {
      const host = hostRef.current?.parentElement; // the clipping stage box
      if (!host) return;
      const c = camera.current;
      const next = clamp(c.zoom * factor, ZOOM_MIN, ZOOM_MAX);
      if (next === c.zoom) return;
      // Keep the point under the cursor fixed while zooming.
      const rect = host.getBoundingClientRect();
      const px = cx ?? rect.width / 2;
      const py = cy ?? rect.height / 2;
      const k = next / c.zoom;
      c.pan.x = px - (px - c.pan.x) * k;
      c.pan.y = py - (py - c.pan.y) * k;
      c.zoom = next;
      apply();
    },
    [apply]
  );

  const zoomBy = useCallback((factor: number) => zoomAt(factor), [zoomAt]);

  const panBy = useCallback(
    (dx: number, dy: number) => {
      camera.current.pan.x += dx;
      camera.current.pan.y += dy;
      apply();
    },
    [apply]
  );

  // Fit: like the reference's fitView — scale down just enough that all the
  // content is inside the box (never above 100%, so small maps stay honest
  // and readable), then reset the pan.
  const fit = useCallback(() => {
    const wrapper = hostRef.current, box = wrapper?.parentElement;
    if (!wrapper || !box) {
      camera.current = { zoom: 1, pan: { x: 0, y: 0 } };
      apply();
      return;
    }
    const contentW = Math.max(wrapper.scrollWidth, wrapper.offsetWidth, 1);
    const contentH = Math.max(wrapper.scrollHeight, wrapper.offsetHeight, 1);
    const z = Math.min(1, box.clientWidth / contentW, box.clientHeight / contentH) || 1;
    camera.current = { zoom: clamp(z, ZOOM_MIN, 1), pan: { x: 0, y: 0 } };
    apply();
  }, [apply]);

  // Wheel zoom at the pointer — only when the camera is active (canvas views).
  // Document views keep native scrolling.
  useEffect(() => {
    if (!active) return;
    const host = hostRef.current?.parentElement;
    if (!host) return;
    const onWheel = (e: WheelEvent) => {
      // Let the browser handle zoom chords (ctrl+wheel = browser zoom).
      if (e.ctrlKey || e.metaKey) return;
      const target = e.target as HTMLElement | null;
      if (target?.closest("input, textarea, select, [data-lens-ui]")) return;
      e.preventDefault();
      const rect = host.getBoundingClientRect();
      const delta = e.deltaY * (e.deltaMode === 1 ? 16 : e.deltaMode === 2 ? rect.height : 1);
      const factor = Math.exp(-clamp(delta, -240, 240) * 0.002);
      zoomAt(factor, e.clientX - rect.left, e.clientY - rect.top);
    };
    host.addEventListener("wheel", onWheel, { passive: false });
    return () => host.removeEventListener("wheel", onWheel);
  }, [active, zoomAt]);

  // Drag to pan: starts on the stage background (or anywhere with the pan
  // modifier), never on a button/link/bubble.
  const onPointerDown = useCallback(
    (e: React.PointerEvent) => {
      if (!active) return;
      if (e.button !== 0) return;
      const target = e.target as HTMLElement;
      if (target.closest("button, a, input, textarea, select, [role=button], [data-lens-ui], svg .map-node")) return;
      const host = hostRef.current?.parentElement;
      if (!host) return;
      drag.current = { sx: e.clientX, sy: e.clientY, px: camera.current.pan.x, py: camera.current.pan.y, moved: false };
      host.setPointerCapture?.(e.pointerId);
    },
    [active]
  );

  const onPointerMove = useCallback(
    (e: React.PointerEvent) => {
      const d = drag.current;
      if (!d) return;
      const dx = e.clientX - d.sx;
      const dy = e.clientY - d.sy;
      if (!d.moved && Math.hypot(dx, dy) > 4) d.moved = true;
      if (d.moved) {
        camera.current.pan = { x: d.px + dx, y: d.py + dy };
        apply();
      }
    },
    [apply]
  );

  // Keyboard parity: +/− zoom, 0 fits, arrows pan, when the stage has focus.
  const onKeyDown = useCallback(
    (e: React.KeyboardEvent) => {
      if (!active) return;
      const target = e.target as HTMLElement;
      if (target.closest("input, textarea, select")) return;
      const step = 48;
      if (e.key === "+" || e.key === "=") { e.preventDefault(); zoomBy(1.15); }
      else if (e.key === "-") { e.preventDefault(); zoomBy(1 / 1.15); }
      else if (e.key === "0") { e.preventDefault(); fit(); }
      else if (e.key === "ArrowLeft") { e.preventDefault(); panBy(step, 0); }
      else if (e.key === "ArrowRight") { e.preventDefault(); panBy(-step, 0); }
      else if (e.key === "ArrowUp") { e.preventDefault(); panBy(0, step); }
      else if (e.key === "ArrowDown") { e.preventDefault(); panBy(0, -step); }
    },
    [active, zoomBy, fit, panBy]
  );

  // A click that follows a real drag should not land on content (panning a
  // map must never pick a bubble by accident).
  const suppressClick = useRef(false);
  const onClickCapture = useCallback((e: React.MouseEvent) => {
    if (suppressClick.current) {
      e.preventDefault();
      e.stopPropagation();
      suppressClick.current = false;
    }
  }, []);

  const handleUp = useCallback(() => {
    if (drag.current?.moved) suppressClick.current = true;
    drag.current = null;
  }, []);

  // Callback ref for the transformed wrapper. The hook keeps its actual
  // ref private — callers only ever receive functions and state, so no
  // render path can read a ref value. (Deliberately not named "*Ref":
  // the hooks lint treats any such identifier as a ref.) On unmount, wipe
  // the inline transform — React may reuse this DOM node for another view,
  // and a stale zoom must never leak into it.
  const attach = useCallback((el: HTMLDivElement | null) => {
    const prev = (hostRef as { current: HTMLDivElement | null }).current;
    if (!el && prev) {
      prev.style.transform = "";
      delete prev.dataset.zoom;
    }
    (hostRef as { current: HTMLDivElement | null }).current = el;
  }, []);

  return {
    attach,
    zoom,
    zoomBy,
    panBy,
    fit,
    cameraProps: {
      onPointerDown,
      onPointerMove,
      onPointerUp: handleUp,
      onPointerLeave: handleUp,
      onKeyDown,
      onClickCapture,
    },
  };
}

/** The zoom toolbar: plain words, real buttons, keyboard-equivalent actions. */
export function CameraControls({
  zoom,
  onZoom,
  onFit,
}: {
  zoom: number;
  onZoom: (factor: number) => void;
  onFit: () => void;
}) {
  const pct = Math.round(zoom * 100);
  return (
    <div className="flex items-center gap-1" data-lens-ui>
      <button
        type="button"
        onClick={() => onZoom(1 / 1.15)}
        aria-label="Zoom out"
        title="Zoom out (−)"
        className="h-7 w-7 rounded-md border border-stone-200 bg-white text-stone-600 hover:border-emerald-300 hover:text-emerald-800 grid place-items-center"
      >
        <Minus className="h-3.5 w-3.5" />
      </button>
      <span className="text-[11px] tabular-nums text-stone-500 w-11 text-center" role="status" aria-label={`Zoom level ${pct} percent`}>
        {pct}%
      </span>
      <button
        type="button"
        onClick={() => onZoom(1.15)}
        aria-label="Zoom in"
        title="Zoom in (+)"
        className="h-7 w-7 rounded-md border border-stone-200 bg-white text-stone-600 hover:border-emerald-300 hover:text-emerald-800 grid place-items-center"
      >
        <Plus className="h-3.5 w-3.5" />
      </button>
      <button
        type="button"
        onClick={onFit}
        aria-label="Fit the whole picture"
        title="Back to 100% (0)"
        className="h-7 rounded-md border border-stone-200 bg-white text-stone-600 hover:border-emerald-300 hover:text-emerald-800 px-2 flex items-center gap-1 text-[11px]"
      >
        {zoom === 1 ? <Scan className="h-3 w-3" /> : <Maximize2 className="h-3 w-3" />} Fit
      </button>
    </div>
  );
}
