"use strict";
// The three renderers consume this same projection. No renderer performs retrieval.
const Evidence = {
  project(answer, filter = "", hidden = new Set()) {
    const term = filter.toLowerCase();
    const items = answer.items.filter(i => !hidden.has(i.id) && `${i.label} ${i.kind} ${i.claimClass} ${i.detail?.queryDecision || ""}`.toLowerCase().includes(term));
    const ids = new Set(items.map(i => i.id));
    return {items, edges: answer.edges.filter(e => ids.has(e.from) && ids.has(e.to))};
  },
  layout(population) {
    // Nodes occupy disjoint rows. All edges route outside the node column, so
    // they cannot cross unrelated nodes. Shared lanes/crossings remain disclosed.
    const nodes = population.items.map((item, index) => ({...item, x: 30, y: 30 + index * 80, width: 420, height: 58}));
    const byId = new Map(nodes.map(n => [n.id, n]));
    const edges = population.edges.map((e, index) => {
      const a = byId.get(e.from), b = byId.get(e.to), lane = 480 + (index % 16) * 12;
      const y1 = a.y + 20, y2 = b.y + 40;
      return {...e, path: `M450 ${y1} H${lane} V${y2} H450`};
    });
    return {nodes, edges, width: 720, height: Math.max(160, nodes.length * 80 + 40)};
  },
  markdown(rendered) {
    const a = rendered.answer;
    const clean = s => String(s).replace(/[\r\n]/g, " ").replace(/[`<>]/g, "");
    return [`# ${clean(a.question)}`, "", `Repository: ${a.repositoryId} · revision: ${a.revisionId}`,
      `Status: ${a.status} · freshness: ${rendered.freshness} · challenged: ${rendered.challenged}`, rendered.freshnessScope || "Freshness concerns the indexed supported source snapshot only.", "", a.summary,
      "", "## Evidence gaps", ...a.gaps.map(g => `- ${clean(g)}`), "", "## Items",
      ...a.items.flatMap(i => [`- ${clean(i.label)} [${i.claimClass}]${i.detail?.queryDecision ? ` [query decision: ${i.detail.queryDecision}]` : ""}`, ...i.citations.map(c => `  - ${clean(c.file)}:${c.startLine}–${c.endLine}; snapshot ${c.revisionId}; hash ${c.sourceHash}`)]),
      "", "## Relationships", ...a.edges.map(e => `- ${e.from} → ${e.to}: ${e.kind} [${e.claimClass}]`)].join("\n");
  }
};
if (typeof module !== "undefined") module.exports = Evidence;
if (typeof document !== "undefined") boot();
function boot() {
  const $ = id => document.getElementById(id);
  const token = document.querySelector('meta[name="cie-token"]').content;
  let state = {repositories: [], answers: [], jobs: [], workspaces: [], features: []};
  let capabilities = {stories: []}, active = null, selected = null, parent = null, representation = "table", filter = "", hidden = new Set(), polling = false;
  let openAnswers = null;
  const el = (tag, text, cls) => {const n = document.createElement(tag); if (text !== undefined) n.textContent = text; if (cls) n.className = cls; return n;};
  const button = (label, action) => {const b = el("button", label); b.type = "button"; b.addEventListener("click", () => run(action)); return b;};
  function announce(message) {$("notice").textContent = message;}
  function fail(error) {$("error").hidden = false; $("error").textContent = error.message || "The operation could not complete. Your saved answers remain available.";}
  async function run(action) {$("error").hidden = true; try {await action();} catch (error) {fail(error);}}
  async function api(path, method = "GET", data) {
    const controller = new AbortController(), timeout = setTimeout(() => controller.abort(), 30000);
    try {
      const response = await fetch(path, {method, headers: {"X-Cie-Token": token, ...(data !== undefined ? {"Content-Type": "application/json"} : {})}, body: data === undefined ? undefined : JSON.stringify(data), signal: controller.signal});
      const result = await response.json();
      if (!response.ok) throw new Error(`${result.error?.code || response.status}: ${result.error?.message || "Request failed"}`);
      if(method === "DELETE") window.cieNotifyDeletion?.();
      return result;
    } catch (error) {
      if (error.name === "AbortError") throw new Error("The request timed out. Saved answers are preserved. Refresh state before retrying a write; it may have completed.");
      throw error;
    } finally {clearTimeout(timeout);}
  }
  function page(name) {document.querySelectorAll('[id^="page-"]').forEach(n => {n.hidden = n.id !== `page-${name}`;}); document.querySelectorAll("[data-page]").forEach(b => b.setAttribute("aria-pressed", String(b.dataset.page === name)));}
  document.querySelectorAll("[data-page]").forEach(b => b.addEventListener("click", () => page(b.dataset.page)));
  function setTheme(value) {document.documentElement.dataset.theme = value; $("theme").value = value; try {localStorage.setItem("cie-theme", value);} catch (_) {/* Storage unavailable: current session still works. */}}
  let theme = "light"; try {theme = localStorage.getItem("cie-theme") || theme;} catch (_) {}
  setTheme(["light", "dark", "contrast"].includes(theme) ? theme : "light");
  $("theme").addEventListener("change", e => setTheme(e.target.value));
  async function refresh() {
    state = await api("/api/state");
    if (openAnswers === null) openAnswers = new Set(state.workspaces[0]?.answerIds || []);
    for (const id of openAnswers) if (!state.answers.some(a => a.id === id)) openAnswers.delete(id);
    renderState();
  }
  function renderState() {
    for (const id of ["repo-select", "feature-repo"]) {
      const select = $(id), prior = select.value; select.replaceChildren();
      for (const r of state.repositories) {const o = el("option", `${r.name} · ${r.revisions.length ? r.revisions.at(-1).id : "not indexed"}`); o.value = r.id; select.append(o);}
      if (state.repositories.some(r => r.id === prior)) select.value = prior;
    }
    $("repositories").replaceChildren();
    if (!state.repositories.length) $("repositories").append(el("p", "No repositories connected.", "muted"));
    for (const r of state.repositories) {
      const card = el("div", undefined, "card"); card.append(el("strong", r.name), el("small", r.path), el("p", r.revisions.length ? `${r.revisions.length} snapshots · ${r.revisions.at(-1).symbols} symbols` : "Not indexed yet"));
      const busy = state.jobs.some(j => j.repositoryId === r.id && ["queued", "running", "cancelling"].includes(j.state));
      const index = button(busy ? "Index job active" : "Index snapshot", async () => {await api(`/api/repositories/${r.id}/index`, "POST", {}); await refresh(); announce("Index job queued. Results publish only after completion and freshness checks.");}); index.disabled = busy;
      card.append(index, button("Inspect scope", () => inspectRepository(r.id)), button("Delete derived records", async () => {if (!confirm(`Delete derived records for ${r.name}? Source files remain unchanged. Open workspace tabs will refresh to clear cached evidence.`)) return; await api(`/api/repositories/${r.id}`, "DELETE"); if (active?.answer.repositoryId === r.id) {active = null; $("answer").replaceChildren(el("p", "Repository evidence was deleted. No saved conclusion is displayed."));} await refresh();})); $("repositories").append(card);
    }
    $("jobs").replaceChildren();
    for (const j of state.jobs.slice(-8).reverse()) {
      const card = el("div", undefined, "card"); card.append(el("strong", `${j.id} · ${j.state}`), el("small", `${j.stage} · ${j.completedUnits}/${j.totalUnits} stages`));
      const progress = el("progress"); progress.max = j.totalUnits; progress.value = j.completedUnits; progress.setAttribute("aria-label", `${j.id} stage progress`); card.append(progress);
      if (j.issue) card.append(el("p", j.issue));
      if (["queued", "running", "cancelling"].includes(j.state)) {const cancel = button("Cancel publication", async () => {await api(`/api/jobs/${j.id}/cancel`, "POST", {}); await refresh();}); cancel.disabled = j.state === "cancelling"; card.append(cancel);}
      $("jobs").append(card);
    }
    if (!state.jobs.length) $("jobs").append(el("p", "No background work.", "muted"));
    $("workspaces").replaceChildren();
    for (const w of state.workspaces) $("workspaces").append(button(`${w.name} (${w.answerIds.length})`, async () => {openAnswers = new Set(w.answerIds); renderTabs(); const id = w.activeAnswer || w.answerIds[0]; if (id) await loadAnswer(id); else {active = null; $("answer").replaceChildren(el("p", "This workspace has no saved answers."));}}));
    renderTabs(); renderFeatures();
  }
  function renderTabs() {
    $("answer-tabs").replaceChildren();
    for (const a of state.answers.filter(a => openAnswers.has(a.id))) {
      const b = button(`${a.question} · ${a.status}`, () => loadAnswer(a.id)); b.setAttribute("aria-pressed", String(active?.answer.id === a.id)); $("answer-tabs").append(b);
    }
  }
  async function inspectRepository(id) {const r = await api(`/api/repositories/${id}`); $("readiness").replaceChildren(el("pre", JSON.stringify(r, null, 2))); page("investigate"); announce(`Scope for ${r.name}: ${r.freshness}, ${r.symbolCount} symbols.`);}
  $("connect").addEventListener("submit", event => {event.preventDefault(); run(async () => {const r = await api("/api/repositories", "POST", {path: $("repo-path").value, ...($("repo-name").value.trim() ? {name: $("repo-name").value.trim()} : {})}); await refresh(); $("repo-select").value = r.id; announce(`Connected ${r.name}. Index it to make evidence available.`);});});
  function renderContext() {$("context").replaceChildren(); if (parent) $("context").append(button(`Follow-up: ${parent} · ${active?.answer.revisionId || "snapshot unavailable"} × clear`, () => {parent = null; renderContext();})); if (selected) $("context").append(button(`Selected: ${selected.label} · ${active?.answer.revisionId || "snapshot unavailable"} × clear`, () => {selected = null; renderContext();})); if (parent || selected) $("context").append(button("Clear all context", () => {parent=null;selected=null;renderContext();}));}
  $("ask").addEventListener("submit", event => {event.preventDefault(); run(async () => {const submit = $("ask-button"); submit.disabled = true; announce("Reading indexed evidence…"); try {const body = {repositoryId: $("repo-select").value, question: $("question").value}; if ($("intent").value) body.intent = $("intent").value; if (selected?.id.startsWith("symbol:")) body.symbolId = selected.id.slice(7); else if (selected?.detail?.symbolId) body.symbolId = selected.detail.symbolId; if (parent) body.parentAnswer = parent; if ((parent || selected) && active?.answer.repositoryId === body.repositoryId) body.expectedRevision = active.answer.revisionId; const result = await api("/api/questions", "POST", body); openAnswers.add(result.answer.id); await refresh(); showAnswer(result); announce(`Answer ${result.answer.status}; ${result.answer.items.length} evidence items.`);} finally {submit.disabled = false;}});});
  $("repo-select").addEventListener("change", () => {selected = null; parent = null; renderContext(); $("readiness").replaceChildren();});
  async function loadAnswer(id) {showAnswer(await api(`/api/answers/${id}`)); renderTabs();}
  function download(name, content, type) {const url = URL.createObjectURL(new Blob([content], {type})); const a = el("a"); a.href = url; a.download = name; a.click(); setTimeout(() => URL.revokeObjectURL(url), 1000);}
  function showAnswer(result) {active = result; filter = ""; hidden = new Set(); selected = null; representation = result.answer.preferredView === "graph" ? "graph" : "table"; $("repo-select").value = result.answer.repositoryId; parent = result.answer.id; renderContext(); renderAnswer(); renderTabs(); page("investigate");}
  function renderAnswer() {
    if (!active) return;
    const a = active.answer, box = $("answer"); box.replaceChildren(el("h2", a.question));
    const status = el("p", `${a.status} · freshness: ${active.freshness} · ${active.challenged ? "challenged" : "no recorded challenge"}`, active.freshness !== "current" || active.challenged ? "warning" : "badge"); box.append(status, el("p", active.freshnessScope || "Freshness concerns the indexed supported source snapshot only.", "muted"), el("p", `Repository ${a.repositoryId} · snapshot ${a.revisionId}`, "code-location"), el("p", `Interpretation: ${a.interpretation}`), el("p", a.summary));
    if (a.gaps.length) {const d = el("details"); d.open = true; d.append(el("summary", `Evidence gaps (${a.gaps.length})`)); const list = el("ul"); a.gaps.forEach(g => list.append(el("li", g))); d.append(list); box.append(d);}
    if (active.hiddenClaims) box.append(el("p", `${active.hiddenClaims} refuted claims withheld; historical evidence retained.`, "warning"));
    const actions = el("div", undefined, "view-tools");
    actions.append(button("Recheck freshness", () => loadAnswer(a.id)), button("Export JSON", async () => {const current = await api(`/api/answers/${a.id}/export`); download(`${a.id}.json`, JSON.stringify(current, null, 2), "application/json");}), button("Export Markdown", async () => {const current = await api(`/api/answers/${a.id}/export`); download(`${a.id}.md`, Evidence.markdown(current), "text/markdown");}), button("Close tab", () => {openAnswers.delete(a.id); active = null; parent = null; selected = null; box.replaceChildren(el("p", "Answer closed. It remains in saved workspaces.")); renderTabs(); renderContext();}), button("Delete saved answer", async () => {if (!confirm("Delete this answer and its reviews? Open workspace tabs will refresh.")) return; await api(`/api/answers/${a.id}`, "DELETE"); active = null; parent = null; selected = null; box.replaceChildren(el("p", "Answer deleted.")); await refresh(); renderContext();})); box.append(actions);
    if (a.intent === "concepts") actions.append(button("Concept judgments and engine scores", async () => {
      const report = await api(`/api/concept-reviews/${a.repositoryId}/${a.revisionId}`);
      document.getElementById("concept-judgments")?.remove();
      const panel=el("section");panel.id="concept-judgments";panel.append(el("h3","Engine scores versus your judgments"),el("p",`${report.reviewedUniqueCards} unique cards reviewed; minimum ${report.minimumForAgreementSummary} for agreement summaries. Calibrated probability: not available.${report.cardsCapped ? " Card display capped at 500." : ""}`));
      report.gaps.forEach(g=>panel.append(el("p",g,"muted")));
      if(report.agreementSummaryAvailable)for(const bin of report.bins.filter(b=>b.displayable))panel.append(el("p",`Score ${bin.lowerScore.toFixed(1)}–${bin.upperScore.toFixed(1)}: ${bin.confirm} confirm, ${bin.dispute} dispute, ${bin.refute} refute (${bin.reviewed} judgments). User agreement, not expert accuracy.`));
      for(const card of report.cards){const row=el("div",undefined,"card");row.append(el("strong",card.label),el("p",`Uncalibrated engine score: ${card.uncalibratedScore}; judgment: ${card.judgment || "not reviewed"}`));if(card.reason)row.append(el("p",card.reason));row.append(button("Inspect or change concept review",()=>inspect({id:card.id,label:card.label,kind:"inferredConcept",claimClass:"analyzerInference",citations:card.citations,detail:{uncalibratedScore:card.uncalibratedScore,scope:"A judgment or label does not establish business truth."}})));panel.append(row);}box.append(panel);
    }));
    const choices = el("div", undefined, "view-tools");
    for (const view of ["table", "graph", "outline"]) {const b = button(view[0].toUpperCase() + view.slice(1), () => {representation = view; renderAnswer();}); b.setAttribute("aria-pressed", String(view === representation)); choices.append(b);} box.append(choices);
    const filterLabel = el("label", "Filter this answer's population"); filterLabel.htmlFor = "population-filter";
    const input = el("input"); input.id = "population-filter"; input.value = filter; input.addEventListener("input", () => {filter = input.value; renderPopulation();}); box.append(filterLabel, input, el("div", undefined, "legend"));
    box.lastChild.textContent = "[Code observation] cited static source · [Analyzer inference] interpretation · dashed borders mark inference. Confirming never upgrades evidence.";
    const content = el("div"); content.id = "population"; box.append(content); renderPopulation();
  }
  function renderPopulation() {
    const box = $("population"); if (!box || !active) return; box.replaceChildren();
    const p = Evidence.project(active.answer, filter, hidden);
    box.append(el("p", `${p.items.length} of ${active.answer.items.length} answer items · ${p.edges.length} represented relationships · ${hidden.size} locally hidden`));
    if (hidden.size) box.append(button("Reveal locally hidden items", () => {hidden.clear(); renderPopulation();}));
    if (!p.items.length) {box.append(el("p", "No matching items in this view. This does not establish absence outside the indexed scope.", "empty")); return;}
    if (representation === "table") table(p, box);
    else if (representation === "outline") outline(p, box);
    else graph(p, box);
  }
  function itemButton(item) {return button(item.label, () => inspect(item));}
  function table(p, box) {
    const wrap = el("div", undefined, "table-wrap"), t = el("table"), caption = el("caption", "Evidence population with static relationships"); t.append(caption);
    const head = el("thead"), row = el("tr"); ["Element", "Kind", "How known", "Query membership", "Connections", "Focus"].forEach(text => {const th = el("th", text); th.scope = "col"; row.append(th);}); head.append(row); t.append(head);
    const body = el("tbody");
    for (const i of p.items) {const tr = el("tr"); const name = el("td"); name.append(itemButton(i)); tr.append(name, el("td", i.kind), el("td", i.claimClass), el("td", i.detail?.queryDecision || "Not filtered"), el("td", p.edges.filter(e => e.from === i.id || e.to === i.id).map(e => `${e.from} → ${e.to} [${e.kind}]`).join("; ") || "No represented links")); const focus = el("td"); focus.append(button("Hide locally", () => {hidden.add(i.id); renderPopulation();})); tr.append(focus); body.append(tr);} t.append(body); wrap.append(t); box.append(wrap);
  }
  function outline(p, box) {
    const list = el("ol", undefined, "outline");
    for (const i of p.items) {const li = el("li"); li.append(itemButton(i), el("span", ` — ${i.kind}; ${i.claimClass}${i.detail?.queryDecision ? `; query ${i.detail.queryDecision}` : ""}`)); const edges = p.edges.filter(e => e.from === i.id); if (edges.length) {const links = el("ul"); edges.forEach(e => links.append(el("li", `${e.kind} → ${p.items.find(n => n.id === e.to)?.label || e.to} [${e.claimClass}]`))); li.append(links);} list.append(li);} box.append(list);
  }
  function graph(p, box) {
    const layout = Evidence.layout(p), ns = "http://www.w3.org/2000/svg", svgEl = (tag, attrs) => {const e = document.createElementNS(ns, tag); for (const [k,v] of Object.entries(attrs)) e.setAttribute(k, String(v)); return e;};
    box.append(el("p", "Layout: fixed readable rows; edges route outside unrelated nodes. Edge lanes may overlap. Positions carry no causal or importance meaning. Scroll to reach all items.", "legend"));
    const wrap = el("div", undefined, "graph-wrap"), svg = svgEl("svg", {width: layout.width, height: layout.height, tabindex: 0, role: "group", "aria-label": `Evidence graph: ${p.items.length} nodes, ${p.edges.length} links. Arrow keys select; Enter inspects. Table and Outline contain every item and link.`});
    layout.edges.forEach(e => {const path = svgEl("path", {d:e.path}); const title = svgEl("title", {}); title.textContent = `${e.from} → ${e.to}: ${e.kind} [${e.claimClass}]`; path.append(title); svg.append(path);});
    const groups = layout.nodes.map(n => {const g = svgEl("g", {class:n.claimClass === "analyzerInference" ? "inference" : ""}); g.append(svgEl("rect", {x:n.x, y:n.y, width:n.width, height:n.height, rx:5})); const label = svgEl("text", {x:n.x+12,y:n.y+23}); label.textContent = n.label.length > 53 ? n.label.slice(0,50)+"…" : n.label; const known = svgEl("text", {x:n.x+12,y:n.y+44}); known.textContent = `[${n.claimClass}] ${n.kind}${n.detail?.queryDecision ? `; ${n.detail.queryDecision}` : ""}`; const title = svgEl("title", {}); title.textContent = `${n.label}: ${n.claimClass}; ${n.kind}${n.detail?.queryDecision ? `; query membership ${n.detail.queryDecision}` : ""}`; g.append(label,known,title); g.addEventListener("click", () => run(() => inspect(n))); svg.append(g); return g;});
    let index = 0;
    function select() {groups.forEach((g,i) => g.classList.toggle("selected", i === index)); svg.setAttribute("aria-label", `${p.items[index].label}; ${p.items[index].claimClass}. ${p.edges.filter(e => e.from === p.items[index].id || e.to === p.items[index].id).length} connections. Enter inspects.`); groups[index].scrollIntoView({block:"nearest",inline:"nearest"});}
    svg.addEventListener("keydown", event => {if (["ArrowDown","ArrowRight","ArrowUp","ArrowLeft","Home","End","Enter"].includes(event.key)) {event.preventDefault(); if (event.key === "Enter") run(() => inspect(p.items[index])); else {if (event.key === "Home") index=0; else if (event.key === "End") index=p.items.length-1; else index=Math.max(0,Math.min(p.items.length-1,index+(["ArrowDown","ArrowRight"].includes(event.key)?1:-1))); select();}}});
    svg.addEventListener("focus", select); wrap.append(svg); box.append(wrap);
  }
  function inspect(item) {
    selected = item; renderContext(); $("inspector-title").textContent = item.label; const box = $("inspector-body"); box.replaceChildren(el("p", `${item.kind} · ${item.claimClass}`, "badge"), el("p", `Evidence snapshot ${active.answer.revisionId} · freshness ${active.freshness}`));
    if (item.detail?.promisedByName) {const comparison=el("div",undefined,"row");const promise=el("div",undefined,"card"),observed=el("div",undefined,"card");promise.append(el("h3","Name suggests"),el("p",item.detail.promisedByName.join(", ")));observed.append(el("h3","Analyzer observes"),el("p",item.detail.observedEffects.join(", ") || "No modeled effects recorded"));comparison.append(promise,observed);box.append(comparison,el("p",`Missing in the model: ${item.detail.missingInModel}. ${item.detail.scope}`,"warning"));}
    if (!item.citations.length) box.append(el("p", "No direct source citation for this interpreted aggregate. Do not treat it as proven behavior.", "warning"));
    for (const c of item.citations) {const card = el("div", undefined, "card"); card.append(el("h3", `${c.file}:${c.startLine}–${c.endLine}`), el("small", `Repository ${c.repositoryId} · snapshot ${c.revisionId} · source hash ${c.sourceHash}`), el("pre", c.excerpt)); box.append(card);}
    const detail = el("details"); detail.append(el("summary", "Analyzer detail and limitations"), el("pre", JSON.stringify(item.detail, null, 2))); box.append(detail);
    const edges = active.answer.edges.filter(e => e.from === item.id || e.to === item.id); box.append(el("h3", "Represented relationships")); edges.forEach(e => box.append(el("p", `${e.from} → ${e.to} · ${e.kind} [${e.claimClass}]`)));
    const form = el("form"), label = el("label", "Review explanation (required, at least 5 characters)"); label.htmlFor="review-reason"; const reason=el("textarea"); reason.id="review-reason"; reason.required=true; reason.minLength=5; reason.maxLength=2000;
    const verdictLabel=el("label", "Review decision"); verdictLabel.htmlFor="review-verdict"; const verdict=el("select"); verdict.id="review-verdict"; for (const v of ["confirm","dispute","refute"]) {const o=el("option",v);o.value=v;verdict.append(o);}
    const submit=el("button","Save review"); submit.type="submit"; form.append(label,reason,verdictLabel,verdict,submit); form.addEventListener("submit", event => {event.preventDefault();run(async () => {const result=await api("/api/reviews","POST",{answerId:active.answer.id,itemId:item.id,verdict:verdict.value,reason:reason.value});$("inspector").close();active=result;renderAnswer();announce(`Review recorded. ${result.hiddenClaims} refuted claims withheld.`);});}); box.append(el("p", "Refuting hides this claim and its links. Confirming does not promote an inference to proof."),form); if (!$("inspector").open) $("inspector").showModal();
  }
  $("close-inspector").addEventListener("click", () => $("inspector").close());
  $("save-workspace").addEventListener("submit", event => {event.preventDefault();run(async () => {await api("/api/workspaces","POST",{name:$("workspace-name").value,answerIds:[...openAnswers],activeAnswer:active?.answer.id || null});await refresh();announce("Workspace saved. Exact evidence snapshots will be checked when reopened.");});});
  $("cross-search").addEventListener("submit", event => {event.preventDefault();run(async () => {const result=await api("/api/search","POST",{query:$("search-query").value});const box=$("search-results");box.replaceChildren(el("p",`${result.results.length} matches${result.capped?" (capped; refine query)":""}. ${result.scope}`)); for (const i of result.results) {const c=el("div",undefined,"card");c.append(el("strong",i.name),el("p",`${i.repositoryName} · ${i.repositoryId} · ${i.revisionId}`),el("pre",i.citation?.excerpt || "No source citation available"),button("Inspect in repository",async () => {const a=await api("/api/questions","POST",{repositoryId:i.repositoryId,question:i.name,intent:"search",symbolId:i.symbolId});openAnswers.add(a.answer.id);await refresh();showAnswer(a);}));box.append(c);} announce(`Search returned ${result.results.length} matches in indexed snapshots.`);});});
  $("feature-form").addEventListener("submit", event => {event.preventDefault();run(async () => {const criteria=$("feature-criteria").value.split("\n").map(s=>s.trim()).filter(Boolean);await api("/api/features","POST",{repositoryId:$("feature-repo").value,prompt:$("feature-prompt").value,criteria});await refresh();announce("Planning draft saved. No code generated or applied.");});});
  function renderFeatures() {const box=$("feature-drafts");box.replaceChildren();for (const f of state.features) {const c=el("div",undefined,"card");c.append(el("h2",f.prompt),el("p",`${f.status} · ${f.repositoryId} · ${f.revisionId}`));const list=el("ol");f.criteria.forEach(s=>list.append(el("li",s)));c.append(list,el("p",`Name overlap hints: ${f.relatedSymbols.join(", ") || "none"}`));f.limitations.forEach(s=>c.append(el("p",s,"muted")));box.append(c);}}
  function renderStories() {const term=$("story-filter").value.toLowerCase(), rows=capabilities.stories.filter(s=>`${s.id} ${s.title} ${s.status}`.toLowerCase().includes(term));const counts={};capabilities.stories.forEach(s=>counts[s.status]=(counts[s.status]||0)+1);$("story-counts").textContent=`${capabilities.stories.length} stories · ${Object.entries(counts).map(([k,v])=>`${v} ${k}`).join(" · ")} · ${rows.length} shown`;const box=$("stories");box.replaceChildren();for (const s of rows) {const c=el("div",undefined,"story");c.append(el("h3",`${s.id}: ${s.title}`),el("p",s.status,"badge"),el("p",s.note),el("small",`Evidence: ${(s.evidence||[]).join("; ") || "No acceptance evidence yet"}`));box.append(c);}}
  $("story-filter").addEventListener("input",renderStories);
  function renderGallery() {
    const entries=[
      ["Table","Exact evidence population and represented connections.","overview"],
      ["Graph","Static call or concept relationships; no runtime order implied.","callers"],
      ["Outline","Accessible reading order of every item and outgoing link.","concepts"],
      ["Retained source facts","Guard/call/data-site records from the indexed parser pass, with scope limits.",null,"facts"],
      ["Rule catalogue","Extracted declarations and candidate obligations, with approval limits.","rules"],
      ["Behavior summaries","Static modeled effects, not proof of successful execution.","behaviors"],
      ["Naming anomalies","Name expectations versus observed model effects; candidates only.","naming"],
      ["Recorded timeline","Imported event intervals; producer execution and clock accuracy are not independently attested.",null,"observations"],
      ["Workflow scenario","Reachability in a user-declared graph; not production simulation.",null,"scenarios"],
      ["History and comparison","Bounded Git metadata and retained source-text comparisons.",null,"orientation"],
      ["Latency chart","Unavailable: representative workload distributions and a latency chart adapter are not available.",null],
      ["Runtime replay","Unavailable: recorded events can be inspected, but no execution replay engine is available.",null]
    ];
    for(const [name,description,intent,area] of entries){const c=el("div",undefined,"card");c.append(el("h2",name),el("p",description));if(intent)c.append(button("Prepare question",()=>{page("investigate");$("intent").value=intent;$("question").value=intent==="callers"?"Show callers of ":intent;$("question").focus();}));if(area)c.append(button("Open evidence area",()=>document.querySelector(`[data-page="${area}"]`).click()));$("gallery").append(c);}
  }
  document.addEventListener("cie-open-answer", event => run(async () => {openAnswers.add(event.detail.answer.id); await refresh(); showAnswer(event.detail);}));
  document.addEventListener("cie-state-changed", () => run(refresh));
  async function start() {await refresh();capabilities=await api("/api/capabilities");renderStories();renderGallery();const last=state.workspaces[0]?.activeAnswer;if(last && openAnswers.has(last))await loadAnswer(last);announce("Local workspace ready. No cloud model is connected.");}
  run(start);
  setInterval(async () => {if (polling || document.hidden || !state.jobs.some(j=>["queued","running","cancelling"].includes(j.state))) return;polling=true;try {const before=JSON.stringify(state.jobs);await refresh();if(JSON.stringify(state.jobs)!==before)announce("Background job state updated. Inspect the jobs list for results or limitations.");}catch(error){fail(error);}finally{polling=false;}},1500);
}
