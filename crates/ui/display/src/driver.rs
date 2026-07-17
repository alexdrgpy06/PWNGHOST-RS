//! Display driver.
//!
//! ## Which controller is this actually driving?
//!
//! The physical panel used by PWNGHOST is a **Waveshare e-Paper HAT V4**
//! (2.13"/2.7"/2.9", SPI). Despite the workspace's `Cargo.toml` historically
//! listing an `ssd1306` dependency, SSD1306 is the controller chip in
//! Waveshare's small *I2C OLED* HATs -- an unrelated product line. Waveshare's
//! own V4 e-Paper HAT specification sheets and wiki pages confirm the 2.13"
//! V4 panel uses the **SSD1680** controller over SPI, and Waveshare states
//! the V4 revision is "fully compatible with V3" at the protocol level (V4
//! just adds a faster-refresh demo/LUT on the same hardware interface). The
//! 2.7"/2.9" V4 panels are the same SSD1680-family SPI protocol at different
//! resolutions. None of this is an SSD1306 (I2C OLED) device.
//!
//! [`epd-waveshare`](https://docs.rs/epd-waveshare) is the standard, actively
//! maintained Rust driver for exactly these panels: its `epd2in13_v2` module
//! (built with the crate's default `epd2in13_v3` cargo feature, i.e. the V3
//! waveform tables) targets the V2/V3 hardware that V4 is protocol-compatible
//! with, and it ships matching `epd2in7`/`epd2in9_v2` modules for the other
//! two sizes SPEC.md calls out. This crate now depends on `epd-waveshare`
//! (behind the `hardware` feature) for the e-ink path; see `hardware.rs`.
//!
//! `ssd1306` is *not* removed: `DisplayType::Ssd1306_128x64` was already a
//! selectable config variant (`ui.display.display_type = "ssd1306"`) for
//! anyone who's actually wired up a small I2C OLED HAT instead of the
//! e-ink panel, and nothing else in the workspace depended on the
//! previously-unused `ssd1306` dependency declared in the root `Cargo.toml`
//! (it wasn't even referenced by this crate before now). It's kept, wired
//! for real over I2C, and made `optional`/feature-gated like everything else
//! hardware-related.
//!
//! ## Portability
//!
//! To keep the crate portable and testable off-Pi, the driver keeps a
//! software copy of the framebuffer and logs its operations by default. The
//! real SPI/GPIO bus writes live behind the `hardware` Cargo feature (see
//! `hardware.rs`), matching the `linux-gpio` feature convention used by
//! `crates/fw-patcher`. Building without `--features hardware` (e.g. on this
//! Windows dev machine, or in CI) never touches Linux-only APIs.

use anyhow::Result;
use tracing::info;

#[cfg(feature = "hardware")]
use crate::hardware;

/// Display configuration
#[derive(Debug, Clone)]
pub struct DisplayConfig {
    pub width: u32,
    pub height: u32,
    pub i2c_address: u8,
    pub i2c_bus: String,
    pub rotation: DisplayRotation,
    pub display_type: DisplayType,

    /// SPI device node for the Waveshare e-ink panel (ignored for
    /// `Ssd1306_128x64`, which uses `i2c_bus`/`i2c_address` instead).
    pub spi_path: String,
    /// SPI clock in Hz. Waveshare's reference driver code uses 4 MHz.
    pub spi_hz: u32,
    /// GPIO character-device chip, e.g. `/dev/gpiochip0` (BCM GPIOs on a Pi
    /// Zero W/2W are on gpiochip0).
    pub gpio_chip: String,
    /// BCM pin number for the panel's BUSY line. Waveshare's standard
    /// e-Paper HAT wiring uses GPIO 24.
    pub pin_busy: u32,
    /// BCM pin number for the panel's DC (data/command) line. Standard
    /// wiring uses GPIO 25.
    pub pin_dc: u32,
    /// BCM pin number for the panel's RST line. Standard wiring uses
    /// GPIO 17. (CS is not listed here: it's asserted automatically by the
    /// SPI controller via the `spi_path` chip-select, e.g.
    /// `/dev/spidev0.0` == CE0, matching Waveshare's standard HAT wiring.)
    pub pin_rst: u32,
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
            spi_path: "/dev/spidev0.0".to_string(),
            spi_hz: 4_000_000,
            gpio_chip: "/dev/gpiochip0".to_string(),
            pin_busy: 24,
            pin_dc: 25,
            pin_rst: 17,
        }
    }
}

/// Which concrete panel driver to dispatch to. The `DisplayType`/config
/// string alone doesn't encode physical panel size (the config schema only
/// exposes `"waveshare_v4"` / `"waveshare_v3"` / `"ssd1306"` -- see
/// `crates/config/src/schema.rs`'s `DisplayUiConfig`), so size is
/// disambiguated from the configured width/height against the three known
/// Waveshare V4 panel resolutions SPEC.md calls out (2.13"/2.7"/2.9").
// Only reachable from `hardware.rs` (behind the `hardware` feature); the
// pure helpers below are exercised directly by unit tests so they stay
// testable without that feature, which makes them look dead to a plain
// `cargo clippy`/`cargo check` (no features, no test cfg).
#[cfg_attr(not(feature = "hardware"), allow(dead_code))]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PanelKind {
    /// 122x250, SSD1680 (epd-waveshare's `epd2in13_v2` module, built against
    /// its default `epd2in13_v3` waveform feature -- see module docs above).
    Epd2in13,
    /// 128x296, SSD1680-family (epd-waveshare's `epd2in9_v2` module).
    Epd2in9,
    /// Small I2C OLED HAT (not a Waveshare e-ink panel).
    Ssd1306_128x64,
}

#[cfg_attr(not(feature = "hardware"), allow(dead_code))]
impl PanelKind {
    /// Canonical (width, height) for the concrete controller, as declared
    /// by the upstream driver crate (`Epd2in13::WIDTH/HEIGHT`, etc.).
    pub(crate) fn dimensions(self) -> (u32, u32) {
        match self {
            PanelKind::Epd2in13 => (122, 250),
            PanelKind::Epd2in9 => (128, 296),
            PanelKind::Ssd1306_128x64 => (128, 64),
        }
    }

    pub(crate) fn resolve(display_type: DisplayType, width: u32, height: u32) -> Self {
        if display_type == DisplayType::Ssd1306_128x64 {
            return PanelKind::Ssd1306_128x64;
        }
        // Match ignoring orientation (rotation may swap width/height).
        let (long, short) = (width.max(height), width.min(height));
        match (long, short) {
            (296, 128) => PanelKind::Epd2in9,
            // 2.7" (264x176) isn't supported here: the published
            // epd-waveshare 0.6.0 (the pinned version) has no plain
            // monochrome epd2in7 module, only the bicolor epd2in7b variant
            // (confirmed by inspecting the actual published crate source --
            // a plain epd2in7 module exists on the upstream repo's git HEAD
            // but was never released to crates.io). Falls back to 2.13"
            // rather than silently misdrawing on a 2.7" panel.
            // 250x122 (the crate's own default) and anything else
            // unrecognized also fall back to the 2.13" panel, which is also
            // the size used in PLAN.md's hardware test matrix.
            _ => PanelKind::Epd2in13,
        }
    }
}

/// Convert this crate's internal packed framebuffer representation (as
/// produced by `crate::layout`: row-major, LSB-first per byte, no row
/// padding, bit == 1 meaning "pixel on"/ink) into the row-padded,
/// MSB-first-per-byte, byte-boundary-per-row format e-ink controllers in
/// the SSD1680 family (and `epd-waveshare`'s `update_frame`/
/// `update_and_display_frame`) expect, with polarity flipped to match
/// `epd_waveshare::color::Color`'s convention (bit == 1 => White/background,
/// bit == 0 => Black/ink; see `epd_waveshare::color::Color::bitmask`).
///
/// This is a pure function so it's testable without the `hardware` feature
/// or any real panel attached -- see the unit tests below. The actual wire
/// format (bit order polarity, LUT/refresh semantics) still needs
/// confirmation against a real panel; see `hardware.rs` for what remains
/// unverified without physical hardware.
#[cfg_attr(not(feature = "hardware"), allow(dead_code))]
pub(crate) fn repack_for_panel(src: &[u8], width: u32, height: u32) -> Vec<u8> {
    let width = width as usize;
    let height = height as usize;
    let row_bytes = width.div_ceil(8);
    // Start all-white (0xFF), then clear bits where our source has ink.
    let mut dst = vec![0xFFu8; row_bytes * height];
    for y in 0..height {
        for x in 0..width {
            let src_index = y * width + x;
            let src_byte = src_index / 8;
            let src_bit = src_index % 8;
            if src_byte >= src.len() {
                continue;
            }
            let on = (src[src_byte] >> src_bit) & 1 != 0;
            if on {
                let dst_byte = y * row_bytes + x / 8;
                let mask = 0x80u8 >> (x % 8);
                dst[dst_byte] &= !mask;
            }
        }
    }
    dst
}

/// Number of bytes needed for a `width` x `height` 1bpp packed framebuffer
/// using this crate's internal (unpadded, flat-bitstream) convention.
pub(crate) fn packed_frame_bytes(width: u32, height: u32) -> usize {
    ((width as u64 * height as u64).div_ceil(8)) as usize
}

/// Software fallback backend: stores the framebuffer and logs, doing no
/// real I/O. Used whenever the `hardware` feature is disabled, and also as
/// the state kept alongside the real backend so `last_frame()` keeps
/// working identically either way.
struct SoftBackend {
    last_frame: Vec<u8>,
}

enum Backend {
    Soft(SoftBackend),
    // Boxed: HardwarePanel is >1KB (holds a full EinkPanel/OledDisplay),
    // dwarfing SoftBackend and tripping clippy::large_enum_variant.
    #[cfg(feature = "hardware")]
    Hardware(Box<hardware::HardwarePanel>),
}

/// Display driver holding the current framebuffer state.
pub struct DisplayDriver {
    config: DisplayConfig,
    awake: bool,
    initialized: bool,
    backend: Backend,
}

impl DisplayDriver {
    pub fn new(config: &DisplayConfig) -> Result<Self> {
        let bytes = packed_frame_bytes(config.width, config.height);
        Ok(Self {
            config: config.clone(),
            awake: false,
            initialized: false,
            backend: Backend::Soft(SoftBackend {
                last_frame: vec![0; bytes],
            }),
        })
    }

    pub async fn init(&mut self) -> Result<()> {
        info!(
            "Initializing {:?} display ({}x{}) on {}",
            self.config.display_type, self.config.width, self.config.height, self.config.i2c_bus
        );

        #[cfg(feature = "hardware")]
        {
            let panel_kind = PanelKind::resolve(
                self.config.display_type,
                self.config.width,
                self.config.height,
            );
            match hardware::HardwarePanel::open(&self.config, panel_kind) {
                Ok(panel) => {
                    self.backend = Backend::Hardware(Box::new(panel));
                }
                Err(err) => {
                    // Don't silently fall back to the no-op backend: on a
                    // real device this is the entire point of building with
                    // `--features hardware`, so surface the error instead
                    // of pretending the panel initialized.
                    return Err(err.context("failed to initialize hardware display backend"));
                }
            }
        }

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

        match &mut self.backend {
            Backend::Soft(soft) => {
                soft.last_frame.clear();
                soft.last_frame.extend_from_slice(buffer);
            }
            #[cfg(feature = "hardware")]
            Backend::Hardware(panel) => {
                panel.push_frame(buffer, self.config.width, self.config.height, partial)?;
            }
        }
        Ok(())
    }

    pub async fn sleep(&mut self) -> Result<()> {
        self.awake = false;
        info!("Display asleep");
        #[cfg(feature = "hardware")]
        if let Backend::Hardware(panel) = &mut self.backend {
            panel.sleep()?;
        }
        Ok(())
    }

    pub async fn wake(&mut self) -> Result<()> {
        self.awake = true;
        info!("Display awake");
        #[cfg(feature = "hardware")]
        if let Backend::Hardware(panel) = &mut self.backend {
            panel.wake()?;
        }
        Ok(())
    }

    pub fn is_awake(&self) -> bool {
        self.awake
    }

    /// The most recent framebuffer that was flushed to the panel. Only
    /// tracked by the software backend (the hardware backend streams
    /// straight to the panel and doesn't keep a host-side copy); returns
    /// an empty slice when running against real hardware.
    pub fn last_frame(&self) -> &[u8] {
        match &self.backend {
            Backend::Soft(soft) => &soft.last_frame,
            #[cfg(feature = "hardware")]
            Backend::Hardware(_) => &[],
        }
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
        assert_eq!(config.pin_busy, 24);
        assert_eq!(config.pin_dc, 25);
        assert_eq!(config.pin_rst, 17);
    }

    // With the `hardware` feature, init() deliberately does not fall back to
    // the no-op backend on failure (see its doc comment) -- it always tries
    // real SPI/GPIO I/O, which doesn't exist in a plain test/CI environment,
    // so `.unwrap()` below would always panic there. This test exercises
    // the soft-backend integration and is only meaningful without that
    // feature; the real hardware path needs an actual panel to verify.
    #[cfg(not(feature = "hardware"))]
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

    #[test]
    fn test_panel_kind_resolve_matches_known_sizes() {
        assert_eq!(
            PanelKind::resolve(DisplayType::WaveshareV4, 250, 122),
            PanelKind::Epd2in13
        );
        assert_eq!(
            PanelKind::resolve(DisplayType::WaveshareV4, 122, 250),
            PanelKind::Epd2in13
        );
        assert_eq!(
            PanelKind::resolve(DisplayType::WaveshareV4, 264, 176),
            PanelKind::Epd2in13,
            "2.7\" isn't supported (no plain mono epd2in7 module in the pinned epd-waveshare 0.6.0), falls back to 2.13\""
        );
        assert_eq!(
            PanelKind::resolve(DisplayType::WaveshareV4, 296, 128),
            PanelKind::Epd2in9
        );
        assert_eq!(
            PanelKind::resolve(DisplayType::Ssd1306_128x64, 128, 64),
            PanelKind::Ssd1306_128x64
        );
    }

    #[test]
    fn test_panel_kind_dimensions_roundtrip() {
        for kind in [
            PanelKind::Epd2in13,
            PanelKind::Epd2in9,
            PanelKind::Ssd1306_128x64,
        ] {
            let (w, h) = kind.dimensions();
            assert!(w > 0 && h > 0);
        }
    }

    #[test]
    fn test_repack_for_panel_all_white_by_default() {
        // A 9x3 all-off source (9 columns, so row padding actually kicks in:
        // 2 bytes/row instead of a suspiciously-round 8) should repack to
        // all-0xFF (all white/background).
        let width = 9u32;
        let height = 3u32;
        let src = vec![0u8; packed_frame_bytes(width, height)];
        let dst = repack_for_panel(&src, width, height);
        assert_eq!(dst.len(), 2 * 3); // row_bytes=ceil(9/8)=2, 3 rows
        assert!(dst.iter().all(|&b| b == 0xFF));
    }

    #[test]
    fn test_repack_for_panel_sets_black_bit_msb_first() {
        // Turn on pixel (0,0) in our internal LSB-first flat convention,
        // then confirm the repacked buffer clears the MSB of byte 0 (x=0 is
        // the leftmost pixel, which is bit 7 per
        // `epd_waveshare::color::Color::bitmask`'s `0x80 >> (pos % 8)`).
        let width = 16u32;
        let height = 1u32;
        let mut src = vec![0u8; packed_frame_bytes(width, height)];
        src[0] |= 1; // index 0 = (x=0,y=0)
        let dst = repack_for_panel(&src, width, height);
        assert_eq!(
            dst[0], 0x7F,
            "expected only MSB cleared (black), got {:#04x}",
            dst[0]
        );
        assert_eq!(dst[1], 0xFF);
    }

    #[test]
    fn test_repack_for_panel_row_padding() {
        // width=9 means each row needs 2 bytes (9 bits, padded to 16), even
        // though our internal flat format has no row boundaries at all.
        let width = 9u32;
        let height = 2u32;
        let mut src = vec![0u8; packed_frame_bytes(width, height)];
        // Turn on the last pixel of row 0 (x=8, y=0) -> flat index 8.
        src[1] |= 1; // index 8 -> byte 1, bit 0
        let dst = repack_for_panel(&src, width, height);
        assert_eq!(dst.len(), 4); // 2 bytes/row * 2 rows
                                  // x=8 is bit 0 of the *second* byte in the row (0x80 >> (8%8) == 0x80).
        assert_eq!(dst[0], 0xFF); // x=0..7 all white
        assert_eq!(dst[1], 0x7F); // x=8 (bit 7 of row-byte 1) is black
    }
}
