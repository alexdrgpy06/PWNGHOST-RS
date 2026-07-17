#!/bin/bash -e
# 00-install-pwnghost/00-run.sh - Install pwnghost-rs config + systemd unit.
#
# The pwnghost-rs binary itself and AngryOxide were already installed by
# stage4/00-install-artifacts. This stage lays down /etc/pwnghost, the
# default config.toml, the pwnghost-rs.service unit, and the manual
# monstart/monstop helper scripts.

on_chroot << EOF
# Create config directory
mkdir -p /etc/pwnghost/conf.d
mkdir -p /etc/pwnghost/handshakes
mkdir -p /var/log/pwnghost
mkdir -p /var/tmp/pwnghost
mkdir -p /usr/local/share/pwnghost/custom-plugins

# Install default config
cat > /etc/pwnghost/config.toml << 'CONFIG_EOF'
# PWNGHOST-RS Configuration for Raspberry Pi Zero W / Zero 2W
# Waveshare V4 e-ink display (2.13" / 2.7" / 2.9")

[main]
name = "pwnghost"
lang = "en"
iface = "wlan0"
mon_start_cmd = "/usr/bin/monstart"
mon_stop_cmd = "/usr/bin/monstop"
mon_max_blind_epochs = 5
no_restart = false
whitelist = []
confd = "/etc/pwnghost/conf.d/"
custom_plugin_repos = []
custom_plugins = "/usr/local/share/pwnghost/custom-plugins/"

[main.log]
path = "/var/log/pwnghost/pwnghost.log"
path_debug = "/var/log/pwnghost/pwnghost-debug.log"

[main.log.rotation]
enabled = true
size = "10M"

[personality]
bored_num_epochs = 50
sad_num_epochs = 100
angry_num_epochs = 200
lonely_num_epochs = 150
bond_encounters_factor = 1.0
max_interactions = 10
throttle = 30
reward_handshake = 100
reward_new_ap = 10
reward_association = 5
penalty_missed = -10
penalty_reboot = -50
min_recon_time = 5
max_recon_time = 30
hop_recon_time = 10
deauth = false
associate = false
min_rssi = -80

[ui.web]
enabled = true
address = "0.0.0.0"
auth = false
username = "changeme"
password = "changeme"
origin = ""
port = 8080
on_frame = ""

[ui.web.theme]
accent_r = 76
accent_g = 175
accent_b = 80

[ui.display]
enabled = true
rotation = 180
display_type = "waveshare_v4"

[ui.faces]
png = false
position_x = 0
position_y = 16

[bettercap]
handshakes = "/etc/pwnghost/handshakes"
silence = [
    "ble.device.new", "ble.device.lost",
    "wifi.client.new", "wifi.client.lost",
    "wifi.ap.new", "wifi.ap.lost",
    "mod.started"
]

[fs]
enabled = true

[fs.mounts.log]
enabled = true
mount = "/var/log/pwnghost/"
size = "50M"
sync = 60
zram = true
rsync = true

[fs.mounts.data]
enabled = true
mount = "/var/tmp/pwnghost"
size = "10M"
sync = 3600
zram = true
rsync = true

[plugins.auto_tune]
enabled = true

[plugins.auto_backup]
enabled = true

[plugins.auto_update]
enabled = true

[plugins.grid]
enabled = true

[plugins.webcfg]
enabled = true
CONFIG_EOF

# Install systemd service
cat > /etc/systemd/system/pwnghost-rs.service << 'SERVICE_EOF'
[Unit]
Description=PWNGHOST-RS - Rust Pwnagotchi Implementation
Documentation=https://github.com/pwnghost-rs/pwnghost-rs
After=network.target
Wants=network.target

[Service]
Type=notify
NotifyAccess=main
ExecStart=/usr/local/bin/pwnghost-rs --config /etc/pwnghost/config.toml
Restart=on-failure
RestartSec=5
StartLimitIntervalSec=60
StartLimitBurst=3

# Security hardening
NoNewPrivileges=yes
PrivateTmp=yes
ProtectSystem=strict
ProtectHome=yes
ReadWritePaths=/etc/pwnghost /var/log/pwnghost /var/tmp/pwnghost /run/pwnghost
CapabilityBoundingSet=CAP_NET_ADMIN CAP_NET_RAW CAP_SYS_ADMIN CAP_DAC_OVERRIDE CAP_SYS_RESOURCE
AmbientCapabilities=CAP_NET_ADMIN CAP_NET_RAW

# Resource limits
LimitNOFILE=65536
LimitNPROC=4096
MemoryMax=50M
CPUQuota=80%

# Logging
StandardOutput=journal
StandardError=journal
SyslogIdentifier=pwnghost-rs

[Install]
WantedBy=multi-user.target
SERVICE_EOF

# Manual monitor-mode helper scripts, for interactive/admin use only.
# AngryOxide manages monitor mode itself via netlink at runtime (it takes a
# normal interface name, e.g. wlan0, and puts it into monitor mode itself) -
# neither pwnghost-rs.service nor any boot-time script calls these; they
# exist purely so an operator can toggle monitor mode by hand over SSH.
cat > /usr/bin/monstart << 'MONSTART_EOF'
#!/bin/bash
# Put the Wi-Fi radio into monitor mode as wlan0mon (manual/admin use only).
set -e
IFACE="\${1:-wlan0}"
ip link set "\$IFACE" down
iw dev "\$IFACE" set type monitor
ip link set "\$IFACE" name "\${IFACE}mon" 2>/dev/null || true
ip link set "\${IFACE}mon" up
MONSTART_EOF

cat > /usr/bin/monstop << 'MONSTOP_EOF'
#!/bin/bash
# Return the radio to managed mode (manual/admin use only).
set -e
IFACE="\${1:-wlan0}"
MON="\${IFACE}mon"
ip link set "\$MON" down 2>/dev/null || true
iw dev "\$MON" set type managed 2>/dev/null || true
ip link set "\$MON" name "\$IFACE" 2>/dev/null || true
ip link set "\$IFACE" up
MONSTOP_EOF

chmod +x /usr/bin/monstart /usr/bin/monstop

# wlan_keepalive: the binary itself was installed by stage4 (cross-compiled
# from crates/fw-patcher/vendor/wlan_keepalive.c). This unit's shape (path,
# unit name, positional "iface poll_ms" ExecStart) must match exactly what
# crates/fw-patcher/src/keepalive.rs writes at runtime, since that module
# may rewrite/renew this same file - the two are meant to be identical, not
# two competing implementations. This is NOT the old bash ping/arping stub
# (that never worked: a monitor-mode interface has no IP stack to route
# ICMP/ARP through) - it is the real raw-AF_PACKET probe-request daemon.
cat > /etc/systemd/system/wlan_keepalive.service << 'KEEPALIVE_EOF'
[Unit]
Description=WiFi monitor interface keepalive (BCM43436B0 SDIO bus)
Documentation=https://github.com/pwnghost-rs/pwnghost-rs
After=network.target
Wants=network.target

[Service]
Type=simple
ExecStart=/usr/local/bin/wlan_keepalive wlan0mon 100
Restart=always
RestartSec=3
Nice=10
StandardOutput=journal
StandardError=journal

# Security hardening
NoNewPrivileges=yes
PrivateTmp=yes
ProtectSystem=strict
ProtectHome=yes

[Install]
WantedBy=multi-user.target
KEEPALIVE_EOF

# Enable the daemons; leave it disabled-by-default's dependencies (network
# etc.) to systemd ordering. Group/permission setup happens once the runtime
# overlay (stage5/01-runtime-overlay) has created /etc/pwnghost ownership.
systemctl enable pwnghost-rs.service
systemctl enable wlan_keepalive.service
EOF
