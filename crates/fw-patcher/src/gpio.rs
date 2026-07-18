//! GPIO control for WL_REG_ON power cycling (BCM43436B0)
//!
//! The real implementation uses `rppal` and is only compiled when the
//! `linux-gpio` feature is enabled (i.e. on the Raspberry Pi target). On
//! other build targets a stub is provided so the workspace still compiles.
//!
//! # DANGER: do not wire this into automatic recovery for BCM43436B0
//! The sibling `oxigotchi` project *used to* GPIO-power-cycle WL_REG_ON as
//! part of its automatic WiFi-crash recovery, and removed it after
//! discovering it corrupts BCM43436B0 in production: toggling WL_REG_ON on
//! this chip reverts it to stock (non-nexmon) firmware, losing
//! monitor-mode support entirely until a physical power cycle (unplug
//! USB / PiSugar) -- see `oxigotchi/rust/src/recovery/mod.rs` (the comment
//! block above `HardRecover`) and `oxigotchi/rust/src/main.rs` around the
//! `handle_recovery_action` match. oxigotchi's hard recovery now explicitly
//! *refuses* the GPIO cycle and surfaces a "firmware crash" status instead,
//! waiting for a human to physically power-cycle the device.
//!
//! This module is kept for manual/diagnostic use (e.g. an explicit CLI
//! subcommand or a `mock`-mode test), but nothing in this crate calls it
//! automatically -- see `lib.rs::apply_on_first_boot` and
//! `monitor::run_monitor_task`, which both deliberately avoid it.

use anyhow::Result;

/// GPIO pin for WL_REG_ON on Pi Zero 2W (BCM43436B0).
///
/// GPIO 41, not 22 -- corrected from an earlier "same as Pi 3B+" assumption
/// after a fresh audit of oxigotchi's actual field-tested recovery scripts
/// (`tools/wifi-recovery.sh`, `tools/wifi-watchdog.sh`, both use
/// `pinctrl set 41 op dl/dh` against this exact chip/board combination,
/// 2026-07-18). Per the module-level DANGER note, this pin is never
/// toggled automatically either way -- but a wrong constant here would be
/// a silent landmine for the manual/diagnostic path this module exists
/// for, toggling the wrong line instead of doing nothing or erroring.
pub const WL_REG_ON_PIN: u8 = 41;

/// Default pulse duration for power cycle (ms)
pub const DEFAULT_PULSE_MS: u64 = 100;

/// Power cycle the WiFi chip via WL_REG_ON GPIO.
///
/// See the module-level DANGER note: on BCM43436B0 this reverts the chip to
/// stock firmware (no nexmon monitor-mode support) until a physical power
/// cycle. Only call this deliberately (e.g. a manual recovery CLI command
/// or diagnostics); never from an automatic health/recovery loop.
pub async fn power_cycle_wl_reg_on() -> Result<()> {
    power_cycle_wl_reg_on_with_params(WL_REG_ON_PIN, DEFAULT_PULSE_MS).await
}

#[cfg(feature = "linux-gpio")]
mod imp {
    use super::*;
    use anyhow::Context;
    use rppal::gpio::{Gpio, OutputPin};
    use std::time::Duration;
    use tokio::time::sleep;
    use tracing::info;

    /// Power cycle with custom pin and duration
    pub async fn power_cycle_wl_reg_on_with_params(pin: u8, pulse_ms: u64) -> Result<()> {
        info!("Power cycling WL_REG_ON on GPIO {} for {}ms", pin, pulse_ms);

        let gpio = Gpio::new().context("Failed to initialize GPIO")?;

        let mut wl_reg_on = gpio
            .get(pin)
            .with_context(|| format!("Failed to get GPIO {}", pin))?
            .into_output_low();

        // Ensure it's low first
        wl_reg_on.set_low();
        sleep(Duration::from_millis(10)).await;

        // Pulse high
        wl_reg_on.set_high();
        sleep(Duration::from_millis(pulse_ms)).await;

        // Pulse low
        wl_reg_on.set_low();
        sleep(Duration::from_millis(10)).await;

        info!("WL_REG_ON power cycle complete");
        Ok(())
    }

    /// Set WL_REG_ON high (enable chip)
    pub async fn set_wl_reg_on_high(pin: u8) -> Result<OutputPin> {
        let gpio = Gpio::new().context("Failed to initialize GPIO")?;

        let mut wl_reg_on = gpio
            .get(pin)
            .with_context(|| format!("Failed to get GPIO {}", pin))?
            .into_output_low();

        wl_reg_on.set_high();
        info!("WL_REG_ON set HIGH on GPIO {}", pin);
        Ok(wl_reg_on)
    }

    /// Set WL_REG_ON low (disable chip)
    pub async fn set_wl_reg_on_low(pin: u8) -> Result<OutputPin> {
        let gpio = Gpio::new().context("Failed to initialize GPIO")?;

        let mut wl_reg_on = gpio
            .get(pin)
            .with_context(|| format!("Failed to get GPIO {}", pin))?
            .into_output_low();

        wl_reg_on.set_low();
        info!("WL_REG_ON set LOW on GPIO {}", pin);
        Ok(wl_reg_on)
    }

    /// Full power cycle sequence with chip enable/disable
    pub async fn full_power_cycle(pin: u8, pulse_ms: u64) -> Result<()> {
        info!("Starting full power cycle sequence on GPIO {}", pin);

        // 1. Disable chip
        let _pin_low = set_wl_reg_on_low(pin).await?;
        sleep(Duration::from_millis(100)).await;

        // 2. Pulse high to reset
        let _pin_high = set_wl_reg_on_high(pin).await?;
        sleep(Duration::from_millis(pulse_ms)).await;

        // 3. Keep high for normal operation (pin stays high after this)

        info!("Full power cycle sequence complete");
        Ok(())
    }
}

#[cfg(not(feature = "linux-gpio"))]
mod imp {
    use super::*;
    use tracing::warn;

    fn unavailable(what: &str) -> Result<()> {
        warn!("{what}: GPIO not available on this build (rebuild with --features linux-gpio)");
        anyhow::bail!("GPIO support not compiled in (enable the `linux-gpio` feature)")
    }

    /// Power cycle with custom pin and duration (stub without `linux-gpio`)
    pub async fn power_cycle_wl_reg_on_with_params(pin: u8, _pulse_ms: u64) -> Result<()> {
        unavailable(&format!("power_cycle_wl_reg_on GPIO {pin}"))
    }

    /// Full power cycle sequence (stub without `linux-gpio`)
    pub async fn full_power_cycle(pin: u8, _pulse_ms: u64) -> Result<()> {
        unavailable(&format!("full_power_cycle GPIO {pin}"))
    }
}

pub use imp::*;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_gpio_constants() {
        assert_eq!(WL_REG_ON_PIN, 41);
        assert_eq!(DEFAULT_PULSE_MS, 100);
    }
}
