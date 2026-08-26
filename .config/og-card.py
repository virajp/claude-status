#!/usr/bin/env python3
"""Regenerate site/static/og-card.png — the link-preview image.

NOT part of any build. Nothing in CI, the test suite or `site:build` runs this;
it exists so the card is reproducible rather than a binary nobody can rebuild.
Run it by hand after changing the brand, the tagline or the statusline
screenshot:

    python3 -m venv /tmp/og && /tmp/og/bin/pip install pillow fonttools brotli
    /tmp/og/bin/python .config/og-card.py

Pillow is the only reason this is Python. The tree is otherwise Rust and zola,
and this script is deliberately outside it: no manifest, no lockfile, nothing
imported by anything.

EVERYTHING IT DRAWS IS READ FROM THE REPOSITORY, never restated here:

  - the colours come out of `site/static/style.css`, parsed from the token
    block, so a palette change cannot leave the card behind;
  - the faces are the same self-hosted woff2 the site serves, decompressed to
    TTF in a temp dir because FreeType cannot read woff2;
  - the headline and tagline come from `site/content/_index.md` and
    `site/config.toml`;
  - the bar is `site/static/statusline.png`, the real screenshot the landing
    page uses.

The one thing stated here is the LAYOUT, because a layout is not a value.
"""

import io
import os
import re
import struct
import sys
import tempfile

from PIL import Image, ImageDraw, ImageFont
from fontTools.ttLib import TTFont

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
OUT = os.path.join(ROOT, "site/static/og-card.png")

# 1200x630 is the size every consumer of `og:image` is tuned for; WhatsApp
# crops toward the centre, so nothing that matters goes near an edge.
W, H = 1200, 630
MARGIN = 72


def token(css, name):
    """One custom property's value, from the stylesheet's own token block."""
    m = re.search(rf"^\s*{re.escape(name)}:\s*(#[0-9a-fA-F]{{3,8}});", css, re.M)
    if not m:
        sys.exit(f"{name} is not in style.css — the card cannot invent a colour")
    return m.group(1)


def face(rel, tmp):
    """A woff2 from the site, as a TTF path FreeType will open."""
    src = os.path.join(ROOT, "site/static/fonts", rel)
    dst = os.path.join(tmp, rel.replace(".woff2", ".ttf"))
    f = TTFont(src)
    f.flavor = None
    f.save(dst)
    return dst


def main():
    css = open(os.path.join(ROOT, "site/static/style.css"), encoding="utf-8").read()
    ink_page = token(css, "--ink-800")
    ink_sunken = token(css, "--ink-900")
    amber = token(css, "--amber-500")
    text_hi = token(css, "--slate-300")
    text_body = token(css, "--slate-200")
    text_muted = token(css, "--slate-100")
    hairline = token(css, "--border-hairline")

    index = open(os.path.join(ROOT, "site/content/_index.md"), encoding="utf-8").read()
    headline = re.search(r'^headline\s*=\s*"(.+)"', index, re.M).group(1)
    conf = open(os.path.join(ROOT, "site/config.toml"), encoding="utf-8").read()
    tagline = re.search(r'^description\s*=\s*"(.+)"', conf, re.M).group(1)
    base_url = re.search(r'^base_url\s*=\s*"(.+)"', conf, re.M).group(1)
    host = base_url.split("//", 1)[1].rstrip("/")

    with tempfile.TemporaryDirectory() as tmp:
        mono_500 = face("ibm-plex-mono-500.woff2", tmp)
        mono_600 = face("ibm-plex-mono-600.woff2", tmp)
        mono_400 = face("ibm-plex-mono-400.woff2", tmp)

        f_word = ImageFont.truetype(mono_600, 44)
        f_word_light = ImageFont.truetype(mono_500, 44)
        f_head = ImageFont.truetype(mono_500, 56)
        f_tag = ImageFont.truetype(mono_400, 27)
        f_host = ImageFont.truetype(mono_400, 24)

        img = Image.new("RGB", (W, H), ink_page)
        d = ImageDraw.Draw(img)

        # ---- the mark, from base.html's inline SVG (viewBox 0 0 72 24) ----
        # Scaled 2.4x. The polyline is the same point list; carrying the real
        # geometry rather than an approximation is why this is drawn and not
        # pasted from an export that could drift.
        s = 2.4
        ox, oy = MARGIN, MARGIN
        d.rounded_rectangle([ox, oy, ox + 72 * s, oy + 24 * s], radius=12 * s, fill=amber)
        pts = [(10, 12), (22, 12), (27, 5), (33, 19), (39, 12), (62, 12)]
        d.line([(ox + x * s, oy + y * s) for x, y in pts],
               fill=ink_page, width=int(2.6 * s), joint="curve")

        # ---- the wordmark, beside the mark ----
        wx = ox + 72 * s + 28
        wy = oy + (24 * s - 44) / 2 - 4
        d.text((wx, wy), "claude", font=f_word_light, fill=text_body)
        wx += d.textlength("claude", font=f_word_light)
        d.text((wx, wy), "status", font=f_word, fill=amber)

        # ---- headline, wrapped to the card rather than to a guess ----
        y = oy + 24 * s + 92
        words, line, lines = headline.split(), "", []
        for word in words:
            trial = (line + " " + word).strip()
            if d.textlength(trial, font=f_head) > W - 2 * MARGIN and line:
                lines.append(line)
                line = word
            else:
                line = trial
        lines.append(line)
        for ln in lines:
            d.text((MARGIN, y), ln, font=f_head, fill=text_hi)
            y += 70

        d.text((MARGIN, y + 12), tagline, font=f_tag, fill=text_muted)

        # ---- the bar itself, along the bottom ----
        # The product is a status line, so the screenshot IS the picture. It is
        # 16.6:1, which is exactly why it works as a band and not as the whole
        # card.
        bar = Image.open(os.path.join(ROOT, "site/static/statusline.png")).convert("RGB")
        bw = W - 2 * MARGIN
        bh = round(bar.height * bw / bar.width)
        bar = bar.resize((bw, bh), Image.LANCZOS)
        by = H - MARGIN - bh
        d.rounded_rectangle([MARGIN - 14, by - 14, MARGIN + bw + 14, by + bh + 14],
                            radius=14, fill=ink_sunken, outline=hairline, width=1)
        img.paste(bar, (MARGIN, by))

        d.text((MARGIN, by - 58), host, font=f_host, fill=text_muted)

        img.save(OUT, "PNG", optimize=True)

    w, h = struct.unpack(">II", open(OUT, "rb").read()[16:24])
    print(f"wrote {OUT} {w}x{h} {os.path.getsize(OUT)} bytes")


if __name__ == "__main__":
    main()
