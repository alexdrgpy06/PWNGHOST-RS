//! E-ink display driver for PWNGHOST-RS

pub mod driver;
pub mod fonts;
#[cfg(feature = "hardware")]
mod hardware;
mod kaomoji_font_data;
pub mod layout;
pub mod ttf;

pub use driver::{DisplayConfig, DisplayDriver, DisplayRotation, DisplayType};
pub use fonts::FontRegistry;
pub use layout::{LayoutConfig, LayoutEngine};

use anyhow::Result;
use pwncore::Mood;
use tokio::sync::Mutex;

/// High-level display abstraction
pub struct Display {
    driver: Mutex<DisplayDriver>,
    layout: LayoutEngine,
    framebuffer: Mutex<Vec<u8>>,
    width: u32,
    height: u32,
}

impl Display {
    /// Create new display
    pub fn new(config: DisplayConfig) -> Result<Self> {
        let driver = DisplayDriver::new(&config)?;
        let (width, height) = (config.width, config.height);
        let layout = LayoutEngine::new(LayoutConfig::default());

        Ok(Self {
            driver: Mutex::new(driver),
            layout,
            framebuffer: Mutex::new(vec![0; driver::packed_frame_bytes(width, height)]),
            width,
            height,
        })
    }

    /// Initialize display hardware
    pub async fn init(&self) -> Result<()> {
        let mut driver = self.driver.lock().await;
        driver.init().await
    }

    /// Clear display buffer
    pub async fn clear(&self) -> Result<()> {
        let mut fb = self.framebuffer.lock().await;
        fb.fill(0);
        Ok(())
    }

    /// Draw pwnagotchi frame
    #[allow(clippy::too_many_arguments)]
    pub async fn draw_pwnagotchi_frame(
        &self,
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
        let mut fb = self.framebuffer.lock().await;
        fb.fill(0);

        // Draw using layout engine
        self.layout.draw_pwnagotchi_frame(
            &mut fb,
            self.width,
            self.height,
            channel,
            aps_count,
            uptime,
            name,
            status,
            face,
            handshakes,
            total_handshakes,
            level,
            xp,
            mode,
            friend,
        )?;

        Ok(())
    }

    /// Update physical display
    pub async fn update(&self, partial: bool) -> Result<()> {
        let fb = self.framebuffer.lock().await;
        let mut driver = self.driver.lock().await;
        driver.update(&fb, partial).await
    }

    /// Force full refresh
    pub async fn force_refresh(&self) -> Result<()> {
        self.update(false).await
    }

    /// Show shutdown screen
    pub async fn show_shutdown(&self) -> Result<()> {
        let mut fb = self.framebuffer.lock().await;
        fb.fill(0);

        // Draw shutdown face
        let face = "(⌐■_■)";
        self.layout
            .draw_face_centered(&mut fb, self.width, self.height, face)?;

        self.force_refresh().await
    }

    /// Put display to sleep
    pub async fn sleep(&self) -> Result<()> {
        let mut driver = self.driver.lock().await;
        driver.sleep().await
    }

    /// Wake display
    pub async fn wake(&self) -> Result<()> {
        let mut driver = self.driver.lock().await;
        driver.wake().await
    }

    /// Get display dimensions
    pub fn dimensions(&self) -> (u32, u32) {
        (self.width, self.height)
    }

    /// Encode the current framebuffer as a PNG (grayscale, ink = black on
    /// white), for the web UI's live display view -- real pwnagotchi serves
    /// exactly this: the same rendered frame the panel shows, as a PNG at
    /// `/ui` (see `pwnagotchi/ui/web/handler.py::ui()`). The internal
    /// framebuffer is 1bpp packed (bit == 1 => ink); we expand it to an
    /// 8-bit grayscale image so any browser can display it without a custom
    /// decoder. At 250x122 this is a few KB and encodes in well under a
    /// millisecond, cheap enough to regenerate on every 1s display tick.
    pub async fn frame_png(&self) -> Result<Vec<u8>> {
        let fb = self.framebuffer.lock().await;
        let (w, h) = (self.width, self.height);
        let mut gray = vec![0xFFu8; (w as usize) * (h as usize)];
        for y in 0..h {
            for x in 0..w {
                let idx = (y as usize) * (w as usize) + (x as usize);
                let byte = idx / 8;
                let bit = idx % 8;
                if byte < fb.len() && (fb[byte] >> bit) & 1 != 0 {
                    gray[idx] = 0x00; // ink
                }
            }
        }
        let mut out = Vec::new();
        {
            let mut encoder = png::Encoder::new(&mut out, w, h);
            encoder.set_color(png::ColorType::Grayscale);
            encoder.set_depth(png::BitDepth::Eight);
            let mut writer = encoder
                .write_header()
                .map_err(|e| anyhow::anyhow!("png header: {e}"))?;
            writer
                .write_image_data(&gray)
                .map_err(|e| anyhow::anyhow!("png data: {e}"))?;
        }
        Ok(out)
    }
}

/// Get face for mood. Delegates to the canonical table in
/// [`pwncore::Mood::face`] (single source of truth for faces).
pub fn face_for_mood(mood: Mood) -> &'static str {
    mood.face()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_face_for_mood() {
        // face() picks randomly among a mood's real variants -- check
        // membership, not a single fixed value.
        assert!(Mood::Happy.face_variants().contains(&face_for_mood(Mood::Happy)));
        assert!(Mood::Sleep.face_variants().contains(&face_for_mood(Mood::Sleep)));
        assert!(Mood::Angry.face_variants().contains(&face_for_mood(Mood::Angry)));
    }
}
