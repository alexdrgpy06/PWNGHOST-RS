# Dockerfile for building PWNGHOST-RS SD card image using pi-gen
# Based on Debian bookworm (Raspberry Pi OS base) - has proper armhf packages

FROM debian:bookworm-slim

# Install pi-gen dependencies
RUN apt-get update && apt-get install -y \
    binfmt-support \
    btrfs-progs \
    coreutils \
    cpio \
    curl \
    dosfstools \
    e2fsprogs \
    fdisk \
    file \
    gawk \
    git \
    grep \
    gzip \
    kpartx \
    libarchive-tools \
    libcap2-bin \
    liblz4-tool \
    openssl \
    parted \
    pigz \
    policycoreutils \
    python3 \
    python3-apt \
    python3-distro \
    python3-jinja2 \
    python3-pexpect \
    python3-psutil \
    python3-yaml \
    qemu-user-static \
    rsync \
    sed \
    sudo \
    systemd-container \
    tar \
    unzip \
    util-linux \
    wget \
    xz-utils \
    # Build tools for cross-compilation
    build-essential \
    gcc-arm-linux-gnueabihf \
    g++-arm-linux-gnueabihf \
    libc6-dev:armhf \
    libstdc++-12-dev:armhf \
    pkg-config \
    libssl-dev:armhf \
    libudev-dev:armhf \
    libsqlite3-dev:armhf \
    && rm -rf /var/lib/apt/lists/*

# Install Rust for cross-compilation
RUN curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y --default-toolchain stable \
    && . "$HOME/.cargo/env" \
    && rustup target add arm-unknown-linux-gnueabihf armv7-unknown-linux-gnueabihf

# Set up cross-compilation environment
ENV CARGO_HOME=/root/.cargo
ENV PATH=/root/.cargo/bin:$PATH
ENV PKG_CONFIG_ALLOW_CROSS=1
ENV PKG_CONFIG_PATH=/usr/lib/arm-linux-gnueabihf/pkgconfig

WORKDIR /workspace

# Copy entire workspace
COPY . /workspace

# Build Rust workspace for both targets
RUN . "$HOME/.cargo/env" && \
    cargo build --release --target arm-unknown-linux-gnueabihf --workspace && \
    cargo build --release --target armv7-unknown-linux-gnueabihf --workspace

# Copy built binaries to a known location for pi-gen stages
RUN mkdir -p /workspace/artifacts/arm-unknown-linux-gnueabihf /workspace/artifacts/armv7-unknown-linux-gnueabihf && \
    cp /workspace/target/arm-unknown-linux-gnueabihf/release/pwnghost-rs /workspace/artifacts/arm-unknown-linux-gnueabihf/ && \
    cp /workspace/target/armv7-unknown-linux-gnueabihf/release/pwnghost-rs /workspace/artifacts/armv7-unknown-linux-gnueabihf/

# Cross-compile the vendored wlan_keepalive C daemon (crates/fw-patcher/vendor/
# wlan_keepalive.c - a real BCM43436B0 SDIO-bus keepalive, ported verbatim
# from oxigotchi/tools/wlan_keepalive.c; see crates/fw-patcher/src/keepalive.rs
# for the systemd-unit contract pi-gen's stage5 installs it under). Plain C,
# no extra deps, so the same armhf cross-gcc used nowhere else in this
# Dockerfile otherwise is enough - one binary per target, using the same
# -mcpu split already used for the Rust rustflags (arm1176jzf-s for the
# ARMv6 Pi Zero W target, cortex-a53 for the ARMv7 Pi Zero 2W target).
RUN arm-linux-gnueabihf-gcc -O2 -mcpu=arm1176jzf-s -mfpu=vfp -mfloat-abi=hard \
      -o /workspace/artifacts/arm-unknown-linux-gnueabihf/wlan_keepalive \
      /workspace/crates/fw-patcher/vendor/wlan_keepalive.c && \
    arm-linux-gnueabihf-gcc -O2 -mcpu=cortex-a53 -mfpu=neon-vfpv4 -mfloat-abi=hard \
      -o /workspace/artifacts/armv7-unknown-linux-gnueabihf/wlan_keepalive \
      /workspace/crates/fw-patcher/vendor/wlan_keepalive.c

# Build the SD card image
CMD ["/bin/bash", "-c", "cd /workspace/pi-gen && ./build.sh"]