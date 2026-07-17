#!/bin/bash -e

# Stage 2: Base OS - Raspberry Pi OS Bookworm armhf
# This stage creates the base rootfs using debootstrap

set -e

# Source common functions
source "${0%/*}/../common.sh"

# Bookworm armhf base
DEBOOTSTRAP_MIRROR="http://raspbian.raspberrypi.org/raspbian"
DEBOOTSTRAP_COMPONENTS="main contrib non-free non-free-firmware"
DEBOOTSTRAP_EXTRA_PACKAGES="raspberrypi-kernel raspberrypi-bootloader firmware-raspberrypi"

on_chroot << EOF
# Configure apt sources for bookworm
cat > /etc/apt/sources.list << 'SOURCES_EOF'
deb http://raspbian.raspberrypi.org/raspbian/ bookworm main contrib non-free non-free-firmware
deb-src http://raspbian.raspberrypi.org/raspbian/ bookworm main contrib non-free non-free-firmware
SOURCES_EOF

# Add Raspberry Pi specific repo
cat > /etc/apt/sources.list.d/raspi.list << 'RASPI_EOF'
deb http://archive.raspberrypi.org/debian/ bookworm main
# Uncomment line below then 'apt-get update' to enable 'apt-get source'
#deb-src http://archive.raspberrypi.org/debian/ bookworm main
RASPI_EOF

# Import Raspberry Pi signing key
apt-key adv --keyserver keyserver.ubuntu.com --recv-keys 54C3DD61 2>/dev/null || true

# Update and install base packages
apt-get update
apt-get install -y --no-install-recommends \
    raspberrypi-kernel \
    raspberrypi-bootloader \
    firmware-raspberrypi \
    firmware-brcm80211 \
    bluez \
    bluez-tools \
    pi-bluetooth \
    wpasupplicant \
    iw \
    wireless-tools \
    ethtool \
    net-tools \
    iproute2 \
    dhcpcd5 \
    systemd \
    systemd-sysv \
    dbus \
    policykit-1 \
    sudo \
    ca-certificates \
    curl \
    wget \
    gnupg \
    lsb-release \
    raspi-config \
    rpi-eeprom \
    rpi-eeprom-images \
    linux-firmware \
    initramfs-tools \
    keyboard-configuration \
    console-setup \
    locales \
    tzdata \
    fake-hwclock \
    ntp \
    chrony \
    logrotate \
    rsyslog \
    vim-tiny \
    less \
    htop \
    iotop \
    iftop \
    nethogs \
    tcpdump \
    wireshark-common \
    tshark \
    git \
    build-essential \
    pkg-config \
    cmake \
    python3 \
    python3-pip \
    python3-venv \
    lua5.4 \
    liblua5.4-dev \
    pkg-config \
    libssl-dev \
    libdbus-1-dev \
    libudev-dev \
    libpcap-dev \
    libnl-3-dev \
    libnl-genl-3-dev \
    linux-headers-$(uname -r)

# Enable services
systemctl enable ssh
systemctl enable dhcpcd
systemctl enable bluetooth
systemctl enable wpa_supplicant
systemctl enable chrony

# Configure locale
sed -i 's/^# en_US.UTF-8/en_US.UTF-8/' /etc/locale.gen
locale-gen en_US.UTF-8
update-locale LANG=en_US.UTF-8

# Configure timezone
echo "UTC" > /etc/timezone
dpkg-reconfigure -f noninteractive tzdata

# Configure keyboard
sed -i 's/^XKBLAYOUT=.*/XKBLAYOUT="us"/' /etc/default/keyboard
dpkg-reconfigure -f noninteractive keyboard-configuration

# Set hostname
echo "pwnagotchi" > /etc/hostname
sed -i 's/127.0.1.1.*/127.0.1.1\tpwnagotchi/' /etc/hosts

# Configure network interfaces for USB gadget (g_ether)
cat > /etc/network/interfaces.d/usb0 << 'USB_EOF'
allow-hotplug usb0
iface usb0 inet static
    address 10.0.0.1
    netmask 255.255.255.0
    network 10.0.0.0
    broadcast 10.0.0.255
USB_EOF

# Configure cmdline.txt for USB gadget
if ! grep -q "modules-load=dwc2,g_ether" /boot/cmdline.txt; then
    sed -i 's/$/ modules-load=dwc2,g_ether/' /boot/cmdline.txt
fi

# Enable dwc2 overlay
echo "dtoverlay=dwc2" >> /boot/config.txt

# Add g_ether to modules
echo "g_ether" >> /etc/modules

# Create pwnagotchi directories
mkdir -p /etc/pwnagotchi/conf.d
mkdir -p /etc/pwnagotchi/handshakes
mkdir -p /etc/pwnagotchi/log
mkdir -p /usr/local/share/pwnagotchi/custom-plugins
mkdir -p /var/tmp/pwnagotchi

# Set permissions
chown -R pi:pi /etc/pwnagotchi /usr/local/share/pwnagotchi /var/tmp/pwnagotchi

# Clean apt cache
apt-get clean
rm -rf /var/lib/apt/lists/*
EOF