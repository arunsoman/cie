<!-- Thanks for the PR! A short, honest description is more useful than a long one. -->

## What & why

<!-- One or two sentences: what does this change do, and why? Reference the issue if there is one (`Closes #123`). -->

## What changed

<!-- Bullet list of the user-visible / API-visible changes. Note any breaking changes explicitly. -->

## Does it respect the design boundary?

cie's essence is **understand + answer** (turn source into a queryable
graph and answer questions about it) — it is **not** a build system,
compiler, or test runner. See
[`docs/language-agnostic-design.md`](../docs/language-agnostic-design.md).

- [ ] This keeps cie a thin intelligence layer over the project's own toolchain (it does not ask cie to *implement* compile/build).
- [ ] This does **not** re-introduce `forge` / `be-v2` / `protobox` coupling (node identity stays `urn:cie:`, workspace stays cie-owned, no host-project soft imports).

## Tool-surface invariants

If this adds or changes a `ToolService` method, `cie.routes.TOOLS` entry,
`WRITE_TOOLS` entry, or an HTTP-only `_tool_*` helper:

- [ ] `tests/test_tool_surface_invariants.py` still passes (`pytest tests/`)
- [ ] New mutating method added to `WRITE_TOOLS` in `cie/tool_policy.py`?
- [ ] New HTTP-only helper added to `HTTP_ONLY_HELPERS` in the invariant test (or — preferred — made a real `ToolService` method)?

## Tests

- [ ] `pytest tests/` passes locally
- [ ] New behavior covered by a test (or an explicit note on why it isn't testable here)

## Security-relevant?

If this touches the file jail (`cie.tools.view._jail`), the `run` subprocess
jail, MCP policy enforcement, or adds any "disable the jail" / "register-all-
then-refuse" path: call it out here and request a review.

- [ ] Not security-relevant
- [ ] Security-relevant — flagged below