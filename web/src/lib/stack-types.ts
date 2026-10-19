// ---------- the tech-stack artifact ----------
//
// Built engine-side by the rust crate `cie-stack` (`cie stack <dir> --json`):
// manifest detection → factory → the matching stack's parser (Spring over
// java+kotlin first; other stacks are honest stubs). Deterministic reads of
// the project's OWN files — no LLM, no network, nothing executed.
//
// This module is the CLIENT contract only (types + guard). The JSON is
// camelCase, matching serde's rename_all.

export interface TechStack {
  v: 1;
  /** what the build files say, in plain words */
  build: BuildInfo;
  /** JVM = java + kotlin files counted as ONE family (they share the classpath) */
  languages: StackLang[];
  /** frameworks detected from the build files */
  frameworks: StackFramework[];
  /** what the project ships as — docker files read as written */
  docker: DockerInfo;
  /** JVM-family deep facts — ONLY for JVM stacks */
  jvm?: JvmStack | null;
  /** every stack's routes, one flat list (the Master table reads this) */
  routes: RouteSite[];
  /** every stack's mains (spring-boot class, rust fn main, …) */
  mains: MainSite[];
  /** branch-capable per-route data flows — one per handler file, ALL stacks */
  flows: ControllerFlow[];
  /** detected applications (one per manifest found, any depth) */
  apps: AppSite[];
  /** app config file facts (port, profiles, datasource) — as written */
  config: StackConfigEntry[];
  /** plain notes: what the build files didn't say, plus honest caps */
  notes: string[];
  /** how much of the tree the pass actually looked at */
  coverage: { filesScanned: number; manifestsRead: string[]; dockerfilesRead: string[] };
  generatedAt: number;
}

export interface BuildInfo {
  tool: "maven" | "gradle" | "cargo" | "npm" | "go" | "python" | "mix" | "unknown";
  /** module names for multi-module builds (settings.gradle/pom aggregator) */
  modules: string[];
  /** the artifact name as written (jar name, package name…) */
  artifactName: string;
}

export interface StackLang {
  family: "jvm-mixed" | "java" | "kotlin" | "rust" | "javascript" | "typescript" | "python" | "go";
  files: number;
  /** true when java+kotlin both>0 — one runtime, two syntaxes */
  sharedRuntime?: boolean;
}

export interface StackFramework {
  /** "spring-boot" | "axum" | "express" | … */
  id: string;
  /** "Spring Boot 3.2.5" */
  name: string;
  /** the manifest file that grounded the claim */
  evidence: string;
  /** starter modules named in the build file, as written */
  modules: string[];
}

export interface DockerInfo {
  present: boolean;
  dockerfiles: { path: string; baseImage?: string; exposedPorts: number[] }[];
  compose: {
    path: string;
    services: { name: string; image?: string; ports: string[] }[];
  } | null;
  /** one honest line: "read as written; CIE doesn't build or run containers." */
  note: string;
}

export interface StackConfigEntry {
  rel: string;
  port?: number;
  profiles?: string;
  datasource?: string; // url AS WRITTEN (may name an env var — that's fine, quoted as written)
  /** the app directory this config belongs to (multi-app repos) */
  service?: string;
  note: string;
}

/** one detected application — a manifest at any depth starts an app */
export interface AppSite {
  /** the directory the build file lives in ("" = the repo root) */
  dir: string;
  name: string;
  build: string;
  framework: string;
  evidence: string;
}

// ---- generic per-stack routes + mains (every parser fills these) ----

export interface RouteSite {
  verb: string; // "GET" | "POST" | "ALL" (not stated)
  path: string;
  /** class/module the handler lives in */
  owner: string;
  method: string;
  rel: string;
  line: number;
  params: string[];
  returns: string;
  /** the annotation/route text that grounded the claim */
  evidence: string;
  /** the app directory this route serves (multi-app repos); "" = root */
  service?: string;
}

export interface MainSite {
  name: string;
  rel: string;
  line: number;
  /** "spring-boot" | "main-fn" */
  kind: string;
  lang: string;
}

// ---- JVM deep facts (spring parser) ----

export interface JvmStack {
  configClasses: JvmConfigCls[];
  services: JvmService[];
  repositories: JvmRepository[];
  entities: JvmEntity[];
  /** @Scheduled across ALL classes (services, configs, components) */
  scheduledIn: JvmScheduled[];
  /** kafka/rabbit/jms listeners anywhere in the code — feeds the mains graph */
  listeners: JvmListener[];
  /** outbound messaging/http the code itself names */
  outbound: JvmOutbound[];
  /** .java + .kt files scanned (NOT counting src/test) */
  scanned: number;
  javaFiles: number;
  kotlinFiles: number;
}

export interface JvmConfigCls {
  name: string;
  rel: string;
  line: number;
  lang: string;
  /** "configuration" | "security" | "feign" | "messaging" */
  kind: string;
}

export interface JvmService {
  name: string;
  rel: string;
  line: number;
  lang: string;
  injections: Injection[];
}

export interface Injection {
  field: string;
  type: string; // as written, generics included
}

export interface JvmRepository {
  name: string;
  rel: string;
  line: number;
  lang: string;
  extends: string; // the interface clause as written
  entity?: string;
}

export interface JvmEntity {
  name: string;
  rel: string;
  line: number;
  lang: string;
  table?: string;
  props: Prop[];
}

export interface Prop {
  name: string;
  type: string;
}

export interface JvmScheduled {
  cls: string;
  method: string;
  /** cron="..." / fixedRate=... AS WRITTEN */
  trigger: string;
  rel: string;
  line: number;
}

export interface JvmListener {
  cls: string;
  method: string;
  target: string; // the topic/queue annotation argument
  rel: string;
  line: number;
}

export interface JvmOutbound {
  /** "rest-template" | "web-client" | "feign" | "kafka-producer" */
  kind: string;
  cls: string;
  rel: string;
}

export interface ControllerFlow {
  name: string;
  rel: string;
  basePath: string;
  flows: HandlerFlow[];
}

export interface HandlerFlow {
  method: string;
  verb: string;
  path: string;
  root: FlowNode;
  /** the honest cap + never-evaluated sentence, one line */
  honest: string;
}

/** One node of the branch-capable data flow. Decisions carry children =
 *  branch arms; every node cites file+line. All text AS WRITTEN. */
export interface FlowNode {
  id: string;
  /** "request" | "action" | "decision" | "throw" | "return" */
  kind: string;
  label: string;
  /** "client"|"controller"|"service"|"repository"|"database"|"response"|"decision"|"other" */
  layer: string;
  type?: string | null;
  rel?: string | null;
  line?: number | null;
  children: FlowBranch[];
}

export interface FlowBranch {
  /** "if (x > cap)", "else", '"CLEAR"' — as written */
  label: string;
  nodes: FlowNode[];
}

/** client-safe, no imports needed elsewhere */
export function isTechStack(o: unknown): o is TechStack {
  const t = o as TechStack | null;
  return !!t && typeof t === "object" && t.v === 1 && !!t.build && Array.isArray(t.frameworks) && !!t.docker && Array.isArray(t.routes);
}

/** one line for the step list, honest about what was and wasn't parsed */
export function stackDetail(s: TechStack): string {
  const parts: string[] = [];
  const jvm = s.jvm;
  if (jvm) {
    parts.push(`${s.routes.length} route(s), ${jvm.services.length} service(s), ${jvm.repositories.length} repository(ies), ${jvm.entities.length} entity/entities, ${jvm.scheduledIn.length} scheduled, ${jvm.listeners.length} listener(s)`);
  }
  if (s.apps && s.apps.length > 1) {
    parts.push(
      `${s.apps.length} application(s) detected: ${s.apps.slice(0, 5).map((a) => `${a.dir || "root"} (${a.framework})`).join(", ")}`
    );
  }
  if (s.docker.present) {
    const dports = s.docker.dockerfiles.flatMap((d) => d.exposedPorts);
    const cports = s.docker.compose?.services.flatMap((x) => x.ports) ?? [];
    const all = [...new Set([...dports.map(String), ...cports])];
    parts.push(
      `${s.docker.compose?.services.length ?? 0} compose service(s)${s.docker.dockerfiles.length ? ", " + s.docker.dockerfiles.length + " Dockerfile(s)" : ""}${all.length ? `, ports ${all.join(", ")}` : ""}`
    );
  }
  if (s.languages.length) {
    parts.push(s.languages.map((l) => `${l.files} ${l.family} file(s)`).join(" · "));
  }
  return parts.join(" — ");
}