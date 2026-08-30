"""`cie init` — one-command onboarding (roadmap R15, delta S2): detect
installed MCP clients, register cie's stdio server, write the context
files an agent needs (GitNexus's AGENTS.md/CLAUDE.md pattern).

Design decisions, stated:

- **Server entry format per client, JSON-merge for the auto ones.**
  Claude Code reads a project-local `.mcp.json`
  (`{"mcpServers": {<name>: {command, args}}}`) and Cursor reads
  `~/.cursor/mcp.json` with the same shape — both are JSON, so
  registration is a guarded merge (existing keys untouched, cie's entry
  only added when missing). Codex (`~/.codex/config.toml`) is DETECTED
  but never auto-edited (no TOML writer dependency; printing the exact
  snippet to add is the honest v1, no silent partial edits).
- **Readonly until chosen.** The generated entry carries
  `--policy readonly` by default; the operator opts the client into
  writes with `--policy full`. Default-safe avoids granting write
  privileges to a client the user didn't consciously grant them — the
  policy-differentiator story (d4) in product form.
- **Managed context blocks.** AGENTS.md / CLAUDE.md get a marked
  `<!-- cie:init:begin -->...<!-- cie:init:end -->` block via
  append-or-replace: user content outside the markers is never touched,
  re-running updates only the block.
- **Idempotent.** Every step checks-and-skips; the command reports what
  it did, what it skipped, and what needs a human (Codex snippet).
"""

from __future__ import annotations

import json
from dataclasses import dataclass, field
from pathlib import Path

_CIE_SERVER_NAME = "cie"

#: The generated context block markers — managed region, per the
#: append-or-replace contract above.
BEGIN = "<!-- cie:init:begin -->"
END = "<!-- cie:init:end -->"

SERVER_ARGS_TEMPLATE = [
    "cie-mcp",
    "{root}",
    "--embedded",
    "--policy",
    "{policy}",
]


@dataclass
class InitReport:
    actions: list[str] = field(default_factory=list)
    skipped: list[str] = field(default_factory=list)
    human_actions: list[str] = field(default_factory=list)

    def merged(self) -> dict:
        return {
            "actions": self.actions,
            "skipped": self.skipped,
            "human_actions": self.human_actions,
        }


def detect_clients(home: Path, project_root: Path) -> list[str]:
    """Which MCP clients look installed — config-file presence, best
    effort, in check order. Claude Code counts via either the user
    config or a project-local `.mcp.json`."""
    found: list[str] = []
    if (home / ".claude.json").exists() or (project_root / ".mcp.json").exists():
        found.append("claude-code")
    if (home / ".cursor").exists():
        found.append("cursor")
    if (home / ".codex").exists():
        found.append("codex")
    return found


def _server_entry(root: Path, policy: str) -> dict:
    return {
        "command": "cie-mcp",
        "args": [str(root), "--embedded", "--policy", policy],
    }


def _merge_json_servers(path: Path, root: Path, policy: str, report: InitReport) -> None:
    """Guarded merge into a `{"mcpServers": {}}`-shaped JSON file:
    create-if-missing, else add only the missing entry (existing keys and
    other servers' entries are never touched)."""
    data: dict = {}
    if path.exists():
        try:
            data = json.loads(path.read_text(encoding="utf-8"))
        except (ValueError, OSError):
            report.skipped.append(
                f"{path}: not valid JSON — refusing to edit; merge this "
                f"entry by hand: {_server_entry(root, policy)}"
            )
            return
    servers = data.setdefault("mcpServers", {})
    if not isinstance(servers, dict):
        report.skipped.append(
            f"{path}: 'mcpServers' is not an object — refusing to edit"
        )
        return
    if _CIE_SERVER_NAME in servers:
        report.skipped.append(f"{path}: '{_CIE_SERVER_NAME}' already registered")
        return
    servers[_CIE_SERVER_NAME] = _server_entry(root, policy)
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(json.dumps(data, indent=2) + "\n", encoding="utf-8")
    report.actions.append(f"{path}: registered '{_CIE_SERVER_NAME}' stdio server")


def _codex_snippet(root: Path, policy: str) -> str:
    entry = _server_entry(root, policy)
    lines = [
        f"[mcp_servers.{_CIE_SERVER_NAME}]",
        f'command = "{entry["command"]}"',
        "args = [" + ", ".join(json.dumps(a) for a in entry["args"]) + "]",
    ]
    return "\n".join(lines)


def _context_block(root: Path, policy: str) -> str:
    return f"""{BEGIN}
## cie — Code Insight Engine (managed by `cie init`)

cie indexes this project into `.cie/` (graph + task/QA + hierarchy
stores). Call cie's MCP tools (`search_symbol`, `callers`, `callees`,
`file_skeleton`, `traceability_orphans`, ...) instead of grepping:
`callers("name")` answers "who calls this" with receiver-resolved,
confidence-tagged edges; grep cannot.

- Server: `cie-mcp {root} --embedded --policy {policy}` (already
  registered for this project by `cie init`).
- Share a snapshot: `cie export-html . --out snapshot.html` (static
  file, no server).
- Re-index after landing changes: `cie index {root}`.
- Task/QA layer: `tasks:pending` / `push_tasks` / `record_coverage`.
{END}"""


def write_context_file(path: Path, root: Path, policy: str, report: InitReport) -> None:
    """Append-or-replace the managed block (never touches user content
    outside the markers)."""
    block = _context_block(root, policy)
    if path.exists():
        try:
            existing = path.read_text(encoding="utf-8")
        except OSError as exc:
            report.skipped.append(f"{path}: unreadable ({exc})")
            return
        if BEGIN in existing and END in existing:
            pre = existing.split(BEGIN, 1)[0]
            post = existing.split(END, 1)[1]
            path.write_text(pre + block + post, encoding="utf-8")
            report.actions.append(f"{path}: managed block refreshed")
            return
        path.write_text(existing.rstrip() + "\n\n" + block, encoding="utf-8")
        report.actions.append(f"{path}: managed block appended")
        return
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text("# " + path.name + "\n\n" + block + "\n", encoding="utf-8")
    report.actions.append(f"{path}: created with context block")


def run_init(
    project_root: Path,
    home: Path,
    *,
    clients: list[str] | None = None,
    policy: str = "readonly",
    context: bool = True,
) -> InitReport:
    """The `cie init` implementation; returns the report of what
    happened. `clients` overrides detection (empty = detected set)."""
    root = Path(project_root).resolve()
    report = InitReport()
    targets = clients if clients is not None else detect_clients(home, root)

    if "claude-code" in targets:
        # project-local is the shared-with-the-team location; the user
        # file is left alone unless the project one exists... the project
        # `.mcp.json` is what a fresh clone's onboarding wants.
        _merge_json_servers(root / ".mcp.json", root, policy, report)
    if "cursor" in targets:
        _merge_json_servers(home / ".cursor" / "mcp.json", root, policy, report)
    if "codex" in targets:
        report.human_actions.append(
            "codex detected (~/.codex) — add this to ~/.codex/config.toml "
            "(codex never auto-edited):\n" + _codex_snippet(root, policy)
        )

    if context:
        write_context_file(root / "AGENTS.md", root, policy, report)
        write_context_file(root / "CLAUDE.md", root, policy, report)

    if not report.actions and not report.skipped and not report.human_actions:
        report.human_actions.append(
            "no MCP client config found — install one of Claude Code / "
            "Cursor / Codex, or pass --client explicitly; you can still "
            "hand-register: " + _codex_snippet(root, policy)
        )
    return report