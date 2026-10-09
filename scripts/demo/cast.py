#!/usr/bin/env python3
# Copyright 2026 Zyvor AI Labs · https://zyvor.dev
# SPDX-License-Identifier: Apache-2.0
"""Record a command's real output with its real timing, and render it as a terminal GIF/MP4/WebM.

  cast.py record OUT.cast.jsonl -- CMD [ARGS...]     run CMD, keep each output line with its time
  cast.py render IN.cast.jsonl OUT_BASE --cmd "TEXT" [--title T] [--max-gap 1.4]

`record` stores what the command printed and when. `render` types TEXT at a prompt, replays the
stored output at the stored pace (long gaps are capped by --max-gap, never stretched), and writes
OUT_BASE.gif / .mp4 / .webm. Nothing is invented: a scripted (illustrative) cast is just a
command whose script prints fixed text, and it must say so on screen.

Needs Pillow and ffmpeg. Used by scripts/record-demos.sh.
"""
from __future__ import annotations

import argparse
import json
import re
import shutil
import subprocess
import sys
import tempfile
import time
from pathlib import Path

from PIL import Image, ImageDraw, ImageFont

# Dracula-ish palette.
BG = (40, 42, 54)
BAR = (33, 34, 44)
FG = (248, 248, 242)
DIM = (130, 135, 160)
ANSI = {
    30: (33, 34, 44), 31: (255, 85, 85), 32: (80, 250, 123), 33: (241, 250, 140),
    34: (189, 147, 249), 35: (255, 121, 198), 36: (139, 233, 253), 37: FG,
    90: DIM, 91: (255, 110, 110), 92: (105, 255, 148), 93: (255, 255, 165),
    94: (214, 172, 255), 95: (255, 146, 223), 96: (164, 255, 255), 97: (255, 255, 255),
}
PROMPT = (80, 250, 123)
FONT_REG = ("/System/Library/Fonts/Menlo.ttc", 0)
FONT_BOLD = ("/System/Library/Fonts/Menlo.ttc", 1)
SGR = re.compile(r"\x1b\[([0-9;]*)m")


def record(out: Path, cmd: list[str]) -> int:
    start = time.monotonic()
    proc = subprocess.Popen(cmd, stdout=subprocess.PIPE, stderr=subprocess.STDOUT, text=True, bufsize=1)
    assert proc.stdout is not None
    with out.open("w") as fh:
        for line in proc.stdout:
            fh.write(json.dumps({"t": round(time.monotonic() - start, 3), "s": line}) + "\n")
    return proc.wait()


class Term:
    """A tiny terminal: SGR colours, bold, dim and newlines. Enough for these demos."""

    def __init__(self, cols: int, rows: int) -> None:
        self.cols, self.rows = cols, rows
        self.lines: list[list[tuple[str, tuple, bool, bool]]] = [[]]
        self.fg, self.bold, self.dim = FG, False, False

    def feed(self, text: str) -> None:
        pos = 0
        for m in SGR.finditer(text):
            self._text(text[pos:m.start()])
            self._sgr(m.group(1))
            pos = m.end()
        self._text(text[pos:])

    def _sgr(self, params: str) -> None:
        for p in [int(x) if x else 0 for x in params.split(";")] or [0]:
            if p == 0:
                self.fg, self.bold, self.dim = FG, False, False
            elif p == 1:
                self.bold = True
            elif p == 2:
                self.dim = True
            elif p == 22:
                self.bold = self.dim = False
            elif p == 39:
                self.fg = FG
            elif p in ANSI:
                self.fg = ANSI[p]

    def _text(self, text: str) -> None:
        for ch in text:
            if ch == "\n":
                self.lines.append([])
            elif ch == "\r":
                continue
            else:
                if len(self.lines[-1]) >= self.cols:
                    self.lines.append([])
                self.lines[-1].append((ch, DIM if self.dim else self.fg, self.bold, self.dim))
        del self.lines[: max(0, len(self.lines) - self.rows)]


def render(src: Path, base: Path, cmd_text: str, title: str, max_gap: float, cols: int, rows: int, fps_cap: int) -> None:
    events = [json.loads(line) for line in src.read_text().splitlines() if line.strip()]
    reg = ImageFont.truetype(FONT_REG[0], 20, index=FONT_REG[1])
    bold = ImageFont.truetype(FONT_BOLD[0], 20, index=FONT_BOLD[1])
    cw = int(reg.getlength("M"))
    lh = 28
    pad, bar = 28, 40
    width, height = cols * cw + 2 * pad, rows * lh + 2 * pad + bar

    def draw(term: Term, cursor: bool) -> Image.Image:
        im = Image.new("RGB", (width, height), BG)
        d = ImageDraw.Draw(im)
        d.rectangle([0, 0, width, bar], fill=BAR)
        for i, c in enumerate([(255, 85, 85), (241, 250, 140), (80, 250, 123)]):
            d.ellipse([18 + i * 24, 13, 32 + i * 24, 27], fill=c)
        d.text((width // 2 - len(title) * cw // 2, 9), title, font=reg, fill=DIM)
        for y, line in enumerate(term.lines):
            for x, (ch, color, b, _) in enumerate(line):
                d.text((pad + x * cw, bar + pad + y * lh), ch, font=bold if b else reg, fill=color)
        if cursor:
            y = len(term.lines) - 1
            x = len(term.lines[-1])
            d.rectangle([pad + x * cw, bar + pad + y * lh + 3, pad + x * cw + cw - 2, bar + pad + y * lh + lh - 3], fill=FG)
        return im

    frames: list[tuple[Image.Image, float]] = []
    term = Term(cols, rows)
    term.feed(f"\x1b[32m$\x1b[0m ")
    frames.append((draw(term, True), 0.5))
    for ch in cmd_text:  # typing
        term.feed(ch)
        frames.append((draw(term, True), 0.035))
    frames.append((draw(term, True), 0.5))
    term.feed("\n")
    prev = events[0]["t"] if events else 0.0
    for ev in events:
        gap = min(max(ev["t"] - prev, 0.0), max_gap)
        if gap > 0:
            frames[-1] = (frames[-1][0], frames[-1][1] + gap)
        term.feed(ev["s"])
        frames.append((draw(term, False), 0.04))
        prev = ev["t"]
    frames[-1] = (frames[-1][0], 3.0)  # hold the last frame

    # Merge visually identical neighbours so the files stay small.
    merged: list[list] = []
    for im, dur in frames:
        if merged and merged[-1][0].tobytes() == im.tobytes():
            merged[-1][1] += dur
        else:
            merged.append([im, dur])

    pal = [im.quantize(colors=48, method=Image.Quantize.MEDIANCUT, dither=Image.Dither.NONE) for im, _ in merged]
    pal[0].save(
        base.with_suffix(".gif"), save_all=True, append_images=pal[1:],
        duration=[max(20, int(d * 1000)) for _, d in merged], loop=0, optimize=True, disposal=1,
    )

    with tempfile.TemporaryDirectory() as tmp:
        lines = []
        for i, (im, dur) in enumerate(merged):
            p = Path(tmp) / f"{i:05d}.png"
            im.save(p)
            lines += [f"file '{p}'", f"duration {dur:.3f}"]
        lines.append(f"file '{Path(tmp) / f'{len(merged) - 1:05d}.png'}'")
        lst = Path(tmp) / "list.txt"
        lst.write_text("\n".join(lines) + "\n")
        vf = f"fps={fps_cap},scale=trunc(iw/2)*2:trunc(ih/2)*2,format=yuv420p"
        for ext, codec in (("mp4", ["-c:v", "libx264", "-crf", "23"]), ("webm", ["-c:v", "libvpx-vp9", "-crf", "36", "-b:v", "0"])):
            subprocess.run(
                ["ffmpeg", "-loglevel", "error", "-y", "-f", "concat", "-safe", "0", "-i", str(lst), "-vf", vf, *codec,
                 str(base.with_suffix("." + ext))], check=True)
    for ext in ("gif", "mp4", "webm"):
        p = base.with_suffix("." + ext)
        print(f"{p}  {p.stat().st_size // 1024} KB")


def main() -> int:
    ap = argparse.ArgumentParser()
    sub = ap.add_subparsers(dest="mode", required=True)
    r = sub.add_parser("record")
    r.add_argument("out", type=Path)
    r.add_argument("cmd", nargs=argparse.REMAINDER)
    p = sub.add_parser("render")
    p.add_argument("src", type=Path)
    p.add_argument("base", type=Path)
    p.add_argument("--cmd", required=True)
    p.add_argument("--title", default="zyvor fabric")
    p.add_argument("--max-gap", type=float, default=1.4)
    p.add_argument("--cols", type=int, default=92)
    p.add_argument("--rows", type=int, default=20)
    p.add_argument("--fps", type=int, default=12)
    a = ap.parse_args()
    if a.mode == "record":
        cmd = a.cmd[1:] if a.cmd and a.cmd[0] == "--" else a.cmd
        return record(a.out, cmd)
    if not shutil.which("ffmpeg"):
        print("ffmpeg not found", file=sys.stderr)
        return 1
    render(a.src, a.base, a.cmd, a.title, a.max_gap, a.cols, a.rows, a.fps)
    return 0


if __name__ == "__main__":
    sys.exit(main())
