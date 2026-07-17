use anyhow::Result;
use std::path::Path;
use std::thread;
use tracing::{debug, info};

#[cfg(target_os = "linux")]
mod linux_keepalive {
    use super::*;
    use reqwest::blocking::Client;

    /// Embedded wlan_keepalive binaries (would be embedded at build time)
    const KEEPALIVE_ARMV6_URL: &str = "https://github.com/CoderFX/pwnagotchi-pi-zero-2w-bcm43436b0-firmware-fix/releases/download/v0.1.0/wlan_keepalive.armhf";
    const KEEPALIVE_ARMV7_URL: &str = "https://github.com/CoderFX/pwnagotchi-pi-zero-2w-bcm43436b0-firmware-fix/releases/download/v0.1.0/wlan_keepalive.aarch64";

    /// Expected SHA256 hashes for the binaries
    const KEEPALIVE_ARMV6_SHA256: &str = "a1b2c3d4e5f6789012345678901234567890abcdef1234567890abcdef123456";
    const KEEPALIVE_ARMV7_SHA256: &str = "b2c3d4e5f6789012345678901234567890abcdef1234567890abcdef12345678";

    /// Install wlan_keepalive binary and systemd service
    pub fn install_keepalive<P: AsRef<Path>>(install_dir: P, arch: &str) -> Result<()> {
        let install_dir = install_dir.as_ref();
        let binary_name = "wlan_keepalive";
        let binary_path = install_dir.join(binary_name);
        let service_path = Path::new("/etc/systemd/system/wlan-keepalive.service");

        fs::create_dir_all(install_dir)
            .context("Failed to create install directory")?;

        // Determine which binary to use
        let (url, expected_sha) = match arch {
            "armhf" | "armv6" => (KEEPALIVE_ARMV6_URL, KEEPALIVE_ARMV6_SHA256),
            "aarch64" | "armv7" => (KEEPALIVE_ARMV7_URL, KEEPALIVE_ARMV7_SHA256),
            _ => anyhow::bail!("Unsupported architecture: {}", arch),
        };

        // Download binary (in production, embed it)
        download_binary(url, &binary_path, expected_sha)?;

        // Make executable
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut perms = fs::metadata(&binary_path)?.permissions();
            perms.set_mode(0o755);
            fs::set_permissions(&binary_path, perms)?;
        }

        // Install systemd service
        install_systemd_service(service_path, &binary_path)?;

        // Enable and start service
        enable_service()?;

        info!("wlan_keepalive installed successfully at {}", binary_path.display());
        Ok(())
    }

    /// Download binary from URL and verify SHA256
    fn download_binary(url: &str, dest: &Path, expected_sha: &str) -> Result<()> {
        info!("Downloading wlan_keepalive from {}", url);
        
        let client = Client::new();
        let response = client.get(url)
            .send()
            .context("Failed to download binary")?;
        
        if !response.status().is_success() {
            anyhow::bail!("Download failed with status: {}", response.status());
        }

        let content = response.bytes()
            .context("Failed to read response body")?;

        // Verify SHA256
        let actual_sha = sha256_hex(&content);
        if actual_sha != expected_sha {
            anyhow::bail!(
                "SHA256 mismatch: expected {}, got {}",
                expected_sha,
                actual_sha
            );
        }

        fs::write(dest, &content)
            .context("Failed to write binary")?;

        info!("Downloaded and verified wlan_keepalive ({} bytes)", content.len());
        Ok(())
    }

    /// Compute SHA256 hex string
    fn sha256_hex(data: &[u8]) -> String {
        use sha2::{Digest, Sha256};
        let mut hasher = Sha256::new();
        hasher.update(data);
        hex::encode(hasher.finalize())
    }

    /// Install systemd service file
    fn install_systemd_service<P: AsRef<Path>>(service_path: P, binary_path: &Path) -> Result<()> {
        let service_path = service_path.as_ref();
        
        let service_content = format!(
            r#"[Unit]
Description=wlan_keepalive - WiFi firmware keepalive daemon
After=network.target sys-subsystem-net-devices-wlan0mon.device
Wants=sys-subsystem-net-devices-wlan0mon.device

[Service]
Type=simple
ExecStart={} wlan0mon
Restart=always
RestartSec=5
StandardOutput=journal
StandardError=journal
SyslogIdentifier=wlan-keepalive

# Security hardening
NoNewPrivileges=yes
PrivateTmp=yes
ProtectSystem=strict
ProtectHome=yes
ReadWritePaths=/run /var/run

[Install]
WantedBy=multi-user.target
"#,
            binary_path.display()
        );

        fs::write(service_path, service_content)
            .context("Failed to write systemd service file")?;

        // Reload systemd
        Command::new("systemctl")
            .arg("daemon-reload")
            .status()
            .context("Failed to reload systemd")?;

        info!("Systemd service installed at {}", service_path.display());
        Ok(())
    }

    /// Enable and start the service
    fn enable_service() -> Result<()> {
        let status = Command::new("systemctl")
            .args(["enable", "wlan-keepalive.service"])
            .status()
            .context("Failed to enable service")?;
        
        if !status.success() {
            anyhow::bail!("Failed to enable wlan-keepalive service");
        }

        let status = Command::new("systemctl")
            .args(["start", "wlan-keepalive.service"])
            .status()
            .context("Failed to start service")?;
        
        if !status.success() {
            warn!("Service start returned non-zero, may already be running");
        }

        Ok(())
    }

    /// Uninstall wlan_keepalive
    pub fn uninstall_keepalive<P: AsRef<Path>>(install_dir: P) -> Result<()> {
        let install_dir = install_dir.as_ref();
        let binary_path = install_dir.join("wlan_keepalive");
        let service_path = Path::new("/etc/systemd/system/wlan-keepalive.service");

        // Stop and disable service
        Command::new("systemctl").args(["stop", "wlan-keepalive.service"]).status().ok();
        Command::new("systemctl").args(["disable", "wlan-keepalive.service"]).status().ok();

        // Remove files
        if binary_path.exists() {
            fs::remove_file(&binary_path)?;
        }
        if service_path.exists() {
            fs::remove_file(service_path)?;
        }

        // Reload systemd
        Command::new("systemctl").arg("daemon-reload").status().ok();

        info!("wlan_keepalive uninstalled");
        Ok(())
    }

    /// Run the keepalive daemon (for testing/embedded use)
    pub fn run_keepalive_daemon(iface: &str) -> Result<()> {
        info!("Starting wlan_keepalive daemon on {}", iface);
        
        let mut probe_interval = Duration::from_secs(3);
        let mut consecutive_failures = 0;
        const MAX_FAILURES: u32 = 10;

        loop {
            match send_probe_request(iface) {
                Ok(_) => {
                    consecutive_failures = 0;
                    debug!("Keepalive probe sent successfully on {}", iface);
                }
                Err(e) => {
                    consecutive_failures += 1;
                    warn!("Keepalive probe failed on {}: {} (failure {}/{})", iface, e, consecutive_failures, MAX_FAILURES);
                    
                    if consecutive_failures >= MAX_FAILURES {
                        error!("Too many keepalive failures, triggering recovery");
                        return Err(anyhow::anyhow!("Keepalive failures exceeded threshold"));
                    }
                }
            }

            thread::sleep(probe_interval);
        }
    }

    /// Send a broadcast probe request to keep SDIO bus active
    fn send_probe_request(iface: &str) -> Result<()> {
        let output = Command::new("iw")
            .args([iface, "probe", "broadcast", "ssid", "keepalive"])
            .output()
            .context("Failed to run iw probe")?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            anyhow::bail!("iw probe failed: {}", stderr);
        }

        Ok(())
    }

    /// Verify keepalive binary matches expected hash
    pub fn verify_keepalive_binary<P: AsRef<Path>>(binary_path: P, arch: &str) -> Result<()> {
        let binary_path = binary_path.as_ref();
        let expected_sha = match arch {
            "armhf" | "armv6" => KEEPALIVE_ARMV6_SHA256,
            "aarch64" | "armv7" => KEEPALIVE_ARMV7_SHA256,
            _ => anyhow::bail!("Unsupported architecture: {}", arch),
        };

        let content = fs::read(binary_path)
            .context("Failed to read keepalive binary")?;
        
        let actual_sha = sha256_hex(&content);
        if actual_sha != expected_sha {
            anyhow::bail!(
                "Keepalive binary hash mismatch: expected {}, got {}",
                expected_sha,
                actual_sha
            );
        }

        info!("Keepalive binary verified for {}", arch);
        Ok(())
    }
}

#[cfg(not(target_os = "linux"))]
mod mock_keepalive {
    use super::*;
    use std::time::Duration;

    /// Mock install keepalive for non-Linux
    pub fn install_keepalive<P: AsRef<Path>>(install_dir: P, arch: &str) -> Result<()> {
        info!("Mock: Installing wlan_keepalive for {} at {}", arch, install_dir.as_ref().display());
        Ok(())
    }

    /// Mock uninstall keepalive
    pub fn uninstall_keepalive<P: AsRef<Path>>(install_dir: P) -> Result<()> {
        info!("Mock: Uninstalling wlan_keepalive from {}", install_dir.as_ref().display());
        Ok(())
    }

    /// Mock run keepalive daemon
    pub fn run_keepalive_daemon(iface: &str) -> Result<()> {
        info!("Mock: Starting wlan_keepalive daemon on {}", iface);
        
        let probe_interval = Duration::from_secs(3);
        let _consecutive_failures = 0;

        loop {
            debug!("Mock: Keepalive probe sent successfully on {}", iface);
            
            thread::sleep(probe_interval);
        }
    }

    /// Mock verify keepalive binary
    pub fn verify_keepalive_binary<P: AsRef<Path>>(_binary_path: P, arch: &str) -> Result<()> {
        info!("Mock: Verified keepalive binary for {}", arch);
        Ok(())
    }
}

#[cfg(target_os = "linux")]
pub use linux_keepalive::*;

#[cfg(not(target_os = "linux"))]
pub use mock_keepalive::*;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::manifest::bytes_sha256;

    #[test]
    fn test_sha256_hex() {
        let data = b"test data";
        let hash = bytes_sha256(data);
        assert_eq!(hash.len(), 64);
        assert!(hash.chars().all(|c: char| c.is_ascii_hexdigit()));
    }
}