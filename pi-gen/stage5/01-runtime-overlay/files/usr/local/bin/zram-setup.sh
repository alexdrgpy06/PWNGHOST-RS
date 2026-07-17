#!/bin/bash
# zram-setup.sh - Set up a zram device for RAM-backed storage.
# Usage: zram-setup.sh <name> <size> <mountpoint>
#
# name is either "log" (-> /var/log/pwnghost) or "data" (-> /var/tmp/pwnghost),
# matching pwnghost-rs's config.toml [fs.mounts.log]/[fs.mounts.data].

set -euo pipefail

NAME="$1"
SIZE="$2"
MOUNTPOINT="$3"

if [[ -z "$NAME" || -z "$SIZE" || -z "$MOUNTPOINT" ]]; then
    echo "Usage: $0 <name> <size> <mountpoint>"
    echo "Example: $0 log 50M /var/log/pwnghost"
    exit 1
fi

# Determine zram device number
if [[ "$NAME" == "log" ]]; then
    ZRAM_NUM=0
elif [[ "$NAME" == "data" ]]; then
    ZRAM_NUM=1
else
    echo "Unknown zram name: $NAME"
    exit 1
fi

ZRAM_DEV="/dev/zram$ZRAM_NUM"
PERSISTENT_DIR="/var/lib/pwnghost/$NAME"

echo "Setting up zram$ZRAM_NUM ($NAME, $SIZE) at $MOUNTPOINT"

# Load zram module if not loaded
modprobe zram num_devices=2

# Reset device
echo 1 > "/sys/block/zram$ZRAM_NUM/reset" 2>/dev/null || true

# Set compression algorithm
echo zstd > "/sys/block/zram$ZRAM_NUM/comp_algorithm"

# Set size
echo "$SIZE" > "/sys/block/zram$ZRAM_NUM/disksize"

# Format as ext4
mkfs.ext4 -F -L "pwnghost-$NAME" "$ZRAM_DEV" >/dev/null 2>&1

# Mount with noatime to reduce writes
mkdir -p "$MOUNTPOINT"
mount -o noatime,nodiratime,discard "$ZRAM_DEV" "$MOUNTPOINT"

# Restore whatever was synced out to persistent storage at the last clean
# shutdown / periodic rsync-zram.sh run, so logs/data survive a reboot
# instead of starting empty every time the zram fs is recreated from scratch.
if [[ -d "$PERSISTENT_DIR" ]]; then
    rsync -a "$PERSISTENT_DIR/" "$MOUNTPOINT/" 2>/dev/null || true
fi

# Set permissions
chmod 755 "$MOUNTPOINT"

echo "zram$ZRAM_NUM mounted at $MOUNTPOINT"
