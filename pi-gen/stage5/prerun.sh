#!/bin/bash -e
# stage5/prerun.sh - inherit the rootfs built by stage4 (rust binary + AngryOxide).

if [ ! -d "${ROOTFS_DIR}" ]; then
	copy_previous
fi
