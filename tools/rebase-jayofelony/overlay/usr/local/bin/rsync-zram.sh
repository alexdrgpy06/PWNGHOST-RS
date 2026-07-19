#!/bin/bash
# rsync-zram.sh - Periodically sync zram mounts to persistent storage, so a
# power loss (no clean shutdown, e.g. this device has no power button) only
# loses whatever changed since the last run of this timer.

set -euo pipefail

LOG_DIR="/var/log/pwnghost"
DATA_DIR="/var/tmp/pwnghost"
PERSISTENT_LOG="/var/lib/pwnghost/log"
PERSISTENT_DATA="/var/lib/pwnghost/data"

mkdir -p "$PERSISTENT_LOG" "$PERSISTENT_DATA"

if [[ -d "$LOG_DIR" ]]; then
    rsync -a --delete "$LOG_DIR/" "$PERSISTENT_LOG/" 2>/dev/null || true
fi

if [[ -d "$DATA_DIR" ]]; then
    rsync -a --delete "$DATA_DIR/" "$PERSISTENT_DATA/" 2>/dev/null || true
fi

exit 0
