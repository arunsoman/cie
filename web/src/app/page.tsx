'use client'

// CIE — Your code, in plain words.
// A friendly front for the CIE engine: load the sample project or drop in
// your own engine exports (report.json / viewspec.json / concepts.json).

import { useEffect, useRef, useState } from "react";
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
import { useToast } from "@/hooks/use-toast";
import { useCie } from "@/lib/cie-store";
import type { CieReport, CieViewSpec, ConceptTree } from "@/lib/cie-types";
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
import { Upload, RotateCcw, Compass, FileUp, CheckCircle2 } from "lucide-react";

function isValidReport(o: unknown): o is CieReport {
  const r = o as CieReport;
  return !!r && Array.isArray(r.files) && Array.isArray(r.findings) && Array.isArray(r.symbols);
}
function isValidViewspec(o: unknown): o is CieViewSpec {
  const v = o as CieViewSpec;
  return !!v && Array.isArray(v.elements) && Array.isArray(v.edges);
}
function isValidConcepts(o: unknown): o is ConceptTree {
  const c = o as ConceptTree;
  return !!c && Array.isArray(c.nodes);
}

function UploadDialog() {
  const { setCustom, reset } = useCie();
  const { toast } = useToast();
  const reportRef = useRef<HTMLInputElement>(null);
  const vizRef = useRef<HTMLInputElement>(null);
  const conceptsRef = useRef<HTMLInputElement>(null);
  const [got, setGot] = useState({ report: false, viewspec: false, concepts: false });
  const [parsed, setParsed] = useState<{ report: CieReport | null; viewspec: CieViewSpec | null; concepts: ConceptTree | null; name: string }>({
    report: null, viewspec: null, concepts: null, name: "",
  });

  const read = (file: File, slot: "report" | "viewspec" | "concepts") => {
    const reader = new FileReader();
    reader.onload = () => {
      try {
        const obj = JSON.parse(String(reader.result));
        if (slot === "report" && isValidReport(obj)) {
          setParsed((p) => ({ ...p, report: obj, name: file.name }));
          setGot((g) => ({ ...g, report: true }));
        } else if (slot === "viewspec" && isValidViewspec(obj)) {
          setParsed((p) => ({ ...p, viewspec: obj, name: file.name }));
          setGot((g) => ({ ...g, viewspec: true }));
        } else if (slot === "concepts" && isValidConcepts(obj)) {
          setParsed((p) => ({ ...p, concepts: obj, name: file.name }));
          setGot((g) => ({ ...g, concepts: true }));
        } else {
          toast({ title: "That file doesn't look right", description: `Please choose the ${slot === "report" ? "report" : slot} file CIE created for you.`, variant: "destructive" });
        }
      } catch {
        toast({ title: "Couldn't read that file", description: "Please choose a file that ends in .json", variant: "destructive" });
      }
    };
    reader.readAsText(file);
  };

  const apply = () => {
    if (!parsed.report && !parsed.viewspec) {
      toast({ title: "Almost there", description: "Please add at least the report file (report.json).", variant: "destructive" });
      return;
    }
    setCustom(parsed.report, parsed.viewspec, parsed.concepts, parsed.name ? `Your data: ${parsed.name}` : "Your data");
    toast({ title: "Your data is loaded", description: "Everything below now shows your project." });
  };

  const slot = (label: string, required: boolean, filled: boolean, ref: React.RefObject<HTMLInputElement | null>, key: "report" | "viewspec" | "concepts") => (
    <button
      type="button"
      onClick={() => ref.current?.click()}
      className={`w-full text-left rounded-lg border p-4 flex items-center gap-3 transition-colors ${filled ? "border-emerald-300 bg-emerald-50" : "border-stone-200 bg-white hover:border-stone-300"}`}
    >
      {filled ? <CheckCircle2 className="h-5 w-5 text-emerald-700 shrink-0" /> : <FileUp className="h-5 w-5 text-stone-400 shrink-0" />}
      <span className="flex-1">
        <span className="block text-sm font-medium text-stone-900">{label} {required && <span className="text-rose-600">*</span>}</span>
        <span className="block text-xs text-stone-500">{filled ? "Ready" : "Tap to choose a file"}</span>
      </span>
      <input ref={ref} type="file" accept=".json,application/json" className="hidden" onChange={(e) => { const f = e.target.files?.[0]; if (f) read(f, key); }} />
    </button>
  );

  return (
    <DialogContent className="sm:max-w-md">
      <DialogHeader>
        <DialogTitle>See your own project here</DialogTitle>
        <DialogDescription>
          Run these two commands in your project folder, then add the files they create:
          <span className="block mt-2 rounded-md bg-stone-900 text-stone-100 font-mono text-xs px-3 py-2 leading-relaxed">
            cie report . --json &gt; report.json<br />
            cie viz . --out viewspec.json
          </span>
        </DialogDescription>
      </DialogHeader>
      <div className="space-y-2.5">
        {slot("1. The health report", true, got.report, reportRef, "report")}
        {slot("2. The picture/map file", false, got.viewspec, vizRef, "viewspec")}
        {slot("3. The 'what it does' list", false, got.concepts, conceptsRef, "concepts")}
        <div className="flex gap-2 pt-1">
          <Button onClick={apply} className="flex-1 bg-emerald-700 hover:bg-emerald-800 text-white">Show my project</Button>
          <Button variant="outline" onClick={() => { reset(); toast({ title: "Back to the sample project" }); }}>Use sample</Button>
        </div>
      </div>
    </DialogContent>
  );
}

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
              <UploadDialog />
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
