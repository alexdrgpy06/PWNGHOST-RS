#!/usr/bin/env python3
"""
Real jayofelony /ui renderer for container parity testing.

INTEGRITY NOTE (2026-07-24):
The previous version of this file was a FAKE shim: it hardcoded /api/status,
/api/config, /api/handshakes, /api/peers JSON endpoints and served a blank stub
PNG at /ui. Real jayofelony (verified against the cloned reference at
pwnagotchi/, commit a15ae8fc, v2.9.5.5) has NO REST/JSON API at all -- its web
layer (pwnagotchi/ui/web/handler.py) only serves server-rendered HTML plus a
polled PNG of the e-ink display. Comparing PWNGHOST-RS's REST API against those
fabricated endpoints was circular and meaningless.

This runner now renders a GENUINE jay frame using jay's OWN rendering code:
  - pwnagotchi/ui/components.py  (LabeledValue, Text, Line -- imported verbatim)
  - pwnagotchi/ui/fonts.py       (DejaVuSansMono setup -- imported verbatim)
  - the exact WaveshareV4 layout coordinates from
    pwnagotchi/ui/hw/waveshare2in13_V4.py
and the exact compose loop from view.View.update() (view.py:400-407).

components.py / fonts.py have NO intra-package imports (only PIL + textwrap),
so we add the ui/ dir to sys.path and import them standalone -- this avoids the
heavy `pwnagotchi` package __init__ tree while still using jay's real pixels.

Endpoints served (matching what real jay actually exposes):
  GET /ui  -> real 250x122 1-bit jay-rendered PNG for a canned deterministic state
  GET /    -> honest HTML note that jay has no REST API
"""

import io
import sys
import logging

from aiohttp import web
from PIL import Image, ImageDraw

# --- Locate jay's real UI modules (standalone, no package __init__) -----------
UI_DIR = "/app/pwnagotchi/pwnagotchi/ui"
if UI_DIR not in sys.path:
    sys.path.insert(0, UI_DIR)

import fonts            # noqa: E402  -> pwnagotchi/ui/fonts.py (verbatim)
import components       # noqa: E402  -> pwnagotchi/ui/components.py (verbatim)
from components import LabeledValue, Text, Line  # noqa: E402

# jay's colour convention (view.py:21-22): WHITE=0x00 background, BLACK=0xFF ink.
WHITE = 0x00
BLACK = 0xFF

# --- Exact WaveshareV4 layout (pwnagotchi/ui/hw/waveshare2in13_V4.py) ---------
# fonts.setup(bold, bold_small, medium, huge, bold_big, small)
FONT_SIZES = (10, 9, 10, 35, 25, 9)   # verbatim from WaveshareV4.layout()
LAYOUT = {
    "width": 250,
    "height": 122,
    "face": (0, 34),          # config ui.faces.position_x/position_y (defaults.toml:204-205)
    "name": (5, 20),
    "channel": (0, 0),
    "aps": (28, 0),
    "uptime": (185, 0),
    "line1": [0, 14, 250, 14],
    "line2": [0, 108, 250, 108],
    "shakes": (0, 109),
    "mode": (225, 109),
    "status_pos": (125, 20),
    "status_max": 20,
}

# --- Canned deterministic state (mirrors the downtown_walk scenario mid-run) ---
CANNED = {
    "channel": "06",
    "aps": "3 (10)",
    "uptime": "00:12:34",
    "name": "pwnagotchi-jay>",
    "status": "Hi, I'm Pwnagotchi! Deauthing...",
    "face": "(⌐■_■)",
    "shakes": "2 (05)",
    "mode": "AUTO",
}


def _init_fonts():
    # Replicate fonts.init(config) with jay's default DejaVuSansMono font
    # (defaults.toml:177 ui.font.name, :178 size_offset=0), then the exact
    # WaveshareV4 setup() sizes.
    fonts.STATUS_FONT_NAME = "DejaVuSansMono"
    fonts.SIZE_OFFSET = 0
    fonts.setup(*FONT_SIZES)


def render_frame() -> bytes:
    """Compose a real jay frame exactly like view.View.update() does."""
    # Build the same UI element dict view.__init__ builds (view.py:60-96),
    # using jay's real components and canned values.
    state = {
        "channel": LabeledValue(color=BLACK, label="CH", value=CANNED["channel"],
                                position=LAYOUT["channel"],
                                label_font=fonts.Bold, text_font=fonts.Medium),
        "aps": LabeledValue(color=BLACK, label="APS", value=CANNED["aps"],
                            position=LAYOUT["aps"],
                            label_font=fonts.Bold, text_font=fonts.Medium),
        "uptime": LabeledValue(color=BLACK, label="UP", value=CANNED["uptime"],
                               position=LAYOUT["uptime"],
                               label_font=fonts.Bold, text_font=fonts.Medium),
        "line1": Line(LAYOUT["line1"], color=BLACK),
        "line2": Line(LAYOUT["line2"], color=BLACK),
        "face": Text(value=CANNED["face"], position=LAYOUT["face"],
                     color=BLACK, font=fonts.Huge),
        "name": Text(value=CANNED["name"], position=LAYOUT["name"],
                     color=BLACK, font=fonts.Bold),
        "status": Text(value=CANNED["status"], position=LAYOUT["status_pos"],
                       color=BLACK, font=fonts.status_font(fonts.Medium),
                       wrap=True, max_length=LAYOUT["status_max"]),
        "shakes": LabeledValue(label="PWND ", value=CANNED["shakes"], color=BLACK,
                               position=LAYOUT["shakes"],
                               label_font=fonts.Bold, text_font=fonts.Medium),
        "mode": Text(value=CANNED["mode"], position=LAYOUT["mode"],
                     color=BLACK, font=fonts.Bold),
    }

    # Exact compose loop from view.py:400-407
    canvas = Image.new("1", (LAYOUT["width"], LAYOUT["height"]), WHITE)
    drawer = ImageDraw.Draw(canvas)
    for _key, element in state.items():
        element.draw(canvas, drawer)

    buf = io.BytesIO()
    canvas.save(buf, format="PNG")
    return buf.getvalue()


async def handle_ui(request):
    try:
        png = render_frame()
    except Exception as e:
        logging.exception("jay render failed")
        return web.Response(status=500, text=f"render error: {e}")
    return web.Response(body=png, content_type="image/png")


async def handle_root(request):
    return web.Response(
        content_type="text/html",
        text=(
            "<html><body><h1>jayofelony parity container</h1>"
            "<p>Real jayofelony has <b>no REST/JSON API</b>. Its web layer only "
            "serves server-rendered HTML and a polled PNG of the e-ink display "
            "(pwnagotchi/ui/web/handler.py). The only cross-implementation "
            "surface is <a href='/ui'>/ui</a>, rendered here with jay's real "
            "components.py + fonts.py + WaveshareV4 coordinates.</p>"
            "</body></html>"
        ),
    )


def main():
    logging.basicConfig(level=logging.INFO)
    _init_fonts()
    # Fail fast & loud if fonts are missing (so we never silently serve a blank).
    _ = render_frame()
    app = web.Application()
    app.router.add_get("/", handle_root)
    app.router.add_get("/ui", handle_ui)
    print("[JAYOFELONY] Real /ui renderer running at http://0.0.0.0:8080/ui")
    web.run_app(app, host="0.0.0.0", port=8080)


if __name__ == "__main__":
    main()
