//! E-ink display driver for PWNGHOST-RS

pub mod driver;
pub mod layout;
pub mod fonts;

pub use driver::{DisplayDriver, DisplayConfig};
pub use layout::{LayoutEngine, LayoutConfig};
pub use fonts::{FontManager, FontConfig};

use anyhow::Result;
use pwncore::Mood;
use std::path::Path;
use tokio::sync::Mutex;
use tracing::{info, warn};

/// High-level display abstraction
pub struct Display {
    driver: Mutex<DisplayDriver>,
    layout: LayoutEngine,
    framebuffer: Mutex<Vec<u8>>,
    width: u32,
    height: u32,
    config: DisplayConfig,
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
            framebuffer: Mutex::new(vec![0; (width * height / 8) as usize]),
            width,
            height,
            config,
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
    pub async fn draw_pwnagotchi_frame(
        &self,
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
        let mut fb = self.framebuffer.lock().await;
        fb.fill(0);

        // Draw using layout engine
        self.layout.draw_pwnagotchi_frame(
            &mut fb,
            self.width,
            self.height,
            channel,
            aps_count,
            bt_connected,
            uptime,
            name,
            phrase,
            face,
            handshakes,
            level,
            mode,
            cpu_temp,
            ram_used,
            ram_total,
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
        self.layout.draw_text_centered(&mut fb, self.width, self.height, face)?;

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
}

/// Get face for mood
pub fn face_for_mood(mood: Mood) -> &'static str {
    match mood {
        Mood::LookR => "( ⚆_⚆)",
        Mood::LookL => "(☉_☉ )",
        Mood::LookRHappy => "( ◕‿◕)",
        Mood::LookLHappy => "(◕‿◕ )",
        Mood::Sleep => "(⇀‿‿↼)",
        Mood::Awake => "(◕‿‿◕)",
        Mood::Bored => "(-__-)",
        Mood::Intense => "(°▃▃°)",
        Mood::Cool => "(⌐■_■)",
        Mood::Happy => "(•‿‿•)",
        Mood::Excited => "(ᵔ◡◡ᵔ)",
        Mood::Grateful => "(^‿‿^)",
        Mood::Motivated => "(☼‿‿☼)",
        Mood::Demotivated => "(≖__≖)",
        Mood::Smart => "(✜‿‿✜)",
        Mood::Lonely => "(ب__ب)",
        Mood::Sad => "(╥☁╥ )",
        Mood::Angry => "(-_-')",
        Mood::Friend => "(♥‿‿♥)",
        Mood::Broken => "(☓‿‿☓)",
        Mood::Upload => "(1__0)",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_face_for_mood() {
        assert_eq!(face_for_mood(Mood::Happy), "(•‿‿•)");
        assert_eq!(face_for_mood(Mood::Sleep), "(⇀‿‿↼)");
        assert_eq!(face_for_mood(Mood::Angry), "(-_-')");
    }
}