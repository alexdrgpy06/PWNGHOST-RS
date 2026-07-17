#!/bin/bash -e
# stage3/prerun.sh - inherit the rootfs built by stage2.
#
# pi-gen runs this before stage3's sub-stages. Without copy_previous the
# stage3 rootfs never exists and every sub-stage silently no-ops.

if [ ! -d "${ROOTFS_DIR}" ]; then
	copy_previous
fi
