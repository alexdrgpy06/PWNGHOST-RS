#!/bin/bash -e

# Stage 4: Rust toolchain + cross-compile all crates

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
    gcc-arm-linux-gnueabihf \
    libc6-dev-armhf-cross \
    libstdc++-12-dev-armhf-cross \
    pkg-config \
    libssl-dev:armhf \
    libudev-dev:armhf \
    libsqlite3-dev:armhf

# Set up cross-compilation environment
cat > ~/.cargo/config.toml << 'CARGO_EOF'
[target.arm-unknown-linux-gnueabihf]
linker = "arm-linux-gnueabihf-gcc"
rustflags = ["-C", "link-arg=-fuse-ld=lld"]

[target.armv7-unknown-linux-gnueabihf]
linker = "arm-linux-gnueabihf-gcc"
rustflags = ["-C", "link-arg=-fuse-ld=lld"]
CARGO_EOF

# Create build script
cat > /home/$FIRST_USER_NAME/build_pwnghost.sh << 'BUILD_EOF'
#!/bin/bash
set -e

cd /home/$FIRST_USER_NAME/pwnghost-rs

# Cross-compile for Pi Zero W (ARMv6)
cargo build --release --target arm-unknown-linux-gnueabihf --workspace

# Cross-compile for Pi Zero 2W (ARMv7)
cargo build --release --target armv7-unknown-linux-gnueabihf --workspace

# Copy binaries to staging
mkdir -p /home/$FIRST_USER_NAME/pwnghost-rs/build/armv6
mkdir -p /home/$FIRST_USER_NAME/pwnghost-rs/build/armv7

cp target/arm-unknown-linux-gnueabihf/release/pwnghost-rs /home/$FIRST_USER_NAME/pwnghost-rs/build/armv6/
cp target/armv7-unknown-linux-gnueabihf/release/pwnghost-rs /home/$FIRST_USER_NAME/pwnghost-rs/build/armv7/

BUILD_EOF

chmod +x /home/$FIRST_USER_NAME/build_pwnghost.sh
chown $FIRST_USER_NAME:$FIRST_USER_NAME /home/$FIRST_USER_NAME/build_pwnghost.sh

EOF