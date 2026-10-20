import re

p = '/home/z/my-project/web/src/components/cie/selected-context-panel.tsx'
s = open(p, encoding='utf-8').read()

# --- FlowChart builder: use the call graph byId map + string callees ---
old_flow = '''  // chips first (in pick order), then their recorded call neighbourhood
  for (const c of chips) addNode(c.id, c.label, "function");
  for (const s of report.summaries ?? []) {
    const caller = s.qual_name || s.name;
    if (!selectedQuals.has(caller)) continue;
    for (const callee of s.callees ?? []) {
      const calleeSym = report.symbols.find((x) => x.id === callee);
      const calleeQual = calleeSym?.qual_name || calleeSym?.name;
      if (!calleeQual) continue; // unresolved — never an edge
      addNode(calleeQual, calleeSym.name, "function");
      const key = `${caller}->${calleeQual}`;
      if (!seen.has(key)) {
        seen.add(key);
        links.push({ from: caller, to: calleeQual, label: "calls", certainty: "Fact" });
      }
    }
  }'''
new_flow = '''  // chips first (in pick order), then their recorded call neighbourhood.
  // summaries.callees are RESOLVED qual names (strings); unresolved ones
  // live in unresolved_callees and never become edges.
  const g = buildCallGraph(report);
  const qualOf = (symId: number): string | null => {
    const sym = g.byId.get(symId);
    return sym ? sym.qual_name || sym.name : null;
  };
  for (const c of chips) addNode(c.id, c.label, "function");
  for (const sum of report.summaries ?? []) {
    const caller = qualOf(Number(sum.sym));
    if (!caller || !selectedQuals.has(caller)) continue;
    for (const calleeQual of sum.callees ?? []) {
      if (!calleeQual) continue; // unresolved — never an edge
      const calleeSym = symbolByQualName(report, calleeQual);
      addNode(calleeQual, calleeSym?.name ?? calleeQual, "function");
      const key = `${caller}->${calleeQual}`;
      if (!seen.has(key)) {
        seen.add(key);
        links.push({ from: caller, to: calleeQual, label: "calls", certainty: "Fact" });
      }
    }
  }'''
assert old_flow in s, "flow block"
s = s.replace(old_flow, new_flow)

# --- SequenceChart builder ---
old_seq = '''  const msgs: ChatChartSpec["msgs"] = [];
  const incoming = new Map<string, number>();
  for (const s of report.summaries ?? []) {
    const caller = s.qual_name || s.name;
    if (!selected.has(caller)) continue;
    for (const callee of s.callees ?? []) {
      const cs = report.symbols.find((x) => x.id === callee);
      const cq = cs?.qual_name || cs?.name;
      if (!cq || !selected.has(cq) || cq === caller) continue;
      msgs.push({ from: caller, to: cq, label: "calls", status: "recorded" });
      incoming.set(cq, (incoming.get(cq) ?? 0) + 1);
    }
  }'''
new_seq = '''  const msgs: ChatChartSpec["msgs"] = [];
  const incoming = new Map<string, number>();
  for (const sum of report.summaries ?? []) {
    const cs = report.symbols.find((x) => x.id === Number(sum.sym));
    const caller = cs?.qual_name || cs?.name;
    if (!caller || !selected.has(caller)) continue;
    for (const cq of sum.callees ?? []) {
      if (!cq || !selected.has(cq) || cq === caller) continue;
      msgs.push({ from: caller, to: cq, label: "calls", status: "confirmed" });
      incoming.set(cq, (incoming.get(cq) ?? 0) + 1);
    }
  }'''
assert old_seq in s, "seq block"
s = s.replace(old_seq, new_seq)

# --- imports: buildCallGraph + symbolByQualName from atlas ---
old_imp = 'import type { CieReport, SymbolInfo } from "@/lib/cie-types";'
new_imp = 'import type { CieReport } from "@/lib/cie-types";\nimport { buildCallGraph, symbolByQualName } from "@/lib/atlas";'
assert old_imp in s
s = s.replace(old_imp, new_imp)

# drop the local symbolOf duplicate
s = s.replace('''export function symbolOf(report: CieReport, qual: string): SymbolInfo | null {
  const s = report.symbols.find(
    (x) => x.qual_name === qual || x.qual_name === `${qual}()` || x.name === qual
  );
  return s ?? null;
}

''', '')
s = s.replace('''    const sym = symbolOf(report, qual);
''', '''    const sym = symbolByQualName(report, qual);
''')

open(p, 'w', encoding='utf-8').write(s)
print("rewritten")
