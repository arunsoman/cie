"use client";

// The hover-expansion fisheye lens over the Map — a faithful port of the
// bounded radial lens from github.com/arunsoman/code-intelligence
// (apps/web/src/fisheye.ts), adapted from a G6 scene model to DOM-sampled
// map bubbles.
//
// What carries over, unchanged in spirit:
//   — bounded radial projection: everything inside the lens magnifies toward
//     the centre; the boundary and everything outside it stay FIXED
//     (projectLens: scale = R·m / (R + (m−1)·d));
//   — automatic sizing: ~35% of the shorter side, capped, inner ring at 88%;
//   — a 350 ms hover decides "you meant it", with exponential easing toward
//     the pointer; leaving the lens (with a corridor back to the source)
//     dismisses it;
//   — the base map never mutates: the lens paints on its own transparent
//     canvas overlay; source geometry, labels and layout are untouched
//     (US-023: local magnification, base geometry fixed);
//   — rings, a Pause/Resume control, and keyboard parity: L pauses or
//     resumes, Escape dismisses (US-140);
//   — state is published as data-attributes so tests — and honesty checks —
//     can see when the lens is on.

import { useEffect, useRef, useState, type RefObject } from "react";
import { MoveVertical } from "lucide-react";

export interface Point { x: number; y: number }

const clamp = (n: number, lo: number, hi: number) => Math.max(lo, Math.min(hi, n));

/** Bounded radial lens: the boundary and everything outside it stay fixed. */
export function projectLens(point: Point, centre: Point, magnification: number, radius: number): Point {
  const dx = point.x - centre.x, dy = point.y - centre.y, distance = Math.hypot(dx, dy);
  if (radius <= 0 || distance >= radius || magnification === 1) return { ...point };
  const m = clamp(magnification, 0.35, 4);
  const scale = (radius * m) / (radius + (m - 1) * distance);
  return { x: centre.x + dx * scale, y: centre.y + dy * scale };
}

/** Automatic sizing: 35% of the shorter side, capped at 260px radius. */
export function automaticLensGeometry(width: number, height: number) {
  const short = Math.max(0, Math.min(width, height));
  const outer = Math.max(0, Math.min(short * 0.35, 260, (short - 16) / 2));
  return { outer, inner: outer * 0.88 };
}

export function boundedLensCentre(point: Point, width: number, height: number, radius: number): Point {
  return {
    x: clamp(point.x, radius + 8, Math.max(radius + 8, width - radius - 8)),
    y: clamp(point.y, radius + 8, Math.max(radius + 8, height - radius - 8)),
  };
}

// The map's own certainty palette — same colours as the SVG below the lens.
const MODE_STYLE: Record<string, { stroke: string; fill: string; text: string; dash?: number[] }> = {
  Fact: { stroke: "#059669", fill: "#ecfdf5", text: "#065f46" },
  Inference: { stroke: "#d97706", fill: "#fffbeb", text: "#78350f", dash: [5, 3] },
  Hypothesis: { stroke: "#78716c", fill: "#f5f5f4", text: "#44403c", dash: [2, 3] },
};

interface LensNode {
  id: string;
  label: string;
  kind: string;
  mode: string;
  file: string;
  x: number; y: number; w: number; h: number;
}

const contains = (n: LensNode, p: Point) =>
  Math.abs(p.x - n.x) <= n.w / 2 && Math.abs(p.y - n.y) <= n.h / 2;

const MAG = 2.2;              // peak magnification at the lens centre
const HOVER_MS = 350;         // dwell before the lens opens (you meant it)

export function MapFisheye({ hostRef, enabled = true }: { hostRef: RefObject<HTMLDivElement | null>; enabled?: boolean }) {
  const pausedRef = useRef(false);
  const [paused, setPaused] = useState(false);
  const wrapRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    const hostEl = hostRef.current;
    if (!hostEl) return;
    const host: HTMLDivElement = hostEl;

    const canvas = document.createElement("canvas");
    canvas.className = "fisheye-surface";
    canvas.setAttribute("aria-hidden", "true");
    canvas.style.position = "absolute";
    canvas.style.inset = "0";
    canvas.style.width = "100%";
    canvas.style.height = "100%";
    canvas.style.zIndex = "20";
    canvas.style.pointerEvents = "none";
    host.append(canvas);
    const ctx = canvas.getContext("2d")!;

    const state = { easing: !window.matchMedia("(prefers-reduced-motion: reduce)").matches };
    let target: Point = { x: 0, y: 0 }, centre = { ...target }, anchor = { ...target };
    let captured: LensNode | null = null;
    let active = false, currentM = MAG, raf = 0, last = 0, disposed = false;
    let hoverTimer: ReturnType<typeof setTimeout> | undefined, hoverId = "";

    const geometry = () => automaticLensGeometry(host.clientWidth, host.clientHeight);
    const position = (e: PointerEvent | MouseEvent): Point => {
      const b = host.getBoundingClientRect();
      return { x: e.clientX - b.left, y: e.clientY - b.top };
    };

    const scene = (): LensNode[] => {
      const bounds = host.getBoundingClientRect();
      return [...host.querySelectorAll<HTMLElement | SVGElement>(".map-node")].map((el) => {
        const box = el.getBoundingClientRect();
        const mode = el.getAttribute("data-mode") ?? "Hypothesis";
        return {
          id: el.getAttribute("data-node-id") ?? "",
          label: el.getAttribute("data-label") ?? "",
          kind: el.getAttribute("data-kind") ?? "",
          mode,
          file: el.getAttribute("data-file") ?? "",
          x: box.left - bounds.left + box.width / 2,
          y: box.top - bounds.top + box.height / 2,
          w: box.width, h: box.height,
        };
      });
    };

    const publish = () => {
      host.dataset.lensActive = String(active && !pausedRef.current);
      host.dataset.lensNode = captured?.id ?? "";
      host.dataset.lensX = String(Math.round(centre.x));
      host.dataset.lensY = String(Math.round(centre.y));
      host.dataset.lensMagnification = String(currentM.toFixed(2));
    };

    const schedule = () => { if (!raf && !disposed) raf = requestAnimationFrame(paint); };

    function paint(now: number) {
      raf = 0;
      const dt = Math.min(0.05, (now - (last || now - 16)) / 1000); last = now;
      const k = state.easing ? 1 - Math.exp(-dt * 16) : 1;
      centre.x += (target.x - centre.x) * k;
      centre.y += (target.y - centre.y) * k;
      currentM += (MAG - currentM) * k;

      const w = host.clientWidth, h = host.clientHeight, dpr = window.devicePixelRatio || 1;
      const geo = geometry();
      const radius = geo.outer, inner = geo.inner;
      target = boundedLensCentre(target, w, h, radius);
      if (!state.easing) centre = { ...target };
      if (canvas.width !== Math.round(w * dpr) || canvas.height !== Math.round(h * dpr)) {
        canvas.width = Math.round(w * dpr); canvas.height = Math.round(h * dpr);
      }
      ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
      ctx.clearRect(0, 0, w, h);

      if (!active || pausedRef.current || !captured || radius <= 0) { publish(); return; }

      // Project every bubble through the bounded lens — the boundary ring and
      // everything beyond it never move.
      const projected = scene().map((n) => {
        const p = projectLens({ x: n.x, y: n.y }, centre, currentM, inner);
        const s = projectLens({ x: n.x + n.w / 2, y: n.y }, centre, currentM, inner);
        const halfW = Math.max(4, Math.abs(s.x - p.x));
        const scale = halfW / Math.max(1, n.w / 2);
        return { n, x: p.x, y: p.y, w: n.w * scale, h: n.h * scale };
      });

      ctx.save();
      ctx.beginPath();
      ctx.arc(centre.x, centre.y, inner, 0, Math.PI * 2);
      ctx.clip();
      ctx.fillStyle = "#fffffffb";
      ctx.fillRect(centre.x - inner, centre.y - inner, inner * 2, inner * 2);

      // Bubbles near enough to matter, drawn small→large so the focus wins.
      const visible = projected
        .filter((p) => Math.hypot(p.x - centre.x, p.y - centre.y) < inner + Math.max(p.w, p.h) / 2 + 40)
        .sort((a, b) => a.w * a.h - b.w * b.h);
      for (const p of visible) {
        if (p.n.id === captured!.id) continue;
        drawBubble(p.n, p.x, p.y, p.w, p.h, Math.max(8, 10 * (p.w / Math.max(p.n.w, 1))), false);
      }
      // The captured bubble, enlarged and glowing, with its plain-words card.
      const cap = projected.find((p) => p.n.id === captured!.id)!;
      drawBubble(cap.n, cap.x, cap.y, Math.max(cap.w, inner * 0.72), Math.max(cap.h, 52), 15, true);
      const lines = [
        cap.n.kind,
        cap.n.mode === "Fact" ? "Proven — checked directly" : cap.n.mode === "Inference" ? "Good guess — CIE worked it out" : "Not sure yet",
        cap.n.file ? `in ${cap.n.file}` : "",
      ].filter(Boolean);
      ctx.font = "11px system-ui, sans-serif";
      ctx.textAlign = "center";
      let ly = cap.y + cap.h / 2 + 18;
      for (const line of lines) {
        if (ly > centre.y + inner - 12) break;
        ctx.fillStyle = "#78716c";
        ctx.fillText(line, centre.x, ly, inner * 1.5);
        ly += 15;
      }

      ctx.fillStyle = "#57534e";
      ctx.font = "600 11px system-ui, sans-serif";
      ctx.fillText(`Focused: ${captured!.label}`, centre.x, centre.y - inner + 18, inner * 1.6);
      ctx.restore();

      // Rings — solid boundary at the outer radius, dashed at the working radius.
      ctx.strokeStyle = "rgba(5,150,105,.35)";
      ctx.lineWidth = 1;
      ctx.beginPath(); ctx.arc(centre.x, centre.y, radius, 0, Math.PI * 2); ctx.stroke();
      ctx.setLineDash([6, 7]);
      ctx.strokeStyle = "rgba(120,113,108,.45)";
      ctx.beginPath(); ctx.arc(centre.x, centre.y, inner, 0, Math.PI * 2); ctx.stroke();
      ctx.setLineDash([]);

      publish();
      if (state.easing && (Math.hypot(target.x - centre.x, target.y - centre.y) > 0.2 || Math.abs(currentM - MAG) > 0.005)) schedule();
    }

    function drawBubble(n: LensNode, x: number, y: number, w: number, h: number, fontPx: number, focused: boolean) {
      const s = MODE_STYLE[n.mode] ?? MODE_STYLE.Hypothesis;
      ctx.save();
      if (focused) { ctx.shadowColor = s.stroke; ctx.shadowBlur = 16; }
      ctx.beginPath();
      ctx.roundRect(x - w / 2, y - h / 2, w, h, Math.min(12, h / 2));
      ctx.fillStyle = s.fill;
      ctx.fill();
      ctx.shadowBlur = 0;
      ctx.strokeStyle = s.stroke;
      ctx.lineWidth = focused ? 2.5 : 1.4;
      if (s.dash) ctx.setLineDash(s.dash);
      ctx.stroke();
      ctx.setLineDash([]);
      // The label — never colour alone: the text always rides with the shape.
      ctx.fillStyle = s.text;
      ctx.textAlign = "center";
      ctx.textBaseline = "middle";
      ctx.font = `600 ${fontPx}px system-ui, sans-serif`;
      let label = n.label;
      const maxW = w - 14;
      while (label.length && ctx.measureText(label + "…").width > maxW) label = label.slice(0, -1);
      if (label !== n.label) label += "…";
      ctx.fillText(label, x, y, maxW);
      ctx.restore();
    }

    const ignored = (e: Event) =>
      !!(e.target as Element)?.closest?.("[data-lens-ui], input, textarea, select, button, a");

    const hit = (p: Point): LensNode | undefined => {
      const nodes = scene().filter((n) => !ignoredShape(n) && contains(n, p));
      return nodes.sort((a, b) => a.w * a.h - b.w * b.h)[0];
    };
    const ignoredShape = (n: LensNode) => n.w <= 0 || n.h <= 0;

    const clearHover = () => { clearTimeout(hoverTimer); hoverTimer = undefined; hoverId = ""; };

    const activate = (point: Point, node: LensNode) => {
      clearHover();
      const geo = geometry();
      captured = node;
      anchor = { ...point };
      target = boundedLensCentre(point, host.clientWidth, host.clientHeight, geo.outer);
      if (!active) centre = { ...target };
      active = true;
      schedule();
    };

    const move = (e: PointerEvent) => {
      if (ignored(e) || pausedRef.current) return;
      const mouse = position(e);
      if (active) {
        // Keep a small corridor from the source bubble to its lens, so tiny
        // pointer wobble doesn't flicker the lens closed.
        const dx = centre.x - anchor.x, dy = centre.y - anchor.y, len = dx * dx + dy * dy;
        const t = len ? clamp(((mouse.x - anchor.x) * dx + (mouse.y - anchor.y) * dy) / len, 0, 1) : 0;
        const corridor = Math.hypot(mouse.x - anchor.x - t * dx, mouse.y - anchor.y - t * dy) <= 20;
        if (Math.hypot(mouse.x - centre.x, mouse.y - centre.y) <= geometry().inner || corridor) return;
        active = false;
        captured = null;
        schedule();
      }
      const candidate = hit(mouse);
      if (!candidate) { clearHover(); return; }
      if (candidate.id === hoverId) return;
      clearHover();
      hoverId = candidate.id;
      hoverTimer = setTimeout(() => { if (!disposed) activate(mouse, candidate); }, HOVER_MS);
    };

    const leave = () => { clearHover(); if (!pausedRef.current) { active = false; captured = null; schedule(); } };

    // While the lens is open the wheel belongs to the lens, not the camera —
    // otherwise zooming under an open preview would move the floor.
    const wheelBlock = (e: WheelEvent) => {
      if (!active || pausedRef.current) return;
      e.preventDefault();
      e.stopImmediatePropagation();
    };

    const key = (e: KeyboardEvent) => {
      if ((e.target as HTMLElement)?.closest?.("input, textarea, select")) return;
      if (e.key.toLowerCase() === "l") {
        e.preventDefault(); e.stopImmediatePropagation();
        clearHover();
        pausedRef.current = !pausedRef.current;
        if (pausedRef.current) { active = false; captured = null; }
        setPaused(pausedRef.current);
        schedule();
      } else if (e.key === "Escape" && active) {
        e.preventDefault(); e.stopImmediatePropagation();
        clearHover();
        active = false; captured = null;
        schedule();
      }
    };

    host.addEventListener("pointermove", move);
    host.addEventListener("pointerleave", leave);
    host.addEventListener("wheel", wheelBlock, { capture: true, passive: false });
    window.addEventListener("keydown", key, true);

    const observer = new ResizeObserver(() => {
      const geo = geometry();
      target = boundedLensCentre(target, host.clientWidth, host.clientHeight, geo.outer);
      schedule();
    });
    observer.observe(host);

    return () => {
      disposed = true;
      clearHover();
      cancelAnimationFrame(raf);
      observer.disconnect();
      host.removeEventListener("pointermove", move);
      host.removeEventListener("pointerleave", leave);
      host.removeEventListener("wheel", wheelBlock, { capture: true } as EventListenerOptions);
      window.removeEventListener("keydown", key, true);
      canvas.remove();
      delete host.dataset.lensActive;
      delete host.dataset.lensNode;
    };
  }, [hostRef]);

  const setPausedNow = (v: boolean) => {
    pausedRef.current = v;
    setPaused(v);
  };

  return (
    <div
      className="absolute bottom-3 right-3 z-30 flex flex-col items-end gap-1 rounded-xl border border-stone-200 bg-white/90 backdrop-blur px-2.5 py-1.5 shadow-sm"
      role="group"
      aria-label="Fisheye lens"
      data-lens-ui
      ref={wrapRef}
    >
      <button
        type="button"
        aria-pressed={paused}
        onClick={() => setPausedNow(!paused)}
        className="text-[11px] font-medium text-stone-700 hover:text-emerald-800 whitespace-nowrap"
      >
        {paused ? "Resume hover expansion" : "Pause hover expansion"}
      </button>
      <span className="text-[10px] text-stone-400 flex items-center gap-1 whitespace-nowrap">
        <MoveVertical className="h-3 w-3" /> hover a bubble · L pause · Esc close
      </span>
    </div>
  );
}
