//! TrueType face/text rendering, matching how real pwnagotchi draws its UI.
//!
//! Real pwnagotchi (`pwnagotchi/ui/fonts.py`) renders every text element --
//! and crucially the face -- as **DejaVuSansMono** / **DejaVuSansMono-Bold**
//! outlines rasterized by PIL/FreeType. The 2.13" V4 panel uses
//! `fonts.setup(10, 9, 10, 35, 25, 9)` (`hw/waveshare2in13_V4.py::layout`),
//! i.e. the face is `fonts.Huge` = DejaVuSansMono-Bold at **35 px**, and the
//! body/labels are ~10 px.
//!
//! Our previous approach blitted a 16x16 GNU Unifont *bitmap* atlas upscaled
//! 2x by nearest-neighbor, which looked blocky and nothing like the original.
//! This module replaces that with real outline rasterization via
//! [`fontdue`] (pure Rust, no FreeType/C dependency, cross-compiles cleanly
//! to the ARMv6 Pi Zero W target), bundling the actual DejaVu TTFs so the
//! result is pixel-faithful to real pwnagotchi wherever the font has the
//! glyph.
//!
//! DejaVu Sans Mono does not cover every kaomoji symbol (e.g. `⚆` U+2686);
//! for those codepoints the caller falls back to the existing Unifont bitmap
//! atlas (see `crate::fonts::kaomoji_glyph`). This module reports coverage so
//! the caller can make that decision per-glyph.

use fontdue::{Font, FontSettings};
use std::cell::RefCell;
use std::collections::HashMap;

/// Bundled DejaVuSansMono-Bold -- the face font (`fonts.Huge`) and the bold
/// body labels. Embedded so the binary is self-contained (no runtime font
/// files on the device).
static FONT_BOLD_BYTES: &[u8] = include_bytes!("../assets/DejaVuSansMono-Bold.ttf");
/// Bundled DejaVuSansMono (regular) -- the medium/status/small text.
static FONT_REGULAR_BYTES: &[u8] = include_bytes!("../assets/DejaVuSansMono.ttf");

thread_local! {
    static RENDERER: RefCell<Option<TtfRenderer>> = const { RefCell::new(None) };
}

/// A single rasterized glyph: its coverage bitmap (0..=255 per pixel, row-
/// major, `width * height` bytes) plus the metrics needed to place it.
#[derive(Clone)]
pub struct RasterGlyph {
    pub width: usize,
    pub height: usize,
    /// Coverage, one byte per pixel (0 = transparent, 255 = full ink).
    pub coverage: Vec<u8>,
    /// Pen advance to the next glyph, in pixels (already rounded).
    pub advance: i32,
    /// Left bearing: x offset from the pen to the bitmap's left edge.
    pub left: i32,
    /// Top bearing: y offset from the glyph's baseline up to the bitmap's top
    /// edge (positive = above baseline).
    pub top: i32,
}

/// Which bundled face to rasterize with.
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub enum TtfStyle {
    Bold,
    Regular,
}

struct TtfRenderer {
    bold: Font,
    regular: Font,
    /// Cache keyed by (style, char, px-size). With `ui.fps = 1` only a
    /// handful of distinct glyphs are drawn per second, so this stays tiny
    /// and eliminates repeated rasterization cost on the ARMv6 target.
    cache: HashMap<(TtfStyle, char, u32), Option<RasterGlyph>>,
}

impl TtfRenderer {
    fn new() -> Self {
        let settings = FontSettings::default();
        let bold = Font::from_bytes(FONT_BOLD_BYTES, settings)
            .expect("bundled DejaVuSansMono-Bold.ttf must parse");
        let regular = Font::from_bytes(FONT_REGULAR_BYTES, settings)
            .expect("bundled DejaVuSansMono.ttf must parse");
        Self {
            bold,
            regular,
            cache: HashMap::new(),
        }
    }

    fn font(&self, style: TtfStyle) -> &Font {
        match style {
            TtfStyle::Bold => &self.bold,
            TtfStyle::Regular => &self.regular,
        }
    }

    /// Rasterize `ch` at `px` in `style`, or `None` if the font has no glyph
    /// for that codepoint (so the caller can fall back to the Unifont atlas).
    /// `fontdue` maps a missing codepoint to glyph index 0 (`.notdef`); we
    /// detect that via `lookup_glyph_index` and return `None` rather than
    /// drawing a tofu box.
    fn rasterize(&mut self, style: TtfStyle, ch: char, px: u32) -> Option<RasterGlyph> {
        if let Some(cached) = self.cache.get(&(style, ch, px)) {
            return cached.clone();
        }
        let font = self.font(style);
        // A space (or any zero-coverage glyph) still has a real advance and
        // must render as blank -- keep it, don't treat it as missing.
        let has_glyph = ch == ' ' || font.lookup_glyph_index(ch) != 0;
        let result = if has_glyph {
            let (metrics, coverage) = font.rasterize(ch, px as f32);
            Some(RasterGlyph {
                width: metrics.width,
                height: metrics.height,
                coverage,
                advance: metrics.advance_width.round() as i32,
                left: metrics.xmin,
                // fontdue's ymin is the offset from baseline to the *bottom*
                // of the bitmap (y-up). The top edge is ymin + height.
                top: metrics.ymin + metrics.height as i32,
            })
        } else {
            None
        };
        self.cache.insert((style, ch, px), result.clone());
        result
    }
}

/// Rasterize `ch` at `px` pixels in `style`. Returns `None` if the bundled
/// DejaVu font lacks the glyph (caller should fall back to the Unifont atlas).
pub fn rasterize_glyph(style: TtfStyle, ch: char, px: u32) -> Option<RasterGlyph> {
    RENDERER.with(|r| {
        let mut slot = r.borrow_mut();
        if slot.is_none() {
            *slot = Some(TtfRenderer::new());
        }
        slot.as_mut().unwrap().rasterize(style, ch, px)
    })
}

/// The pen advance (px) for `ch` at `px` in `style`, whether or not the glyph
/// has visible ink. Falls back to a monospace-cell advance (`px * 0.6`,
/// DejaVu Sans Mono's advance is 0.602 em) if the font lacks the glyph, so a
/// Unifont-fallback glyph still advances sensibly.
pub fn advance_of(style: TtfStyle, ch: char, px: u32) -> i32 {
    match rasterize_glyph(style, ch, px) {
        Some(g) => g.advance,
        None => (px as f32 * 0.602).round() as i32,
    }
}

/// The font's ascent (baseline-to-top, px) at `px` in `style`. Used to
/// convert a top-left text anchor (real pwnagotchi/PIL convention) into the
/// per-glyph baseline positions fontdue works in: for a line drawn with its
/// top at `y`, the baseline sits at `y + ascent`.
pub fn line_ascent(style: TtfStyle, px: u32) -> i32 {
    RENDERER.with(|r| {
        let mut slot = r.borrow_mut();
        if slot.is_none() {
            *slot = Some(TtfRenderer::new());
        }
        let renderer = slot.as_ref().unwrap();
        renderer
            .font(style)
            .horizontal_line_metrics(px as f32)
            .map(|m| m.ascent.round() as i32)
            .unwrap_or(px as i32)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_rasterize_basic_ascii() {
        let g = rasterize_glyph(TtfStyle::Bold, 'A', 35).expect("DejaVu has 'A'");
        assert!(g.width > 0 && g.height > 0);
        assert_eq!(g.coverage.len(), g.width * g.height);
        assert!(g.advance > 0);
        // At 35px the 'A' should be a substantial glyph, not a 16px cell.
        assert!(g.height > 20, "expected a ~35px-tall glyph, got {}", g.height);
    }

    #[test]
    fn test_space_is_present_but_blank() {
        let g = rasterize_glyph(TtfStyle::Regular, ' ', 20).expect("space is a real glyph");
        // Space has advance but no ink.
        assert!(g.advance > 0);
        assert!(g.coverage.iter().all(|&c| c == 0));
    }

    #[test]
    fn test_missing_glyph_returns_none() {
        // U+5355 (单), a CJK ideograph, is not in DejaVu Sans Mono; it must
        // report missing so the caller falls back to the Unifont atlas.
        // (Notably, DejaVu Sans Mono Bold *does* cover every symbol our
        // current face set actually uses -- ⚆ ☉ ◕ ⇀ ↼ ≖ ° ▃ ⌐ ■ • ᵔ ◡ ☼ ✜
        // ب ╥ ☁ ♥ ☓ etc. -- so the TTF path renders all real faces natively
        // and this fallback is a rarely-hit safety net.)
        assert!(rasterize_glyph(TtfStyle::Bold, '\u{5355}', 35).is_none());
    }

    #[test]
    fn test_monospace_advance_is_uniform() {
        // DejaVu Sans Mono: every covered ASCII glyph shares one advance.
        let a = advance_of(TtfStyle::Regular, 'a', 20);
        let w = advance_of(TtfStyle::Regular, 'W', 20);
        let i = advance_of(TtfStyle::Regular, 'i', 20);
        assert_eq!(a, w);
        assert_eq!(a, i);
    }

    #[test]
    fn test_advance_scales_with_size() {
        let small = advance_of(TtfStyle::Bold, 'M', 10);
        let big = advance_of(TtfStyle::Bold, 'M', 35);
        assert!(big > small);
    }
}
