#!/bin/bash
# zram-teardown.sh - Tear down a zram device, syncing it to persistent
# storage first.
# Usage: zram-teardown.sh <name>

set -euo pipefail

NAME="$1"

if [[ -z "$NAME" ]]; then
    echo "Usage: $0 <name>"
    exit 1
fi

# Determine zram device number
if [[ "$NAME" == "log" ]]; then
    ZRAM_NUM=0
    MOUNTPOINT="/var/log/pwnghost"
elif [[ "$NAME" == "data" ]]; then
    ZRAM_NUM=1
    MOUNTPOINT="/var/tmp/pwnghost"
else
    echo "Unknown zram name: $NAME"
    exit 1
fi

echo "Tearing down zram$ZRAM_NUM ($NAME)"

# Sync to disk first
if [[ -n "$MOUNTPOINT" && -d "$MOUNTPOINT" ]]; then
    mkdir -p "/var/lib/pwnghost/$NAME"
    rsync -a --delete "$MOUNTPOINT/" "/var/lib/pwnghost/$NAME/" 2>/dev/null || true
fi

# Unmount
umount "$MOUNTPOINT" 2>/dev/null || true

# Reset zram device
echo 1 > "/sys/block/zram$ZRAM_NUM/reset" 2>/dev/null || true

echo "zram$ZRAM_NUM torn down"
