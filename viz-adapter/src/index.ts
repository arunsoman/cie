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

export * from './types.js';
export { VizError, NegotiationError } from './errors.js';
export type { GateErrorCode } from './errors.js';
export { gate, ELEMENT_CAP } from './gate.js';
export {
  stableStringify,
  recomputeSemanticHash,
  recomputeRenderHash,
  verifySemanticHash,
  verifyRenderHash,
} from './hash.js';
export { audit } from './audit.js';
export type { AuditFinding } from './audit.js';
export {
  ENGINE_ORDER,
  chooseEngine,
  isEligible,
  decideAll,
  disclose,
  renderHeadless,
  renderHashOf,
} from './negotiator.js';
export type { EngineDecision, NegotiationDisclosure, HeadlessResult } from './negotiator.js';
export { engineBanner, layoutChip, fidelityBanner } from './disclosure.js';
export type { EngineBanner, LayoutChip, FidelityBanner } from './disclosure.js';
export { textOutline, outlineSummary, renderOutline } from './outline.js';
export type { OutlineEntry } from './outline.js';
export { CATALOG, CATALOG_SUMMARY } from './catalog.js';
export type { CatalogEntry, CatalogCategory, G6Status } from './catalog.js';

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
} from './engines/g6.js';
export {
  MermaidAdapter,
  adapt as mermaidAdapt,
  MERMAID_CLASSDEFS,
  MERMAID_LAYOUTS,
} from './engines/mermaid.js';
export type { MermaidOutput } from './engines/mermaid.js';
export {
  ReactFlowAdapter,
  adapt as reactflowAdapt,
  layeredPositions,
  RF_CLASSES,
  REACTFLOW_LAYOUTS,
} from './engines/reactflow.js';
export type { RFGraph, RFNode, RFEdge } from './engines/reactflow.js';
export {
  JointJSAdapter,
  adapt as jointjsAdapt,
  JOINT_DASH,
  JOINT_STROKE,
  JOINTJS_LAYOUTS,
} from './engines/jointjs.js';
export type { JointGraph, JointElement, JointLink } from './engines/jointjs.js';
export {
  computeFlameGeometry,
  depthColor,
  FLAME_DASH,
  FLAME_STROKE,
  FLAME_REGISTRATION_SNIPPET,
} from './engines/flame.js';
export type { FlameGeometry, FlameFrame, FlameNodeIn, FlameEdgeIn } from './engines/flame.js';

/** The default registry: all four engines, negotiator-ready. */
import { G6Adapter } from './engines/g6.js';
import { JointJSAdapter } from './engines/jointjs.js';
import { ReactFlowAdapter } from './engines/reactflow.js';
import { MermaidAdapter } from './engines/mermaid.js';
import type { EngineAdapter, EngineId } from './types.js';

export function defaultRegistry(): Map<EngineId, EngineAdapter> {
  const adapters: EngineAdapter[] = [
    new G6Adapter(),
    new JointJSAdapter(),
    new ReactFlowAdapter(),
    new MermaidAdapter(),
  ];
  return new Map(adapters.map((a) => [a.id, a]));
}
