//! Firmware crash monitoring via SDIO RAMRW netlink
//!
//! Reads BCM43436B0 firmware crash counters via netlink to nexmon driver

use anyhow::Result;
use libc::{c_int, c_uint, c_void, sockaddr_nl, timeval, AF_NETLINK, SOCK_RAW, SOL_SOCKET};
use std::mem;
use std::time::Duration;
use tokio::time::sleep;
use tracing::{debug, info, warn};

/// Netlink family for nexmon (from nexmon driver)
pub const NETLINK_NEXMON: c_int = 31;
/// Command for SDIO RAMRW
pub const CMD_SDIO_RAMRW: c_uint = 0x500;

/// Firmware health status
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FirmwareHealth {
    Healthy,
    Degraded,
    Critical,
    Unknown,
}

/// Firmware monitor for crash counter tracking
pub struct FirmwareMonitor {
    prev_crash_suppress: u32,
    prev_hardfault: u32,
    crash_suppress: u32,
    hardfault: u32,
    health: FirmwareHealth,
    initialized: bool,
}

impl FirmwareMonitor {
    pub fn new() -> Self {
        Self {
            prev_crash_suppress: 0,
            prev_hardfault: 0,
            crash_suppress: 0,
            hardfault: 0,
            health: FirmwareHealth::Unknown,
            initialized: false,
        }
    }

    /// Update counters from raw values
    pub fn update_counters(&mut self, crash_suppress: u32, hardfault: u32) {
        if !self.initialized {
            self.prev_crash_suppress = crash_suppress;
            self.prev_hardfault = hardfault;
            self.crash_suppress = crash_suppress;
            self.hardfault = hardfault;
            self.initialized = true;
            self.health = FirmwareHealth::Healthy;
            return;
        }

        self.prev_crash_suppress = self.crash_suppress;
        self.prev_hardfault = self.hardfault;
        self.crash_suppress = crash_suppress;
        self.hardfault = hardfault;

        let delta_crash = crash_suppress.saturating_sub(self.prev_crash_suppress);
        let delta_fault = hardfault.saturating_sub(self.prev_hardfault);
        let total_delta = delta_crash + delta_fault;

        self.health = if total_delta == 0 {
            FirmwareHealth::Healthy
        } else if total_delta <= 3 {
            FirmwareHealth::Degraded
        } else {
            FirmwareHealth::Critical
        };
    }

    /// Poll firmware counters via SDIO RAMRW
    pub async fn poll(&mut self) -> FirmwareHealth {
        let crash = match sdio_read(ADDR_CRASH_SUPPRESS, 4).await {
            Ok(data) if data.len() == 4 => u32::from_le_bytes(data[..4].try_into().unwrap()),
            _ => {
                self.health = FirmwareHealth::Unknown;
                return self.health;
            }
        };

        let fault = match sdio_read(ADDR_HARDFAULT, 4).await {
            Ok(data) if data.len() == 4 => u32::from_le_bytes(data[..4].try_into().unwrap()),
            _ => {
                self.health = FirmwareHealth::Unknown;
                return self.health;
            }
        };

        self.update_counters(crash, fault);
        self.health
    }

    pub fn health(&self) -> FirmwareHealth {
        self.health
    }

    /// Reset firmware counters to zero
    pub async fn reset_counters(&mut self) -> Result<()> {
        sdio_write(ADDR_CRASH_SUPPRESS, &[0, 0, 0, 0]).await?;
        sdio_write(ADDR_HARDFAULT, &[0, 0, 0, 0]).await?;

        self.prev_crash_suppress = 0;
        self.prev_hardfault = 0;
        self.crash_suppress = 0;
        self.hardfault = 0;
        self.health = FirmwareHealth::Healthy;

        Ok(())
    }
}

impl Default for FirmwareMonitor {
    fn default() -> Self {
        Self::new()
    }
}

/// Firmware RAM addresses for crash counters (from CoderFX analysis)
/// Layer 2: fatal_error_wrapper suppression counter
pub const ADDR_CRASH_SUPPRESS: u32 = 0x03C094;
/// Layer 3: hardfault_recovery counter
pub const ADDR_HARDFAULT: u32 = 0x03C098;

/// Build netlink frame for SDIO RAMRW
fn build_netlink_frame(cmd: c_uint, set: bool, payload: &[u8]) -> Vec<u8> {
    let total_len = 16 + 8 + 8 + payload.len(); // nlmsghdr + nexudp + ioctl + payload
    let mut frame = Vec::with_capacity(total_len);

    // nlmsghdr (16 bytes)
    frame.extend_from_slice(&(total_len as u32).to_le_bytes()); // nlmsg_len
    frame.extend_from_slice(&0u16.to_le_bytes()); // nlmsg_type
    frame.extend_from_slice(&0u16.to_le_bytes()); // nlmsg_flags
    frame.extend_from_slice(&0u32.to_le_bytes()); // nlmsg_seq
    frame.extend_from_slice(&0u32.to_le_bytes()); // nlmsg_pid

    // nexudp_header (8 bytes)
    frame.extend_from_slice(b"NEX"); // magic
    frame.push(0); // type = NEXUDP_IOCTL
    frame.extend_from_slice(&0u32.to_le_bytes()); // security cookie

    // ioctl_header (8 bytes)
    frame.extend_from_slice(&cmd.to_le_bytes()); // cmd
    frame.extend_from_slice(&(set as u32).to_le_bytes()); // set flag

    // payload
    frame.extend_from_slice(payload);

    frame
}

/// Build read payload
fn build_read_payload(addr: u32, length: u32) -> Vec<u8> {
    let mut p = Vec::with_capacity(8);
    p.extend_from_slice(&addr.to_le_bytes());
    p.extend_from_slice(&length.to_le_bytes());
    p
}

/// Build write payload
fn build_write_payload(addr: u32, data: &[u8]) -> Vec<u8> {
    let mut p = Vec::with_capacity(4 + data.len());
    p.extend_from_slice(&addr.to_le_bytes());
    p.extend_from_slice(data);
    p
}

/// Read from firmware RAM via SDIO RAMRW
pub async fn sdio_read(addr: u32, length: u32) -> Result<Vec<u8>> {
    let payload = build_read_payload(addr, length);
    let frame = build_netlink_frame(CMD_SDIO_RAMRW, false, &payload);

    unsafe {
        let fd = libc::socket(AF_NETLINK, SOCK_RAW, NETLINK_NEXMON);
        if fd < 0 {
            return Err(anyhow::anyhow!(
                "netlink socket failed: {}",
                std::io::Error::last_os_error()
            ));
        }

        // Bind
        let mut sa: sockaddr_nl = mem::zeroed();
        sa.nl_family = AF_NETLINK as u16;
        if libc::bind(
            fd,
            &sa as *const _ as *const libc::sockaddr,
            mem::size_of::<sockaddr_nl>() as u32,
        ) < 0
        {
            libc::close(fd);
            return Err(anyhow::anyhow!(
                "netlink bind failed: {}",
                std::io::Error::last_os_error()
            ));
        }

        // Set timeout
        let tv = timeval {
            tv_sec: 3,
            tv_usec: 0,
        };
        libc::setsockopt(
            fd,
            SOL_SOCKET,
            libc::SO_RCVTIMEO,
            &tv as *const _ as *const c_void,
            mem::size_of::<timeval>() as u32,
        );

        // Send
        let sent = libc::send(fd, frame.as_ptr() as *const c_void, frame.len(), 0);
        if sent < 0 {
            libc::close(fd);
            return Err(anyhow::anyhow!(
                "netlink send failed: {}",
                std::io::Error::last_os_error()
            ));
        }

        // Receive
        let mut resp = vec![0u8; 4096];
        let n = libc::recv(fd, resp.as_mut_ptr() as *mut c_void, resp.len(), 0);
        libc::close(fd);

        if n < 0 {
            return Err(anyhow::anyhow!(
                "netlink recv failed: {}",
                std::io::Error::last_os_error()
            ));
        }

        let n = n as usize;
        if n < 16 + length as usize {
            return Err(anyhow::anyhow!(
                "short response: got {} bytes, need {}",
                n,
                16 + length
            ));
        }

        // Skip nlmsghdr (16 bytes) + nexudp (8) + ioctl (8) = 32 bytes
        // Data starts at offset 32
        let data_start = 32;
        let data_end = data_start + length as usize;
        Ok(resp[data_start..data_end].to_vec())
    }
}

/// Write to firmware RAM via SDIO RAMRW
pub async fn sdio_write(addr: u32, data: &[u8]) -> Result<()> {
    let payload = build_write_payload(addr, data);
    let frame = build_netlink_frame(CMD_SDIO_RAMRW, true, &payload);

    unsafe {
        let fd = libc::socket(AF_NETLINK, SOCK_RAW, NETLINK_NEXMON);
        if fd < 0 {
            return Err(anyhow::anyhow!(
                "netlink socket failed: {}",
                std::io::Error::last_os_error()
            ));
        }

        let mut sa: sockaddr_nl = mem::zeroed();
        sa.nl_family = AF_NETLINK as u16;
        if libc::bind(
            fd,
            &sa as *const _ as *const libc::sockaddr,
            mem::size_of::<sockaddr_nl>() as u32,
        ) < 0
        {
            libc::close(fd);
            return Err(anyhow::anyhow!(
                "netlink bind failed: {}",
                std::io::Error::last_os_error()
            ));
        }

        let tv = timeval {
            tv_sec: 3,
            tv_usec: 0,
        };
        libc::setsockopt(
            fd,
            SOL_SOCKET,
            libc::SO_RCVTIMEO,
            &tv as *const _ as *const c_void,
            mem::size_of::<timeval>() as u32,
        );

        let sent = libc::send(fd, frame.as_ptr() as *const c_void, frame.len(), 0);
        libc::close(fd);

        if sent < 0 {
            return Err(anyhow::anyhow!(
                "netlink send failed: {}",
                std::io::Error::last_os_error()
            ));
        }

        Ok(()) // SET operations: timeout on recv = success
    }
}

/// Background monitor task
pub async fn run_monitor_task(mut monitor: FirmwareMonitor, interval_secs: u64) {
    info!(
        "Starting firmware crash monitor (interval: {}s)",
        interval_secs
    );

    loop {
        let health = monitor.poll().await;
        match health {
            FirmwareHealth::Healthy => {
                debug!("Firmware healthy");
            }
            FirmwareHealth::Degraded => {
                warn!(
                    "Firmware degraded: crash_suppress={}, hardfault={}",
                    monitor.crash_suppress, monitor.hardfault
                );
            }
            FirmwareHealth::Critical => {
                warn!(
                    "Firmware critical! crash_suppress={}, hardfault={}",
                    monitor.crash_suppress, monitor.hardfault
                );
                // Could trigger healing here
            }
            FirmwareHealth::Unknown => {
                warn!("Firmware health unknown (nexmon not available?)");
            }
        }

        sleep(Duration::from_secs(interval_secs)).await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_firmware_monitor_new() {
        let monitor = FirmwareMonitor::new();
        assert!(!monitor.initialized);
        assert_eq!(monitor.health(), FirmwareHealth::Unknown);
    }

    #[test]
    fn test_firmware_monitor_health_assessment() {
        let mut monitor = FirmwareMonitor::new();

        // First update sets baseline
        monitor.update_counters(5, 2);
        assert_eq!(monitor.health(), FirmwareHealth::Healthy);

        // Same values = healthy
        monitor.update_counters(5, 2);
        assert_eq!(monitor.health(), FirmwareHealth::Healthy);

        // Small increase = degraded
        monitor.update_counters(7, 2); // +2
        assert_eq!(monitor.health(), FirmwareHealth::Degraded);

        // Larger increase = critical
        monitor.update_counters(12, 5); // +5 total
        assert_eq!(monitor.health(), FirmwareHealth::Critical);
    }

    #[test]
    fn test_read_payload() {
        let p = build_read_payload(0x1000, 4);
        assert_eq!(p.len(), 8);
        assert_eq!(u32::from_le_bytes(p[0..4].try_into().unwrap()), 0x1000);
        assert_eq!(u32::from_le_bytes(p[4..8].try_into().unwrap()), 4);
    }

    #[test]
    fn test_write_payload() {
        let p = build_write_payload(0x1000, &[0xAA, 0xBB, 0xCC, 0xDD]);
        assert_eq!(p.len(), 8);
        assert_eq!(u32::from_le_bytes(p[0..4].try_into().unwrap()), 0x1000);
        assert_eq!(&p[4..8], &[0xAA, 0xBB, 0xCC, 0xDD]);
    }
}
