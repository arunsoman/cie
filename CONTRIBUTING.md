# Contributing to cie

Thanks for considering a contribution to **cie — Code Insight Engine**.
This is a small project; the bar is "tests pass, the design boundary holds,
and the change is explained," not a heavyweight process.

## Quick dev setup

```bash
git clone https://github.com/arunsoman/cie.git
cd cie
pip install -e ".[mcp,http]"      # core + the optional MCP and HTTP extras
pip install pytest                # dev dependency
pytest tests/                     # run the suite
```

`.[mcp,http]` is required for the **full** test suite:
- `tests/test_mcp_server.py` needs the optional `mcp` SDK (`importorskip`
  guards it, so without `mcp` those tests skip rather than fail).
- `tests/test_tool_surface_invariants.py` imports `cie.routes`, which
  imports FastAPI — that's the `[http]` extra.

## Running the tests

```bash
pytest tests/
```

The pyproject sets `testpaths = ["tests"]`, so bare `pytest` works too.
**Do not run `pytest` from the repo root with default discovery** and expect
it to be clean — `cie/test_orchestration.py` and `cie/test_synthesis.py`
are *feature modules* (not test files) whose names happen to start with
`test_`; they import a host-project module (`core.llm`) that isn't on the
path in a standalone checkout. Scoping to `tests/` (via `testpaths`) avoids
collecting them.

## The one invariant that must always hold

`tests/test_tool_surface_invariants.py` pins the boundary between cie's
three front-ends (MCP / HTTP / CLI) so they can't silently drift. When you
add a new `ToolService` method:

- it automatically shows up over MCP and in `describe()` (introspected, no
  manual list to update), **and**
- you **must** add a matching `POST /tools/{tool}` entry in `cie.routes.TOOLS`
  or the invariant `test_every_tool_service_method_is_exposed_over_http`
  fails.

If you add a new HTTP-only helper (a `_tool_*` function in `cie.routes.py`
that isn't backed by a `ToolService` method), either make it a real
`ToolService` method (preferred — MCP/CLI get it for free) or add its name
to `HTTP_ONLY_HELPERS` in the invariant test. A surprise HTTP-only tool
that the MCP server and CLI can't see is exactly the drift this test
exists to prevent.

If the new method mutates state, add it to `WRITE_TOOLS` in
`cie/tool_policy.py` so read-only policies (`inspector`/`readonly`) deny
it — `test_write_tools_are_all_real_tool_service_methods` guards against
stale/typo'd entries.

## Design boundary — please respect it

cie's essence is **understand + answer**: turn source into a queryable
graph and answer structural/semantic questions about it. It is **not** a
build system, compiler, or test runner. See
[`docs/language-agnostic-design.md`](docs/language-agnostic-design.md)
for the full boundary, the `ProjectDescriptor` design, and the tool
triage. In particular:

- Don't add `compile()`/`build()` methods that cie *implements*. Build/run
  is the project's job; cie invokes the project's own commands through
  the jailed `run` oracle, it doesn't own a toolchain.
- Don't re-introduce `forge` / `be-v2` / `protobox` coupling. cie was
  carved out of the protobox `be-v2` backend and deliberately decoupled;
  node identity is `urn:cie:`, the workspace is cie-owned, and the policy
  names are `full`/`readonly` (the old `forge`/`inspector`/etc. names are
  deprecated back-compat aliases). New code should be consumer-agnostic.

## Security-relevant changes

The file tools jail unconditionally under the project root
(`cie.tools.view._jail`); `CIE_RUN_ROOT` can *widen* the `run` jail only.
There is deliberately **no "disable the jail" toggle** — don't add one
without a security review and an explicit tracking issue. The MCP
server's policy enforcement (denied tools are never *registered*, not
merely refused) is a load-bearing security property; keep it.

## Commit messages

Conventional, descriptive. A short subject line + a body explaining *why*.
Reference the issue/PR where relevant. Example shape is in `git log`.

## Reporting issues

Open a GitHub issue. For security vulnerabilities, see
[`SECURITY.md`](SECURITY.md) — **do not** open a public issue for those.

## License

By contributing, you agree your contributions are licensed under the
project's [MIT license](LICENSE).