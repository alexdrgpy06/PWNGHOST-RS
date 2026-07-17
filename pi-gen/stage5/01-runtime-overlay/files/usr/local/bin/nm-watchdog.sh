#!/bin/bash
# nm-watchdog.sh - Restart NetworkManager if it stops running.
#
# The BCM4343x shares its bus with Bluetooth; NetworkManager occasionally
# wedges during monitor-mode transitions. This keeps it alive so BT PAN / USB
# fallback networking recovers on its own.

set -u

while true; do
    if ! systemctl is-active --quiet NetworkManager.service; then
        logger -t nm-watchdog "NetworkManager not active, restarting"
        systemctl restart NetworkManager.service || true
        sleep 10
    fi
    sleep 30
done
