//! GPIO control for WL_REG_ON power cycling (BCM43436B0)

use anyhow::{Context, Result};
use rppal::gpio::{Gpio, Level, OutputPin};
use std::time::Duration;
use tokio::time::sleep;
use tracing::{info, warn};

/// GPIO pin for WL_REG_ON on Pi Zero 2W (GPIO 22, same as Pi 3B+)
pub const WL_REG_ON_PIN: u8 = 22;

/// Default pulse duration for power cycle (ms)
pub const DEFAULT_PULSE_MS: u64 = 100;

/// Power cycle the WiFi chip via WL_REG_ON GPIO
pub async fn power_cycle_wl_reg_on() -> Result<()> {
    power_cycle_wl_reg_on_with_params(WL_REG_ON_PIN, DEFAULT_PULSE_MS).await
}

/// Power cycle with custom pin and duration
pub async fn power_cycle_wl_reg_on_with_params(pin: u8, pulse_ms: u64) -> Result<()> {
    info!("Power cycling WL_REG_ON on GPIO {} for {}ms", pin, pulse_ms);

    let gpio = Gpio::new()
        .context("Failed to initialize GPIO")?;

    let mut wl_reg_on = gpio.get(pin)
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
    let gpio = Gpio::new()
        .context("Failed to initialize GPIO")?;

    let mut wl_reg_on = gpio.get(pin)
        .with_context(|| format!("Failed to get GPIO {}", pin))?
        .into_output_low();

    wl_reg_on.set_high();
    info!("WL_REG_ON set HIGH on GPIO {}", pin);
    Ok(wl_reg_on)
}

/// Set WL_REG_ON low (disable chip)
pub async fn set_wl_reg_on_low(pin: u8) -> Result<OutputPin> {
    let gpio = Gpio::new()
        .context("Failed to initialize GPIO")?;

    let mut wl_reg_on = gpio.get(pin)
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

    // 3. Keep high for normal operation
    // (Pin stays high after this)

    info!("Full power cycle sequence complete");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_gpio_constants() {
        assert_eq!(WL_REG_ON_PIN, 22);
        assert_eq!(DEFAULT_PULSE_MS, 100);
    }
}