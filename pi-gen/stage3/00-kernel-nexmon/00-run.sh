#!/bin/bash -e

# Stage 3: Kernel + nexmon + firmware

on_chroot << EOF
# Install kernel headers and build dependencies for nexmon
apt-get update
apt-get install -y \
    raspberrypi-kernel-headers \
    build-essential \
    bc \
    bison \
    flex \
    libssl-dev \
    libelf-dev \
    dkms \
    git \
    cmake \
    libncurses5-dev

# Install firmware
apt-get install -y \
    firmware-brcm80211 \
    bluez \
    pi-bluetooth

# Clone and build nexmon
cd /home/$FIRST_USER_NAME
git clone --depth 1 --branch bookworm https://github.com/seemoo-lab/nexmon.git
cd nexmon
source setup_env.sh
make
make install

# Install nexmon binaries to system
cp -r buildtools/ /usr/local/bin/nexmon-buildtools/
cp scripts/*.sh /usr/local/bin/

# Install BCM43436B0 firmware patches
mkdir -p /lib/firmware/brcm/
cp -r firmware/brcmfmac43436-sdio.bin /lib/firmware/brcm/
cp -r firmware/brcmfmac43436-sdio.txt /lib/firmware/brcm/

EOF