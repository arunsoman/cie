/**
 * The chart catalog (PRD Part VI) — 76 diagrams the engine must produce.
 * DATA, not branching: the catalog is the enumeration of what exists
 * today; the registry is the enumeration of what CAN exist. `g6Status`
 * Native / Custom / Alt-engine; extension budget: 4 custom node types,
 * 2 custom edge types, 2 custom layouts, 2 custom plugins.
 */

export type G6Status = 'Native' | 'Custom' | 'Alt-engine';
export type CatalogCategory =
  | 'A Structural / UML'
  | 'B Behavioral / UML'
  | 'C Architecture'
  | 'D Data'
  | 'E Code-level'
  | 'F Runtime'
  | 'G Quality'
  | 'H Decision'
  | 'I Epistemic'
  | 'J Journey'
  | 'K Organizational';

export interface CatalogEntry {
  id: string; // A1..K3
  name: string;
  category: CatalogCategory;
  shape: string;
  bestEngine: 'g6' | 'mermaid' | 'jointjs' | 'reactflow' | 'custom-svg';
  g6Status: G6Status;
  layout: string;
  cieUse: string;
}

const e = (
  id: string,
  name: string,
  category: CatalogCategory,
  shape: string,
  bestEngine: CatalogEntry['bestEngine'],
  g6Status: G6Status,
  layout: string,
  cieUse: string,
): CatalogEntry => ({ id, name, category, shape, bestEngine, g6Status, layout, cieUse });

export const CATALOG: CatalogEntry[] = [
  // ── A Structural / UML (13) ──────────────────────────────────────────
  e('A1', 'UML class', 'A Structural / UML', 'class+attrs+methods → inheritance, association, dependency', 'g6', 'Custom', 'dagre', 'CAP-12.1, CAP-1.7'),
  e('A2', 'Component', 'A Structural / UML', 'component+interface → provides, requires', 'g6', 'Native', 'dagre', 'CAP-1.7'),
  e('A3', 'Package', 'A Structural / UML', 'package → imports', 'g6', 'Native', 'combo-combined', 'CAP-1.3'),
  e('A4', 'Deployment', 'A Structural / UML', 'node+artifact → deployed-to', 'g6', 'Native', 'dagre', 'CAP-9.6'),
  e('A5', 'Object', 'A Structural / UML', 'instance+values → links', 'g6', 'Native', 'force', 'CAP-13.6'),
  e('A6', 'Composite structure', 'A Structural / UML', 'part+port → wiring', 'g6', 'Custom', 'dagre', 'CAP-1.10'),
  e('A7', 'Use case', 'A Structural / UML', 'actor+usecase → include, extend', 'g6', 'Custom', 'radial', 'CAP-14.2'),
  e('A8', 'Layer', 'A Structural / UML', 'layer → allowed/forbidden', 'g6', 'Native', 'grid', 'CAP-1.1'),
  e('A9', 'C4 Context', 'A Structural / UML', 'person+system → uses', 'g6', 'Native', 'dagre', 'CAP-12.1'),
  e('A10', 'C4 Container', 'A Structural / UML', 'container+person → uses', 'g6', 'Native', 'combo-combined', 'CAP-12.1'),
  e('A11', 'C4 Component', 'A Structural / UML', 'component+container → uses', 'g6', 'Native', 'combo-combined', 'CAP-12.1'),
  e('A12', 'C4 Code', 'A Structural / UML', 'class+package (reuses A1+A3)', 'g6', 'Native', 'dagre', 'CAP-12.1'),
  e('A13', 'Hexagonal', 'A Structural / UML', 'domain+port+adapter', 'g6', 'Native', 'concentric', 'CAP-12.1'),
  // ── B Behavioral / UML (7) ───────────────────────────────────────────
  e('B1', 'Activity / swimlane', 'B Behavioral / UML', 'action+decision → control flow', 'g6', 'Native', 'combo-combined', 'CAP-12.5'),
  e('B2', 'State machine', 'B Behavioral / UML', 'state+initial+final → transitions', 'g6', 'Native', 'dagre', 'CAP-1.6'),
  e('B3', 'Sequence', 'B Behavioral / UML', 'lifeline+message → ordered', 'mermaid', 'Alt-engine', 'sequence', 'CAP-17.7, RT-05'),
  e('B4', 'Communication', 'B Behavioral / UML', 'object+message', 'g6', 'Native', 'force', 'CAP-17.7'),
  e('B5', 'Interaction overview', 'B Behavioral / UML', 'frame → flow', 'g6', 'Native', 'dagre', 'CAP-17.7'),
  e('B6', 'Timing', 'B Behavioral / UML', 'lifeline+state band', 'custom-svg', 'Alt-engine', 'timeline', 'CAP-4.7'),
  e('B7', 'BPMN', 'B Behavioral / UML', 'task+gateway+event+pool', 'g6', 'Native', 'combo-combined', 'CAP-12.5'),
  // ── C Architecture (9) ───────────────────────────────────────────────
  e('C1', 'Event-driven / message flow', 'C Architecture', 'producer+topic+consumer', 'g6', 'Native', 'dagre', 'CAP-1.10'),
  e('C2', 'Saga / compensation', 'C Architecture', 'step+compensation → forward, compensate', 'g6', 'Native', 'dagre', 'CAP-13.1'),
  e('C3', 'Outbox', 'C Architecture', 'aggregate+outbox+relay', 'g6', 'Native', 'dagre', 'CAP-1.10'),
  e('C4', 'Event storming', 'C Architecture', 'command+aggregate+event+policy', 'g6', 'Native', 'grid', 'CAP-12.7'),
  e('C5', 'Domain model / bounded contexts', 'C Architecture', 'aggregate+entity+value', 'g6', 'Native', 'combo-combined', 'CAP-12.1'),
  e('C6', 'Microservices graph', 'C Architecture', 'service → HTTP, message', 'g6', 'Native', 'force', 'CAP-9.6, CAP-17.8'),
  e('C7', 'Service dependency', 'C Architecture', 'service → depends-on', 'g6', 'Native', 'dagre', 'CAP-9.6'),
  e('C8', 'Feature module map', 'C Architecture', 'feature+module', 'g6', 'Native', 'grid', 'CAP-14.1'),
  e('C9', 'Modular monolith', 'C Architecture', 'module → allowed/forbidden', 'g6', 'Native', 'combo-combined', 'CAP-12.2'),
  // ── D Data (6) ───────────────────────────────────────────────────────
  e('D1', 'ER diagram', 'D Data', 'entity+attribute → relation', 'g6', 'Native', 'dagre', 'CAP-7.8'),
  e('D2', 'Data flow (DFD)', 'D Data', 'process+store+external → data flow', 'g6', 'Native', 'dagre', 'CAP-2.1, CAP-7.3'),
  e('D3', 'Data lineage', 'D Data', 'field+writer+reader', 'g6', 'Native', 'dagre', 'CAP-2.1'),
  e('D4', 'Schema evolution', 'D Data', 'table+column → added, removed', 'g6', 'Native', 'dagre timeline', 'CAP-7.8'),
  e('D5', 'Table relationship matrix', 'D Data', 'table × table', 'g6', 'Native', 'grid', 'CAP-7.8'),
  e('D6', 'Data mutation flow', 'D Data', 'mutation+field', 'g6', 'Native', 'dagre', 'CAP-2.1'),
  // ── E Code-level (10) ────────────────────────────────────────────────
  e('E1', 'Call graph', 'E Code-level', 'function → calls (tier)', 'g6', 'Native', 'dagre', 'CAP-1.3, CAP-10.2'),
  e('E2', 'CFG', 'E Code-level', 'block → branch', 'g6', 'Native', 'dagre', 'CAP-4.1, CAP-13.4'),
  e('E3', 'PDG', 'E Code-level', 'statement → data+control dep', 'g6', 'Native', 'force', 'CAP-13.2'),
  e('E4', 'Data flow / def-use', 'E Code-level', 'def+use → reaching-def', 'g6', 'Native', 'dagre', 'CAP-2.1'),
  e('E5', 'Module dependency', 'E Code-level', 'module → imports', 'g6', 'Native', 'dagre', 'CAP-1.3'),
  e('E6', 'Dead code reachability', 'E Code-level', 'function+entry → reachable', 'g6', 'Native', 'radial', 'CAP-1.4'),
  e('E7', 'Hot path', 'E Code-level', 'function → frequency-weighted call', 'g6', 'Native', 'force', 'CAP-4.7'),
  e('E8', 'Import/export graph', 'E Code-level', 'file → imports, re-exports', 'g6', 'Native', 'dagre', 'CAP-1.8'),
  e('E9', 'IR/pseudocode structure', 'E Code-level', 'block+instruction', 'g6', 'Native', 'compact-box', 'ANL-05'),
  e('E10', 'Concept atlas', 'E Code-level', 'cluster → similarity', 'g6', 'Native', 'fruchterman', 'CAP-11.1'),
  // ── F Runtime (7) ────────────────────────────────────────────────────
  e('F1', 'Trace waterfall', 'F Runtime', 'span → parent-child, time', 'custom-svg', 'Alt-engine', 'timeline', 'CAP-8.1, CAP-8.5'),
  e('F2', 'Flame graph', 'F Runtime', 'stack frame → contains', 'g6', 'Custom', 'cie-flame', 'CAP-4.7, ANL-15'),
  e('F3', 'Runtime overlay', 'F Runtime', 'architecture tinted by runtime', 'g6', 'Native', 'dagre', 'CAP-8.7'),
  e('F4', 'Concurrency / race windows', 'F Runtime', 'lane+statement → interleaving', 'g6', 'Native', 'combo-combined', 'CAP-13.5'),
  e('F5', 'Lock ordering', 'F Runtime', 'lock → ordering', 'g6', 'Native', 'dagre', 'CAP-13.5'),
  e('F6', 'Resource lifecycle', 'F Runtime', 'acquire+release', 'g6', 'Native', 'dagre', 'CAP-4.4'),
  e('F7', 'Incident timeline', 'F Runtime', 'event → occurred-before', 'g6', 'Native', 'dagre timeline', 'CAP-8.5, CAP-17.7'),
  // ── G Quality (5) ────────────────────────────────────────────────────
  e('G1', 'Test-confidence matrix', 'G Quality', 'behavior × test', 'g6', 'Native', 'grid', 'CAP-3.1'),
  e('G2', 'Test guarantee matrix', 'G Quality', 'behavior × guarantee', 'g6', 'Native', 'grid', 'CAP-3.4'),
  e('G3', 'Policy enforcement matrix', 'G Quality', 'route × rule', 'g6', 'Native', 'grid', 'CAP-7.5'),
  e('G4', 'Idempotency matrix', 'G Quality', 'operation × retry', 'g6', 'Native', 'grid', 'CAP-13.1'),
  e('G5', 'Coverage map', 'G Quality', 'file → coverage', 'g6', 'Native', 'grid/treemap', 'CAP-3.1'),
  // ── H Decision (5) ───────────────────────────────────────────────────
  e('H1', 'Decision table', 'H Decision', 'condition × action', 'g6', 'Native', 'grid', 'CAP-12.7'),
  e('H2', 'FMEA table', 'H Decision', 'mode+severity+detection+occurrence', 'g6', 'Native', 'grid', 'CAP-13.5'),
  e('H3', 'Metrics map', 'H Decision', 'metric × scope', 'g6', 'Native', 'grid', 'CAP-4.1'),
  e('H4', 'Counterfactual overlay', 'H Decision', 'current+hypothetical', 'g6', 'Native', 'dagre', 'CAP-17.8'),
  e('H5', 'Change-risk terrain', 'H Decision', 'file cell weighted', 'g6', 'Native', 'treemap', 'CAP-10.4'),
  // ── I Epistemic (7) ──────────────────────────────────────────────────
  e('I1', 'Hypothesis graph', 'I Epistemic', 'hypothesis+evidence → supports, contradicts', 'g6', 'Native', 'force', 'CAP-8.5, INV-07'),
  e('I2', 'Failure space / invariant map', 'I Epistemic', 'failure+invariant', 'g6', 'Native', 'force', 'CAP-13.2'),
  e('I3', 'Trust boundary', 'I Epistemic', 'zone+gate+resource', 'g6', 'Native', 'combo-combined', 'CAP-2.8'),
  e('I4', 'Archaeology chain', 'I Epistemic', 'commit+change → caused', 'g6', 'Native', 'dagre timeline', 'CAP-15.1'),
  e('I5', 'Semantic diff timeline', 'I Epistemic', 'before+after', 'g6', 'Native', 'dagre', 'CAP-1.10'),
  e('I6', 'Concept atlas', 'I Epistemic', 'concept → similarity', 'g6', 'Native', 'fruchterman', 'FR-40–44'),
  e('I7', 'Concept hierarchy', 'I Epistemic', 'concept → contains', 'g6', 'Native', 'compact-box, mindmap', 'FR-40–44'),
  // ── J Journey (4) ────────────────────────────────────────────────────
  e('J1', 'Transaction journey', 'J Journey', 'step+branch → ordered', 'g6', 'Native', 'combo-combined', 'CAP-1.10'),
  e('J2', 'User journey', 'J Journey', 'step+branch (user-facing)', 'g6', 'Native', 'combo-combined', 'CAP-14.2'),
  e('J3', 'API journey', 'J Journey', 'request+handler+service+repo', 'g6', 'Native', 'dagre', 'CAP-1.10'),
  e('J4', 'Process orchestration', 'J Journey', 'activity+task', 'g6', 'Native', 'dagre', 'CAP-12.5'),
  // ── K Organizational (3) ─────────────────────────────────────────────
  e('K1', 'Ownership map', 'K Organizational', 'file+owner → owned-by', 'g6', 'Native', 'grid', 'CAP-6.5'),
  e('K2', 'Knowledge distribution', 'K Organizational', 'file+knowledge density', 'g6', 'Native', 'grid', 'CAP-6.5'),
  e('K3', 'Concept hierarchy (2-axis)', 'K Organizational', 'containment × specialization', 'g6', 'Native', 'compact-box', 'FR-40–44'),
];

export const CATALOG_SUMMARY = {
  total: CATALOG.length,
  native: CATALOG.filter((c) => c.g6Status === 'Native').length,
  custom: CATALOG.filter((c) => c.g6Status === 'Custom').length,
  altEngine: CATALOG.filter((c) => c.g6Status === 'Alt-engine').length,
  extensionBudget: { customNodes: 4, customEdges: 2, customLayouts: 2, customPlugins: 2 },
};
