//! Font handling for the display.
//!
//! Wraps `embedded-graphics` built-in monospace ASCII fonts and exposes a
//! small registry so callers can look them up by name, plus a pre-rasterized
//! bitmap glyph atlas (see [`kaomoji_font_data`]) covering the Unicode
//! kaomoji glyphs used by the classic pwnagotchi face system (`agent::faces`)
//! that `embedded-graphics`'s built-in ASCII fonts cannot render at all.
//!
//! ## Kaomoji glyph atlas provenance
//!
//! `kaomoji_font_data.rs` is a *generated* file: it is not hand-written and
//! should not be hand-edited. It was produced by rasterizing every
//! codepoint used across the face strings in `crates/agent/src/faces.rs`
//! and `crates/ui/display/src/lib.rs::face_for_mood` out of **GNU Unifont
//! 16.0.04** (<https://unifoundry.com/unifont/>), which is dual-licensed
//! under the SIL Open Font License 1.1 and the GNU GPLv2+ with the GNU font
//! embedding exception (either license permits embedding the resulting
//! bitmap subset in this project). Unifont was chosen because it is a
//! single font with near-total Unicode BMP coverage (Latin, Arabic,
//! Kannada, CJK, and the various symbol blocks the kaomoji set spans all at
//! once), and because it *is already a bitmap font* -- rasterizing it at
//! its native 16px em produces crisp, un-antialiased 1bpp glyphs with no
//! runtime TTF parsing required, which matches SPEC.md's stated preference
//! for "pre-rasterized" face rendering on a resource-constrained ARMv6
//! target.
//!
//! To regenerate `kaomoji_font_data.rs` after the face/kaomoji set changes:
//! 1. Download a recent Unifont TTF release, e.g.
//!    `https://github.com/multitheftauto/unifont/releases/download/vX.Y.Z/unifont-X.Y.Z.ttf`
//!    (official builds no longer ship a `.ttf` directly; this is a
//!    community mirror that rebuilds one from the official `.hex`/`.bdf`
//!    sources on every release).
//! 2. Extract the codepoints used in every face string (see the two files
//!    above) and rasterize each one to a 16x16 1bpp cell, rows packed
//!    MSB-first and padded to a byte boundary per row, `bit == 1` meaning
//!    "ink"/pixel-on. Any Unicode combining mark (there are two in the
//!    current set, U+0300 and U+0301) must be rasterized with its ink
//!    shifted into the cell (Unifont defines combining marks with a
//!    *negative* x-origin meant to stack over the previous glyph) and
//!    listed in `COMBINING_MARKS` so [`crate::layout`] composites it onto
//!    the previous cell instead of advancing the cursor.
//! 3. Emit `(char, [u8; GLYPH_BYTES])` pairs sorted by `char` into
//!    `KAOMOJI_GLYPHS` so [`kaomoji_glyph`] can binary-search it.
//!
//! This was done with a Python + Pillow (FreeType-backed) one-off script;
//! any TTF rasterizer (`ab_glyph`, `fontdue`, FreeType) would work
//! equivalently for a future `build.rs`-based regeneration step.

use crate::kaomoji_font_data;
use embedded_graphics::mono_font::{ascii, MonoFont};
use std::collections::HashMap;

pub use kaomoji_font_data::{GLYPH_BYTES, GLYPH_CELL_H, GLYPH_CELL_W};

/// Look up the pre-rasterized bitmap for `ch`, if this glyph atlas covers it.
///
/// Returns a `GLYPH_CELL_W x GLYPH_CELL_H` 1bpp bitmap, rows packed
/// MSB-first and padded to a byte boundary per row (bit == 1 means "ink").
pub fn kaomoji_glyph(ch: char) -> Option<&'static [u8; GLYPH_BYTES]> {
    kaomoji_font_data::KAOMOJI_GLYPHS
        .binary_search_by_key(&ch, |(c, _)| *c)
        .ok()
        .map(|idx| &kaomoji_font_data::KAOMOJI_GLYPHS[idx].1)
}

/// True if `ch` is a zero-advance-width combining mark in the kaomoji glyph
/// set (must be composited onto the previously drawn cell rather than
/// advancing the cursor to a new one).
pub fn is_combining_mark(ch: char) -> bool {
    kaomoji_font_data::COMBINING_MARKS.contains(&ch)
}

/// The cursor advance width (unscaled, in pixels) for a rasterized glyph
/// cell. GNU Unifont is a *dual-width* bitmap font: Latin/ASCII/punctuation
/// codepoints (e.g. `(`, `_`, `)`, space) only ever paint the left half of
/// their `GLYPH_CELL_W`-wide cell, while wide symbol/CJK codepoints (many
/// of the mood-face glyphs, e.g. U+2686) use the full cell. Advancing every
/// glyph by the full cell width regardless -- what this atlas's consumers
/// used to do -- doubles the on-screen width of every narrow character,
/// which is why kaomoji faces rendered as isolated characters scattered
/// across nearly the full panel width instead of a compact face (confirmed
/// against a real device photo). Detected here by checking whether any ink
/// falls in the right half of the cell; narrow glyphs never do.
pub fn glyph_advance_width(bits: &[u8; GLYPH_BYTES]) -> u32 {
    let row_bytes = (GLYPH_CELL_W as usize).div_ceil(8);
    if row_bytes < 2 {
        return GLYPH_CELL_W;
    }
    let is_wide = (0..GLYPH_CELL_H as usize).any(|row| {
        bits[row * row_bytes + 1..(row + 1) * row_bytes]
            .iter()
            .any(|&b| b != 0)
    });
    if is_wide {
        GLYPH_CELL_W
    } else {
        GLYPH_CELL_W / 2
    }
}

/// Font registry mapping names to built-in monospace fonts.
pub struct FontRegistry {
    fonts: HashMap<String, &'static MonoFont<'static>>,
}

impl FontRegistry {
    pub fn new() -> Self {
        let mut registry = Self {
            fonts: HashMap::new(),
        };
        registry.register_builtin_fonts();
        registry
    }

    fn register_builtin_fonts(&mut self) {
        self.fonts.insert("small".to_string(), &ascii::FONT_6X10);
        self.fonts.insert("regular".to_string(), &ascii::FONT_8X13);
        self.fonts
            .insert("bold".to_string(), &ascii::FONT_9X15_BOLD);
        self.fonts.insert("large".to_string(), &ascii::FONT_10X20);
    }

    pub fn get(&self, name: &str) -> Option<&'static MonoFont<'static>> {
        self.fonts.get(name).copied()
    }

    pub fn register(&mut self, name: String, font: &'static MonoFont<'static>) {
        self.fonts.insert(name, font);
    }

    pub fn len(&self) -> usize {
        self.fonts.len()
    }

    pub fn is_empty(&self) -> bool {
        self.fonts.is_empty()
    }
}

impl Default for FontRegistry {
    fn default() -> Self {
        Self::new()
    }
}

/// Default font for body text.
pub fn default_font() -> &'static MonoFont<'static> {
    &ascii::FONT_6X10
}

/// Bold font for headers.
pub fn bold_font() -> &'static MonoFont<'static> {
    &ascii::FONT_9X15_BOLD
}

/// Small font for the status bar.
pub fn small_font() -> &'static MonoFont<'static> {
    &ascii::FONT_6X10
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_font_registry() {
        let registry = FontRegistry::new();
        assert!(!registry.is_empty());
        assert!(registry.get("small").is_some());
        assert!(registry.get("bold").is_some());
        assert!(registry.get("nonexistent").is_none());
    }
}
