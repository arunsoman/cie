'use client'

// CIE — Your code, in plain words.
// A friendly front for the CIE engine: load the sample project, connect a
// public git repo (parsed in the background), or drop in your own engine
// exports (report.json / viewspec.json / concepts.json).

import { useEffect, useState } from "react";
import { Card, CardContent } from "@/components/ui/card";
import { Button } from "@/components/ui/button";
import { Badge } from "@/components/ui/badge";
import { Tabs, TabsList, TabsTrigger } from "@/components/ui/tabs";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogHeader,
  DialogTitle,
  DialogTrigger,
} from "@/components/ui/dialog";
import { useCie } from "@/lib/cie-store";
import {
  StartTab,
  MapTab,
  ConceptsTab,
  FindingsTab,
  PromisesTab,
  TrustTab,
  LoadingTab,
} from "@/components/cie/sections";
import { ChartsTab } from "@/components/cie/charts";
import { AskTab } from "@/components/cie/chat-tab";
import { UseYourOwnDialog } from "@/components/cie/use-your-own-dialog";
import { Upload, RotateCcw, Compass } from "lucide-react";

export default function Home() {
  const { source, label, loading, loadError, loadDemo } = useCie();
  const [tab, setTab] = useState("start");

  useEffect(() => { void loadDemo(); }, [loadDemo]);

  return (
    <div className="min-h-screen flex flex-col bg-stone-50">
      {/* Header */}
      <header className="sticky top-0 z-40 border-b border-stone-200 bg-white/90 backdrop-blur">
        <div className="max-w-6xl mx-auto px-4 sm:px-6 h-16 flex items-center justify-between gap-3">
          <div className="flex items-center gap-3 min-w-0">
            <div className="flex h-9 w-9 items-center justify-center rounded-xl bg-emerald-700 text-white shrink-0">
              <Compass className="h-5 w-5" />
            </div>
            <div className="min-w-0">
              <p className="font-bold text-stone-900 leading-tight">CIE</p>
              <p className="text-xs text-stone-500 truncate">Your code, in plain words</p>
            </div>
          </div>
          <div className="flex items-center gap-2">
            {source && (
              <Badge variant="outline" className={source === "demo" ? "bg-stone-100 text-stone-600 border-stone-200 hidden sm:inline-flex" : "bg-emerald-50 text-emerald-800 border-emerald-200 hidden sm:inline-flex"}>
                {source === "demo" ? "Sample project" : "Your project"}
              </Badge>
            )}
            <Dialog>
              <DialogTrigger asChild>
                <Button size="sm" className="bg-emerald-700 hover:bg-emerald-800 text-white">
                  <Upload className="h-4 w-4 mr-1.5" /> Use your own
                </Button>
              </DialogTrigger>
              <DialogContent className="sm:max-w-lg">
                <DialogHeader>
                  <DialogTitle>See your own project here</DialogTitle>
                  <DialogDescription>
                    Connect a public repo and CIE reads it in the background — or upload the files the
                    <span className="font-mono text-xs bg-stone-100 rounded px-1">cie</span> command creates. Either way, every page below switches to your code.
                  </DialogDescription>
                </DialogHeader>
                <UseYourOwnDialog />
              </DialogContent>
            </Dialog>
          </div>
        </div>
      </header>

      {/* Hero strip */}
      <section className="border-b border-stone-200 bg-white">
        <div className="max-w-6xl mx-auto px-4 sm:px-6 py-8">
          <h1 className="text-2xl sm:text-3xl font-bold text-stone-900 tracking-tight">
            See what your code really does — <span className="text-emerald-800">without reading a single line.</span>
          </h1>
          <p className="mt-2 text-stone-600 max-w-2xl leading-relaxed">
            {loading ? "Loading the sample project…" : label}. CIE read the whole project and turned it
            into maps, promises and friendly advice. No technical words needed.
          </p>
        </div>
      </section>

      {/* Body */}
      <main className="flex-1 w-full max-w-6xl mx-auto px-4 sm:px-6 py-6">
        {loadError ? (
          <Card className="border-rose-200">
            <CardContent className="p-8 text-center space-y-3">
              <p className="font-semibold text-rose-900">{loadError}</p>
              <Button variant="outline" onClick={() => void loadDemo()}>
                <RotateCcw className="h-4 w-4 mr-1.5" /> Try again
              </Button>
            </CardContent>
          </Card>
        ) : (
          <Tabs value={tab} onValueChange={setTab}>
            <TabsList className="mb-6 flex-wrap h-auto gap-1 bg-stone-100 p-1">
              <TabsTrigger value="start">Start here</TabsTrigger>
              <TabsTrigger value="map">Map</TabsTrigger>
              <TabsTrigger value="does">What it does</TabsTrigger>
              <TabsTrigger value="fixes">Things to look at</TabsTrigger>
              <TabsTrigger value="promises">Promises</TabsTrigger>
              <TabsTrigger value="numbers">Numbers</TabsTrigger>
              <TabsTrigger value="ask">Ask CIE</TabsTrigger>
              <TabsTrigger value="trust">Can I trust this?</TabsTrigger>
            </TabsList>
            {loading ? (
              <LoadingTab />
            ) : (
              <>
                <StartTab />
                <MapTab />
                <ConceptsTab />
                <FindingsTab />
                <PromisesTab />
                <ChartsTab />
                <AskTab />
                <TrustTab />
              </>
            )}
          </Tabs>
        )}
      </main>

      {/* Footer — sticks to bottom, pushed down naturally by content */}
      <footer className="mt-auto border-t border-stone-200 bg-white">
        <div className="max-w-6xl mx-auto px-4 sm:px-6 py-4 flex flex-col sm:flex-row items-center justify-between gap-2 text-xs text-stone-500">
          <p>CIE shows what it knows and how sure it is. It never hides a guess.</p>
          <p>Built on the CIE engine · everything on this page comes with proof.</p>
        </div>
      </footer>
    </div>
  );
}
