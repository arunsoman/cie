#!/usr/bin/env python3
"""cie demo — PTY session recorder (production-house take logger).

Records a REAL terminal session through a pty:
  - asciinema v2 cast (uncut, timestamped, playable/uploadable)
  - PNG frames rendered via pyte (truecolor-aware: 256/RGB colors kept)
  - a take log (JSON): duration, frames, watcher interventions

Nothing here fabricates content: frames are real screen states, in
chronological order; the cast is the uncut proof.
"""
from __future__ import annotations

import fcntl
import json
import os
import pty
import re
import select
import signal
import struct
import termios
import time
from pathlib import Path

import pyte
from PIL import Image, ImageDraw, ImageFont

# --- house style -----------------------------------------------------------
COLS, ROWS = 100, 30
FS, LH, PAD = 15, 17, 20
FONT_R = "/usr/share/fonts/Adwaita/AdwaitaMono-Regular.ttf"
FONT_B = "/usr/share/fonts/Adwaita/AdwaitaMono-Bold.ttf"
BG = (15, 20, 32)      # #0f1420 --bg
FG = (219, 226, 240)   # #dbe2f0 --fg
NAMED = {
    "black": (26, 32, 48), "red": (224, 108, 117), "green": (67, 199, 143),
    "brown": (224, 180, 90), "yellow": (224, 180, 90), "blue": (103, 176, 255),
    "magenta": (198, 160, 246), "cyan": (94, 197, 235), "white": (219, 226, 240),
    "gray": (147, 161, 187), "grey": (147, 161, 187),
}


def _col(v, default):
    if v in (None, "default"):
        return default
    if isinstance(v, str) and v.startswith("#") and len(v) == 7:
        try:
            return tuple(int(v[i:i + 2], 16) for i in (1, 3, 5))
        except ValueError:
            return default
    return NAMED.get(v, default)


class Recorder:
    def __init__(self, out: Path, cmd: list[str], cols=COLS, rows=ROWS,
                 cwd: str | None = None):
        self.cwd = cwd
        self.out = out
        out.mkdir(parents=True, exist_ok=True)
        self.frames_dir = out / "frames"
        self.frames_dir.mkdir(exist_ok=True)
        for f in self.frames_dir.glob("*.png"):
            f.unlink()
        self.cmd = cmd
        self.screen = pyte.Screen(cols, rows)
        self.stream = pyte.ByteStream(self.screen)
        self.cast = open(out / "session.cast", "w", encoding="utf-8")
        self.cast.write(json.dumps({
            "version": 2, "width": cols, "height": rows,
            "timestamp": int(time.time()), "env": {"SHELL": "/bin/bash"},
        }) + "\n")
        self.font = ImageFont.truetype(FONT_R, FS)
        self.fontb = ImageFont.truetype(FONT_B, FS)
        bb = self.font.getbbox("M")
        self.cw = bb[2] - bb[0] or 9
        self.W = self.cw * cols + PAD * 2
        self.H = LH * rows + PAD * 2
        self.t0 = None
        self.nframes = 0
        self.last_hash = None
        self.log: list[dict] = []

    # -- render -------------------------------------------------------------
    def render(self, path: Path):
        img = Image.new("RGB", (self.W, self.H), BG)
        d = ImageDraw.Draw(img)
        for y in range(self.screen.lines):
            line = self.screen.buffer[y]
            for x in range(self.screen.columns):
                ch = line[x]
                if not ch.data or ch.data == " " or ord(ch.data[0]) < 32:
                    continue
                if ch.reverse or ch.bg not in (None, "default"):
                    d.rectangle(
                        [PAD + x * self.cw, PAD + y * LH - 2,
                         PAD + (x + 1) * self.cw, PAD + (y + 1) * LH - 1],
                        fill=_col(ch.bg, FG if ch.reverse else BG))
                fg = _col(ch.fg, FG)
                if ch.reverse:
                    fg = BG if ch.bg in (None, "default") else _col(ch.bg, FG)
                f = self.fontb if ch.bold else self.font
                d.text((PAD + x * self.cw, PAD + y * LH - 3), ch.data, font=f, fill=fg)
        img.save(path)

    def snapshot(self, force=False):
        rowsig = []
        for y in range(self.screen.lines):
            for c in self.screen.buffer[y].values():
                rowsig.append((c.data, str(c.fg), str(c.bg), c.bold, c.reverse))
        h = hash(tuple(rowsig))
        if not force and h == self.last_hash:
            return
        self.last_hash = h
        self.render(self.frames_dir / f"f{self.nframes:05d}.png")
        self.nframes += 1

    # -- session ------------------------------------------------------------
    def _at_prompt(self, prompt_regex: str) -> bool:
        """Last non-blank screen line shows a live prompt — typing now
        would otherwise feed a still-running child's stdin (v3 bug: the
        'c' of 'claude mcp list' was eaten by the still-running
        `claude mcp add` process)."""
        for line in reversed(self.screen.display):
            if line.strip():
                return bool(re.search(prompt_regex, line))
        return False

    def run(self, script: list[dict], idle_done: float = 6.0,
            max_wait: float = 300.0, watchers=None,
            prompt_regex: str = r"❯|\$\s*$"):
        """script steps, executed at their 'at' offsets (seconds from start):
          {'at': 1.0, 'type': 'cie index .', 'delay': 0.03}   # typed, then ⏎
          {'at': 5.0, 'send': 'y\r'}                          # raw keys
        watchers: [(regex, keys, note)] auto-responses to prompts.
        Session ends when all input is done + screen idle `idle_done`s,
        or at `max_wait`.
        """
        watchers = watchers or []
        self._stop = False
        self._last_snap = 0.0
        self._watcher_last: dict[int, float] = {}
        # normalize: sort by 'at', default 0
        steps = sorted(script, key=lambda s: s.get("at", 0.0))
        pid, fd = pty.fork()
        if pid == 0:
            os.environ["TERM"] = "xterm-256color"
            os.environ["PS1"] = (
                "\\[\\e[1;38;2;103;176;255m\\]\\W\\[\\e[0m\\] "
                "\\[\\e[38;2;147;161;187m\\]❯\\[\\e[0m\\] ")
            if self.cwd:
                os.chdir(self.cwd)
            os.execvp(self.cmd[0], self.cmd)
        fcntl.ioctl(fd, termios.TIOCSWINSZ, struct.pack("HHHH", ROWS, COLS, 0, 0))
        self.t0 = time.time()
        pending: bytes = b""
        pending_delay = 0.03
        pending_next = 0.0
        last_change = time.time()

        def _send(data: str):
            os.write(fd, data.encode())

        while True:
            now = time.time()
            el = now - self.t0
            # fire due script steps — SERIALIZED: a new step only starts
            # when no typing is in flight AND the prompt is back (a step
            # may carry force=True to bypass the prompt gate, e.g. for a
            # TUI that never shows a shell prompt)
            while (steps and not pending
                   and el >= steps[0].get("at", 0.0)
                   and (steps[0].get("force") or (
                       self._at_prompt(prompt_regex)
                       and (now - last_change) > 0.45))):
                st = steps.pop(0)
                if "type" in st:
                    pending = st["type"].encode()
                    pending_delay = st.get("delay", 0.03)
                    pending_next = now + 0.6
                    self.log.append({"t": round(el, 2),
                                     "type": st["type"][:70]})
                    break
                elif "send" in st:
                    _send(st["send"])
                    self.log.append({"t": round(el, 2),
                                     "sent": st["send"].replace("\r", "⏎")[:40]})
            # typing
            if pending and now >= pending_next:
                os.write(fd, pending[:1])
                pending = pending[1:]
                pending_next = now + pending_delay
                if not pending:
                    _send("\r")
                    last_change = time.time()
            # read output
            r, _, _ = select.select([fd], [], [], 0.02)
            if r:
                try:
                    data = os.read(fd, 65536)
                except OSError:
                    break
                if not data:
                    break
                self.stream.feed(data)
                self.cast.write(json.dumps(
                    ["o", round(el, 6), data.decode("utf-8", "replace")]) + "\n")
                self.cast.flush()
                last_change = time.time()
                self.snapshot()
                txt = "\n".join(self.screen.display)
                for wi, (rx, keys, note) in enumerate(watchers):
                    if re.search(rx, txt, re.I):
                        if now - self._watcher_last.get(wi, -99.0) < 3.0:
                            break            # cooldown: a still-visible
                        self._watcher_last[wi] = now   # match must not spam
                        if keys is None:          # stop-marker: end the take
                            self._stop = True
                            self.log.append({"t": round(el, 2),
                                             "watcher": note + " [STOP]"})
                            break
                        _send(keys)
                        self.log.append({"t": round(el, 2), "watcher": note})
                        last_change = time.time()
                        time.sleep(0.2)
                        break
            elif now - self._last_snap >= 0.3:   # periodic frame, cheap:
                self._last_snap = now            # hash only on change/output
                self.snapshot()
            # done?
            if self._stop:
                self.log.append({"t": round(el, 2), "note": "stop-marker"})
                break
            if not steps and not pending and (now - last_change) > idle_done:
                break
            if el > max_wait:
                self.log.append({"t": round(el, 2), "note": "max_wait reached"})
                break
        try:
            _send("\x03")
            time.sleep(0.2)
        except OSError:
            pass
        try:
            os.kill(pid, signal.SIGKILL)
        except ProcessLookupError:
            pass
        os.close(fd)
        try:
            os.waitpid(pid, 0)
        except ChildProcessError:
            pass
        self.cast.close()
        meta = {"cmd": self.cmd, "duration_s": round(time.time() - self.t0, 2),
                "frames": self.nframes, "canvas": [self.W, self.H],
                "events": self.log}
        (self.out / "take.json").write_text(json.dumps(meta, indent=1))
        return meta


if __name__ == "__main__":
    print("import me; see takes.py")