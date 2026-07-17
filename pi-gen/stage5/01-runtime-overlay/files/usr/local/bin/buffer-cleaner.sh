#!/bin/bash
# buffer-cleaner.sh - Flush kernel caches and clean up stale buffers every
# 5 minutes. Prevents memory pressure on the 512MB Pi Zero / Zero 2W.
sync
echo 1 > /proc/sys/vm/drop_caches 2>/dev/null
journalctl --vacuum-size=16M 2>/dev/null
