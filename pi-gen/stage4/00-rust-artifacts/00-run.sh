#!/bin/bash -e

# Stage 4: Rust toolchain + cross-compiled artifacts

on_chroot << EOF
# Install Rust
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y --default-toolchain stable
source \$HOME/.cargo/env

# Add cross-compilation targets
rustup target add arm-unknown-linux-gnueabihf
rustup target add armv7-unknown-linux-gnueabihf

# Install cross-compilation tools
apt-get update
apt-get install -y \
    gcc-arm-linux-gnueabihf \
    g++-arm-linux-gnueabihf \
    libc6-dev:armhf \
    libstdc++-12-dev:armhf \
    pkg-config \
    libssl-dev:armhf \
    libudev-dev:armhf \
    libsqlite3-dev:armhf

# Set up cross-compilation environment
cat > ~/.cargo/config.toml << 'CARGO_EOF'
[target.arm-unknown-linux-gnueabihf]
linker = "arm-linux-gnueabihf-gcc"
rustflags = [
  "-C", "link-arg=-fuse-ld=lld",
  "-C", "target-cpu=arm1176jzf-s",
  "-C", "link-arg=-Wl,--as-needed"
]

[target.armv7-unknown-linux-gnueabihf]
linker = "arm-linux-gnueabihf-gcc"
rustflags = [
  "-C", "link-arg=-fuse-ld=lld",
  "-C", "target-cpu=cortex-a53",
  "-C", "link-arg=-Wl,--as-needed"
]

[build]
target = "armv7-unknown-linux-gnueabihf"
CARGO_EOF

# Set up cross-compilation environment
cat > /etc/profile.d/cross-compile.sh << 'PROFILE_EOF'
export PKG_CONFIG_ALLOW_CROSS=1
export PKG_CONFIG_PATH=/usr/lib/arm-linux-gnueabihf/pkgconfig
PROFILE_EOF

EOF