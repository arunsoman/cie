// The language inventory — one honest source of truth for everything the
// dashboard says about "what CIE can read".
//
// The SUPPORTED table mirrors the engine's registry exactly
// (cie-ingest/src/lib.rs → default_registry + each adapter's extensions()).
// When the engine gains a language, this table gains the same row — nothing
// here is claimed unless the engine really parses it.
//
// Safe to import from client and server code (pure data, no node imports).

// ---------- supported languages (engine registry, v1.2.0) ----------

export interface SupportedLanguage {
  id: string;
  name: string;
  /** Bare extensions (no dot), exactly as the engine adapter claims them. */
  extensions: string[];
  /** Plain-words: what CIE actually reads in these files. */
  reads: string;
  /** Plain-words honesty note: what CIE does NOT do with this language. */
  limits: string;
}

export const SUPPORTED_LANGUAGES: SupportedLanguage[] = [
  {
    id: "python",
    name: "Python",
    extensions: ["py", "pyw"],
    reads: "modules, functions, classes, imports and calls — plus signs of reading/writing files, network calls and database access.",
    limits: "It reads the code as written. It doesn't run it, so anything that only happens while the program runs stays out of sight.",
  },
  {
    id: "javascript",
    name: "JavaScript",
    extensions: ["js", "jsx", "mjs", "cjs"],
    reads: "functions, classes, imports and calls — including React-style JSX components — plus file, network and database effects it can spot.",
    limits: "It reads the code as written. It doesn't run a browser or a server, so runtime behaviour stays out of sight.",
  },
  {
    id: "typescript",
    name: "TypeScript",
    extensions: ["ts", "tsx"],
    reads: "functions, classes, imports and calls — including typed React components — with the same effect-spotting as JavaScript.",
    limits: "It reads the code as written. It doesn't type-check the project or run it.",
  },
  {
    id: "java",
    name: "Java",
    extensions: ["java"],
    reads: "classes, methods, fields, imports and calls — the everyday shape of a Java project, from controllers to helpers.",
    limits: "It reads the code as written. It doesn't run the JVM or know what the libraries do behind the scenes.",
  },
  {
    id: "rust",
    name: "Rust",
    extensions: ["rs"],
    reads: "functions, structs, traits, modules, imports and calls — following the module tree the way the code lays it out.",
    limits: "It reads the code as written. It doesn't compile it, run the borrow checker, or execute macros.",
  },
  {
    id: "go",
    name: "Go",
    extensions: ["go"],
    reads: "functions, types, imports and calls, plus file, network and database effects it can spot.",
    limits: "It reads the code as written. It doesn't run the goroutines or watch what the runtime does.",
  },
  {
    id: "c",
    name: "C",
    extensions: ["c", "h"],
    reads: "functions, structs, includes and calls — including memory and file operations it can spot.",
    limits: "It reads the code as written. It doesn't run it or trace the pointers at runtime.",
  },
  {
    id: "cpp",
    name: "C++",
    extensions: ["cpp", "cc", "cxx", "hpp", "hh"],
    reads: "classes, functions, includes and calls — the everyday shape of a C++ project.",
    limits: "It reads the code as written. Templates that only resolve at compile time stay out of sight.",
  },
  {
    id: "csharp",
    name: "C#",
    extensions: ["cs"],
    reads: "classes, methods, imports and calls — the everyday shape of a .NET project.",
    limits: "It reads the code as written. It doesn't run the runtime or know what the framework does behind the scenes.",
  },
  {
    id: "php",
    name: "PHP",
    extensions: ["php", "php3", "php4", "php5", "phtml"],
    reads: "functions, classes, includes and calls, plus file, network and database effects it can spot.",
    limits: "It reads the code as written. It doesn't serve pages or run the web server.",
  },
  {
    id: "sql",
    name: "SQL",
    extensions: ["sql", "ddl"],
    reads: "tables, views, stored procedures and functions, CREATE EXTENSION imports, and the table-to-table links the statements write down — foreign keys, the tables a view reads, the tables a statement changes.",
    limits: "It reads the statements as written. It doesn't run them, resolve schema search paths, or follow SQL built at runtime.",
  },
  {
    id: "shell",
    name: "Shell scripts",
    extensions: ["sh", "bash", "zsh"],
    reads: "functions, sourced files, the external commands scripts run, and the effects those commands have — file reads and writes, network calls, database tools, process execution.",
    limits: "It reads the script as written. It doesn't run it or expand variables, so commands hidden behind $VARS stay unknown — and it says so.",
  },
];

/** Plain sentence naming every supported language (used in honest empty states). */
export const SUPPORTED_SUMMARY = `Right now CIE reads ${SUPPORTED_LANGUAGES.length} languages: ${SUPPORTED_LANGUAGES.map((l) => l.name).join(", ")}.`;

// ---------- frameworks (manifest-level detection, plain words) ----------
// Framework detection is honest about its depth: CIE sees the manifest and
// the code — it does not gain framework superpowers. Every profile says so.

export interface FrameworkProfile {
  id: string;
  name: string;
  /** language id from SUPPORTED_LANGUAGES this framework is written in */
  languageId: string;
  /** Plain-words: what CIE sees in a project like this. */
  plain: string;
  /** Plain-words honesty note about the limits. */
  honesty: string;
}

export const FRAMEWORK_PROFILES: Record<string, FrameworkProfile> = {
  "spring-boot": {
    id: "spring-boot",
    name: "Spring Boot",
    languageId: "java",
    plain:
      "CIE reads every Java file — controllers, services, repositories, configuration classes — and maps how they call each other.",
    honesty:
      "CIE reads it as ordinary Java. It doesn't start Spring, so anything that happens only inside the framework's magic (injection, proxies, auto-configuration) stays out of the map.",
  },
  nextjs: {
    id: "nextjs",
    name: "Next.js",
    languageId: "typescript",
    plain:
      "CIE reads your pages, components and API routes as ordinary TypeScript/JavaScript and maps how they connect.",
    honesty:
      "CIE reads the code as written. It doesn't render pages or run the server, so what the framework does at request time stays out of the map.",
  },
  rust: {
    id: "rust",
    name: "Rust",
    languageId: "rust",
    plain:
      "CIE reads your source files and follows the module tree — functions, structs, traits and how they call each other.",
    honesty:
      "CIE reads the code as written. It doesn't compile it or run the borrow checker, so macro-generated code stays out of the map.",
  },
  react: {
    id: "react",
    name: "React",
    languageId: "javascript",
    plain: "CIE reads your components as ordinary JavaScript/TypeScript functions and classes.",
    honesty: "It sees the component code — not what the browser paints.",
  },
  vue: {
    id: "vue",
    name: "Vue",
    languageId: "javascript",
    plain: "CIE reads the JavaScript/TypeScript parts of your project.",
    honesty: ".vue single-file components are their own format — CIE can't read those files yet.",
  },
  svelte: {
    id: "svelte",
    name: "Svelte",
    languageId: "javascript",
    plain: "CIE reads the JavaScript/TypeScript parts of your project.",
    honesty: ".svelte files are their own format — CIE can't read those files yet.",
  },
  express: {
    id: "express",
    name: "Express",
    languageId: "javascript",
    plain: "CIE reads your routes and handlers as ordinary JavaScript.",
    honesty: "It reads the code as written — it doesn't send requests to the running server.",
  },
  nestjs: {
    id: "nestjs",
    name: "NestJS",
    languageId: "typescript",
    plain: "CIE reads your modules, controllers and services as ordinary TypeScript.",
    honesty: "Dependency injection happens at runtime — CIE sees the classes, not the wiring.",
  },
  django: {
    id: "django",
    name: "Django",
    languageId: "python",
    plain: "CIE reads your views, models and helpers as ordinary Python.",
    honesty: "CIE doesn't run the server, so URL routing and ORM magic stay at the code's surface.",
  },
  flask: {
    id: "flask",
    name: "Flask",
    languageId: "python",
    plain: "CIE reads your routes and handlers as ordinary Python.",
    honesty: "It reads the code as written — it doesn't send requests to the running app.",
  },
  fastapi: {
    id: "fastapi",
    name: "FastAPI",
    languageId: "python",
    plain: "CIE reads your endpoints and helpers as ordinary Python.",
    honesty: "It reads the code as written — it doesn't call the API.",
  },
  axum: {
    id: "axum",
    name: "Axum",
    languageId: "rust",
    plain: "CIE reads your handlers and modules as ordinary Rust.",
    honesty: "It reads the code as written — it doesn't serve requests.",
  },
  actix: {
    id: "actix",
    name: "Actix Web",
    languageId: "rust",
    plain: "CIE reads your handlers and modules as ordinary Rust.",
    honesty: "It reads the code as written — it doesn't serve requests.",
  },
  rocket: {
    id: "rocket",
    name: "Rocket",
    languageId: "rust",
    plain: "CIE reads your handlers and modules as ordinary Rust.",
    honesty: "It reads the code as written — it doesn't serve requests.",
  },
  gin: {
    id: "gin",
    name: "Gin",
    languageId: "go",
    plain: "CIE reads your handlers as ordinary Go.",
    honesty: "It reads the code as written — it doesn't call the endpoints.",
  },
  laravel: {
    id: "laravel",
    name: "Laravel",
    languageId: "php",
    plain: "CIE reads your classes and functions as ordinary PHP.",
    honesty: "CIE doesn't run the framework, so its behind-the-scenes wiring stays out of the map.",
  },
  symfony: {
    id: "symfony",
    name: "Symfony",
    languageId: "php",
    plain: "CIE reads your classes and functions as ordinary PHP.",
    honesty: "CIE doesn't run the framework, so its behind-the-scenes wiring stays out of the map.",
  },
  aspnet: {
    id: "aspnet",
    name: "ASP.NET Core",
    languageId: "csharp",
    plain: "CIE reads your classes and methods as ordinary C#.",
    honesty: "CIE doesn't run the server, so framework behaviour stays out of the map.",
  },
};

// ---------- known-but-unsupported languages ----------
// Languages real repos commonly use that the engine does NOT parse yet.
// Each has a plain-words "why" so the honesty report never feels like a dead end.

export interface UnsupportedLanguage {
  id: string;
  name: string;
  extensions: string[];
  /** Plain-words: what this language is usually for, and that CIE skips it. */
  why: string;
}

export const KNOWN_UNSUPPORTED: UnsupportedLanguage[] = [
  { id: "ruby", name: "Ruby", extensions: ["rb", "erb", "rake"], why: "used for scripts and web backends — CIE skips these files for now." },
  { id: "kotlin", name: "Kotlin", extensions: ["kt", "kts"], why: "common in Android apps and Gradle builds — CIE skips these files for now." },
  { id: "swift", name: "Swift", extensions: ["swift"], why: "used for iPhone and Mac apps — CIE skips these files for now." },
  { id: "scala", name: "Scala", extensions: ["scala"], why: "used for big-data and JVM projects — CIE skips these files for now." },
  { id: "dart", name: "Dart", extensions: ["dart"], why: "used for Flutter apps — CIE skips these files for now." },
  { id: "perl", name: "Perl", extensions: ["pl", "pm"], why: "used for text processing and scripts — CIE skips these files for now." },
  { id: "haskell", name: "Haskell", extensions: ["hs", "lhs"], why: "used for academic and finance code — CIE skips these files for now." },
  { id: "lua", name: "Lua", extensions: ["lua"], why: "used for games and embedded scripts — CIE skips these files for now." },
  { id: "elixir", name: "Elixir", extensions: ["ex", "exs"], why: "used for realtime web servers — CIE skips these files for now." },
  { id: "clojure", name: "Clojure", extensions: ["clj", "cljs", "cljc", "edn"], why: "used for data and JVM projects — CIE skips these files for now." },
  { id: "zig", name: "Zig", extensions: ["zig"], why: "used for systems code — CIE skips these files for now." },
  { id: "nim", name: "Nim", extensions: ["nim"], why: "used for systems code — CIE skips these files for now." },
  { id: "r", name: "R", extensions: ["r"], why: "used for statistics — CIE skips these files for now." },
  { id: "objc", name: "Objective-C", extensions: ["m", "mm"], why: "used in older Apple apps — CIE skips these files for now." },
  { id: "vue-sfc", name: "Vue components", extensions: ["vue"], why: "single-file components — CIE reads the project's JS/TS instead." },
  { id: "svelte-sfc", name: "Svelte components", extensions: ["svelte"], why: "single-file components — CIE reads the project's JS/TS instead." },
];

// ---------- extension lookup ----------

const SUPPORTED_BY_EXT = new Map<string, SupportedLanguage>();
for (const lang of SUPPORTED_LANGUAGES) {
  for (const ext of lang.extensions) {
    if (!SUPPORTED_BY_EXT.has(ext)) SUPPORTED_BY_EXT.set(ext, lang);
  }
}
const UNSUPPORTED_BY_EXT = new Map<string, UnsupportedLanguage>();
for (const lang of KNOWN_UNSUPPORTED) {
  for (const ext of lang.extensions) {
    if (!UNSUPPORTED_BY_EXT.has(ext)) UNSUPPORTED_BY_EXT.set(ext, lang);
  }
}

export function supportedLanguageForExtension(ext: string): SupportedLanguage | null {
  return SUPPORTED_BY_EXT.get(ext.replace(/^\./, "").toLowerCase()) ?? null;
}

export function unsupportedLanguageForExtension(ext: string): UnsupportedLanguage | null {
  return UNSUPPORTED_BY_EXT.get(ext.replace(/^\./, "").toLowerCase()) ?? null;
}

// ---------- the shared scan shape (server fills it, client renders it) ----------
// The RepoScan type lives here so client code never imports a node-only module.

export interface RepoLangCount {
  id: string;
  name: string;
  files: number;
}

export interface RepoFrameworkHit {
  id: string; // FrameworkProfile id
  name: string; // display name (may include detail, e.g. "Next.js (app router)")
  languageId: string;
  plain: string;
  honesty: string;
}

export interface RepoScan {
  dir: string;
  name: string;
  fileCount: number; // code-ish files walked (after ignores)
  otherFiles: number; // files with no known language (docs, configs, styles…)
  languages: RepoLangCount[]; // supported, biggest first
  unsupported: RepoLangCount[]; // known-but-unsupported, biggest first
  frameworks: RepoFrameworkHit[];
  readPlain: string;
  unsupportedPlain: string;
  allReadable: boolean; // every known-language file is readable by CIE
  nothingReadable: boolean; // no supported-language files at all
}
