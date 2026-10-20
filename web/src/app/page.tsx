'use client'

// CIE — Your code, in plain words.
//
// The workspace: a topbar that carries the model picker and the main menus,
// the chart engine in the middle, and the Ask-CIE chat docked on the right —
// the layout made popular by the code-intelligence UI, speaking CIE's
// plain-words language all the way through.
//
//   ┌──────────────────────────────────────────────────────────┐
//   │ topbar: [CIE] [who answers ▾] [project]   Explore Charts Trust ⚙ │
//   ├───────────────────────────────────────────┬──────────────┤
//   │ stage: the view (map / charts / stories)  │ Ask CIE      │
//   │                                           │ chat, with   │
//   │                                           │ charts that  │
//   │                                           │ go big on    │
//   │                                           │ the canvas   │
//   └───────────────────────────────────────────┴──────────────┘

import { useEffect } from "react";
import { readLastWorkspace, restoreJobResult, forgetWorkspace } from "@/lib/workspace-memory";
import { Card, CardContent } from "@/components/ui/card";
import { Button } from "@/components/ui/button";
import { RotateCcw, PanelRightClose, PanelRightOpen } from "lucide-react";
import { useCie } from "@/lib/cie-store";
import { Topbar } from "@/components/cie/topbar";
import { Stage } from "@/components/cie/stage";
import { ChatPanel } from "@/components/cie/chat-panel";
import { SourcePanel } from "@/components/cie/source-panel";

export default function Home() {
  // Ask CIE is collapsible so the chart stage can take the full width on
  // desktop; the choice is remembered per browser like the model settings
  // are (default open — the server render shows it open, same as always).
  const { loading, loadError, loadDemo, hydrateLlm, chatOpen, setChatOpen, hydrateChatOpen } = useCie();

  // Launch with the LAST OPENED WORKSPACE, not the default sample: the
  // browser records each open (workspace-memory) and this effect reopens it
  // — samples directly, connect-a-repo workspaces through their persisted
  // result. When the record can't be honored (a missing job, a wiped jobs
  // root, a first visit) the honest default is the sample project, as always.
  useEffect(() => {
    void (async () => {
      const last = readLastWorkspace();
      if (last && last.kind === "job") {
        const restored = await restoreJobResult(last.jobId).catch(() => false);
        if (restored) return;
      }
      if (last && last.kind === "sample") {
        const ok = await loadDemo(last.demo).catch(() => null);
        if (ok !== null) return;
      }
      if (last) forgetWorkspace();
      void loadDemo();
    })();
  }, [loadDemo]);
  useEffect(() => { hydrateLlm(); }, [hydrateLlm]);
  useEffect(() => { hydrateChatOpen(); }, [hydrateChatOpen]);
  const toggleChat = () => setChatOpen(!chatOpen);

  return (
    <div className="lg:h-screen lg:flex lg:flex-col lg:bg-stone-100 lg:overflow-hidden bg-stone-100">
      <Topbar />

      {loadError ? (
        <main className="flex-1 grid place-items-center p-6">
          <Card className="border-rose-200 max-w-md w-full">
            <CardContent className="p-8 text-center space-y-3">
              <p className="font-semibold text-rose-900">{loadError}</p>
              <Button variant="outline" onClick={() => void loadDemo()}>
                <RotateCcw className="h-4 w-4 mr-1.5" /> Try again
              </Button>
            </CardContent>
          </Card>
        </main>
      ) : (
        <div className="flex-1 min-h-0 flex flex-col lg:flex-row lg:relative">
          {/* the chart engine, in the middle — 70vh on phones, everything on
              desktop; full width on desktop whenever Ask CIE is collapsed,
              so a chart-heavy view like Atlas can use the whole screen. */}
          <div className="h-[70vh] shrink-0 lg:h-auto lg:flex-1 flex flex-col min-h-0 min-w-0">
            <Stage />
          </div>

          {/* Ask CIE, docked on the right — collapsible on desktop */}
          {chatOpen ? (
            <aside className="lg:w-[340px] xl:w-[380px] lg:shrink-0 border-t lg:border-t-0 lg:border-l border-stone-200 bg-white min-h-[420px] lg:min-h-0 lg:h-full flex flex-col lg:relative">
              <button
                type="button"
                onClick={toggleChat}
                className="hidden lg:flex absolute -left-3 top-1/2 -translate-y-1/2 h-9 w-6 items-center justify-center rounded-l-md border border-r-0 border-stone-200 bg-white text-stone-400 hover:text-stone-700 hover:bg-stone-50 shadow-sm z-10"
                aria-label="Hide Ask CIE — give the chart stage the full width"
                title="Hide Ask CIE"
              >
                <PanelRightClose className="h-3.5 w-3.5" aria-hidden />
              </button>
              <ChatPanel />
            </aside>
          ) : (
            <button
              type="button"
              onClick={toggleChat}
              className="hidden lg:flex absolute right-0 top-1/2 -translate-y-1/2 h-9 w-6 items-center justify-center rounded-l-md border border-r-0 border-stone-200 bg-white text-stone-400 hover:text-stone-700 hover:bg-stone-50 shadow-sm z-10"
              aria-label="Show Ask CIE"
              title="Show Ask CIE"
            >
              <PanelRightOpen className="h-3.5 w-3.5" aria-hidden />
            </button>
          )}
        </div>
      )}

      {/* click-a-chart-element → the code it came from (overlay panel) */}
      <SourcePanel />
    </div>
  );
}
