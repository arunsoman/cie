// The "plain words" translator — every technical term CIE produces gets a
// friendly, non-technical translation here. This file is the product promise:
// a non-technical person should understand every label on screen.

import type { Finding } from "./cie-types";

// ---- Severity: how urgent is this? ----
export function severityPlain(s: string): { label: string; hint: string; tone: "rose" | "amber" | "stone" } {
  switch (s) {
    case "Critical":
      return { label: "Fix soon", hint: "This could let the wrong people in or lose data.", tone: "rose" };
    case "High":
      return { label: "Please look", hint: "Not an emergency, but worth your attention.", tone: "rose" };
    case "Medium":
      return { label: "Worth checking", hint: "Could cause trouble later.", tone: "amber" };
    default:
      return { label: "Good to know", hint: "No rush — just so you know.", tone: "stone" };
  }
}

// ---- Finding kinds: what went wrong, in plain words ----
const FINDING_TITLES: Record<string, string> = {
  PrintOnlyAuthorization: "Says 'access granted' without really checking",
  SqlInjection: "Strangers could trick the database",
  HardcodedSecret: "A password or key is written directly in the code",
  IgnoredRejection: "The code ignores when something fails",
  SwallowedException: "A problem happens but is quietly hidden",
  ExceptionToSuccess: "When things fail, the code pretends it worked",
  DeadCode: "Code that never runs",
  CoverageGap: "A part we could not fully check",
  LayeringViolation: "Two parts talk that shouldn't",
  NPlusOneQuery: "Asks the database the same question again and again (slow)",
};

export function findingTitle(f: Finding): string {
  return FINDING_TITLES[f.kind] ?? f.kind.replace(/([a-z])([A-Z])/g, "$1 $2");
}

// ---- Promises (contracts) ----
const OBLIGATION_TITLES: Record<string, string> = {
  MustAuthorize: "Check permission first",
  MustNotLeakSecrets: "Keep secrets secret",
  MustValidateInput: "Check user input before using it",
  MustLog: "Keep a record of what happened",
};

export function obligationTitle(o: unknown): string {
  if (typeof o === "string") {
    return OBLIGATION_TITLES[o] ?? o.replace(/([a-z])([A-Z])/g, "$1 $2").replace(/_/g, " ");
  }
  // The engine also emits structured obligations — most commonly the serde
  // externally-tagged enum form {"Custom": "<the rule in the code's own
  // words>"}. Show the engine's words as written; any other shape degrades
  // to its variant name. None of these may ever crash a render.
  if (o && typeof o === "object") {
    const custom = (o as Record<string, unknown>).Custom;
    if (typeof custom === "string" && custom) return custom;
    const key = Object.keys(o as Record<string, unknown>)[0];
    if (typeof key === "string" && key) return key.replace(/_/g, " ");
  }
  return "A promise CIE couldn't name";
}

export function authorityPlain(a: string): { label: string; hint: string } {
  switch (a) {
    case "Explicit":
      return { label: "Written by you", hint: "Found in your code's notes (docstrings/comments)." };
    case "InferredCandidate":
      return { label: "CIE's guess", hint: "CIE thinks this rule applies, based on similar code." };
    default:
      return { label: a, hint: "" };
  }
}

// ---- Certainty (display modes) ----
export function certaintyPlain(m: string): { label: string; hint: string } {
  switch (m) {
    case "Fact":
      return { label: "Proven", hint: "Checked directly against the code. You can rely on this." };
    case "Inference":
      return { label: "Good guess", hint: "CIE worked this out from patterns. Usually right, worth a glance." };
    case "Hypothesis":
      return { label: "Not sure yet", hint: "CIE spotted something but can't confirm it." };
    default:
      return { label: m, hint: "" };
  }
}

// ---- Map element kinds ----
export function elementKindPlain(k: string): string {
  switch (k) {
    case "domain": return "The big picture";
    case "module": return "A code file";
    case "usecase": return "Something it does";
    case "rule": return "A rule it follows";
    case "invariant": return "Something that must always be true";
    default: return k;
  }
}

export function edgeKindPlain(k: string): string {
  switch (k) {
    case "package_containment": return "lives inside";
    case "call_ownership": return "is responsible for";
    case "calls": return "talks to";
    case "cluster_cohesion": return "works together with";
    case "fca_subsumption": return "is part of";
    default: return k.replace(/_/g, " ");
  }
}

// ---- Effects / behaviours ----
const EFFECT_TITLES: Record<string, string> = {
  Print: "Prints messages",
  Log: "Keeps logs",
  Network: "Uses the network",
  FileIO: "Reads or writes files",
  Database: "Talks to the database",
  DbRead: "Reads from the database",
  DbWrite: "Saves things to the database",
  Crypto: "Scrambles and unscrambles secrets",
  Spawn: "Starts background work",
  Sleep: "Waits",
  Time: "Uses the clock",
  Random: "Uses randomness",
};

export function effectPlain(e: string): string {
  return EFFECT_TITLES[e] ?? e.replace(/_/g, " ");
}

// ---- Dictionary shown in the UI ----
export const DICTIONARY: { word: string; means: string }[] = [
  { word: "Promise", means: "Something your code must always do. CIE finds these and checks them." },
  { word: "Named thing", means: "Anything with a name in your code — a function, a class, a module." },
  { word: "Map", means: "A picture of how the parts of your code connect." },
  { word: "Proven", means: "CIE checked it directly. You can rely on it." },
  { word: "Good guess", means: "CIE worked it out from patterns. Usually right, but worth a glance." },
  { word: "Fix soon", means: "The most urgent problems — check these first." },
  { word: "Checked", means: "How much of your code CIE was able to look at." },
];
