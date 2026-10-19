//! The stack artifact — serde structs, camelCase to match the web client's
//! TypeScript contract (web/src/lib/stack-types.ts). Every record carries
//! its own file+line so a UI row can open the real code.

use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StackReport {
    pub v: u32,
    pub build: BuildInfo,
    pub languages: Vec<StackLang>,
    pub frameworks: Vec<StackFramework>,
    pub docker: DockerInfo,
    /// JVM-family deep facts — present ONLY for JVM stacks (java/kotlin)
    pub jvm: Option<JvmStack>,
    /// every stack's routes, one flat list (the Master table reads this)
    pub routes: Vec<RouteSite>,
    /// every stack's mains (spring-boot class, rust fn main, …)
    pub mains: Vec<MainSite>,
    /// the per-controller data flows across ALL services/languages,
    /// branch-capable (name = "dir/Owner" when a service dir exists)
    pub flows: Vec<ControllerFlow>,
    /// one entry per detected APPLICATION (service module) in the repo
    pub apps: Vec<AppSite>,
    /// application.properties / application.yml style entries, as written
    pub config: Vec<ConfigEntry>,
    pub notes: Vec<String>,
    pub coverage: Coverage,
    pub generated_at: u64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BuildInfo {
    pub tool: String, // "maven" | "gradle" | "cargo" | "npm" | "go" | "mix" | "unknown"
    pub modules: Vec<String>,
    pub artifact_name: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StackLang {
    pub family: String, // "jvm-mixed" | "java" | "kotlin" | "rust" | …
    pub files: u32,
    /// true when java+kotlin both present — they share ONE runtime/classpath
    pub shared_runtime: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StackFramework {
    pub id: String, // "spring-boot" | "axum" | …
    pub name: String,
    pub evidence: String, // the manifest file that claimed it
    pub modules: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DockerInfo {
    pub present: bool,
    pub dockerfiles: Vec<Dockerfile>,
    pub compose: Option<ComposeFile>,
    pub note: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Dockerfile {
    pub path: String,
    pub base_image: Option<String>,
    pub exposed_ports: Vec<u32>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ComposeFile {
    pub path: String,
    pub services: Vec<ComposeService>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ComposeService {
    pub name: String,
    pub image: Option<String>,
    pub ports: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ConfigEntry {
    pub rel: String,
    /// the service (app module) the config belongs to — top-level dir, "" at root
    pub service: String,
    pub port: Option<u32>,
    pub profiles: Option<String>,
    pub datasource: Option<String>,
    pub note: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Coverage {
    pub files_scanned: u32,
    pub manifests_read: Vec<String>,
    pub dockerfiles_read: Vec<String>,
}

// ---- JVM deep facts (spring) ----

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct JvmStack {
    /// the annotated classes that AREN'T one of the main families
    pub config_classes: Vec<JvmConfigCls>,
    pub services: Vec<JvmService>,
    pub repositories: Vec<JvmRepository>,
    pub entities: Vec<JvmEntity>,
    /// @Scheduled across ALL classes (services, configs, components)
    pub scheduled_in: Vec<JvmScheduled>,
    /// kafka/rabbit/jms listeners, anywhere in the code
    pub listeners: Vec<JvmListener>,
    /// outbound http the code itself names (@FeignClient/RestTemplate/WebClient)
    pub outbound: Vec<JvmOutbound>,
    /// .java + .kt files scanned (NOT counting src/test|test)
    pub scanned: u32,
    /// java/kotlin file counts feeding the "one runtime, two syntaxes" note
    pub java_files: u32,
    pub kotlin_files: u32,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct JvmConfigCls {
    pub name: String,
    pub rel: String,
    pub line: u32,
    pub lang: String, // "java" | "kotlin"
    pub kind: String, // "configuration" | "security" | "feign" | "messaging"
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct JvmService {
    pub name: String,
    pub rel: String,
    pub line: u32,
    pub lang: String,
    pub injections: Vec<Injection>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Injection {
    pub field: String,
    pub r#type: String, // as written, generics included
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct JvmRepository {
    pub name: String,
    pub rel: String,
    pub line: u32,
    pub lang: String,
    pub extends: String, // the interface clause as written
    pub entity: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct JvmEntity {
    pub name: String,
    pub rel: String,
    pub line: u32,
    pub lang: String,
    pub table: Option<String>,
    pub props: Vec<Prop>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Prop {
    pub name: String,
    pub r#type: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct JvmScheduled {
    pub cls: String,
    pub method: String,
    /// cron="..." / fixedRate=... AS WRITTEN
    pub trigger: String,
    pub rel: String,
    pub line: u32,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct JvmListener {
    pub cls: String,
    pub method: String,
    pub target: String, // the topic/queue annotation argument
    pub rel: String,
    pub line: u32,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct JvmOutbound {
    pub kind: String, // "rest-template" | "web-client" | "feign"
    pub cls: String,
    pub rel: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ControllerFlow {
    pub name: String,
    pub rel: String,
    pub base_path: String,
    pub flows: Vec<HandlerFlow>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HandlerFlow {
    pub method: String,
    pub verb: String,
    pub path: String,
    pub root: FlowNode,
    /// the honest cap + never-evaluated sentence, one line
    pub honest: String,
}

/// One node of the branch-capable data flow. Decisions carry children =
/// branch arms; every node cites file+line. All text AS WRITTEN (truncated
/// for display at the UI's discretion).
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FlowNode {
    pub id: String,
    pub kind: String, // "request" | "action" | "decision" | "throw" | "return"
    pub label: String,
    pub layer: String, // "client"|"controller"|"service"|"repository"|"database"|"response"|"decision"|"other"
    pub r#type: Option<String>,
    pub rel: Option<String>,
    pub line: Option<u32>,
    pub children: Vec<FlowBranch>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FlowBranch {
    pub label: String, // "if (x > cap)", "else", "case 404:"
    pub nodes: Vec<FlowNode>,
}

// ---- generic per-stack sites (every parser fills these) ----

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RouteSite {
    pub verb: String,
    pub path: String,
    pub owner: String, // class/module the handler lives in
    pub method: String,
    /// the service (app module) this route belongs to — top-level dir, "" at root
    pub service: String,
    pub rel: String,
    pub line: u32,
    pub params: Vec<String>,
    pub returns: String,
    /// the annotation/route text that grounded the claim
    pub evidence: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MainSite {
    pub name: String,
    pub rel: String,
    pub line: u32,
    pub kind: String,  // "spring-boot" | "spring-app" | "main-fn"
    pub lang: String,
}

// ---- the per-APPLICATION roll-up (one service, one paragraph line each) ----

/// one detected application: its dir, the language as detected from its OWN
/// manifest, and the HTTP framework named in that manifest
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AppSite {
    /// dir of the app module, "" when the manifest sits at the root
    pub dir: String,
    /// the app's own name (artifact/dir name as written in its manifest)
    pub name: String,
    /// "maven" | "gradle" | "cargo" | "npm" | "python" | "go" | "unknown"
    pub build: String,
    /// "spring" | "express" | "axum" | "flask" | "fastapi" | "unknown" …
    pub framework: String,
    /// the manifest file that named it
    pub evidence: String,
}