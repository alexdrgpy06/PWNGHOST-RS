#!/bin/bash -e
# 00-install-artifacts/00-run.sh - Install the pre-built pwnghost-rs binary
# and the pinned AngryOxide release.
#
# This replaces the previous, broken stage4/00-rust-build and
# stage4/00-rust-artifacts sub-stages, which both tried to install a full
# Rust toolchain and `cargo build --workspace` *inside* the pi-gen qemu-arm
# chroot (slow, duplicated each other, and rebuilt the same workspace
# twice). Cross-compilation already happens once, natively, on the x86
# build host via Dockerfile.builder - see that file's `cargo build --release
# --target ... --workspace` steps, which write to
# /workspace/artifacts/<target-triple>/pwnghost-rs. This stage only copies
# that already-built binary into the image and installs AngryOxide.
#
# RUST_TARGET_TRIPLE and ANGRYOXIDE_ARCH are set in pi-gen/config based on
# PWNGHOST_BOARD (pi-zero-w -> arm-unknown-linux-gnueabihf / linux-arm-musl;
# pi-zero-2w -> armv7-unknown-linux-gnueabihf / linux-armv7hf-musl).

RUST_ARTIFACTS_DIR="${RUST_ARTIFACTS_DIR:-/workspace/artifacts}"
ARTIFACT_BIN="${RUST_ARTIFACTS_DIR}/${RUST_TARGET_TRIPLE}/pwnghost-rs"

if [ ! -f "${ARTIFACT_BIN}" ]; then
	echo "stage4: ERROR pre-built binary not found at ${ARTIFACT_BIN}" >&2
	echo "        Expected Dockerfile.builder to have produced it before pi-gen runs." >&2
	echo "        (RUST_TARGET_TRIPLE=${RUST_TARGET_TRIPLE}, RUST_ARTIFACTS_DIR=${RUST_ARTIFACTS_DIR})" >&2
	exit 1
fi

install -d -m 755 "${ROOTFS_DIR}/usr/local/bin"
install -m 755 "${ARTIFACT_BIN}" "${ROOTFS_DIR}/usr/local/bin/pwnghost-rs"
echo "stage4: installed pwnghost-rs (${RUST_TARGET_TRIPLE}) to /usr/local/bin/pwnghost-rs"

# --- wlan_keepalive ----------------------------------------------------------
# Real BCM43436B0 SDIO-bus keepalive daemon, vendored as C source at
# crates/fw-patcher/vendor/wlan_keepalive.c and cross-compiled per-target by
# Dockerfile.builder (arm-linux-gnueabihf-gcc) into the same artifacts dir as
# the pwnghost-rs binary. See crates/fw-patcher/src/keepalive.rs for the
# systemd-unit contract (path, unit name, positional ExecStart args) that
# stage5/00-install-pwnghost installs against this binary.
KEEPALIVE_BIN="${RUST_ARTIFACTS_DIR}/${RUST_TARGET_TRIPLE}/wlan_keepalive"
if [ ! -f "${KEEPALIVE_BIN}" ]; then
	echo "stage4: ERROR wlan_keepalive binary not found at ${KEEPALIVE_BIN}" >&2
	echo "        Expected Dockerfile.builder to have cross-compiled crates/fw-patcher/vendor/wlan_keepalive.c." >&2
	exit 1
fi
install -m 755 "${KEEPALIVE_BIN}" "${ROOTFS_DIR}/usr/local/bin/wlan_keepalive"
echo "stage4: installed wlan_keepalive (${RUST_TARGET_TRIPLE}) to /usr/local/bin/wlan_keepalive"

# --- AngryOxide ------------------------------------------------------------
# Ragnt/AngryOxide ships prebuilt static-musl release tarballs per arch;
# install the binary directly rather than running their install.sh (which
# targets /usr/bin and adds shell completions we don't need).
on_chroot << CHROOT
set -e
export DEBIAN_FRONTEND=noninteractive
apt-get update
apt-get install -y --no-install-recommends curl ca-certificates
AO_URL="https://github.com/Ragnt/AngryOxide/releases/download/${ANGRYOXIDE_VERSION}/angryoxide-${ANGRYOXIDE_ARCH}.tar.gz"
AO_TMP="\$(mktemp -d)"
echo "stage4: downloading \${AO_URL}"
curl -fsSL "\${AO_URL}" -o "\${AO_TMP}/angryoxide.tar.gz"
tar -xzf "\${AO_TMP}/angryoxide.tar.gz" -C "\${AO_TMP}"
AO_BIN="\$(find "\${AO_TMP}" -maxdepth 2 -type f -name angryoxide | head -1)"
if [ -z "\${AO_BIN}" ]; then
    echo "stage4: ERROR angryoxide binary not found inside release tarball" >&2
    exit 1
fi
install -m 755 "\${AO_BIN}" /usr/local/bin/angryoxide
rm -rf "\${AO_TMP}"
/usr/local/bin/angryoxide --version || echo "stage4: WARNING angryoxide --version failed (expected under qemu-user emulation; binary is still installed)"
apt-get clean
rm -rf /var/lib/apt/lists/*
CHROOT

echo "stage4: installed AngryOxide ${ANGRYOXIDE_VERSION} (${ANGRYOXIDE_ARCH}) to /usr/local/bin/angryoxide"
