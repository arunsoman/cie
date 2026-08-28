# Security Policy

## Reporting a vulnerability

**Do not open a public GitHub issue for security vulnerabilities.**

Please report suspected vulnerabilities privately by email to
**aarunsoman@gmail.com**, with `[cie security]` in the subject line.
Include:

- a description of the issue and its impact,
- the smallest reproduction you can manage (a `.nir`/`.py` snippet, a
  command, or the MCP/HTTP request that triggers it), and
- the `cie` version / commit and your runtime (OS, Python, Neo4j if used).

Acknowledge receipt within a reasonable timeframe (this is a small project,
not a 24/7 operation). Coordinated disclosure is the default; if you have a
preferred disclosure timeline, say so. Please do not publish details until a
fix is available and we've agreed on disclosure.

## Security-relevant surface

If you're auditing cie, these are the load-bearing security properties —
please look here first:

- **File-jail.** The filesystem tools (`view_file` / `write_file` /
  `edit_file` / `delete_file` / `write_files_atomic`) jail every path under
  the project root via `cie.tools.view._jail`, rejecting traversal escapes
  (`..`, absolute paths, symlinks pointing out). The `run` tool's
  subprocess jail is `CIE_RUN_ROOT` (defaults to the project root). There
  is **deliberately no "disable the jail" toggle**; treat any change that
  adds one as security-sensitive and review it carefully.
- **MCP policy enforcement.** `cie-mcp --policy <name>` filters tools via
  `cie.tool_policy`: a denied tool is **never registered** on the MCP
  server, so a client's `tools/list` response never even names it — not
  merely refused at call time. `full` (read+write) is the default; pass
  `readonly` (alias `inspector`) for a less-trusted client. This is a
  load-bearing property: keep it, don't regress to "register all, refuse
  at call."
- **Subprocess `run`.** Arbitrary command execution under a cwd jail with
  a hard timeout. Anyone with `allow_write=True` (the `full` policy) can
  run commands on the host. Only hand a write-enabled policy to a client
  you trust with shell access to that checkout.
- **Provenance.** Every node/edge carries `extracted_at` /
  `extractor_version` / `source_ref`; confidence is stamped at write time
  by the repository, never invented by the pure extractor. GraphRAG
  citations are assembled from the graph, never emitted by the LLM, so
  they can't be fabricated mid-generation.

## Scope

In scope: anything shipped in this repository under `cie/`, the MCP server,
the HTTP routes, the CLI, and the file/subprocess tools.

Out of scope: your Neo4j instance's own auth/hardening (your
responsibility), and any third-party tree-sitter grammar wheels cie loads.