#!/bin/bash -e
# 00-kernel-nexmon/00-run.sh - Build nexmon monitor-mode + injection firmware.
#
# Adapted from the working reference at
# rpiproj/stage3/04-nexmon/00-run.sh (DrSchottky/nexmon fork + toolchain,
# same as jayofelony/pwnagotchi). That script previously shipped this
# repo's original (broken) attempt, which cloned a nonexistent
# `seemoo-lab/nexmon` `bookworm` branch and copied firmware from paths that
# never existed - both replaced here.
#
# Firmware-only install: we patch the brcmfmac firmware blobs (which is what
# enables frame injection / deauth) and keep the stock brcmfmac driver,
# which already supports monitor mode via `iw`/netlink on kernel 6.x. This
# deliberately avoids building/replacing the kernel module, so there is no
# kernel-version / vermagic matching to get wrong. AngryOxide (installed in
# stage4) manages monitor mode itself via netlink at runtime - this stage
# only needs to get the right firmware blobs onto the image.
#
# Chip coverage:
#   bcm43430a1  - Pi Zero W, and older Pi Zero 2 W board revisions.
#   bcm43436b0  - Pi Zero 2 W (BCM43436B0 chip revision).
# Both are built below. PWNGHOST_ENABLE_BCM43436B0_PATCH (default: 1) is an
# escape hatch: the upstream reference script we adapted this from had this
# chip's patch DISABLED, with a comment documenting a suspected firmware/
# kernel version mismatch causing hard SDIO/WiFi crashes on that project's
# unpinned-kernel pi-gen build. We don't have hardware here to confirm
# whether that applies to this build too, so it's left on by default but
# trivially toggleable (`-e PWNGHOST_ENABLE_BCM43436B0_PATCH=0` at build
# time) if the same instability shows up during hardware validation.

on_chroot << CHROOT
set -e
export DEBIAN_FRONTEND=noninteractive

apt-get update
# Full nexmon build dependency set (per the nexmon README + jayofelony).
apt-get install -y --no-install-recommends \
    git build-essential gcc-arm-none-eabi \
    gawk xxd qpdf bc \
    autoconf automake libtool texinfo bison flex libfl-dev pkg-config \
    libgmp3-dev libmpfr-dev libmpc-dev libisl-dev zlib1g-dev libssl-dev

# The nexmon b43 assembler links the lex library via the legacy "-ll" flag,
# but modern flex only ships libfl (no libl). Provide a libl.a -> libfl.a
# compatibility symlink so the link succeeds (arch-independent lookup).
LIBFL="\$(find /usr/lib -name 'libfl.a' 2>/dev/null | head -1)"
if [ -n "\$LIBFL" ]; then
    ln -sf "\$LIBFL" "\$(dirname "\$LIBFL")/libl.a"
    echo "nexmon: linked \$(dirname "\$LIBFL")/libl.a -> \$LIBFL"
else
    echo "nexmon: WARNING libfl.a not found; b43 assembler link may fail" >&2
fi

FWDIR=/usr/lib/firmware/brcm
SRC=/usr/local/src/nexmon
rm -rf "\$SRC"
git clone --depth 1 https://github.com/DrSchottky/nexmon.git "\$SRC"

# --- source-level stability patch: disable reload_brcm in stop_monitor_interface ---
# jayofelony/oxigotchi's field-tested fix for a brcmfmac SDIO reset/hang seen
# when the monitor interface is torn down: comment out the reload_brcm call
# inside any stop_monitor_interface() shell function shipped by nexmon's own
# utility scripts, BEFORE building, so the patched behaviour is baked into
# whatever nexmon installs. This is a source patch, not a runtime binary
# patcher (see oxigotchi/tools/apply_patches.sh for the equivalent patch
# applied against a running device's /usr/bin/pwnlib in that project).
# Not every nexmon fork/revision ships a script with this exact function, so
# this is a best-effort scan across the cloned source tree and is a safe
# no-op when no match is found.
REPATCHED=0
while IFS= read -r -d '' f; do
    if grep -q 'stop_monitor_interface' "\$f" 2>/dev/null && grep -q 'reload_brcm' "\$f" 2>/dev/null; then
        sed -i '/stop_monitor_interface/,/^}/ s/^\([[:space:]]*\)reload_brcm\b/\1#reload_brcm  # disabled: SDIO crash fix (pwnghost-rs)/' "\$f"
        echo "nexmon: patched reload_brcm out of stop_monitor_interface in \$f"
        REPATCHED=1
    fi
done < <(find "\$SRC" -type f \( -name '*.sh' -o -name 'Makefile' \) -print0 2>/dev/null)
if [ "\$REPATCHED" = "0" ]; then
    echo "nexmon: no stop_monitor_interface/reload_brcm shell function found in this fork; nothing to patch (expected - this build doesn't ship a reload_brcm helper at all, see stage5 monstart/monstop)"
fi

cd "\$SRC"
# Build the nexmon utilities / flashpatch toolchain (uses gcc-arm-none-eabi).
source ./setup_env.sh
make

install -d -m 755 "\$FWDIR"

build_patch() {
    local chip="\$1" ver="\$2" out="\$3"
    echo "nexmon: building firmware patch \${chip}/\${ver}"
    ( source "\${SRC}/setup_env.sh" && cd "\${SRC}/patches/\${chip}/\${ver}/nexmon" && make )
    local built="\${SRC}/patches/\${chip}/\${ver}/nexmon/\${out}"
    if [ ! -f "\$built" ]; then
        echo "nexmon: ERROR expected firmware \${out} not produced for \${chip}" >&2
        exit 1
    fi
    install -D -m 644 "\$built" "\${FWDIR}/\${out}"
    echo "nexmon: installed \${FWDIR}/\${out}"
}

# Pi Zero W and older Pi Zero 2 W revisions.
build_patch bcm43430a1 7_45_41_46 brcmfmac43430-sdio.bin
cp -f "\${FWDIR}/brcmfmac43430-sdio.bin" "\${FWDIR}/brcmfmac43430b0-sdio.bin"

# Pi Zero 2 W (BCM43436B0). See the header comment above re: the escape
# hatch env var - enabled by default for this build. Substituted from the
# BUILD HOST's environment (not the chroot's) so it can be set via
# \`PWNGHOST_ENABLE_BCM43436B0_PATCH=0 ./build.sh\` or a CI env var.
if [ "${PWNGHOST_ENABLE_BCM43436B0_PATCH:-1}" = "1" ]; then
    build_patch bcm43436b0 9_88_4_65 brcmfmac43436-sdio.bin
    cp -f "\${FWDIR}/brcmfmac43436-sdio.bin" "\${FWDIR}/brcmfmac43436s-sdio.bin"
else
    echo "nexmon: PWNGHOST_ENABLE_BCM43436B0_PATCH=0, skipping bcm43436b0 patch (stock firmware-brcm80211 blob stays in place)"
fi

# Trim build artefacts and the largest build-only package to keep the image
# small; ignore errors so cleanup can't fail the build.
rm -rf "\$SRC"
apt-get purge -y gcc-arm-none-eabi 2>/dev/null || true
apt-get autoremove -y 2>/dev/null || true
apt-get clean
rm -rf /var/lib/apt/lists/*

echo "nexmon: firmware install complete"
CHROOT
