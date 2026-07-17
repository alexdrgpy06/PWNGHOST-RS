#!/bin/bash -e

# Stage 3: Kernel 6.6 + Nexmon monitor-mode patch
# Builds nexmon patched brcmfmac kernel module

set -e

source "${0%/*}/../common.sh"

on_chroot << 'EOF'
# Install build dependencies
apt-get update
apt-get install -y --no-install-recommends \
    git \
    build-essential \
    linux-headers-$(uname -r) \
    bc \
    bison \
    flex \
    libssl-dev \
    libelf-dev \
    dwarves

# Clone and build nexmon
cd /opt
git clone --depth 1 --branch bookworm https://github.com/seemoo-lab/nexmon.git
cd nexmon

# Setup build environment
source setup_env.sh

# Build for BCM43436 (Pi Zero 2W) and BCM43430 (Pi Zero W)
make -j$(nproc) -C buildtools/isel
make -j$(nproc) -C buildtools/r2c

# Build firmware patching tools
make -j$(nproc) -C utilities/brcmfmac_patcher

# Build kernel modules for both chips
# Pi Zero 2W (BCM43436B0)
make -j$(nproc) -C brcmfmac_4.3.6.0/nexmon/ PI_KERNEL_DIR=/lib/modules/$(uname -r)/build
# Pi Zero W (BCM43430)
make -j$(nproc) -C brcmfmac_4.3.6.0/nexmon/ PI_KERNEL_DIR=/lib/modules/$(uname -r)/build

# Install modules
make -C brcmfmac_4.3.6.0/nexmon/ install PI_KERNEL_DIR=/lib/modules/$(uname -r)/build

# Update initramfs
update-initramfs -u

# Clean build deps (keep kernel headers for runtime)
apt-get remove -y git build-essential bc bison flex libssl-dev libelf-dev dwarves
apt-get autoremove -y
apt-get clean
rm -rf /var/lib/apt/lists/*
EOF