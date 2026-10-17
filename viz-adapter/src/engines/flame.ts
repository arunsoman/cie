/**
 * Flame graph as G6 extensions (PRD Part V) — the proof that the
 * architecture generalizes: a chart G6 does not ship is ~60 lines of
 * registered extension code with NO schema change and NO adapter change.
 *
 * `computeFlameGeometry` is the pure core of the `cie-flame` custom
 * layout (rect-in-rect hierarchical packing — NOT a graph layout), fully
 * headless-testable:
 *   root width == canvas width
 *   child widths proportional to samples
 *   siblings pack left-to-right without overlap
 *   depth → y monotone
 * The G6 wrapper (`registerFlameExtensions`) binds this to
 * `register(ExtensionCategory.LAYOUT, 'cie-flame', …)` and
 * `register(ExtensionCategory.NODE, 'cie-flame-frame', …)` in a host page.
 */

export interface FlameNodeIn {
  id: string;
  label: string;
  displayMode: string;
  samples: number;
}

export interface FlameEdgeIn {
  source: string;
  target: string;
}

export interface FlameFrame {
  id: string;
  x: number; // left edge
  y: number; // top edge
  width: number;
  height: number;
  depth: number;
}

export interface FlameGeometry {
  root: string;
  frames: Record<string, FlameFrame>;
  order: string[]; // depth-first draw order
  canvasWidth: number;
  levelHeight: number;
}

/**
 * Pack stack frames: every node's width ∝ samples/rootSamples * canvasW;
 * position determined by the cumulative widths of siblings to the left.
 * Deterministic: children in edge insertion order.
 */
export function computeFlameGeometry(
  nodes: FlameNodeIn[],
  edges: FlameEdgeIn[],
  canvasWidth = 960,
  levelHeight = 24,
): FlameGeometry {
  const childrenOf: Record<string, string[]> = {};
  const parentOf: Record<string, string> = {};
  for (const e of edges) {
    (childrenOf[e.source] ??= []).push(e.target);
    parentOf[e.target] = e.source;
  }
  const byId = new Map(nodes.map((n) => [n.id, n]));
  const root = nodes.find((n) => !(n.id in parentOf));
  if (!root || root.samples <= 0) {
    return { root: root?.id ?? '', frames: {}, order: [], canvasWidth, levelHeight };
  }

  const frames: Record<string, FlameFrame> = {};
  const order: string[] = [];
  const visit = (id: string, depth: number, xOffset: number): void => {
    const node = byId.get(id);
    if (!node) return;
    const width = (node.samples / root.samples) * canvasWidth;
    frames[id] = {
      id,
      x: xOffset,
      y: depth * levelHeight,
      width,
      height: levelHeight,
      depth,
    };
    order.push(id);
    let childX = xOffset;
    for (const child of childrenOf[id] ?? []) {
      const childNode = byId.get(child);
      if (!childNode) continue;
      const childW = (childNode.samples / root.samples) * canvasWidth;
      visit(child, depth + 1, childX);
      childX += childW;
    }
  };
  visit(root.id, 0, 0);
  return { root: root.id, frames, order, canvasWidth, levelHeight };
}

/**
 * The epistemic law is preserved on flame frames (PRD Part V §14):
 * stroke + dash come from displayMode, fill from depth.
 */
export const FLAME_STROKE: Record<string, string> = {
  Fact: '#2ea043',
  Inference: '#d29922',
  Hypothesis: '#f85149',
  Fog: '#8b949e',
};

export const FLAME_DASH: Record<string, number[] | undefined> = {
  Fact: undefined,
  Inference: [6, 4],
  Hypothesis: [2, 4],
  Fog: [1, 3],
};

/** Depth → heat colour (pure). */
export function depthColor(depth: number): string {
  const t = Math.min(depth, 8) / 8;
  const r = Math.round(22 + t * 110);
  const g = Math.round(27 + t * 40);
  const b = Math.round(34 + t * 10);
  return `rgb(${r},${g},${b})`;
}

/**
 * Host-page registration snippet (PRD §12–13): binds the pure geometry to
 * G6's extension registry. Deliberately NOT executed headless — it is the
 * only engine-coupled line of the flame graph.
 */
export const FLAME_REGISTRATION_SNIPPET = `
import { register, ExtensionCategory, BaseLayout, Rect } from '@antv/g6';
import { computeFlameGeometry, FLAME_STROKE, FLAME_DASH, depthColor } from 'cie-viz/engines/flame';

class FlameLayout extends BaseLayout {
  id = 'cie-flame';
  async execute(model) {
    const geo = computeFlameGeometry(
      model.nodes.map(n => ({ id: n.id, label: n.data.label, displayMode: n.data.displayMode,
                              samples: n.data.fields.samples ?? 0 })),
      model.edges.map(e => ({ source: e.source, target: e.target })),
      this.canvasWidth ?? 960, this.levelHeight ?? 24);
    for (const n of model.nodes) {
      const f = geo.frames[n.id];
      if (!f) continue;
      n.style.x = f.x + f.width / 2;            // G6 uses center-x
      n.style.y = f.y + f.height / 2;
      n.style.size = [f.width, f.height];
    }
  }
}
register(ExtensionCategory.LAYOUT, 'cie-flame', FlameLayout);

class FlameFrame extends Rect { /* label truncation per PRD §13 */ }
register(ExtensionCategory.NODE, 'cie-flame-frame', FlameFrame);
`;
