//! Layout engine for the display.
//!
//! Renders the pwnagotchi status frame into a 1-bit-per-pixel packed
//! framebuffer using `embedded-graphics` and its built-in ASCII fonts.

use anyhow::Result;
use embedded_graphics::{
    mono_font::{ascii::FONT_6X10, ascii::FONT_9X15, MonoTextStyle},
    pixelcolor::BinaryColor,
    prelude::*,
    text::{Alignment, Text},
};

/// Layout configuration
#[derive(Debug, Clone)]
pub struct LayoutConfig {
    pub face_x: i32,
    pub face_y: i32,
    pub status_x: i32,
    pub status_y: i32,
    pub info_x: i32,
    pub info_y: i32,
    pub font_size: u32,
}

impl Default for LayoutConfig {
    fn default() -> Self {
        Self {
            face_x: 0,
            face_y: 16,
            status_x: 125,
            status_y: 20,
            info_x: 0,
            info_y: 85,
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

    /// Draw a complete pwnagotchi frame into `buffer`.
    #[allow(clippy::too_many_arguments)]
    pub fn draw_pwnagotchi_frame(
        &self,
        buffer: &mut [u8],
        width: u32,
        height: u32,
        channel: u8,
        aps_count: usize,
        bt_connected: bool,
        uptime: &str,
        name: &str,
        phrase: &str,
        face: &str,
        handshakes: u32,
        level: u32,
        mode: &str,
        cpu_temp: Option<f32>,
        ram_used: u64,
        ram_total: u64,
    ) -> Result<()> {
        let mut fb = FrameBuffer::new(buffer, width, height);
        let small = MonoTextStyle::new(&FONT_6X10, BinaryColor::On);
        let big = MonoTextStyle::new(&FONT_9X15, BinaryColor::On);

        // Top status bar: name + channel + APs + BT.
        let bt = if bt_connected { "BT" } else { "--" };
        let status = format!("{name} CH{channel} AP{aps_count} {bt}");
        draw_line(&mut fb, &status, &small, self.config.status_x.min(0), 8)?;

        // Face and phrase in the middle.
        draw_line(
            &mut fb,
            face,
            &big,
            self.config.face_x,
            self.config.face_y + 24,
        )?;
        draw_line(
            &mut fb,
            phrase,
            &small,
            self.config.face_x,
            self.config.face_y + 44,
        )?;

        // Info bar: uptime, level/xp, handshakes, mode.
        let info = format!("UP {uptime}  L{level}  HS{handshakes}  {mode}");
        draw_line(
            &mut fb,
            &info,
            &small,
            self.config.info_x,
            self.config.info_y,
        )?;

        // Resource footer.
        let temp = cpu_temp
            .map(|t| format!("{t:.0}C"))
            .unwrap_or_else(|| "--".to_string());
        let footer = format!("T{temp} RAM {ram_used}/{ram_total}MB");
        draw_line(
            &mut fb,
            &footer,
            &small,
            self.config.info_x,
            self.config.info_y + 12,
        )?;

        Ok(())
    }

    /// Draw a single line of text centered horizontally.
    pub fn draw_text_centered(
        &self,
        buffer: &mut [u8],
        width: u32,
        height: u32,
        text: &str,
    ) -> Result<()> {
        let mut fb = FrameBuffer::new(buffer, width, height);
        let style = MonoTextStyle::new(&FONT_9X15, BinaryColor::On);
        Text::with_alignment(
            text,
            Point::new(width as i32 / 2, height as i32 / 2),
            style,
            Alignment::Center,
        )
        .draw(&mut fb)
        .ok();
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
    Text::new(text, Point::new(x, y), *style).draw(fb).ok();
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_layout_config_default() {
        let config = LayoutConfig::default();
        assert_eq!(config.face_x, 0);
        assert_eq!(config.face_y, 16);
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
}
