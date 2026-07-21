#!/bin/bash -e
# build.sh - Rebase PWNGHOST-RS onto a jayofelony/pwnagotchi release image
# (see the BASE_VERSION case statement below for supported versions and
# why each is a candidate), instead of building the OS from scratch via
# pi-gen.
#
# Why: this project's own from-scratch pi-gen build has spent real
# engineering effort this session getting USB gadget networking right; a
# real, actively-maintained pwnagotchi image already has that solved
# along with real nexmon monitor-mode/injection support for both
# BCM43430 (Pi Zero W) and BCM43436B0 (Pi Zero 2W) -- confirmed directly
# by decompressing and inspecting the actual kernel module, not inferred
# from a package name (see tools/rebase-jayofelony/README.md for the
# full findings, including which base versions actually boot on real
# hardware -- that's still being narrowed down as of this revision).
# Rather than re-deriving that hardware-enablement layer, this script
# keeps it untouched and strips out everything specific to the Python
# pwnagotchi/bettercap/pwngrid stack, replacing it with our own compiled
# pwnghost-rs binary and systemd units.
#
# This does NOT replace or modify the existing pi-gen build in any way --
# it produces a separate, alternate image so both can be compared.
#
# Usage: BOARD=pi-zero-w BASE_VERSION=2.8.9 ./build.sh
#   BOARD: pi-zero-w or pi-zero-2w
#   BASE_VERSION: 2.9.5.3 (default) or 2.8.9
# Requires: an already cross-compiled artifacts/ dir from Dockerfile.builder
#   (artifacts/arm-unknown-linux-gnueabihf/{pwnghost-rs,wlan_keepalive} and
#   artifacts/armv7-unknown-linux-gnueabihf/{pwnghost-rs,wlan_keepalive}).
# Must run inside the Docker image built from this directory's Dockerfile
# (privileged, for loop-mount + chroot).

set -euo pipefail

# Bullseye only -- 2.9.5.3 (Bookworm) has confirmed real-hardware boot
# problems on both boards (kernel panic on Pi Zero 2W, blank-HDMI hang on
# Pi Zero W, see the BASE_VERSION case statement below) and is never the
# right default despite being what jayofelony's own repo currently tags
# "latest". 2.8.9 (32-bit bullseye) is the confirmed-working base for
# both boards; bullseye64/2.6.4 is the opt-in 64-bit variant for
# pi-zero-2w only, never picked by default.
BASE_VERSION="${BASE_VERSION:-2.8.9}"

# 64-bit bases (currently just bullseye64) are only viable on Pi Zero 2W
# hardware -- Pi Zero W's BCM2835/ARM1176JZF-S is ARMv6, 32-bit only, and
# physically cannot boot an aarch64 kernel at all. Checked here, early,
# so a bad BOARD/BASE_VERSION combination fails fast instead of partway
# through a multi-minute image download.
case "$BASE_VERSION" in
    bullseye64) IS_64BIT=1 ;;
    *) IS_64BIT=0 ;;
esac

# Docker containers typically only pre-populate a handful of /dev/loopN
# device nodes -- `losetup -f` happily reports the next free NAME (e.g.
# /dev/loop9) even when that node doesn't actually exist yet, and the
# subsequent attach fails with "No such file or directory" (confirmed by
# a real first run of this script). This needs two loop devices at once
# (boot + root), so make sure enough nodes exist up front rather than
# assuming the container's defaults are enough.
for i in $(seq 0 15); do
    [ -e "/dev/loop$i" ] || mknod -m 660 "/dev/loop$i" b 7 "$i"
done

# binfmt_misc is a kernel-global facility: registering qemu-arm at Docker
# *build* time (this image's Dockerfile installs qemu-user-static via
# apt) does not persist into a later, separate `docker run` -- each run
# gets a fresh mount namespace, and the registration from the build-time
# container is long gone. Confirmed the hard way on a real first run:
# the strip/splice chroot steps got most of the way through, then failed
# with "qemu-arm: Could not open '/lib/ld-linux-armhf.so.3'" the moment a
# later chroot command executed an armhf binary. Re-register at runtime,
# every run, rather than assuming the image already has it.
mount -t binfmt_misc binfmt_misc /proc/sys/fs/binfmt_misc 2>/dev/null || true
update-binfmt --remove qemu-arm 2>/dev/null || true
update-binfmt --enable qemu-arm 2>/dev/null || true
# Same story for aarch64, only actually needed for a bullseye64 build --
# harmless no-op registration otherwise.
update-binfmt --remove qemu-aarch64 2>/dev/null || true
update-binfmt --enable qemu-aarch64 2>/dev/null || true

# `update-binfmt` isn't guaranteed to exist (confirmed on real hardware:
# missing entirely in one run despite having worked in a prior run of
# the exact same image -- Docker Desktop's own WSL2 VM can already have
# ARM emulation registered under a *different* name, e.g. `arm` instead
# of `qemu-arm`, via its own bundled binfmt provisioning, which survives
# VM restarts independently of anything this script does). Accept
# whichever handler is actually present rather than hardcoding one name.
BOARD="${BOARD:?Set BOARD=pi-zero-w or BOARD=pi-zero-2w}"
if [ "$IS_64BIT" = "1" ] && [ "$BOARD" != "pi-zero-2w" ]; then
    echo "build.sh: BASE_VERSION='$BASE_VERSION' is a 64-bit base, only supported on BOARD=pi-zero-2w (got BOARD='$BOARD')" >&2
    exit 1
fi
case "$BOARD" in
    pi-zero-w)  RUST_TARGET="arm-unknown-linux-gnueabihf" ;;
    pi-zero-2w)
        if [ "$IS_64BIT" = "1" ]; then
            RUST_TARGET="aarch64-unknown-linux-gnu"
        else
            RUST_TARGET="armv7-unknown-linux-gnueabihf"
        fi
        ;;
    *) echo "build.sh: unknown BOARD='$BOARD' (expected pi-zero-w or pi-zero-2w)" >&2; exit 1 ;;
esac

# Candidate binfmt handler names differ by target architecture -- an
# aarch64 chroot needs qemu-aarch64 registered, not qemu-arm, and vice
# versa. Same "Docker Desktop's WSL2 VM may already have this registered
# under a short name" caveat as before applies to both families.
if [ "$IS_64BIT" = "1" ]; then
    BINFMT_CANDIDATES="qemu-aarch64 aarch64"
    BINFMT_INTERPRETER_DEFAULT="/usr/bin/qemu-aarch64-static"
else
    BINFMT_CANDIDATES="qemu-arm arm"
    BINFMT_INTERPRETER_DEFAULT="/usr/bin/qemu-arm-static"
fi

BINFMT_HANDLER=""
for candidate in $BINFMT_CANDIDATES; do
    if [ -e "/proc/sys/fs/binfmt_misc/$candidate" ]; then
        BINFMT_HANDLER="$candidate"
        break
    fi
done
if [ -z "$BINFMT_HANDLER" ]; then
    echo "build.sh: no binfmt_misc handler registered for this target (checked: $BINFMT_CANDIDATES) -- chroot commands will not run" >&2
    exit 1
fi
echo "build.sh: using binfmt handler '$BINFMT_HANDLER'"

# The registered handler's own `interpreter` path is whatever *that*
# registration says (Docker Desktop's own `arm`/`aarch64` handlers point
# at `/usr/bin/qemu-arm`/`/usr/bin/qemu-aarch64`, not the `-static`
# filenames this image's Dockerfile installs). Whatever it is, that exact
# path has to exist inside the chroot too, since the kernel resolves it
# relative to the executing process's own root, not this container's.
# Read it dynamically instead of assuming a name.
BINFMT_INTERPRETER="$(awk '/^interpreter /{print $2}' "/proc/sys/fs/binfmt_misc/$BINFMT_HANDLER")"
if [ -z "$BINFMT_INTERPRETER" ]; then
    BINFMT_INTERPRETER="$BINFMT_INTERPRETER_DEFAULT"
fi

WORK_DIR="${WORK_DIR:-/work}"
ARTIFACTS_DIR="${ARTIFACTS_DIR:-$WORK_DIR/artifacts}"
OVERLAY_DIR="${OVERLAY_DIR:-$WORK_DIR/overlay}"

# RAW_IMG/BOOT_MNT/ROOT_MNT below are fixed paths under WORK_DIR, not
# scoped per board/run -- two `build.sh` invocations against the same
# WORK_DIR (e.g. a manual run and an unrelated automated/agent-driven one
# kicked off around the same time) will silently race on the same
# base.img and mount points, and on `losetup -f` picking loop devices
# from the same shared host kernel table under --privileged. One run's
# EXIT cleanup trap unmounting things out from under the other's rsync
# mid-copy reproduces exactly the "overlay files silently missing after
# rsync" failure this was debugged from -- confirmed as the likely cause,
# not reproduced with a byte-for-byte-identical single run once nothing
# else was touching WORK_DIR. Fail fast and loud instead of racing.
LOCK_FILE="$WORK_DIR/.build.lock"
exec 9>"$LOCK_FILE"
if ! flock -n 9; then
    echo "build.sh: another build is already running against WORK_DIR=$WORK_DIR (lock: $LOCK_FILE) -- refusing to race it. Wait for it to finish, or use a separate WORK_DIR." >&2
    exit 1
fi

# Which jayofelony release to rebase onto. Real hardware testing across
# several candidates so far (all confirmed to have genuine nexmon --
# decompressed and grepped the *active* brcmfmac.ko directly, not
# inferred from a package name, since that's the one check that's
# actually proven reliable across versions):
#   - v2.9.5.4 (Trixie, the original base): hangs black-screen/no-LED on
#     Pi Zero W, and kernel panics on Pi Zero 2W were also seen on the
#     v2.9.5.3-rebased image, so this whole 2.9.x lineage's real-hardware
#     compatibility on these two boards is still unresolved, not just a
#     v2.9.5.4-vs-Trixie problem as first assumed.
#   - v2.9.5.3 (Bookworm): built and shipped as rebased-v2, but the user
#     saw a kernel panic on Pi Zero 2W and a stuck/blank-HDMI hang after
#     the first-boot user/pass prompt on Pi Zero W -- not yet confirmed
#     whether that's this base image itself or something in this script's
#     modifications; still an open question.
#   - v2.8.9 (bullseye): the user's own direct prior experience is that
#     this one actually boots and runs on this exact hardware. Different
#     install layout from the two above -- pwnagotchi is a system-wide
#     pip install (/usr/local/lib/python3.9/dist-packages/pwnagotchi),
#     not an isolated venv, and there's no Go/Rust toolchain to strip at
#     all (confirmed absent, not just untested).
#   - bullseye64 (v2.6.4, from the separate jayofelony/pwnagotchi-bullseye
#     repo, NOT jayofelony/pwnagotchi -- that repo's own v2.8.9 also has a
#     "-64bit" asset but it's built on Bookworm despite the version number
#     matching the 32-bit bullseye one, confirmed by mounting it directly:
#     /etc/os-release there is bookworm, not bullseye, so it doesn't
#     avoid whatever's causing the v2.9.5.3/Bookworm boot problems above.
#     v2.6.4 is the *last* pwnagotchi-bullseye release with a real arm64
#     asset -- v2.8.4, that repo's actual latest, dropped back to armhf-
#     only. Pi Zero 2W-only: Pi Zero W's ARMv6 SoC cannot run an aarch64
#     kernel at all, checked earlier via the IS_64BIT/BOARD guard. Directly
#     confirmed by mounting the real image (read-only loop mount, same
#     method as every other version note here): genuine Debian 11
#     bullseye, kernel 6.1.21-v8+, brcmfmac.ko has real nexmon patches
#     (grepped nexmon_nl_ioctl_handler / brcmf_cfg80211_nexmon_set_channel
#     / nexmon/patches/driver/brcmfmac_6.1.y-nexmon/*.c paths out of the
#     active module directly, not inferred), pwnagotchi is the same
#     system-wide pip install layout as v2.8.9 above (not a venv), same
#     three systemd units (bettercap/pwnagotchi/pwngrid-peer), and
#     cmdline.txt/config.txt already carry dwc2/g_ether same as the
#     others. Real-hardware boot validation on Pi Zero 2W is still
#     pending -- this is a currently-untested candidate being added
#     because the user's own separate hands-on testing indicates a
#     bullseye64 image boots reliably where 2.9.5.3/Bookworm didn't, not
#     because this pipeline has proven it end to end yet.
case "$BASE_VERSION" in
    2.9.5.3)
        IMG_XZ="$WORK_DIR/pwnagotchi-2.9.5.3-32bit.img.xz"
        IMG_URL="https://github.com/jayofelony/pwnagotchi/releases/download/v2.9.5.3/pwnagotchi-2.9.5.3-32bit.img.xz"
        IMG_SHA256="e2f691a4b974afeffe05071b42a0c14e54b127aa491a9d54de9820f6bd2df69b"
        ;;
    2.8.9)
        IMG_XZ="$WORK_DIR/pwnagotchi-2.8.9-32bit.img.xz"
        IMG_URL="https://github.com/jayofelony/pwnagotchi/releases/download/v2.8.9/pwnagotchi-2.8.9-32bit.img.xz"
        IMG_SHA256="030c7d759cd130ef00bd6e8e741461b9aaea1f013c1a6be9eb2e87062066aa0f"
        ;;
    bullseye64)
        IMG_XZ="$WORK_DIR/pwnagotchi-rpi-bullseye-2.6.4-arm64.img.xz"
        IMG_URL="https://github.com/jayofelony/pwnagotchi-bullseye/releases/download/v2.6.4/pwnagotchi-rpi-bullseye-2.6.4-arm64.img.xz"
        IMG_SHA256="ba21a1ee196f5bb8171a0932329e7fab478ffd7fb19f84412e6df96670f90299"
        ;;
    *)
        echo "build.sh: unknown BASE_VERSION='$BASE_VERSION' (expected 2.9.5.3, 2.8.9, or bullseye64)" >&2
        exit 1
        ;;
esac

RAW_IMG="$WORK_DIR/base.img"
BOOT_MNT="$WORK_DIR/mnt-boot"
ROOT_MNT="$WORK_DIR/mnt-root"
# BASE_VERSION is part of the output name -- without it, building
# BASE_VERSION=2.8.9 for a board silently overwrote a previous
# BASE_VERSION=2.9.5.3 build for the same board (confirmed the hard way:
# lost track of which build was actually being tested on real hardware).
OUT_IMG="$WORK_DIR/pwnghost-rs-rebased-${BOARD}-${BASE_VERSION}.img"
OUT_XZ="$WORK_DIR/pwnghost-rs-rebased-${BOARD}-${BASE_VERSION}.img.xz"

log() { echo -e "\e[32m=== $* ===\e[0m"; }

# --- 1. Download + verify -------------------------------------------------
if [ ! -f "$IMG_XZ" ]; then
    log "Downloading base image"
    curl -L -o "$IMG_XZ" "$IMG_URL"
fi
log "Verifying checksum"
echo "${IMG_SHA256}  ${IMG_XZ}" | sha256sum -c -

log "Decompressing"
rm -f "$RAW_IMG"
xz -dk -c "$IMG_XZ" > "$RAW_IMG"

# --- 2. Parse MBR, extract partition offsets ------------------------------
log "Parsing partition table"
read -r BOOT_OFFSET BOOT_SIZE ROOT_OFFSET ROOT_SIZE <<< "$(python3 - "$RAW_IMG" <<'PYEOF'
import struct, sys
path = sys.argv[1]
with open(path, 'rb') as f:
    mbr = f.read(512)
parts = []
for i in range(4):
    e = mbr[446 + i*16 : 446 + (i+1)*16]
    ptype = e[4]
    if ptype == 0:
        continue
    lba = struct.unpack('<I', e[8:12])[0]
    sectors = struct.unpack('<I', e[12:16])[0]
    parts.append((ptype, lba * 512, sectors * 512))
# boot = FAT partition (type 0x0c/0x0b/0x06), root = Linux (type 0x83)
boot = next(p for p in parts if p[0] in (0x0c, 0x0b, 0x06))
root = next(p for p in parts if p[0] == 0x83)
print(boot[1], boot[2], root[1], root[2])
PYEOF
)"
log "boot: offset=$BOOT_OFFSET size=$BOOT_SIZE / root: offset=$ROOT_OFFSET size=$ROOT_SIZE"

# --- 3. Loop-mount both partitions, writable ------------------------------
# The cleanup trap is registered BEFORE any loop/mount operation begins
# (not after, as an earlier revision had it) so a failure partway through
# this setup -- confirmed to happen for real: losetup failing on the
# second device if the container didn't have enough /dev/loopN nodes --
# still tears down whatever had already been attached, rather than
# leaking a mounted loop device out of a script that already exited.
BOOT_LOOP=""
ROOT_LOOP=""
cleanup() {
    set +e
    umount "$ROOT_MNT/proc" 2>/dev/null
    umount "$ROOT_MNT/sys" 2>/dev/null
    umount "$ROOT_MNT/dev" 2>/dev/null
    umount "$BOOT_MNT" 2>/dev/null
    umount "$ROOT_MNT" 2>/dev/null
    [ -n "$BOOT_LOOP" ] && losetup -d "$BOOT_LOOP" 2>/dev/null
    [ -n "$ROOT_LOOP" ] && losetup -d "$ROOT_LOOP" 2>/dev/null
}
trap cleanup EXIT

mkdir -p "$BOOT_MNT" "$ROOT_MNT"
BOOT_LOOP="$(losetup -f)"
losetup -o "$BOOT_OFFSET" --sizelimit "$BOOT_SIZE" "$BOOT_LOOP" "$RAW_IMG"
ROOT_LOOP="$(losetup -f)"
losetup -o "$ROOT_OFFSET" --sizelimit "$ROOT_SIZE" "$ROOT_LOOP" "$RAW_IMG"
mount "$BOOT_LOOP" "$BOOT_MNT"
mount "$ROOT_LOOP" "$ROOT_MNT"

# --- 4. Set up chroot (qemu-{arm,aarch64}-static, depending on target) ----
log "Setting up chroot"
QEMU_STATIC_BIN="/usr/bin/qemu-arm-static"
[ "$IS_64BIT" = "1" ] && QEMU_STATIC_BIN="/usr/bin/qemu-aarch64-static"
cp "$QEMU_STATIC_BIN" "$ROOT_MNT$QEMU_STATIC_BIN"
# Also place it at whatever path the active binfmt registration's own
# `interpreter` field points to, if that's a different filename (see the
# binfmt handler detection above) -- the kernel resolves that path
# relative to the chroot's own root, so it has to exist there under
# that exact name too, not just as the plain `qemu-*-static` name.
if [ "$BINFMT_INTERPRETER" != "$QEMU_STATIC_BIN" ]; then
    mkdir -p "$ROOT_MNT$(dirname "$BINFMT_INTERPRETER")"
    cp "$QEMU_STATIC_BIN" "$ROOT_MNT$BINFMT_INTERPRETER"
fi
mount --bind /proc "$ROOT_MNT/proc"
mount --bind /sys "$ROOT_MNT/sys"
mount --bind /dev "$ROOT_MNT/dev"

# --- 5. Strip Python pwnagotchi/pwngrid stack + toolchains ----------------
# Every path here was confirmed by directly inspecting a mounted copy of
# the specific release(s) targeted (see README.md) -- nothing here is
# guessed from install scripts alone, since paths have genuinely differed
# between releases (the pwnagotchi venv location differs from what
# v2.9.5.x's own install script source suggests; v2.8.9 doesn't use a
# venv at all -- see BASE_VERSION comment above). `rm -rf` on a path that
# doesn't exist on a given base version is a silent no-op, so this list
# is deliberately a superset covering every version this script supports
# rather than branching per-version.
#
# Phase 1 of the rework switched this project's capture backend from
# AngryOxide to bettercap (AngryOxide cannot capture at all on this
# hardware's FullMAC/nexmon chip -- see crates/bettercap's doc comment),
# so unlike earlier revisions of this script, bettercap's real binary
# (`/usr/local/bin/bettercap`) is now KEPT -- pwnghost-rs drives it
# directly over its REST API, the same architecture real pwnagotchi uses.
# Only the original jayofelony systemd unit is removed here; our own
# replacement (`overlay/etc/systemd/system/bettercap.service`, pointed at
# our own api.rest credentials) is installed by the overlay-copy step
# below. pwngrid (mesh) is a separate, not-yet-decided workstream (see
# REWORK_PLAN.md) and stays removed.
log "Stripping Python pwnagotchi/pwngrid stack (keeping bettercap's binary)"
chroot "$ROOT_MNT" /bin/bash -euo pipefail -c '
systemctl disable pwnagotchi.service pwngrid-peer.service 2>/dev/null || true
rm -f /etc/systemd/system/pwnagotchi.service \
      /etc/systemd/system/bettercap.service \
      /etc/systemd/system/pwngrid-peer.service
rm -f /usr/bin/pwnagotchi /usr/bin/pwnagotchi-launcher /usr/local/bin/pwnagotchi
rm -rf /home/pi/.pwn /home/pi/bettercap /home/pi/pwngrid
rm -f /home/pi/firmware-nexmon_0.2_all.deb.1 /home/pi/firmware-nexmon_0.2_all.deb.2
rm -f /usr/local/bin/pwngrid
rm -rf /usr/local/go
rm -rf /root/.cargo /root/.rustup
# v2.8.9-style system-wide pip install (no isolated venv) -- remove the
# package and its metadata directly rather than a venv rm -rf.
rm -rf /usr/local/lib/python3.9/dist-packages/pwnagotchi \
       /usr/local/lib/python3.9/dist-packages/pwnagotchi-*.dist-info
apt-get purge -y golang-go golang-1.2* build-essential 2>/dev/null || true
apt-get autoremove -y --purge 2>/dev/null || true
apt-get clean
rm -rf /var/lib/apt/lists/*
'

# --- 6. Splice in our own binary + config + systemd units -----------------
log "Installing pwnghost-rs ($RUST_TARGET) + overlay"
install -m 755 "$ARTIFACTS_DIR/$RUST_TARGET/pwnghost-rs" "$ROOT_MNT/usr/local/bin/pwnghost-rs"
install -m 755 "$ARTIFACTS_DIR/$RUST_TARGET/wlan_keepalive" "$ROOT_MNT/usr/local/bin/wlan_keepalive"

# AngryOxide is no longer part of this image (Phase 1 of the rework
# replaced it with bettercap -- kept installed on this base image already,
# see step 5 above -- because AngryOxide cannot capture at all on this
# hardware's FullMAC/nexmon chip; confirmed on real hardware: Frames: 0,
# high ERs, "NetworkDown"/os error 132 (ERFKILL), across every attempt).
# hcxtools is still needed (hcxpcapngtool validates bettercap's captures
# exactly as it did AngryOxide's).
log "Installing hcxtools"
chroot "$ROOT_MNT" /bin/bash -euo pipefail -c "
export DEBIAN_FRONTEND=noninteractive
# Raspberry Pi's own mirror (raspbian.raspberrypi.org, the source for the
# 'rpi' component hcxtools lives in) has been observed refusing
# connections or failing to serve its component index intermittently --
# confirmed transient (a plain connectivity test to the same host
# succeeds moments later) but real enough to have failed 3 build attempts
# in a row. Retry the whole update+install a few times with backoff
# rather than letting one flaky window fail the entire multi-minute image
# build.
for attempt in 1 2 3 4 5; do
    if apt-get update && apt-get install -y --no-install-recommends curl ca-certificates hcxtools; then
        break
    fi
    if [ \"\$attempt\" = 5 ]; then
        echo 'build.sh: apt-get update/install failed 5 times, giving up' >&2
        exit 1
    fi
    echo \"build.sh: apt-get update/install failed (attempt \$attempt/5), retrying in \$((attempt * 10))s\" >&2
    sleep \$((attempt * 10))
done
apt-get clean
rm -rf /var/lib/apt/lists/*
"

# This base image's config.txt enables dtoverlay=spi1-3cs (SPI1 with 3
# chip-selects: CE0=GPIO18, CE1=GPIO17, CE2=GPIO16) for every board
# variant ([pi0]/[pi3]/[pi4] sections). We don't use SPI1 for anything,
# but CE1=GPIO17 is the exact same pin the Waveshare e-Paper HAT uses for
# its RST line (BUSY=24, DC=25, RST=17 -- Waveshare's standard pinout).
# Confirmed on real hardware via strace: pwnghost-rs's GPIO_GET_LINEHANDLE_IOCTL
# for pin 17 fails with EBUSY (already claimed by the spi1-3cs overlay at
# boot), which is why the display previously failed to initialize.
# Disabling this overlay (verified live over SSH to fix it) frees GPIO17
# back to plain GPIO use.
log "Disabling dtoverlay=spi1-3cs (conflicts with e-ink RST on GPIO17)"
sed -i 's/^dtoverlay=spi1-3cs/#dtoverlay=spi1-3cs/' "$BOOT_MNT/config.txt"

# overlay/ (see this directory) carries our own config.toml,
# pwnghost-rs.service, wlan_keepalive.service, monstart/monstop, and the
# reliability units (zram-backed logging, bootlog, safe-shutdown) ported
# verbatim from pi-gen/stage5 -- deliberately excludes anything
# USB-gadget/NetworkManager-related, since this base image's own
# rpi-usb-gadget package + NetworkManager setup is the thing being kept
# intact, not replaced.
#
# -K (--keep-dirlinks) is required: this rootfs is usrmerged (/lib is a
# symlink to usr/lib), but overlay/ has a real directory at
# lib/systemd/system-shutdown/. Without -K, rsync's default behavior when
# the source has a real directory where the destination has a symlink is
# to delete the destination symlink and create a real directory in its
# place -- destroying the /lib -> usr/lib symlink and, with it, every
# other path that resolved through it (confirmed the hard way on a real
# run: the very next chroot command failed with "qemu-arm: Could not open
# '/lib/ld-linux-armhf.so.3'", because that file actually lives at
# /usr/lib/ld-linux-armhf.so.3 and was only reachable via the now-gone
# symlink). This is the exact same bug already documented in
# pi-gen/stage5/01-runtime-overlay/00-run.sh for the from-scratch build --
# missed here because this script wasn't written by copying that one.
rsync -a -K "$OVERLAY_DIR/" "$ROOT_MNT/"
# ROOT_MNT is a loop-mounted ext4 filesystem living inside base.img, which
# itself sits on WORK_DIR -- a Docker Desktop bind mount from the Windows
# host (virtiofs/9p). Confirmed the hard way: files rsync just wrote were
# verifiably present via a plain `ls` on ROOT_MNT immediately afterward,
# yet the very next `chroot` below -- a fresh chroot() re-resolving paths
# from scratch -- got ENOENT for those exact same files. That's a
# read-after-write visibility gap somewhere in the nested
# bind-mount/loop-device stack, not a real missing-file bug. `sync`
# forces the write-back before anything re-resolves paths through this
# mount, which is cheap here (single-digit MB of overlay data).
sync "$ROOT_MNT"

# The chroot step below reads files this same rsync just wrote. Confirmed
# by direct diagnosis (stat *inside* a chroot on this exact path returns
# a valid inode with correct size/mtime, yet the *next* separate chroot()
# call on the same path gets ENOENT) that this is a transient visibility
# race somewhere in the nested loop-device-inside-a-Windows-virtiofs-bind-
# mount stack, not a real missing-file bug and not something this script
# can fix at the root (candidates: virtiofs metadata caching, WSL2 mount
# propagation lag, antivirus scanning the loop-backed image file
# mid-write -- unconfirmed, no way to isolate further from inside the
# container). The whole block is idempotent (mkdir -p, chmod, and
# `systemctl enable` on an already-enabled unit are all safe to repeat),
# so retry it as a unit with a settle delay rather than trying to patch
# around one specific failure mode blind.
OVERLAY_INSTALL_ATTEMPTS=5
for attempt in $(seq 1 "$OVERLAY_INSTALL_ATTEMPTS"); do
    if chroot "$ROOT_MNT" /bin/bash -euo pipefail -c '
mkdir -p /etc/pwnghost/conf.d /etc/pwnghost/handshakes /var/log/pwnghost /var/tmp/pwnghost /var/lib/pwnghost /var/tmp/pwnghost/bettercap-output
chmod +x /usr/bin/monstart /usr/bin/monstop /usr/local/bin/*.sh 2>/dev/null || true
chmod +x /usr/local/bin/pwnghost-monstart-if-needed 2>/dev/null || true
chmod +x /lib/systemd/system-shutdown/safe-shutdown.sh 2>/dev/null || true
# bt-pan-connect/disconnect have no .sh extension, so the *.sh glob above
# does not cover them -- named explicitly, matching the from-scratch pi-gen build
# stage5/01-runtime-overlay/00-run.sh does for the same two files.
chmod +x /usr/local/bin/bt-pan-connect /usr/local/bin/bt-pan-disconnect 2>/dev/null || true
# The overlay is bind-mounted in from a Windows/NTFS host, whose file
# modes do not survive the Docker Desktop bind-mount + rsync chain
# reliably -- confirmed the hard way: systemd warned these two were
# "marked executable"/"marked world-writable" on a real run despite the
# source files being 644 on disk. Set the correct mode explicitly here
# instead of trusting whatever rsync copied.
#
# Named explicitly (not a *.service/*.timer glob over the whole
# directory) -- a glob also matches every pre-existing unit this base
# image already ships, including at least one dangling symlink on
# v2.8.9 (dbus-org.freedesktop.ModemManager1.service), on which chmod
# fails outright and, under set -e, aborted the entire strip+install
# step on a real run.
chmod 644 /etc/systemd/system/pwnghost-rs.service \
          /etc/systemd/system/bettercap.service \
          /etc/systemd/system/wlan_keepalive.service \
          /etc/systemd/system/wifi-country.service \
          /etc/systemd/system/zram-log.service \
          /etc/systemd/system/zram-data.service \
          /etc/systemd/system/rsync-zram.service \
          /etc/systemd/system/rsync-zram.timer \
          /etc/systemd/system/buffer-cleaner.service \
          /etc/systemd/system/buffer-cleaner.timer \
          /etc/systemd/system/bootlog.service \
          /etc/systemd/system/safe-shutdown.service
# wifi-country.service (ported from the from-scratch pi-gen build overlay
# -- see pi-gen/stage5/01-runtime-overlay -- never carried over here
# originally, see README.md "Not carried over in this first pass" note)
# unblocks rfkill and sets the regulatory domain before pwnghost-rs starts.
# Its absence is a confirmed real bug, not a theoretical gap: a real-hardware
# boot of this rebased image showed wlan0mon permanently stuck in
# "Operation not possible due to RF-kill" (dhcpcd/wlan_keepalive logs),
# which is exactly what this service exists to prevent -- this base image
# never runs a wifi-country first-boot step on its own the way stock
# Raspberry Pi OS does via raspi-config.
systemctl enable pwnghost-rs.service bettercap.service wlan_keepalive.service wifi-country.service
systemctl enable zram-log.service zram-data.service rsync-zram.timer buffer-cleaner.timer bootlog.service safe-shutdown.service 2>/dev/null || true
# bt-agent.service registers a NoInputNoOutput pairing agent so a phone
# can pair without a PIN prompt -- ported over from the from-scratch pi-gen build
# stage5/01-runtime-overlay (which already had this working) since this
# this pipeline overlay never carried it, meaning BT pairing/tethering
# was silently missing on every rebase-pipeline image despite the base
# jayofelony image already shipping the bt-agent/bluetoothctl binaries
# this depends on (confirmed directly by mounting the base image).
# bt-pan@.service is a template unit started per-device MAC at runtime
# (systemctl start bt-pan@AA-BB-CC-DD-EE-FF.service -- bt-pan-connect/
# disconnect expect dashes in the instance name and convert back to
# colon format internally) -- not enabled here, matching the pi-gen build;
# bt_tether.lua (crates/lua) is what actually starts/stops it once a
# phone is configured.
systemctl enable bt-agent.service 2>/dev/null || true
'; then
        break
    fi
    if [ "$attempt" = "$OVERLAY_INSTALL_ATTEMPTS" ]; then
        echo "build.sh: overlay-install chroot step failed $OVERLAY_INSTALL_ATTEMPTS times in a row -- giving up (see the visibility-race comment above)" >&2
        exit 1
    fi
    echo "build.sh: overlay-install chroot step failed (attempt $attempt/$OVERLAY_INSTALL_ATTEMPTS) -- retrying after a sync+settle delay" >&2
    sync "$ROOT_MNT"
    sleep 2
done

# --- 7. Unmount, fsck, shrink, recompress ---------------------------------
log "Unmounting for fsck"
umount "$ROOT_MNT/dev"
umount "$ROOT_MNT/sys"
umount "$ROOT_MNT/proc"
umount "$BOOT_MNT"
umount "$ROOT_MNT"

log "Filesystem check (must pass clean before this is trusted on a real SD card)"
e2fsck -f -y "$ROOT_LOOP"

log "Zeroing free blocks for better compression (best-effort, bounded)"
# zerofree only improves the xz compression ratio -- it isn't required
# for a correct image, unlike e2fsck above. It produces no output for a
# long stretch while scanning every block on a ~7GB filesystem, which hit
# a "no output for too long" kill on this environment's background-task
# runner twice in a row with nothing else running concurrently. Bound it
# with `timeout` and continue without it if it doesn't finish in time,
# rather than let a pure size optimization take down the whole pipeline.
timeout 300 zerofree "$ROOT_LOOP" || log "zerofree did not finish in time -- continuing without it (compression will be slightly larger, not incorrect)"

log "Shrinking filesystem to minimum + slack"
MIN_BLOCKS="$(resize2fs -P "$ROOT_LOOP" 2>&1 | grep -oE '[0-9]+$')"
BLOCK_SIZE="$(dumpe2fs -h "$ROOT_LOOP" 2>/dev/null | awk '/Block size/{print $3}')"
# 256MB slack so first boot's filesystem resize (raspberrypi-sys-mods'
# rpi-resize, already present in this base image) has room to expand into
# on the real SD card, and isn't starting from a filesystem sized exactly
# to its current contents with zero headroom.
SLACK_BLOCKS=$(( 256 * 1024 * 1024 / BLOCK_SIZE ))
NEW_BLOCKS=$(( MIN_BLOCKS + SLACK_BLOCKS ))
resize2fs "$ROOT_LOOP" "${NEW_BLOCKS}"
NEW_ROOT_SIZE=$(( NEW_BLOCKS * BLOCK_SIZE ))

losetup -d "$ROOT_LOOP"
losetup -d "$BOOT_LOOP"
trap - EXIT

log "Rewriting partition table + truncating image (in place -- no full-image copy)"
python3 - "$RAW_IMG" "$ROOT_OFFSET" "$NEW_ROOT_SIZE" <<'PYEOF'
import struct, sys
path, root_offset, new_root_size = sys.argv[1], int(sys.argv[2]), int(sys.argv[3])
new_root_sectors = (new_root_size + 511) // 512
new_end_byte = root_offset + new_root_sectors * 512

with open(path, 'r+b') as f:
    mbr = bytearray(f.read(512))
    # Find the 0x83 (Linux) partition entry and rewrite its sector count.
    for i in range(4):
        off = 446 + i*16
        if mbr[off+4] == 0x83:
            struct.pack_into('<I', mbr, off+12, new_root_sectors)
            break
    else:
        raise SystemExit("could not find root (0x83) partition entry")
    f.seek(0)
    f.write(mbr)
    f.truncate(new_end_byte)
print(f"truncated to {new_end_byte} bytes (root: {new_root_sectors} sectors from offset {root_offset})")
PYEOF
mv "$RAW_IMG" "$OUT_IMG"

log "Compressing"
# -v: periodic progress output, so a long silent stretch on a large file
# doesn't look identical to a hung process to anything watching for output.
# xz's own default output path (${OUT_IMG}.xz) is already exactly $OUT_XZ
# -- no rename needed (an earlier revision tried to `mv` it onto itself,
# which fails with "are the same file").
# --memlimit-compress: xz -T0's auto-thread mode otherwise self-limits to
# ~25% of detected RAM by default (confirmed live: it silently dropped
# from 6 threads to 1 on a host with plenty of free memory, turning a
# multi-core box into a single-threaded compression run). 80% is
# generous headroom while still leaving room for the rest of the system.
xz -T0 -9 -v -f --memlimit-compress=80% "$OUT_IMG"

log "Done: $OUT_XZ"
ls -lh "$OUT_XZ"
