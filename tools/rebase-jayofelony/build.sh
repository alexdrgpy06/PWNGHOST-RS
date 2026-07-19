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

BASE_VERSION="${BASE_VERSION:-2.9.5.3}"

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
if [ ! -e /proc/sys/fs/binfmt_misc/qemu-arm ]; then
    echo "build.sh: qemu-arm binfmt registration failed -- chroot commands will not run" >&2
    exit 1
fi

BOARD="${BOARD:?Set BOARD=pi-zero-w or BOARD=pi-zero-2w}"
case "$BOARD" in
    pi-zero-w)  RUST_TARGET="arm-unknown-linux-gnueabihf" ;;
    pi-zero-2w) RUST_TARGET="armv7-unknown-linux-gnueabihf" ;;
    *) echo "build.sh: unknown BOARD='$BOARD' (expected pi-zero-w or pi-zero-2w)" >&2; exit 1 ;;
esac

WORK_DIR="${WORK_DIR:-/work}"
ARTIFACTS_DIR="${ARTIFACTS_DIR:-$WORK_DIR/artifacts}"
OVERLAY_DIR="${OVERLAY_DIR:-$WORK_DIR/overlay}"

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
    *)
        echo "build.sh: unknown BASE_VERSION='$BASE_VERSION' (expected 2.9.5.3 or 2.8.9)" >&2
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

# --- 4. Set up chroot (qemu-arm-static for armhf binaries) ----------------
log "Setting up chroot"
cp /usr/bin/qemu-arm-static "$ROOT_MNT/usr/bin/qemu-arm-static"
mount --bind /proc "$ROOT_MNT/proc"
mount --bind /sys "$ROOT_MNT/sys"
mount --bind /dev "$ROOT_MNT/dev"

# --- 5. Strip Python pwnagotchi/bettercap/pwngrid stack + toolchains ------
# Every path here was confirmed by directly inspecting a mounted copy of
# the specific release(s) targeted (see README.md) -- nothing here is
# guessed from install scripts alone, since paths have genuinely differed
# between releases (the pwnagotchi venv location differs from what
# v2.9.5.x's own install script source suggests; v2.8.9 doesn't use a
# venv at all -- see BASE_VERSION comment above). `rm -rf` on a path that
# doesn't exist on a given base version is a silent no-op, so this list
# is deliberately a superset covering every version this script supports
# rather than branching per-version.
log "Stripping Python pwnagotchi/bettercap/pwngrid stack"
chroot "$ROOT_MNT" /bin/bash -euo pipefail -c '
systemctl disable pwnagotchi.service bettercap.service pwngrid-peer.service 2>/dev/null || true
rm -f /etc/systemd/system/pwnagotchi.service \
      /etc/systemd/system/bettercap.service \
      /etc/systemd/system/pwngrid-peer.service
rm -f /usr/bin/pwnagotchi /usr/bin/pwnagotchi-launcher /usr/local/bin/pwnagotchi
rm -rf /home/pi/.pwn /home/pi/bettercap /home/pi/pwngrid
rm -f /home/pi/firmware-nexmon_0.2_all.deb.1 /home/pi/firmware-nexmon_0.2_all.deb.2
rm -f /usr/local/bin/bettercap /usr/local/bin/pwngrid
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

chroot "$ROOT_MNT" /bin/bash -euo pipefail -c '
mkdir -p /etc/pwnghost/conf.d /etc/pwnghost/handshakes /var/log/pwnghost /var/tmp/pwnghost /var/lib/pwnghost
chmod +x /usr/bin/monstart /usr/bin/monstop /usr/local/bin/*.sh 2>/dev/null || true
chmod +x /lib/systemd/system-shutdown/safe-shutdown.sh 2>/dev/null || true
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
          /etc/systemd/system/wlan_keepalive.service \
          /etc/systemd/system/zram-log.service \
          /etc/systemd/system/zram-data.service \
          /etc/systemd/system/rsync-zram.service \
          /etc/systemd/system/rsync-zram.timer \
          /etc/systemd/system/buffer-cleaner.service \
          /etc/systemd/system/buffer-cleaner.timer \
          /etc/systemd/system/bootlog.service \
          /etc/systemd/system/safe-shutdown.service
systemctl enable pwnghost-rs.service wlan_keepalive.service
systemctl enable zram-log.service zram-data.service rsync-zram.timer buffer-cleaner.timer bootlog.service safe-shutdown.service 2>/dev/null || true
'

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
xz -T0 -9 -v -f "$OUT_IMG"

log "Done: $OUT_XZ"
ls -lh "$OUT_XZ"
