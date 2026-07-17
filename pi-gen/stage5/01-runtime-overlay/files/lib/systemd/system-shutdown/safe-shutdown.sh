#!/bin/bash
# safe-shutdown.sh - Safe shutdown handler for pwnghost-rs.
# Called by systemd-shutdown on every shutdown/reboot/halt, and by
# safe-shutdown.service earlier in the same sequence, so the zram-backed
# log/data mounts get flushed to disk before power-off even though this
# device typically has no graceful button-triggered shutdown path.

set -euo pipefail

LOG_FILE="/var/lib/pwnghost/safe-shutdown.log"
ZRAM_LOG="/var/log/pwnghost"
ZRAM_DATA="/var/tmp/pwnghost"
PERSISTENT_LOG="/var/lib/pwnghost/log"
PERSISTENT_DATA="/var/lib/pwnghost/data"

mkdir -p "$(dirname "$LOG_FILE")"

log() {
    echo "[$(date '+%Y-%m-%d %H:%M:%S')] $*" | tee -a "$LOG_FILE"
}

log "=== Safe shutdown initiated ==="

log "Stopping pwnghost-rs.service..."
systemctl stop pwnghost-rs.service 2>/dev/null || true
sleep 2
if systemctl is-active pwnghost-rs.service &>/dev/null; then
    log "pwnghost-rs still running, sending SIGKILL..."
    pkill -9 pwnghost-rs 2>/dev/null || true
    sleep 1
fi

log "Syncing zram mounts to disk..."
if mountpoint -q "$ZRAM_LOG"; then
    rsync -a --delete "$ZRAM_LOG/" "$PERSISTENT_LOG/" 2>&1 | tee -a "$LOG_FILE" || true
fi
if mountpoint -q "$ZRAM_DATA"; then
    rsync -a --delete "$ZRAM_DATA/" "$PERSISTENT_DATA/" 2>&1 | tee -a "$LOG_FILE" || true
fi

sync

log "Unmounting zram devices..."
systemctl stop zram-log.service 2>/dev/null || true
systemctl stop zram-data.service 2>/dev/null || true

sync
sync

log "=== Safe shutdown complete ==="
exit 0
