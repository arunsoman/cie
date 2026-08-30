# ci security model — what is isolated, what is NOT

Upfront, because vague security claims are worse than stated limits:
**cie's tool surface assumes a trusted local user by default.** The
`run` tool executes shell commands; the write tools touch the
filesystem under the project root; the HTTP/MCP surfaces exist to be
connected to by agents the user chose to connect. This document states
the boundary precisely so users and integrators can layer their own
hardening on top (or choose not to run the parts they don't need).

## The `run` tool: exactly what the jail is

(cite: `cie/tools/runner.py` — the implementation, not a summary of it)

1. **cwd jail.** `run` resolves the working directory and refuses
   anything that resolves outside `allowed_root` (the project root the
   ToolService was constructed with; `CIE_RUN_ROOT` widens it AT THE
   OPERATOR'S EXPLICIT CHOICE — widening it hands the tool that much
   filesystem reach).
2. **Hard timeout + process-group kill.** On expiry the whole process
   group is SIGKILLed (`start_new_session=True` makes the child a group
   leader so `os.killpg` reaps grandchildren).
3. **Bounded output.** Captured output is capped (`DEFAULT_MAX_BYTES`),
   head+tail truncated with an explicit marker.

## What the jail is NOT

Plainly, per the honesty rules this repo runs on:

- **No filesystem sandbox beyond cwd.** A child process runs as the
  user's own process; shell redirections, `..` paths INSIDE command
  strings, and subprocesses of subprocesses are the child's business.
  The jail constrains the working directory cie validates, not what a
  determined command can reach with user-level permissions.
- **No network restriction.** A command can open sockets.
- **No container isolation.** Subprocess isolation only — stated the
  same way in `cie/routes.py` since the first session.
- **No sanitization of `cmd`.** It executes with `shell=True`
  deliberately (an agent writes real shell); the defense is the policy
  layer below, not pretending the string is safe.

## The boundary that matters: ToolPolicy per caller

The run/write surface is not offered to untrusted callers by default:

- **HTTP** (`POST /tools/run`): the default policy is read-only
  (`inspector`) — `run` is refused **server-side** with
  `kind="forbidden"` BEFORE any process starts, verified per policy in
  `tests/test_http_policy.py` (write-refusal matrix incl. an explicit
  `run` case).
- **MCP**: `cie-mcp --policy readonly` never REGISTERS write tools — a
  read-only client's `tools/list` does not even carry `run` (pinned in
  `tests/test_mcp_server.py` and re-verified per transport by
  `tool-test-lab/dogfood_mcp_http.py`).
- **Direct in-process callers** (forge's backends) are trusted by
  construction — the policy layer is for external boundaries.

An MCP tool's `WRITE_TOOLS` membership is the single classification —
server-enforced, matching the "never trust the client" differentiator
(`docs/competitive-landscape.md` #4).

## Optional container seam (R16): CIE_RUN_WRAPPER

`CIE_RUN_WRAPPER` prefixes every `run` command with a wrapper, e.g.:

```bash
CIE_RUN_WRAPPER='docker run --rm -v {root}:{root} -w {root}'
```

`{root}` expands to the resolved jail root. **This is convenience, not
enforcement:** cie validates the same cwd jail and timeout around the
wrapper invocation; it does NOT verify what the wrapper itself does
(image contents, mount scope, network flags). Review the wrapper like
any security-relevant configuration. The boundary is unchanged — the
wrapper is a hardening layer the operator chose, not a cie guarantee.

## Threat model summary

| Caller | run/write reachable | Defense in force |
|---|---|---|
| Local user (CLI, direct) | yes | cwd jail + timeout; user is trusted |
| HTTP (default) | **no** — 403 | server-side ToolPolicy (inspector) |
| HTTP (write opt-in) | yes | same policy + CSRF-origin guard |
| MCP `readonly` | **no** — not even registered | same |
| MCP `full` | yes | operator's explicit choice, loopback default |

## Non-goals

No multi-tenancy. No secrets management. No sandbox-for-untrusted-code
guarantees — run something like [firejail]/[bwrap]/a container around
the whole server if that's the threat you face. This document's purpose
is that nobody has to reverse-engineer the above from the code.