#!/usr/bin/env python3
"""cie demo — post-production: cards, lower-thirds, scheduled re-renders
of the uncut casts, GIF assembly. The casts are the source of truth;
every GIF frame is a REAL screen state at a scheduled moment (content
never edited - time is the only thing compressed)."""
from __future__ import annotations

import bisect
import json
import subprocess
from pathlib import Path

import pyte
from PIL import Image, ImageDraw, ImageFont

# --- house style -----------------------------------------------------------
W, H = 1000, 585
BG = (15, 20, 32)       # #0f1420
PANEL = (23, 30, 46)    # #171e2e
FG = (219, 226, 240)    # #dbe2f0
DIM = (147, 161, 187)   # #93a1bb
ACCENT = (103, 176, 255)  # #67b0ff
OK = (67, 199, 143)     # #43c78f
F_MONO = "/usr/share/fonts/Adwaita/AdwaitaMono-Regular.ttf"
F_MONO_B = "/usr/share/fonts/Adwaita/AdwaitaMono-Bold.ttf"
F_SANS = "/usr/share/fonts/Adwaita/AdwaitaSans-Regular.ttf"
F_SANS_I = "/usr/share/fonts/Adwaita/AdwaitaSans-Italic.ttf"

import sys
sys.path.insert(0, "/tmp/prod")
from recorder import Recorder, COLS, ROWS  # render machinery

# --- cast player: real screen states at scheduled times --------------------
class CastPlayer:
    def __init__(self, cast_path: str):
        self.events = []
        for line in open(cast_path):
            ev = json.loads(line)
            if len(ev) == 3:
                self.events.append((ev[1], ev[2]))
        self.ts = [t for t, _ in self.events]

    def render_at(self, t: float) -> Image.Image:
        rec = Recorder.__new__(Recorder)   # reuse render(), no session
        rec.screen = pyte.Screen(COLS, ROWS)
        rec.stream = pyte.ByteStream(rec.screen)
        rec.font = ImageFont.truetype(Recorder and F_MONO, 15)
        rec.fontb = ImageFont.truetype(F_MONO_B, 15)
        bb = rec.font.getbbox("M")
        rec.cw = bb[2] - bb[0] or 9
        rec.W = rec.cw * COLS + 20 * 2
        rec.H = 17 * ROWS + 20 * 2
        n = bisect.bisect_right(self.ts, t)
        for _, data in self.events[:n]:
            rec.stream.feed(data.encode())
        img = Image.new("RGB", (rec.W, rec.H), BG)
        d = ImageDraw.Draw(img)
        # inline copy of Recorder.render body (it renders self.screen)
        for y in range(rec.screen.lines):
            line = rec.screen.buffer[y]
            for x in range(rec.screen.columns):
                ch = line[x]
                if not ch.data or ch.data == " " or ord(ch.data[0]) < 32:
                    continue
                from recorder import _col
                if ch.reverse or ch.bg not in (None, "default"):
                    d.rectangle(
                        [20 + x * rec.cw, 20 + y * 17 - 2,
                         20 + (x + 1) * rec.cw, 20 + (y + 1) * 17 - 1],
                        fill=_col(ch.bg, FG if ch.reverse else BG))
                fg = _col(ch.fg, FG)
                if ch.reverse:
                    fg = BG if ch.bg in (None, "default") else _col(ch.bg, FG)
                f = rec.fontb if ch.bold else rec.font
                d.text((20 + x * rec.cw, 20 + y * 17 - 3), ch.data, font=f, fill=fg)
        return img


def fit(img: Image.Image) -> Image.Image:
    """Cover-fit any frame onto the 1000x585 canvas."""
    s = max(W / img.width, H / img.height)
    img = img.resize((round(img.width * s), round(img.height * s)), Image.LANCZOS)
    x = (img.width - W) // 2
    y = (img.height - H) // 2
    return img.crop((x, y, x + W, y + H))


def lower_third(img: Image.Image, label: str, fact: str) -> Image.Image:
    d = ImageDraw.Draw(img, "RGBA")
    bar_h = 46
    y0 = H - bar_h
    d.rectangle([0, y0, W, H], fill=PANEL + (235,))
    d.rectangle([0, y0, 6, H], fill=ACCENT)
    f1 = ImageFont.truetype(F_MONO_B, 15)
    f2 = ImageFont.truetype(F_SANS, 14)
    d.text((22, y0 + 8), label.upper(), font=f1, fill=ACCENT)
    d.text((22 + f1.getbbox(label.upper())[2] + 18, y0 + 10), fact, font=f2, fill=FG)
    return img


def title_card() -> Image.Image:
    img = Image.new("RGB", (W, H), BG)
    d = ImageDraw.Draw(img)
    d.rectangle([0, 0, W, 6], fill=ACCENT)
    word = ImageFont.truetype(F_MONO_B, 96)
    tag = ImageFont.truetype(F_SANS, 21)
    sub = ImageFont.truetype(F_SANS_I, 16)
    d.text((70, 120), "cie", font=word, fill=FG)
    d.rectangle([78, 232, 128, 236], fill=ACCENT)
    d.text((70, 260), "the only code graph that knows", font=tag, fill=FG)
    d.text((70, 290), "which tasks and tests actually implement your code", font=tag, fill=FG)
    d.text((70, 372), "30-second demo - dogfooding: cie, asked about itself", font=sub, fill=DIM)
    d.text((70, 396), "kannamma-labs/cie @ v0.1.4 - real session, uncut cast linked", font=sub, fill=DIM)
    return img


def end_card() -> Image.Image:
    img = Image.new("RGB", (W, H), BG)
    d = ImageDraw.Draw(img)
    d.rectangle([0, 0, W, 6], fill=ACCENT)
    f1 = ImageFont.truetype(F_MONO_B, 26)
    f2 = ImageFont.truetype(F_SANS, 18)
    f3 = ImageFont.truetype(F_SANS_I, 15)
    lines = [
        ("135 read tools - zero-config SQLite or Neo4j", OK),
        ("read-only by default - works offline", OK),
        ("1,902 nodes / 6,581 edges / 562 TESTS edges - indexed in 1.9s", FG),
        ("github.com/kannamma-labs/cie", ACCENT),
    ]
    y = 150
    for text, col in lines:
        d.text((70, y), text, font=f2, fill=col)
        y += 44
    d.text((70, y + 26), "pip install \"cie-mcp[mcp]\" - then: cie init .", font=f1, fill=FG)
    d.text((70, y + 78), "GIF edited for time only - the uncut session is linked in the README",
           font=f3, fill=DIM)
    return img


# --- the 30-second cut (@12fps = 360 frames) -------------------------------
TAKES = Path("/tmp/prod/takes")
SEQ = Path("/tmp/prod/seq")
SEQ.mkdir(exist_ok=True)
for f in SEQ.glob("*.png"):
    f.unlink()

fps = 12
frame_plan: list[Image.Image] = []

s1s2 = CastPlayer(str(TAKES / "S1S2v4" / "session.cast"))
s3s4 = CastPlayer(str(TAKES / "S3S4" / "session.cast"))
s5t = CastPlayer(str(TAKES / "S5term" / "session.cast"))


def hold(img, seconds):
    for _ in range(round(seconds * fps)):
        frame_plan.append(img.copy())


def sweep(cast, t0, t1, seconds, post=None):
    """Time-compress a real cast interval [t0,t1] into `seconds`."""
    n = max(1, round(seconds * fps))
    for i in range(n):
        t = t0 + (t1 - t0) * i / (n - 1)
        img = fit(cast.render_at(t))
        if post:
            img = post(img)
        frame_plan.append(img)


# S0 title (2.0s)
hold(title_card(), 2.0)
# S1 (3.6s): clone ffwd 0.9s -> checkout 0.5 -> index typing+run+output 2.2
sweep(s1s2, 2.0, 9.5, 0.9, lambda i: lower_third(i, "Scene 01 - index", "git clone github.com/kannamma-labs/cie (real, public tag v0.1.4)"))
sweep(s1s2, 12.0, 13.6, 0.5, lambda i: lower_third(i, "Scene 01 - index", "checkout v0.1.4 - the release that fixed TESTS edges today"))
sweep(s1s2, 14.5, 19.3, 2.2, lambda i: lower_third(i, "Scene 01 - index", "cie index . - 1,902 nodes, 6,581 edges, 4,169 calls - 1.9s real time"))
# S2 (3.0s): mcp add 1.3 + list/Connected 1.7
sweep(s1s2, 21.0, 26.5, 1.3, lambda i: lower_third(i, "Scene 02 - register", "one command - the README one-liner, read-only policy"))
sweep(s1s2, 36.8, 44.0, 1.7, lambda i: lower_third(i, "Scene 02 - register", "claude mcp list: cie ... Connected"))
# S3 (2.6s): the question types
sweep(s3s4, 7.0, 11.6, 2.6, lambda i: lower_third(i, "Scene 03 - the question", "asked once, in plain words"))
# S4 (12.4s): tools + answer, chronological, end on the completed screen
sweep(s3s4, 12.0, 30.0, 2.6, lambda i: lower_third(i, "Scene 04 - impact + tests", "cie's tools only - no file reads, no grep"))
sweep(s3s4, 30.0, 60.0, 4.2, lambda i: lower_third(i, "Scene 04 - impact + tests", "callers - affected_by - test_map"))
sweep(s3s4, 60.0, 92.0, 4.4, lambda i: lower_third(i, "Scene 04 - impact + tests", "7 pinning tests, line numbers, today's bugfix cited"))
img_done = lower_third(fit(s3s4.render_at(96.0)), "Scene 04 - impact + tests", "the regression net, ranked")
hold(img_done, 1.2)
# S5 (4.0s): export flash + browser
sweep(s5t, 1.0, 3.4, 1.0, lambda i: lower_third(i, "Scene 05 - snapshot", "one HTML file - no server, no signup"))
for i, name in enumerate(["0_top", "1_mid", "2_chains", "3_orphans"]):
    b = fit(Image.open(TAKES / "S5" / f"browser_{name}.png").convert("RGB"))
    b = lower_third(b, "Scene 05 - snapshot", ["overview", "task & test chains", "task & test chains", "orphans: no test, no contract"][i])
    hold(b, 0.75)
# S6 end card (2.0s)
hold(end_card(), 2.0)

for i, img in enumerate(frame_plan):
    img.save(SEQ / f"f{i:05d}.png")
print("frames:", len(frame_plan), "duration:", round(len(frame_plan) / fps, 2), "s")

# --- GIF assembly ----------------------------------------------------------
out = Path("/tmp/prod/cie-demo-30s.gif")
subprocess.run([
    "ffmpeg", "-y", "-framerate", str(fps), "-i", str(SEQ / "f%05d.png"),
    "-vf", "split[s0][s1];[s0]palettegen=max_colors=192[p];[s1][p]paletteuse=dither=bayer:bayer_scale=3",
    str(out),
], check=True, capture_output=True)
mb = out.stat().st_size / 1e6
print(f"GIF: {out} - {mb:.1f} MB")