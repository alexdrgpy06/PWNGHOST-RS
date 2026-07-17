#!/bin/bash -e
# stage4/prerun.sh - inherit the rootfs built by stage3 (nexmon firmware).

if [ ! -d "${ROOTFS_DIR}" ]; then
	copy_previous
fi
