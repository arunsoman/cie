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
import { Card, CardContent } from "@/components/ui/card";
import { Button } from "@/components/ui/button";
import { RotateCcw } from "lucide-react";
import { useCie } from "@/lib/cie-store";
import { Topbar } from "@/components/cie/topbar";
import { Stage } from "@/components/cie/stage";
import { ChatPanel } from "@/components/cie/chat-panel";

export default function Home() {
  const { loading, loadError, loadDemo, hydrateLlm } = useCie();

  // The sample project loads on arrival; the model settings hydrate from
  // localStorage after mount (the server render shows honest placeholders).
  useEffect(() => { void loadDemo(); }, [loadDemo]);
  useEffect(() => { hydrateLlm(); }, [hydrateLlm]);

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
        <div className="flex-1 min-h-0 flex flex-col lg:flex-row">
          {/* the chart engine, in the middle — 70vh on phones, everything on desktop */}
          <div className="h-[70vh] shrink-0 lg:h-auto lg:flex-1 flex flex-col min-h-0">
            <Stage />
          </div>

          {/* Ask CIE, docked on the right */}
          <aside className="lg:w-[340px] xl:w-[380px] lg:shrink-0 border-t lg:border-t-0 lg:border-l border-stone-200 bg-white min-h-[420px] lg:min-h-0 lg:h-full flex flex-col">
            <ChatPanel />
          </aside>
        </div>
      )}
    </div>
  );
}
