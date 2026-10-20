/**
 * The ViewSpec contract v2 + the multi-engine adapter surface
 * (CIE-VIZ-MULTI PRD Parts I–II). CIE emits a semantic graph and NEVER
 * names an engine (VIZ-10); engines are adapters behind one interface;
 * the negotiator picks the engine; the gate enforces honesty once.
 */

/** Epistemic display mode — the four honest states. */
export type DisplayMode = 'Fact' | 'Inference' | 'Hypothesis' | 'Fog';

/** Open element vocabulary (new kinds are registrations, not changes). */
export type ElementKind = string;

/** Registered engine ids — deterministic negotiation order: g6 → jointjs
 *  → reactflow → mermaid. */
export type EngineId = 'g6' | 'jointjs' | 'reactflow' | 'mermaid';

/** `{ file, start, end }` byte-span evidence — REQUIRED everywhere. */
export interface EvidenceRef {
  file: string;
  start: number;
  end: number;
}

export interface Salience {
  factors: string[];
  weight: number;
  whyShown: string;
}

export interface ViewElement {
  id: string;
  elementKind: ElementKind;
  displayMode: DisplayMode;
  label: string;
  evidence: EvidenceRef;
  claimId?: string;
  counterArgument?: string;
  fogReason?: string;
  salience: Salience;
  fields: Record<string, unknown>;
  parentId?: string;
}

export interface ViewEdge {
  id: string;
  source: string;
  target: string;
  edgeKind: string;
  displayMode: DisplayMode;
  evidence: EvidenceRef;
  counterArgument?: string;
}

export interface ViewGroup {
  id: string;
  label: string;
  kind: string;
}

export interface Gap {
  id: string;
  area: string;
  why: string;
}

/**
 * RFC-0024 patch 3 (ux-spec §2.2/§13.1) — the canonical shapes inside the
 * open `fields` bag. `bodyLines` is the component-board's evidence-gated
 * narration (gate: every line carries its own EvidenceRef); `alsoEvidence`
 * carries SECOND recorded locations for paired popups (drift doc↔code,
 * dep-edge from↔to). Both are optional; presence is validated by the gate.
 */
export interface BodyLine {
  text: string;
  line: number;
  evidence: EvidenceRef;
}

export interface HiddenCandidate {
  id: string;
  reason: string;
  salienceWeight: number;
  /** RFC-0024 patch 2 (G1) — human row text for the disclosure popup;
   *  absent ⇒ the UI falls back to `id` and says so. */
  label?: string;
  /** RFC-0024 patch 2 — plain-words element kind of the hidden row
   *  (container / module / symbol / edge / …); absent ⇒ `unknown`. */
  kind?: string;
}

export interface EpistemicFooter {
  facts: number;
  inferences: number;
  hypotheses: number;
  fog: number;
  stale: number;
  caption: string;
}

export interface ScopeCoverage {
  analyzed: boolean;
  ratio?: number;
}

export interface ScopeAttestation {
  revision: number;
  environments: string[];
  coverage: ScopeCoverage;
}

export interface LayoutHint {
  strategy: string;
  params?: Record<string, unknown>;
}

export interface FisheyePrefs {
  trigger?: 'pointermove' | 'click' | 'drag';
  d?: number;
  r?: number;
  maxR?: number;
  minR?: number;
  maxD?: number;
  minD?: number;
  scaleRBy?: 'wheel' | 'drag';
  scaleDBy?: 'wheel' | 'drag';
  showDPercent?: boolean;
  [extra: string]: unknown;
}

export interface Preferences {
  fisheye?: FisheyePrefs;
  minimap?: boolean | { size?: [number, number]; position?: string };
  edgeBundling?: boolean;
  history?: boolean;
  renderer?: 'canvas' | 'svg' | 'webgl';
}

export interface ViewSpec {
  elements: ViewElement[];
  edges: ViewEdge[];
  groups?: ViewGroup[];
  footer: EpistemicFooter;
  gaps: Gap[];
  hidden: HiddenCandidate[];
  scope: ScopeAttestation;
  layoutHint?: LayoutHint;
  preferences?: Preferences;
  /** Cache key — sha256 over the canonical SEMANTIC payload
   *  {elements, edges, groups, gaps, footer, scope}. A layout or engine
   *  swap NEVER invalidates it (PRD §3, VIZ-02). */
  semanticHash: string;
  /** Render-artifact key — sha256 over {semanticHash, layoutHint,
   *  preferences, engineId}. The producer seals the engine-neutral
   *  baseline (engineId ""); adapters recompute with their own id (PRD §3,
   *  VIZ-03; CIE never names an engine, VIZ-10). */
  renderHash: string;
}

// ── Part II: the EngineAdapter contract ────────────────────────────────

export interface CapabilitySet {
  fisheye: boolean;
  customNodes: boolean;
  customEdges: boolean;
  customLayouts: boolean;
  plugins: boolean;
  interactive: boolean;
  editable: boolean;
  combos: boolean;
  animatedLayout: boolean;
  svgExport: boolean;
  canvasExport: boolean;
  webgl: boolean;
}

export interface EngineLimits {
  maxNodes: number;
  maxEdges: number;
  supportedLayouts: string[];
  unsupportedFeatures: string[];
}

/** Fidelity of the epistemic encoding for one rendering (PRD §7). */
export type EncodingFidelity = 'Full' | 'Degraded' | 'Unsupported';

export interface LayoutOutcome {
  requested: string | undefined;
  actual: string;
  fidelity: 'Exact' | 'Substituted' | 'Unavailable';
}

export interface RenderOptions {
  container?: string;
  renderer?: 'canvas' | 'svg' | 'webgl';
  fisheye?: boolean;
  /** Extra engine-native options (passed through untouched). */
  [extra: string]: unknown;
}

/** Audit report — provenance (VIZ-H) + encoding fidelity (VIZ-V-01). */
export interface AuditReport {
  findings: { rule: string; detail: string }[];
  encodingFidelity: EncodingFidelity;
  layout: LayoutOutcome;
}

/**
 * RFC-0024 patch 1 — who moved the selection: a human click in the
 * canvas ('user') or a host call (chips, plan, conductor —
 * 'programmatic'). Engines MUST tag every callback. */
export type SelectionOrigin = 'user' | 'programmatic';

/** RFC-0024 patch 1 — selection callback: element ids in pick order. */
export type SelectionListener = (ids: string[], origin: SelectionOrigin) => void;

/**
 * RFC-0024 stage 1 — the host-injected render runtime. The PAGE bundles
 * @antv/g6 and owns pixels + events; the adapter package stays
 * headless-pure (no DOM, no engine import). The negotiator hands the
 * host the ADAPTER-PREPARED data + options after negotiate → gate →
 * adapt → configure, so there is still exactly one render path
 * (VIZ-09) and no bypass of the gate.
 */
export interface HostRenderRuntime {
  render(
    engineId: EngineId,
    container: HTMLElement,
    spec: ViewSpec,
    options: RenderOptions,
  ): Promise<RenderHandle>;
  relayout?(engineId: EngineId, handle: RenderHandle, layout: LayoutHint): Promise<LayoutOutcome>;
}

/**
 * RFC-0024 patch 1 (ux-spec §2.3/§13.1) — the selection surface on every
 * render handle. `onSelection` subscribes (returns the unsubscribe fn);
 * `setSelected` moves the selection (loop-guarded: re-setting an identical
 * set never re-fires listeners, and re-entrant calls from inside a
 * listener are coalesced, never recursed — see selection.ts).
 */
export interface RenderHandle {
  engineId: EngineId;
  semanticHash: string;
  renderHash: string;
  layout: LayoutOutcome;
  onSelection(cb: SelectionListener): () => void;
  setSelected(ids: string[], origin: SelectionOrigin): void;
  dispose(): Promise<void>;
}

/**
 * Every engine is an adapter implementing this interface (PRD §4). The
 * browser-bound methods (render/relayout/dispose) are only callable in a
 * host page; the headless pipeline uses gate/adapt/configure/audit.
 */
export interface EngineAdapter {
  readonly id: EngineId;
  readonly capabilities: CapabilitySet;
  readonly limits: EngineLimits;

  /** The single honesty enforcement point — throws before any mapping. */
  gate(spec: ViewSpec): void;
  /** ViewSpec → engine-native data (pure). */
  adapt(spec: ViewSpec): unknown;
  /** Engine-native options (pure). */
  configure(spec: ViewSpec, options: RenderOptions): unknown;
  /** Browser render (typed but only executable in a host page). */
  render(container: HTMLElement, spec: ViewSpec, options: RenderOptions): Promise<RenderHandle>;
  relayout(handle: RenderHandle, layout: LayoutHint): Promise<LayoutOutcome>;
  audit(spec: ViewSpec, layout?: LayoutOutcome): AuditReport;
}

/** Negotiation request (PRD §6). */
export interface EngineRequest {
  required: Partial<CapabilitySet>;
  preferred?: EngineId;
  graphSize: { nodes: number; edges: number };
  layoutHint?: LayoutHint;
}

// ── G6 data model (business fields under `data`) ───────────────────────

export interface G6NodeDatum {
  id: string;
  data: {
    elementKind: ElementKind;
    displayMode: DisplayMode;
    label: string;
    evidence: EvidenceRef;
    claimId?: string;
    counterArgument?: string;
    fogReason?: string;
    salienceWeight: number;
    fields: Record<string, unknown>;
    parentId?: string;
  };
}

export interface G6EdgeDatum {
  id: string;
  source: string;
  target: string;
  data: {
    edgeKind: string;
    displayMode: DisplayMode;
    evidence: EvidenceRef;
    counterArgument?: string;
  };
}

export interface GraphData {
  nodes: G6NodeDatum[];
  edges: G6EdgeDatum[];
  combos: { id: string; data: { label: string; kind: string } }[];
}
