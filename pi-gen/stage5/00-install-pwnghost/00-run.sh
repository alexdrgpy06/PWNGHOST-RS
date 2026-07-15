#!/bin/bash -e

# Stage 5: Install pwnghost-rs + systemd units + config

on_chroot << EOF
# Install built binaries
mkdir -p /usr/local/bin/
cp /home/$FIRST_USER_NAME/pwnghost-rs/build/armv7/pwnghost-rs /usr/local/bin/
chmod +x /usr/local/bin/pwnghost-rs

# Create config directory
mkdir -p /etc/pwnghost/conf.d
mkdir -p /etc/pwnghost/handshakes
mkdir -p /var/log/pwnghost
mkdir -p /var/tmp/pwnghost

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

# Install monstart/monstop scripts for monitor mode
cat > /usr/bin/monstart << 'MONSTART_EOF'
#!/bin/bash
# Start monitor mode using nexmon

INTERFACE="${1:-wlan0}"
MON_INTERFACE="${INTERFACE}mon"

# Bring down interface
ip link set $INTERFACE down 2>/dev/null || true

# Set monitor mode
iw dev $INTERFACE set type monitor 2>/dev/null || {
    # Fallback to nexmon monstart
    /usr/local/bin/nexmon-buildtools/monstart $INTERFACE
}

# Bring up monitor interface
ip link set $MON_INTERFACE up 2>/dev/null || true

echo "Monitor mode started on $MON_INTERFACE"
MONSTART_EOF

cat > /usr/bin/monstop << 'MONSTOP_EOF'
#!/bin/bash
# Stop monitor mode

INTERFACE="${1:-wlan0}"
MON_INTERFACE="${INTERFACE}mon"

# Bring down monitor interface
ip link set $MON_INTERFACE down 2>/dev/null || true

# Set back to managed mode
iw dev $MON_INTERFACE set type managed 2>/dev/null || {
    /usr/local/bin/nexmon-buildtools/monstop $INTERFACE
}

# Bring up managed interface
ip link set $INTERFACE up 2>/dev/null || true

echo "Monitor mode stopped, $INTERFACE back to managed"
MONSTOP_EOF

chmod +x /usr/bin/monstart /usr/bin/monstop

# Install wlan_keepalive daemon
cat > /usr/local/bin/wlan_keepalive << 'KEEPALIVE_EOF'
#!/bin/bash
# WiFi keepalive daemon for BCM43436B0 stability

INTERFACE="${1:-wlan0mon}"
PING_TARGET="8.8.8.8"
INTERVAL=30

while true; do
    if ! ping -c 1 -W 2 -I $INTERFACE $PING_TARGET >/dev/null 2>&1; then
        # Try to reset interface
        ip link set $INTERFACE down
        sleep 1
        ip link set $INTERFACE up
        sleep 2
    fi
    sleep $INTERVAL
done
KEEPALIVE_EOF

chmod +x /usr/local/bin/wlan_keepalive

# Create systemd service for keepalive
cat > /etc/systemd/system/wlan-keepalive.service << 'KA_EOF'
[Unit]
Description=WiFi Keepalive for BCM43436B0
After=network.target
Wants=network.target

[Service]
Type=simple
ExecStart=/usr/local/bin/wlan_keepalive wlan0mon
Restart=always
RestartSec=5
StandardOutput=journal
StandardError=journal

# Security hardening
NoNewPrivileges=yes
PrivateTmp=yes
ProtectSystem=strict
ProtectHome=yes
ReadWritePaths=/run/wlan_keepalive

[Install]
WantedBy=multi-user.target
KA_EOF

# Enable services
systemctl enable pwnghost-rs.service
systemctl enable wlan-keepalive.service

# Set permissions
chown -R pwn:pwn /etc/pwnghost /var/log/pwnghost /var/tmp/pwnghost

EOF