#!/bin/bash -e

# Stage 2: Base OS (bookworm armhf)

on_chroot << EOF
# Update and install base packages
apt-get update
apt-get install -y \
    sudo \
    curl \
    wget \
    git \
    vim \
    htop \
    iw \
    wireless-tools \
    net-tools \
    iputils-ping \
    dnsutils \
    usbutils \
    pciutils \
    lsb-release \
    ca-certificates \
    gnupg \
    software-properties-common

# Create pwn user
useradd -m -s /bin/bash -G sudo,plugdev,video,i2c,spi,gpio,input $FIRST_USER_NAME
echo "$FIRST_USER_NAME:$FIRST_USER_PASS" | chpasswd

# Configure sudo without password for pwn user
echo "$FIRST_USER_NAME ALL=(ALL) NOPASSWD:ALL" > /etc/sudoers.d/010_pwn-nopasswd
EOF