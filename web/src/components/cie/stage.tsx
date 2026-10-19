"use client";

// The middle of the workspace: the chart rendering engine.
//
// Shape borrowed from code-intelligence's main column — a stage that owns the
// canvas with real camera controls, a toolbar that says what you're looking
// at, and a footer that keeps CIE's honesty notes on the canvas itself:
//
//   ┌ toolbar: [view title + caption] ……… [− 100% + Fit] ┐
//   │ stage:  document views scroll normally;             │
//   │         canvas views (map, chat charts) get the     │
//   │         camera: wheel zoom at the pointer, drag to  │
//   │         pan, keyboard parity, fisheye lens on the   │
//   │         map (hover 350 ms, L pause, Esc close)      │
//   └ footer: what the picture knows, and how sure it is ┘

import { useEffect, useRef } from "react";
import { Card, CardContent } from "@/components/ui/card";
import { Map as MapIcon, Maximize2, Sparkles } from "lucide-react";
import { useCie, type StageView } from "@/lib/cie-store";
import { CameraControls, useCamera } from "@/components/cie/camera-stage";
import { MapFisheye } from "@/components/cie/map-fisheye";
import { ChatChart } from "@/components/cie/chat-chart";
import { certaintyPlain } from "@/lib/plain-words";
import {
  LoadingView,
  StartView,
  MapViewSection,
  MapInfoCard,
  ConceptsView,
  FindingsView,
  PromisesView,
  TrustView,
} from "@/components/cie/sections";
import { DepsView, RuntimeView } from "@/components/cie/runtime-views";
import { NumbersView } from "@/components/cie/charts";
import { MenuView } from "@/components/cie/menu-tab";
import { QualityView } from "@/components/cie/quality-view";

const CANVAS_VIEWS: StageView[] = ["map", "chart"];

const TITLES: Record<StageView, { title: string; caption: string }> = {
  start: { title: "Start here", caption: "What CIE does with your code, in three steps." },
  map: { title: "The map of your code", caption: "Bubbles are parts of your code; lines show how they relate." },
  does: { title: "What it does", caption: "Each code file, in plain words." },
  fixes: { title: "Things to look at", caption: "Sorted by urgency — red first." },
  promises: { title: "Promises", caption: "Things the code must always do." },
  numbers: { title: "Your project in pictures", caption: "Same facts as the other views, drawn as charts." },
  menu: { title: "One-tap chart menu", caption: "Charts with zero typing — health, quality metrics, and all 33 stories with real data." },
  trust: { title: "Can I trust this?", caption: "What CIE knows, what it guessed, what it couldn't check." },
  deps: { title: "Dependencies", caption: "What your project declares it needs — names and versions as written." },
  runtime: { title: "Watching it run", caption: "Measured runtime evidence — every number names the tool that measured it." },
  quality: { title: "Quality metrics", caption: "The eight classic measures, computed by the ETL collector — formulas and sources shown." },
  chart: { title: "Chart", caption: "Drawn from your loaded report." },
};

export function Stage() {
  const { view, loading, viewspec, canvasChart } = useCie();
  const canvas = CANVAS_VIEWS.includes(view);
  // Destructured on purpose: the hooks lint treats member access on a
  // hook-returned object as ref access during render.
  const { attach, zoom, zoomBy, fit, cameraProps } = useCamera(canvas);
  // The clip box the lens samples from — the same element the camera props
  // sit on, so the lens and the camera agree on screen coordinates.
  const clipRef = useRef<HTMLDivElement>(null);

  // A fresh canvas view starts fitted — the whole picture is visible first,
  // then the visitor decides where to look closer. (camera.fit is stable.)
  useEffect(() => {
    if (canvas) {
      const id = requestAnimationFrame(() => fit());
      return () => cancelAnimationFrame(id);
    }
  }, [view, canvas, fit]);

  const meta = TITLES[view];

  return (
    <main className="flex-1 min-w-0 min-h-0 flex flex-col bg-white" aria-label="Chart workspace">
      {/* toolbar — what you're looking at, and the camera controls */}
      <div className="shrink-0 h-11 px-3 sm:px-4 flex items-center gap-3 border-b border-stone-200 bg-white">
        <h1 className="text-sm font-semibold text-stone-900 truncate flex items-center gap-2 min-w-0">
          {view === "map" ? <MapIcon className="h-4 w-4 text-emerald-700 shrink-0" /> : view === "chart" ? <Maximize2 className="h-4 w-4 text-emerald-700 shrink-0" /> : <Sparkles className="h-4 w-4 text-emerald-700 shrink-0" />}
          <span className="truncate">{meta.title}</span>
        </h1>
        <p className="hidden md:block text-xs text-stone-500 truncate">{meta.caption}</p>
        <div className="flex-1" />
        {canvas && <CameraControls zoom={zoom} onZoom={zoomBy} onFit={fit} />}
      </div>

      {/* the stage itself */}
      {loading ? (
        <div className="flex-1 min-h-0 overflow-y-auto p-4 sm:p-6">
          <LoadingView />
        </div>
      ) : canvas ? (
        <div
          key="canvas-stage"
          ref={clipRef}
          className="relative flex-1 min-h-0 overflow-hidden select-none bg-stone-50 outline-none focus-visible:ring-2 focus-visible:ring-inset focus-visible:ring-emerald-600/30"
          style={{
            backgroundImage: "radial-gradient(rgba(120,113,108,.14) 1px, transparent 1px)",
            backgroundSize: "22px 22px",
          }}
          tabIndex={0}
          role="application"
          aria-label={`${meta.title}. Scroll zooms, drag pans, arrow keys pan, plus and minus zoom, zero fits.`}
          {...cameraProps}
        >
          {/* the camera wrapper — a pure transform; content geometry never changes */}
          <div
            ref={attach}
            className="absolute top-0 left-0 w-full origin-top-left will-change-transform"
          >
            {view === "map" ? (
              <MapViewSection />
            ) : canvasChart ? (
              <ChartOnCanvas />
            ) : (
              <div className="p-10 text-center text-sm text-stone-400">
                No chart on the canvas right now — ask CIE something and its chart opens here automatically.
              </div>
            )}
          </div>

          {view === "map" && <MapFisheye hostRef={clipRef} />}

          {/* the picked bubble's facts — a fixed-size overlay, not zoomed */}
          {view === "map" && (
            <div className="absolute top-3 right-3 z-30" data-lens-ui>
              <MapInfoCard />
            </div>
          )}
        </div>
      ) : (
        <div key="doc-stage" className="flex-1 min-h-0 overflow-y-auto" data-view={view}>
          <div className="p-4 sm:p-6 max-w-6xl mx-auto">
            {view === "start" && <StartView />}
            {view === "does" && <ConceptsView />}
            {view === "fixes" && <FindingsView />}
            {view === "promises" && <PromisesView />}
            {view === "numbers" && <NumbersView />}
            {view === "menu" && <MenuView />}
            {view === "trust" && <TrustView />}
            {view === "deps" && <DepsView />}
            {view === "runtime" && <RuntimeView />}
            {view === "quality" && <QualityView />}
          </div>
        </div>
      )}

      {/* honesty footer — lives on the canvas, like the reference's epistemic footer */}
      {canvas && !loading && (
        <footer className="shrink-0 border-t border-stone-200 bg-white px-3 sm:px-4 py-2 flex flex-wrap items-center gap-x-4 gap-y-1 text-[11px] text-stone-500">
          {view === "map" && viewspec?.footer && (
            <span className="inline-flex items-center gap-1.5 flex-wrap">
              <span className="font-medium text-stone-600">How sure:</span>
              <span>{viewspec.footer.facts} {certaintyPlain("Fact").label.toLowerCase()}</span>
              <span aria-hidden>·</span>
              <span>{viewspec.footer.inferences} {certaintyPlain("Inference").label.toLowerCase()}</span>
              <span aria-hidden>·</span>
              <span>{viewspec.footer.hypotheses} {certaintyPlain("Hypothesis").label.toLowerCase()}</span>
              {viewspec.footer.caption && <span className="text-stone-400">· “{viewspec.footer.caption}”</span>}
            </span>
          )}
          {view === "chart" && canvasChart && (
            <span className="text-stone-400 truncate">
              {canvasChart.catalog ? `catalog ${canvasChart.catalog} · ` : ""}drawn from your loaded report — the chat keeps the small version.
            </span>
          )}
          <span className="flex-1" />
          <span className="text-stone-400">
            {view === "map"
              ? "Scroll to zoom · drag to pan · hover a bubble for the lens · tap a bubble to ask about it"
              : "Scroll to zoom · drag to pan"}
          </span>
        </footer>
      )}
    </main>
  );
}

/** A chart the chat sent to the canvas — the big, quiet version. */
function ChartOnCanvas() {
  const { canvasChart, closeCanvasChart } = useCie();
  if (!canvasChart) return null;
  return (
    <div className="p-4 sm:p-6 w-full">
      <Card className="border-stone-200 shadow-sm w-full">
        <CardContent className="p-5 w-full">
          <ChatChart spec={canvasChart} />
        </CardContent>
      </Card>
      <button
        type="button"
        onClick={closeCanvasChart}
        className="mt-3 text-xs text-stone-500 hover:text-stone-800 underline"
      >
        Close this chart and go back to Start here
      </button>
    </div>
  );
}
