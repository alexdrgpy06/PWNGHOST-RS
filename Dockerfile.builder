# Dockerfile for building PWNGHOST-RS SD card image using pi-gen
# Based on Debian bookworm (Raspberry Pi OS base) - has proper armhf packages

FROM debian:bookworm-slim

# Cross-arch (armhf) packages below aren't visible to apt until the foreign
# architecture is registered -- without this, apt-get install just reports
# "Unable to locate package" for every ":armhf" package (confirmed by a real
# CI failure).
RUN dpkg --add-architecture armhf

# Install pi-gen dependencies. This list is meant to satisfy
# pi-gen/scripts/dependencies_check against pi-gen/depends -- it was hand-
# assembled by guesswork rather than derived from that file, and a real
# build.sh run surfaced 9 genuinely missing tools: quilt, qemu-user-binfmt,
# debootstrap, zerofree, zip, xxd, kmod, bc, arch-test.
#
# qemu-user-static is deliberately NOT installed: pi-gen/depends wants the
# `qemu-arm` binary from qemu-user-binfmt (this vendored pi-gen uses
# binfmt_misc's persistent-interpreter registration rather than copying a
# static qemu binary into the target rootfs), and the two packages Conflict:
# at the dpkg level -- confirmed by a real `apt-get install` failure
# ("you have held broken packages") when both were listed.
RUN apt-get update && apt-get install -y \
    arch-test \
    bc \
    binfmt-support \
    btrfs-progs \
    coreutils \
    cpio \
    curl \
    debootstrap \
    dosfstools \
    e2fsprogs \
    fdisk \
    file \
    gawk \
    git \
    grep \
    gzip \
    kmod \
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
    qemu-user-binfmt \
    quilt \
    rsync \
    sed \
    sudo \
    systemd-container \
    tar \
    unzip \
    util-linux \
    wget \
    xxd \
    xz-utils \
    zerofree \
    zip \
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
# Without an explicit target linker, cargo falls back to the host's plain
# `cc` to link ARM object files, which fails ("incompatible with
# elf64-x86-64") -- confirmed by a real local Docker build. .github/workflows/
# test.yml's cross-compile-check job already sets these; Dockerfile.builder
# never did.
ENV CARGO_TARGET_ARM_UNKNOWN_LINUX_GNUEABIHF_LINKER=arm-linux-gnueabihf-gcc
ENV CARGO_TARGET_ARMV7_UNKNOWN_LINUX_GNUEABIHF_LINKER=arm-linux-gnueabihf-gcc
ENV CC_arm_unknown_linux_gnueabihf=arm-linux-gnueabihf-gcc
ENV CC_armv7_unknown_linux_gnueabihf=arm-linux-gnueabihf-gcc
ENV CXX_arm_unknown_linux_gnueabihf=arm-linux-gnueabihf-g++
ENV CXX_armv7_unknown_linux_gnueabihf=arm-linux-gnueabihf-g++
ENV PKG_CONFIG_PATH=/usr/lib/arm-linux-gnueabihf/pkgconfig
# rustc's thin-LTO codegen (crates/../Cargo.toml [profile.release]) merges
# codegen units at link time -- left at cargo's default (one job per
# visible CPU), this spiked peak RSS enough to crash the host machine on a
# real local build even with Docker Desktop's WSL2 VM already capped in
# .wslconfig (thrashing the VM's swap file froze the whole host rather
# than cleanly OOM-killing the container). Capping parallelism trades some
# build wall-clock time for a peak memory footprint that actually fits.
ENV CARGO_BUILD_JOBS=2

WORKDIR /workspace

# Copy entire workspace
COPY . /workspace

# Build Rust workspace for both targets, with the real-hardware features
# enabled (e-ink SPI/GPIO output, GPIO WiFi-chip power-cycle) -- without
# these, pwnghost-rs falls back to no-op display/GPIO backends and silently
# does nothing on real hardware (confirmed the hard way: shipped without
# them in the first image, the e-ink stayed blank). package/feature syntax
# is used rather than a bare --features flag so this doesn't depend on how
# a given cargo version resolves --features against --workspace when only
# some member crates define the feature.
RUN . "$HOME/.cargo/env" && \
    cargo build --release --target arm-unknown-linux-gnueabihf --workspace \
        --features pwnghost-rs/hardware --features pwnghost-rs/linux-gpio && \
    cargo build --release --target armv7-unknown-linux-gnueabihf --workspace \
        --features pwnghost-rs/hardware --features pwnghost-rs/linux-gpio

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
# -marm forces ARM (not Thumb) instruction encoding: Debian's cross-gcc
# defaults to Thumb, and arm1176jzf-s (ARMv6) only has Thumb-1, which can't
# express the gnueabihf hard-float VFP calling convention ("sorry,
# unimplemented: Thumb-1 'hard-float' VFP ABI" -- confirmed by a real local
# build). Applied to both targets for consistency, though only the ARMv6
# one is strictly required.
RUN arm-linux-gnueabihf-gcc -O2 -marm -mcpu=arm1176jzf-s -mfpu=vfp -mfloat-abi=hard \
      -o /workspace/artifacts/arm-unknown-linux-gnueabihf/wlan_keepalive \
      /workspace/crates/fw-patcher/vendor/wlan_keepalive.c && \
    arm-linux-gnueabihf-gcc -O2 -marm -mcpu=cortex-a53 -mfpu=neon-vfpv4 -mfloat-abi=hard \
      -o /workspace/artifacts/armv7-unknown-linux-gnueabihf/wlan_keepalive \
      /workspace/crates/fw-patcher/vendor/wlan_keepalive.c

# Build the SD card image.
#
# binfmt_misc registration (docker/setup-qemu-action, or tonistiigi/binfmt)
# happens on the HOST/runner and doesn't imply the pseudo-filesystem is
# mounted inside *this* container's own mount namespace -- `--privileged`
# grants the capability but doesn't bind the mount in automatically.
# pi-gen's dependencies_check greps /proc/mounts for a literal
# "binfmt_misc" mount and fails otherwise ("Module binfmt_misc not loaded
# in host"), confirmed by a real CI run where the mount was missing even
# with setup-qemu-action already run. Mount it explicitly, ignoring
# failure in case the host already exposes it (some environments do).
CMD ["/bin/bash", "-c", "mount -t binfmt_misc binfmt_misc /proc/sys/fs/binfmt_misc 2>/dev/null; cd /workspace/pi-gen && ./build.sh"]