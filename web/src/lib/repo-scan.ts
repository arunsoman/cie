// Server-only repo scanner: walks a folder (bounded, read-only), counts the
// languages by extension using the engine-mirrored tables in
// language-profiles.ts, and reads the small manifest files (package.json,
// pom.xml, Cargo.toml, …) to detect frameworks.
//
// This is the PREVIEW + honesty report. The engine itself remains the source
// of truth for what actually gets parsed — this module never reads code file
// contents, only names, extensions and manifests.
//
// Must never be imported from client code.

import { promises as fs } from "fs";
import path from "path";
import {
  SUPPORTED_LANGUAGES,
  SUPPORTED_SUMMARY,
  supportedLanguageForExtension,
  unsupportedLanguageForExtension,
  KNOWN_UNSUPPORTED,
  FRAMEWORK_PROFILES,
  type RepoScan,
  type RepoLangCount,
  type RepoFrameworkHit,
} from "./language-profiles";

const UNSUPPORTED_BY_ID = new Map(KNOWN_UNSUPPORTED.map((l) => [l.id, l]));

// ---------- walk limits (mirrors the engine's ignore list + caches) ----------

const IGNORED_DIRS = new Set([
  // exactly the engine's §17.2 list
  ".git", "node_modules", "target", "vendor", ".venv", "dist", "build",
  "venv", "__pycache__", ".cie", ".idea", ".vscode",
  // common caches/outputs that are virtually always skipped too
  ".next", "out", "coverage", ".tox", ".mypy_cache", ".pytest_cache",
  ".gradle", "obj", "Pods", ".cache", ".turbo", "site-packages",
]);

const MAX_FILES = 60_000;
const MAX_DEPTH = 14;
const MAX_MANIFEST_BYTES = 512 * 1024;
const SCAN_BUDGET_MS = 20_000;

interface WalkResult {
  files: string[]; // absolute paths of non-ignored files (capped)
  truncated: boolean;
}

async function walk(dir: string, depth: number, acc: WalkResult, deadline: number): Promise<void> {
  if (acc.files.length >= MAX_FILES || depth > MAX_DEPTH || Date.now() > deadline) {
    if (Date.now() > deadline || acc.files.length >= MAX_FILES) acc.truncated = true;
    return;
  }
  let entries;
  try {
    entries = await fs.readdir(dir, { withFileTypes: true });
  } catch {
    return; // unreadable dir (permissions, symlink loops) — skip quietly
  }
  for (const e of entries) {
    if (acc.files.length >= MAX_FILES || Date.now() > deadline) {
      acc.truncated = true;
      return;
    }
    const full = path.join(dir, e.name);
    if (e.isDirectory()) {
      if (IGNORED_DIRS.has(e.name)) continue;
      await walk(full, depth + 1, acc, deadline);
    } else if (e.isFile()) {
      acc.files.push(full);
      if (acc.files.length >= MAX_FILES) acc.truncated = true;
    }
  }
}

// ---------- manifest helpers ----------

async function readTextIfExists(file: string): Promise<string | null> {
  try {
    const st = await fs.stat(file);
    if (!st.isFile() || st.size > MAX_MANIFEST_BYTES) return null;
    return await fs.readFile(file, "utf8");
  } catch {
    return null;
  }
}

async function dirExists(dir: string, name: string): Promise<boolean> {
  try {
    const st = await fs.stat(path.join(dir, name));
    return st.isDirectory();
  } catch {
    return false;
  }
}

async function fileExists(dir: string, name: string): Promise<boolean> {
  try {
    const st = await fs.stat(path.join(dir, name));
    return st.isFile();
  } catch {
    return false;
  }
}

function depsOf(pkg: Record<string, unknown>): string[] {
  const out: string[] = [];
  for (const key of ["dependencies", "devDependencies", "peerDependencies"]) {
    const obj = pkg[key];
    if (obj && typeof obj === "object") out.push(...Object.keys(obj as Record<string, string>));
  }
  return out;
}

interface FrameworkDraft {
  id: string;
  detail?: string; // e.g. "app router"
}

async function detectFrameworks(dir: string): Promise<FrameworkDraft[]> {
  const hits: FrameworkDraft[] = [];
  const push = (id: string, detail?: string) => {
    if (!hits.some((h) => h.id === id)) hits.push({ id, detail });
  };

  // --- Node / JS ecosystem ---
  const pkgRaw = await readTextIfExists(path.join(dir, "package.json"));
  if (pkgRaw) {
    try {
      const pkg = JSON.parse(pkgRaw) as Record<string, unknown>;
      const deps = depsOf(pkg);
      const has = (n: string) => deps.some((d) => d === n || d.startsWith(`${n}/`));
      if (has("next")) {
        const appRouter = (await dirExists(dir, "app")) || (await dirExists(dir, "src/app"));
        const pagesRouter = (await dirExists(dir, "pages")) || (await dirExists(dir, "src/pages"));
        push("nextjs", appRouter ? "app router" : pagesRouter ? "pages router" : undefined);
      }
      if (has("react")) push("react");
      if (has("vue")) push("vue");
      if (has("svelte")) push("svelte");
      if (has("express")) push("express");
      if (has("@nestjs/core")) push("nestjs");
    } catch {
      /* unparseable package.json — no framework claims from it */
    }
  }
  if (
    (await fileExists(dir, "next.config.js")) ||
    (await fileExists(dir, "next.config.ts")) ||
    (await fileExists(dir, "next.config.mjs"))
  ) {
    push("nextjs", (await dirExists(dir, "app")) || (await dirExists(dir, "src/app")) ? "app router" : undefined);
  }

  // --- Java ecosystem ---
  const pom = await readTextIfExists(path.join(dir, "pom.xml"));
  if (pom && /spring-boot|org\.springframework\.boot/i.test(pom)) push("spring-boot");
  for (const gradle of ["build.gradle", "build.gradle.kts"]) {
    const g = await readTextIfExists(path.join(dir, gradle));
    if (g && /org\.springframework\.boot/i.test(g)) push("spring-boot");
  }

  // --- Rust ecosystem ---
  const cargo = await readTextIfExists(path.join(dir, "Cargo.toml"));
  if (cargo) {
    push("rust"); // any Cargo project — the plain "Rust project" profile
    if (/\baxum\b/.test(cargo)) push("axum");
    if (/actix-web/.test(cargo)) push("actix");
    if (/\brocket\b/.test(cargo)) push("rocket");
  }

  // --- Go ecosystem ---
  const gomod = await readTextIfExists(path.join(dir, "go.mod"));
  if (gomod) {
    if (/gin-gonic/.test(gomod)) push("gin");
  }

  // --- Python ecosystem ---
  const req = (await readTextIfExists(path.join(dir, "requirements.txt"))) ?? "";
  const pyproject = (await readTextIfExists(path.join(dir, "pyproject.toml"))) ?? "";
  const pyBoth = req + "\n" + pyproject;
  if (/^\s*django[\s=>~<]/im.test(pyBoth) || /django/i.test(pyproject)) push("django");
  if (/^\s*flask[\s=>~<]/im.test(pyBoth) || /flask/i.test(pyproject)) push("flask");
  if (/^\s*fastapi[\s=>~<]/im.test(pyBoth) || /fastapi/i.test(pyproject)) push("fastapi");

  // --- PHP ecosystem ---
  const composer = await readTextIfExists(path.join(dir, "composer.json"));
  if (composer) {
    if (/laravel\/framework/.test(composer)) push("laravel");
    if (/symfony\//.test(composer)) push("symfony");
  }

  // --- .NET ecosystem ---
  // any *.csproj at the root mentioning the Web SDK
  try {
    const rootEntries = await fs.readdir(dir, { withFileTypes: true });
    for (const e of rootEntries) {
      if (e.isFile() && e.name.endsWith(".csproj")) {
        const csproj = await readTextIfExists(path.join(dir, e.name));
        if (csproj && /Microsoft\.NET\.Sdk\.Web/i.test(csproj)) push("aspnet");
      }
    }
  } catch {
    /* root unreadable — handled by the caller */
  }

  return hits;
}

// ---------- the scan ----------

export async function scanRepoLanguages(dir: string, repoName: string): Promise<RepoScan> {
  const deadline = Date.now() + SCAN_BUDGET_MS;
  const acc: WalkResult = { files: [], truncated: false };
  await walk(dir, 0, acc, deadline);

  const supportedCounts = new Map<string, number>();
  const unsupportedCounts = new Map<string, number>();
  let other = 0;

  for (const file of acc.files) {
    const ext = path.extname(file).slice(1).toLowerCase();
    if (!ext) {
      other += 1;
      continue;
    }
    const sup = supportedLanguageForExtension(ext);
    if (sup) {
      supportedCounts.set(sup.id, (supportedCounts.get(sup.id) ?? 0) + 1);
      continue;
    }
    const unsup = unsupportedLanguageForExtension(ext);
    if (unsup) {
      unsupportedCounts.set(unsup.id, (unsupportedCounts.get(unsup.id) ?? 0) + 1);
      continue;
    }
    other += 1;
  }

  const languages: RepoLangCount[] = SUPPORTED_LANGUAGES.filter((l) => supportedCounts.has(l.id))
    .map((l) => ({ id: l.id, name: l.name, files: supportedCounts.get(l.id) ?? 0 }))
    .sort((a, b) => b.files - a.files);

  const unsupported: RepoLangCount[] = [...unsupportedCounts.entries()]
    .map(([id, files]) => ({ id, name: UNSUPPORTED_BY_ID.get(id)?.name ?? id, files }))
    .sort((a, b) => b.files - a.files);

  const frameworks: RepoFrameworkHit[] = (await detectFrameworks(dir)).flatMap((h) => {
    const profile = FRAMEWORK_PROFILES[h.id];
    if (!profile) return [];
    return [
      {
        id: profile.id,
        name: h.detail ? `${profile.name} (${h.detail})` : profile.name,
        languageId: profile.languageId,
        plain: profile.plain,
        honesty: profile.honesty,
      },
    ];
  });

  const readableTotal = languages.reduce((n, l) => n + l.files, 0);
  const unreadableTotal = unsupported.reduce((n, l) => n + l.files, 0);

  const readPlain =
    readableTotal === 0
      ? "No files here use a language CIE can read yet."
      : readableTotal === acc.files.length && unreadableTotal === 0
        ? `CIE can read every code file here — ${languages.map((l) => `${l.name} (${l.files})`).join(", ")}.`
        : `CIE will read ${languages.map((l) => `${l.name} (${l.files})`).join(", ")}.`;

  const unsupportedPlain =
    unreadableTotal === 0
      ? ""
      : `CIE can't read ${unsupported.map((l) => `${l.name} (${l.files})`).join(", ")} yet. ${SUPPORTED_SUMMARY} Anything in those files won't appear in the map or the lists — and CIE won't guess about them.`;

  return {
    dir,
    name: repoName,
    fileCount: acc.files.length,
    otherFiles: other,
    languages,
    unsupported,
    frameworks,
    readPlain,
    unsupportedPlain,
    allReadable: readableTotal > 0 && unreadableTotal === 0,
    nothingReadable: readableTotal === 0,
  };
}
