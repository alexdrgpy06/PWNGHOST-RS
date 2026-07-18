#!/bin/bash -e
# 04-pwnghost-base/00-run.sh - PWNGHOST-RS specific base packages + user setup.
#
# stage2/01-sys-tweaks (vendored from upstream pi-gen) already created
# FIRST_USER_NAME with adduser, set its password, and added it to the usual
# hardware-access groups (plugdev, video, i2c, spi, gpio, input, ...). This
# stage only adds what's specific to PWNGHOST-RS: the runtime packages the
# daemon / overlay scripts need that aren't already pulled in by the
# upstream Raspberry Pi OS Lite package set.
#
# NOTE: this used to also grant FIRST_USER_NAME passwordless sudo, matching
# jayofelony/pwnagotchi's actual default (verified by reading their
# stage3/07-patches/files/user-data cloud-init file). Deliberately dropped:
# the default credentials now match that convention (pi/raspberry, for
# drop-in-replacement fidelity), but sudo still requires the password as a
# real security improvement over the reference image, since there's no
# longer an interactive first-boot wizard prompting the user to change it.

on_chroot << EOF
set -e
export DEBIAN_FRONTEND=noninteractive
apt-get update

# Runtime packages required by pwnghost-rs / the runtime overlay (stage5)
# that aren't already covered by upstream stage2 packages.
apt-get install -y --no-install-recommends \
    libpcap0.8 \
    aircrack-ng \
    bluez-tools \
    dbus \
    dnsmasq-base \
    i2c-tools \
    lsb-release \
    software-properties-common

# Optional, nice-to-have tools: best-effort so one missing package can't
# abort the whole image build.
for pkg in hcxtools hcxdumptool zram-tools minisign; do
    apt-get install -y --no-install-recommends "\$pkg" || echo "pwnghost-rs: optional package '\$pkg' unavailable, skipping"
done

apt-get clean
rm -rf /var/lib/apt/lists/*
EOF

# --- Disable the userconf-pi interactive first-boot wizard -----------------
# stage1/01-sys-tweaks already creates the "pwn" user with a fixed password
# (pi-gen/config: FIRST_USER_NAME/FIRST_USER_PASS) precisely so this image
# has known-working default credentials, like upstream pwnagotchi images.
# But DISABLE_FIRST_BOOT_USER_RENAME only controls pi-gen's OWN build-time
# rename-user step (export-image/01-user-rename) -- it's unrelated to
# userconf-pi (installed via stage2/01-sys-tweaks' package list), which
# ships its own separate userconfig.service. That unit is enabled by
# default and, on a CLI-boot (non-desktop) image like this one, launches an
# interactive whiptail wizard on tty8 waiting to rename/re-password
# whatever account is on UID 1000 -- undermining the fixed "pwn" account
# the moment anyone (or anything) interacts with it. Disable it explicitly,
# matching exactly what userconf-pi's own `cancel-rename` helper does.
on_chroot << EOF
systemctl disable userconfig 2>/dev/null || true
systemctl enable getty@tty1 2>/dev/null || true
EOF

# --- Boot partition placeholders --------------------------------------------
# ENABLE_SSH=1 (pi-gen/config) already enables sshd via stage2/01-sys-tweaks'
# vendored `systemctl enable ssh`; also drop the stock Raspberry Pi Imager
# convention (an empty `ssh` file on the boot partition) as a harmless
# belt-and-suspenders in case someone re-flashes just the boot partition.
touch "${ROOTFS_DIR}/boot/firmware/ssh"

# wpa_supplicant.conf template for SAFE-mode / initial WiFi setup: a user can
# rename this to wpa_supplicant.conf on the boot partition before first boot
# to pre-seed a home network for headless internet-connected setup. Note:
# Raspberry Pi OS bookworm manages WiFi via NetworkManager by default, not
# wpa_supplicant/dhcpcd directly - NetworkManager's own
# etc-firstrun/nmcli path is the "current" mechanism, but it still honours
# a legacy /boot/firmware/wpa_supplicant.conf via raspberrypi-net-mods'
# migration shim on first boot, so this template is kept for that
# compatibility path (not independently verified on real hardware here).
cat > "${ROOTFS_DIR}/boot/firmware/wpa_supplicant.conf.example" << 'EOF'
# Rename this file to "wpa_supplicant.conf" (same /boot/firmware partition)
# before first boot to pre-seed a WiFi network for initial headless setup /
# SAFE-mode internet access. Not required if you're only using the USB
# tether (see cmdline.txt / usb-net.service) or Bluetooth PAN tether.
country=US
ctrl_interface=DIR=/var/run/wpa_supplicant GROUP=netdev
update_config=1

network={
    ssid="YOUR_WIFI_SSID"
    psk="YOUR_WIFI_PASSWORD"
}
EOF

# WiFi regulatory-domain override for stage5's wifi-country.sh (drop a
# plain-text 2-letter country code here before first boot, e.g. "GB").
cat > "${ROOTFS_DIR}/boot/firmware/country.txt.example" << 'EOF'
US
EOF

# --- Login banner ------------------------------------------------------
# Classic pwnagotchi shows a helpful console banner at login (service
# status, web UI URL, where to look for logs/progress). This image shipped
# with none at all -- a bare login prompt with no hint of what's running
# or how to check on it. Uses Debian's standard update-motd.d mechanism
# (dynamic, run at login time) so it reflects live state rather than
# baking in stale info at build time.
install -d -m 755 "${ROOTFS_DIR}/etc/update-motd.d"
cat > "${ROOTFS_DIR}/etc/update-motd.d/50-pwnghost-rs" << 'MOTD_EOF'
#!/bin/bash
STATUS="$(systemctl is-active pwnghost-rs.service 2>/dev/null || echo unknown)"
IP="$(hostname -I 2>/dev/null | awk '{print $1}')"
cat << BANNER

  (◕‿‿◕)  pwnghost-rs

  Service:   ${STATUS}
  Web UI:    http://${IP:-<device-ip>}:8080/
  Logs:      journalctl -u pwnghost-rs -f
  Progress:  cat /var/lib/pwnghost/recovery.json
  Config:    /etc/pwnghost/config.toml

  Default login is pi/raspberry -- change it once you've confirmed
  everything works (passwd).

BANNER
MOTD_EOF
chmod 755 "${ROOTFS_DIR}/etc/update-motd.d/50-pwnghost-rs"
