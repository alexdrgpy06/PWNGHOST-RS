#!/bin/bash -e
# stage5/prerun.sh - inherit the rootfs built by stage4 (pwnghost-rs binary).

if [ ! -d "${ROOTFS_DIR}" ]; then
	copy_previous
fi
