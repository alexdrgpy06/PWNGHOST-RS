//! SSD1306 display driver

use anyhow::Result;
use embedded_graphics::prelude::*;
use ssd1306::{prelude::*, I2CDisplayInterface, Ssd1306};
use linux_embedded_hal::I2cdev;
use std::path::Path;
use tracing::{info, warn};

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
    SSD1306_128x64,
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

/// Display driver wrapping SSD1306
pub struct DisplayDriver {
    display: Option<Ssd1306<I2CInterface<I2cdev>, ssd1306::size::DisplaySize250x122, ssd1306::mode::BufferedGraphicsMode>>,
    config: DisplayConfig,
    awake: bool,
}

impl DisplayDriver {
    pub fn new(config: &DisplayConfig) -> Result<Self> {
        Ok(Self {
            display: None,
            config: config.clone(),
            awake: false,
        })
    }

    pub async fn init(&mut self) -> Result<()> {
        info!("Initializing display on {}", self.config.i2c_bus);

        let i2c = I2cdev::new(&self.config.i2c_bus)
            .map_err(|e| anyhow::anyhow!("Failed to open I2C bus: {}", e))?;

        let interface = I2CDisplayInterface::new(i2c);

        let mut display = Ssd1306::new(interface, ssd1306::size::DisplaySize250x122, ssd1306::rotation::DisplayRotation::Rotate180)
            .into_buffered_graphics_mode();

        display.init()
            .map_err(|e| anyhow::anyhow!("Display init failed: {}", e))?;

        self.display = Some(display);
        self.awake = true;

        info!("Display initialized successfully");
        Ok(())
    }

    pub async fn update(&mut self, buffer: &[u8], partial: bool) -> Result<()> {
        if let Some(display) = &mut self.display {
            // Convert buffer to display format and flush
            // This is simplified - real implementation would use embedded_graphics drawables
            display.flush()
                .map_err(|e| anyhow::anyhow!("Display flush failed: {}", e))?;
        }
        Ok(())
    }

    pub async fn sleep(&mut self) -> Result<()> {
        if let Some(display) = &mut self.display {
            // Send sleep command
            self.awake = false;
        }
        Ok(())
    }

    pub async fn wake(&mut self) -> Result<()> {
        if let Some(display) = &mut self.display {
            // Send wake command
            self.awake = true;
        }
        Ok(())
    }

    pub fn is_awake(&self) -> bool {
        self.awake
    }
}

type I2CInterface = ssd1306::interface::I2CInterface<I2cdev>;

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
}