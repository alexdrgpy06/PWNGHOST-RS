//! Layout engine for the display.
//!
//! Renders the pwnagotchi status frame into a 1-bit-per-pixel packed
//! framebuffer using `embedded-graphics` and its built-in ASCII fonts.

use crate::fonts;
use anyhow::Result;
use embedded_graphics::{
    mono_font::{ascii::FONT_6X10, ascii::FONT_9X15, MonoTextStyle},
    pixelcolor::BinaryColor,
    prelude::*,
    primitives::{Line, Primitive, PrimitiveStyle},
    text::{Alignment, Baseline, Text},
};

/// Layout configuration.
///
/// Every position below is copied directly from real jayofelony/
/// pwnagotchi's own display driver for this exact panel --
/// `pwnagotchi/ui/hw/waveshare2in13_V4.py`'s `layout()` method (fetched
/// from the live repo, not eyeballed from a screenshot):
/// ```python
/// self._layout['face'] = (0, 40)
/// self._layout['name'] = (5, 20)
/// self._layout['channel'] = (0, 0)
/// self._layout['aps'] = (28, 0)
/// self._layout['uptime'] = (185, 0)
/// self._layout['line1'] = [0, 14, 250, 14]
/// self._layout['line2'] = [0, 108, 250, 108]
/// self._layout['friend_face'] = (0, 92)
/// self._layout['friend_name'] = (40, 94)
/// self._layout['shakes'] = (0, 109)
/// self._layout['mode'] = (225, 109)
/// self._layout['status'] = {'pos': (125, 20), 'max': 20}
/// ```
/// `name` and `status` share `y=20`, both *above* `face`'s `y=40` --
/// status's word-wrap extends downward from there, which is why it
/// visually reads as "beside" the face in a real screenshot despite
/// having a different anchor point. There is no dedicated CPU-temp/RAM
/// field anywhere in the real layout (that's normally a plugin adding
/// its own UI element at runtime, a capability this project's Lua
/// plugins don't have) -- rather than invent a row real pwnagotchi
/// doesn't have, that data stays on the web dashboard and in
/// `memtemp.status`/the log instead of crowding this screen.
#[derive(Debug, Clone)]
pub struct LayoutConfig {
    pub name_x: i32,
    pub name_y: i32,
    pub mode_x: i32,
    pub mode_y: i32,
    pub channel_x: i32,
    pub channel_y: i32,
    pub aps_x: i32,
    pub aps_y: i32,
    pub uptime_x: i32,
    pub uptime_y: i32,
    pub line1_y: i32,
    pub face_x: i32,
    pub face_y: i32,
    /// Integer upscale factor for the main face's glyph cells (16x16
    /// base). Real pwnagotchi renders its face in a much larger font
    /// (`fonts.setup(..., huge=35, ...)`) than the rest of the UI
    /// (~9-10) -- rendering it at the same size as everything else was
    /// the single biggest reason this used to look "not even close to
    /// the original" even once every field held real data. The bitmap
    /// glyph atlas is a fixed 16px cell (not a scalable vector font like
    /// real pwnagotchi's), so this is an approximation of the same
    /// visual proportion, not an exact 35pt match.
    pub face_scale: i32,
    pub friend_face_x: i32,
    pub friend_face_y: i32,
    pub friend_x: i32,
    pub friend_y: i32,
    pub line2_y: i32,
    pub status_x: i32,
    pub status_y: i32,
    /// Word-wrap width for the status/phrase text, in characters --
    /// matches real pwnagotchi's `Text(wrap=True, max_length=20)`.
    pub status_max_chars: usize,
    pub shakes_x: i32,
    pub shakes_y: i32,
    pub font_size: u32,
}

impl Default for LayoutConfig {
    fn default() -> Self {
        // 250x122 landscape canvas (Waveshare 2.13" V4) -- see the
        // struct doc comment above for the real source these positions
        // come from.
        Self {
            channel_x: 0,
            channel_y: 0,
            aps_x: 28,
            aps_y: 0,
            uptime_x: 185,
            uptime_y: 0,
            line1_y: 14,
            name_x: 5,
            name_y: 20,
            status_x: 125,
            status_y: 20,
            status_max_chars: 20,
            face_x: 0,
            face_y: 40,
            face_scale: 2,
            friend_face_x: 0,
            friend_face_y: 92,
            friend_x: 40,
            friend_y: 94,
            line2_y: 108,
            shakes_x: 0,
            shakes_y: 109,
            mode_x: 225,
            mode_y: 109,
            font_size: 12,
        }
    }
}

/// A borrowed 1bpp packed framebuffer that implements `DrawTarget`.
///
/// Bit layout: pixel `(x, y)` maps to bit `y * width + x`, packed LSB-first
/// into `buffer`. Out-of-bounds pixels are ignored.
struct FrameBuffer<'a> {
    buffer: &'a mut [u8],
    width: u32,
    height: u32,
}

impl<'a> FrameBuffer<'a> {
    fn new(buffer: &'a mut [u8], width: u32, height: u32) -> Self {
        Self {
            buffer,
            width,
            height,
        }
    }

    fn set_pixel(&mut self, x: i32, y: i32, on: bool) {
        if x < 0 || y < 0 || x as u32 >= self.width || y as u32 >= self.height {
            return;
        }
        let index = y as usize * self.width as usize + x as usize;
        let byte = index / 8;
        let bit = (index % 8) as u8;
        if byte >= self.buffer.len() {
            return;
        }
        if on {
            self.buffer[byte] |= 1 << bit;
        } else {
            self.buffer[byte] &= !(1 << bit);
        }
    }
}

impl OriginDimensions for FrameBuffer<'_> {
    fn size(&self) -> Size {
        Size::new(self.width, self.height)
    }
}

impl DrawTarget for FrameBuffer<'_> {
    type Color = BinaryColor;
    type Error = core::convert::Infallible;

    fn draw_iter<I>(&mut self, pixels: I) -> core::result::Result<(), Self::Error>
    where
        I: IntoIterator<Item = Pixel<Self::Color>>,
    {
        for Pixel(coord, color) in pixels {
            self.set_pixel(coord.x, coord.y, color.is_on());
        }
        Ok(())
    }
}

/// Layout engine for drawing pwnagotchi frames
pub struct LayoutEngine {
    config: LayoutConfig,
}

impl LayoutEngine {
    pub fn new(config: LayoutConfig) -> Self {
        Self { config }
    }

    pub fn config(&self) -> &LayoutConfig {
        &self.config
    }

    /// Draw a complete pwnagotchi frame into `buffer`. Position-for-
    /// position match of real jayofelony/pwnagotchi's own
    /// `waveshare2in13_V4.py` layout (see the `LayoutConfig` doc comment
    /// for the exact source) -- channel/APs/uptime row, a divider,
    /// name+status (both above the face), the face itself, the closest
    /// mesh friend's own face + name/signal/handshakes line, a second
    /// divider, then PWND + mode. There is no CPU-temp/RAM field: real
    /// pwnagotchi doesn't have one without a plugin dynamically adding a
    /// UI element, a capability this project's Lua plugins don't have
    /// (see `LayoutConfig`'s doc comment) -- that data lives on the web
    /// dashboard and in the `memtemp` plugin's own log/status file
    /// instead of crowding a screen real pwnagotchi doesn't crowd either.
    #[allow(clippy::too_many_arguments)]
    pub fn draw_pwnagotchi_frame(
        &self,
        buffer: &mut [u8],
        width: u32,
        height: u32,
        channel: u8,
        aps_count: usize,
        uptime: &str,
        name: &str,
        status: &str,
        face: &str,
        handshakes: u32,
        total_handshakes: u32,
        level: u32,
        xp: u32,
        mode: &str,
        friend: Option<(&str, &str)>,
    ) -> Result<()> {
        let mut fb = FrameBuffer::new(buffer, width, height);
        let small = MonoTextStyle::new(&FONT_6X10, BinaryColor::On);
        let line_style = PrimitiveStyle::with_stroke(BinaryColor::On, 1);

        // Channel / APs / uptime, as separate label+value pairs (real
        // pwnagotchi's `LabeledValue` widget) instead of one
        // concatenated string.
        draw_labeled_value(
            &mut fb,
            "CH",
            &format!("{channel:02}"),
            &small,
            self.config.channel_x,
            self.config.channel_y,
        )?;
        draw_labeled_value(
            &mut fb,
            "APS",
            &aps_count.to_string(),
            &small,
            self.config.aps_x,
            self.config.aps_y,
        )?;
        draw_labeled_value(
            &mut fb,
            "UP",
            uptime,
            &small,
            self.config.uptime_x,
            self.config.uptime_y,
        )?;

        // Divider between the top info row and the name/status/face
        // section -- the horizontal rule is one of the most recognizable
        // pieces of pwnagotchi's screen; a stack of unbordered text lines
        // with no section structure reads as "not the real UI" even once
        // every field shows real data.
        draw_horizontal_divider(&mut fb, width, self.config.line1_y, line_style);

        // Name and status share the same y as real pwnagotchi (both
        // *above* the face's own y) -- status's word-wrap extends
        // downward from here, which is why it visually ends up reading
        // as "beside" the face in a real screenshot despite anchoring
        // higher than the face does.
        draw_line(
            &mut fb,
            &format!("{name}>"),
            &small,
            self.config.name_x,
            self.config.name_y,
        )?;
        for (i, line) in wrap_status_text(status, self.config.status_max_chars)
            .into_iter()
            .take(2)
            .enumerate()
        {
            draw_line(
                &mut fb,
                &line,
                &small,
                self.config.status_x,
                self.config.status_y + i as i32 * 10,
            )?;
        }

        // Face. Faces are kaomoji (e.g. "( ⚆_⚆)", "(♥‿‿♥)") whose glyphs
        // are outside embedded-graphics's built-in ASCII fonts, so
        // they're drawn from the pre-rasterized bitmap glyph atlas in
        // `crate::fonts` instead of `big`/FONT_9X15. Drawn at
        // `face_scale` (default 2x the base 16x16 glyph cell) so it
        // reads as the dominant element the way real pwnagotchi's much
        // larger `Huge` face font does, instead of blending in at the
        // same size as every other field.
        draw_kaomoji_line(
            &mut fb,
            face,
            self.config.face_x,
            self.config.face_y,
            self.config.face_scale,
        );

        // Closest mesh friend, if any -- matches real pwnagotchi's
        // `set_closest_peer`/`friend_face`/`friend_name` fields (the
        // friend's own mood face, plus signal bars + name + handshake
        // counts), populated from `agent::MeshManager`.
        if let Some((friend_face, friend_line)) = friend {
            draw_kaomoji_line(
                &mut fb,
                friend_face,
                self.config.friend_face_x,
                self.config.friend_face_y,
                1,
            );
            draw_line(
                &mut fb,
                friend_line,
                &small,
                self.config.friend_x,
                self.config.friend_y,
            )?;
        }

        // Divider between the face/friend section and the PWND/mode
        // section below it.
        draw_horizontal_divider(&mut fb, width, self.config.line2_y, line_style);

        // PWND: current-epoch handshakes (lifetime total), plus
        // level/XP -- real pwnagotchi doesn't track level/XP itself
        // (that's this project's own RL-agent progression system), so
        // it's folded into this row rather than claiming a base-UI field
        // that doesn't exist upstream.
        draw_labeled_value(
            &mut fb,
            "PWND",
            &format!("{handshakes} ({total_handshakes}) L{level} XP{xp}"),
            &small,
            self.config.shakes_x,
            self.config.shakes_y,
        )?;

        // Operating mode. This project always runs autonomously (no
        // manual/AI toggle exists), so `mode` is a constant "AUTO" -- a
        // real, honest value, not a placeholder standing in for an
        // unimplemented feature.
        draw_line(&mut fb, mode, &small, self.config.mode_x, self.config.mode_y)?;

        Ok(())
    }

    /// Draw a single line of ASCII text centered horizontally, using
    /// `embedded-graphics`'s built-in font. Not suitable for kaomoji faces
    /// (use [`Self::draw_face_centered`] for those).
    pub fn draw_text_centered(
        &self,
        buffer: &mut [u8],
        width: u32,
        height: u32,
        text: &str,
    ) -> Result<()> {
        let mut fb = FrameBuffer::new(buffer, width, height);
        let style = MonoTextStyle::new(&FONT_9X15, BinaryColor::On);
        // `Baseline::Middle` (not the crate's default `Alphabetic`) so
        // the text is actually vertically centered on `height / 2`
        // instead of having that point treated as its bottom edge --
        // see `draw_line`'s doc comment for why Alphabetic baseline is
        // the wrong default here.
        let text_style = embedded_graphics::text::TextStyleBuilder::new()
            .alignment(Alignment::Center)
            .baseline(Baseline::Middle)
            .build();
        Text::with_text_style(
            text,
            Point::new(width as i32 / 2, height as i32 / 2),
            style,
            text_style,
        )
        .draw(&mut fb)
        .ok();
        Ok(())
    }

    /// Draw a kaomoji face string centered horizontally, using the
    /// pre-rasterized bitmap glyph atlas (see `crate::fonts`).
    pub fn draw_face_centered(
        &self,
        buffer: &mut [u8],
        width: u32,
        height: u32,
        face: &str,
    ) -> Result<()> {
        let mut fb = FrameBuffer::new(buffer, width, height);
        let text_w = kaomoji_line_width(face, 1);
        let x = (width as i32 - text_w) / 2;
        let y = (height as i32 - fonts::GLYPH_CELL_H as i32) / 2;
        draw_kaomoji_line(&mut fb, face, x, y, 1);
        Ok(())
    }

    /// Draw text at an explicit position.
    pub fn draw_text(
        &self,
        buffer: &mut [u8],
        width: u32,
        height: u32,
        text: &str,
        x: i32,
        y: i32,
    ) -> Result<()> {
        let mut fb = FrameBuffer::new(buffer, width, height);
        let style = MonoTextStyle::new(&FONT_6X10, BinaryColor::On);
        draw_line(&mut fb, text, &style, x, y)
    }
}

fn draw_line(
    fb: &mut FrameBuffer<'_>,
    text: &str,
    style: &MonoTextStyle<'_, BinaryColor>,
    x: i32,
    y: i32,
) -> Result<()> {
    // `Baseline::Top`, not the crate's default `Baseline::Alphabetic`:
    // every y-coordinate in this layout (copied from real pwnagotchi's
    // own PIL-based `waveshare2in13_V4.py`, which anchors text at its
    // top-left corner) assumes `y` is the top of the glyph. Confirmed by
    // direct experiment that the default Alphabetic baseline treats `y`
    // as the text's *bottom* edge instead, so most of a glyph drawn at
    // e.g. y=0 (the channel/aps/uptime row) rendered above row 0 and got
    // silently clipped off the framebuffer -- on real hardware this
    // showed up as the top status row being reduced to unreadable
    // fragments (confirmed against a real device photo).
    Text::with_baseline(text, Point::new(x, y), *style, Baseline::Top)
        .draw(fb)
        .ok();
    Ok(())
}

/// Draw a "LABEL value" pair the way real pwnagotchi's `LabeledValue`
/// widget does: the label and value are two separately-positioned draw
/// calls (offset a fixed gap apart), not one concatenated string. Assumes
/// `FONT_6X10`, whose glyphs are exactly 6px wide, so the value's x
/// position can be computed directly from the label's length instead of
/// needing a text-measurement API.
fn draw_labeled_value(
    fb: &mut FrameBuffer<'_>,
    label: &str,
    value: &str,
    style: &MonoTextStyle<'_, BinaryColor>,
    x: i32,
    y: i32,
) -> Result<()> {
    draw_line(fb, label, style, x, y)?;
    let value_x = x + label.chars().count() as i32 * 6 + 4;
    draw_line(fb, value, style, value_x, y)
}

/// Draw a full-width horizontal divider line at `y` -- the section
/// borders that make this read as an actual pwnagotchi screen instead of
/// unbordered stacked text.
fn draw_horizontal_divider(fb: &mut FrameBuffer<'_>, width: u32, y: i32, style: PrimitiveStyle<BinaryColor>) {
    Line::new(Point::new(0, y), Point::new(width as i32 - 1, y))
        .into_styled(style)
        .draw(fb)
        .ok();
}

/// Word-wrap `text` into lines of at most `max_chars` characters, the way
/// real pwnagotchi's `status` field wraps via Python's
/// `textwrap.TextWrapper`. A word longer than `max_chars` on its own
/// still gets its own line rather than being split mid-word.
fn wrap_status_text(text: &str, max_chars: usize) -> Vec<String> {
    if max_chars == 0 {
        return vec![text.to_string()];
    }
    let mut lines = Vec::new();
    let mut current = String::new();
    for word in text.split_whitespace() {
        if current.is_empty() {
            current.push_str(word);
        } else if current.chars().count() + 1 + word.chars().count() <= max_chars {
            current.push(' ');
            current.push_str(word);
        } else {
            lines.push(std::mem::take(&mut current));
            current.push_str(word);
        }
    }
    if !current.is_empty() || lines.is_empty() {
        lines.push(current);
    }
    lines
}

/// Draw a kaomoji/face string using the pre-rasterized bitmap glyph atlas
/// (`crate::fonts::kaomoji_glyph`) instead of `embedded-graphics`'s built-in
/// ASCII fonts, which lack coverage for these codepoints entirely.
///
/// Each non-combining character advances the cursor by its *own* rasterized
/// width (see [`fonts::glyph_advance_width`], scaled by `scale`) -- GNU
/// Unifont is dual-width, so narrow glyphs (most ASCII: `(`, `_`, `)`,
/// space) advance half as far as wide ones (many mood-face symbols).
/// Advancing every glyph by a uniform full-cell width, as this used to do,
/// doubles the gap after every narrow character, which is why a face like
/// "( ⚆_⚆)" rendered as isolated characters scattered across nearly the
/// full panel width instead of a compact face (confirmed against a real
/// device photo). Combining marks (e.g. U+0301 in "•́") are composited onto
/// the previously drawn cell instead of advancing. Any codepoint the atlas
/// doesn't cover is skipped but still advances (by a narrow-glyph width)
/// rather than corrupting the rest of the line.
///
/// `scale` matters for visual fidelity against real pwnagotchi: its face
/// renders in a much larger font (`fonts.Huge`, size 25) than the rest of
/// the UI (`fonts.Medium`/`Bold`, size ~9-10) -- roughly 2.5x. Rendering
/// the face at the same size as everything else (scale=1, as this used
/// to do unconditionally) reads as "not even close to the original" even
/// with the right characters. The main face uses scale=2 (see
/// `draw_pwnagotchi_frame`); the friend line keeps scale=1, matching
/// the reference layout's own much smaller friend-face treatment.
fn draw_kaomoji_line(fb: &mut FrameBuffer<'_>, text: &str, x: i32, y: i32, scale: i32) {
    let mut cursor_x = x;
    let mut prev_cell: Option<(i32, i32)> = None;

    for ch in text.chars() {
        if fonts::is_combining_mark(ch) {
            if let Some((px, py)) = prev_cell {
                if let Some(bits) = fonts::kaomoji_glyph(ch) {
                    blit_glyph(fb, bits, px, py, scale);
                }
            }
            continue;
        }

        let advance = match fonts::kaomoji_glyph(ch) {
            Some(bits) => {
                blit_glyph(fb, bits, cursor_x, y, scale);
                fonts::glyph_advance_width(bits)
            }
            None => fonts::GLYPH_CELL_W / 2,
        };
        prev_cell = Some((cursor_x, y));
        cursor_x += advance as i32 * scale;
    }
}

/// Total pixel width [`draw_kaomoji_line`] would occupy for `text` at the
/// given `scale` (combining marks don't add width).
fn kaomoji_line_width(text: &str, scale: i32) -> i32 {
    text.chars()
        .filter(|c| !fonts::is_combining_mark(*c))
        .map(|c| {
            let advance = fonts::kaomoji_glyph(c)
                .map(fonts::glyph_advance_width)
                .unwrap_or(fonts::GLYPH_CELL_W / 2);
            advance as i32 * scale
        })
        .sum()
}

/// Blit a single pre-rasterized glyph cell onto `fb` at `(x, y)` (top-left
/// origin), each source pixel replicated as a `scale x scale` block. Bits
/// are packed MSB-first, padded to a byte boundary per row; bit == 1 means
/// "ink"/pixel-on.
fn blit_glyph(fb: &mut FrameBuffer<'_>, bits: &[u8; fonts::GLYPH_BYTES], x: i32, y: i32, scale: i32) {
    let row_bytes = (fonts::GLYPH_CELL_W as usize).div_ceil(8);
    for gy in 0..fonts::GLYPH_CELL_H as i32 {
        for gx in 0..fonts::GLYPH_CELL_W as i32 {
            let byte = bits[gy as usize * row_bytes + (gx as usize / 8)];
            let bit = (byte >> (7 - (gx as u32 % 8))) & 1;
            if bit == 1 {
                for sy in 0..scale {
                    for sx in 0..scale {
                        fb.set_pixel(x + gx * scale + sx, y + gy * scale + sy, true);
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_layout_config_default() {
        let config = LayoutConfig::default();
        assert_eq!(config.face_x, 0);
        assert_eq!(config.face_y, 40);
        assert_eq!(config.name_x, 5);
        assert_eq!(config.mode_x, 225);
    }

    #[test]
    fn test_draw_sets_pixels() {
        let width = 128u32;
        let height = 64u32;
        let mut buffer = vec![0u8; (width * height / 8) as usize];
        let engine = LayoutEngine::new(LayoutConfig::default());
        engine
            .draw_text(&mut buffer, width, height, "HELLO", 0, 12)
            .unwrap();
        // Some pixels should now be set.
        assert!(buffer.iter().any(|&b| b != 0));
    }

    #[test]
    fn test_draw_face_centered_renders_kaomoji() {
        // Regression test for the "kaomoji renders as tofu" bug: every mood
        // face in `agent::faces` must actually paint pixels, not just
        // advance the cursor over unsupported codepoints.
        let width = 128u32;
        let height = 64u32;
        for face in ["( ⚆_⚆)", "(♥‿‿♥)", "(单__单)", "(•̀ᴗ•́)", "(ب__ب)"] {
            let mut buffer = vec![0u8; (width * height / 8) as usize];
            let engine = LayoutEngine::new(LayoutConfig::default());
            engine
                .draw_face_centered(&mut buffer, width, height, face)
                .unwrap();
            assert!(
                buffer.iter().any(|&b| b != 0),
                "face {face:?} produced an empty framebuffer"
            );
        }
    }

    // Shared test scaffolding: 250x122 matches this project's default
    // DisplayConfig (Waveshare 2.13" V4 landscape).
    fn new_test_buffer() -> (u32, u32, Vec<u8>) {
        let (width, height) = (250u32, 122u32);
        let buffer = vec![0u8; (width as usize * height as usize).div_ceil(8)];
        (width, height, buffer)
    }

    fn pixel_on(buffer: &[u8], width: u32, x: u32, y: u32) -> bool {
        let index = (y as usize) * (width as usize) + (x as usize);
        (buffer[index / 8] >> (index % 8)) & 1 != 0
    }

    fn region_has_pixels(
        buffer: &[u8],
        width: u32,
        x_range: std::ops::Range<u32>,
        y_range: std::ops::Range<u32>,
    ) -> bool {
        x_range
            .clone()
            .any(|x| y_range.clone().any(|y| pixel_on(buffer, width, x, y)))
    }

    #[test]
    fn test_draw_pwnagotchi_frame_places_labeled_fields_at_configured_positions() {
        let (width, height, mut buffer) = new_test_buffer();
        let config = LayoutConfig::default();
        let (ch_x, ch_y) = (config.channel_x as u32, config.channel_y as u32);
        let (aps_x, aps_y) = (config.aps_x as u32, config.aps_y as u32);
        let (up_x, up_y) = (config.uptime_x as u32, config.uptime_y as u32);
        let engine = LayoutEngine::new(config);
        engine
            .draw_pwnagotchi_frame(
                &mut buffer, width, height, 6, 3, "01:02:03", "pwn", "hello", "(◕‿‿◕)", 1, 5, 2,
                150, "AUTO", None,
            )
            .unwrap();

        assert!(
            region_has_pixels(&buffer, width, ch_x..ch_x + 20, ch_y..ch_y + 10),
            "expected CH field near (channel_x={ch_x}, channel_y={ch_y})"
        );
        assert!(
            region_has_pixels(&buffer, width, aps_x..aps_x + 20, aps_y..aps_y + 10),
            "expected APS field near (aps_x={aps_x}, aps_y={aps_y})"
        );
        assert!(
            region_has_pixels(&buffer, width, up_x..up_x + 40, up_y..up_y + 10),
            "expected UP field near (uptime_x={up_x}, uptime_y={up_y})"
        );
    }

    #[test]
    fn test_draw_pwnagotchi_frame_draws_section_dividers() {
        // The horizontal divider lines separating the top info row, the
        // face/friend section, and the status/PWND section are what
        // makes this read as an actual pwnagotchi screen instead of
        // stacked, unbordered lines of text.
        let (width, height, mut buffer) = new_test_buffer();
        let config = LayoutConfig::default();
        let (line1_y, line2_y) = (config.line1_y as u32, config.line2_y as u32);
        let engine = LayoutEngine::new(config);
        engine
            .draw_pwnagotchi_frame(
                &mut buffer, width, height, 1, 0, "0s", "pwn", "", "", 0, 0, 0, 0, "AUTO", None,
            )
            .unwrap();

        let row_coverage =
            |y: u32| (0..width).filter(|&x| pixel_on(&buffer, width, x, y)).count();

        assert!(
            row_coverage(line1_y) > (width as usize) / 2,
            "expected a full-width divider at line1_y={line1_y}"
        );
        assert!(
            row_coverage(line2_y) > (width as usize) / 2,
            "expected a full-width divider at line2_y={line2_y}"
        );
    }

    #[test]
    fn test_draw_pwnagotchi_frame_draws_friend_line_when_present() {
        let (width, height, mut buffer) = new_test_buffer();
        let config = LayoutConfig::default();
        let (friend_x, friend_y) = (config.friend_x as u32, config.friend_y as u32);
        let engine = LayoutEngine::new(config);
        engine
            .draw_pwnagotchi_frame(
                &mut buffer,
                width,
                height,
                1,
                0,
                "0s",
                "pwn",
                "",
                "",
                0,
                0,
                0,
                0,
                "AUTO",
                Some(("(♥‿‿♥)", "|||| buddy 2 (5)")),
            )
            .unwrap();

        assert!(
            region_has_pixels(&buffer, width, friend_x..friend_x + 60, friend_y..friend_y + 10),
            "expected a friend line near (friend_x={friend_x}, friend_y={friend_y})"
        );
    }

    #[test]
    fn test_draw_pwnagotchi_frame_draws_friend_face_when_present() {
        let (width, height, mut buffer) = new_test_buffer();
        let config = LayoutConfig::default();
        let (ff_x, ff_y) = (config.friend_face_x as u32, config.friend_face_y as u32);
        let engine = LayoutEngine::new(config);
        engine
            .draw_pwnagotchi_frame(
                &mut buffer, width, height, 1, 0, "0s", "pwn", "", "", 0, 0, 0, 0, "AUTO",
                Some(("(♥‿‿♥)", "|||| buddy 2 (5)")),
            )
            .unwrap();

        assert!(
            region_has_pixels(&buffer, width, ff_x..ff_x + 16, ff_y..ff_y + 16),
            "expected the friend's own face bitmap near (friend_face_x={ff_x}, friend_face_y={ff_y})"
        );
    }

    #[test]
    fn test_draw_pwnagotchi_frame_face_renders_larger_than_base_cell() {
        // Regression test for the "face isn't even close to the original"
        // gap: the main face must render noticeably bigger than a single
        // base 16x16 glyph cell (real pwnagotchi uses a much larger font
        // for the face than the rest of the UI), not the same size as
        // every other field.
        let (width, height, mut buffer) = new_test_buffer();
        let config = LayoutConfig::default();
        assert!(config.face_scale >= 2, "face_scale should upscale the face, not render it 1:1");
        let (face_x, face_y, scale) = (config.face_x as u32, config.face_y as u32, config.face_scale as u32);
        let engine = LayoutEngine::new(config);
        engine
            .draw_pwnagotchi_frame(
                &mut buffer, width, height, 1, 0, "0s", "pwn", "", "(◕‿‿◕)", 0, 0, 0, 0, "AUTO",
                None,
            )
            .unwrap();

        // A 1x render would never place ink below `face_y + GLYPH_CELL_H`
        // (the base cell's own height). Parenthesis characters span the
        // full cell top-to-bottom, so a genuinely scaled render must put
        // ink somewhere in the extended region a 1x render couldn't reach.
        let base_cell_bottom = face_y + fonts::GLYPH_CELL_H;
        let scaled_cell_bottom = face_y + fonts::GLYPH_CELL_H * scale;
        assert!(
            region_has_pixels(&buffer, width, face_x..face_x + 20, base_cell_bottom..scaled_cell_bottom),
            "expected face ink below the base 16px cell height (in the {scale}x-scaled region), \
             found none -- face may still be rendering at 1x"
        );
    }

    #[test]
    fn test_wrap_status_text_splits_on_word_boundaries() {
        let wrapped = wrap_status_text("the quick brown fox jumps", 10);
        assert_eq!(wrapped, vec!["the quick", "brown fox", "jumps"]);
    }

    #[test]
    fn test_wrap_status_text_short_text_is_single_line() {
        assert_eq!(wrap_status_text("hi", 10), vec!["hi"]);
    }

    #[test]
    fn test_kaomoji_line_width_ignores_combining_marks() {
        // "•́" is bullet (U+2022) + combining acute (U+0301): width should
        // count only the bullet's cell. U+2022 is a narrow glyph in the
        // atlas (all ink in the left half of its cell -- GNU Unifont is
        // dual-width), so its advance is half `GLYPH_CELL_W`, not the
        // full cell (a full-cell assumption here would silently
        // reintroduce the double-spacing bug `glyph_advance_width` fixes).
        assert_eq!(
            kaomoji_line_width("\u{2022}\u{0301}", 1),
            fonts::GLYPH_CELL_W as i32 / 2
        );
    }

    #[test]
    fn test_kaomoji_line_width_sums_mixed_narrow_and_wide_glyphs() {
        // '(' and '_' are narrow (half-cell); U+2686 (⚆) is wide
        // (full-cell) -- confirmed directly from the atlas data. A face
        // like "(_⚆" should sum each glyph's own advance, not assume a
        // uniform cell width for all three.
        let half = fonts::GLYPH_CELL_W as i32 / 2;
        let full = fonts::GLYPH_CELL_W as i32;
        assert_eq!(kaomoji_line_width("(_\u{2686}", 1), half + half + full);
    }

    #[test]
    fn test_draw_kaomoji_line_advances_narrow_glyphs_by_half_cell() {
        // Regression test for the "face characters scattered across the
        // whole screen" bug: drawing two narrow glyphs back-to-back must
        // place the second glyph's ink starting at exactly one
        // half-cell-width away from the first, not a full cell.
        let (width, height) = (40u32, 16u32);
        let mut buffer = vec![0u8; (width as usize * height as usize).div_ceil(8)];
        let mut fb = FrameBuffer::new(&mut buffer, width, height);
        draw_kaomoji_line(&mut fb, "((", 0, 0, 1);
        let half = fonts::GLYPH_CELL_W / 2;
        assert!(
            region_has_pixels(&buffer, width, half..half + 8, 0..16),
            "expected second '(' glyph's ink starting at half-cell x={half}"
        );
    }
}
