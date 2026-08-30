"""Sandboxed command execution (the ``run`` tool's oracle).

Runs a shell command inside a jailed working directory with a hard timeout
and bounded, head+tail-truncated output, so an agent loop can use pytest /
linters / git as an oracle without ever dumping unbounded text into a prompt.

Security note (integration spec T1.4 / §5): this executes LLM-generated code.
The v0 isolation level is ``subprocess`` only: a cwd jail plus a hard timeout
that kills the whole process group. It is NOT a security boundary against
hostile code — run the cie process itself inside a container with no
network for real isolation. ``shell=True`` is deliberate (the tool exists to
run arbitrary shell command strings); callers must treat ``cmd`` as trusted
input from the orchestrating agent, never from untrusted users.
"""

from __future__ import annotations

import os
import signal
import subprocess
from dataclasses import dataclass
from pathlib import Path

#: Explicit marker emitted when a successful command printed nothing; models
#: misread silence as a hung or broken tool (integration spec T1.4).
NO_OUTPUT_MESSAGE = "[command ran successfully and produced no output]"

#: Default cap on captured output bytes (head + tail + truncation marker).
DEFAULT_MAX_BYTES = 6144


@dataclass(frozen=True)
class RunResult:
    """Outcome of one sandboxed command run.

    Attributes:
        exit_code: Process exit code (negative when killed by a signal,
            e.g. ``-signal.SIGKILL`` after a timeout).
        output: Merged stdout+stderr, possibly head+tail truncated, or
            :data:`NO_OUTPUT_MESSAGE` when the command succeeded silently.
        timed_out: True when the hard timeout fired and the process group
            was killed.
        output_truncated: True when captured output exceeded ``max_bytes``
            and was cut to head + marker + tail.
    """

    exit_code: int
    output: str
    timed_out: bool
    output_truncated: bool


def _truncate_head_tail(text: str, max_bytes: int) -> tuple[str, bool]:
    """Cut ``text`` to at most ``max_bytes`` total, keeping head and tail.

    The middle is replaced by the exact marker ``[... truncated N bytes ...]``
    where N is the number of omitted bytes. The returned string (head +
    marker + tail, measured in UTF-8 bytes before lossy decoding) never
    exceeds ``max_bytes``.
    """
    data = text.encode("utf-8", errors="replace")
    if len(data) <= max_bytes:
        return text, False

    omitted = 0
    marker = b""
    head_len = tail_len = 0
    # The marker embeds the omitted byte count, whose digit count affects how
    # much budget remains for head/tail; iterate to a fixed point (converges
    # in at most a couple of rounds).
    for _ in range(4):
        budget = max_bytes - len(marker)
        if budget < 0:
            # Degenerate tiny max_bytes: no room even for the marker.
            return marker.decode()[:max_bytes], True
        head_len = budget // 2
        tail_len = budget - head_len
        omitted = len(data) - head_len - tail_len
        new_marker = f"[... truncated {omitted} bytes ...]".encode()
        if new_marker == marker:
            break
        marker = new_marker

    head = data[:head_len].decode("utf-8", errors="replace")
    tail = data[len(data) - tail_len:].decode("utf-8", errors="replace") if tail_len else ""
    return f"{head}{marker.decode()}{tail}", True


def run_command(
    cmd: str,
    cwd: Path,
    timeout: int = 300,
    max_bytes: int = DEFAULT_MAX_BYTES,
    allowed_root: Path | None = None,
) -> RunResult:
    """Run ``cmd`` in ``cwd`` with a hard timeout and bounded output.

    Args:
        cmd: Shell command string, executed with ``shell=True`` (deliberate;
            see module docstring).
        cwd: Working directory for the command. Resolved and required to stay
            under ``allowed_root``.
        timeout: Hard wall-clock limit in seconds. On expiry the whole
            process group is killed (``start_new_session=True`` makes the
            child a group leader, so ``os.killpg`` reaps grandchildren too).
        max_bytes: Cap on captured output; excess is head+tail truncated with
            the marker ``[... truncated N bytes ...]``.
        allowed_root: Jail root for ``cwd``. Defaults to ``cwd`` itself,
            which makes the escape check a no-op; callers (ToolService) pass
            the project root.

    Returns:
        A :class:`RunResult` with merged stdout/stderr. A command that exits
        0 with no output yields ``output == NO_OUTPUT_MESSAGE``.

    Raises:
        ValueError: If ``cwd`` does not exist or is not a directory, or
            resolves outside ``allowed_root`` (cwd jail).
    """
    jail_root = Path(allowed_root) if allowed_root is not None else Path(cwd)
    jail_root = jail_root.resolve()
    resolved_cwd = Path(cwd).resolve()
    if not resolved_cwd.is_dir():
        raise ValueError(f"cwd does not exist or is not a directory: {cwd}")
    if resolved_cwd != jail_root and jail_root not in resolved_cwd.parents:
        raise ValueError(
            f"cwd {resolved_cwd} escapes allowed root {jail_root}; "
            "run commands must stay inside the project directory"
        )

    # R16 — optional container seam: CIE_RUN_WRAPPER prefixes the command
    # with a caller-supplied wrapper (e.g. `docker run --rm -v {root}:{root}
    # -w {root}`) — CONVENIENCE, NOT ENFORCEMENT: the same cwd jail and
    # timeout apply, but cie does not verify what the wrapper does (the
    # boundary statement lives in docs/security.md). A wrapper may use the
    # {root} placeholder (the resolved jail root). Absent env, behavior is
    # unchanged.
    wrapper = os.environ.get("CIE_RUN_WRAPPER", "").strip()
    if wrapper:
        cmd = f"{wrapper.format(root=resolved_cwd)} {cmd}" if "{root}" in wrapper else f"{wrapper} {cmd}"

    proc = subprocess.Popen(
        cmd,
        shell=True,
        cwd=str(resolved_cwd),
        stdout=subprocess.PIPE,
        stderr=subprocess.STDOUT,
        start_new_session=True,
        text=True,
        errors="replace",
    )
    timed_out = False
    try:
        output, _ = proc.communicate(timeout=timeout)
    except subprocess.TimeoutExpired:
        timed_out = True
        try:
            os.killpg(proc.pid, signal.SIGKILL)
        except (ProcessLookupError, PermissionError):
            proc.kill()
        output, _ = proc.communicate()

    output = output or ""
    output, truncated = _truncate_head_tail(output, max_bytes)
    if not timed_out and proc.returncode == 0 and not output:
        output = NO_OUTPUT_MESSAGE

    return RunResult(
        exit_code=proc.returncode if proc.returncode is not None else -1,
        output=output,
        timed_out=timed_out,
        output_truncated=truncated,
    )
