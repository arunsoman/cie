/**
 * cie-viz-engines — the CIE-VIZ-MULTI adapter layer.
 *
 *   gate(spec)                  honesty, once, before anything maps
 *   chooseEngine(request, reg)  the pure negotiator (capability →
 *                              preference → size → deterministic order)
 *   renderHeadless(...)         the ONLY pipeline: negotiate → gate →
 *                              adapt (VIZ-09: adapters are never invoked
 *                              without a negotiator decision)
 *   recomputeSemanticHash / recomputeRenderHash   PRD §3 hash split
 *   textOutline / renderOutline VIZ-V-02 authoritative linearization
 *   CATALOG                     76 charts, bounded only by registration
 *
 * Engine adapters: g6 (full, extension surface), jointjs (full),
 * reactflow (full via custom node), mermaid (degraded, disclosed).
 */

export * from './types';
export { VizError, NegotiationError } from './errors';
export type { GateErrorCode } from './errors';
export { gate, gateFields, ELEMENT_CAP } from './gate';
export {
  stableStringify,
  recomputeSemanticHash,
  recomputeRenderHash,
  verifySemanticHash,
  verifyRenderHash,
} from './hash';
export { audit } from './audit';
export type { AuditFinding } from './audit';
export {
  ENGINE_ORDER,
  chooseEngine,
  isEligible,
  decideAll,
  disclose,
  renderHeadless,
  renderHosted,
  renderHashOf,
} from './negotiator';
export type { EngineDecision, NegotiationDisclosure, HeadlessResult, HostedResult } from './negotiator';
export { engineBanner, layoutChip, fidelityBanner, hiddenRows } from './disclosure';
export type { EngineBanner, LayoutChip, FidelityBanner, HiddenRow } from './disclosure';
export { createSelectionHub } from './selection';
export type { SelectionHub } from './selection';
export { textOutline, outlineSummary, renderOutline } from './outline';
export {
  MODE_CHANNELS,
  SettleDetector,
  THEME_TOKENS,
  announce,
  colourAloneAudit,
  expansionLens,
  gestureToProposal,
  keyboardMap,
  layoutQuality,
  levelOfDetail,
  rovingOrder,
  verifyParity,
  verifyTheme,
} from './interaction';
export type {
  Gesture,
  GestureKind,
  KeyBinding,
  LensData,
  LodDecision,
  LodTier,
  Position,
  Proposal,
  Theme,
  ThemeToken,
  LayoutQuality,
} from './interaction';
export type { OutlineEntry } from './outline';
export { CATALOG, CATALOG_SUMMARY, actionsFor, CATEGORY_DEFAULT_ACTIONS } from './catalog';
export type { CatalogEntry, CatalogCategory, G6Status, ChartAction } from './catalog';

export {
  G6Adapter,
  adapt as g6Adapt,
  graphOptions as g6GraphOptions,
  autoLayout as g6AutoLayout,
  buildPlugins,
  nodeTypeFor,
  NODE_TYPE,
  DASH,
  STROKE,
  EDGE_DASH,
  WORDS,
  G6_LAYOUTS,
  layoutOutcomeFor as g6LayoutOutcome,
} from './engines/g6';
export {
  MermaidAdapter,
  adapt as mermaidAdapt,
  MERMAID_CLASSDEFS,
  MERMAID_LAYOUTS,
} from './engines/mermaid';
export type { MermaidOutput } from './engines/mermaid';
export {
  ReactFlowAdapter,
  adapt as reactflowAdapt,
  layeredPositions,
  RF_CLASSES,
  REACTFLOW_LAYOUTS,
} from './engines/reactflow';
export type { RFGraph, RFNode, RFEdge } from './engines/reactflow';
export {
  JointJSAdapter,
  adapt as jointjsAdapt,
  JOINT_DASH,
  JOINT_STROKE,
  JOINTJS_LAYOUTS,
} from './engines/jointjs';
export type { JointGraph, JointElement, JointLink } from './engines/jointjs';
export {
  computeFlameGeometry,
  depthColor,
  FLAME_DASH,
  FLAME_STROKE,
  FLAME_REGISTRATION_SNIPPET,
} from './engines/flame';
export type { FlameGeometry, FlameFrame, FlameNodeIn, FlameEdgeIn } from './engines/flame';

/** The default registry: all four engines, negotiator-ready. */
import { G6Adapter } from './engines/g6';
import { JointJSAdapter } from './engines/jointjs';
import { ReactFlowAdapter } from './engines/reactflow';
import { MermaidAdapter } from './engines/mermaid';
import type { EngineAdapter, EngineId } from './types';

export function defaultRegistry(): Map<EngineId, EngineAdapter> {
  const adapters: EngineAdapter[] = [
    new G6Adapter(),
    new JointJSAdapter(),
    new ReactFlowAdapter(),
    new MermaidAdapter(),
  ];
  return new Map(adapters.map((a) => [a.id, a]));
}
export { planSheet } from './manager';
export type { ChartPlan } from './manager';
export { SelectionConductor } from './conductor';
