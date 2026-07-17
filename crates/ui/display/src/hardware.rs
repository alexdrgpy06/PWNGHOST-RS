//! Real SPI/GPIO/I2C hardware I/O for the display, compiled only when the
//! `hardware` Cargo feature is enabled (mirrors `fw-patcher`'s `linux-gpio`
//! feature convention).
//!
//! Two independent backends live here:
//! - **E-ink** (`DisplayType::WaveshareV4`/`WaveshareV3`): driven via
//!   [`epd-waveshare`](https://docs.rs/epd-waveshare) over SPI + 3 GPIO
//!   lines (BUSY/DC/RST; CS is handled by the SPI controller itself). See
//!   `driver.rs`'s module docs for why `epd-waveshare` (not `ssd1306`) is
//!   the correct driver for these panels.
//! - **OLED** (`DisplayType::Ssd1306_128x64`): driven via
//!   [`ssd1306`](https://docs.rs/ssd1306) over I2C, for anyone who's wired
//!   up that small OLED HAT instead of the e-ink panel.
//!
//! Both backends use [`linux-embedded-hal`](https://docs.rs/linux-embedded-hal)
//! for the actual Linux `/dev/spidev*` + `/dev/gpiochip*` (character-device
//! GPIO, not the deprecated sysfs interface) + `/dev/i2c-*` plumbing.
//!
//! ## Verification status (read before trusting this on real hardware)
//!
//! This module was written against the documented APIs of `epd-waveshare`
//! 0.6, `linux-embedded-hal` 0.4, and `ssd1306` 0.10 (constructor
//! signatures, module layout, and the SSD1680 pixel-packing convention were
//! all confirmed against upstream source/docs during development). It could
//! **not** be compiled here: this is a Windows dev machine with no ARM
//! hardware, and `linux-embedded-hal`'s spidev/gpio-cdev backends are
//! Linux-only, so they don't even type-check on a non-Linux host. Treat
//! this as structurally-correct-by-research, not hardware-verified. Before
//! trusting it on a real panel, at minimum:
//! - `cargo check -p ui-display --features hardware` on the actual Pi
//!   target (or any Linux host) to catch any signature drift against the
//!   pinned crate versions.
//! - Confirm the BUSY/DC/RST BCM pin numbers and the SPI device path match
//!   the physical wiring (defaults here follow Waveshare's standard e-Paper
//!   HAT pinout and reference driver code: BUSY=24, DC=25, RST=17,
//!   `/dev/spidev0.0` for CE0).
//! - Confirm on-panel that `repack_for_panel`'s bit polarity (bit=1=white)
//!   and MSB-first/row-padded layout actually produces a correct image
//!   rather than a mirrored/inverted one -- this was derived from reading
//!   `epd_waveshare::color::Color::bitmask`'s source, not from a frame
//!   captured off real silicon.
//! - Confirm `DisplayRotation` (this crate's) is applied correctly; today
//!   this module does not rotate the buffer itself; `PanelKind::dimensions`
//!   picks the panel's native (non-rotated) orientation, and callers are
//!   expected to have already rendered into that orientation via
//!   `crate::layout`. True 90/270 rotation support (swapping which axis is
//!   the "long" one) is not implemented here.

use crate::driver::{repack_for_panel, DisplayConfig, PanelKind};
use anyhow::{anyhow, Context, Result};
use epd_waveshare::{epd2in13_v2::Epd2in13, epd2in9_v2::Epd2in9, prelude::*};
use linux_embedded_hal::{
    gpio_cdev::{Chip, LineRequestFlags},
    spidev::{SpiModeFlags, SpidevOptions},
    CdevPin, Delay, I2cdev, SpidevDevice,
};
use tracing::{info, warn};

type Pin = CdevPin;

/// Which concrete e-ink driver struct we're holding. There's no plain
/// monochrome epd2in7 variant here (see `PanelKind::resolve`'s doc comment)
/// -- the published epd-waveshare 0.6.0 only ships the bicolor epd2in7b for
/// that panel size, so 2.7" falls back to the 2.13" driver upstream.
enum EinkPanel {
    Epd2in13(Epd2in13<SpidevDevice, Pin, Pin, Pin, Delay>),
    Epd2in9(Epd2in9<SpidevDevice, Pin, Pin, Pin, Delay>),
}

type OledDisplay = ssd1306::Ssd1306<
    ssd1306::prelude::I2CInterface<I2cdev>,
    ssd1306::prelude::DisplaySize128x64,
    ssd1306::mode::BufferedGraphicsMode<ssd1306::prelude::DisplaySize128x64>,
>;

/// Live hardware backend: either the e-ink SPI panel or the OLED I2C panel.
pub(crate) struct HardwarePanel {
    kind: PanelKind,
    inner: PanelInner,
}

enum PanelInner {
    Eink {
        spi: SpidevDevice,
        delay: Delay,
        panel: EinkPanel,
    },
    // Boxed: OledDisplay is >1KB, dwarfing the Eink variant's fields and
    // tripping clippy::large_enum_variant.
    Oled {
        display: Box<OledDisplay>,
    },
}

fn open_spi(config: &DisplayConfig) -> Result<SpidevDevice> {
    let mut spi = SpidevDevice::open(&config.spi_path)
        .with_context(|| format!("opening SPI device {}", config.spi_path))?;
    let options = SpidevOptions::new()
        .bits_per_word(8)
        .max_speed_hz(config.spi_hz)
        .mode(SpiModeFlags::SPI_MODE_0)
        .build();
    spi.configure(&options)
        .context("configuring SPI mode/speed")?;
    Ok(spi)
}

fn request_pin(
    chip: &mut Chip,
    offset: u32,
    consumer: &str,
    flags: LineRequestFlags,
    default: u8,
) -> Result<Pin> {
    let line = chip
        .get_line(offset)
        .with_context(|| format!("getting GPIO line {offset}"))?;
    let handle = line
        .request(flags, default, consumer)
        .with_context(|| format!("requesting GPIO line {offset} ({consumer})"))?;
    CdevPin::new(handle).map_err(|e| anyhow!("wrapping GPIO line {offset} as CdevPin: {e:?}"))
}

impl HardwarePanel {
    /// Open and initialize the panel described by `config`/`kind` over
    /// real SPI/I2C + GPIO.
    pub(crate) fn open(config: &DisplayConfig, kind: PanelKind) -> Result<Self> {
        info!(
            "Opening real hardware display backend: kind={:?} spi={} gpio_chip={} i2c={}",
            kind, config.spi_path, config.gpio_chip, config.i2c_bus
        );

        if kind == PanelKind::Ssd1306_128x64 {
            return Self::open_oled(config);
        }

        let mut spi = open_spi(config)?;
        let mut chip = Chip::new(&config.gpio_chip)
            .with_context(|| format!("opening GPIO chip {}", config.gpio_chip))?;
        let busy = request_pin(
            &mut chip,
            config.pin_busy,
            "pwnghost-eink-busy",
            LineRequestFlags::INPUT,
            0,
        )?;
        let dc = request_pin(
            &mut chip,
            config.pin_dc,
            "pwnghost-eink-dc",
            LineRequestFlags::OUTPUT,
            0,
        )?;
        let rst = request_pin(
            &mut chip,
            config.pin_rst,
            "pwnghost-eink-rst",
            LineRequestFlags::OUTPUT,
            1,
        )?;
        let mut delay = Delay;

        let panel = match kind {
            PanelKind::Epd2in13 => EinkPanel::Epd2in13(
                Epd2in13::new(&mut spi, busy, dc, rst, &mut delay, None)
                    .map_err(|e| anyhow!("Epd2in13::new failed: {e:?}"))?,
            ),
            PanelKind::Epd2in9 => EinkPanel::Epd2in9(
                Epd2in9::new(&mut spi, busy, dc, rst, &mut delay, None)
                    .map_err(|e| anyhow!("Epd2in9::new failed: {e:?}"))?,
            ),
            PanelKind::Ssd1306_128x64 => unreachable!("handled by open_oled above"),
        };

        Ok(Self {
            kind,
            inner: PanelInner::Eink { spi, delay, panel },
        })
    }

    fn open_oled(config: &DisplayConfig) -> Result<Self> {
        // `ssd1306::mode::DisplayConfig` is the trait providing `.init()`;
        // imported unnamed (`as _`) since `crate::driver::DisplayConfig`
        // (our own config struct, imported above) already owns that name.
        // The crate is built without ssd1306's "async" feature (see root
        // Cargo.toml) specifically so this trait's sync `init`/methods
        // match linux-embedded-hal's I2cdev, which only implements the
        // sync embedded-hal::i2c::I2c, not embedded_hal_async's.
        use ssd1306::mode::DisplayConfig as _;
        let i2c = I2cdev::new(&config.i2c_bus)
            .with_context(|| format!("opening I2C bus {}", config.i2c_bus))?;
        let interface = ssd1306::I2CDisplayInterface::new_custom_address(i2c, config.i2c_address);
        let mut display = ssd1306::Ssd1306::new(
            interface,
            ssd1306::prelude::DisplaySize128x64,
            ssd1306::prelude::DisplayRotation::Rotate0,
        )
        .into_buffered_graphics_mode();
        display
            .init()
            .map_err(|e| anyhow!("Ssd1306::init failed: {e:?}"))?;

        Ok(Self {
            kind: PanelKind::Ssd1306_128x64,
            inner: PanelInner::Oled {
                display: Box::new(display),
            },
        })
    }

    /// Push a freshly rendered frame (this crate's internal packed
    /// convention, see `crate::layout`) to the panel.
    pub(crate) fn push_frame(
        &mut self,
        buffer: &[u8],
        width: u32,
        height: u32,
        partial: bool,
    ) -> Result<()> {
        match &mut self.inner {
            PanelInner::Eink { spi, delay, panel } => {
                let (panel_w, panel_h) = self.kind.dimensions();
                if panel_w != width || panel_h != height {
                    warn!(
                        "framebuffer is {}x{} but panel {:?} is natively {}x{}; \
                         proceeding, but check DisplayConfig::width/height / rotation",
                        width, height, self.kind, panel_w, panel_h
                    );
                }
                let packed = repack_for_panel(buffer, width, height);
                macro_rules! push {
                    ($p:expr) => {{
                        if partial {
                            $p.set_lut(spi, delay, Some(RefreshLut::Quick))
                                .map_err(|e| anyhow!("set_lut(Quick) failed: {e:?}"))?;
                            $p.update_and_display_frame(spi, &packed, delay)
                                .map_err(|e| anyhow!("update_and_display_frame failed: {e:?}"))?;
                        } else {
                            $p.set_lut(spi, delay, Some(RefreshLut::Full))
                                .map_err(|e| anyhow!("set_lut(Full) failed: {e:?}"))?;
                            $p.update_frame(spi, &packed, delay)
                                .map_err(|e| anyhow!("update_frame failed: {e:?}"))?;
                            $p.display_frame(spi, delay)
                                .map_err(|e| anyhow!("display_frame failed: {e:?}"))?;
                        }
                    }};
                }
                match panel {
                    EinkPanel::Epd2in13(p) => push!(p),
                    EinkPanel::Epd2in9(p) => push!(p),
                }
                Ok(())
            }
            PanelInner::Oled { display } => {
                // ssd1306's BufferedGraphicsMode implements
                // `embedded_graphics::DrawTarget<Color = BinaryColor>`;
                // bridge our internal packed convention (row-major,
                // LSB-first per byte, no row padding, bit=1=on) onto it
                // pixel-by-pixel rather than poking its internal buffer
                // layout directly.
                use embedded_graphics::{pixelcolor::BinaryColor, prelude::*, Pixel};
                let mut pixels = Vec::with_capacity((width * height) as usize);
                for y in 0..height {
                    for x in 0..width {
                        let idx = (y * width + x) as usize;
                        let byte = idx / 8;
                        let bit = idx % 8;
                        let on = buffer
                            .get(byte)
                            .map(|b| (b >> bit) & 1 != 0)
                            .unwrap_or(false);
                        pixels.push(Pixel(
                            Point::new(x as i32, y as i32),
                            if on {
                                BinaryColor::On
                            } else {
                                BinaryColor::Off
                            },
                        ));
                    }
                }
                display
                    .draw_iter(pixels)
                    .map_err(|e| anyhow!("Ssd1306 draw_iter failed: {e:?}"))?;
                display
                    .flush()
                    .map_err(|e| anyhow!("Ssd1306 flush failed: {e:?}"))?;
                Ok(())
            }
        }
    }

    pub(crate) fn sleep(&mut self) -> Result<()> {
        match &mut self.inner {
            PanelInner::Eink { spi, delay, panel } => {
                macro_rules! sleep {
                    ($p:expr) => {
                        $p.sleep(spi, delay)
                            .map_err(|e| anyhow!("panel sleep() failed: {e:?}"))
                    };
                }
                match panel {
                    EinkPanel::Epd2in13(p) => sleep!(p),
                    EinkPanel::Epd2in9(p) => sleep!(p),
                }
            }
            PanelInner::Oled { .. } => {
                // ssd1306's BufferedGraphicsMode doesn't expose a sleep/
                // display-off command through the same trait surface; a
                // real implementation would send the SSD1306 "display off"
                // command (0xAE) directly via the interface. Not wired up
                // for the OLED path yet.
                warn!("OLED backend: sleep() not implemented, no-op");
                Ok(())
            }
        }
    }

    pub(crate) fn wake(&mut self) -> Result<()> {
        match &mut self.inner {
            PanelInner::Eink { spi, delay, panel } => {
                macro_rules! wake {
                    ($p:expr) => {
                        $p.wake_up(spi, delay)
                            .map_err(|e| anyhow!("panel wake_up() failed: {e:?}"))
                    };
                }
                match panel {
                    EinkPanel::Epd2in13(p) => wake!(p),
                    EinkPanel::Epd2in9(p) => wake!(p),
                }
            }
            PanelInner::Oled { .. } => {
                warn!("OLED backend: wake() not implemented, no-op");
                Ok(())
            }
        }
    }
}
