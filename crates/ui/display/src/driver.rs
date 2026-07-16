//! Display driver.
//!
//! The physical panel used by PWNGHOST is a Waveshare e-ink / SSD1306-class
//! module driven over I2C/SPI, which is only present on the Raspberry Pi. To
//! keep the crate portable and testable, the driver keeps a software copy of
//! the framebuffer and logs its operations; the actual bus writes are a thin
//! layer that can be added behind a hardware feature on-device.

use anyhow::Result;
use tracing::info;

/// Display configuration
#[derive(Debug, Clone)]
pub struct DisplayConfig {
    pub width: u32,
    pub height: u32,
    pub i2c_address: u8,
    pub i2c_bus: String,
    pub rotation: DisplayRotation,
    pub display_type: DisplayType,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DisplayRotation {
    Rotate0,
    Rotate90,
    Rotate180,
    Rotate270,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DisplayType {
    WaveshareV4,
    WaveshareV3,
    Ssd1306_128x64,
}

impl DisplayType {
    /// Parse a display type from its config string (falls back to WaveshareV4).
    pub fn from_config_str(s: &str) -> Self {
        match s.to_lowercase().as_str() {
            "waveshare_v3" => DisplayType::WaveshareV3,
            "ssd1306" | "ssd1306_128x64" => DisplayType::Ssd1306_128x64,
            _ => DisplayType::WaveshareV4,
        }
    }
}

impl Default for DisplayConfig {
    fn default() -> Self {
        Self {
            width: 250,
            height: 122,
            i2c_address: 0x3C,
            i2c_bus: "/dev/i2c-1".to_string(),
            rotation: DisplayRotation::Rotate180,
            display_type: DisplayType::WaveshareV4,
        }
    }
}

/// Display driver holding the current framebuffer state.
pub struct DisplayDriver {
    config: DisplayConfig,
    awake: bool,
    initialized: bool,
    /// Last framebuffer pushed to the panel (1bpp packed).
    last_frame: Vec<u8>,
}

impl DisplayDriver {
    pub fn new(config: &DisplayConfig) -> Result<Self> {
        let bytes = (config.width * config.height / 8) as usize;
        Ok(Self {
            config: config.clone(),
            awake: false,
            initialized: false,
            last_frame: vec![0; bytes],
        })
    }

    pub async fn init(&mut self) -> Result<()> {
        info!(
            "Initializing {:?} display ({}x{}) on {}",
            self.config.display_type, self.config.width, self.config.height, self.config.i2c_bus
        );
        self.initialized = true;
        self.awake = true;
        Ok(())
    }

    pub async fn update(&mut self, buffer: &[u8], partial: bool) -> Result<()> {
        if !self.initialized {
            anyhow::bail!("Display not initialized");
        }
        info!(
            "Flushing {} bytes to display (partial={})",
            buffer.len(),
            partial
        );
        // Store the frame; on-device this is where the I2C/SPI flush happens.
        self.last_frame.clear();
        self.last_frame.extend_from_slice(buffer);
        Ok(())
    }

    pub async fn sleep(&mut self) -> Result<()> {
        self.awake = false;
        info!("Display asleep");
        Ok(())
    }

    pub async fn wake(&mut self) -> Result<()> {
        self.awake = true;
        info!("Display awake");
        Ok(())
    }

    pub fn is_awake(&self) -> bool {
        self.awake
    }

    /// The most recent framebuffer that was flushed to the panel.
    pub fn last_frame(&self) -> &[u8] {
        &self.last_frame
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_display_config_default() {
        let config = DisplayConfig::default();
        assert_eq!(config.width, 250);
        assert_eq!(config.height, 122);
        assert_eq!(config.i2c_address, 0x3C);
    }

    #[tokio::test]
    async fn test_update_requires_init() {
        let config = DisplayConfig::default();
        let mut driver = DisplayDriver::new(&config).unwrap();
        assert!(driver.update(&[0u8; 10], false).await.is_err());
        driver.init().await.unwrap();
        assert!(driver.update(&[0u8; 10], false).await.is_ok());
    }

    #[test]
    fn test_display_type_from_str() {
        assert_eq!(
            DisplayType::from_config_str("waveshare_v4"),
            DisplayType::WaveshareV4
        );
        assert_eq!(
            DisplayType::from_config_str("waveshare_v3"),
            DisplayType::WaveshareV3
        );
        assert_eq!(
            DisplayType::from_config_str("ssd1306"),
            DisplayType::Ssd1306_128x64
        );
    }
}
