"use client";

// The workspace topbar — model picker on the left, main menus on the right,
// exactly the shape that code-intelligence made work well:
//   [brand] [model chip ▾] [repo chip] ……… [Explore ▾] [Charts ▾] [Trust ▾] [⚙]
//
// The model chip is the status chip made clickable: it always names who is
// answering (built-in AI, Ollama, or an OpenAI-compatible service), and one
// click opens the full connection settings. No hidden defaults, no restarts.

import { useState } from "react";
import { Button } from "@/components/ui/button";
import { Badge } from "@/components/ui/badge";
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuLabel,
  DropdownMenuSeparator,
  DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import {
  BrainCog,
  ChevronDown,
  Compass,
  ListChecks,
  Map as MapIcon,
  Menu as MenuIcon,
  Package,
  PieChart,
  Settings2,
  ShieldCheck,
  Sparkles,
  Timer,
  Upload,
  Wand2,
  LayoutGrid,
} from "lucide-react";
import { useCie, type StageView } from "@/lib/cie-store";
import { providerLabel } from "@/lib/llm-config";
import { UseYourOwnDialog } from "@/components/cie/use-your-own-dialog";
import { LlmSettingsDialog } from "@/components/cie/llm-settings";

const EXPLORE_VIEWS: StageView[] = ["start", "map", "does", "fixes", "promises"];
const CHART_VIEWS: StageView[] = ["numbers", "menu", "chart"];
const WATCH_VIEWS: StageView[] = ["runtime", "deps"];

function NavButton({ children, active, ...props }: { children: React.ReactNode; active?: boolean } & React.ButtonHTMLAttributes<HTMLButtonElement>) {
  // Spreads Radix's trigger props through — a custom trigger child that
  // dropped them would leave the menu permanently closed.
  return (
    <button
      type="button"
      className={`h-9 rounded-lg px-3 text-sm font-medium inline-flex items-center gap-1 transition-colors ${
        active
          ? "bg-emerald-50 text-emerald-900"
          : "text-stone-600 hover:text-stone-900 hover:bg-stone-100"
      }`}
      {...props}
    >
      {children}
    </button>
  );
}

export function Topbar() {
  const { source, label, view, setView, llm, setLlm, llmSettingsOpen, setLlmSettingsOpen, loading, clearChat, closeCanvasChart } = useCie();
  const [ownOpen, setOwnOpen] = useState(false);

  const go = (v: StageView) => () => setView(v);

  return (
    <header className="shrink-0 border-b border-stone-200 bg-white/95 backdrop-blur z-40">
      <div className="h-14 px-3 sm:px-4 flex items-center gap-2 min-w-0">
        {/* brand */}
        <div className="flex items-center gap-2.5 shrink-0">
          <div className="flex h-8 w-8 items-center justify-center rounded-lg bg-emerald-700 text-white">
            <Compass className="h-4 w-4" />
          </div>
          <div className="hidden md:block leading-tight">
            <p className="font-bold text-stone-900 text-sm">CIE</p>
            <p className="text-[10px] text-stone-500 -mt-0.5">Your code, in plain words</p>
          </div>
        </div>

        {/* model chip — who answers, one click from anywhere */}
        <button
          type="button"
          onClick={() => setLlmSettingsOpen(true)}
          title="Who answers your questions — click to change"
          aria-haspopup="dialog"
          className="h-8 shrink-0 max-w-[180px] sm:max-w-[220px] rounded-full border border-stone-200 bg-stone-50 hover:border-emerald-300 hover:bg-emerald-50 px-3 inline-flex items-center gap-1.5 text-xs text-stone-700 transition-colors"
        >
          <BrainCog className="h-3.5 w-3.5 text-emerald-700 shrink-0" />
          <span className="truncate font-medium">{loading ? "…" : providerLabel(llm)}</span>
          <ChevronDown className="h-3 w-3 text-stone-400 shrink-0" aria-hidden />
        </button>

        {/* repo chip — what is loaded; clicking opens "use your own" */}
        <button
          type="button"
          onClick={() => setOwnOpen(true)}
          title={loading ? "Loading…" : `${label} — click to use your own project`}
          className="hidden sm:inline-flex h-8 min-w-0 max-w-[230px] rounded-full border border-stone-200 bg-white px-3 items-center gap-1.5 text-xs text-stone-600 hover:border-emerald-300 transition-colors"
        >
          <span
            className={`h-2 w-2 rounded-full shrink-0 ${loading ? "bg-amber-400 animate-pulse" : source === "demo" ? "bg-stone-400" : "bg-emerald-500"}`}
            aria-hidden
          />
          <span className="truncate">{loading ? "Loading the sample project…" : label || "No project loaded"}</span>
        </button>

        <div className="flex-1 min-w-2" />

        {/* main menus — top right */}
        <nav className="hidden lg:flex items-center gap-0.5" aria-label="Main">
          <DropdownMenu>
            <DropdownMenuTrigger asChild>
              <NavButton active={EXPLORE_VIEWS.includes(view)}>
                Explore <ChevronDown className="h-3 w-3 opacity-50" aria-hidden />
              </NavButton>
            </DropdownMenuTrigger>
            <DropdownMenuContent align="start" className="w-60">
              <DropdownMenuLabel>Look around</DropdownMenuLabel>
              <DropdownMenuItem onClick={go("start")} className={view === "start" ? "bg-emerald-50" : ""}>
                <Sparkles className="h-4 w-4 text-stone-400" /> Start here
              </DropdownMenuItem>
              <DropdownMenuItem onClick={go("map")} className={view === "map" ? "bg-emerald-50" : ""}>
                <MapIcon className="h-4 w-4 text-stone-400" /> The map of your code
              </DropdownMenuItem>
              <DropdownMenuItem onClick={go("does")} className={view === "does" ? "bg-emerald-50" : ""}>
                <Wand2 className="h-4 w-4 text-stone-400" /> What it does
              </DropdownMenuItem>
              <DropdownMenuSeparator />
              <DropdownMenuLabel>Pay attention to</DropdownMenuLabel>
              <DropdownMenuItem onClick={go("fixes")} className={view === "fixes" ? "bg-emerald-50" : ""}>
                <ListChecks className="h-4 w-4 text-stone-400" /> Things to look at
              </DropdownMenuItem>
              <DropdownMenuItem onClick={go("promises")} className={view === "promises" ? "bg-emerald-50" : ""}>
                <ShieldCheck className="h-4 w-4 text-stone-400" /> Promises
              </DropdownMenuItem>
            </DropdownMenuContent>
          </DropdownMenu>

          <DropdownMenu>
            <DropdownMenuTrigger asChild>
              <NavButton active={CHART_VIEWS.includes(view)}>
                Charts <ChevronDown className="h-3 w-3 opacity-50" aria-hidden />
              </NavButton>
            </DropdownMenuTrigger>
            <DropdownMenuContent align="start" className="w-64">
              <DropdownMenuItem onClick={go("numbers")} className={view === "numbers" ? "bg-emerald-50" : ""}>
                <PieChart className="h-4 w-4 text-stone-400" /> Your project in pictures
              </DropdownMenuItem>
              <DropdownMenuItem onClick={go("menu")} className={view === "menu" ? "bg-emerald-50" : ""}>
                <LayoutGrid className="h-4 w-4 text-stone-400" /> One-tap chart menu
              </DropdownMenuItem>
              <DropdownMenuSeparator />
              <p className="px-2 py-1.5 text-xs text-stone-500 leading-relaxed">
                Charts the chat draws can go big on the canvas — use the button on any answer.
              </p>
            </DropdownMenuContent>
          </DropdownMenu>

          <DropdownMenu>
            <DropdownMenuTrigger asChild>
              <NavButton active={WATCH_VIEWS.includes(view)}>
                Watching it run <ChevronDown className="h-3 w-3 opacity-50" aria-hidden />
              </NavButton>
            </DropdownMenuTrigger>
            <DropdownMenuContent align="start" className="w-64">
              <DropdownMenuItem onClick={go("runtime")} className={view === "runtime" ? "bg-emerald-50" : ""}>
                <Timer className="h-4 w-4 text-stone-400" /> Runtime evidence
              </DropdownMenuItem>
              <DropdownMenuItem onClick={go("deps")} className={view === "deps" ? "bg-emerald-50" : ""}>
                <Package className="h-4 w-4 text-stone-400" /> Dependencies
              </DropdownMenuItem>
              <DropdownMenuSeparator />
              <p className="px-2 py-1.5 text-xs text-stone-500 leading-relaxed">
                Measured evidence only: every number names the tool that measured it. No collected evidence? The page says so — and shows how to collect it.
              </p>
            </DropdownMenuContent>
          </DropdownMenu>

          <DropdownMenu>
            <DropdownMenuTrigger asChild>
              <NavButton active={view === "trust"}>
                Trust <ChevronDown className="h-3 w-3 opacity-50" aria-hidden />
              </NavButton>
            </DropdownMenuTrigger>
            <DropdownMenuContent align="start" className="w-60">
              <DropdownMenuItem onClick={go("trust")} className={view === "trust" ? "bg-emerald-50" : ""}>
                <ShieldCheck className="h-4 w-4 text-stone-400" /> Can I trust this?
              </DropdownMenuItem>
            </DropdownMenuContent>
          </DropdownMenu>
        </nav>

        {/* Use your own — always-visible action on desktop */}
        <Button
          size="sm"
          className="hidden md:inline-flex h-8 bg-emerald-700 hover:bg-emerald-800 text-white shrink-0"
          onClick={() => setOwnOpen(true)}
        >
          <Upload className="h-3.5 w-3.5" />
          <span className="hidden xl:inline">Use your own</span>
        </Button>

        {/* mobile: everything in one menu */}
        <div className="lg:hidden flex items-center">
          <DropdownMenu>
            <DropdownMenuTrigger asChild>
              <Button variant="outline" size="icon" className="h-8 w-8" aria-label="Menu">
                <MenuIcon className="h-4 w-4" />
              </Button>
            </DropdownMenuTrigger>
            <DropdownMenuContent align="end" className="w-64">
              <DropdownMenuLabel>Look around</DropdownMenuLabel>
              <DropdownMenuItem onClick={go("start")} className={view === "start" ? "bg-emerald-50" : ""}>Start here</DropdownMenuItem>
              <DropdownMenuItem onClick={go("map")} className={view === "map" ? "bg-emerald-50" : ""}>The map</DropdownMenuItem>
              <DropdownMenuItem onClick={go("does")} className={view === "does" ? "bg-emerald-50" : ""}>What it does</DropdownMenuItem>
              <DropdownMenuItem onClick={go("fixes")} className={view === "fixes" ? "bg-emerald-50" : ""}>Things to look at</DropdownMenuItem>
              <DropdownMenuItem onClick={go("promises")} className={view === "promises" ? "bg-emerald-50" : ""}>Promises</DropdownMenuItem>
              <DropdownMenuSeparator />
              <DropdownMenuLabel>Watching it run</DropdownMenuLabel>
              <DropdownMenuItem onClick={go("runtime")} className={view === "runtime" ? "bg-emerald-50" : ""}>Runtime evidence</DropdownMenuItem>
              <DropdownMenuItem onClick={go("deps")} className={view === "deps" ? "bg-emerald-50" : ""}>Dependencies</DropdownMenuItem>
              <DropdownMenuSeparator />
              <DropdownMenuLabel>Charts</DropdownMenuLabel>
              <DropdownMenuItem onClick={go("numbers")} className={view === "numbers" ? "bg-emerald-50" : ""}>In pictures</DropdownMenuItem>
              <DropdownMenuItem onClick={go("menu")} className={view === "menu" ? "bg-emerald-50" : ""}>One-tap menu</DropdownMenuItem>
              <DropdownMenuSeparator />
              <DropdownMenuItem onClick={go("trust")} className={view === "trust" ? "bg-emerald-50" : ""}>Can I trust this?</DropdownMenuItem>
              <DropdownMenuSeparator />
              <DropdownMenuItem onClick={() => setOwnOpen(true)}>
                <Upload className="h-4 w-4 text-stone-400" /> Use your own project
              </DropdownMenuItem>
              <DropdownMenuItem onClick={() => setLlmSettingsOpen(true)}>
                <Settings2 className="h-4 w-4 text-stone-400" /> Who answers — {providerLabel(llm)}
              </DropdownMenuItem>
              <DropdownMenuItem onClick={() => { clearChat(); closeCanvasChart(); }}>
                <Sparkles className="h-4 w-4 text-stone-400" /> Clear the conversation
              </DropdownMenuItem>
            </DropdownMenuContent>
          </DropdownMenu>
        </div>

        {/* settings gear — desktop */}
        <div className="hidden lg:block">
          <DropdownMenu>
            <DropdownMenuTrigger asChild>
              <Button variant="outline" size="icon" className="h-8 w-8" aria-label="Settings">
                <Settings2 className="h-4 w-4 text-stone-500" />
              </Button>
            </DropdownMenuTrigger>
            <DropdownMenuContent align="end" className="w-60">
              <DropdownMenuItem onClick={() => setLlmSettingsOpen(true)}>
                <BrainCog className="h-4 w-4 text-stone-400" /> Who answers — {providerLabel(llm)}
              </DropdownMenuItem>
              <DropdownMenuItem onClick={() => { clearChat(); }}>
                <Sparkles className="h-4 w-4 text-stone-400" /> Clear the conversation
              </DropdownMenuItem>
              <DropdownMenuSeparator />
              <div className="px-2 py-1.5">
                {source && (
                  <Badge variant="outline" className={source === "demo" ? "bg-stone-100 text-stone-600 border-stone-200" : "bg-emerald-50 text-emerald-800 border-emerald-200"}>
                    {source === "demo" ? "Sample project loaded" : "Your project loaded"}
                  </Badge>
                )}
                <p className="text-[11px] text-stone-400 mt-1.5 leading-relaxed">
                  CIE shows what it knows and how sure it is. It never hides a guess.
                </p>
              </div>
            </DropdownMenuContent>
          </DropdownMenu>
        </div>
      </div>

      {/* the two dialogs the topbar owns */}
      <LlmSettingsDialog
        open={llmSettingsOpen}
        onOpenChange={setLlmSettingsOpen}
        config={llm}
        onSave={setLlm}
      />
      <Dialog open={ownOpen} onOpenChange={setOwnOpen}>
        <DialogContent className="sm:max-w-lg">
          <DialogHeader>
            <DialogTitle>See your own project here</DialogTitle>
            <DialogDescription>
              Connect a public repo, browse this computer, or upload the files the{" "}
              <span className="font-mono text-xs bg-stone-100 rounded px-1">cie</span> command creates. Either way,
              every view switches to your code — and the chat knows it too.
            </DialogDescription>
          </DialogHeader>
          <UseYourOwnDialog />
        </DialogContent>
      </Dialog>
    </header>
  );
}
