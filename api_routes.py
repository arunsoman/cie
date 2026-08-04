"""Frontend-call <-> backend-route resolver: the one HTTP-API-boundary
question `cie`'s tree-sitter AST layer genuinely cannot answer on its own
(`cie/models.py`'s `NodeKind` has no route/decorator/API-call concept, and
`cie/extract.py` doesn't extract one — see
`be-v2/docs/design/ui-engineer-cie-knowledge-graph.md` §1.C question 6,
which this module closes).

Deliberately NOT modeled as first-class graph nodes/edges inside the
general call-graph — that would be a much bigger schema migration for a
narrow need. Instead this is a small, self-contained, on-demand resolver:

1. Regex-scan every ``.py`` file under ``be-v2/src`` *and* ``backend/src``
   for FastAPI route decorators (``@router.get/post/put/delete/patch(...)``,
   ``@app.get(...)``), capturing method + path template + owning file/line
   + handler function name. Both trees are scanned — not just be-v2 — because
   most routes are still genuinely served by ``backend/`` today (see
   `be-v2/docs/backend-migration-status.md`); this is read-only, factual
   route-shape indexing, not a design reference (the "never use backend/ as
   a design source" CLAUDE.md directive is about feature *design*, not
   about answering "which of the two currently-running services actually
   owns this HTTP path").
2. Regex-scan every ``.ts``/``.tsx`` file under ``frontend/src`` for
   ``fetch(...)``, ``request(...)`` (the ``frontend/src/lib/api.ts`` JSON
   wrapper almost every store calls through), and ``streamFetch(...)``
   (its raw-Response sibling, used for SSE/blob endpoints) call sites,
   capturing the path expression + owning file/line + enclosing
   function/store-action name.
3. Normalize both sides' path templates (``{task_id}``, ``${taskId}``,
   ``:id`` all become a common ``*`` wildcard) and match by same-shape
   comparison.
4. Parse ``frontend/vite.config.js``'s dev-proxy rule list (an ordered set
   of prefix -> target mappings, plus one path-regex-gated router for
   ``/api/projects``) to attribute each frontend call to whichever backend
   — ``be-v2`` or ``backend`` — actually serves it today, not always be-v2.

Computed on demand and cached per process (module-level, keyed by repo
root) — this is read-only text scanning over a few hundred files, fast
enough that a persisted Neo4j edge type isn't worth the schema churn for
this narrow a need. Call :func:`invalidate_cache` after a same-process
edit to route/proxy files if a fresh answer is needed without restarting.
"""

from __future__ import annotations

import re
from dataclasses import dataclass, asdict
from pathlib import Path
from typing import Optional

# ---------------------------------------------------------------------------
# Path normalization — turn every flavor of "this segment is a parameter"
# into one common wildcard so a frontend template and a backend template
# compare as the same shape.
# ---------------------------------------------------------------------------

_TEMPLATE_EXPR_RE = re.compile(r"\$\{[^}]*\}")  # `${taskId}`, `${encodeURIComponent(x)}`
_BRACE_PARAM_RE = re.compile(r"\{[^}]+\}")  # FastAPI `{task_id}`
_COLON_PARAM_RE = re.compile(r":[A-Za-z_][A-Za-z0-9_]*")  # Express-style `:id` (not used here, kept for generality)


def normalize_path(path: str) -> str:
    """Strip query string + collapse every parameter flavor to `*`."""
    path = path.split("?", 1)[0]
    path = _TEMPLATE_EXPR_RE.sub("*", path)
    path = _BRACE_PARAM_RE.sub("*", path)
    path = _COLON_PARAM_RE.sub("*", path)
    path = path.rstrip("/")
    return path or "/"


def _paths_match(a_normalized: str, b_normalized: str) -> bool:
    a_parts = a_normalized.strip("/").split("/")
    b_parts = b_normalized.strip("/").split("/")
    if len(a_parts) != len(b_parts):
        return False
    return all(a == "*" or b == "*" or a == b for a, b in zip(a_parts, b_parts))


# ---------------------------------------------------------------------------
# Backend route extraction
# ---------------------------------------------------------------------------

_ROUTE_DECORATOR_RE = re.compile(
    r'@(?:router|app)\.(get|post|put|delete|patch)\(\s*["\']([^"\']+)["\']',
    re.IGNORECASE,
)
_ROUTER_PREFIX_RE = re.compile(r'APIRouter\([^)]*\bprefix\s*=\s*["\']([^"\']+)["\']')
_DEF_RE = re.compile(r"^\s*(?:async\s+)?def\s+([A-Za-z_][A-Za-z0-9_]*)")
_DECORATOR_LINE_RE = re.compile(r"^\s*@")


@dataclass(frozen=True)
class BackendRoute:
    method: str
    path: str
    normalized_path: str
    file: str
    line: int
    handler: str
    service: str  # "be-v2" | "backend"

    def as_dict(self) -> dict:
        return asdict(self)


def _extract_routes_from_file(path: Path, service: str, repo_root: Path) -> list[BackendRoute]:
    try:
        text = path.read_text(encoding="utf-8", errors="replace")
    except OSError:
        return []
    if "APIRouter" not in text and "FastAPI(" not in text:
        return []
    lines = text.splitlines()
    prefix_match = _ROUTER_PREFIX_RE.search(text)
    prefix = prefix_match.group(1).rstrip("/") if prefix_match else ""
    rel = str(path.relative_to(repo_root))
    routes: list[BackendRoute] = []
    for idx, line in enumerate(lines):
        m = _ROUTE_DECORATOR_RE.search(line)
        if not m:
            continue
        method, route_path = m.group(1).upper(), m.group(2)
        full_path = f"{prefix}{route_path}" if prefix else route_path
        handler = ""
        for j in range(idx + 1, min(idx + 8, len(lines))):
            candidate = lines[j]
            if not candidate.strip() or _DECORATOR_LINE_RE.match(candidate):
                continue
            dm = _DEF_RE.match(candidate)
            if dm:
                handler = dm.group(1)
            break
        routes.append(
            BackendRoute(
                method=method,
                path=full_path,
                normalized_path=normalize_path(full_path),
                file=rel,
                line=idx + 1,
                handler=handler,
                service=service,
            )
        )
    return routes


def extract_backend_routes(repo_root: Path) -> list[BackendRoute]:
    """Every FastAPI route decorator under be-v2/src AND backend/src."""
    routes: list[BackendRoute] = []
    for service, subdir in (("be-v2", "be-v2/src"), ("backend", "backend/src")):
        base = repo_root / subdir
        if not base.is_dir():
            continue
        for py_file in base.rglob("*.py"):
            if "__pycache__" in py_file.parts:
                continue
            routes.extend(_extract_routes_from_file(py_file, service, repo_root))
    return routes


# ---------------------------------------------------------------------------
# Frontend call-site extraction
# ---------------------------------------------------------------------------

_CALL_RE = re.compile(r"\b(?:fetch|request|streamFetch)\(\s*(.*)$")
_STRING_RE = re.compile(r"""[`'"]([^`'"]*)[`'"]""")

_FN_DECL_PATTERNS = (
    re.compile(r"^\s*export\s+(?:async\s+)?function\s+([A-Za-z_$][\w$]*)"),
    re.compile(r"^\s*(?:async\s+)?function\s+([A-Za-z_$][\w$]*)"),
    re.compile(r"^\s*export\s+const\s+([A-Za-z_$][\w$]*)\s*=\s*(?:async\s*)?\("),
    re.compile(r"^\s*const\s+([A-Za-z_$][\w$]*)\s*=\s*(?:async\s*)?\("),
    # Zustand store action shorthand: `kill: async (taskId) => {`
    re.compile(r"^\s*([A-Za-z_$][\w$]*)\s*:\s*async\s*\("),
    re.compile(r"^\s*([A-Za-z_$][\w$]*)\s*\([^)]*\)\s*(?:=>)?\s*\{"),
)


@dataclass(frozen=True)
class FrontendCall:
    path: str
    normalized_path: str
    file: str
    line: int
    enclosing: str

    def as_dict(self) -> dict:
        return asdict(self)


def _enclosing_symbol(lines: list[str], call_line_idx: int) -> str:
    """Nearest preceding function/store-action declaration, scanned upward."""
    for i in range(call_line_idx, -1, -1):
        for pattern in _FN_DECL_PATTERNS:
            m = pattern.match(lines[i])
            if m:
                return m.group(1)
    return ""


def _extract_calls_from_file(path: Path, repo_root: Path) -> list[FrontendCall]:
    try:
        text = path.read_text(encoding="utf-8", errors="replace")
    except OSError:
        return []
    if not any(tok in text for tok in ("fetch(", "request(", "streamFetch(")):
        return []
    lines = text.splitlines()
    rel = str(path.relative_to(repo_root))
    calls: list[FrontendCall] = []
    for idx, line in enumerate(lines):
        m = _CALL_RE.search(line)
        if not m:
            continue
        window = m.group(1)
        # The URL argument sometimes lands on the next 1-2 lines (a
        # multi-line call, e.g. `streamFetch(\n  \`/api/...\`,\n  {...})`).
        for j in range(idx + 1, min(idx + 3, len(lines))):
            if _STRING_RE.search(window):
                break
            window += " " + lines[j]
        sm = _STRING_RE.search(window)
        if not sm:
            continue
        raw_path = sm.group(1)
        if not raw_path.startswith("/"):
            continue  # skip external URLs / non-API-shaped literals
        calls.append(
            FrontendCall(
                path=raw_path,
                normalized_path=normalize_path(raw_path),
                file=rel,
                line=idx + 1,
                enclosing=_enclosing_symbol(lines, idx),
            )
        )
    return calls


def extract_frontend_calls(repo_root: Path) -> list[FrontendCall]:
    base = repo_root / "frontend" / "src"
    if not base.is_dir():
        return []
    calls: list[FrontendCall] = []
    for suffix in ("*.ts", "*.tsx"):
        for f in base.rglob(suffix):
            if "node_modules" in f.parts:
                continue
            calls.extend(_extract_calls_from_file(f, repo_root))
    return calls


# ---------------------------------------------------------------------------
# vite.config.js dev-proxy rule parsing — decides which backend (be-v2 vs
# backend/) actually serves a given path today.
# ---------------------------------------------------------------------------


@dataclass(frozen=True)
class ProxyRule:
    prefix: str
    target: Optional[str]  # "be-v2" | "backend" | None (router-gated, see path_regex)
    path_regex: Optional[re.Pattern] = None

    def resolve(self, path: str) -> Optional[str]:
        if not path.startswith(self.prefix):
            return None
        if self.path_regex is not None:
            # `path` carries our own `*` wildcard placeholder for unresolved
            # params (see normalize_path), but the vite regex itself
            # expects a real path segment (e.g. `[\w-]+`, `[^/]+`) — `*`
            # matches neither, so substitute a benign word-char stand-in
            # just for this membership test (the placeholder never affects
            # `normalized_path` shown in results, only this regex probe).
            probe = path.replace("*", "x")
            return "be-v2" if self.path_regex.search(probe) else "backend"
        return self.target


def _extract_js_regex_literal(text: str, slash_index: int) -> tuple[str, int]:
    """``text[slash_index] == '/'`` (opening delimiter of a JS regex
    literal). Returns ``(body, index_after_closing_slash)``, respecting
    backslash-escapes so an escaped ``\\/`` inside the body doesn't
    terminate the scan early — and, critically, respecting character-class
    brackets: JS regex doesn't require escaping ``/`` inside ``[...]``
    (this repo's rule set uses exactly that, e.g. ``[^/]+``), so an
    unescaped ``/`` there must NOT be treated as the closing delimiter."""
    assert text[slash_index] == "/"
    i = slash_index + 1
    body_chars: list[str] = []
    in_class = False
    while i < len(text):
        c = text[i]
        if c == "\\" and i + 1 < len(text):
            body_chars.append(c)
            body_chars.append(text[i + 1])
            i += 2
            continue
        if c == "[":
            in_class = True
        elif c == "]":
            in_class = False
        elif c == "/" and not in_class:
            return "".join(body_chars), i + 1
        body_chars.append(c)
        i += 1
    raise ValueError("unterminated regex literal in vite.config.js")


def parse_vite_proxy_rules(vite_config_text: str) -> list[ProxyRule]:
    """Parse `server.proxy`'s ordered rule list from vite.config.js's source.

    Handles the two shapes actually used in this repo: a plain
    ``{ target: bev2Host | backendHost, ... }`` entry, and the one
    ``router: (req) => (req.url && /<regex>/.test(req.url) ? bev2Host :
    backendHost)`` entry (currently ``/api/projects``) — extracted as a
    path-regex-gated rule rather than a constant target.
    """
    marker = "proxy: {"
    idx = vite_config_text.find(marker)
    if idx == -1:
        return []
    body_start = idx + len(marker)
    depth = 1
    i = body_start
    while depth > 0 and i < len(vite_config_text):
        if vite_config_text[i] == "{":
            depth += 1
        elif vite_config_text[i] == "}":
            depth -= 1
        i += 1
    body = vite_config_text[body_start : i - 1]

    entry_re = re.compile(r"""['"](?P<key>/[^'"]*)['"]\s*:\s*\{""")
    rules: list[ProxyRule] = []
    pos = 0
    while True:
        m = entry_re.search(body, pos)
        if not m:
            break
        key = m.group("key")
        brace_start = m.end() - 1
        depth = 1
        j = brace_start + 1
        while depth > 0 and j < len(body):
            if body[j] == "{":
                depth += 1
            elif body[j] == "}":
                depth -= 1
            j += 1
        entry_body = body[brace_start + 1 : j - 1]
        pos = j

        path_regex: Optional[re.Pattern] = None
        target: Optional[str] = None
        router_marker = "req.url && /"
        r_idx = entry_body.find(router_marker)
        if r_idx != -1:
            slash_idx = r_idx + len("req.url && ")
            regex_body, _end = _extract_js_regex_literal(entry_body, slash_idx)
            py_pattern = regex_body.replace("\\/", "/")
            try:
                path_regex = re.compile(py_pattern)
            except re.error:
                path_regex = None
        elif "bev2Host" in entry_body:
            target = "be-v2"
        elif "backendHost" in entry_body:
            target = "backend"

        rules.append(ProxyRule(prefix=key, target=target, path_regex=path_regex))
    return rules


def resolve_service_for_path(path: str, rules: list[ProxyRule]) -> Optional[str]:
    """Which service (``"be-v2"``/``"backend"``) vite's dev proxy sends
    ``path`` to today, per the first matching rule (insertion order = match
    order, mirroring vite's own prefix-match-in-order behavior). ``None``
    means no rule matched (e.g. a non-``/api`` cie route, or truly
    unproxied)."""
    for rule in rules:
        resolved = rule.resolve(path)
        if resolved is not None:
            return resolved
    return None


# ---------------------------------------------------------------------------
# Repo-root discovery + per-process cache
# ---------------------------------------------------------------------------

_INDEX_CACHE: dict[Path, tuple[list[BackendRoute], list[FrontendCall], list[ProxyRule]]] = {}


def find_repo_root(start: Path) -> Path:
    """Walk up from ``start`` to the checkout root containing
    ``be-v2``/``backend``/``frontend`` as siblings (this repo's fixed
    layout) — ``start`` is usually already ``be-v2`` (cie's cwd/root) or
    the checkout root itself."""
    candidates = [start] + list(start.parents)
    for candidate in candidates:
        if (candidate / "frontend").is_dir() and (
            (candidate / "be-v2").is_dir() or (candidate / "backend").is_dir()
        ):
            return candidate
    if start.name == "be-v2" and (start.parent / "frontend").is_dir():
        return start.parent
    return start


def _load_index(repo_root: Path) -> tuple[list[BackendRoute], list[FrontendCall], list[ProxyRule]]:
    if repo_root not in _INDEX_CACHE:
        routes = extract_backend_routes(repo_root)
        calls = extract_frontend_calls(repo_root)
        vite_path = repo_root / "frontend" / "vite.config.js"
        rules = (
            parse_vite_proxy_rules(vite_path.read_text(encoding="utf-8", errors="replace"))
            if vite_path.is_file()
            else []
        )
        _INDEX_CACHE[repo_root] = (routes, calls, rules)
    return _INDEX_CACHE[repo_root]


def invalidate_cache(repo_root: Optional[Path] = None) -> None:
    """Drop the cached index (all projects, or just ``repo_root``)."""
    if repo_root is None:
        _INDEX_CACHE.clear()
    else:
        _INDEX_CACHE.pop(repo_root, None)


# ---------------------------------------------------------------------------
# Public resolvers
# ---------------------------------------------------------------------------


#: `backend/`'s dedicated reverse-proxy router (see that file's module
#: docstring): every route it declares is a pure httpx forward to be-v2,
#: registered first so it wins the path match in prod even though the
#: *real* implementation lives in be-v2. A resolved match landing here is
#: correct (that's genuinely the first hop) but incomplete on its own —
#: `resolve_api_route` follows it to the be-v2 route with the same
#: normalized path and attaches it as `forwards_to`, so "which backend
#: route does this hit" doesn't stop at a shim with no real logic in it.
_PROXY_SHIM_FILES = frozenset({"backend/src/be_code_gen/api/routes/bev2_proxy.py"})


def resolve_api_route(query_path: str, repo_root: Path) -> list[dict]:
    """Frontend call path -> matching backend route(s).

    ``query_path`` may be a literal path, a template-literal expression
    (``${taskId}``), or a FastAPI-style template (``{task_id}``) — all
    normalize to the same wildcard shape before matching. Results are
    filtered to whichever service (be-v2/backend) `vite.config.js`'s dev
    proxy actually routes this path to today; if that filter yields
    nothing (e.g. a proxy-parsing edge case), falls back to unfiltered
    shape matches so a real route is never hidden by a parsing gap.

    A match landing on `backend/`'s ``bev2_proxy.py`` (a pure forward-to
    -be-v2 shim, see `_PROXY_SHIM_FILES`) gets a ``forwards_to`` key
    pointing at the be-v2 route that actually implements it, so the
    answer doesn't dead-end at a shim with no real logic in it.
    """
    routes, _calls, proxy_rules = _load_index(repo_root)
    normalized_query = normalize_path(query_path)
    service = resolve_service_for_path(normalized_query, proxy_rules)

    shape_matches = [r for r in routes if _paths_match(normalized_query, r.normalized_path)]
    scoped = [r for r in shape_matches if service is None or r.service == service]
    chosen = scoped or shape_matches

    results: list[dict] = []
    for route in chosen:
        entry = route.as_dict()
        if route.file in _PROXY_SHIM_FILES:
            entry["is_forwarding_shim"] = True
            real = next(
                (o for o in shape_matches if o.service == "be-v2" and o is not route),
                None,
            )
            if real is not None:
                entry["forwards_to"] = real.as_dict()
        results.append(entry)
    return results


def api_call_sites(route_query: str, repo_root: Path) -> list[dict]:
    """Backend route (path template or handler function name) -> frontend
    call site(s) that hit it."""
    routes, calls, _proxy_rules = _load_index(repo_root)
    by_handler = [r for r in routes if r.handler == route_query]
    targets = (
        {r.normalized_path for r in by_handler}
        if by_handler
        else {normalize_path(route_query)}
    )
    results = [
        c for c in calls if any(_paths_match(c.normalized_path, t) for t in targets)
    ]
    return [c.as_dict() for c in results]
