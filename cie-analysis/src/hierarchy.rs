//! Hierarchy construction and ConceptNode DAG (PRD §26–27 / FR-40–44).
//!
//! Merge sources (FR-42): package containment (files/classes), FCA
//! subsumption (concept lattice), and call ownership (rule nodes). Every
//! edge carries a reason. Behaviors/rules roll up to ancestors (FR-43).
//! Output matches the user-facing artifact shape of §27 (acceptance #27).

use crate::behavior::BehaviorResult;
use crate::fca;
use cie_concepts::{BehaviorRef, ConceptDag, ConceptKind, ConceptNode, EdgeReason, RuleRef};
use cie_contract::Contract;
use cie_core::types::{ConceptNodeId, Ns, SymbolId};
use cie_semantic::facts::{SymbolKind, SymbolStore};
use cie_stack::types::StackReport;
use cie_summary::EffectKind;
use std::collections::{BTreeMap, BTreeSet};

/// What the labeler may read about one member function of a concept: the
/// story lives in the members (names, tags, effects), not in the joined
/// attribute words. Mirrored in cie-llm as `ConceptEvidence` (no
/// dependency edge cie-analysis → cie-llm; cie-ingest converts).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LabelSymbolEvidence {
    pub name: String,
    pub tags: Vec<String>,
    pub effects: Vec<String>,
}

/// Label function hook: the CLI supplies either the offline deterministic
/// labeler or an LLM-backed one. Labels never decide membership.
/// Label provider: returns (label, source) where source is the
/// LabelRecord source ("llm" | "offline" | "cache") — TASK-079 binds
/// LLM-proposed labels as inert `LlmCandidate` rules. A3: the hook also
/// receives the concept's member evidence (bounded, extent order,
/// deterministic), so a real provider can name the story.
pub type LabelFn<'a> =
    &'a dyn Fn(&[String], &str, &[LabelSymbolEvidence]) -> (String, &'static str);

/// RFC-0019 §6.4: the C4 view of the persisted stack facts, precomputed so
/// every tier below is a lookup. `stack: None` skips the whole structure —
/// the exact pre-RFC shape.
struct C4 {
    /// file path -> (its Container node, its component node)
    file_home: BTreeMap<String, (ConceptNodeId, ConceptNodeId)>,
    /// file path -> the app dir that claims it (aggregate scoping)
    file_app: BTreeMap<String, String>,
    /// app dir -> recorded route-handler class names (routes[].owner)
    route_owners: BTreeMap<String, BTreeSet<String>>,
    /// app dir -> recorded repository class names (jvm.repositories[].name)
    repo_names: BTreeMap<String, BTreeSet<String>>,
    /// app dir -> classes a recorded repository's supertype NAMES as the
    /// entity it serves (JpaRepository<Invoice, Long> → Invoice) — the
    /// strongest recorded aggregate evidence
    repo_entities: BTreeMap<String, BTreeSet<String>>,
    /// app dir -> recorded framework-role class names (routes[].owner,
    /// @Service, @Repository, @Entity, config classes, boot mains) — these
    /// are NEVER aggregates: they are the framework's own furniture
    role_names: BTreeMap<String, BTreeSet<String>>,
    /// app dir -> class names reachable FROM a recorded route handler
    /// (bounded BFS over recorded summaries' callees)
    route_reached: BTreeMap<String, BTreeSet<String>>,
    /// files with a recorded route (the http-route FCA attribute)
    route_files: BTreeSet<String>,
    /// files of @Service classes — the recorded DI-injected layer
    di_files: BTreeSet<String>,
    /// app dirs whose docker governance is NOT "none"
    managed_dirs: BTreeSet<String>,
}

/// One AppSite's slice of the stack facts, scoped for the tier build.
struct AppSlice<'a> {
    name: String,
    routes: Vec<&'a str>,
    mains: Vec<&'a str>,
    services: Vec<&'a str>,
    repositories: Vec<&'a str>,
    entities: Vec<&'a str>,
    configs: Vec<&'a str>,
}

fn app_slice<'a>(stack: &'a StackReport, dir: &'a str) -> AppSlice<'a> {
    let under = |rel: &str| -> bool {
        if dir.is_empty() {
            // a root app claims files no other app claims — resolved by the
            // caller via `claimed_elsewhere`; here accept everything
            true
        } else {
            rel.starts_with(&format!("{dir}/"))
        }
    };
    let jvm = stack.jvm.as_ref();
    AppSlice {
        name: if dir.is_empty() {
            // a root-rooted app: call it what its manifest named it (the
            // same name the container node carries) — "app root" only when
            // no name survives. This string labels the components too
            // ("mipay-server boot", not "app root boot").
            stack
                .apps
                .iter()
                .find(|a| a.dir.is_empty() && !a.name.is_empty())
                .map(|a| a.name.clone())
                .unwrap_or_else(|| "app root".to_string())
        } else {
            dir.rsplit('/').next().unwrap_or(dir).to_string()
        },
        routes: stack.routes.iter().filter(|r| under(&r.rel)).map(|r| r.rel.as_str()).collect(),
        mains: stack.mains.iter().filter(|m| under(&m.rel)).map(|m| m.rel.as_str()).collect(),
        services: jvm
            .map(|j| j.services.iter().filter(|s| under(&s.rel)).map(|s| s.rel.as_str()).collect())
            .unwrap_or_default(),
        repositories: jvm
            .map(|j| {
                j.repositories
                    .iter()
                    .filter(|s| under(&s.rel))
                    .map(|s| s.rel.as_str())
                    .collect()
            })
            .unwrap_or_default(),
        entities: jvm
            .map(|j| j.entities.iter().filter(|s| under(&s.rel)).map(|s| s.rel.as_str()).collect())
            .unwrap_or_default(),
        configs: jvm
            .map(|j| {
                j.config_classes
                    .iter()
                    .filter(|s| under(&s.rel))
                    .map(|s| s.rel.as_str())
                    .collect()
            })
            .unwrap_or_default(),
    }
}

/// the file's role home inside its app, in priority order (RFC §6.4 L2):
/// DDD subpackage > recorded route > recorded boot > recorded jvm layer >
/// role/suffix naming > core fallback. (role, recorded). `rel_to_app` is
/// the path under the app dir (DDD segments, naming); `rel_full` is the
/// repo-relative path the stack artifact's recorded rels speak.
fn role_of(rel_to_app: &str, rel_full: &str, app: &AppSlice) -> (&'static str, bool) {
    for seg in rel_to_app.split('/') {
        match seg {
            "domain" => return ("domain", false), // Subdomain — structure-based
            "application" => return ("application", false),
            "infrastructure" => return ("infrastructure", false),
            _ => {}
        }
    }
    if app.routes.contains(&rel_full) {
        return ("web", true);
    }
    if app.mains.contains(&rel_full) {
        return ("boot", true);
    }
    if app.services.contains(&rel_full) {
        return ("service", true);
    }
    if app.repositories.contains(&rel_full) {
        return ("repository", true);
    }
    if app.entities.contains(&rel_full) {
        return ("entity", true);
    }
    if app.configs.contains(&rel_full) {
        return ("config", true);
    }
    // role/suffix naming — inference, never recorded
    let stem = rel_to_app.rsplit('/').next().unwrap_or(rel_to_app);
    let stem = stem
        .rsplit_once('.')
        .map(|(s, _)| s)
        .unwrap_or(stem)
        .to_ascii_lowercase();
    let has = |needles: &[&str]| needles.iter().any(|n| stem.contains(n));
    if has(&["service", "usecase", "use_case"]) {
        ("service", false)
    } else if has(&["repository", "repo", "store", "dao", "gateway", "persistence"]) {
        ("repository", false)
    } else if has(&["controller", "route", "handler", "endpoint", "resource"]) {
        ("web", false)
    } else if has(&["entity", "model", "dto", "value"]) {
        ("entity", false)
    } else if has(&["main", "server", "boot", "launcher"]) {
        ("boot", false)
    } else if has(&["config", "settings"]) {
        ("config", false)
    } else {
        ("core", false)
    }
}

impl C4 {
    /// Build the L1 (Container) and L2 (Service/Subdomain) tiers from the
    /// persisted stack facts. Deterministic: apps in report order, files in
    /// the store's sorted path order, roles in the priority order above.
    fn build(dag: &mut ConceptDag, store: &SymbolStore, stack: &StackReport, now: Ns) -> C4 {
        let mut file_home = BTreeMap::new();
        let mut file_app: BTreeMap<String, String> = BTreeMap::new();
        let mut route_owners = BTreeMap::new();
        let mut repo_names = BTreeMap::new();
        let mut repo_entities: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
        let mut role_names: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
        let route_reached: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
        let mut route_files = BTreeSet::new();
        let mut di_files = BTreeSet::new();
        let mut managed_dirs = BTreeSet::new();

        let files = store.files(); // sorted paths — deterministic tiering
        // app dirs longest-first so a nested app claims before its parent
        let mut dirs: Vec<&String> = stack.apps.iter().map(|a| &a.dir).collect();
        dirs.sort_by_key(|d| std::cmp::Reverse(d.len()));

        for app in &stack.apps {
            let dir = app.dir.as_str();
            // ---- L1: the container (recorded: manifest + docker evidence)
            let mut cnode = ConceptNode::new(0, container_label(app), ConceptKind::Container, now);
            cnode.primary_edge = EdgeReason::PackageContainment;
            cnode.provenance = "stack:apps".into();
            cnode.confidence = 1.0; // recorded — the manifest named it
            cnode.intent = vec![
                match app.docker.as_ref().map(|g| g.kind.as_str()) {
                    Some("compose") => "docker:compose",
                    Some("own-dockerfile") => "docker:own-dockerfile",
                    Some(_) => "docker:none",
                    None => "docker:unknown",
                }
                .to_string(),
            ];
            let cid = dag.push(cnode);
            // the Domain root is always node 0 (pushed first in build_hierarchy)
            dag.link_child(0, cid);

            // ---- L2: components (roles / subpackages) over the app's files
            let slice = app_slice(stack, dir);
            let app_routes: Vec<_> = stack
                .routes
                .iter()
                .filter(|r| slice.routes.contains(&r.rel.as_str()))
                .collect();
            if !app_routes.is_empty() {
                route_owners.insert(
                    dir.to_string(),
                    app_routes.iter().map(|r| r.owner.clone()).collect(),
                );
            }
            let jvm = stack.jvm.as_ref();
            let app_repos: Vec<_> = jvm
                .map(|j| {
                    j.repositories
                        .iter()
                        .filter(|s| slice.repositories.contains(&s.rel.as_str()))
                        .collect()
                })
                .unwrap_or_default();
            repo_names.insert(
                dir.to_string(),
                app_repos.iter().map(|s| s.name.clone()).collect(),
            );
            repo_entities.insert(
                dir.to_string(),
                app_repos
                    .iter()
                    .filter_map(|s| s.entity.clone())
                    .collect(),
            );
            // the framework's own furniture: never aggregates
            let mut roles: BTreeSet<String> = app_routes.iter().map(|r| r.owner.clone()).collect();
            if let Some(j) = jvm {
                for s in j.services.iter().filter(|s| slice.services.contains(&s.rel.as_str())) {
                    roles.insert(s.name.clone());
                }
                for s in j.repositories.iter().filter(|s| slice.repositories.contains(&s.rel.as_str())) {
                    roles.insert(s.name.clone());
                }
                for s in j.entities.iter().filter(|s| slice.entities.contains(&s.rel.as_str())) {
                    roles.insert(s.name.clone());
                }
                for s in j.config_classes.iter().filter(|s| slice.configs.contains(&s.rel.as_str())) {
                    roles.insert(s.name.clone());
                }
            }
            for m in stack.mains.iter().filter(|m| slice.mains.contains(&m.rel.as_str())) {
                roles.insert(m.name.clone());
            }
            role_names.insert(dir.to_string(), roles);
            for r in &slice.routes {
                route_files.insert(r.to_string());
            }
            for s in &slice.services {
                di_files.insert(s.to_string());
            }
            match app.docker.as_ref().map(|g| g.kind.as_str()) {
                Some(k) if k != "none" => {
                    managed_dirs.insert(dir.to_string());
                }
                _ => {}
            }

            // one component node per distinct home, in first-seen order
            let mut components: BTreeMap<String, ConceptNodeId> = BTreeMap::new();
            for path in &files {
                let rel = match rel_under(dir, path) {
                    Some(r) => r,
                    None => continue, // not this app's file
                };
                if dir.is_empty() && claimed_elsewhere(&dirs, dir, path) {
                    continue; // a root app only keeps files no subdir app claims
                }
                let (role, recorded) = role_of(&rel, path, &slice);
                let home_key = role.to_string();
                let comp = *components.entry(home_key.clone()).or_insert_with(|| {
                    let (label, kind, conf) = if matches!(
                        role,
                        "domain" | "application" | "infrastructure"
                    ) {
                        (format!("{} {role}", slice.name), ConceptKind::Subdomain, 0.7)
                    } else if recorded {
                        (format!("{} {role}", slice.name), ConceptKind::Service, 1.0)
                    } else {
                        (format!("{} {role}", slice.name), ConceptKind::Service, 0.7)
                    };
                    let mut n = ConceptNode::new(0, label, kind, now);
                    n.primary_edge = EdgeReason::ClusterCohesion;
                    n.provenance = "stack:roles".into();
                    n.confidence = conf;
                    let id = dag.push(n);
                    dag.link_child(cid, id);
                    id
                });
                file_home.insert(path.clone(), (cid, comp));
                file_app.insert(path.clone(), dir.to_string());
            }
        }
        C4 {
            file_home,
            file_app,
            route_owners,
            repo_names,
            repo_entities,
            role_names,
            route_reached,
            route_files,
            di_files,
            managed_dirs,
        }
    }

    /// RFC-0019 §6.4 aggregate arm 3's reachability input: the class names
    /// reachable FROM a recorded route handler, bounded BFS (3 hops, visited
    /// set) over the recorded summaries' callees. Needs `behavior`, so it
    /// runs after `build` (which only sees the stack artifact).
    fn fill_route_reached(
        &mut self,
        store: &SymbolStore,
        behavior: &BehaviorResult,
        stack: &StackReport,
    ) {
        for app in &stack.apps {
            let Some(owners) = self.route_owners.get(app.dir.as_str()) else {
                continue;
            };
            // start: every method of every recorded route-owner class
            let mut frontier: Vec<SymbolId> = Vec::new();
            let mut visited: BTreeSet<SymbolId> = BTreeSet::new();
            for sym in &store.symbols {
                if sym.is_function_like() {
                    if let Some(p) = sym.parent {
                        if let Some(psym) = store.get(p) {
                            if owners.contains(&psym.name) && visited.insert(sym.id) {
                                frontier.push(sym.id);
                            }
                        }
                    }
                }
            }
            let mut reached: BTreeSet<String> = BTreeSet::new();
            for _ in 0..3 {
                let mut next: Vec<SymbolId> = Vec::new();
                for id in &frontier {
                    if let Some(s) = behavior.summaries.get(id) {
                        for c in &s.callees {
                            if visited.insert(*c) {
                                if let Some(csym) = store.get(*c) {
                                    if let Some(p) = csym.parent {
                                        if let Some(psym) = store.get(p) {
                                            reached.insert(psym.name.clone());
                                        }
                                    }
                                }
                                next.push(*c);
                            }
                        }
                    }
                }
                if next.is_empty() {
                    break;
                }
                frontier = next;
            }
            self.route_reached.insert(app.dir.clone(), reached);
        }
    }

    fn component_of(&self, path: &str) -> Option<ConceptNodeId> {
        self.file_home.get(path).map(|(_, c)| *c)
    }

    fn app_of(&self, path: &str) -> Option<&str> {
        self.file_app.get(path).map(String::as_str)
    }
}

fn container_label(app: &cie_stack::types::AppSite) -> String {
    if !app.name.is_empty() {
        app.name.clone()
    } else if !app.dir.is_empty() {
        app.dir.clone()
    } else {
        "app root".into()
    }
}

/// the file's path relative to the app dir (None when outside it)
fn rel_under(dir: &str, path: &str) -> Option<String> {
    if dir.is_empty() {
        Some(path.to_string())
    } else {
        path.strip_prefix(&format!("{dir}/")).map(|s| s.to_string())
    }
}

/// is `path` claimed by any OTHER app dir (longest-first)?
fn claimed_elsewhere(dirs: &[&String], own: &str, path: &str) -> bool {
    dirs.iter().any(|d| {
        !d.is_empty() && d.as_str() != own && path.starts_with(&format!("{d}/"))
    })
}

pub fn build_hierarchy(
    store: &SymbolStore,
    behavior: &BehaviorResult,
    contracts: &[Contract],
    label_fn: LabelFn,
    now: Ns,
    stack: Option<&StackReport>,
) -> ConceptDag {
    let mut dag = ConceptDag::default();

    // ---- Domain root ----
    let root = dag.push(ConceptNode::new(0, "Project", ConceptKind::Domain, now));

    // ---- RFC-0019 §6.4: the C4 tiers from the persisted stack facts ----
    // L1 Containers (one per AppSite) and L2 components (framework roles /
    // DDD subpackages), then files re-parent under their component. With
    // stack: None nothing here runs — the exact pre-RFC shape.
    let mut c4 = stack.map(|st| C4::build(&mut dag, store, st, now));
    if let (Some(c4), Some(st)) = (c4.as_mut(), stack) {
        c4.fill_route_reached(store, behavior, st);
    }
    let module_parent = |path: &str, root: ConceptNodeId| -> ConceptNodeId {
        match &c4 {
            Some(c4) => c4.component_of(path).unwrap_or(root),
            None => root,
        }
    };

    // ---- Module nodes (package containment) ----
    let mut module_nodes: BTreeMap<String, ConceptNodeId> = BTreeMap::new();
    for path in store.files() {
        let mut n = ConceptNode::new(0, path.clone(), ConceptKind::Module, now);
        n.primary_edge = EdgeReason::PackageContainment;
        n.provenance = "rule:package".into();
        let id = dag.push(n);
        dag.link_child(module_parent(&path, root), id);
        module_nodes.insert(path, id);
    }

    // ---- Class / service nodes + symbol membership ----
    let mut node_of_symbol: BTreeMap<SymbolId, ConceptNodeId> = BTreeMap::new();
    let mut behaviors_added: BTreeSet<SymbolId> = BTreeSet::new();

    for sym in &store.symbols {
        match sym.kind {
            SymbolKind::Class
            | SymbolKind::Struct
            | SymbolKind::Trait
            | SymbolKind::Enum
            | SymbolKind::Interface => {
                let methods = store.children_of(sym.id);
                let path = store.files.path(sym.loc.file);
                // RFC-0019 §6.4: the money-words Aggregate heuristic retires
                // wherever stack facts exist. Aggregate = a recorded route
                // owner that (within a bounded hop budget) reaches a
                // recorded repository — two evidence links, not vocabulary.
                // The vocabulary fallback survives ONLY where no stack facts
                // reach (no stack artifact, or files outside every app).
                let in_app = c4.as_ref().and_then(|c| c.app_of(path)).map(String::from);
                let kind = if let Some(app_dir) = &in_app {
                    let c4 = c4.as_ref().unwrap();
                    let aggregate = c4
                        .repo_entities
                        .get(app_dir.as_str())
                        .map(|e| e.contains(sym.name.as_str()))
                        .unwrap_or(false)
                        || (!c4
                            .role_names
                            .get(app_dir.as_str())
                            .map(|r| r.contains(sym.name.as_str()))
                            .unwrap_or(false)
                            && c4
                                .route_reached
                                .get(app_dir.as_str())
                                .map(|r| r.contains(sym.name.as_str()))
                                .unwrap_or(false)
                            && c4
                                .repo_names
                                .get(app_dir.as_str())
                                .map(|repos| {
                                    reaches_repository(store, behavior, methods, repos)
                                })
                                .unwrap_or(false));
                    if aggregate {
                        ConceptKind::Aggregate
                    } else if methods.is_empty() {
                        ConceptKind::Value
                    } else {
                        ConceptKind::Entity
                    }
                } else {
                    let moneyish = methods.iter().any(|m| {
                        store
                            .get(*m)
                            .map(|s| {
                                let n = s.name.to_ascii_lowercase();
                                n.contains("charge")
                                    || n.contains("pay")
                                    || n.contains("refund")
                                    || n.contains("transfer")
                            })
                            .unwrap_or(false)
                    });
                    if moneyish {
                        ConceptKind::Aggregate
                    } else if methods.is_empty() {
                        ConceptKind::Value
                    } else {
                        ConceptKind::Entity
                    }
                };
                let mut n = ConceptNode::new(0, sym.name.clone(), kind, now);
                n.primary_edge = EdgeReason::PackageContainment;
                n.provenance = if matches!(kind, ConceptKind::Aggregate) && in_app.is_some() {
                    "stack:aggregate".into() // recorded: route owner + repo reach
                } else {
                    "rule:structure".into()
                };
                n.symbols = vec![sym.id];
                let id = dag.push(n);
                if let Some(pn) = module_nodes.get(path) {
                    dag.link_child(*pn, id);
                } else {
                    dag.link_child(root, id);
                }
                // the class also links straight to its component (RFC §6.4
                // re-parenting: the primary chain stays Module → class*, the
                // component edge keeps the role visible as its own reason)
                if let Some(c4) = &c4 {
                    if let Some(comp) = c4.component_of(path) {
                        let _ = dag.try_link_child(comp, id, EdgeReason::CallOwnership);
                    }
                }
                node_of_symbol.insert(sym.id, id);
            }
            _ => {}
        }
    }

    // Attach function behaviors: to class node when method, else module node.
    for sym in &store.symbols {
        if !sym.is_function_like() {
            continue;
        }
        let Some(summary) = behavior.summaries.get(&sym.id) else {
            continue;
        };
        let lower_name = sym.name.to_ascii_lowercase();
        let label = if summary.tags.contains("auth")
            && !lower_name.starts_with("auth")
            && !lower_name.starts_with("verify")
        {
            format!("Authorize {}", humanize(&sym.name))
        } else if summary.tags.contains("auth") {
            humanize(&sym.name)
        } else if summary.tags.contains("money") {
            format!("Move money via {}", humanize(&sym.name))
        } else if summary.tags.contains("query") {
            format!("Query data via {}", humanize(&sym.name))
        } else if summary.tags.contains("mutation") {
            format!("Mutate data via {}", humanize(&sym.name))
        } else {
            humanize(&sym.name)
        };
        let bref = BehaviorRef {
            summary: sym.id,
            kind: label,
            confidence: 1.0, // mechanical derivation from summaries
        };
        let target: ConceptNodeId = match sym.parent {
            Some(p) if node_of_symbol.contains_key(&p) => node_of_symbol[&p],
            _ => module_nodes.get(store.files.path(sym.loc.file)).copied().unwrap_or(root),
        };
        if let Some(n) = dag.get_mut(target) {
            n.behaviors.push(bref);
        }
        behaviors_added.insert(sym.id);
        // symbol membership rolls up later
        if let Some(n) = dag.get_mut(target) {
            if !n.symbols.contains(&sym.id) {
                n.symbols.push(sym.id);
            }
        }
    }

    // Attach the remaining named-but-not-behavioural symbols to their file's
    // module node — SQL tables and views (v1.2.1), constants, and the module
    // symbols themselves. A schema file's tables ARE its content: without
    // this pass the module node has no symbols, no evidence, and the map
    // would honestly-but-uselessly hide every SQL file.
    for sym in &store.symbols {
        if sym.is_function_like() || node_of_symbol.contains_key(&sym.id) {
            continue; // functions attach above; type-likes got their own nodes
        }
        if let Some(pn) = module_nodes.get(store.files.path(sym.loc.file)) {
            if let Some(n) = dag.get_mut(*pn) {
                if !n.symbols.contains(&sym.id) {
                    n.symbols.push(sym.id);
                }
            }
        }
    }

    // ---- FCA concepts -> UseCase/Subdomain nodes (FcaSubsumption edges) ----
    // RFC-0019 §6.4: the lattice intents gain the stack facts — http-route,
    // db-write, di-injected, docker-managed — so the same function that used
    // to share only its tags now shares WHERE it sits in the served surface.
    let objects: Vec<(SymbolId, BTreeSet<String>)> = behavior
        .summaries
        .iter()
        .filter(|(id, _)| {
            store
                .get(**id)
                .map(|s| s.is_function_like() && !s.is_test())
                .unwrap_or(false)
        })
        .map(|(id, s)| {
            let mut attrs: BTreeSet<String> = s.tags.clone();
            for e in &s.effects {
                attrs.insert(e.as_str().to_string());
            }
            if let Some(c4) = &c4 {
                let sym = store.get(*id);
                let path = sym.map(|s| store.files.path(s.loc.file)).unwrap_or("");
                if c4.route_files.contains(path) {
                    attrs.insert("http-route".into());
                }
                if s.effects.contains(&EffectKind::DbWrite) {
                    attrs.insert("db-write".into());
                }
                if c4.di_files.contains(path) {
                    attrs.insert("di-injected".into());
                }
                if let Some(app) = c4.app_of(path) {
                    if c4.managed_dirs.contains(app) {
                        attrs.insert("docker-managed".into());
                    }
                }
            }
            (*id, attrs)
        })
        .collect();

    let lattice = fca::build_lattice(&objects);

    // ---- Induction chain: usecase -> broader usecase (A1) ----
    // fca::Concept.parents holds LOWER covers (minimal strict
    // intent-supersets: more specific). Inverting gives the UPPER covers —
    // the strictly broader concepts each usecase can hang from. The sorted
    // lattice (extent desc) places every broader concept before the
    // narrower one, so one forward pass resolves the whole chain.
    let mut upper_covers: Vec<BTreeSet<usize>> = vec![BTreeSet::new(); lattice.len()];
    for (i, c) in lattice.iter().enumerate() {
        for &j in &c.parents {
            upper_covers[j].insert(i);
        }
    }
    // lattice index -> dag id of the concepts that survived the noise filter
    let mut surviving: BTreeMap<usize, ConceptNodeId> = BTreeMap::new();
    for (ix, concept) in lattice.iter().enumerate() {
        // Skip top (empty intent) and tiny single-symbol concepts already
        // attached via structure — overlap is fine, noise is not.
        if concept.intent.is_empty() || concept.extent.len() < 2 {
            continue;
        }
        // A3: the labeler sees the cluster's members — names, tags,
        // effects — so a real provider can name the story, not join tag
        // words. Bounded: at most 12 members in extent order (SymbolId
        // ascending, deterministic); the offline provider ignores the
        // evidence and returns the mechanical fallback.
        let evidence: Vec<LabelSymbolEvidence> = concept
            .extent
            .iter()
            .take(12)
            .filter_map(|sid| {
                let sym = store.get(*sid)?;
                let summary = behavior.summaries.get(sid)?;
                Some(LabelSymbolEvidence {
                    name: sym.name.clone(),
                    tags: summary.tags.iter().cloned().collect(),
                    effects: summary
                        .effects
                        .iter()
                        .map(|e| e.as_str().to_string())
                        .collect(),
                })
            })
            .collect();
        let (label, label_source) = label_fn(
            &concept.intent.iter().cloned().collect::<Vec<_>>(),
            &fca::concept_label(&concept.intent),
            &evidence,
        );
        let mut n = ConceptNode::new(0, label, ConceptKind::UseCase, now);
        n.intent = concept.intent.iter().cloned().collect();
        n.primary_edge = EdgeReason::FcaSubsumption;
        n.provenance = "fca:godin-v1".into();
        n.symbols = concept.extent.iter().copied().collect();
        n.confidence = 0.8; // derived concept, mechanical but heuristic scope
        let id = dag.push(n);
        surviving.insert(ix, id);

        // Primary parent = the BROADest surviving upper cover (ascending
        // lattice index = descending extent, deterministic). Remaining
        // surviving covers become traceable FcaSubsumption extra edges,
        // recorded ONLY on this node — never mirrored into the cover's
        // children, so an inferred usecase can't drag a recorded node's
        // weakest-link rollup (same rule as the member-class edges below).
        // The only skip above a survivor is the top concept: any broader
        // concept of an extent>=2 concept also has extent>=2.
        let mut primary: Option<ConceptNodeId> = None;
        let mut extra_covers: Vec<ConceptNodeId> = Vec::new();
        for &cover in &upper_covers[ix] {
            if let Some(&pid) = surviving.get(&cover) {
                if primary.is_none() {
                    primary = Some(pid);
                } else {
                    extra_covers.push(pid);
                }
            }
        }
        match primary {
            Some(pid) => {
                // Covers form a strict intent order — a cycle is
                // impossible; fall back to the root rather than panic if
                // that invariant is ever violated.
                if dag
                    .try_link_child(pid, id, EdgeReason::FcaSubsumption)
                    .is_err()
                {
                    dag.link_child(root, id);
                }
            }
            None => dag.link_child(root, id),
        }
        if !extra_covers.is_empty() {
            if let Some(node) = dag.get_mut(id) {
                for pid in extra_covers {
                    node.extra_edges.push((pid, EdgeReason::FcaSubsumption));
                }
            }
        }

        // TASK-079: an LLM-proposed label binds as an inert LlmCandidate
        // rule — visible, traceable, never decisive (acceptance #28).
        if label_source == "llm" {
            if let Some(node) = dag.get_mut(id) {
                node.rules.push(RuleRef {
                    contract: 9_000_000u32 + id,
                    kind: format!("llm-label:{}", node.label),
                    authority: cie_contract::Authority::LlmCandidate,
                });
                node.provenance = "label:llm".into();
            }
        }

        // Extra DAG edge: majority module (ClusterCohesion) when most of the
        // extent lives in one file (acceptance #30 — multi-parent, traceable).
        let mut per_module: BTreeMap<String, usize> = BTreeMap::new();
        for s in &concept.extent {
            if let Some(sym) = store.get(*s) {
                *per_module.entry(store.files.path(sym.loc.file).to_string()).or_default() += 1;
            }
        }
        if let Some((best_path, _)) = per_module.iter().max_by_key(|(_, c)| **c) {
            if concept.extent.len() >= 2 {
                if let Some(&mn) = module_nodes.get(best_path) {
                    if let Some(node) = dag.get_mut(id) {
                        node.extra_edges.push((mn, EdgeReason::ClusterCohesion));
                    }
                    if let Some(mnode) = dag.get_mut(mn) {
                        if !mnode.children.contains(&id) {
                            mnode.children.push(id);
                        }
                    }
                }
            }
        }

        // Extra DAG edges: every class this concept's member functions
        // actually belong to (FcaSubsumption) — the precise set it cuts
        // across, not just the one majority file above. Recorded ONLY on
        // the UseCase's own `extra_edges` (traceable, never read by
        // `rollup_node`) — NOT mirrored into the class's `children`: that
        // list feeds weakest-link confidence rollup (FR-43), and an
        // inferred (0.8-confidence) cross-cutting concept must never drag
        // down a recorded (1.0-confidence) class/component's rolled-up
        // certainty just because one of its methods also participates in
        // this concept.
        let mut linked_classes: BTreeSet<ConceptNodeId> = BTreeSet::new();
        for s in &concept.extent {
            let Some(sym) = store.get(*s) else { continue };
            let Some(owner) = sym.parent else { continue };
            let Some(&class_id) = node_of_symbol.get(&owner) else { continue };
            if !linked_classes.insert(class_id) {
                continue;
            }
            if let Some(node) = dag.get_mut(id) {
                node.extra_edges.push((class_id, EdgeReason::FcaSubsumption));
            }
        }
    }

    // ---- Rule nodes from explicit contracts (CallOwnership edges) ----
    let mut rules_added: BTreeSet<u32> = BTreeSet::new();
    for c in contracts {
        let kind = if matches!(c.authority, cie_contract::Authority::Explicit) {
            ConceptKind::Rule
        } else {
            ConceptKind::Invariant
        };
        let mut n = ConceptNode::new(0, c.expr.clone(), kind, now);
        n.primary_edge = EdgeReason::CallOwnership;
        n.provenance = "rule:contract".into();
        n.symbols = c.symbols.clone();
        n.rules.push(RuleRef {
            contract: c.id,
            kind: c.obligation.as_str(),
            authority: c.authority,
        });
        n.confidence = match c.authority {
            cie_contract::Authority::Explicit => 1.0,
            _ => 0.6,
        };
        let id = dag.push(n);
        rules_added.insert(c.id);
        // Parent: node containing the first bound symbol.
        let parent = c
            .symbols
            .first()
            .and_then(|s| node_of_symbol.get(s).copied())
            .or_else(|| {
                c.symbols
                    .first()
                    .and_then(|s| store.get(*s))
                    .and_then(|sym| module_nodes.get(store.files.path(sym.loc.file)).copied())
            })
            .unwrap_or(root);
        dag.link_child(parent, id);
    }

    // ---- Rollup (FR-43) ----
    dag.rollup();

    // ---- Unattached lists (acceptance #28) ----
    let mut attached_summaries: BTreeSet<SymbolId> = BTreeSet::new();
    let mut attached_contracts: BTreeSet<u32> = BTreeSet::new();
    for n in &dag.nodes {
        for b in &n.behaviors {
            attached_summaries.insert(b.summary);
        }
        for r in &n.rules {
            attached_contracts.insert(r.contract);
        }
    }
    for (id, summary) in &behavior.summaries {
        if !attached_summaries.contains(id) {
            dag.unattached_behaviors.push(BehaviorRef {
                summary: *id,
                kind: store
                    .get(*id)
                    .map(|s| s.qual_name.clone())
                    .unwrap_or_default(),
                confidence: 1.0,
            });
        }
        let _ = summary;
    }
    for c in contracts {
        if !attached_contracts.contains(&c.id) {
            dag.unattached_rules.push(RuleRef {
                contract: c.id,
                kind: c.obligation.as_str(),
                authority: c.authority,
            });
        }
    }

    let _ = behaviors_added; // retained for future incremental invalidation
    dag
}

/// A5: the usecase-first projection. One record per UseCase node (id
/// order, deterministic), carrying everything a view needs so the web
/// layer never re-derives it:
///   * `chain` — the primary-parent climb up to the Domain root (the same
///     walk `depth_of`/`invalidate` speak, so invalidation waves and the
///     reader's story stay one structure);
///   * `narrower` — immediate UseCase children;
///   * `members` — the extent resolved to names/locations/owners;
///   * `member_classes` — the class-like FcaSubsumption extra edges,
///     resolved, each with exactly the extent members that belong to it
///     (derived from `sym.parent`, the same rule pass 4/5 used to place
///     the member in the first place).
/// Pure function of (dag, store): no behavior summaries, no stack, no
/// labels — labels never decide membership, and this projection is
/// membership only.
pub fn usecase_projection(
    dag: &ConceptDag,
    store: &SymbolStore,
) -> Vec<cie_concepts::UsecaseView> {
    // Class-like nodes (pass 4 emits Aggregate/Entity/Value for every
    // type-like symbol) → the node's own type-like symbol. A method's
    // `sym.parent` is that symbol, which is how this map mirrors the
    // `node_of_symbol` lookups the hierarchy itself made.
    let mut class_of_owner: BTreeMap<SymbolId, ConceptNodeId> = BTreeMap::new();
    for n in &dag.nodes {
        if !matches!(
            n.kind,
            ConceptKind::Aggregate | ConceptKind::Entity | ConceptKind::Value
        ) {
            continue;
        }
        for sid in &n.symbols {
            if store.get(*sid).map(|s| s.kind.is_type_like()).unwrap_or(false) {
                class_of_owner.insert(*sid, n.id);
                break; // the type-like symbol is unique per class node
            }
        }
    }

    let mut out = Vec::new();
    for n in &dag.nodes {
        if n.kind != ConceptKind::UseCase {
            continue;
        }
        // Primary-parent climb, nearest first, defensive hop cap (the same
        // bound every other chain walk on the DAG carries).
        let mut chain = Vec::new();
        let mut cursor = n.parent;
        let mut hops = 0usize;
        while let Some(p) = cursor {
            match dag.get(p) {
                Some(parent) => {
                    chain.push(cie_concepts::UsecaseLink {
                        id: parent.id,
                        label: parent.label.clone(),
                        kind: parent.kind,
                    });
                    cursor = parent.parent;
                }
                None => break,
            }
            hops += 1;
            if hops > dag.nodes.len() {
                break; // corrupted chain cannot loop forever (FR-7)
            }
        }

        let narrower: Vec<ConceptNodeId> = n
            .children
            .iter()
            .copied()
            .filter(|&c| dag.get(c).map(|c| c.kind == ConceptKind::UseCase).unwrap_or(false))
            .collect();

        let mut members = Vec::with_capacity(n.symbols.len());
        // extent member id -> owning class node id (pass-4 rule: sym.parent)
        let mut per_class: BTreeMap<ConceptNodeId, Vec<SymbolId>> = BTreeMap::new();
        for sid in &n.symbols {
            let Some(sym) = store.get(*sid) else { continue };
            let owner = sym.parent.and_then(|p| store.get(p)).map(|p| p.name.clone());
            members.push(cie_concepts::UsecaseMember {
                symbol: *sid,
                name: sym.name.clone(),
                qual_name: sym.qual_name.clone(),
                file: store.files.path(sym.loc.file).to_string(),
                line: store.files.line(sym.loc),
                owner: owner.clone(),
            });
            if let Some(owner_id) = sym.parent {
                if let Some(&class_node) = class_of_owner.get(&owner_id) {
                    per_class.entry(class_node).or_default().push(*sid);
                }
            }
        }

        let member_classes: Vec<cie_concepts::UsecaseClass> = per_class
            .iter()
            .filter_map(|(&class_node, member_ids)| {
                let cn = dag.get(class_node)?;
                Some(cie_concepts::UsecaseClass {
                    id: cn.id,
                    label: cn.label.clone(),
                    kind: cn.kind,
                    members: member_ids.clone(),
                })
            })
            .collect();

        out.push(cie_concepts::UsecaseView {
            id: n.id,
            label: n.label.clone(),
            intent: n.intent.clone(),
            confidence: n.confidence,
            coverage_ratio: n.coverage_ratio,
            coverage_complete: n.coverage_complete,
            provenance: n.provenance.clone(),
            chain,
            narrower,
            members,
            member_classes,
        });
    }
    out
}

/// RFC-0019 §6.4: does ANY of `methods` (or their callees, bounded at three
/// hops with a visited set) resolve to a symbol whose OWNING class is one of
/// the recorded repository classes? Pure store/summary lookups — no naming
/// heuristics on the caller side; the repo names themselves ARE the evidence.
fn reaches_repository(
    store: &SymbolStore,
    behavior: &BehaviorResult,
    methods: &[SymbolId],
    repo_names: &BTreeSet<String>,
) -> bool {
    if repo_names.is_empty() {
        return false;
    }
    let mut visited: BTreeSet<SymbolId> = methods.iter().copied().collect();
    let mut frontier: Vec<SymbolId> = methods.to_vec();
    for _ in 0..3 {
        let mut next: Vec<SymbolId> = Vec::new();
        for id in &frontier {
            if let Some(sym) = store.get(*id) {
                if let Some(p) = sym.parent {
                    if let Some(psym) = store.get(p) {
                        if repo_names.contains(&psym.name) {
                            return true;
                        }
                    }
                }
            }
            if let Some(s) = behavior.summaries.get(id) {
                for c in &s.callees {
                    if visited.insert(*c) {
                        next.push(*c);
                    }
                }
            }
        }
        if next.is_empty() {
            break;
        }
        frontier = next;
    }
    false
}

fn humanize(name: &str) -> String {
    let mut out = String::new();
    for (i, part) in name.split('_').enumerate() {
        if part.is_empty() {
            continue;
        }
        if i > 0 {
            out.push(' ');
        }
        let mut chars = part.chars();
        if let Some(c) = chars.next() {
            out.extend(c.to_uppercase());
            out.push_str(chars.as_str());
        }
    }
    if out.is_empty() {
        name.to_string()
    } else {
        out
    }
}
