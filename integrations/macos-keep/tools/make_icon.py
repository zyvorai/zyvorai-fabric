#!/usr/bin/env python3
"""Draws the Solvor app icon and the menu-bar glyph, and writes the asset catalogue.

The mark is an original face (two eyes, a smile) — not a letter, not anyone else's mascot — on an Apple-blue gradient, inside a
sealed "cell" frame whose rings are closing. One drop of Zyvor orange (`--hs-accent-fill #ff5a15` at zyvor.dev) sits on it, the
same warm glow the rest of the app keeps for a single accent. Needs rsvg-convert (brew install librsvg). Run from anywhere;
writes Resources/Assets.xcassets and docs/assets/merion/solvor-icon.png.
"""
import json, os, subprocess, sys

here = os.path.dirname(os.path.abspath(__file__))
root = os.path.join(here, "..")
assets = os.path.join(root, "Resources", "Assets.xcassets")
docs_png = os.path.join(root, "..", "..", "docs", "assets", "keep", "solvor-icon.png")

# The face (a 64-unit box, like FaceMark in Theme.swift): two eyes and a smile, mapped to the 1024 canvas.
def _pt(x, y, k, cx, cy):
    return (cx + (x - 32) * k, cy + (y - 32) * k)

def _smile(k, cx, cy):
    (sx1, sy1) = _pt(17, 40, k, cx, cy)
    (sx2, sy2) = _pt(47, 40, k, cx, cy)
    (scx, scy) = _pt(32, 57, k, cx, cy)
    return f"M{sx1:.1f},{sy1:.1f} Q{scx:.1f},{scy:.1f} {sx2:.1f},{sy2:.1f}"

def face_paths(k, cx=512, cy=512):
    """Eyes as one stroked path, for the detailed icon and the menu-bar glyph."""
    (ex1, ey1), (ex1b, ey1b) = _pt(22, 25, k, cx, cy), _pt(22, 35, k, cx, cy)
    (ex2, ey2), (ex2b, ey2b) = _pt(42, 25, k, cx, cy), _pt(42, 35, k, cx, cy)
    eyes = f"M{ex1:.1f},{ey1:.1f} L{ex1b:.1f},{ey1b:.1f} M{ex2:.1f},{ey2:.1f} L{ex2b:.1f},{ey2b:.1f}"
    return eyes, _smile(k, cx, cy)

def dot_face(k, cx, cy):
    """Eyes as two filled dots, legible at 16 px where two short line strokes blur into the smile."""
    return _pt(22, 30, k, cx, cy), _pt(42, 30, k, cx, cy), _smile(k, cx, cy)

def icon_svg(simple=False):
    """The app icon: an original face inside a sealed cell whose rings are closing around it, on a deep-to-bright Apple blue with a
    glass sheen, a warm orange glow, and one orange spark. `simple` drops the rings, sparks and glass sheen and draws a bigger, bolder
    face with dot eyes instead of stroked ones — used at 16 and 32 px, where the detailed version blurs into mud; the detailed one is
    used from 128 px up, where it rewards a closer look."""
    def star(cx, cy, r, op, color="#fff"):
        k = r * 0.18
        return (f'<path d="M{cx},{cy-r} Q{cx+k},{cy-k} {cx+r},{cy} Q{cx+k},{cy+k} {cx},{cy+r} Q{cx-k},{cy+k} {cx-r},{cy} Q{cx-k},{cy-k} {cx},{cy-r} Z" '
                f'fill="{color}" fill-opacity="{op}"/>')
    if simple:
        (ex1, ey1), (ex2, ey2), smile = dot_face(6.6, 512, 528)
        return f'''<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 1024 1024" width="1024" height="1024">
  <defs>
    <linearGradient id="bg" x1="130" y1="90" x2="900" y2="950" gradientUnits="userSpaceOnUse">
      <stop offset="0" stop-color="#74baff"/><stop offset="0.38" stop-color="#0071e3"/><stop offset="0.82" stop-color="#0052a3"/><stop offset="1" stop-color="#012a52"/>
    </linearGradient>
    <radialGradient id="light" cx="26%" cy="14%" r="70%">
      <stop offset="0" stop-color="#fff" stop-opacity="0.5"/><stop offset="1" stop-color="#fff" stop-opacity="0"/>
    </radialGradient>
  </defs>
  <rect x="100" y="100" width="824" height="824" rx="188" fill="url(#bg)"/>
  <rect x="100" y="100" width="824" height="824" rx="188" fill="url(#light)"/>
  <circle cx="{ex1:.1f}" cy="{ey1:.1f}" r="42" fill="#fff"/>
  <circle cx="{ex2:.1f}" cy="{ey2:.1f}" r="42" fill="#fff"/>
  <path d="{smile}" fill="none" stroke="#fff" stroke-width="60" stroke-linecap="round"/>
  <path d="M810,214 L836,270 L892,296 L836,322 L810,378 L784,322 L728,296 L784,270 Z" fill="#ff5a15"/>
</svg>
'''
    eyes, smile = face_paths(3.6, 512, 522)
    return f'''<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 1024 1024" width="1024" height="1024">
  <defs>
    <linearGradient id="bg" x1="130" y1="90" x2="900" y2="950" gradientUnits="userSpaceOnUse">
      <stop offset="0" stop-color="#74baff"/><stop offset="0.38" stop-color="#0071e3"/><stop offset="0.82" stop-color="#0052a3"/><stop offset="1" stop-color="#012a52"/>
    </linearGradient>
    <radialGradient id="light" cx="26%" cy="14%" r="70%">
      <stop offset="0" stop-color="#fff" stop-opacity="0.55"/><stop offset="0.5" stop-color="#fff" stop-opacity="0.08"/><stop offset="1" stop-color="#fff" stop-opacity="0"/>
    </radialGradient>
    <radialGradient id="vig" cx="50%" cy="46%" r="72%">
      <stop offset="0.62" stop-color="#001a33" stop-opacity="0"/><stop offset="1" stop-color="#001a33" stop-opacity="0.42"/>
    </radialGradient>
    <linearGradient id="edge" x1="0" y1="0" x2="0" y2="1">
      <stop offset="0" stop-color="#fff" stop-opacity="0.75"/><stop offset="0.5" stop-color="#fff" stop-opacity="0.08"/><stop offset="1" stop-color="#bcd9ff" stop-opacity="0.35"/>
    </linearGradient>
    <linearGradient id="sheen" x1="0" y1="0" x2="0" y2="1">
      <stop offset="0" stop-color="#fff" stop-opacity="0.55"/><stop offset="1" stop-color="#fff" stop-opacity="0"/>
    </linearGradient>
    <linearGradient id="facefill" x1="0" y1="0" x2="0" y2="1">
      <stop offset="0" stop-color="#ffffff"/><stop offset="1" stop-color="#eaf4ff"/>
    </linearGradient>
    <linearGradient id="cell" x1="0" y1="0" x2="1" y2="1">
      <stop offset="0" stop-color="#fff" stop-opacity="0.30"/><stop offset="1" stop-color="#fff" stop-opacity="0.06"/>
    </linearGradient>
    <filter id="shadow" x="-20%" y="-20%" width="140%" height="150%">
      <feDropShadow dx="0" dy="24" stdDeviation="28" flood-color="#00203d" flood-opacity="0.45"/>
    </filter>
    <filter id="faceshadow" x="-25%" y="-25%" width="150%" height="160%">
      <feDropShadow dx="0" dy="12" stdDeviation="12" flood-color="#00305c" flood-opacity="0.5"/>
    </filter>
    <filter id="soft"><feGaussianBlur stdDeviation="46"/></filter>
    <clipPath id="sq"><rect x="100" y="100" width="824" height="824" rx="188"/></clipPath>
  </defs>
  <rect x="100" y="100" width="824" height="824" rx="188" fill="url(#bg)" filter="url(#shadow)"/>
  <g clip-path="url(#sq)">
    <rect x="100" y="100" width="824" height="824" fill="url(#light)"/>
    <rect x="100" y="100" width="824" height="824" fill="url(#vig)"/>
    <!-- the one drop of Zyvor orange -->
    <ellipse cx="660" cy="900" rx="280" ry="200" fill="#ff8a3a" fill-opacity="0.16" filter="url(#soft)"/>
    <!-- the rings closing around the cell -->
    <rect x="182" y="182" width="660" height="660" rx="150" fill="none" stroke="#fff" stroke-opacity="0.16" stroke-width="10"/>
    <rect x="222" y="222" width="580" height="580" rx="132" fill="none" stroke="#fff" stroke-opacity="0.26" stroke-width="12"/>
    <rect x="262" y="262" width="500" height="500" rx="114" fill="none" stroke="#fff" stroke-opacity="0.40" stroke-width="14"/>
  </g>
  <!-- the sealed cell: glass -->
  <rect x="312" y="312" width="400" height="400" rx="92" fill="url(#cell)" stroke="#fff" stroke-opacity="0.55" stroke-width="10"/>
  <path d="M330,420 Q330,330 420,330 L604,330 Q694,330 694,420 Q512,470 330,420 Z" fill="url(#sheen)" fill-opacity="0.55"/>
  <!-- an original face: two eyes, a smile -->
  <path d="{eyes}" fill="none" stroke="url(#facefill)" stroke-width="34" stroke-linecap="round" filter="url(#faceshadow)"/>
  <path d="{smile}" fill="none" stroke="url(#facefill)" stroke-width="34" stroke-linecap="round" filter="url(#faceshadow)"/>
  <!-- sparks: the biggest one is the orange drop -->
  {star(786, 262, 46, 0.95, "#ff5a15")}
  {star(262, 774, 26, 0.7)}
  {star(742, 742, 16, 0.55)}
  <rect x="103" y="103" width="818" height="818" rx="185" fill="none" stroke="url(#edge)" stroke-width="6"/>
</svg>
'''

def glyph_svg():
    """Monochrome, for the menu bar (template image): the cell frame and the face."""
    eyes, smile = face_paths(9.0)
    return f'''<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 1024 1024" width="1024" height="1024">
  <rect x="140" y="140" width="744" height="744" rx="170" fill="none" stroke="#000" stroke-width="70"/>
  <path d="{eyes}" fill="none" stroke="#000" stroke-width="80" stroke-linecap="round"/>
  <path d="{smile}" fill="none" stroke="#000" stroke-width="80" stroke-linecap="round"/>
</svg>
'''

def render(svg_text, size, out):
    os.makedirs(os.path.dirname(out), exist_ok=True)
    p = subprocess.run(["rsvg-convert", "-w", str(size), "-h", str(size), "-o", out], input=svg_text.encode(), capture_output=True)
    if p.returncode != 0:
        sys.exit(p.stderr.decode())

def write_json(path, obj):
    os.makedirs(os.path.dirname(path), exist_ok=True)
    open(path, "w").write(json.dumps(obj, indent=2) + "\n")

svg = icon_svg()
svg_simple = icon_svg(simple=True)
open(os.path.join(here, "icon.svg"), "w").write(svg)
open(os.path.join(here, "icon-simple.svg"), "w").write(svg_simple)
open(os.path.join(here, "glyph.svg"), "w").write(glyph_svg())

# App icon: macOS wants 16, 32, 128, 256, 512 at 1x and 2x. 16 and 32 (the sizes actually shown at their native size, in the
# Dock's smaller views, Finder lists, and Spotlight) use the simplified mark; the rest use the detailed one.
iconset = os.path.join(assets, "AppIcon.appiconset")
images = []
for base in (16, 32, 128, 256, 512):
    for scale in (1, 2):
        name = f"icon_{base}x{base}{'@2x' if scale == 2 else ''}.png"
        render(svg_simple if base <= 32 else svg, base * scale, os.path.join(iconset, name))
        images.append({"idiom": "mac", "size": f"{base}x{base}", "scale": f"{scale}x", "filename": name})
write_json(os.path.join(iconset, "Contents.json"), {"images": images, "info": {"version": 1, "author": "xcode"}})

# Menu-bar glyph as a template image.
glyph = os.path.join(assets, "MenuBarGlyph.imageset")
files = []
for scale in (1, 2, 3):
    name = f"glyph@{scale}x.png"
    render(glyph_svg(), 18 * scale, os.path.join(glyph, name))
    files.append({"idiom": "universal", "scale": f"{scale}x", "filename": name})
write_json(os.path.join(glyph, "Contents.json"), {"images": files, "info": {"version": 1, "author": "xcode"}, "properties": {"template-rendering-intent": "template"}})

# The Z mark alone, for the in-app logo where a bitmap is wanted, and a docs image.
write_json(os.path.join(assets, "Contents.json"), {"info": {"version": 1, "author": "xcode"}})
render(svg, 512, docs_png)
import tempfile
render(svg, 1024, os.path.join(tempfile.gettempdir(), "solvor-icon-1024.png"))
print("icon written:", os.path.relpath(iconset), "and", os.path.relpath(glyph))
