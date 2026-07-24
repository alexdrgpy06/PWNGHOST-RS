#!/bin/bash -e
# 00-install-pwnghost/00-run.sh - Install pwnghost-rs config + systemd unit.
#
# The pwnghost-rs binary itself was already installed by
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
iface = "wlan0mon"
mon_start_cmd = "/usr/bin/monstart"
mon_stop_cmd = "/usr/bin/monstop"
mon_max_blind_epochs = 50
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
deauth = true
associate = true
min_rssi = -200

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
position_y = 34

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
# StartLimitIntervalSec=/StartLimitBurst= belong in [Unit], not [Service]
# -- confirmed by a real systemd warning on hardware ("Unknown key name
# 'StartLimitIntervalSec' in section 'Service', ignoring") on the
# bullseye-era systemd the tools/rebase-jayofelony pipeline targets;
# [Unit] is the universally correct location across systemd versions.
StartLimitIntervalSec=60
StartLimitBurst=3

[Service]
Type=notify
NotifyAccess=main
ExecStart=/usr/local/bin/pwnghost-rs --config /etc/pwnghost/config.toml
Restart=on-failure
RestartSec=5
# The app's own main loop calls sd_notify::watchdog() every 15s (see
# crates/pwnghost-rs/src/main.rs's watchdog_interval), but that call is
# inert without this: without WatchdogSec=, systemd never arms watchdog
# tracking at all, so a genuinely hung (not crashed) process would never
# be noticed or restarted. 45s is 3x the app's own 15s notify cadence,
# tolerating a couple of missed beats from scheduling jitter before
# treating it as a real hang. Mirrors the same fix in
# tools/rebase-jayofelony/overlay/etc/systemd/system/pwnghost-rs.service.
WatchdogSec=45

# Deliberately NOT sandboxed with ProtectSystem=strict/PrivateTmp=yes/
# ReadWritePaths= (an earlier revision had all three). None of the
# reference implementation's own services (bettercap.service,
# pwnagotchi.service, pwngrid-peer.service -- confirmed by reading them
# directly from a real jayofelony image) use ANY systemd sandboxing at
# all; they're plain Type=simple + Restart=always. Our own hardening was
# never adapted from anything proven, and it caused two separate real
# failures on actual hardware (via the tools/rebase-jayofelony pipeline,
# which uses this same unit content, so the bug was universal to every
# image built this session, not specific to the rebase):
# ProtectSystem=strict requires every ReadWritePaths= entry to already
# exist as a bind-mountable path, and broke first on /run/pwnghost
# (tmpfs, wiped every boot -- fixed with RuntimeDirectory=), then, once
# PrivateTmp=yes was also removed for conflicting with it, broke again
# on /var/tmp/pwnghost via the exact same mechanism. Rather than keep
# chasing more paths ProtectSystem=strict might object to, matching what
# real users' images actually run in production is the safer bet.
# RuntimeDirectory= and NoNewPrivileges= are kept -- neither caused any
# issue, and both are still real value without ProtectSystem set.
NoNewPrivileges=yes
RuntimeDirectory=pwnghost
CapabilityBoundingSet=CAP_NET_ADMIN CAP_NET_RAW CAP_SYS_ADMIN CAP_DAC_OVERRIDE CAP_SYS_RESOURCE
AmbientCapabilities=CAP_NET_ADMIN CAP_NET_RAW

# Resource limits. MemoryMax raised from an earlier, untested 50M --
# real memory usage under actual AngryOxide + web UI + RL operation has
# never been measured on hardware yet (every real-hardware run so far
# never got past the sandboxing failures above), and 50M was a guess,
# not a measurement. Better to give real headroom now and tighten later
# once actual usage is known, than risk a silent, hard-to-diagnose
# OOM-kill on top of everything else this session already found.
LimitNOFILE=65536
LimitNPROC=4096
MemoryMax=200M
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
# "wlan0", not "wlan0mon": AngryOxide manages monitor mode itself via
# netlink and never renames the interface (see pwnghost-rs's main.rs) --
# confirmed on real hardware that the old "wlan0mon" arg meant this daemon
# was watching an interface that never existed, and it sat idle for its
# entire run ("wlan_keepalive: stopped (0 total frames)" in its own log).
ExecStart=/usr/local/bin/wlan_keepalive wlan0 100
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
