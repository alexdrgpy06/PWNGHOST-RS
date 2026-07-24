//! Ground-up SPI/GPIO driver for the Waveshare 2.13" e-Paper **V4** HAT
//! (SSD1680, V4 register/waveform sequence), ported byte-for-byte from the
//! real Python source pwnagotchi ships --
//! `pwnagotchi/ui/hw/libs/waveshare/epaper/v2in13_V4/epd2in13_V4.py` and
//! that package's `epdconfig.py` (the `RaspberryPi` backend class),
//! fetched directly from jayofelony/pwnagotchi via `gh api
//! .../contents/...`, not guessed.
//!
//! ## Why this exists instead of using `epd-waveshare`
//!
//! `epd-waveshare` 0.6.0 (the crate this project used before for this
//! panel) has **no V4 module** for the 2.13" size at all -- only
//! `epd2in13_v2` (confirmed directly against the published crate source:
//! `~/.cargo/registry/.../epd-waveshare-0.6.0/src/` lists `epd2in13_v2`,
//! `epd2in13bc`, `epd2in9(_v2)`, nothing V4). V2 and V4 are *not*
//! register/waveform-identical despite being nominally "the same SSD1680
//! family": comparing the two real Python drivers directly shows V4 uses a
//! different Border Waveform value between init (`0x05`) and partial
//! refresh (`0x80`), a different Display Update Control byte for partial
//! refresh (`0xFF`, vs whatever bit-flags `epd-waveshare`'s V2 driver
//! builds for its own "Quick" mode), and V4's `displayPartial()` never
//! rewrites the "old"/baseline RAM buffer (register `0x26`) that the
//! partial-refresh waveform math is computed against -- it's seeded once,
//! by `displayPartBaseImage()`, and left alone for the rest of the
//! session. Sending the wrong panel generation's LUT/register sequence
//! into real V4 silicon is a textbook cause of incomplete pixel
//! transitions, i.e. visible ghosting -- diagnosed as exactly that on real
//! hardware, which is why this module exists instead of a config tweak.
//!
//! ## Buffer format
//!
//! Callers hand this driver `crate::driver::repack_for_panel`'s output
//! directly (row-padded, MSB-first-per-byte, bit=1=white/bit=0=black).
//! This matches the real driver's own `getbuffer()` (PIL `'1'`-mode
//! `.tobytes('raw')` uses the same convention), so no additional
//! repacking happens in this module.

use anyhow::{anyhow, Result};
use embedded_hal::delay::DelayNs;
use embedded_hal::digital::{InputPin, OutputPin};
use embedded_hal::spi::SpiDevice;

/// Native panel resolution (`EPD_WIDTH`/`EPD_HEIGHT` in the real driver).
pub(crate) const WIDTH: u32 = 122;
pub(crate) const HEIGHT: u32 = 250;

/// Owns the panel's GPIO lines (BUSY/DC/RST/PWR). SPI and the delay
/// provider are borrowed per call, matching both this crate's existing
/// `epd-waveshare` calling convention in `hardware.rs` and embedded-hal's
/// own idiom of not owning a shared bus.
pub(crate) struct EpdV4<BUSY, DC, RST, PWR> {
    busy: BUSY,
    dc: DC,
    rst: RST,
    pwr: PWR,
}

impl<BUSY, DC, RST, PWR> EpdV4<BUSY, DC, RST, PWR>
where
    BUSY: InputPin,
    DC: OutputPin,
    RST: OutputPin,
    PWR: OutputPin,
{
    /// Powers the panel on (`epdconfig.py::module_init`'s
    /// `GPIO_PWR_PIN.on()` -- gates power to the whole e-ink stack; never
    /// wired up at all in this project before, unlike busy/dc/rst), then
    /// runs the real driver's exact `init()` register sequence, then
    /// `Clear(0xFF)` + seeds the partial-refresh baseline with a blank
    /// image via `display_part_base_image` -- matching the real
    /// `WaveshareV4.initialize()` wrapper's call order exactly
    /// (`init()` -> `Clear(0xFF)` -> `displayPartBaseImage(<blank image>)`).
    /// Every real frame after this goes through `display_partial` alone,
    /// same as the reference's `render()`.
    pub(crate) fn new<SPI: SpiDevice, DELAY: DelayNs>(
        spi: &mut SPI,
        busy: BUSY,
        dc: DC,
        rst: RST,
        pwr: PWR,
        delay: &mut DELAY,
    ) -> Result<Self> {
        let mut epd = Self { busy, dc, rst, pwr };
        epd.pwr
            .set_high()
            .map_err(|_| anyhow!("PWR pin set_high failed"))?;
        epd.init(spi, delay)?;
        epd.clear(spi, delay, 0xFF)?;
        let blank = vec![0xFFu8; buffer_len()];
        epd.display_part_base_image(spi, delay, &blank)?;
        Ok(epd)
    }

    fn send_command<SPI: SpiDevice>(&mut self, spi: &mut SPI, command: u8) -> Result<()> {
        self.dc
            .set_low()
            .map_err(|_| anyhow!("DC pin set_low failed"))?;
        spi.write(&[command])
            .map_err(|_| anyhow!("SPI write (command) failed"))?;
        Ok(())
    }

    fn send_data<SPI: SpiDevice>(&mut self, spi: &mut SPI, data: u8) -> Result<()> {
        self.dc
            .set_high()
            .map_err(|_| anyhow!("DC pin set_high failed"))?;
        spi.write(&[data])
            .map_err(|_| anyhow!("SPI write (data) failed"))?;
        Ok(())
    }

    fn send_data_slice<SPI: SpiDevice>(&mut self, spi: &mut SPI, data: &[u8]) -> Result<()> {
        self.dc
            .set_high()
            .map_err(|_| anyhow!("DC pin set_high failed"))?;
        spi.write(data)
            .map_err(|_| anyhow!("SPI write (data slice) failed"))?;
        Ok(())
    }

    /// `EPD.reset()` exactly: RST high(20ms) -> low(2ms) -> high(20ms).
    fn hard_reset<DELAY: DelayNs>(&mut self, delay: &mut DELAY) -> Result<()> {
        self.rst
            .set_high()
            .map_err(|_| anyhow!("RST pin set_high failed"))?;
        delay.delay_ms(20);
        self.rst
            .set_low()
            .map_err(|_| anyhow!("RST pin set_low failed"))?;
        delay.delay_ms(2);
        self.rst
            .set_high()
            .map_err(|_| anyhow!("RST pin set_high failed"))?;
        delay.delay_ms(20);
        Ok(())
    }

    /// `EPD.ReadBusy()` exactly: BUSY high == busy (per the real driver's
    /// own comment, "0: idle, 1: busy"), poll every 10ms.
    fn read_busy<DELAY: DelayNs>(&mut self, delay: &mut DELAY) -> Result<()> {
        while self
            .busy
            .is_high()
            .map_err(|_| anyhow!("BUSY pin read failed"))?
        {
            delay.delay_ms(10);
        }
        Ok(())
    }

    fn set_window<SPI: SpiDevice>(
        &mut self,
        spi: &mut SPI,
        x_start: u32,
        y_start: u32,
        x_end: u32,
        y_end: u32,
    ) -> Result<()> {
        self.send_command(spi, 0x44)?; // SET_RAM_X_ADDRESS_START_END_POSITION
        self.send_data(spi, ((x_start >> 3) & 0xFF) as u8)?;
        self.send_data(spi, ((x_end >> 3) & 0xFF) as u8)?;

        self.send_command(spi, 0x45)?; // SET_RAM_Y_ADDRESS_START_END_POSITION
        self.send_data(spi, (y_start & 0xFF) as u8)?;
        self.send_data(spi, ((y_start >> 8) & 0xFF) as u8)?;
        self.send_data(spi, (y_end & 0xFF) as u8)?;
        self.send_data(spi, ((y_end >> 8) & 0xFF) as u8)?;
        Ok(())
    }

    fn set_cursor<SPI: SpiDevice>(&mut self, spi: &mut SPI, x: u32, y: u32) -> Result<()> {
        self.send_command(spi, 0x4E)?; // SET_RAM_X_ADDRESS_COUNTER
        self.send_data(spi, (x & 0xFF) as u8)?;

        self.send_command(spi, 0x4F)?; // SET_RAM_Y_ADDRESS_COUNTER
        self.send_data(spi, (y & 0xFF) as u8)?;
        self.send_data(spi, ((y >> 8) & 0xFF) as u8)?;
        Ok(())
    }

    /// `EPD.TurnOnDisplay()` exactly (full-refresh activation).
    fn turn_on_display<SPI: SpiDevice, DELAY: DelayNs>(
        &mut self,
        spi: &mut SPI,
        delay: &mut DELAY,
    ) -> Result<()> {
        self.send_command(spi, 0x22)?; // Display Update Control
        self.send_data(spi, 0xF7)?;
        self.send_command(spi, 0x20)?; // Activate Display Update Sequence
        self.read_busy(delay)
    }

    /// `EPD.TurnOnDisplayPart()` exactly -- note the different Display
    /// Update Control byte (`0xFF`) vs `turn_on_display`'s `0xF7`.
    fn turn_on_display_part<SPI: SpiDevice, DELAY: DelayNs>(
        &mut self,
        spi: &mut SPI,
        delay: &mut DELAY,
    ) -> Result<()> {
        self.send_command(spi, 0x22)?;
        self.send_data(spi, 0xFF)?;
        self.send_command(spi, 0x20)?;
        self.read_busy(delay)
    }

    /// `EPD.init()` exactly.
    fn init<SPI: SpiDevice, DELAY: DelayNs>(
        &mut self,
        spi: &mut SPI,
        delay: &mut DELAY,
    ) -> Result<()> {
        self.hard_reset(delay)?;
        self.read_busy(delay)?;
        self.send_command(spi, 0x12)?; // SWRESET
        self.read_busy(delay)?;

        self.send_command(spi, 0x01)?; // Driver output control
        self.send_data(spi, 0xF9)?;
        self.send_data(spi, 0x00)?;
        self.send_data(spi, 0x00)?;

        self.send_command(spi, 0x11)?; // data entry mode
        self.send_data(spi, 0x03)?;

        self.set_window(spi, 0, 0, WIDTH - 1, HEIGHT - 1)?;
        self.set_cursor(spi, 0, 0)?;

        self.send_command(spi, 0x3C)?; // BorderWaveform
        self.send_data(spi, 0x05)?;

        self.send_command(spi, 0x21)?; // Display update control
        self.send_data(spi, 0x00)?;
        self.send_data(spi, 0x80)?;

        self.send_command(spi, 0x18)?; // Read built-in temperature sensor
        self.send_data(spi, 0x80)?;

        self.read_busy(delay)
    }

    /// `EPD.displayPartBaseImage()` exactly -- writes the same image into
    /// both the "new" (`0x24`) and "old"/baseline (`0x26`) RAM buffers,
    /// then does a real full-refresh activation. Called once, with a blank
    /// image, from `new()` -- matches the real `WaveshareV4.initialize()`
    /// wrapper exactly (it also only ever calls this once, with a blank
    /// `Image.new('1', ..., 255)`, never with real content).
    fn display_part_base_image<SPI: SpiDevice, DELAY: DelayNs>(
        &mut self,
        spi: &mut SPI,
        delay: &mut DELAY,
        image: &[u8],
    ) -> Result<()> {
        self.send_command(spi, 0x24)?;
        self.send_data_slice(spi, image)?;
        self.send_command(spi, 0x26)?;
        self.send_data_slice(spi, image)?;
        self.turn_on_display(spi, delay)
    }

    /// `EPD.displayPartial()` exactly. Deliberately does **not** rewrite
    /// the `0x26` "old" RAM buffer that `display_part_base_image` seeded
    /// once at construction -- the real driver never does either, so the
    /// panel's own partial-refresh waveform math keeps comparing every
    /// subsequent frame against that one fixed (blank) baseline for the
    /// rest of the session. This is a known characteristic of this exact
    /// vendor sequence, not a bug introduced here; every real frame this
    /// project renders goes through this one function, exactly matching
    /// the reference's `render()` always calling `displayPartial()`
    /// unconditionally, with no periodic full-refresh cycle of its own.
    pub(crate) fn display_partial<SPI: SpiDevice, DELAY: DelayNs>(
        &mut self,
        spi: &mut SPI,
        delay: &mut DELAY,
        image: &[u8],
    ) -> Result<()> {
        // Quick reset pulse, NOT the full `hard_reset` (no 20ms holds) --
        // ported exactly from `displayPartial`'s own inline reset-pin
        // toggle, distinct from `init`'s `reset()` call.
        self.rst
            .set_low()
            .map_err(|_| anyhow!("RST pin set_low failed"))?;
        delay.delay_ms(1);
        self.rst
            .set_high()
            .map_err(|_| anyhow!("RST pin set_high failed"))?;

        self.send_command(spi, 0x3C)?; // BorderWaveform -- 0x80 here, vs init's 0x05
        self.send_data(spi, 0x80)?;

        self.send_command(spi, 0x01)?; // Driver output control
        self.send_data(spi, 0xF9)?;
        self.send_data(spi, 0x00)?;
        self.send_data(spi, 0x00)?;

        self.send_command(spi, 0x11)?; // data entry mode
        self.send_data(spi, 0x03)?;

        self.set_window(spi, 0, 0, WIDTH - 1, HEIGHT - 1)?;
        self.set_cursor(spi, 0, 0)?;

        self.send_command(spi, 0x24)?; // WRITE_RAM
        self.send_data_slice(spi, image)?;
        self.turn_on_display_part(spi, delay)
    }

    /// `EPD.Clear()` exactly.
    fn clear<SPI: SpiDevice, DELAY: DelayNs>(
        &mut self,
        spi: &mut SPI,
        delay: &mut DELAY,
        color: u8,
    ) -> Result<()> {
        let fill = vec![color; buffer_len()];
        self.send_command(spi, 0x24)?;
        self.send_data_slice(spi, &fill)?;
        self.turn_on_display(spi, delay)
    }

    /// `EPD.sleep()`'s deep-sleep command only. Does **not** replicate the
    /// reference's accompanying `module_exit()` (SPI close + PWR pin off):
    /// that has no equivalent here since this driver's SPI/GPIO handles
    /// stay open for the life of the process (a long-running service, not
    /// the reference's one-shot script) -- `wake_up()` (a full re-`init()`,
    /// matching `epd-waveshare`'s own `wake_up() == init()` convention for
    /// this chip family; a hardware reset is the standard way to bring an
    /// SSD1680 back from deep-sleep mode 1) is how this project resumes.
    pub(crate) fn sleep<SPI: SpiDevice, DELAY: DelayNs>(
        &mut self,
        spi: &mut SPI,
        delay: &mut DELAY,
    ) -> Result<()> {
        self.send_command(spi, 0x10)?;
        self.send_data(spi, 0x01)?;
        delay.delay_ms(2000);
        Ok(())
    }

    /// Re-runs `init()` -- see `sleep`'s doc comment for why.
    pub(crate) fn wake_up<SPI: SpiDevice, DELAY: DelayNs>(
        &mut self,
        spi: &mut SPI,
        delay: &mut DELAY,
    ) -> Result<()> {
        self.init(spi, delay)
    }
}

/// Bytes needed for one full `WIDTH` x `HEIGHT` frame in this panel's
/// row-padded, byte-per-8-pixels convention (`linewidth * height` in the
/// real driver: `ceil(EPD_WIDTH / 8) * EPD_HEIGHT`).
fn buffer_len() -> usize {
    (WIDTH as usize).div_ceil(8) * HEIGHT as usize
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_buffer_len_matches_real_driver_linewidth_times_height() {
        // EPD_WIDTH=122 isn't a multiple of 8, so linewidth = 122/8 + 1 = 16
        // (matching `Clear()`'s own `int(self.width/8) + 1` branch).
        assert_eq!(buffer_len(), 16 * 250);
    }
}
