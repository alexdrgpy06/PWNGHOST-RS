//! Layout engine for the display.
//!
//! Renders the pwnagotchi status frame into a 1-bit-per-pixel packed
//! framebuffer using `embedded-graphics` and its built-in ASCII fonts.

use crate::fonts;
use crate::ttf::{self, TtfStyle};
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
    /// Face font size in pixels. Real pwnagotchi's 2.13" V4 layout uses
    /// `fonts.setup(..., huge=35, ...)` -- DejaVuSansMono-Bold at 35 px --
    /// for the face, the dominant element on the screen. We render the same
    /// font at the same size via `crate::ttf`, so this is an exact match to
    /// the original rather than the old bitmap-upscale approximation.
    pub face_size: u32,
    pub friend_face_x: i32,
    pub friend_face_y: i32,
    /// Friend-face font size in pixels -- smaller than the main face,
    /// matching real pwnagotchi's much smaller `friend_face` treatment.
    pub friend_face_size: u32,
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
            face_size: 35,
            friend_face_x: 0,
            friend_face_y: 92,
            friend_face_size: 16,
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
        _level: u32,
        _xp: u32,
        mode: &str,
        friend: Option<(&str, &str)>,
    ) -> Result<()> {
        let mut fb = FrameBuffer::new(buffer, width, height);
        let line_style = PrimitiveStyle::with_stroke(BinaryColor::On, 1);

        // Channel / APs / uptime using DejaVu TTF (Bold label, Regular value)
        draw_labeled_value_ttf(
            &mut fb,
            "CH",
            &format!("{channel:02}"),
            self.config.channel_x,
            self.config.channel_y,
        );
        draw_labeled_value_ttf(
            &mut fb,
            "APS",
            &aps_count.to_string(),
            self.config.aps_x,
            self.config.aps_y,
        );
        draw_labeled_value_ttf(
            &mut fb,
            "UP",
            uptime,
            self.config.uptime_x,
            self.config.uptime_y,
        );

        // Divider between the top info row and the name/status/face section
        draw_horizontal_divider(&mut fb, width, self.config.line1_y, line_style);

        // Name and status using DejaVu TTF
        draw_ttf_line(
            &mut fb,
            &format!("{name}>"),
            self.config.name_x,
            self.config.name_y,
            9,
            TtfStyle::Bold,
        );
        for (i, line) in wrap_status_text(status, self.config.status_max_chars)
            .into_iter()
            .take(2)
            .enumerate()
        {
            draw_ttf_line(
                &mut fb,
                &line,
                self.config.status_x,
                self.config.status_y + i as i32 * 10,
                9,
                TtfStyle::Regular,
            );
        }

        // Face. Real pwnagotchi renders this as DejaVuSansMono-Bold at 35pt
        draw_ttf_line(
            &mut fb,
            face,
            self.config.face_x,
            self.config.face_y,
            self.config.face_size,
            TtfStyle::Bold,
        );

        // Closest mesh friend, if any
        if let Some((friend_face, friend_line)) = friend {
            draw_ttf_line(
                &mut fb,
                friend_face,
                self.config.friend_face_x,
                self.config.friend_face_y,
                self.config.friend_face_size,
                TtfStyle::Bold,
            );
            draw_ttf_line(
                &mut fb,
                friend_line,
                self.config.friend_x,
                self.config.friend_y,
                9,
                TtfStyle::Regular,
            );
        }

        // Divider between face/friend section and PWND/mode section
        draw_horizontal_divider(&mut fb, width, self.config.line2_y, line_style);

        // PWND: handshakes count formatted cleanly with DejaVu TTF
        draw_labeled_value_ttf(
            &mut fb,
            "PWND ",
            &format!("{handshakes} ({total_handshakes})"),
            self.config.shakes_x,
            self.config.shakes_y,
        );

        // Operating mode
        draw_ttf_line(
            &mut fb,
            mode,
            self.config.mode_x,
            self.config.mode_y,
            9,
            TtfStyle::Bold,
        );

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

    /// Draw a kaomoji face string centered horizontally, using the bundled
    /// DejaVuSansMono-Bold TTF at the face size (see `crate::ttf`).
    pub fn draw_face_centered(
        &self,
        buffer: &mut [u8],
        width: u32,
        height: u32,
        face: &str,
    ) -> Result<()> {
        let mut fb = FrameBuffer::new(buffer, width, height);
        let px = self.config.face_size;
        let text_w = ttf_line_width(face, px, TtfStyle::Bold);
        let x = (width as i32 - text_w) / 2;
        let y = (height as i32 - px as i32) / 2;
        draw_ttf_line(&mut fb, face, x, y, px, TtfStyle::Bold);
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
fn draw_labeled_value_ttf(
    fb: &mut FrameBuffer<'_>,
    label: &str,
    value: &str,
    x: i32,
    y: i32,
) {
    let label_w = draw_ttf_line(fb, label, x, y, 9, TtfStyle::Bold);
    draw_ttf_line(fb, value, x + label_w, y, 9, TtfStyle::Regular);
}

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
fn draw_horizontal_divider(
    fb: &mut FrameBuffer<'_>,
    width: u32,
    y: i32,
    style: PrimitiveStyle<BinaryColor>,
) {
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

/// Draw a face/text string using the bundled DejaVuSansMono TTF at `px`
/// pixels, matching real pwnagotchi's PIL/FreeType rendering (the face is
/// DejaVuSansMono-Bold @ 35 px). `(x, y)` is the top-left anchor, the same
/// convention real pwnagotchi's layout uses.
///
/// For each character we rasterize the outline via `crate::ttf` (smooth,
/// size-accurate) and threshold its coverage into the 1bpp framebuffer.
/// DejaVu covers every symbol our face set uses, so this is pixel-faithful
/// to the original. For any codepoint DejaVu lacks (rare -- e.g. a CJK
/// symbol), we fall back to the legacy Unifont bitmap atlas
/// (`fonts::kaomoji_glyph`), upscaled to approximately `px` so it doesn't
/// look tiny beside the TTF glyphs. Returns the total advanced width in px.
fn draw_ttf_line(
    fb: &mut FrameBuffer<'_>,
    text: &str,
    x: i32,
    y: i32,
    px: u32,
    style: TtfStyle,
) -> i32 {
    let ascent = ttf::line_ascent(style, px);
    let mut pen_x = x;
    for ch in text.chars() {
        match ttf::rasterize_glyph(style, ch, px) {
            Some(g) => {
                // Place the glyph bitmap: its left edge at pen+left, its top
                // edge at baseline-top where baseline = y + ascent.
                blit_coverage(
                    fb,
                    &g.coverage,
                    g.width,
                    g.height,
                    pen_x + g.left,
                    y + ascent - g.top,
                );
                pen_x += g.advance;
            }
            None => {
                // Unifont fallback for a codepoint DejaVu lacks. Upscale the
                // 16 px cell to ~px so it visually matches the TTF glyphs.
                let scale = ((px as i32 + 8) / 16).max(1);
                if let Some(bits) = fonts::kaomoji_glyph(ch) {
                    blit_glyph(fb, bits, pen_x, y, scale);
                }
                pen_x += ttf::advance_of(style, ch, px);
            }
        }
    }
    pen_x - x
}

/// Total pixel width [`draw_ttf_line`] would occupy for `text` at `px`.
fn ttf_line_width(text: &str, px: u32, style: TtfStyle) -> i32 {
    text.chars().map(|c| ttf::advance_of(style, c, px)).sum()
}

/// Threshold-blit a fontdue coverage bitmap (`width * height` bytes, one per
/// pixel, 0..=255) onto `fb` at top-left `(x, y)`. Coverage > 127 becomes
/// ink -- a simple 1bpp threshold, which is what the e-ink panel needs.
fn blit_coverage(
    fb: &mut FrameBuffer<'_>,
    coverage: &[u8],
    width: usize,
    height: usize,
    x: i32,
    y: i32,
) {
    for gy in 0..height {
        for gx in 0..width {
            if coverage[gy * width + gx] > 127 {
                fb.set_pixel(x + gx as i32, y + gy as i32, true);
            }
        }
    }
}

/// Blit a single pre-rasterized Unifont glyph cell onto `fb` at `(x, y)`
/// (top-left origin), each source pixel replicated as a `scale x scale`
/// block. Bits are packed MSB-first, padded to a byte boundary per row;
/// bit == 1 means "ink". Retained only as the missing-glyph fallback path
/// inside [`draw_ttf_line`].
fn blit_glyph(
    fb: &mut FrameBuffer<'_>,
    bits: &[u8; fonts::GLYPH_BYTES],
    x: i32,
    y: i32,
    scale: i32,
) {
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
                &mut buffer,
                width,
                height,
                6,
                3,
                "01:02:03",
                "pwn",
                "hello",
                "(◕‿‿◕)",
                1,
                5,
                2,
                150,
                "AUTO",
                None,
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
                None,
            )
            .unwrap();

        let row_coverage = |y: u32| {
            (0..width)
                .filter(|&x| pixel_on(&buffer, width, x, y))
                .count()
        };

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
            region_has_pixels(
                &buffer,
                width,
                friend_x..friend_x + 60,
                friend_y..friend_y + 10
            ),
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
            region_has_pixels(&buffer, width, ff_x..ff_x + 16, ff_y..ff_y + 16),
            "expected the friend's own face bitmap near (friend_face_x={ff_x}, friend_face_y={ff_y})"
        );
    }

    #[test]
    fn test_draw_pwnagotchi_frame_face_renders_at_ttf_size() {
        // Regression test for the "face isn't even close to the original"
        // gap: the face must render at the real pwnagotchi size (35 px
        // DejaVuSansMono-Bold), noticeably taller than the old 16 px base
        // cell -- so there must be face ink well below `face_y + 16`.
        let (width, height, mut buffer) = new_test_buffer();
        let config = LayoutConfig::default();
        assert_eq!(
            config.face_size, 35,
            "face should render at real pwnagotchi's 35px"
        );
        let (face_x, face_y, px) = (config.face_x as u32, config.face_y as u32, config.face_size);
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
                "(◕‿‿◕)",
                0,
                0,
                0,
                0,
                "AUTO",
                None,
            )
            .unwrap();

        // A 16px render would never place ink below `face_y + 16`. A 35px
        // TTF face reaches well beyond that. The parens span most of the
        // face height, so there must be ink in the lower part of the glyph.
        let old_cell_bottom = face_y + 16;
        let ttf_bottom = (face_y + px).min(height);
        assert!(
            region_has_pixels(
                &buffer,
                width,
                face_x..face_x + 30,
                old_cell_bottom..ttf_bottom
            ),
            "expected face ink below the old 16px cell (in the 35px TTF region), \
             found none -- face may still be rendering small"
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
    fn test_ttf_line_width_is_monospace_multiple() {
        // DejaVuSansMono is monospace: a line's TTF width is N glyphs times
        // the single-glyph advance. Every ASCII face char shares that advance.
        let px = 35u32;
        let single = ttf::advance_of(TtfStyle::Bold, '(', px);
        assert_eq!(ttf_line_width("(-_-)", px, TtfStyle::Bold), single * 5);
    }

    #[test]
    fn test_ttf_line_width_scales_with_size() {
        // A face rendered at the 35px face size is wider than at a small size.
        let big = ttf_line_width("(-_-)", 35, TtfStyle::Bold);
        let small = ttf_line_width("(-_-)", 12, TtfStyle::Bold);
        assert!(big > small);
    }

    #[test]
    fn test_draw_ttf_line_advances_left_to_right() {
        // Drawing a multi-char face must place later glyphs to the right of
        // earlier ones (monospace advance), producing a compact face across
        // roughly `chars * advance` pixels -- not scattered across the panel.
        let (width, height) = (250u32, 40u32);
        let mut buffer = vec![0u8; (width as usize * height as usize).div_ceil(8)];
        let mut fb = FrameBuffer::new(&mut buffer, width, height);
        let advanced = draw_ttf_line(&mut fb, "(-_-)", 0, 0, 35, TtfStyle::Bold);
        let expected = ttf_line_width("(-_-)", 35, TtfStyle::Bold);
        assert_eq!(advanced, expected);
        // The 5-glyph face at ~21px advance should span well under half the
        // 250px panel, i.e. it's compact, not spread across the screen.
        assert!(
            advanced < 130,
            "face should be compact, spanned {advanced}px"
        );
        assert!(
            advanced > 40,
            "face should have real width, spanned {advanced}px"
        );
    }
}
